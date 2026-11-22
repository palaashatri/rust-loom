//! The inspector's text style controls: showing the selected object's look and
//! applying a choice to every selected text object or shape as one undo step.

use std::rc::Rc;

use loom_present_core::{ElementStyle, PresentationSession, TextAlign, PALETTE};
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::object_parts::{color, look};
use crate::{refresh, set_status, ElementInk, GuiState, LoomColorSwatch, PresentApp};

const OFFICE_SWATCHES: [(&str, &str); 48] = [
    ("#FFFFFF", "White"),
    ("#000000", "Black"),
    ("#E7E6E6", "Light Gray"),
    ("#44546A", "Slate"),
    ("#4472C4", "Blue"),
    ("#ED7D31", "Orange"),
    ("#A5A5A5", "Gray"),
    ("#FFC000", "Gold"),
    ("#F2F2F2", "White 5%"),
    ("#7F7F7F", "Gray 50%"),
    ("#D0CECE", "Light Gray 10%"),
    ("#D6DCE4", "Slate 80%"),
    ("#D9E2F3", "Blue 80%"),
    ("#FCE4D6", "Orange 80%"),
    ("#EDEDED", "Gray 80%"),
    ("#FFF2CC", "Gold 80%"),
    ("#D8D8D8", "White 15%"),
    ("#595959", "Gray 35%"),
    ("#AEABAB", "Light Gray 25%"),
    ("#ADB9CA", "Slate 60%"),
    ("#B4C6E7", "Blue 60%"),
    ("#F8CBAD", "Orange 60%"),
    ("#DBDBDB", "Gray 60%"),
    ("#FFE699", "Gold 60%"),
    ("#BFBFBF", "White 25%"),
    ("#3F3F3F", "Gray 25%"),
    ("#757070", "Light Gray 50%"),
    ("#8497B0", "Slate 40%"),
    ("#8EA9DB", "Blue 40%"),
    ("#F4B084", "Orange 40%"),
    ("#C9C9C9", "Gray 40%"),
    ("#FFD966", "Gold 40%"),
    ("#A5A5A5", "White 35%"),
    ("#262626", "Gray 15%"),
    ("#3A3838", "Light Gray 75%"),
    ("#333F4F", "Slate 25%"),
    ("#305496", "Blue 25%"),
    ("#C65911", "Orange 25%"),
    ("#7B7B7B", "Gray 25%"),
    ("#BF8F00", "Gold 25%"),
    ("#7F7F7F", "White 50%"),
    ("#0C0C0C", "Gray 5%"),
    ("#161616", "Light Gray 90%"),
    ("#222B35", "Slate 50%"),
    ("#203764", "Blue 50%"),
    ("#833C0C", "Orange 50%"),
    ("#525252", "Gray 50%"),
    ("#806000", "Gold 50%"),
];

fn slot_of(hex: Option<&str>) -> i32 {
    hex.and_then(|hex| PALETTE.iter().position(|p| p.eq_ignore_ascii_case(hex)))
        .map_or(-1, |slot| slot as i32)
}

/// Show the active object's style in the inspector; hide the controls when it
/// is a picture, table or chart. The family also reports when the selected
/// objects disagree, instead of showing the active object's family for all.
pub(crate) fn publish(
    app: &PresentApp,
    session: &PresentationSession,
    element: &loom_present_core::SlideElement,
) {
    let ink = app.global::<ElementInk>();
    let Some(active_look) = look(element) else {
        ink.set_available(false);
        return;
    };
    ink.set_available(true);
    ink.set_size_label(format!("{:.0} pt", active_look.size_pt).into());
    ink.set_bold(active_look.bold);
    ink.set_italic(active_look.italic);
    ink.set_align(active_look.align);
    ink.set_color_slot(slot_of(element.style.color.as_deref()));
    ink.set_fill_slot(slot_of(element.style.fill.as_deref()));
    let (family, mixed) = session
        .document
        .active_slide()
        .map(|slide| {
            let families: Vec<Option<String>> = slide
                .elements
                .iter()
                .filter(|candidate| session.selected_elements.contains(&candidate.id))
                .filter(|candidate| look(candidate).is_some())
                .map(|candidate| {
                    candidate
                        .style
                        .font_family
                        .as_deref()
                        .and_then(|name| loom_present_core::normalize_font_family(name).ok())
                })
                .collect();
            match families.split_first() {
                Some((first, rest)) => {
                    let mixed = rest.iter().any(|family| family != first);
                    (
                        if mixed {
                            String::new()
                        } else {
                            first.clone().unwrap_or_default()
                        },
                        mixed,
                    )
                }
                None => (String::new(), false),
            }
        })
        .unwrap_or_default();
    ink.set_font_family(family.into());
    ink.set_font_family_mixed(mixed);
}

