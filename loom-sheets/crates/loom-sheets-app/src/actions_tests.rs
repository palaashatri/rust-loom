//! Unit tests for Loom Sheets actions and table mutations.

use std::rc::Rc;

use loom_desktop::{FileFilter, ScriptedFileDialogs};
use loom_sheets_core::{
    export_xlsx_from_grid, CellAlignment, CellRange, CellRef, ChartKind, ChartSeries, ChartSpec,
    PivotAggregation, Sheet, SheetModel,
};

use super::*;
use crate::starter_workbook;

fn make_test_state() -> Rc<GuiState> {
    let dialogs = Rc::new(ScriptedFileDialogs::new([], []));
    Rc::new(GuiState::new(
        starter_workbook(),
        None,
        dialogs,
        FileFilter::new("Workbook", ["loomtable"]).unwrap(),
        FileFilter::new("CSV", ["csv"]).unwrap(),
        FileFilter::new("CSV", ["csv"]).unwrap(),
        FileFilter::new("Excel", ["xlsx"]).unwrap(),
    ))
}

#[test]
fn test_multi_sheet_state_creation_and_switching() {
    let state = make_test_state();
    assert_eq!(state.sheets.borrow().len(), 1);
    assert_eq!(*state.active_sheet_index.borrow(), 0);
    assert_eq!(state.current.borrow().name, "Budget");

    // Add a second sheet
    let mut s2 = Sheet::new("Expenses");
    s2.set_str("A1", "Groceries");
    s2.set_str("B1", "250");
    state.sheets.borrow_mut().push(s2.clone());
    assert_eq!(state.sheets.borrow().len(), 2);

    // Switch to sheet 1
    let cur = state.current.borrow().clone();
    let active = *state.active_sheet_index.borrow();
    state.sheets.borrow_mut()[active] = cur;
    *state.active_sheet_index.borrow_mut() = 1;
    *state.current.borrow_mut() = s2;

    assert_eq!(*state.active_sheet_index.borrow(), 1);
    assert_eq!(state.current.borrow().name, "Expenses");
    assert_eq!(
        state.current.borrow().raw(CellRef::parse("A1").unwrap()),
        Some("Groceries")
    );

    // Switch back to sheet 0
    let cur = state.current.borrow().clone();
    let active = *state.active_sheet_index.borrow();
    state.sheets.borrow_mut()[active] = cur;
    *state.active_sheet_index.borrow_mut() = 0;
    *state.current.borrow_mut() = state.sheets.borrow()[0].clone();

    assert_eq!(*state.active_sheet_index.borrow(), 0);
    assert_eq!(state.current.borrow().name, "Budget");
    assert_eq!(
        state.current.borrow().raw(CellRef::parse("A1").unwrap()),
        Some("Item")
    );
}

#[test]
fn test_add_row_and_undo() {
    let state = make_test_state();
    let dims_before = state.current.borrow().dimensions();

    let next_row = dims_before.rows;
    let cell = CellRef {
        row: next_row,
        col: 0,
    };
    let committed = commit_formula_edit(
        &mut state.current.borrow_mut(),
        &mut state.undo_stack.borrow_mut(),
        &mut state.redo_stack.borrow_mut(),
        cell,
        "New Item",
    );
    assert!(committed);
    assert_eq!(state.current.borrow().raw(cell), Some("New Item"));
    assert_eq!(state.undo_stack.borrow().len(), 1);

    // Undo the row addition
    let tx = state.undo_stack.borrow_mut().pop().unwrap();
    tx.revert(&mut state.current.borrow_mut());
    assert_eq!(state.current.borrow().raw(cell), None);
}

