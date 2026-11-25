//! Wiring for Present P0 parity features: StyleEffectsPanel, AnimationInspector,
//! Hidden Slide toggle, Group/Ungroup, Lock/Unlock, Flip H/V, and Lists.

use std::cell::RefCell;
use std::rc::Rc;

use loom_present_core::animations::{
    add_animation, remove_animation, AnimationEffect, AnimationPhase, BuildTrigger,
};
use loom_present_core::group_lock::{flip_elements, group_elements, set_locked, ungroup_elements};
use loom_present_core::list_text::{format_list_paragraphs, parse_list_paragraphs, ListStyle};
use loom_present_core::rich_style::{DropShadowConfig, ElementEffects, GradientFill, StrokeDash};
use loom_present_core::{PresentationSession, SlideElement};
use slint::ComponentHandle;

use crate::{refresh, set_status, ElementInk, GuiState, PresentApp};

pub(crate) fn publish_selection_effects(app: &PresentApp, session: &PresentationSession) {
    let ink = app.global::<ElementInk>();
    let Some(slide) = session.document.active_slide() else {
        return;
    };
    ink.set_build_count(slide.animations.len() as i32);
    ink.set_slide_hidden(slide.hidden);
    ink.set_slide_footer_text(session.document.footer_text.as_str().into());
    ink.set_show_slide_number(session.document.show_slide_number);
    ink.set_active_slide_number((session.document.active_index + 1) as i32);
    let selected: Vec<&SlideElement> = slide
        .elements
        .iter()
        .filter(|e| session.selected_elements.contains(&e.id))
        .collect();
    if let Some(first) = selected.first() {
        let eff = first.effects.clone().unwrap_or_default();
        ink.set_effects_opacity_pct((eff.opacity * 100.0).round() as i32);
        ink.set_effects_corner_radius_px(eff.corner_radius.round() as i32);
        ink.set_effects_stroke_width_px(eff.stroke_width.round() as i32);
        ink.set_effects_stroke_dash(match eff.stroke_dash {
            StrokeDash::Solid => 0,
            StrokeDash::Dashed => 1,
            StrokeDash::Dotted => 2,
        });
        ink.set_effects_has_shadow(eff.shadow.is_some());
        ink.set_effects_has_gradient(eff.gradient.is_some());
        ink.set_selection_locked(selected.iter().any(|e| e.locked));
        ink.set_selection_grouped(selected.iter().any(|e| e.group_id.is_some()));
    } else {
        ink.set_selection_locked(false);
        ink.set_selection_grouped(false);
    }
}

fn mutate_selected_effects(
    app: &PresentApp,
    state: &Rc<GuiState>,
    label: &str,
    mut f: impl FnMut(&mut ElementEffects),
) {
    let mut session = state.session.borrow_mut();
    let ids = session.selected_elements.clone();
    if ids.is_empty() {
        return;
    }
    session.checkpoint();
    let mut changed = false;
    if let Some(slide) = session.document.active_slide_mut() {
        for el in &mut slide.elements {
            if ids.contains(&el.id) && !el.locked {
                let mut eff = el.effects.clone().unwrap_or_default();
                f(&mut eff);
                el.effects = if eff.is_default() { None } else { Some(eff) };
                changed = true;
            }
        }
    }
    if changed {
        drop(session);
        refresh(app, state);
        set_status(app, label);
    } else {
        set_status(app, "Selection is locked");
    }
}

