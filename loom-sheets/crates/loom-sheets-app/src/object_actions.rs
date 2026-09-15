//! Direct manipulation of worksheet objects.

use std::rc::Rc;
use std::sync::Arc;

use loom_desktop::NativeMenuBar;
use loom_sheets_core::style::FillColor;
use loom_sheets_core::{CellRef, Sheet, SheetObject};
use slint::ComponentHandle;

use crate::{
    apply_sheet, project_current_without_reveal, push_history, sync_menu_state, GuiState,
    SheetTransaction, SheetsApp,
};

const MIN_OBJECT_WIDTH: u32 = 80;
const MIN_OBJECT_HEIGHT: u32 = 48;
const MAX_OBJECT_SIZE: u32 = 4096;
const DRAG_DIMENSION_HEADROOM: u32 = 1024;

/// Add a deterministic object-rich fixture to the starter workbook for the
/// native Linux screenshot and visual smoke probe.
pub(crate) fn seed_demo_objects(sheet: &mut Sheet) {
    let mut callout = SheetObject::shape(CellRef { row: 2, col: 3 }, "Drag or resize me");
    callout.width = 300;
    callout.height = 120;
    callout.fill = FillColor::Purple;
    sheet.objects.push(callout);

    let mut note = SheetObject::shape(CellRef { row: 8, col: 1 }, "Anchored worksheet object");
    note.width = 260;
    note.height = 88;
    note.fill = FillColor::Green;
    sheet.objects.push(note);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ObjectGestureMode {
    Move,
    Resize,
}

#[derive(Debug, Clone)]
pub(crate) struct ObjectGesture {
    pub(crate) index: usize,
    pub(crate) before: Sheet,
    pub(crate) mode: ObjectGestureMode,
}

fn finite_delta(value: f32) -> f32 {
    if value.is_finite() {
        value
    } else {
        0.0
    }
}

/// Map a rendered pointer delta to the worksheet anchor under the pointer.
/// The sparse custom row/column dimensions are included so a drag crossing a
/// resized band lands on the same cell boundary the grid displays.
pub(crate) fn anchor_after_drag(
    sheet: &Sheet,
    start: CellRef,
    delta_x: f32,
    delta_y: f32,
    zoom: f32,
    viewport_width: f32,
) -> CellRef {
    let zoom = if zoom.is_finite() && zoom > 0.0 {
        zoom.clamp(0.5, 3.0)
    } else {
        1.0
    };
    let default_col_width = crate::grid_default_col_width(sheet, viewport_width) * zoom;
    let default_row_height = crate::GRID_ROW_HEIGHT * zoom;
    let scaled_cols: std::collections::BTreeMap<u32, f32> = sheet
        .col_widths
        .iter()
        .map(|(&col, &width)| (col, width * zoom))
        .collect();
    let scaled_rows: std::collections::BTreeMap<u32, f32> = sheet
        .row_heights
        .iter()
        .map(|(&row, &height)| (row, height * zoom))
        .collect();
    let dimensions = sheet.dimensions();
    let col_count = dimensions
        .cols
        .max(start.col.saturating_add(DRAG_DIMENSION_HEADROOM));
    let row_count = dimensions
        .rows
        .max(start.row.saturating_add(DRAG_DIMENSION_HEADROOM));
    let start_x = crate::dimension_offset(start.col, default_col_width, &scaled_cols);
    let start_y = crate::dimension_offset(start.row, default_row_height, &scaled_rows);
    let target_x = (start_x + finite_delta(delta_x)).max(0.0);
    let target_y = (start_y + finite_delta(delta_y)).max(0.0);
    CellRef {
        col: crate::dimension_index_at_offset(target_x, col_count, default_col_width, &scaled_cols),
        row: crate::dimension_index_at_offset(
            target_y,
            row_count,
            default_row_height,
            &scaled_rows,
        ),
    }
}

/// Apply a live object move. `start` is the anchor captured at pointer-down;
/// Slint supplies cumulative pointer deltas for each move event.
pub(crate) fn move_object(
    sheet: &mut Sheet,
    index: usize,
    start: CellRef,
    delta_x: f32,
    delta_y: f32,
    zoom: f32,
    viewport_width: f32,
) -> bool {
    let anchor = anchor_after_drag(sheet, start, delta_x, delta_y, zoom, viewport_width);
    let Some(object) = sheet.objects.get_mut(index) else {
        return false;
    };
    if object.anchor == anchor {
        return false;
    }
    object.anchor = anchor;
    true
}

/// Calculate persisted logical-pixel dimensions from a rendered pointer
/// delta, respecting zoom and the same visible minimum used by the handles.
pub(crate) fn resized_dimensions(
    object: &SheetObject,
    delta_x: f32,
    delta_y: f32,
    zoom: f32,
) -> (u32, u32) {
    let zoom = if zoom.is_finite() && zoom > 0.0 {
        zoom.clamp(0.5, 3.0)
    } else {
        1.0
    };
    let width = (object.width as f32 + finite_delta(delta_x) / zoom)
        .round()
        .clamp(MIN_OBJECT_WIDTH as f32, MAX_OBJECT_SIZE as f32) as u32;
    let height = (object.height as f32 + finite_delta(delta_y) / zoom)
        .round()
        .clamp(MIN_OBJECT_HEIGHT as f32, MAX_OBJECT_SIZE as f32) as u32;
    (width, height)
}

/// Apply a live resize to one object and report whether its dimensions moved.
pub(crate) fn resize_object(
    sheet: &mut Sheet,
    index: usize,
    before: &Sheet,
    delta_x: f32,
    delta_y: f32,
    zoom: f32,
) -> bool {
    let Some(original) = before.objects.get(index) else {
        return false;
    };
    let dimensions = resized_dimensions(original, delta_x, delta_y, zoom);
    let Some(object) = sheet.objects.get_mut(index) else {
        return false;
    };
    if (object.width, object.height) == dimensions {
        return false;
    }
    (object.width, object.height) = dimensions;
    true
}

fn begin_gesture(app: &SheetsApp, state: &Rc<GuiState>, index: i32, mode: ObjectGestureMode) {
    let Ok(index) = usize::try_from(index) else {
        return;
    };
    let before = state.current.borrow().clone();
    if index >= before.objects.len() {
        return;
    }
    app.set_selected_object(index as i32);
    *state.object_gesture.borrow_mut() = Some(ObjectGesture {
        index,
        before,
        mode,
    });
    project_current_without_reveal(app, state);
}

fn update_gesture(
    app: &SheetsApp,
    state: &Rc<GuiState>,
    index: i32,
    delta_x: f32,
    delta_y: f32,
    mode: ObjectGestureMode,
) {
    let Ok(index) = usize::try_from(index) else {
        return;
    };
    let gesture = state.object_gesture.borrow().clone();
    let Some(gesture) = gesture else { return };
    if gesture.index != index || gesture.mode != mode {
        return;
    }
    let changed = {
        let mut current = state.current.borrow_mut();
        match mode {
            ObjectGestureMode::Move => move_object(
                &mut current,
                index,
                gesture.before.objects[index].anchor,
                delta_x,
                delta_y,
                crate::zoom_factor(app),
                app.get_grid_viewport_width(),
            ),
            ObjectGestureMode::Resize => resize_object(
                &mut current,
                index,
                &gesture.before,
                delta_x,
                delta_y,
                crate::zoom_factor(app),
            ),
        }
    };
    if changed {
        project_current_without_reveal(app, state);
    }
}

fn finish_gesture(
    app: &SheetsApp,
    state: &Rc<GuiState>,
    menu_service: &Arc<NativeMenuBar>,
    index: i32,
    mode: ObjectGestureMode,
    cancelled: bool,
) {
    let Ok(index) = usize::try_from(index) else {
        return;
    };
    let gesture = state.object_gesture.borrow_mut().take();
    let Some(gesture) = gesture else { return };
    if gesture.index != index || gesture.mode != mode {
        *state.object_gesture.borrow_mut() = Some(gesture);
        return;
    }
    if cancelled {
        *state.current.borrow_mut() = gesture.before;
        apply_sheet(app, state);
        return;
    }
    let after = state.current.borrow().clone();
    if after.objects == gesture.before.objects {
        return;
    }
    push_history(
        &mut state.undo_stack.borrow_mut(),
        SheetTransaction::Snapshot {
            before: Box::new(gesture.before),
            after: Box::new(after),
        },
    );
    state.redo_stack.borrow_mut().clear();
    sync_menu_state(menu_service, app, state);
}

/// Connect object selection, move, and resize gestures to the undoable sheet
/// model. Live pointer motion is previewed without creating history entries;
/// the completed gesture becomes one snapshot transaction.
pub(crate) fn register_object_actions(
    app: &SheetsApp,
    state: &Rc<GuiState>,
    menu_service: &Arc<NativeMenuBar>,
) {
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_object_move_started(move |index| {
            if let Some(app) = app_ref.upgrade() {
                begin_gesture(&app, &state, index, ObjectGestureMode::Move);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_object_moved(move |index, delta_x, delta_y| {
            if let Some(app) = app_ref.upgrade() {
                update_gesture(
                    &app,
                    &state,
                    index,
                    delta_x,
                    delta_y,
                    ObjectGestureMode::Move,
                );
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_object_move_ended(move |index| {
            if let Some(app) = app_ref.upgrade() {
                finish_gesture(
                    &app,
                    &state,
                    &menu_service,
                    index,
                    ObjectGestureMode::Move,
                    false,
                );
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_object_move_cancelled(move |index| {
            if let Some(app) = app_ref.upgrade() {
                finish_gesture(
                    &app,
                    &state,
                    &menu_service,
                    index,
                    ObjectGestureMode::Move,
                    true,
                );
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_object_resize_started(move |index| {
            if let Some(app) = app_ref.upgrade() {
                begin_gesture(&app, &state, index, ObjectGestureMode::Resize);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_object_resized(move |index, delta_x, delta_y| {
            if let Some(app) = app_ref.upgrade() {
                update_gesture(
                    &app,
                    &state,
                    index,
                    delta_x,
                    delta_y,
                    ObjectGestureMode::Resize,
                );
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_object_resize_ended(move |index| {
            if let Some(app) = app_ref.upgrade() {
                finish_gesture(
                    &app,
                    &state,
                    &menu_service,
                    index,
                    ObjectGestureMode::Resize,
                    false,
                );
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_object_resize_cancelled(move |index| {
            if let Some(app) = app_ref.upgrade() {
                finish_gesture(
                    &app,
                    &state,
                    &menu_service,
                    index,
                    ObjectGestureMode::Resize,
                    true,
                );
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use super::{anchor_after_drag, resized_dimensions};

    #[test]
    fn object_drag_uses_custom_dimension_geometry_and_clamps() {
        let mut sheet = Sheet::new("Objects");
        sheet.set_str("H20", "extent");
        sheet.set_col_width(2, 120.0);
        sheet.set_row_height(1, 40.0);
        let start = CellRef { row: 1, col: 2 };

        let moved = anchor_after_drag(&sheet, start, 120.0, -100.0, 1.0, 640.0);

        assert_eq!(moved, CellRef { row: 0, col: 3 });
    }

    #[test]
    fn object_resize_scales_pointer_delta_and_enforces_minimums() {
        let object = loom_sheets_core::SheetObject::shape(CellRef { row: 0, col: 0 }, "Resizable");

        assert_eq!(resized_dimensions(&object, 100.0, -200.0, 2.0), (290, 48));
    }
}