#[test]
fn test_organize_sorts_rows() {
    let state = make_test_state();
    let cur = state.current.borrow().clone();
    let dims = cur.dimensions();
    let range = CellRange::new(
        CellRef { row: 1, col: 0 },
        CellRef {
            row: dims.rows.saturating_sub(1),
            col: dims.cols.saturating_sub(1),
        },
    );
    let mut model = SheetModel::new(cur);
    let ok = model.sort_rows(range, 0, true);
    assert!(ok.is_ok());

    let val_a = model.sheet.raw(CellRef { row: 1, col: 0 });
    let val_b = model.sheet.raw(CellRef { row: 2, col: 0 });
    assert!(val_a.is_some());
    assert!(val_b.is_some());
    assert!(val_a.unwrap() <= val_b.unwrap());
}

#[test]
fn test_pivot_table_aggregation() {
    let keys = vec!["Food".to_string(), "Rent".to_string(), "Food".to_string()];
    let vals = vec![50.0, 1000.0, 30.0];
    let pivot = compute_pivot(&keys, &vals, PivotAggregation::Sum).unwrap();
    assert_eq!(pivot.len(), 2);
    let food_entry = pivot.iter().find(|(k, _)| k == "Food").unwrap();
    assert_eq!(food_entry.1, 80.0);
    let rent_entry = pivot.iter().find(|(k, _)| k == "Rent").unwrap();
    assert_eq!(rent_entry.1, 1000.0);
}

#[test]
fn test_chart_spec_validation() {
    let spec = ChartSpec {
        kind: ChartKind::Bar,
        title: "Test Chart".into(),
        series: vec![ChartSeries {
            name: "S1".into(),
            categories: vec!["A".into(), "B".into()],
            values: vec![10.0, 20.0],
        }],
    };
    assert!(spec.validate().is_ok());
}

#[test]
fn test_sheet_renaming() {
    let state = make_test_state();
    assert_eq!(state.current.borrow().name, "Budget");
    state.current.borrow_mut().name = "Q1 Forecast".to_string();
    assert_eq!(state.current.borrow().name, "Q1 Forecast");
}

#[test]
fn test_add_table_col_and_undo() {
    let state = make_test_state();
    let dims_before = state.current.borrow().dimensions();
    let next_col = dims_before.cols;
    let cell = CellRef {
        row: 0,
        col: next_col,
    };
    let col_letter = cell.to_a1().trim_end_matches('1').to_string();
    let committed = commit_formula_edit(
        &mut state.current.borrow_mut(),
        &mut state.undo_stack.borrow_mut(),
        &mut state.redo_stack.borrow_mut(),
        cell,
        &col_letter,
    );
    assert!(committed);
    assert_eq!(state.current.borrow().raw(cell), Some(col_letter.as_str()));
    assert_eq!(state.undo_stack.borrow().len(), 1);

    // Undo the column addition
    let tx = state.undo_stack.borrow_mut().pop().unwrap();
    tx.revert(&mut state.current.borrow_mut());
    assert_eq!(state.current.borrow().raw(cell), None);
}

#[test]
fn test_cell_format_and_undo() {
    let state = make_test_state();
    let cell = CellRef::parse("B2").unwrap();
    let committed = commit_formula_edit(
        &mut state.current.borrow_mut(),
        &mut state.undo_stack.borrow_mut(),
        &mut state.redo_stack.borrow_mut(),
        cell,
        "$1200.00",
    );
    assert!(committed);
    assert_eq!(state.current.borrow().raw(cell), Some("$1200.00"));
    assert_eq!(state.undo_stack.borrow().len(), 1);

    // Undo formatting
    let tx = state.undo_stack.borrow_mut().pop().unwrap();
    tx.revert(&mut state.current.borrow_mut());
    assert_eq!(state.current.borrow().raw(cell), Some("1200"));
}

