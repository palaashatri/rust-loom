//! The inspector's text style controls: showing the selected object's look and
//! applying a choice to every selected text object or shape as one undo step.

use std::rc::Rc;

use loom_present_core::{ElementStyle, TextAlign, PALETTE};
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::object_parts::{color, look};
use crate::{refresh, set_status, ElementInk, GuiState, PresentApp};

fn slot_of(hex: Option<&str>) -> i32 {
    hex.and_then(|hex| PALETTE.iter().position(|p| p.eq_ignore_ascii_case(hex)))
        .map_or(-1, |slot| slot as i32)
}

/// Show the active object's style in the inspector; hide the controls when it
/// is a picture, table or chart.
pub(crate) fn publish(app: &PresentApp, element: &loom_present_core::SlideElement) {
    let ink = app.global::<ElementInk>();
    let Some(look) = look(element) else {
        ink.set_available(false);
        return;
    };
    ink.set_available(true);
    ink.set_size_label(format!("{:.0} pt", look.size_pt).into());
    ink.set_bold(look.bold);
    ink.set_italic(look.italic);
    ink.set_align(look.align);
    ink.set_color_slot(slot_of(element.style.color.as_deref()));
    ink.set_fill_slot(slot_of(element.style.fill.as_deref()));
}

pub(crate) fn clear(app: &PresentApp) {
    app.global::<ElementInk>().set_available(false);
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
    let (state, app_ref) = (state.clone(), app.as_weak());
    ink.on_requested(move |kind, value| {
        let Some(app) = app_ref.upgrade() else { return };
        if let Some(words) = apply(&state, kind.as_str(), value) {
            refresh(&app, &state);
            set_status(&app, &words);
        }
    });
}
