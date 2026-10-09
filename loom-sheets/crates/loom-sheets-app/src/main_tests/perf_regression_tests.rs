//! PERF-01 regression tests. Each test counts the work an edit, undo, or sort
//! does rather than timing it: whole-workbook serializations, the bytes an undo
//! entry keeps, and the bytes the history accounts for. Timing budgets live in
//! the opt-in benches in `perf_bench_tests.rs`.

use super::*;
use loom_sheets_core::persistence::workbook_serializations_on_this_thread;

/// A single-sheet state with `cells` numbers in column A.
fn state_with_numbers(cells: u32) -> Rc<GuiState> {
    let mut sheet = Sheet::new("Perf");
    for row in 0..cells {
        sheet.set_raw(CellRef { row, col: 0 }, row.to_string());
    }
    Rc::new(GuiState::new(
        sheet,
        None,
        Rc::new(loom_desktop::ScriptedFileDialogs::new([], [])),
        FileFilter::new("Workbook", ["loomtable"]).expect("filter"),
        FileFilter::new("CSV", ["csv"]).expect("filter"),
        FileFilter::new("CSV", ["csv"]).expect("filter"),
        FileFilter::new("Excel", ["xlsx"]).expect("filter"),
    ))
}

/// Undoing or redoing a committed edit must recheck dirtiness from the content
/// alone. Serializing the whole workbook for that check costs time proportional
/// to the workbook, so the recheck must not serialize it at all.
#[test]
fn undo_and_redo_recheck_dirtiness_without_serializing_the_workbook() {
    let state = state_with_numbers(2_000);
    state.mark_saved();
    let target = CellRef::parse("B7").expect("cell");
    assert!(commit_formula_edit(
        &mut state.current.borrow_mut(),
        &mut state.undo_stack.borrow_mut(),
        &mut state.redo_stack.borrow_mut(),
        target,
        "42",
    ));
    state.mark_content_dirty();

    let before = workbook_serializations_on_this_thread();
    state.recompute_dirty_from_saved();
    assert!(state.is_dirty(), "an edited workbook is unsaved");

    let undone = state.undo_stack.borrow_mut().pop().expect("undo entry");
    undone.revert(&mut state.current.borrow_mut());
    state.recompute_dirty_from_saved();
    assert!(!state.is_dirty(), "undo back to the saved content is clean");

    // Redo applies the entry again; the Redo callback moves it back to the redo stack.
    undone.apply(&mut state.current.borrow_mut());
    state.recompute_dirty_from_saved();
    assert!(state.is_dirty(), "redo returns to the edited content");

    assert_eq!(
        workbook_serializations_on_this_thread() - before,
        0,
        "dirty rechecks serialized the workbook"
    );
}

/// A sort undo entry must not keep a second copy of the sheet. It reorders
/// rows, so it needs only one number per row; keeping two whole sheets
/// (`Snapshot`) costs about a tenth of the cell map even for a small sheet.
#[test]
fn a_sort_keeps_an_undo_entry_far_smaller_than_the_cell_map() {
    let mut sheet = Sheet::new("Sort");
    for row in 0..200u32 {
        for col in 0..100u32 {
            let value = (row * 7_919 + col) % 1_000;
            sheet.set_raw(CellRef { row, col }, value.to_string());
        }
    }
    let footprint = sheet.cells.approximate_bytes();
    let original = sheet.clone();
    let (mut undo, mut redo) = (Vec::new(), Vec::new());
    assert!(crate::actions::sort_table(
        &mut sheet, &mut undo, &mut redo, 0, true
    ));
    let entry = history_bytes(&undo);
    assert!(
        entry * 100 < footprint,
        "sort undo entry keeps {entry} bytes of a {footprint}-byte cell map"
    );

    undo.pop().expect("sort entry").revert(&mut sheet);
    assert_eq!(sheet.cells.len(), original.cells.len());
    for (at, cell) in &original.cells {
        assert_eq!(sheet.cells.get(at), Some(cell), "undo restores {at:?}");
    }
}

/// History accounting must reflect the memory an entry keeps. Two whole-sheet
/// snapshots that share every cell band hold the cell map once, so the
/// accounted bytes must cover that map rather than a fraction of it.
#[test]
fn history_accounting_covers_the_cell_map_its_snapshots_share() {
    let mut sheet = Sheet::new("Shared");
    for row in 0..300u32 {
        sheet.set_raw(CellRef { row, col: 1 }, format!("value {row}"));
    }
    let footprint = sheet.cells.approximate_bytes();
    let (mut undo, mut redo) = (Vec::new(), Vec::new());
    for width in [120.0_f32, 140.0] {
        let before = sheet.clone();
        sheet.col_widths.insert(0, width);
        let after = sheet.clone();
        commit_transaction(
            &mut sheet,
            &mut undo,
            &mut redo,
            SheetTransaction::Snapshot {
                before: Box::new(before),
                after: Box::new(after),
            },
        );
    }
    let accounted = history_bytes(&undo);
    assert!(
        accounted >= footprint,
        "accounted {accounted} bytes but the shared cell map alone is {footprint}"
    );
    assert!(
        accounted < 2 * footprint + 64 * 1024,
        "shared bands were counted once per copy: {accounted} bytes"
    );
}
