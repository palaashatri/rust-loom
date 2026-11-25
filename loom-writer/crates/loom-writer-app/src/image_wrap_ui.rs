//! Wiring for `WriterImageWrapState` inspector controls.

use std::rc::Rc;

use loom_objects::{ImageWrapMode, WrapHorizontalAlign, WrapSide};
use loom_writer_core::WriterDocument;
use slint::ComponentHandle;

use crate::picture_actions::selected_picture;
use crate::{apply_with_history, GuiState, HistoryKind, PageImages, WriterApp};

fn mode_to_index(mode: ImageWrapMode) -> i32 {
    match mode {
        ImageWrapMode::Inline => 0,
        ImageWrapMode::Square => 1,
        ImageWrapMode::Tight => 2,
        ImageWrapMode::TopAndBottom => 3,
        ImageWrapMode::BehindText => 4,
        ImageWrapMode::InFrontOfText => 5,
    }
}

fn index_to_mode(idx: i32) -> ImageWrapMode {
    match idx {
        1 => ImageWrapMode::Square,
        2 => ImageWrapMode::Tight,
        3 => ImageWrapMode::TopAndBottom,
        4 => ImageWrapMode::BehindText,
        5 => ImageWrapMode::InFrontOfText,
        _ => ImageWrapMode::Inline,
    }
}

fn side_to_index(side: WrapSide) -> i32 {
    match side {
        WrapSide::Both => 0,
        WrapSide::LeftOnly => 1,
        WrapSide::RightOnly => 2,
        WrapSide::Largest => 3,
    }
}

fn index_to_side(idx: i32) -> WrapSide {
    match idx {
        1 => WrapSide::LeftOnly,
        2 => WrapSide::RightOnly,
        3 => WrapSide::Largest,
        _ => WrapSide::Both,
    }
}

fn align_to_index(align: WrapHorizontalAlign) -> i32 {
    match align {
        WrapHorizontalAlign::Center => 1,
        WrapHorizontalAlign::Right => 2,
        _ => 0,
    }
}

pub(crate) fn publish(app: &WriterApp, doc: &WriterDocument) {
    let g = app.global::<PageImages>();
    let Some(block) = selected_picture(doc) else {
        if g.get_wrap_has_image() {
            g.set_wrap_has_image(false);
        }
        return;
    };
    let Some(img) = block.image.as_ref() else {
        if g.get_wrap_has_image() {
            g.set_wrap_has_image(false);
        }
        return;
    };
    let wrap = img.wrap.unwrap_or_default();
    g.set_wrap_has_image(true);
    g.set_wrap_mode_index(mode_to_index(wrap.mode));
    g.set_wrap_side_index(side_to_index(wrap.side));
    g.set_wrap_align_index(align_to_index(wrap.horizontal_align));
    let x_pt = match wrap.horizontal_align {
        WrapHorizontalAlign::OffsetPt(x) => x,
        _ => 0.0,
    };
    g.set_wrap_x_offset_label(format!("{x_pt:.0} pt").into());
    g.set_wrap_y_offset_label(format!("{:.0} pt", img.wrap_y_offset_pt).into());
    g.set_wrap_margin_left_label(format!("{:.0} pt", wrap.margins.left_pt).into());
    g.set_wrap_margin_right_label(format!("{:.0} pt", wrap.margins.right_pt).into());
}

fn mutate_selected_wrap(
    app: &WriterApp,
    state: &GuiState,
    f: impl FnOnce(&mut WriterDocument, u64) -> bool,
) {
    let Some(id) = selected_picture(&state.current.borrow()).map(|b| b.id) else {
        return;
    };
    let mut next = state.current.borrow().clone();
    if f(&mut next, id) {
        let _ = apply_with_history(app, state, next, HistoryKind::DocumentAction);
        publish(app, &state.current.borrow());
    }
}

pub(crate) fn wire(app: &WriterApp, state: &Rc<GuiState>) {
    let g = app.global::<PageImages>();
    {
        let app_ref = app.as_weak();
        let state = state.clone();
        g.on_set_wrap_mode(move |idx| {
            if let Some(app) = app_ref.upgrade() {
                mutate_selected_wrap(&app, &state, |doc, id| {
                    doc.set_image_wrap_mode(id, index_to_mode(idx))
                });
            }
        });
    }
    {
        let app_ref = app.as_weak();
        let state = state.clone();
        g.on_set_wrap_side(move |idx| {
            if let Some(app) = app_ref.upgrade() {
                mutate_selected_wrap(&app, &state, |doc, id| {
                    doc.set_image_wrap_side(id, index_to_side(idx))
                });
            }
        });
    }
    {
        let app_ref = app.as_weak();
        let state = state.clone();
        g.on_set_wrap_align(move |idx| {
            if let Some(app) = app_ref.upgrade() {
                let align = match idx {
                    1 => WrapHorizontalAlign::Center,
                    2 => WrapHorizontalAlign::Right,
                    _ => WrapHorizontalAlign::Left,
                };
                mutate_selected_wrap(&app, &state, |doc, id| {
                    let cur = doc.image_wrap(id).unwrap_or_default();
                    doc.set_image_wrap_align(id, cur.horizontal_anchor, cur.vertical_anchor, align)
                });
            }
        });
    }
    {
        let app_ref = app.as_weak();
        let state = state.clone();
        g.on_step_x_offset(move |delta| {
            if let Some(app) = app_ref.upgrade() {
                mutate_selected_wrap(&app, &state, |doc, id| {
                    let cur = doc
                        .image_wrap(id)
                        .map_or(0.0, |w| match w.horizontal_align {
                            WrapHorizontalAlign::OffsetPt(x) => x,
                            _ => 0.0,
                        });
                    let cur_y = doc
                        .blocks
                        .iter()
                        .find(|b| b.id == id)
                        .and_then(|b| b.image.as_ref())
                        .map_or(0.0, |i| i.wrap_y_offset_pt);
                    doc.set_image_wrap_offset(id, (cur + delta).max(0.0), cur_y)
                });
            }
        });
    }
    {
        let app_ref = app.as_weak();
        let state = state.clone();
        g.on_step_y_offset(move |delta| {
            if let Some(app) = app_ref.upgrade() {
                mutate_selected_wrap(&app, &state, |doc, id| {
                    let cur_x = doc
                        .image_wrap(id)
                        .map_or(0.0, |w| match w.horizontal_align {
                            WrapHorizontalAlign::OffsetPt(x) => x,
                            _ => 0.0,
                        });
                    let cur_y = doc
                        .blocks
                        .iter()
                        .find(|b| b.id == id)
                        .and_then(|b| b.image.as_ref())
                        .map_or(0.0, |i| i.wrap_y_offset_pt);
                    doc.set_image_wrap_offset(id, cur_x, (cur_y + delta).max(0.0))
                });
            }
        });
    }
    {
        let app_ref = app.as_weak();
        let state = state.clone();
        g.on_step_wrap_margin(move |side, delta| {
            if let Some(app) = app_ref.upgrade() {
                mutate_selected_wrap(&app, &state, |doc, id| {
                    let mut m = doc
                        .image_wrap(id)
                        .map_or_else(Default::default, |w| w.margins);
                    if side.as_str() == "left" {
                        m.left_pt = (m.left_pt + delta).clamp(0.0, 72.0);
                    } else {
                        m.right_pt = (m.right_pt + delta).clamp(0.0, 72.0);
                    }
                    doc.set_image_wrap_margins(id, m)
                });
            }
        });
    }
}
