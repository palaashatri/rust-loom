//! Direct manipulation of worksheet objects.

use std::rc::Rc;
use std::sync::Arc;

use loom_desktop::NativeMenuBar;
use loom_sheets_core::style::FillColor;
use loom_sheets_core::{CellRef, Sheet, SheetObject};
use slint::ComponentHandle;

use crate::{
    apply_sheet, project_current, project_current_without_reveal, push_history, sync_menu_state,
    GuiState, SheetTransaction, SheetsApp,
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
    pub(crate) before_anchor: CellRef,
    pub(crate) before_width: u32,
    pub(crate) before_height: u32,
    pub(crate) preview_anchor: CellRef,
    pub(crate) preview_width: u32,
    pub(crate) preview_height: u32,
    pub(crate) mode: ObjectGestureMode,
    pub(crate) sheet_index: usize,
    pub(crate) document_generation: u64,
    pub(crate) context_generation: u64,
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

fn resized_dimensions_from_size(
    width: u32,
    height: u32,
    delta_x: f32,
    delta_y: f32,
    zoom: f32,
) -> (u32, u32) {
    let zoom = if zoom.is_finite() && zoom > 0.0 {
        zoom.clamp(0.5, 3.0)
    } else {
        1.0
    };
    let width = (width as f32 + finite_delta(delta_x) / zoom)
        .round()
        .clamp(MIN_OBJECT_WIDTH as f32, MAX_OBJECT_SIZE as f32) as u32;
    let height = (height as f32 + finite_delta(delta_y) / zoom)
        .round()
        .clamp(MIN_OBJECT_HEIGHT as f32, MAX_OBJECT_SIZE as f32) as u32;
    (width, height)
}

fn keyboard_step_object(
    anchor: &mut CellRef,
    width: &mut u32,
    height: &mut u32,
    mode: ObjectGestureMode,
    delta_col: i32,
    delta_row: i32,
) -> bool {
    match mode {
        ObjectGestureMode::Move => {
            let col = anchor.col.saturating_add_signed(delta_col);
            let row = anchor.row.saturating_add_signed(delta_row);
            if (anchor.col, anchor.row) == (col, row) {
                return false;
            }
            anchor.col = col;
            anchor.row = row;
        }
        ObjectGestureMode::Resize => {
            let new_width = (*width as i64 + i64::from(delta_col) * 10)
                .clamp(MIN_OBJECT_WIDTH as i64, MAX_OBJECT_SIZE as i64)
                as u32;
            let new_height = (*height as i64 + i64::from(delta_row) * 10)
                .clamp(MIN_OBJECT_HEIGHT as i64, MAX_OBJECT_SIZE as i64)
                as u32;
            if (*width, *height) == (new_width, new_height) {
                return false;
            }
            *width = new_width;
            *height = new_height;
        }
    }
    true
}

fn reveal_object(app: &SheetsApp, state: &Rc<GuiState>, index: usize) {
    let sheet = state.current.borrow();
    let Some(object) = sheet.objects.get(index) else {
        return;
    };
    let anchor = preview_geometry(state)
        .filter(|(preview_index, _, _, _)| *preview_index == index)
        .map(|(_, anchor, _, _)| anchor)
        .unwrap_or(object.anchor);
    let zoom = crate::zoom_factor(app);
    let columns: std::collections::BTreeMap<u32, f32> = sheet
        .col_widths
        .iter()
        .map(|(&column, &width)| (column, width * zoom))
        .collect();
    let rows: std::collections::BTreeMap<u32, f32> = sheet
        .row_heights
        .iter()
        .map(|(&row, &height)| (row, height * zoom))
        .collect();
    let x = crate::dimension_offset(
        anchor.col,
        crate::grid_default_col_width(&sheet, app.get_grid_viewport_width()) * zoom,
        &columns,
    );
    let y = crate::dimension_offset(anchor.row, crate::GRID_ROW_HEIGHT * zoom, &rows);
    app.set_grid_scroll_x(-x);
    app.set_grid_scroll_y(-y);
    drop(sheet);
    project_current_without_reveal(app, state);
}

fn gesture_context_matches(state: &GuiState, gesture: &ObjectGesture) -> bool {
    *state.active_sheet_index.borrow() == gesture.sheet_index
        && state.open_operations.borrow().document_generation() == gesture.document_generation
        && state.object_context_generation.get() == gesture.context_generation
}

fn apply_gesture_geometry(object: &mut SheetObject, gesture: &ObjectGesture) {
    match gesture.mode {
        ObjectGestureMode::Move => object.anchor = gesture.preview_anchor,
        ObjectGestureMode::Resize => {
            object.width = gesture.preview_width;
            object.height = gesture.preview_height;
        }
    }
}

fn gesture_changed(gesture: &ObjectGesture) -> bool {
    match gesture.mode {
        ObjectGestureMode::Move => gesture.preview_anchor != gesture.before_anchor,
        ObjectGestureMode::Resize => {
            (gesture.preview_width, gesture.preview_height)
                != (gesture.before_width, gesture.before_height)
        }
    }
}

pub(crate) fn preview_geometry(state: &GuiState) -> Option<(usize, CellRef, u32, u32)> {
    let gesture = state.object_gesture.borrow();
    let gesture = gesture.as_ref()?;
    gesture_context_matches(state, gesture).then_some((
        gesture.index,
        gesture.preview_anchor,
        gesture.preview_width,
        gesture.preview_height,
    ))
}

/// What the grid should do with its scroll position when a gesture preview
/// ends. Two positional booleans at the call site were unreadable, and the
/// pairing is what distinguishes a committed move from a cancelled one.
#[derive(Clone, Copy, PartialEq, Eq)]
struct GestureScroll {
    reveal_selection_on_cancel: bool,
    preserve_scroll_after_commit: bool,
}

impl GestureScroll {
    /// Keep the user's current scroll position through the commit.
    const KEEP_ON_COMMIT: Self = Self {
        reveal_selection_on_cancel: false,
        preserve_scroll_after_commit: true,
    };

    /// Put the selected cell back on screen when a preview is cancelled.
    const REVEAL_ON_CANCEL: Self = Self {
        reveal_selection_on_cancel: true,
        preserve_scroll_after_commit: false,
    };

    /// Neither: leave projection to the caller.
    const UNCHANGED: Self = Self {
        reveal_selection_on_cancel: false,
        preserve_scroll_after_commit: false,
    };
}

fn finish_gesture_state(
    app: &SheetsApp,
    state: &Rc<GuiState>,
    menu_service: &Arc<NativeMenuBar>,
    index: usize,
    mode: ObjectGestureMode,
    cancelled: bool,
    scroll: GestureScroll,
) {
    let gesture = state.object_gesture.borrow_mut().take();
    let Some(gesture) = gesture else { return };
    if gesture.index != index || gesture.mode != mode {
        *state.object_gesture.borrow_mut() = Some(gesture);
        return;
    }
    if !gesture_context_matches(state, &gesture) {
        app.set_object_state(0);
        project_current_without_reveal(app, state);
        return;
    }
    if cancelled {
        if scroll.reveal_selection_on_cancel {
            project_current(app, state);
        } else {
            project_current_without_reveal(app, state);
        }
        return;
    }

    if !gesture_changed(&gesture) {
        return;
    }
    let before = state.current.borrow().clone();
    let mut after = before.clone();
    let Some(object) = after.objects.get_mut(index) else {
        return;
    };
    apply_gesture_geometry(object, &gesture);
    *state.current.borrow_mut() = after.clone();
    push_history(
        &mut state.undo_stack.borrow_mut(),
        SheetTransaction::Snapshot {
            before: Box::new(before),
            after: Box::new(after),
        },
    );
    state.redo_stack.borrow_mut().clear();
    let scroll_x = app.get_grid_scroll_x();
    let scroll_y = app.get_grid_scroll_y();
    apply_sheet(app, state);
    if scroll.preserve_scroll_after_commit {
        app.set_grid_scroll_x(scroll_x);
        app.set_grid_scroll_y(scroll_y);
        project_current_without_reveal(app, state);
    }
    sync_menu_state(menu_service, app, state);
}

/// Cancel the current object preview before a context-changing action.
pub(crate) fn cancel_active_gesture(app: &SheetsApp, state: &GuiState) {
    let gesture = state.object_gesture.borrow_mut().take();
    if gesture.is_none() {
        return;
    }
    app.set_object_state(0);
    project_current_without_reveal(app, state);
}

fn handle_keyboard_action(
    app: &SheetsApp,
    state: &Rc<GuiState>,
    index: i32,
    action: i32,
    delta_col: i32,
    delta_row: i32,
    menu_service: &Arc<NativeMenuBar>,
) {
    let Ok(index) = usize::try_from(index) else {
        return;
    };
    match action {
        0 => reveal_object(app, state, index),
        1 | 2 => {
            let mode = if action == 1 {
                ObjectGestureMode::Move
            } else {
                ObjectGestureMode::Resize
            };
            let context_matches = state
                .object_gesture
                .borrow()
                .as_ref()
                .is_some_and(|gesture| gesture_context_matches(state, gesture));
            if !context_matches {
                cancel_active_gesture(app, state);
                return;
            }
            let changed = {
                let mut active = state.object_gesture.borrow_mut();
                let Some(gesture) = active.as_mut() else {
                    return;
                };
                if gesture.index != index || gesture.mode != mode {
                    return;
                }
                keyboard_step_object(
                    &mut gesture.preview_anchor,
                    &mut gesture.preview_width,
                    &mut gesture.preview_height,
                    mode,
                    delta_col,
                    delta_row,
                )
            };
            if changed {
                if mode == ObjectGestureMode::Move {
                    reveal_object(app, state, index);
                } else {
                    project_current_without_reveal(app, state);
                }
            }
        }
        -4..=-1 => {
            let (mode, cancelled) = match action {
                -1 => (ObjectGestureMode::Move, false),
                -2 => (ObjectGestureMode::Resize, false),
                -3 => (ObjectGestureMode::Move, true),
                -4 => (ObjectGestureMode::Resize, true),
                _ => unreachable!(),
            };
            finish_gesture_state(
                app,
                state,
                menu_service,
                index,
                mode,
                cancelled,
                GestureScroll::KEEP_ON_COMMIT,
            );
        }
        _ => {}
    }
}

fn begin_gesture(app: &SheetsApp, state: &Rc<GuiState>, index: i32, mode: ObjectGestureMode) {
    let Ok(index) = usize::try_from(index) else {
        return;
    };
    if state.object_gesture.borrow().is_some() {
        cancel_active_gesture(app, state);
    }
    let (before_anchor, before_width, before_height) = {
        let current = state.current.borrow();
        let Some(object) = current.objects.get(index) else {
            return;
        };
        (object.anchor, object.width, object.height)
    };
    app.set_selected_object(index as i32);
    *state.object_gesture.borrow_mut() = Some(ObjectGesture {
        index,
        before_anchor,
        before_width,
        before_height,
        preview_anchor: before_anchor,
        preview_width: before_width,
        preview_height: before_height,
        mode,
        sheet_index: *state.active_sheet_index.borrow(),
        document_generation: state.open_operations.borrow().document_generation(),
        context_generation: state.object_context_generation.get(),
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
    let context_matches = state
        .object_gesture
        .borrow()
        .as_ref()
        .is_some_and(|gesture| gesture_context_matches(state, gesture));
    if !context_matches {
        cancel_active_gesture(app, state);
        return;
    }
    let changed = {
        let current = state.current.borrow();
        let mut active = state.object_gesture.borrow_mut();
        let Some(gesture) = active.as_mut() else {
            return;
        };
        if gesture.index != index || gesture.mode != mode {
            return;
        }
        match mode {
            ObjectGestureMode::Move => {
                let anchor = anchor_after_drag(
                    &current,
                    gesture.before_anchor,
                    delta_x,
                    delta_y,
                    crate::zoom_factor(app),
                    app.get_grid_viewport_width(),
                );
                if gesture.preview_anchor == anchor {
                    false
                } else {
                    gesture.preview_anchor = anchor;
                    true
                }
            }
            ObjectGestureMode::Resize => {
                let dimensions = resized_dimensions_from_size(
                    gesture.before_width,
                    gesture.before_height,
                    delta_x,
                    delta_y,
                    crate::zoom_factor(app),
                );
                if (gesture.preview_width, gesture.preview_height) == dimensions {
                    false
                } else {
                    (gesture.preview_width, gesture.preview_height) = dimensions;
                    true
                }
            }
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
    finish_gesture_state(
        app,
        state,
        menu_service,
        index,
        mode,
        cancelled,
        if cancelled {
            GestureScroll::REVEAL_ON_CANCEL
        } else {
            GestureScroll::UNCHANGED
        },
    );
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
        let menu_service = menu_service.clone();
        app.on_object_keyboard_action(move |index, action, delta_col, delta_row| {
            if let Some(app) = app_ref.upgrade() {
                handle_keyboard_action(
                    &app,
                    &state,
                    index,
                    action,
                    delta_col,
                    delta_row,
                    &menu_service,
                );
            }
        });
    }
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
    use super::{
        anchor_after_drag, keyboard_step_object, resized_dimensions_from_size, ObjectGestureMode,
    };
    use loom_sheets_core::SheetObject;

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
        assert_eq!(
            resized_dimensions_from_size(240, 100, 100.0, -200.0, 2.0),
            (290, 48)
        );
    }

    #[test]
    fn keyboard_object_steps_move_by_cells_and_resize_in_document_pixels() {
        let mut object = SheetObject::shape(CellRef { row: 2, col: 3 }, "Callout");

        assert!(keyboard_step_object(
            &mut object.anchor,
            &mut object.width,
            &mut object.height,
            ObjectGestureMode::Move,
            1,
            -1
        ));
        assert_eq!(object.anchor, CellRef { row: 1, col: 4 });
        assert!(keyboard_step_object(
            &mut object.anchor,
            &mut object.width,
            &mut object.height,
            ObjectGestureMode::Resize,
            1,
            -1
        ));
        assert_eq!((object.width, object.height), (250, 102));
        assert!(keyboard_step_object(
            &mut object.anchor,
            &mut object.width,
            &mut object.height,
            ObjectGestureMode::Move,
            -1,
            1
        ));
        assert_eq!(object.anchor, CellRef { row: 2, col: 3 });
    }
}
