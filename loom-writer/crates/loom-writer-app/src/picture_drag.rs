//! Mouse and keyboard resizing of the selected picture.
//!
//! A press on a picture selects it: the caret goes to its block and the page
//! draws its outline and corner handles. Dragging a handle resizes the picture
//! while the page shows the new size, and the release commits one undoable
//! step. Escape cancels a drag. Alt+Shift+Right and Alt+Shift+Left grow and
//! shrink the selected picture by ten percent.
//!
//! An inline picture keeps its place in the text: its top edge stays on its
//! line and its alignment still decides where it starts, so the preview and
//! the committed layout put the picture in the same place.

use std::cell::RefCell;

use loom_text::Alignment;
use loom_writer_core::WriterDocument;
use slint::platform::Key;
use slint::SharedString;

use crate::{apply_with_history, picture_actions, picture_view, table_actions};
use crate::{GuiState, HistoryKind, WriterApp};

/// Pointer distance, in pixels, that still counts as on a corner handle.
const HANDLE_REACH_PX: f32 = 6.0;
/// The shorter side of a picture never goes below this, in points.
const MIN_SIDE_PT: f32 = 16.0;
/// Keyboard steps: ten percent larger or smaller.
const GROW: f32 = 1.1;
const SHRINK: f32 = 0.9;
/// A change narrower than this, in points, is no change at all.
const SAME_PT: f32 = 0.5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl Corner {
    /// The direction the corner moves when the picture grows.
    fn outward(self) -> (f32, f32) {
        match self {
            Corner::TopLeft => (-1.0, -1.0),
            Corner::TopRight => (1.0, -1.0),
            Corner::BottomLeft => (-1.0, 1.0),
            Corner::BottomRight => (1.0, 1.0),
        }
    }
}

/// A corner drag in progress. Sizes are in points; pointer positions in pixels.
struct Drag {
    /// Identity of the picture on the page (see `picture_view::picture_key`).
    key: usize,
    corner: Corner,
    start: (f32, f32),
    zoom: f32,
    /// The size and column the picture was laid out in when the drag started.
    shown_width: f32,
    shown_height: f32,
    column_start: f32,
    column: f32,
    alignment: Alignment,
    top: f32,
    /// The stored size of the picture and the text area it must fit.
    stored: (f32, f32),
    usable: (f32, f32),
    /// The width the drag shows now.
    width: f32,
}

thread_local! {
    static DRAG: RefCell<Option<Drag>> = const { RefCell::new(None) };
}

/// The width a picture may take: its shorter side at least sixteen points,
/// no wider than the text width and no taller than the text area, always at
/// the picture's own proportions.
fn clamp_width(width: f32, stored: (f32, f32), usable: (f32, f32)) -> f32 {
    let aspect = stored.0 / stored.1.max(f32::EPSILON);
    let min = MIN_SIDE_PT * aspect.max(1.0);
    let max = usable.0.min(usable.1 * aspect).max(1.0);
    width.max(min.min(max)).min(max)
}

/// How far the layout moves a picture of `width` from the start of its column.
fn alignment_shift(alignment: Alignment, column: f32, width: f32) -> f32 {
    debug_assert!(
        !matches!(alignment, Alignment::Start | Alignment::End),
        "picture positioning requires logical alignment to be resolved"
    );
    match alignment {
        Alignment::Center => ((column - width) / 2.0).max(0.0),
        Alignment::Right => (column - width).max(0.0),
        Alignment::Left | Alignment::Justify | Alignment::Start | Alignment::End => 0.0,
    }
}

fn picture_status(width: f32, height: f32) -> String {
    format!("Picture {width:.0} x {height:.0} pt")
}

fn handle_at(rect: &picture_view::LaidOut, zoom: f32, x: f32, y: f32) -> Option<Corner> {
    let (left, top) = (rect.x * zoom, rect.y * zoom);
    let (right, bottom) = ((rect.x + rect.width) * zoom, (rect.y + rect.height) * zoom);
    [
        (Corner::TopLeft, left, top),
        (Corner::TopRight, right, top),
        (Corner::BottomLeft, left, bottom),
        (Corner::BottomRight, right, bottom),
    ]
    .into_iter()
    .find(|(_, cx, cy)| (x - cx).abs() <= HANDLE_REACH_PX && (y - cy).abs() <= HANDLE_REACH_PX)
    .map(|(corner, _, _)| corner)
}