#[test]
fn test_chart_spec_normalization_pipeline() {
    let spec = ChartSpec {
        kind: ChartKind::Bar,
        title: "Revenue Chart".into(),
        series: vec![ChartSeries {
            name: "Q1".into(),
            categories: vec!["Jan".into(), "Feb".into(), "Mar".into()],
            values: vec![100.0, 200.0, 300.0],
        }],
    };
    let normalized = spec.normalized_points().expect("normalization succeeds");
    assert_eq!(normalized.len(), 1);
    assert_eq!(normalized[0].len(), 3);
    assert!((normalized[0][0] - 0.0).abs() < 1e-4);
    assert!((normalized[0][1] - 0.5).abs() < 1e-4);
    assert!((normalized[0][2] - 1.0).abs() < 1e-4);
}

#[test]
fn test_clear_selection_and_undo() {
    let state = make_test_state();
    let mut sheet = state.current.borrow_mut();
    let mut undo = state.undo_stack.borrow_mut();
    let mut redo = state.redo_stack.borrow_mut();

    let cell = CellRef::parse("A2").unwrap();
    assert_eq!(sheet.raw(cell), Some("Rent"));

    let range = CellRange::new(cell, cell);
    let changed = clear_selection(&mut sheet, &mut undo, &mut redo, range);
    assert!(changed);
    assert_eq!(sheet.raw(cell), None);
    assert_eq!(undo.len(), 1);

    // Undo should restore "Rent"
    let tx = undo.pop().unwrap();
    tx.revert(&mut sheet);
    assert_eq!(sheet.raw(cell), Some("Rent"));

    // Redo should clear again
    tx.apply(&mut sheet);
    assert_eq!(sheet.raw(cell), None);
}

#[test]
fn test_set_selection_alignment_and_undo() {
    let state = make_test_state();
    let mut sheet = state.current.borrow_mut();
    let mut undo = state.undo_stack.borrow_mut();
    let mut redo = state.redo_stack.borrow_mut();

    let r1 = CellRef::parse("A1").unwrap();
    let r2 = CellRef::parse("A2").unwrap();
    let range = CellRange::new(r1, r2);

    assert_eq!(sheet.cell_alignment(r1), CellAlignment::General);
    assert_eq!(sheet.cell_alignment(r2), CellAlignment::General);

    let changed = set_selection_alignment(
        &mut sheet,
        &mut undo,
        &mut redo,
        range,
        CellAlignment::Center,
    );
    assert!(changed);
    assert_eq!(sheet.cell_alignment(r1), CellAlignment::Center);
    assert_eq!(sheet.cell_alignment(r2), CellAlignment::Center);
    assert_eq!(undo.len(), 1);

    // Undo should restore General
    let tx = undo.pop().unwrap();
    tx.revert(&mut sheet);
    assert_eq!(sheet.cell_alignment(r1), CellAlignment::General);
    assert_eq!(sheet.cell_alignment(r2), CellAlignment::General);

    // Redo should set Center again
    tx.apply(&mut sheet);
    assert_eq!(sheet.cell_alignment(r1), CellAlignment::Center);
    assert_eq!(sheet.cell_alignment(r2), CellAlignment::Center);
}