pub(crate) fn handle_arrange_parity(app: &PresentApp, state: &Rc<GuiState>, action: &str) -> bool {
    let mut session = state.session.borrow_mut();
    let ids: Vec<String> = session.selected_elements.to_vec();
    if ids.is_empty() {
        return false;
    }
    session.checkpoint();
    let Some(slide) = session.document.active_slide_mut() else {
        return false;
    };
    let msg = match action {
        "group" => {
            if group_elements(slide, &ids).is_some() {
                "Grouped elements"
            } else {
                "Select 2+ elements to group"
            }
        }
        "ungroup" => {
            let gids: Vec<String> = slide
                .elements
                .iter()
                .filter(|e| ids.contains(&e.id))
                .filter_map(|e| e.group_id.clone())
                .collect();
            let mut n = 0;
            for gid in gids {
                n += ungroup_elements(slide, &gid);
            }
            if n > 0 {
                "Ungrouped elements"
            } else {
                "Selection is not grouped"
            }
        }
        "toggle-lock" => {
            let any_locked = slide
                .elements
                .iter()
                .any(|e| ids.contains(&e.id) && e.locked);
            set_locked(slide, &ids, !any_locked);
            if !any_locked {
                "Locked selection"
            } else {
                "Unlocked selection"
            }
        }
        "flip-h" => {
            if flip_elements(slide, &ids, true) > 0 {
                "Flipped horizontally"
            } else {
                "Selection is locked"
            }
        }
        "flip-v" => {
            if flip_elements(slide, &ids, false) > 0 {
                "Flipped vertically"
            } else {
                "Selection is locked"
            }
        }
        _ => return false,
    };
    drop(session);
    refresh(app, state);
    set_status(app, msg);
    true
}

pub(crate) fn toggle_list_style(app: &PresentApp, state: &Rc<GuiState>, style: ListStyle) {
    let mut session = state.session.borrow_mut();
    let ids = session.selected_elements.clone();
    if let Some(slide) = session.document.active_slide_mut() {
        for el in &mut slide.elements {
            if ids.contains(&el.id) && !el.locked {
                let mut paras = parse_list_paragraphs(&el.content);
                for p in &mut paras {
                    p.style = if p.style == style {
                        ListStyle::None
                    } else {
                        style
                    };
                }
                el.content = format_list_paragraphs(&paras);
            }
        }
    }
    drop(session);
    refresh(app, state);
}

