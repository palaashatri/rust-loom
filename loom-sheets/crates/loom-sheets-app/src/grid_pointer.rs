//! Mouse selection, header selection and resizing, and the name box.
//!
//! The grid reports raw gestures through the `GridPointer` global; this module
//! turns them into selections and undoable size changes.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use loom_desktop::NativeMenuBar;
use loom_sheets_core::{CellRange, CellRef, Sheet};
use slint::{ComponentHandle, Model, SharedString};

use crate::{
    apply_sheet, commit_transaction, evaluate, project_current, project_current_without_reveal,
    selection_from_app, sync_menu_state, update_selection_range, zoom_factor, GridPointer,
    GridSelection, GuiState, SheetTransaction, SheetsApp, DEFAULT_VISIBLE_COLS,
    DEFAULT_VISIBLE_ROWS,
};

pub(crate) const COL_WIDTH_RANGE: (f32, f32) = (24.0, 600.0);
const ROW_HEIGHT_RANGE: (f32, f32) = (12.0, 400.0);

/// A header edge being dragged: the sheet as it was and the size it started at.
struct Resize {
    is_row: bool,
    index: u32,
    before: Sheet,
    start: f32,
}

#[derive(Default)]
struct Gesture {
    /// The release that ends a drag or shift-press must not also count as a
    /// plain click, which would collapse the selection again.
    swallow_click: bool,
    resize: Option<Resize>,
}

thread_local! {
    static GESTURE: RefCell<Gesture> = RefCell::new(Gesture::default());
}

/// Whether the click that follows the last press should be ignored. Reading
/// it clears it.
pub(crate) fn take_swallowed_click() -> bool {
    GESTURE.with(|gesture| std::mem::take(&mut gesture.borrow_mut().swallow_click))
}

/// The index of the cell under a point `local` pixels from the start of cell
/// `origin`, in a row of cells with the given sizes. Points past either end
/// land on the first or last cell.
pub(crate) fn index_at(sizes: &[f32], origin: usize, local: f32) -> usize {
    if sizes.is_empty() {
        return 0;
    }
    let origin = origin.min(sizes.len() - 1);
    let mut position = sizes[..origin].iter().sum::<f32>() + local;
    for (index, size) in sizes.iter().enumerate() {
        if position < *size {
            return index;
        }
        position -= size;
    }
    sizes.len() - 1
}

fn visible_sizes(model: slint::ModelRc<f32>) -> Vec<f32> {
    model.iter().collect()
}

/// The selection for a click on a column or row header: the whole column or
/// row up to the sheet's used extent, with Shift extending from the anchor.
pub(crate) fn header_selection(
    sheet: &Sheet,
    current: GridSelection,
    is_row: bool,
    index: u32,
    shift: bool,
) -> GridSelection {
    let used = sheet.dimensions();
    let last_row = used.rows.max(DEFAULT_VISIBLE_ROWS).saturating_sub(1);
    let last_col = used.cols.max(DEFAULT_VISIBLE_COLS).saturating_sub(1);
    if is_row {
        let from = if shift { current.anchor.row } else { index };
        GridSelection::new(
            CellRef {
                row: from,
                col: last_col,
            },
            CellRef { row: index, col: 0 },
        )
    } else {
        let from = if shift { current.anchor.col } else { index };
        GridSelection::new(
            CellRef {
                row: last_row,
                col: from,
            },
            CellRef { row: 0, col: index },
        )
    }
}

fn cell_pressed(app: &SheetsApp, state: &GuiState, row: i32, col: i32, shift: bool) {
    GESTURE.with(|gesture| gesture.borrow_mut().swallow_click = shift);
    if row < 0 || col < 0 {
        return;
    }
    let pressed = CellRef {
        row: row as u32,
        col: col as u32,
    };
    // A shift-press extends the existing selection, so its drag keeps that
    // anchor; a plain press starts a new one here.
    crate::grid_gestures::begin_drag(if shift {
        selection_from_app(app).anchor
    } else {
        pressed
    });
    if !shift {
        return;
    }
    let target = CellRef {
        row: row as u32,
        col: col as u32,
    };
    let selection = selection_from_app(app).extend(target);
    {
        let sheet = state.current.borrow();
        update_selection_range(app, &sheet, &evaluate(&sheet), selection);
    }
    project_current(app, state);
}

