//! Gestures on the grid beyond a plain click: Ctrl+Arrow jumps, the select-all
//! corner, header-edge double-clicks (autofit), and scrolling while a drag
//! selection is held past the grid's edge.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use loom_desktop::NativeMenuBar;
use loom_sheets_core::{CellRef, Sheet, Value};
use slint::{ComponentHandle, Model, SharedString};

use crate::grid_pointer::{commit_resize, index_at, COL_WIDTH_RANGE};
use crate::{
    evaluate, project_current, project_current_without_reveal, selection_from_app,
    update_selection_range, GridPointer, GridSelection, GuiState, SheetsApp,
    GRID_COLUMN_HEADER_HEIGHT, GRID_ROW_HEADER_WIDTH,
};

/// Average advance of a character in the grid's face, as a fraction of its
/// font size. An estimate: the grid does not measure text.
const GLYPH_EM: f32 = 0.6;
/// Bold text runs a little wider.
const BOLD_FACTOR: f32 = 1.1;
/// Room left around the widest value (cell padding on both sides).
const FIT_PADDING: f32 = 16.0;
/// Pixels a scroll tick moves when the pointer is just outside, and per pixel
/// of overshoot, capped.
const SCROLL_BASE: f32 = 8.0;
const SCROLL_PER_PIXEL: f32 = 0.4;
const SCROLL_MAX: f32 = 48.0;
const SCROLL_TICK: Duration = Duration::from_millis(40);

/// The width that fits the widest displayed value in column `col`, in sheet
/// pixels (before zoom). An empty column goes back to the default width.
pub(crate) fn fit_width(sheet: &Sheet, values: &HashMap<CellRef, Value>, col: u32) -> f32 {
    let mut widest = 0.0_f32;
    for cell in sheet.cells.keys().filter(|cell| cell.col == col) {
        let style = sheet.cell_style(*cell);
        let text = style.format_value(&crate::cell_value(sheet, values, cell.row, cell.col));
        if text.is_empty() {
            continue;
        }
        let size = crate::formatting::effective_font_size(style) as f32;
        let bold = if style.bold { BOLD_FACTOR } else { 1.0 };
        widest = widest.max(text.chars().count() as f32 * size * GLYPH_EM * bold);
    }
    if widest == 0.0 {
        return loom_sheets_core::DEFAULT_COL_WIDTH;
    }
    (widest + FIT_PADDING).clamp(COL_WIDTH_RANGE.0, COL_WIDTH_RANGE.1)
}

fn autofit(app: &SheetsApp, state: &GuiState, menu: &NativeMenuBar, is_row: bool, index: i32) {
    if index < 0 {
        return;
    }
    let index = index as u32;
    let before = state.current.borrow().clone();
    let mut after = before.clone();
    let status = if is_row {
        after.row_heights.remove(&index);
        format!("Row {} height reset to default", index + 1)
    } else {
        let width = fit_width(&before, &evaluate(&before), index);
        after.set_col_width(index, width);
        format!("Column width fitted: {width:.0} px")
    };
    if after.col_widths == before.col_widths && after.row_heights == before.row_heights {
        app.set_status_left(SharedString::from(status));
        return;
    }
    commit_resize(app, state, menu, before, after, &status);
}

fn jump(app: &SheetsApp, state: &GuiState, directions: (i32, i32), shift: bool) {
    {
        let sheet = state.current.borrow();
        let selection = selection_from_app(app);
        let target = crate::grid_navigation::data_edge(&sheet, selection.focus, directions);
        let next = if shift {
            selection.extend(target)
        } else {
            selection.collapse(target)
        };
        update_selection_range(app, &sheet, &evaluate(&sheet), next);
    }
    project_current(app, state);
}

#[derive(Default)]
struct Drag {
    /// Where the pressed selection started; the grid's own element for that
    /// cell is recycled while the grid scrolls, so Rust keeps the anchor.
    anchor: Option<CellRef>,
    /// The pointer relative to the cell area's top-left corner.
    pointer: Option<(f32, f32)>,
}

thread_local! {
    static DRAG: RefCell<Drag> = RefCell::new(Drag::default());
    static TIMER: slint::Timer = slint::Timer::default();
}

pub(crate) fn begin_drag(anchor: CellRef) {
    DRAG.with(|drag| {
        *drag.borrow_mut() = Drag {
            anchor: Some(anchor),
            pointer: None,
        }
    });
}

pub(crate) fn drag_anchor() -> Option<CellRef> {
    DRAG.with(|drag| drag.borrow().anchor)
}

/// The mouse button was released: stop scrolling and forget the drag.
pub(crate) fn end_drag() {
    DRAG.with(|drag| *drag.borrow_mut() = Drag::default());
    TIMER.with(slint::Timer::stop);
}

/// Pixels a scroll tick moves along one axis for a pointer `position` pixels
/// from the start of an area `extent` pixels long; zero while it is inside.
fn axis_step(position: f32, extent: f32) -> f32 {
    let overshoot = if position < 0.0 {
        -position
    } else if position > extent {
        position - extent
    } else {
        return 0.0;
    };
    let speed = (SCROLL_BASE + overshoot * SCROLL_PER_PIXEL).min(SCROLL_MAX);
    if position < 0.0 {
        -speed
    } else {
        speed
    }
}