pub(crate) fn wire(app: &PresentApp, state: &Rc<GuiState>) {
    let ink = app.global::<ElementInk>();

    {
        let app_ref = app.as_weak();
        let state = state.clone();
        ink.on_effects_set_opacity(move |pct| {
            if let Some(app) = app_ref.upgrade() {
                let op = (pct.clamp(10, 100) as f32) / 100.0;
                mutate_selected_effects(&app, &state, "Updated opacity", |e| e.opacity = op);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        let state = state.clone();
        ink.on_effects_set_corner_radius(move |px| {
            if let Some(app) = app_ref.upgrade() {
                let r = px.clamp(0, 48) as f32;
                mutate_selected_effects(&app, &state, "Updated corner radius", |e| {
                    e.corner_radius = r
                });
            }
        });
    }
    {
        let app_ref = app.as_weak();
        let state = state.clone();
        ink.on_effects_set_stroke_width(move |px| {
            if let Some(app) = app_ref.upgrade() {
                let w = px.clamp(0, 12) as f32;
                mutate_selected_effects(&app, &state, "Updated stroke width", |e| {
                    e.stroke_width = w;
                    if w > 0.0 && e.stroke_color.is_none() {
                        e.stroke_color = Some("#1d1d1f".into());
                    }
                });
            }
        });
    }
    {
        let app_ref = app.as_weak();
        let state = state.clone();
        ink.on_effects_cycle_stroke_dash(move || {
            if let Some(app) = app_ref.upgrade() {
                mutate_selected_effects(&app, &state, "Cycled stroke dash", |e| {
                    e.stroke_dash = match e.stroke_dash {
                        StrokeDash::Solid => StrokeDash::Dashed,
                        StrokeDash::Dashed => StrokeDash::Dotted,
                        StrokeDash::Dotted => StrokeDash::Solid,
                    };
                });
            }
        });
    }
    {
        let app_ref = app.as_weak();
        let state = state.clone();
        ink.on_effects_toggle_shadow(move || {
            if let Some(app) = app_ref.upgrade() {
                mutate_selected_effects(&app, &state, "Toggled drop shadow", |e| {
                    e.shadow = if e.shadow.is_some() {
                        None
                    } else {
                        Some(DropShadowConfig::default())
                    };
                });
            }
        });
    }
    {
        let app_ref = app.as_weak();
        let state = state.clone();
        ink.on_effects_toggle_gradient(move || {
            if let Some(app) = app_ref.upgrade() {
                mutate_selected_effects(&app, &state, "Toggled gradient fill", |e| {
                    e.gradient = if e.gradient.is_some() {
                        None
                    } else {
                        Some(GradientFill::default())
                    };
                });
            }
        });
    }
    {
        let app_ref = app.as_weak();
        let state = state.clone();
        ink.on_add_build(move |phase_idx, trigger_idx| {
            if let Some(app) = app_ref.upgrade() {
                let mut session = state.session.borrow_mut();
                let Some(el_id) = session.selected_elements.first().cloned() else {
                    return;
                };
                let phase = match phase_idx {
                    1 => AnimationPhase::Action,
                    2 => AnimationPhase::BuildOut,
                    _ => AnimationPhase::BuildIn,
                };
                let trigger = match trigger_idx {
                    1 => BuildTrigger::WithPrevious,
                    2 => BuildTrigger::AfterPrevious,
                    _ => BuildTrigger::OnClick,
                };
                if let Some(slide) = session.document.active_slide_mut() {
                    add_animation(slide, el_id, AnimationEffect::FadeIn, phase, trigger, 350);
                }
                drop(session);
                refresh(&app, &state);
                set_status(&app, "Added build animation");
            }
        });
    }
    {
        let app_ref = app.as_weak();
        let state = state.clone();
        ink.on_remove_last_build(move || {
            if let Some(app) = app_ref.upgrade() {
                let mut session = state.session.borrow_mut();
                if let Some(slide) = session.document.active_slide_mut() {
                    if !slide.animations.is_empty() {
                        let last = slide.animations.len() - 1;
                        remove_animation(slide, last);
                    }
                }
                drop(session);
                refresh(&app, &state);
                set_status(&app, "Removed build animation");
            }
        });
    }
    {
        let app_ref = app.as_weak();
        ink.on_cycle_build_phase(move || {
            if let Some(app) = app_ref.upgrade() {
                let g = app.global::<ElementInk>();
                g.set_build_phase((g.get_build_phase() + 1) % 3);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        ink.on_cycle_build_trigger(move || {
            if let Some(app) = app_ref.upgrade() {
                let g = app.global::<ElementInk>();
                g.set_build_trigger((g.get_build_trigger() + 1) % 3);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        let state = state.clone();
        ink.on_toggle_slide_hidden(move || {
            if let Some(app) = app_ref.upgrade() {
                let mut session = state.session.borrow_mut();
                let mut hidden = false;
                if let Some(slide) = session.document.active_slide_mut() {
                    slide.hidden = !slide.hidden;
                    hidden = slide.hidden;
                }
                drop(session);
                refresh(&app, &state);
                set_status(
                    &app,
                    if hidden {
                        "Slide hidden"
                    } else {
                        "Slide visible"
                    },
                );
            }
        });
    }
    wire_p1_controls(app, state);
}

pub(crate) fn select_prev_slide(app: &PresentApp, state: &Rc<GuiState>) {
    let target = {
        let session = state.session.borrow();
        let index = session.document.active_index;
        if app.get_is_preview_mode() {
            loom_present_core::animations::prev_visible_slide(&session.document.slides, index)
        } else {
            index.checked_sub(1)
        }
    };
    if let Some(target) = target {
        state.session.borrow_mut().document.select_slide(target);
        state.selected_element.set(0);
        refresh(app, state);
    }
}

pub(crate) fn select_next_slide(app: &PresentApp, state: &Rc<GuiState>) {
    let target = {
        let session = state.session.borrow();
        let index = session.document.active_index;
        if app.get_is_preview_mode() {
            loom_present_core::animations::next_visible_slide(&session.document.slides, index)
        } else if index + 1 < session.document.len() {
            Some(index + 1)
        } else {
            None
        }
    };
    if let Some(target) = target {
        state.session.borrow_mut().document.select_slide(target);
        state.selected_element.set(0);
        refresh(app, state);
    }
}

pub(crate) fn handle_element_moved(app: &PresentApp, state: &Rc<GuiState>, dx: f32, dy: f32) {
    if app.get_is_preview_mode() {
        return;
    }
    let mut drag = state.drag_state.borrow_mut();
    if drag.mode != Some(crate::HandleKind::Move) {
        return;
    }
    let (targets, guides) = {
        let session = state.session.borrow();
        crate::move_targets(&session, &drag.elements, dx, dy)
    };
    let changed = {
        let session = state.session.borrow();
        targets.iter().any(|(id, x, y, width, height)| {
            session
                .document
                .active_slide()
                .and_then(|slide| slide.elements.iter().find(|element| element.id == *id))
                .filter(|element| !element.locked)
                .map(|element| {
                    (element.x - *x).abs() > f32::EPSILON
                        || (element.y - *y).abs() > f32::EPSILON
                        || (element.width - *width).abs() > f32::EPSILON
                        || (element.height - *height).abs() > f32::EPSILON
                })
                .unwrap_or(false)
        })
    };
    if changed {
        let mut session = state.session.borrow_mut();
        if !drag.checkpointed {
            session.checkpoint();
            drag.checkpointed = true;
        }
        for (id, x, y, width, height) in targets {
            let locked = session
                .document
                .active_slide()
                .is_some_and(|s| loom_present_core::group_lock::is_element_locked(s, &id));
            if !locked {
                session.transform_element_no_checkpoint(&id, x, y, width, height);
            }
        }
        drag.guides = guides;
    }
    drop(drag);
    crate::refresh_without_recovery(app, state);
}

thread_local! {
    static FORMAT_PAINTER: RefCell<Option<(loom_present_core::ElementStyle, Option<ElementEffects>)>> = const { RefCell::new(None) };
}

pub(crate) fn wire_p1_controls(app: &PresentApp, state: &Rc<GuiState>) {
    let ink = app.global::<ElementInk>();
    {
        let app_ref = app.as_weak();
        let state = Rc::clone(state);
        ink.on_toggle_slide_sorter(move || {
            if let Some(app) = app_ref.upgrade() {
                let ink = app.global::<ElementInk>();
                ink.set_slide_sorter_open(!ink.get_slide_sorter_open());
                refresh(&app, &state);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        let state = Rc::clone(state);
        app.global::<crate::ObjectParts>()
            .on_sorter_select_slide(move |idx| {
                if let Some(app) = app_ref.upgrade() {
                    if idx >= 0 {
                        state
                            .session
                            .borrow_mut()
                            .document
                            .select_slide(idx as usize);
                        state.selected_element.set(0);
                        refresh(&app, &state);
                    }
                }
            });
    }
    {
        let app_ref = app.as_weak();
        let state = Rc::clone(state);
        ink.on_set_slide_footer(move |text| {
            if let Some(app) = app_ref.upgrade() {
                let mut session = state.session.borrow_mut();
                session.checkpoint();
                session.document.footer_text = text.to_string();
                drop(session);
                refresh(&app, &state);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        let state = Rc::clone(state);
        ink.on_toggle_slide_number(move || {
            if let Some(app) = app_ref.upgrade() {
                let mut session = state.session.borrow_mut();
                session.checkpoint();
                session.document.show_slide_number = !session.document.show_slide_number;
                drop(session);
                refresh(&app, &state);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        let state = Rc::clone(state);
        ink.on_copy_format_style(move || {
            if let Some(app) = app_ref.upgrade() {
                let session = state.session.borrow();
                if let Some(el) = session.document.active_slide().and_then(|s| {
                    s.elements
                        .iter()
                        .find(|e| session.selected_elements.contains(&e.id))
                }) {
                    FORMAT_PAINTER.with(|fp| {
                        *fp.borrow_mut() = Some((el.style.clone(), el.effects.clone()));
                    });
                    app.global::<ElementInk>().set_format_painter_active(true);
                    set_status(&app, "Copied element style");
                }
            }
        });
    }
    {
        let app_ref = app.as_weak();
        let state = Rc::clone(state);
        ink.on_apply_format_style(move || {
            if let Some(app) = app_ref.upgrade() {
                let copied = FORMAT_PAINTER.with(|fp| fp.borrow().clone());
                if let Some((style, effects)) = copied {
                    let mut session = state.session.borrow_mut();
                    let ids = session.selected_elements.clone();
                    if !ids.is_empty() {
                        session.checkpoint();
                        if let Some(slide) = session.document.active_slide_mut() {
                            for el in &mut slide.elements {
                                if ids.contains(&el.id) && !el.locked {
                                    el.style = style.clone();
                                    el.effects = effects.clone();
                                }
                            }
                        }
                        drop(session);
                        refresh(&app, &state);
                        set_status(&app, "Applied element style");
                    }
                }
            }
        });
    }
}

pub(crate) fn handle_handle_moved(
    app: &PresentApp,
    state: &Rc<GuiState>,
    kind: &str,
    dx: f32,
    dy: f32,
) {
    if app.get_is_preview_mode() {
        return;
    }
    let mut drag = state.drag_state.borrow_mut();
    let mode = crate::handle_kind(kind);
    if drag.mode != Some(mode) {
        return;
    }
    let Some(target_id) = drag.target_id.as_deref() else {
        return;
    };
    let Some(element) = drag
        .elements
        .iter()
        .find(|element| element.id == target_id)
        .cloned()
    else {
        return;
    };
    let mut session = state.session.borrow_mut();
    if session
        .document
        .active_slide()
        .is_some_and(|s| loom_present_core::group_lock::is_element_locked(s, &element.id))
    {
        return;
    }
    let (x, y, width, height, changed) = if mode == crate::HandleKind::Rotate {
        let rotation = crate::normalize_angle_degrees(element.rotation_deg + dx * 0.5);
        let changed = session
            .document
            .active_slide()
            .and_then(|slide| slide.elements.iter().find(|item| item.id == element.id))
            .map(|item| (item.rotation_deg - rotation).abs() > f32::EPSILON)
            .unwrap_or(false);
        if changed && !drag.checkpointed {
            session.checkpoint();
            drag.checkpointed = true;
        }
        let changed = session.set_element_rotation_no_checkpoint(&element.id, rotation);
        (element.x, element.y, element.width, element.height, changed)
    } else {
        let ((mut x, mut y, mut width, mut height), guides) =
            crate::resize_targets(&session, &element, mode, dx, dy);
        let is_picture = session
            .document
            .active_slide()
            .and_then(|slide| slide.elements.iter().find(|item| item.id == element.id))
            .is_some_and(|item| item.element_type == loom_present_core::ElementType::Picture);
        if is_picture {
            (x, y, width, height) = crate::lock_aspect(
                (element.x, element.y, element.width, element.height),
                (x, y, width, height),
                matches!(
                    mode,
                    crate::HandleKind::ResizeNorthWest | crate::HandleKind::ResizeSouthWest
                ),
                matches!(
                    mode,
                    crate::HandleKind::ResizeNorthWest | crate::HandleKind::ResizeNorthEast
                ),
            );
        }
        let changed = session
            .document
            .active_slide()
            .and_then(|slide| slide.elements.iter().find(|item| item.id == element.id))
            .map(|item| {
                (item.x - x).abs() > f32::EPSILON
                    || (item.y - y).abs() > f32::EPSILON
                    || (item.width - width).abs() > f32::EPSILON
                    || (item.height - height).abs() > f32::EPSILON
            })
            .unwrap_or(false);
        if changed && !drag.checkpointed {
            session.checkpoint();
            drag.checkpointed = true;
        }
        drag.guides = guides;
        (x, y, width, height, changed)
    };
    if mode != crate::HandleKind::Rotate && changed {
        session.transform_element_no_checkpoint(&element.id, x, y, width, height);
    }
    drop(session);
    drop(drag);
    crate::refresh_without_recovery(app, state);
}
