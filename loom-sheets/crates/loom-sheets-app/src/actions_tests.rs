//! Unit tests for Loom Sheets actions and table mutations.

use std::rc::Rc;

use loom_desktop::{
    build_standard_menu_bar, CommandAction, CommandState, FileFilter, Menu, MenuBarService,
    MenuItem, MenuShortcut, NativeMenuBar, ScriptedFileDialogs,
};
use loom_sheets_core::{
    compute_pivot, evaluate, export_xlsx_from_grid, from_csv, shift_formula_references, to_csv,
    CalcError, CellAlignment, CellRange, CellRef, ChartKind, ChartSeries, ChartSpec,
    PivotAggregation, Sheet, SheetModel, Value,
};

use super::*;
use crate::analysis::{
    line_path_commands, pie_wedge_commands, plan_chart, plan_pivot_sheet, unique_sheet_name,
};
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
fn test_workbook_tab_add_undo_redo() {
    let state = make_test_state();
    assert_eq!(state.sheets.borrow().len(), 1);

    let mut after = state.sheets.borrow().clone();
    after.push(Sheet::new("Second"));
    commit_workbook_transaction(&state, after, 1, None);
    assert_eq!(state.sheets.borrow().len(), 2);
    assert_eq!(*state.active_sheet_index.borrow(), 1);
    assert_eq!(state.current.borrow().name, "Second");
    assert_eq!(state.undo_stack.borrow().len(), 1);

    // Undo restores the single-tab workbook.
    let tx = state.undo_stack.borrow_mut().pop().unwrap();
    match &tx {
        SheetTransaction::Workbook { before, .. } => before.restore(&state),
        _ => panic!("expected workbook transaction"),
    }
    state.redo_stack.borrow_mut().push(tx);
    assert_eq!(state.sheets.borrow().len(), 1);
    assert_eq!(*state.active_sheet_index.borrow(), 0);
    assert_eq!(state.current.borrow().name, "Budget");

    // Redo re-applies the added tab.
    let tx = state.redo_stack.borrow_mut().pop().unwrap();
    match &tx {
        SheetTransaction::Workbook { after, .. } => after.restore(&state),
        _ => panic!("expected workbook transaction"),
    }
    state.undo_stack.borrow_mut().push(tx);
    assert_eq!(state.sheets.borrow().len(), 2);
    assert_eq!(state.current.borrow().name, "Second");
}