#[test]
fn test_per_sheet_undo_stacks_preserved_across_tab_switches() {
    let state = make_test_state();

    // Sheet 0 edit
    let cell_0 = CellRef::parse("B2").unwrap();
    commit_formula_edit(
        &mut state.current.borrow_mut(),
        &mut state.undo_stack.borrow_mut(),
        &mut state.redo_stack.borrow_mut(),
        cell_0,
        "1300",
    );
    assert_eq!(state.undo_stack.borrow().len(), 1);

    // Add and switch to Sheet 1
    let mut s2 = Sheet::new("Expenses");
    s2.set_str("A1", "Groceries");
    state.sheets.borrow_mut().push(s2.clone());

    let cur = state.current.borrow().clone();
    let undo = std::mem::take(&mut *state.undo_stack.borrow_mut());
    let redo = std::mem::take(&mut *state.redo_stack.borrow_mut());
    state.sheet_histories.borrow_mut().insert(0, (undo, redo));
    state.sheets.borrow_mut()[0] = cur;

    *state.active_sheet_index.borrow_mut() = 1;
    *state.current.borrow_mut() = s2;
    assert_eq!(state.undo_stack.borrow().len(), 0);

    // Edit in Sheet 1
    let cell_1 = CellRef::parse("A2").unwrap();
    commit_formula_edit(
        &mut state.current.borrow_mut(),
        &mut state.undo_stack.borrow_mut(),
        &mut state.redo_stack.borrow_mut(),
        cell_1,
        "Snacks",
    );
    assert_eq!(state.undo_stack.borrow().len(), 1);

    // Switch back to Sheet 0
    let cur1 = state.current.borrow().clone();
    let undo1 = std::mem::take(&mut *state.undo_stack.borrow_mut());
    let redo1 = std::mem::take(&mut *state.redo_stack.borrow_mut());
    if state.sheet_histories.borrow().len() <= 1 {
        state
            .sheet_histories
            .borrow_mut()
            .resize_with(2, || (Vec::new(), Vec::new()));
    }
    state.sheet_histories.borrow_mut()[1] = (undo1, redo1);
    state.sheets.borrow_mut()[1] = cur1;

    *state.active_sheet_index.borrow_mut() = 0;
    *state.current.borrow_mut() = state.sheets.borrow()[0].clone();
    let (u0, r0) = state.sheet_histories.borrow()[0].clone();
    *state.undo_stack.borrow_mut() = u0;
    *state.redo_stack.borrow_mut() = r0;
    assert_eq!(state.undo_stack.borrow().len(), 1);

    // Revert Sheet 0 edit
    let tx = state.undo_stack.borrow_mut().pop().unwrap();
    tx.revert(&mut state.current.borrow_mut());
    assert_eq!(state.current.borrow().raw(cell_0), Some("1200"));
}

#[test]
fn test_export_xlsx_from_sheet_grid() {
    let state = make_test_state();
    let sheet = state.current.borrow();
    let sel = select_all_range(&sheet);
    let matrix = copy_selection(&sheet, sel);
    let bytes = export_xlsx_from_grid(&matrix).expect("xlsx export from grid");
    assert!(bytes.len() > 100);
    assert_eq!(&bytes[0..4], b"PK\x03\x04");
}

#[test]
fn test_copy_selection() {
    let state = make_test_state();
    let sheet = state.current.borrow();
    let sel = GridSelection::new(CellRef { row: 0, col: 0 }, CellRef { row: 1, col: 1 });
    let data = copy_selection(&sheet, sel);
    assert_eq!(data.len(), 2);
    assert_eq!(data[0].len(), 2);
    assert_eq!(data[0][0], "Item");
    assert_eq!(data[0][1], "Amount");
    assert_eq!(data[1][0], "Rent");
    assert_eq!(data[1][1], "1200");
}

#[test]
fn test_paste_single_into_range_with_undo() {
    let state = make_test_state();
    let mut sheet = state.current.borrow_mut();
    let mut undo = state.undo_stack.borrow_mut();
    let mut redo = state.redo_stack.borrow_mut();

    let target_sel = GridSelection::new(CellRef { row: 10, col: 0 }, CellRef { row: 11, col: 1 });
    let data = vec![vec!["$99.00".to_string()]];
    let pasted = paste_selection(&mut sheet, &mut undo, &mut redo, target_sel, &data);
    assert_eq!(pasted, 4);
    assert_eq!(sheet.raw(CellRef { row: 10, col: 0 }), Some("$99.00"));
    assert_eq!(sheet.raw(CellRef { row: 11, col: 1 }), Some("$99.00"));
    assert_eq!(undo.len(), 1);

    // Revert undo
    undo.pop().unwrap().revert(&mut sheet);
    assert_eq!(sheet.raw(CellRef { row: 10, col: 0 }), None);
    assert_eq!(sheet.raw(CellRef { row: 11, col: 1 }), None);
}