fn inside(rect: &picture_view::LaidOut, zoom: f32, x: f32, y: f32) -> bool {
    x >= rect.x * zoom
        && x <= (rect.x + rect.width) * zoom
        && y >= rect.y * zoom
        && y <= (rect.y + rect.height) * zoom
}

/// The document offset where a block starts.
fn block_start(document: &WriterDocument, id: u64) -> Option<usize> {
    let mut start = 0usize;
    for block in &document.blocks {
        if block.id == id {
            return Some(start);
        }
        start += block.text.len_bytes() + 1;
    }
    None
}

/// A press on the page. A corner handle of the selected picture starts a
/// resize; any other press on a picture selects it. Returns `false` when the
/// press is not on a picture, so the text editor handles it.
pub(crate) fn pointer_pressed(app: &WriterApp, state: &GuiState, x: f32, y: f32) -> bool {
    if DRAG.with(|slot| slot.borrow().is_some()) {
        return true;
    }
    let laid_out = picture_view::laid_out();
    if laid_out.is_empty() {
        return false;
    }
    let zoom = crate::normalize_page_zoom(state.viewport.borrow().zoom, 1.0);
    let selected = picture_actions::picture_key(&state.current.borrow());
    if let Some(rect) = selected.and_then(|key| laid_out.iter().find(|rect| rect.key == key)) {
        if let Some(corner) = handle_at(rect, zoom, x, y) {
            return begin_drag(state, rect, corner, zoom, (x, y));
        }
    }
    let Some(rect) = laid_out.iter().find(|rect| inside(rect, zoom, x, y)) else {
        return false;
    };
    select(app, state, rect.key);
    true
}

/// Puts the caret at the start of the picture's block, which makes it the
/// selected picture and draws its outline.
fn select(app: &WriterApp, state: &GuiState, key: usize) {
    let offset = {
        let document = state.current.borrow();
        picture_actions::picture_block_for_key(&document, key)
            .and_then(|block| block_start(&document, block.id))
    };
    let Some(offset) = offset else { return };
    if table_actions::get_table_cell_selection_range().is_some() {
        table_actions::clear_table_cell_selection_range();
        crate::refresh_render_projection_of(app, state);
    }
    crate::press_reveal::during_press(|| {
        app.invoke_selection_changed(offset as i32, offset as i32);
    });
}

fn begin_drag(
    state: &GuiState,
    rect: &picture_view::LaidOut,
    corner: Corner,
    zoom: f32,
    start: (f32, f32),
) -> bool {
    let document = state.current.borrow();
    let Some(block) = picture_actions::picture_block_for_key(&document, rect.key) else {
        return false;
    };
    let Some(image) = block.image.as_ref() else {
        return false;
    };
    let style = document.page.page_style();
    let usable = (
        style.width_pt - style.margin_left_pt - style.margin_right_pt,
        style.height_pt - style.margin_top_pt - style.margin_bottom_pt,
    );
    let column = block.text_column_width(&style);
    let alignment = block
        .style
        .alignment
        .resolve_with_direction(block.text.as_str(), block.style.direction);
    let shown_width = rect.width.max(1.0);
    let drag = Drag {
        key: rect.key,
        corner,
        start,
        zoom,
        shown_width,
        shown_height: rect.height.max(1.0),
        column_start: rect.x - alignment_shift(alignment, column, shown_width),
        column,
        alignment,
        top: rect.y,
        stored: (image.width_pt, image.height_pt),
        usable,
        width: shown_width,
    };
    DRAG.with(|slot| *slot.borrow_mut() = Some(drag));
    true
}