fn cell_dragged(app: &SheetsApp, state: &GuiState, origin: (i32, i32), local: (f32, f32)) {
    if origin.0 < 0 || origin.1 < 0 {
        return;
    }
    let (row_origin, col_origin) = (app.get_view_row_origin(), app.get_view_col_origin());
    let widths = visible_sizes(app.get_grid_column_widths());
    let heights = visible_sizes(app.get_grid_row_heights());
    let col =
        col_origin + index_at(&widths, (origin.1 - col_origin).max(0) as usize, local.0) as i32;
    let row =
        row_origin + index_at(&heights, (origin.0 - row_origin).max(0) as usize, local.1) as i32;
    // The pressed cell, remembered in Rust: the grid recycles its elements
    // while it scrolls, so the cell that reports the drag may be another one.
    let start = crate::grid_gestures::drag_anchor().unwrap_or(CellRef {
        row: origin.0 as u32,
        col: origin.1 as u32,
    });
    let target = CellRef {
        row: row as u32,
        col: col as u32,
    };
    let selection = GridSelection::new(start, target);
    if selection == selection_from_app(app) {
        return;
    }
    GESTURE.with(|gesture| gesture.borrow_mut().swallow_click = true);
    {
        let sheet = state.current.borrow();
        update_selection_range(app, &sheet, &evaluate(&sheet), selection);
    }
    project_current_without_reveal(app, state);
}

fn header_pressed(app: &SheetsApp, state: &GuiState, is_row: bool, index: i32, shift: bool) {
    if index < 0 {
        return;
    }
    let selection = {
        let sheet = state.current.borrow();
        let selection =
            header_selection(&sheet, selection_from_app(app), is_row, index as u32, shift);
        update_selection_range(app, &sheet, &evaluate(&sheet), selection);
        selection
    };
    project_current_without_reveal(app, state);
    app.set_status_left(SharedString::from(format!(
        "Selected {}",
        selection.label()
    )));
}

fn header_resized(
    app: &SheetsApp,
    state: &GuiState,
    menu_service: &NativeMenuBar,
    (is_row, index): (bool, i32),
    travel: f32,
    finished: bool,
) {
    if index < 0 {
        return;
    }
    let index = index as u32;
    let zoom = zoom_factor(app).max(0.1);
    let (min, max) = if is_row {
        ROW_HEIGHT_RANGE
    } else {
        COL_WIDTH_RANGE
    };
    let (before, start) = GESTURE.with(|gesture| {
        let mut gesture = gesture.borrow_mut();
        let continuing = matches!(
            &gesture.resize,
            Some(r) if r.is_row == is_row && r.index == index
        );
        if !continuing {
            let before = state.current.borrow().clone();
            let start = if is_row {
                before.row_height(index)
            } else {
                before.col_width(index)
            };
            gesture.resize = Some(Resize {
                is_row,
                index,
                before,
                start,
            });
        }
        let r = gesture.resize.as_ref().expect("set above");
        (r.before.clone(), r.start)
    });
    let size = (start + travel / zoom).clamp(min, max);
    let mut after = before.clone();
    if is_row {
        after.set_row_height(index, size);
    } else {
        after.set_col_width(index, size);
    }
    let label = if is_row { "Row height" } else { "Column width" };
    if !finished {
        *state.current.borrow_mut() = after;
        project_current_without_reveal(app, state);
        app.set_status_left(SharedString::from(format!("{label}: {size:.0} px")));
        return;
    }
    GESTURE.with(|gesture| gesture.borrow_mut().resize = None);
    if (size - start).abs() < 0.5 {
        *state.current.borrow_mut() = before;
        project_current_without_reveal(app, state);
        return;
    }
    commit_resize(
        app,
        state,
        menu_service,
        before,
        after,
        &format!("{label}: {size:.0} px"),
    );
}

/// Record a finished size change as one undoable step.
pub(crate) fn commit_resize(
    app: &SheetsApp,
    state: &GuiState,
    menu_service: &NativeMenuBar,
    before: Sheet,
    after: Sheet,
    status: &str,
) {
    commit_transaction(
        &mut state.current.borrow_mut(),
        &mut state.undo_stack.borrow_mut(),
        &mut state.redo_stack.borrow_mut(),
        SheetTransaction::Snapshot {
            before: Box::new(before),
            after: Box::new(after),
        },
    );
    apply_sheet(app, state);
    sync_menu_state(menu_service, app, state);
    app.set_status_left(SharedString::from(status));
}

/// Where a reference typed in the name box goes: a cell or a range such as
/// `B2:D9`, in either case.
pub(crate) fn parse_reference(text: &str) -> Option<GridSelection> {
    let range = CellRange::parse(&text.trim().to_ascii_uppercase())?;
    Some(GridSelection::new(range.end, range.start))
}