#[test]
fn test_paste_matrix_with_undo_redo() {
    let state = make_test_state();
    let mut sheet = state.current.borrow_mut();
    let mut undo = state.undo_stack.borrow_mut();
    let mut redo = state.redo_stack.borrow_mut();

    let target_sel = GridSelection::new(CellRef { row: 5, col: 5 }, CellRef { row: 5, col: 5 });
    let data = vec![
        vec!["Alpha".to_string(), "Beta".to_string()],
        vec!["Gamma".to_string(), "Delta".to_string()],
    ];
    let pasted = paste_selection(&mut sheet, &mut undo, &mut redo, target_sel, &data);
    assert_eq!(pasted, 4);
    assert_eq!(sheet.raw(CellRef { row: 5, col: 5 }), Some("Alpha"));
    assert_eq!(sheet.raw(CellRef { row: 5, col: 6 }), Some("Beta"));
    assert_eq!(sheet.raw(CellRef { row: 6, col: 5 }), Some("Gamma"));
    assert_eq!(sheet.raw(CellRef { row: 6, col: 6 }), Some("Delta"));

    // Revert
    let tx = undo.pop().unwrap();
    tx.revert(&mut sheet);
    assert_eq!(sheet.raw(CellRef { row: 5, col: 5 }), None);
    assert_eq!(sheet.raw(CellRef { row: 5, col: 6 }), None);

    // Re-apply
    tx.apply(&mut sheet);
    assert_eq!(sheet.raw(CellRef { row: 5, col: 5 }), Some("Alpha"));
    assert_eq!(sheet.raw(CellRef { row: 6, col: 6 }), Some("Delta"));
}

#[test]
fn test_cut_selection_with_undo_redo() {
    let state = make_test_state();
    let mut sheet = state.current.borrow_mut();
    let mut undo = state.undo_stack.borrow_mut();
    let mut redo = state.redo_stack.borrow_mut();

    let sel = GridSelection::new(CellRef { row: 1, col: 0 }, CellRef { row: 1, col: 1 });
    let data = copy_selection(&sheet, sel);
    assert_eq!(data[0][0], "Rent");
    assert_eq!(data[0][1], "1200");

    let cleared = clear_selection(&mut sheet, &mut undo, &mut redo, sel.range());
    assert!(cleared);
    assert_eq!(sheet.raw(CellRef { row: 1, col: 0 }), None);
    assert_eq!(sheet.raw(CellRef { row: 1, col: 1 }), None);
    assert_eq!(undo.len(), 1);

    // Undo restore
    undo.pop().unwrap().revert(&mut sheet);
    assert_eq!(sheet.raw(CellRef { row: 1, col: 0 }), Some("Rent"));
    assert_eq!(sheet.raw(CellRef { row: 1, col: 1 }), Some("1200"));
}

#[test]
fn test_select_all_range() {
    let state = make_test_state();
    let sheet = state.current.borrow();
    let sel = select_all_range(&sheet);
    assert_eq!(sel.anchor, CellRef { row: 0, col: 0 });
    let dims = sheet.dimensions();
    assert_eq!(sel.focus.row, dims.rows - 1);
    assert_eq!(sel.focus.col, dims.cols - 1);
}

#[test]
fn test_delete_row_and_undo_redo() {
    let state = make_test_state();
    let mut sheet = state.current.borrow_mut();
    let mut undo = state.undo_stack.borrow_mut();
    let mut redo = state.redo_stack.borrow_mut();

    let before = sheet.clone();
    assert_eq!(sheet.raw(CellRef { row: 1, col: 0 }), Some("Rent"));
    assert_eq!(sheet.raw(CellRef { row: 2, col: 0 }), Some("Food"));

    let new_sheet = delete_row(&before, 1).expect("delete row 1");
    commit_transaction(
        &mut sheet,
        &mut undo,
        &mut redo,
        SheetTransaction::Snapshot {
            before: Box::new(before),
            after: Box::new(new_sheet),
        },
    );

    // After deleting row 1 ("Rent"), row 1 is now "Food"
    assert_eq!(sheet.raw(CellRef { row: 1, col: 0 }), Some("Food"));
    assert_eq!(undo.len(), 1);

    // Revert restores "Rent"
    let tx = undo.pop().unwrap();
    tx.revert(&mut sheet);
    assert_eq!(sheet.raw(CellRef { row: 1, col: 0 }), Some("Rent"));
    assert_eq!(sheet.raw(CellRef { row: 2, col: 0 }), Some("Food"));

    // Re-apply shifts again
    tx.apply(&mut sheet);
    assert_eq!(sheet.raw(CellRef { row: 1, col: 0 }), Some("Food"));
}