/// How far the grid scrolls on each axis for a pointer at `pointer`, in a cell
/// area of `body` pixels. Positive scrolls toward the end of the sheet.
pub(crate) fn scroll_step(pointer: (f32, f32), body: (f32, f32)) -> (f32, f32) {
    (axis_step(pointer.0, body.0), axis_step(pointer.1, body.1))
}

/// The cell area's size: the grid viewport less its pinned headers.
fn body_size(app: &SheetsApp) -> (f32, f32) {
    (
        (app.get_grid_viewport_width() - GRID_ROW_HEADER_WIDTH).max(1.0),
        (app.get_grid_viewport_height() - GRID_COLUMN_HEADER_HEIGHT).max(1.0),
    )
}

/// Called after every drag move with the pointer's position relative to the
/// cell element it started on: scrolls on a timer while it stays outside.
pub(crate) fn after_drag(
    app_ref: &slint::Weak<SheetsApp>,
    state: &Rc<GuiState>,
    origin: (i32, i32),
    local: (f32, f32),
) {
    let Some(app) = app_ref.upgrade() else {
        return;
    };
    let widths: Vec<f32> = app.get_grid_column_widths().iter().collect();
    let heights: Vec<f32> = app.get_grid_row_heights().iter().collect();
    let col = (origin.1 - app.get_view_col_origin()).max(0) as usize;
    let row = (origin.0 - app.get_view_row_origin()).max(0) as usize;
    let along_x = widths.iter().take(col).sum::<f32>() + local.0;
    let along_y = heights.iter().take(row).sum::<f32>() + local.1;
    let pointer = (
        app.get_grid_col_offset() + along_x + app.get_grid_scroll_x(),
        app.get_grid_row_offset() + along_y + app.get_grid_scroll_y(),
    );
    let outside = scroll_step(pointer, body_size(&app)) != (0.0, 0.0);
    let active = DRAG.with(|drag| {
        let mut drag = drag.borrow_mut();
        if drag.anchor.is_some() {
            drag.pointer = Some(pointer);
        }
        drag.anchor.is_some()
    });
    if !(outside && active) {
        TIMER.with(slint::Timer::stop);
        return;
    }
    TIMER.with(|timer| {
        if !timer.running() {
            let (app_ref, state) = (app_ref.clone(), state.clone());
            timer.start(slint::TimerMode::Repeated, SCROLL_TICK, move || {
                if let Some(app) = app_ref.upgrade() {
                    tick(&app, &state);
                }
            });
        }
    });
}

/// One step of drag auto-scroll: move the grid toward the pointer and extend
/// the selection to the cell now under the (clamped) pointer.
pub(crate) fn tick(app: &SheetsApp, state: &GuiState) {
    let (anchor, pointer) = DRAG.with(|drag| {
        let drag = drag.borrow();
        (drag.anchor, drag.pointer)
    });
    let (Some(anchor), Some(pointer)) = (anchor, pointer) else {
        TIMER.with(slint::Timer::stop);
        return;
    };
    let body = body_size(app);
    let (dx, dy) = scroll_step(pointer, body);
    if (dx, dy) == (0.0, 0.0) {
        TIMER.with(slint::Timer::stop);
        return;
    }
    let max_x = (app.get_grid_content_width() - body.0).max(0.0);
    let max_y = (app.get_grid_content_height() - body.1).max(0.0);
    app.set_grid_scroll_x(-((-app.get_grid_scroll_x() + dx).clamp(0.0, max_x)));
    app.set_grid_scroll_y(-((-app.get_grid_scroll_y() + dy).clamp(0.0, max_y)));
    crate::scroll_projection::project_scroll(app, state);

    let widths: Vec<f32> = app.get_grid_column_widths().iter().collect();
    let heights: Vec<f32> = app.get_grid_row_heights().iter().collect();
    let x =
        pointer.0.clamp(0.0, body.0 - 1.0) - app.get_grid_scroll_x() - app.get_grid_col_offset();
    let y =
        pointer.1.clamp(0.0, body.1 - 1.0) - app.get_grid_scroll_y() - app.get_grid_row_offset();
    let target = CellRef {
        row: (app.get_view_row_origin() as usize + index_at(&heights, 0, y)) as u32,
        col: (app.get_view_col_origin() as usize + index_at(&widths, 0, x)) as u32,
    };
    let selection = GridSelection::new(anchor, target);
    if selection != selection_from_app(app) {
        {
            let sheet = state.current.borrow();
            update_selection_range(app, &sheet, &evaluate(&sheet), selection);
        }
        project_current_without_reveal(app, state);
    }
}

pub(crate) fn wire(app: &SheetsApp, state: &Rc<GuiState>, menu_service: &Arc<NativeMenuBar>) {
    let pointer = app.global::<GridPointer>();
    {
        let (state, app_ref) = (state.clone(), app.as_weak());
        pointer.on_jump(move |row_dir, col_dir, shift| {
            if let Some(app) = app_ref.upgrade() {
                jump(&app, &state, (row_dir, col_dir), shift);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        pointer.on_corner_pressed(move || {
            if let Some(app) = app_ref.upgrade() {
                app.invoke_select_all();
            }
        });
    }
    {
        let (state, app_ref, menu) = (state.clone(), app.as_weak(), menu_service.clone());
        pointer.on_header_autofit(move |is_row, index| {
            if let Some(app) = app_ref.upgrade() {
                if crate::mutation_guard::refused(&app, &state) {
                    return;
                }
                autofit(&app, &state, &menu, is_row, index);
            }
        });
    }
    pointer.on_drag_ended(end_drag);
}