fn goto_cell(app: &SheetsApp, state: &GuiState, text: &str) {
    let Some(selection) = parse_reference(text) else {
        app.set_status_left(SharedString::from(format!(
            "\"{}\" is not a cell or range such as B12 or A1:C5",
            text.trim()
        )));
        return;
    };
    {
        let sheet = state.current.borrow();
        update_selection_range(app, &sheet, &evaluate(&sheet), selection);
    }
    project_current(app, state);
    app.invoke_focus_grid();
}

pub(crate) fn wire(app: &SheetsApp, state: &Rc<GuiState>, menu_service: &Arc<NativeMenuBar>) {
    crate::grid_gestures::wire(app, state, menu_service);
    let pointer = app.global::<GridPointer>();
    {
        let (state, app_ref) = (state.clone(), app.as_weak());
        pointer.on_cell_pressed(move |row, col, shift| {
            if let Some(app) = app_ref.upgrade() {
                cell_pressed(&app, &state, row, col, shift);
            }
        });
    }
    {
        let (state, app_ref) = (state.clone(), app.as_weak());
        pointer.on_cell_dragged(move |row, col, x, y| {
            if let Some(app) = app_ref.upgrade() {
                cell_dragged(&app, &state, (row, col), (x, y));
                crate::grid_gestures::after_drag(&app_ref, &state, (row, col), (x, y));
            }
        });
    }
    {
        let (state, app_ref) = (state.clone(), app.as_weak());
        pointer.on_header_pressed(move |is_row, index, shift| {
            if let Some(app) = app_ref.upgrade() {
                header_pressed(&app, &state, is_row, index, shift);
            }
        });
    }
    {
        let (state, app_ref, menu_service) = (state.clone(), app.as_weak(), menu_service.clone());
        pointer.on_header_resized(move |is_row, index, travel, finished| {
            if let Some(app) = app_ref.upgrade() {
                header_resized(
                    &app,
                    &state,
                    &menu_service,
                    (is_row, index),
                    travel,
                    finished,
                );
            }
        });
    }
    {
        let (state, app_ref) = (state.clone(), app.as_weak());
        pointer.on_goto_cell(move |text| {
            if let Some(app) = app_ref.upgrade() {
                goto_cell(&app, &state, text.as_str());
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(a1: &str) -> CellRef {
        CellRef::parse(a1).unwrap()
    }

    #[test]
    fn a_point_lands_on_the_cell_it_is_over_with_mixed_sizes() {
        let sizes = [100.0, 50.0, 100.0];
        assert_eq!(index_at(&sizes, 0, 40.0), 0);
        assert_eq!(index_at(&sizes, 0, 120.0), 1);
        assert_eq!(index_at(&sizes, 0, 160.0), 2);
        // Starting in the middle cell, moving back and forward.
        assert_eq!(index_at(&sizes, 1, -10.0), 0);
        assert_eq!(index_at(&sizes, 1, 10.0), 1);
        assert_eq!(index_at(&sizes, 1, 60.0), 2);
    }

    #[test]
    fn points_past_either_end_land_on_the_end_cells() {
        let sizes = [100.0, 50.0];
        assert_eq!(index_at(&sizes, 1, -500.0), 0);
        assert_eq!(index_at(&sizes, 0, 9_999.0), 1);
        assert_eq!(index_at(&[], 3, 5.0), 0);
    }

    #[test]
    fn header_clicks_select_the_whole_used_column_or_row() {
        let mut sheet = Sheet::new("h");
        sheet.set_str("D40", "x");
        let current = GridSelection::new(at("B2"), at("B2"));
        let column = header_selection(&sheet, current, false, 2, false);
        assert_eq!(column.range().to_a1(), "C1:C40");
        let row = header_selection(&sheet, current, true, 4, false);
        assert_eq!(row.range().to_a1(), "A5:H5");
    }

    #[test]
    fn shift_clicking_a_header_extends_from_the_anchor() {
        let sheet = Sheet::new("h");
        let first = GridSelection::new(at("A1"), at("A1"));
        let current = header_selection(&sheet, first, false, 1, false);
        let extended = header_selection(&sheet, current, false, 3, true);
        assert_eq!(extended.range().start.col, 1);
        assert_eq!(extended.range().end.col, 3);
    }

    #[test]
    fn name_box_accepts_cells_and_ranges_in_any_case() {
        let cell = parse_reference(" d50 ").unwrap();
        assert_eq!(cell.focus, at("D50"));
        let range = parse_reference("b2:c3").unwrap();
        assert_eq!(range.range().to_a1(), "B2:C3");
        assert_eq!(
            range.focus,
            at("B2"),
            "the active cell is the range's first cell"
        );
        assert!(parse_reference("hello world").is_none());
        assert!(parse_reference("").is_none());
    }
}