#[test]
fn test_workbook_tab_delete_and_rename_undo() {
    let state = make_test_state();
    let mut second = Sheet::new("Keep");
    second.set_str("A1", "precious");
    state.sheets.borrow_mut().push(second);
    *state.active_sheet_index.borrow_mut() = 1;
    *state.current.borrow_mut() = state.sheets.borrow()[1].clone();

    // Delete the second tab, then undo: content must come back.
    let mut after = state.sheets.borrow().clone();
    after.remove(1);
    commit_workbook_transaction(&state, after, 0, Some(1));
    assert_eq!(state.sheets.borrow().len(), 1);
    let tx = state.undo_stack.borrow_mut().pop().unwrap();
    match &tx {
        SheetTransaction::Workbook { before, .. } => before.restore(&state),
        _ => panic!("expected workbook transaction"),
    }
    assert_eq!(state.sheets.borrow().len(), 2);
    assert_eq!(
        state.sheets.borrow()[1].raw(CellRef::parse("A1").unwrap()),
        Some("precious")
    );

    // Rename the active tab, then undo: the old name returns.
    let active = *state.active_sheet_index.borrow();
    let mut renamed = state.sheets.borrow().clone();
    renamed[active].name = "Renamed".to_string();
    commit_workbook_transaction(&state, renamed, active, None);
    assert_eq!(state.current.borrow().name, "Renamed");
    let tx = state.undo_stack.borrow_mut().pop().unwrap();
    match &tx {
        SheetTransaction::Workbook { before, .. } => before.restore(&state),
        _ => panic!("expected workbook transaction"),
    }
    assert_eq!(state.current.borrow().name, "Keep");
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

#[test]
fn test_deep_audit_formulae_and_calculation_engine() {
    let mut sheet = Sheet::new("AuditFormulae");
    sheet.set_str("A1", "10");
    sheet.set_str("A2", "20");
    sheet.set_str("A3", "30");

    // Arithmetic
    sheet.set_str("B1", "=A1 + A2 * 2");
    // Aggregate functions
    sheet.set_str("B2", "=SUM(A1:A3)");
    sheet.set_str("B3", "=AVERAGE(A1:A3)");
    sheet.set_str("B4", "=MIN(A1:A3)");
    sheet.set_str("B5", "=MAX(A1:A3)");

    // Division by zero error
    sheet.set_str("C1", "=A1 / 0");

    // Logical & conditional
    sheet.set_str("D1", "=IF(A1 > 5, 100, 200)");

    let results = evaluate(&sheet);

    assert_eq!(
        results.get(&CellRef::parse("B1").unwrap()),
        Some(&Value::Number(50.0))
    );
    assert_eq!(
        results.get(&CellRef::parse("B2").unwrap()),
        Some(&Value::Number(60.0))
    );
    assert_eq!(
        results.get(&CellRef::parse("B3").unwrap()),
        Some(&Value::Number(20.0))
    );
    assert_eq!(
        results.get(&CellRef::parse("B4").unwrap()),
        Some(&Value::Number(10.0))
    );
    assert_eq!(
        results.get(&CellRef::parse("B5").unwrap()),
        Some(&Value::Number(30.0))
    );

    // Error check
    assert_eq!(
        results.get(&CellRef::parse("C1").unwrap()),
        Some(&Value::Error(CalcError::DivZero))
    );

    // Conditional check
    assert_eq!(
        results.get(&CellRef::parse("D1").unwrap()),
        Some(&Value::Number(100.0))
    );

    // Formula reference shifting
    let shifted = shift_formula_references("=A1+B1", 1, 2);
    assert_eq!(shifted, "=B3+C3");
}

#[test]
fn test_deep_audit_charts_and_visualization_pipeline() {
    let spec = ChartSpec {
        kind: ChartKind::Bar,
        title: "Revenue".into(),
        series: vec![ChartSeries {
            name: "S1".into(),
            categories: vec!["A".into(), "B".into(), "C".into()],
            values: vec![100.0, 200.0, 300.0],
        }],
    };
    assert!(spec.validate().is_ok());
    let norm = spec.normalized_points().expect("normalized points");
    assert_eq!(norm, vec![vec![0.0, 0.5, 1.0]]);

    let line_spec = ChartSpec {
        kind: ChartKind::Line,
        title: "Trends".into(),
        series: vec![ChartSeries {
            name: "S1".into(),
            categories: vec!["A".into(), "B".into(), "C".into()],
            values: vec![10.0, 50.0, 90.0],
        }],
    };
    assert!(line_spec.validate().is_ok());
    assert_eq!(
        line_spec.normalized_points().unwrap(),
        vec![vec![0.0, 0.5, 1.0]]
    );

    let pie_spec = ChartSpec {
        kind: ChartKind::Pie,
        title: "Shares".into(),
        series: vec![ChartSeries {
            name: "S1".into(),
            categories: vec!["A".into(), "B".into()],
            values: vec![25.0, 75.0],
        }],
    };
    assert!(pie_spec.validate().is_ok());
}

#[test]
fn test_deep_audit_multi_sheet_and_undo_isolation() {
    let state = make_test_state();

    // Add Sheet 2 and Sheet 3
    let mut s2 = Sheet::new("Expenses");
    s2.set_str("A1", "Rent");
    state.sheets.borrow_mut().push(s2);
    state
        .sheet_histories
        .borrow_mut()
        .push((Vec::new(), Vec::new()));

    let mut s3 = Sheet::new("Summary");
    s3.set_str("A1", "Net");
    state.sheets.borrow_mut().push(s3);
    state
        .sheet_histories
        .borrow_mut()
        .push((Vec::new(), Vec::new()));

    assert_eq!(state.sheets.borrow().len(), 3);

    // Switch to Sheet 2 (index 1)
    let cur = state.current.borrow().clone();
    state.sheets.borrow_mut()[0] = cur;
    state.sheet_histories.borrow_mut()[0] = (
        state.undo_stack.borrow().clone(),
        state.redo_stack.borrow().clone(),
    );
    *state.active_sheet_index.borrow_mut() = 1;
    *state.current.borrow_mut() = state.sheets.borrow()[1].clone();
    state.undo_stack.borrow_mut().clear();
    state.redo_stack.borrow_mut().clear();

    // Mutate cell on Sheet 2 with undo recording
    let committed = commit_formula_edit(
        &mut state.current.borrow_mut(),
        &mut state.undo_stack.borrow_mut(),
        &mut state.redo_stack.borrow_mut(),
        CellRef::parse("A1").unwrap(),
        "Utilities",
    );
    assert!(committed);

    assert_eq!(
        state.current.borrow().raw(CellRef::parse("A1").unwrap()),
        Some("Utilities")
    );
    assert_eq!(state.undo_stack.borrow().len(), 1);

    // Undo on Sheet 2
    let tx = state.undo_stack.borrow_mut().pop().unwrap();
    tx.revert(&mut state.current.borrow_mut());
    assert_eq!(
        state.current.borrow().raw(CellRef::parse("A1").unwrap()),
        Some("Rent")
    );

    // Verify Sheet 1 was unaffected
    assert_eq!(
        state.sheets.borrow()[0].raw(CellRef::parse("A1").unwrap()),
        Some("Item")
    );

    // Rename active sheet
    state.current.borrow_mut().name = "Operating Expenses".to_string();
    assert_eq!(state.current.borrow().name, "Operating Expenses");

    // Remove active sheet from state
    let active = *state.active_sheet_index.borrow();
    state.sheets.borrow_mut().remove(active);
    state.sheet_histories.borrow_mut().remove(active);
    let next_idx = active.min(state.sheets.borrow().len().saturating_sub(1));
    *state.active_sheet_index.borrow_mut() = next_idx;
    *state.current.borrow_mut() = state.sheets.borrow()[next_idx].clone();

    assert_eq!(state.sheets.borrow().len(), 2);
    assert_eq!(*state.active_sheet_index.borrow(), 1);
    assert_eq!(state.current.borrow().name, "Summary");
}

#[test]
fn test_deep_audit_structural_mutations_and_sizing() {
    let state = make_test_state();

    // Initial starter sheet has 6 rows and 3 columns
    let initial_dims = state.current.borrow().dimensions();
    assert_eq!(initial_dims.rows, 6);
    assert_eq!(initial_dims.cols, 3);

    // Delete row 2
    let before = state.current.borrow().clone();
    let maybe_sheet = delete_row(&before, 2);
    assert!(maybe_sheet.is_some());
    let after = maybe_sheet.unwrap();
    assert_eq!(after.dimensions().rows, 5);

    let tx = SheetTransaction::Snapshot {
        before: Box::new(before),
        after: Box::new(after),
    };
    tx.apply(&mut state.current.borrow_mut());
    assert_eq!(state.current.borrow().dimensions().rows, 5);

    // Undo row deletion
    tx.revert(&mut state.current.borrow_mut());
    assert_eq!(state.current.borrow().dimensions().rows, 6);

    // Sizing adjustments
    state.current.borrow_mut().set_row_height(1, 32.0);
    assert_eq!(state.current.borrow().row_height(1), 32.0);

    state.current.borrow_mut().set_col_width(0, 120.0);
    assert_eq!(state.current.borrow().col_width(0), 120.0);
}

#[test]
fn test_deep_audit_clipboard_and_range_operations() {
    let mut sheet = Sheet::new("ClipboardAudit");
    sheet.set_str("A1", "Alpha");
    sheet.set_str("B1", "Beta");
    sheet.set_str("A2", "Gamma");
    sheet.set_str("B2", "Delta");

    // Copy 2x2 matrix
    let sel = GridSelection::new(CellRef::parse("A1").unwrap(), CellRef::parse("B2").unwrap());
    let matrix = copy_selection(&sheet, sel);
    assert_eq!(
        matrix,
        vec![
            vec!["Alpha".to_string(), "Beta".to_string()],
            vec!["Gamma".to_string(), "Delta".to_string()]
        ]
    );

    // Paste at offset C1
    let mut undo = Vec::new();
    let mut redo = Vec::new();
    let target_sel =
        GridSelection::new(CellRef::parse("C1").unwrap(), CellRef::parse("C1").unwrap());
    let pasted_count = paste_selection(&mut sheet, &mut undo, &mut redo, target_sel, &matrix);
    assert_eq!(pasted_count, 4);
    assert_eq!(sheet.raw(CellRef::parse("C1").unwrap()), Some("Alpha"));
    assert_eq!(sheet.raw(CellRef::parse("D1").unwrap()), Some("Beta"));
    assert_eq!(sheet.raw(CellRef::parse("C2").unwrap()), Some("Gamma"));
    assert_eq!(sheet.raw(CellRef::parse("D2").unwrap()), Some("Delta"));

    // Undo paste
    undo.pop().unwrap().revert(&mut sheet);
    assert_eq!(sheet.raw(CellRef::parse("C1").unwrap()), None);

    // Alignment test
    let align_changed = set_selection_alignment(
        &mut sheet,
        &mut undo,
        &mut redo,
        sel.range(),
        CellAlignment::Right,
    );
    assert!(align_changed);
    assert_eq!(
        sheet.cell_alignment(CellRef::parse("A1").unwrap()),
        CellAlignment::Right
    );

    // Undo alignment
    undo.pop().unwrap().revert(&mut sheet);
    assert_eq!(
        sheet.cell_alignment(CellRef::parse("A1").unwrap()),
        CellAlignment::General
    );
}

#[test]
fn test_deep_audit_data_interop_csv_and_xlsx() {
    let mut sheet = Sheet::new("InteropAudit");
    sheet.set_str("A1", "First Name");
    sheet.set_str("B1", "Notes, with comma");
    sheet.set_str("A2", "Jane");
    sheet.set_str("B2", "Multi\nLine");

    // CSV roundtrip
    let csv = to_csv(&sheet);
    let reloaded = from_csv("InteropAudit", &csv);
    assert_eq!(
        reloaded.raw(CellRef::parse("A1").unwrap()),
        Some("First Name")
    );
    assert_eq!(
        reloaded.raw(CellRef::parse("B1").unwrap()),
        Some("Notes, with comma")
    );
    assert_eq!(
        reloaded.raw(CellRef::parse("B2").unwrap()),
        Some("Multi\nLine")
    );

    // XLSX export
    let grid = vec![
        vec!["ID".to_string(), "Metric".to_string()],
        vec!["1".to_string(), "99.5".to_string()],
    ];
    let xlsx_bytes = export_xlsx_from_grid(&grid).expect("generate xlsx");
    assert!(!xlsx_bytes.is_empty());
    // Verify valid ZIP magic number
    assert_eq!(&xlsx_bytes[0..4], b"PK\x03\x04");
}

#[test]
fn test_deep_audit_native_macos_global_menu_bar() {
    let menu = NativeMenuBar::new();
    let bar = build_standard_menu_bar(
        "Loom Sheets",
        vec![
            MenuItem::action_with_shortcut(
                "file.export_csv",
                "Export CSV...",
                MenuShortcut::primary_shift("C"),
            ),
            MenuItem::action_with_shortcut(
                "file.export_xlsx",
                "Export Excel...",
                MenuShortcut::primary_shift("E"),
            ),
        ],
        vec![],
        vec![],
        vec![Menu::new(
            "Table",
            vec![
                MenuItem::action("table.add_row", "Add Row"),
                MenuItem::action("table.delete_row", "Delete Row"),
                MenuItem::action("table.add_col", "Add Column"),
                MenuItem::action("table.delete_col", "Delete Column"),
                MenuItem::Separator,
                MenuItem::action("table.sort_asc", "Sort Ascending"),
                MenuItem::action("table.sort_desc", "Sort Descending"),
            ],
        )],
    );

    // 1. Menu hierarchy structure
    assert!(bar.find_item("file.new").is_some());
    assert!(bar.find_item("file.save").is_some());
    assert!(bar.find_item("edit.undo").is_some());
    assert!(bar.find_item("table.sort_asc").is_some());
    assert!(bar.find_item("file.export_csv").is_some());

    // 2. Install menu bar
    assert!(menu.install_menu_bar(&bar).is_ok());
    assert!(menu.is_installed());

    // 3. Action sink registration and invocation
    let dispatched = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let dispatched_ref = dispatched.clone();
    menu.register_action_sink(Arc::new(move |action: CommandAction| {
        dispatched_ref.lock().unwrap().push(action.id);
        Ok(())
    }))
    .expect("register sink");

    // Enabled action dispatches
    assert!(menu.dispatch_action("file.save").is_ok());
    assert_eq!(dispatched.lock().unwrap().as_slice(), &["file.save"]);

    // 4. State updates: disable an item and verify dispatch is rejected
    assert!(menu.update_item("file.save", false, None).is_ok());
    assert!(menu.dispatch_action("file.save").is_err());

    // Re-enable and verify dispatch succeeds
    assert!(menu.update_item("file.save", true, None).is_ok());
    assert!(menu.dispatch_action("file.save").is_ok());
    assert_eq!(
        dispatched.lock().unwrap().as_slice(),
        &["file.save", "file.save"]
    );

    // 5. Dynamic command projection update
    let cmd_state = CommandState::action("file.save", "Save Workbook")
        .with_shortcut(MenuShortcut::primary("S"))
        .with_enabled(true);
    assert!(menu.update_command_state(&cmd_state).is_ok());
    assert_eq!(
        menu.installed_menu_bar()
            .unwrap()
            .find_item("file.save")
            .unwrap()
            .label(),
        Some("Save Workbook")
    );
}

#[test]
fn test_plan_chart_validates_source_columns() {
    let mut sheet = Sheet::new("Sales");
    sheet.set_str("A1", "Quarter");
    sheet.set_str("B1", "Revenue");
    sheet.set_str("A2", "Q1");
    sheet.set_str("B2", "15000");

    let chart = plan_chart(&sheet, 0, 1).expect("plannable chart");
    assert_eq!(chart.kind, ChartKind::Bar);
    assert_eq!(chart.title, "Sales Chart");
    assert_eq!((chart.cat_col, chart.val_col), (0, 1));

    let empty = Sheet::new("Empty");
    let err = plan_chart(&empty, 0, 1).expect_err("needs data rows");
    assert!(err.contains("data row"), "unexpected hint: {err}");

    let mut text_only = Sheet::new("Text");
    text_only.set_str("A1", "Name");
    text_only.set_str("A2", "Alice");
    let err = plan_chart(&text_only, 0, 0).expect_err("needs numbers");
    assert!(err.contains('A'), "hint names columns: {err}");
}

#[test]
fn test_chart_path_builders_and_snapshot_undo() {
    let norm = vec![0.0f32, 0.5, 1.0];
    let line = line_path_commands(&norm);
    assert!(line.starts_with('M'));
    assert_eq!(line.matches('L').count(), 2);
    assert!(line_path_commands(&[]).is_empty());

    let wedges = pie_wedge_commands(&[1.0, 1.0, 2.0]);
    assert_eq!(wedges.len(), 3);
    for wedge in &wedges {
        assert!(wedge.starts_with("M50,50"), "wedge: {wedge}");
        assert!(wedge.contains('A'), "wedge: {wedge}");
    }
    let empty_pie = pie_wedge_commands(&[0.0, -1.0]);
    assert_eq!(empty_pie.len(), 1);

    // Inserting a chart is one Snapshot transaction: revert removes it.
    let mut sheet = Sheet::new("Snap");
    sheet.set_str("A2", "Q1");
    sheet.set_str("B2", "5");
    let before = sheet.clone();
    let mut after = before.clone();
    after.chart = Some(plan_chart(&before, 0, 1).expect("plan"));
    let mut undo = Vec::new();
    let mut redo = Vec::new();
    commit_transaction(
        &mut sheet,
        &mut undo,
        &mut redo,
        SheetTransaction::Snapshot {
            before: Box::new(before),
            after: Box::new(after),
        },
    );
    assert!(sheet.chart.is_some());
    let tx = undo.pop().expect("chart tx");
    tx.revert(&mut sheet);
    assert!(sheet.chart.is_none());
}

#[test]
fn test_plan_pivot_sheet_builds_live_cross_sheet_formulas() {
    let mut data = Sheet::new("Data");
    for (c, v) in [
        ("A1", "Category"),
        ("B1", "Amount"),
        ("A2", "Food"),
        ("B2", "10"),
        ("A3", "Rent"),
        ("B3", "20"),
        ("A4", "Food"),
        ("B4", "30"),
    ] {
        data.set_str(c, v);
    }

    let (pivot, groups) = plan_pivot_sheet(&data, 0, 1, PivotAggregation::Sum).expect("plan");
    assert_eq!(groups, 2);
    assert_eq!(pivot.name, "Pivot of Data");
    // Live SUMIF formulas against the source tab, first-appearance order.
    assert_eq!(
        pivot.raw(CellRef::parse("B2").unwrap()),
        Some("=SUMIF(Data!$A$2:$A$4,\"Food\",Data!$B$2:$B$4)")
    );
    assert_eq!(
        pivot.raw(CellRef::parse("B3").unwrap()),
        Some("=SUMIF(Data!$A$2:$A$4,\"Rent\",Data!$B$2:$B$4)")
    );
    let evaluated = loom_sheets_core::workbook::evaluate_workbook(&[data.clone(), pivot]);
    assert_eq!(
        evaluated[1]
            .get(&CellRef::parse("B2").unwrap())
            .map(|v| v.display())
            .as_deref(),
        Some("40")
    );
    assert_eq!(
        evaluated[1]
            .get(&CellRef::parse("B3").unwrap())
            .map(|v| v.display())
            .as_deref(),
        Some("20")
    );

    let (count_pivot, count_groups) =
        plan_pivot_sheet(&data, 0, 1, PivotAggregation::Count).expect("count plan");
    assert_eq!(count_groups, 2);
    assert_eq!(
        count_pivot.raw(CellRef::parse("B2").unwrap()),
        Some("=COUNTIF(Data!$A$2:$A$4,\"Food\")")
    );
    let counted = loom_sheets_core::workbook::evaluate_workbook(&[data.clone(), count_pivot]);
    assert_eq!(
        counted[1]
            .get(&CellRef::parse("B2").unwrap())
            .map(|v| v.display())
            .as_deref(),
        Some("2")
    );
}

#[test]
fn test_plan_pivot_validates_and_names_uniquely() {
    let empty = Sheet::new("Empty");
    let err = plan_pivot_sheet(&empty, 0, 1, PivotAggregation::Sum).expect_err("needs rows");
    assert!(err.contains("data row"), "unexpected hint: {err}");

    let mut text_only = Sheet::new("Text");
    text_only.set_str("A1", "Name");
    text_only.set_str("A2", "Alice");
    let err = plan_pivot_sheet(&text_only, 0, 0, PivotAggregation::Sum).expect_err("needs numbers");
    assert!(err.contains('A'), "hint names columns: {err}");

    let existing = vec![Sheet::new("Pivot of Data")];
    assert_eq!(
        unique_sheet_name("Pivot of Data", &existing),
        "Pivot of Data 2"
    );
    assert_eq!(unique_sheet_name("Fresh", &existing), "Fresh");
}

#[test]
fn test_borders_fill_and_font_size_toggle_undo() {
    use crate::formatting::{
        adjust_selection_font_size, cycle_selection_fill, set_selection_fill,
        toggle_selection_borders,
    };
    use loom_sheets_core::style::FillColor;

    let mut sheet = Sheet::new("style");
    let range = CellRange::parse("A1:B2").unwrap();
    let mut undo = Vec::new();
    let mut redo = Vec::new();
    let cell = CellRef::parse("A1").unwrap();

    assert!(toggle_selection_borders(
        &mut sheet, &mut undo, &mut redo, range
    ));
    assert!(sheet.cell_style(cell).border);
    assert!(toggle_selection_borders(
        &mut sheet, &mut undo, &mut redo, range
    ));
    assert!(!sheet.cell_style(cell).border);
    assert_eq!(undo.len(), 2);

    assert!(cycle_selection_fill(
        &mut sheet, &mut undo, &mut redo, range
    ));
    assert_eq!(sheet.cell_style(cell).fill, FillColor::Red);
    assert!(set_selection_fill(
        &mut sheet,
        &mut undo,
        &mut redo,
        range,
        FillColor::Blue
    ));
    assert_eq!(sheet.cell_style(cell).fill, FillColor::Blue);
    assert!(!set_selection_fill(
        &mut sheet,
        &mut undo,
        &mut redo,
        range,
        FillColor::Blue
    ));

    assert!(adjust_selection_font_size(
        &mut sheet, &mut undo, &mut redo, range, 1
    ));
    assert_eq!(sheet.cell_style(cell).font_size, Some(15));
    assert!(adjust_selection_font_size(
        &mut sheet, &mut undo, &mut redo, range, -1
    ));
    // Stepping back onto the theme size clears the override.
    assert_eq!(sheet.cell_style(cell).font_size, None);

    // Undo restores defaults in reverse order.
    while let Some(tx) = undo.pop() {
        tx.revert(&mut sheet);
    }
    assert!(sheet.cell_style(cell).is_default());
}