/// A pointer move during a drag: the page shows the picture at the new size.
/// `false` when no drag is running.
pub(crate) fn pointer_moved(x: f32, y: f32) -> bool {
    let preview = DRAG.with(|slot| {
        let mut slot = slot.borrow_mut();
        let drag = slot.as_mut()?;
        let dx = (x - drag.start.0) / drag.zoom;
        let dy = (y - drag.start.1) / drag.zoom;
        let (sx, sy) = drag.corner.outward();
        let across = sx * dx / drag.shown_width;
        let down = sy * dy / drag.shown_height;
        let scale = 1.0
            + if across.abs() >= down.abs() {
                across
            } else {
                down
            };
        let width = clamp_width(drag.shown_width * scale, drag.stored, drag.usable);
        drag.width = width;
        let height = width * drag.stored.1 / drag.stored.0;
        let left = drag.column_start + alignment_shift(drag.alignment, drag.column, width);
        Some((drag.key, left, drag.top, width, height))
    });
    let Some((key, left, top, width, height)) = preview else {
        return false;
    };
    picture_view::preview(key, left, top, width, height);
    true
}

/// The release of a drag. A real size change is one undo step; a release
/// without one leaves the document alone. `false` when no drag is running.
pub(crate) fn pointer_released(app: &WriterApp, state: &GuiState) -> bool {
    let Some(drag) = DRAG.with(|slot| slot.borrow_mut().take()) else {
        return false;
    };
    let id = picture_actions::picture_block_for_key(&state.current.borrow(), drag.key)
        .map(|block| block.id);
    let changed = (drag.width - drag.shown_width).abs() >= SAME_PT;
    if let (true, Some(id)) = (changed, id) {
        let mut next = state.current.borrow().clone();
        if next.scale_image_block(id, drag.width / drag.stored.0) {
            if apply_with_history(app, state, next, HistoryKind::DocumentAction) {
                app.set_status_left(
                    picture_status(drag.width, drag.width * drag.stored.1 / drag.stored.0).into(),
                );
                return true;
            }
        }
    }
    picture_view::restore(drag.key);
    true
}

/// Cancels a drag in progress and shows the picture at its old size. `false`
/// when no drag is running.
pub(crate) fn cancel() -> bool {
    let Some(drag) = DRAG.with(|slot| slot.borrow_mut().take()) else {
        return false;
    };
    picture_view::restore(drag.key);
    true
}

/// Keys for pictures: Escape cancels a drag; Alt+Shift+Right and Alt+Shift+Left
/// resize the selected picture by ten percent. `true` when the key was used.
pub(crate) fn handle_key(
    app: &WriterApp,
    state: &GuiState,
    key: &str,
    shift: bool,
    ctrl: bool,
    alt: bool,
) -> bool {
    let is = |code: Key| {
        let text: SharedString = code.into();
        key == text.as_str()
    };
    if is(Key::Escape) {
        return cancel();
    }
    if !shift || !alt || ctrl {
        return false;
    }
    let factor = if is(Key::RightArrow) {
        GROW
    } else if is(Key::LeftArrow) {
        SHRINK
    } else {
        return false;
    };
    resize_selected(app, state, factor)
}

/// Scales the selected picture by `factor` from its displayed size, within the
/// same limits as a drag, and says the size it now has. `false` when no
/// picture is selected, so the key is not consumed.
fn resize_selected(app: &WriterApp, state: &GuiState, factor: f32) -> bool {
    let (id, stored, usable, shown) = {
        let document = state.current.borrow();
        let Some(block) = picture_actions::selected_picture(&document) else {
            return false;
        };
        let Some(image) = block.image.as_ref() else {
            return false;
        };
        let style = document.page.page_style();
        let usable = (
            style.width_pt - style.margin_left_pt - style.margin_right_pt,
            style.height_pt - style.margin_top_pt - style.margin_bottom_pt,
        );
        let shown = block
            .image_display_size(usable.0, usable.1)
            .map_or(image.width_pt, |(width, _)| width);
        (block.id, (image.width_pt, image.height_pt), usable, shown)
    };
    let aspect = stored.1 / stored.0;
    let target = clamp_width(shown * factor, stored, usable);
    let height = target * aspect;
    if (target - shown).abs() < SAME_PT {
        let limit = if factor > 1.0 { "largest" } else { "smallest" };
        app.set_status_left(
            format!(
                "{} (the {limit} size it can take)",
                picture_status(shown, shown * aspect)
            )
            .into(),
        );
        return true;
    }
    let mut next = state.current.borrow().clone();
    if next.scale_image_block(id, target / stored.0) {
        if !apply_with_history(app, state, next, HistoryKind::DocumentAction) {
            return true;
        }
    }
    app.set_status_left(picture_status(target, height).into());
    true
}