pub(crate) fn clear(app: &PresentApp) {
    let ink = app.global::<ElementInk>();
    ink.set_available(false);
    ink.set_font_family("".into());
    ink.set_font_family_mixed(false);
}

/// A number as the geometry fields show it: whole when it is whole, else one decimal.
fn shown(value: f32) -> String {
    if (value - value.round()).abs() < 0.05 {
        format!("{:.0}", value)
    } else {
        format!("{value:.1}")
    }
}

/// Fill the position, size and rotation fields from the active object.
pub(crate) fn publish_geometry(app: &PresentApp, element: &loom_present_core::SlideElement) {
    let ink = app.global::<ElementInk>();
    ink.set_x_text(shown(element.x).into());
    ink.set_y_text(shown(element.y).into());
    ink.set_w_text(shown(element.width).into());
    ink.set_h_text(shown(element.height).into());
    ink.set_rot_text(shown(element.rotation_deg).into());
}

/// The 1000-unit slide is 13⅓ inches wide in both PDF and PPTX exports.
fn parse_number(field: &str, text: &str) -> Option<f32> {
    let text = text.trim().to_ascii_lowercase();
    let units: &[(&str, f32)] = if field == "rot" {
        &[("°", 1.0), ("deg", 1.0)]
    } else {
        &[
            ("in", 75.0),
            ("cm", 75.0 / 2.54),
            ("mm", 75.0 / 25.4),
            ("pt", 75.0 / 72.0),
        ]
    };
    let (number, factor) = units
        .iter()
        .find_map(|(suffix, factor)| {
            text.strip_suffix(suffix)
                .map(|number| (number.trim(), *factor))
        })
        .unwrap_or((&text, 1.0));
    number
        .parse::<f32>()
        .ok()
        .map(|value| value * factor)
        .filter(|value| value.is_finite())
}