#[test]
fn test_delete_col_and_undo_redo() {
    let state = make_test_state();
    let mut sheet = state.current.borrow_mut();
    let mut undo = state.undo_stack.borrow_mut();
    let mut redo = state.redo_stack.borrow_mut();

    let before = sheet.clone();
    assert_eq!(sheet.raw(CellRef { row: 0, col: 0 }), Some("Item"));
    assert_eq!(sheet.raw(CellRef { row: 0, col: 1 }), Some("Amount"));

    let new_sheet = delete_col(&before, 0).expect("delete col 0");
    commit_transaction(
        &mut sheet,
        &mut undo,
        &mut redo,
        SheetTransaction::Snapshot {
            before: Box::new(before),
            after: Box::new(new_sheet),
        },
    );

    // Col 0 is now "Amount"
    assert_eq!(sheet.raw(CellRef { row: 0, col: 0 }), Some("Amount"));
    assert_eq!(undo.len(), 1);

    // Revert
    let tx = undo.pop().unwrap();
    tx.revert(&mut sheet);
    assert_eq!(sheet.raw(CellRef { row: 0, col: 0 }), Some("Item"));
    assert_eq!(sheet.raw(CellRef { row: 0, col: 1 }), Some("Amount"));
}

#[test]
fn test_sort_table_ascending_and_descending_with_undo() {
    let state = make_test_state();
    let mut sheet = state.current.borrow_mut();
    let mut undo = state.undo_stack.borrow_mut();
    let mut redo = state.redo_stack.borrow_mut();

    // Sort ascending by Col A
    let ok = sort_table(&mut sheet, &mut undo, &mut redo, 0, true);
    assert!(ok);
    assert_eq!(undo.len(), 1);
    let top_row_val = sheet.raw(CellRef { row: 1, col: 0 }).unwrap().to_string();

    // Sort descending by Col A
    let ok_desc = sort_table(&mut sheet, &mut undo, &mut redo, 0, false);
    assert!(ok_desc);
    assert_eq!(undo.len(), 2);
    let desc_row_val = sheet.raw(CellRef { row: 1, col: 0 }).unwrap().to_string();

    // In descending order, first item should be >= top_row_val
    assert!(desc_row_val >= top_row_val);

    // Undo restores ascending
    undo.pop().unwrap().revert(&mut sheet);
    assert_eq!(
        sheet.raw(CellRef { row: 1, col: 0 }).unwrap(),
        top_row_val.as_str()
    );

    // Undo again restores original
    undo.pop().unwrap().revert(&mut sheet);
    assert_eq!(sheet.raw(CellRef { row: 1, col: 0 }).unwrap(), "Rent");
}

#[test]
fn test_freeze_and_unfreeze_panes_with_undo() {
    let mut sheet = Sheet::new("FreezeTest");
    assert_eq!(sheet.freeze_rows, 0);
    assert_eq!(sheet.freeze_cols, 0);

    let before = sheet.clone();
    let mut after = sheet.clone();
    after.freeze_panes(1, 0);

    let tx = SheetTransaction::Snapshot {
        before: Box::new(before),
        after: Box::new(after),
    };

    tx.apply(&mut sheet);
    assert_eq!(sheet.freeze_rows, 1);

    tx.revert(&mut sheet);
    assert_eq!(sheet.freeze_rows, 0);
}