pub(crate) fn apply_geometry(state: &GuiState, field: &str, text: &str) -> Result<bool, String> {
    if text.trim().is_empty() {
        return Ok(false);
    }
    let value = parse_number(field, text).ok_or_else(|| {
        let units = if field == "rot" {
            "° or deg"
        } else {
            "in, cm, mm or pt"
        };
        format!(
            "'{}' is not a number with a supported unit ({units})",
            text.trim()
        )
    })?;
    let mut session = state.session.borrow_mut();
    let selected: Vec<(String, f32, f32, f32, f32)> = session
        .document
        .active_slide()
        .map(|slide| {
            slide
                .elements
                .iter()
                .filter(|element| session.selected_elements.contains(&element.id))
                .map(|element| {
                    (
                        element.id.clone(),
                        element.x,
                        element.y,
                        element.width,
                        element.height,
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    let (id, x, y, width, height) = match selected.as_slice() {
        [] => return Ok(false),
        [element] => (*element).clone(),
        _ => return Err("Select one object to type exact geometry".into()),
    };
    Ok(match field {
        "x" => session.transform_element(&id, value, y, width, height),
        "y" => session.transform_element(&id, x, value, width, height),
        "w" => session.transform_element(&id, x, y, value, height),
        "h" => session.transform_element(&id, x, y, width, value),
        "rot" => session.set_element_rotation(&id, value),
        _ => false,
    })
}

/// Apply one control's choice to the selected text objects and shapes.
/// Returns the status words for a change, or `None` when nothing changed.
pub(crate) fn apply(state: &GuiState, kind: &str, value: i32) -> Option<String> {
    let mut session = state.session.borrow_mut();
    let slide = session.document.active_slide()?;
    let styleable: Vec<(String, crate::object_parts::TextLook)> = slide
        .elements
        .iter()
        .filter(|element| session.selected_elements.contains(&element.id))
        .filter_map(|element| Some((element.id.clone(), look(element)?)))
        .collect();
    let (_, base) = styleable.first()?;
    let (size, bold, italic) = (base.size_pt, base.bold, base.italic);
    let ids: Vec<String> = styleable.iter().map(|(id, _)| id.clone()).collect();
    let slot = usize::try_from(value)
        .ok()
        .and_then(|slot| PALETTE.get(slot));
    let (changed, words) = match kind {
        "size" => {
            let next = ElementStyle::stepped_size(size, value);
            (
                session.restyle_elements(&ids, |style| style.font_size_pt = Some(next)),
                format!("Text size {next:.0} pt"),
            )
        }
        "bold" => (
            session.restyle_elements(&ids, |style| style.bold = Some(!bold)),
            format!("Bold {}", if bold { "off" } else { "on" }),
        ),
        "italic" => (
            session.restyle_elements(&ids, |style| style.italic = !italic),
            format!("Italic {}", if italic { "off" } else { "on" }),
        ),
        "align" => {
            let align = match value {
                0 => TextAlign::Left,
                2 => TextAlign::Right,
                _ => TextAlign::Center,
            };
            (
                session.restyle_elements(&ids, |style| style.align = Some(align)),
                format!("Aligned {align:?}").to_lowercase(),
            )
        }
        "color" => {
            let hex = slot.map(|hex| (*hex).to_string());
            (
                session.restyle_elements(&ids, |style| style.color = hex.clone()),
                if slot.is_some() {
                    "Text colour set"
                } else {
                    "Text colour automatic"
                }
                .to_string(),
            )
        }
        "fill" => {
            let hex = slot.map(|hex| (*hex).to_string());
            (
                session.restyle_elements(&ids, |style| style.fill = hex.clone()),
                if slot.is_some() {
                    "Fill set"
                } else {
                    "Fill removed"
                }
                .to_string(),
            )
        }
        _ => return None,
    };
    changed.then_some(words)
}

/// Apply a font family name, or the theme's face when the field is empty, to
/// every selected text object and shape as one undo step. In a mixed selection
/// the field is empty because the objects disagree; submitting that empty field
/// is a no-op rather than wiping every family.
pub(crate) fn apply_font(state: &GuiState, text: &str) -> Result<Option<String>, String> {
    let family = if text.trim().is_empty() {
        None
    } else {
        Some(loom_present_core::normalize_font_family(text)?)
    };
    let mut session = state.session.borrow_mut();
    let selected: Vec<(String, Option<String>)> = session
        .document
        .active_slide()
        .map(|slide| {
            slide
                .elements
                .iter()
                .filter(|element| session.selected_elements.contains(&element.id))
                .filter(|element| look(element).is_some())
                .map(|element| {
                    let family = element
                        .style
                        .font_family
                        .as_deref()
                        .and_then(|name| loom_present_core::normalize_font_family(name).ok());
                    (element.id.clone(), family)
                })
                .collect()
        })
        .unwrap_or_default();
    let mixed = selected
        .split_first()
        .is_some_and(|(first, rest)| rest.iter().any(|(_, family)| family != &first.1));
    if family.is_none() && mixed {
        return Ok(None);
    }
    let ids: Vec<String> = selected.into_iter().map(|(id, _)| id).collect();
    if ids.is_empty() {
        return Ok(None);
    }
    let changed = session.restyle_elements(&ids, |style| style.font_family = family.clone());
    Ok(changed.then(|| match family {
        Some(family) => format!("Font family {family}"),
        None => "Font family follows the theme".to_string(),
    }))
}

/// Ask for a style change as the inspector's controls do; the shortcuts and the
/// palette go through the same callback.
pub(crate) fn request(app: &PresentApp, kind: &str, value: i32) {
    app.global::<ElementInk>()
        .invoke_requested(kind.into(), value);
}

pub(crate) fn wire(app: &PresentApp, state: &Rc<GuiState>) {
    let ink = app.global::<ElementInk>();
    ink.set_palette(ModelRc::new(VecModel::from(
        PALETTE.iter().map(|hex| color(hex)).collect::<Vec<_>>(),
    )));
    ink.set_color_swatches(ModelRc::new(VecModel::from(
        OFFICE_SWATCHES
            .iter()
            .map(|(hex, name)| LoomColorSwatch {
                swatch: color(hex),
                hex: (*hex).into(),
                name: (*name).into(),
            })
            .collect::<Vec<_>>(),
    )));
    {
        let app_ref = app.as_weak();
        ink.on_color_toggled(move |kind| {
            let Some(app) = app_ref.upgrade() else { return };
            let ink = app.global::<ElementInk>();
            let cur = ink.get_active_palette();
            if cur == kind {
                ink.set_active_palette("".into());
            } else {
                ink.set_active_palette(kind);
                ink.set_hex_error("".into());
            }
        });
    }
    {
        let (state, app_ref) = (state.clone(), app.as_weak());
        ink.on_color_picked(move |kind, hex| {
            let Some(app) = app_ref.upgrade() else { return };
            let hex_val = if hex.is_empty() {
                None
            } else {
                Some(hex.to_string())
            };
            let mut session = state.session.borrow_mut();
            let changed = match kind.as_str() {
                "text" => {
                    let ids: Vec<String> = session.selected_elements.clone();
                    session.restyle_elements(&ids, |s| s.color = hex_val.clone())
                }
                "fill" => {
                    let ids: Vec<String> = session.selected_elements.clone();
                    session.restyle_elements(&ids, |s| s.fill = hex_val.clone())
                }
                "bg" => {
                    if let Some(slide) = session.document.active_slide_mut() {
                        slide.bg_color = hex_val.clone().unwrap_or_default();
                        true
                    } else {
                        false
                    }
                }
                _ => false,
            };
            drop(session);
            app.global::<ElementInk>().set_active_palette("".into());
            if changed {
                refresh(&app, &state);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        ink.on_hex_edited(move |_val| {
            if let Some(app) = app_ref.upgrade() {
                app.global::<ElementInk>().set_hex_error("".into());
            }
        });
    }
    {
        let app_ref = app.as_weak();
        ink.on_hex_submitted(move |kind, val| {
            let Some(app) = app_ref.upgrade() else { return };
            let trimmed = val.trim().trim_start_matches('#');
            if trimmed.len() == 6 && trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
                let normalized = format!("#{}", trimmed.to_ascii_uppercase());
                app.global::<ElementInk>()
                    .invoke_color_picked(kind, normalized.into());
            } else {
                app.global::<ElementInk>()
                    .set_hex_error("Enter #RRGGBB".into());
            }
        });
    }
    {
        let (state, app_ref) = (state.clone(), app.as_weak());
        ink.on_geometry_requested(move |field, text| {
            let Some(app) = app_ref.upgrade() else { return };
            let message = apply_geometry(&state, field.as_str(), text.as_str()).err();
            // Whatever happened, the fields show the object's real values again.
            refresh(&app, &state);
            if let Some(message) = message {
                set_status(&app, &message);
            }
        });
    }
    {
        let (state, app_ref) = (state.clone(), app.as_weak());
        ink.on_font_requested(move |text| {
            let Some(app) = app_ref.upgrade() else { return };
            match apply_font(&state, text.as_str()) {
                Ok(words) => {
                    // The field shows the document's canonical family again.
                    refresh(&app, &state);
                    if let Some(words) = words {
                        set_status(&app, &words);
                    }
                }
                Err(message) => {
                    // A rejected name changes nothing and the field shows the truth again.
                    refresh(&app, &state);
                    set_status(&app, &message);
                }
            }
        });
    }
    {
        let (state, app_ref) = (state.clone(), app.as_weak());
        ink.on_font_cancelled(move || {
            if let Some(app) = app_ref.upgrade() {
                refresh(&app, &state);
            }
        });
    }
    let (state, app_ref) = (state.clone(), app.as_weak());
    ink.on_requested(move |kind, value| {
        let Some(app) = app_ref.upgrade() else { return };
        if let Some(words) = apply(&state, kind.as_str(), value) {
            refresh(&app, &state);
            set_status(&app, &words);
        }
    });
}
