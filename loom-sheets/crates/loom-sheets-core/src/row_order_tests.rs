use super::*;
use crate::{CellRange, Sheet};

fn at(row: u32, col: u32) -> CellRef {
    CellRef { row, col }
}

/// Rows 1..=4 hold keys 3, 1, 2, 1 in column 0 and text in column 1; row 1
/// also has a cell in column 2 that lies outside the sorted columns.
fn sample() -> (Sheet, HashMap<CellRef, Value>) {
    let mut sheet = Sheet::new("Sort");
    let keys = [3.0, 1.0, 2.0, 1.0];
    let mut values = HashMap::new();
    for (offset, key) in keys.iter().enumerate() {
        let row = offset as u32 + 1;
        sheet.set_raw(at(row, 0), key.to_string());
        sheet.set_raw(at(row, 1), format!("row {row}"));
        values.insert(at(row, 0), Value::Number(*key));
    }
    sheet.set_raw(at(1, 2), "outside");
    (sheet, values)
}

fn range() -> CellRange {
    CellRange::new(at(1, 0), at(4, 1))
}

fn texts(sheet: &Sheet) -> Vec<String> {
    (1..=4)
        .map(|row| sheet.raw(at(row, 1)).unwrap_or("").to_string())
        .collect()
}

#[test]
fn ascending_sorts_by_key_and_keeps_ties_in_their_original_order() {
    let (_, values) = sample();
    let order = RowOrder::sorted(range(), 0, true, &values).expect("sort");
    assert_eq!(order.order, vec![1, 3, 2, 0]);
}

#[test]
fn descending_reverses_the_keys_and_still_keeps_ties_stable() {
    let (_, values) = sample();
    let order = RowOrder::sorted(range(), 0, false, &values).expect("sort");
    assert_eq!(order.order, vec![0, 2, 1, 3]);
}

#[test]
fn a_sort_column_outside_the_range_is_rejected() {
    let (_, values) = sample();
    assert!(RowOrder::sorted(range(), 2, true, &values).is_err());
}

#[test]
fn apply_moves_whole_rows_and_leaves_other_columns_alone() {
    let (mut sheet, values) = sample();
    let order = RowOrder::sorted(range(), 0, true, &values).expect("sort");
    order.apply(&mut sheet);
    assert_eq!(texts(&sheet), ["row 2", "row 4", "row 3", "row 1"]);
    assert_eq!(sheet.raw(at(1, 2)), Some("outside"));
    assert_eq!(sheet.raw(at(4, 0)), Some("3"));
}

#[test]
fn revert_restores_the_exact_previous_cells() {
    let (mut sheet, values) = sample();
    let before: Vec<(CellRef, String)> = sheet
        .cells
        .iter()
        .map(|(at, cell)| (*at, cell.raw.clone()))
        .collect();
    let order = RowOrder::sorted(range(), 0, false, &values).expect("sort");
    order.apply(&mut sheet);
    order.revert(&mut sheet);
    let after: Vec<(CellRef, String)> = sheet
        .cells
        .iter()
        .map(|(at, cell)| (*at, cell.raw.clone()))
        .collect();
    assert_eq!(after, before);
}

#[test]
fn an_entry_costs_a_few_bytes_per_row_not_per_cell() {
    let (_, values) = sample();
    let order = RowOrder::sorted(range(), 0, true, &values).expect("sort");
    assert!(order.memory_bytes() < 256);
}

#[test]
fn sorting_a_shared_sheet_reuses_cell_allocations_and_restores_digest_on_revert() {
    let (mut sheet, values) = sample();
    let baseline = sheet.clone();
    let full_range = CellRange::new(at(1, 0), at(4, 2));
    let order = RowOrder::sorted(full_range, 0, true, &values).expect("sort");
    order.apply(&mut sheet);
    assert_ne!(sheet.cells.digest(), baseline.cells.digest());
    // Row 2 moved to row 1 without cloning its underlying Cell strings.
    assert_eq!(
        sheet.raw(at(1, 1)).map(str::as_ptr),
        baseline.raw(at(2, 1)).map(str::as_ptr)
    );
    order.revert(&mut sheet);
    assert_eq!(sheet.cells.digest(), baseline.cells.digest());
    assert_eq!(
        sheet.raw(at(2, 1)).map(str::as_ptr),
        baseline.raw(at(2, 1)).map(str::as_ptr)
    );
}

#[test]
fn only_formulas_about_their_own_row_count_as_row_local() {
    // 0-based row 2 is the sheet's row 3.
    for local in [
        "=B3*C3",
        "=SUM(B3:D3)",
        "=$B3+1",
        "=LOG10(B3)",
        "=\"B9\"&B3",
        "=1+2",
    ] {
        assert!(is_row_local_formula(local, 2), "{local}");
    }
    for crossing in [
        "=B3*C4",
        "=SUM(B1:B3)",
        "=B$3",
        "=B3+Sheet2!A3",
        "=B3+'My Sheet'!A3",
        "=B3+B2",
    ] {
        assert!(!is_row_local_formula(crossing, 2), "{crossing}");
    }
}

/// A table with a Total column: `C = A * B` in every row.
fn priced() -> (Sheet, HashMap<CellRef, Value>) {
    let mut sheet = Sheet::new("Priced");
    for (offset, (qty, price)) in [(3.0, 2.0), (1.0, 10.0), (2.0, 5.0)].iter().enumerate() {
        let row = offset as u32 + 1;
        sheet.set_raw(at(row, 0), qty.to_string());
        sheet.set_raw(at(row, 1), price.to_string());
        sheet.set_raw(at(row, 2), format!("=A{r}*B{r}", r = row + 1));
    }
    let values = crate::evaluate(&sheet);
    (sheet, values)
}

#[test]
fn sorting_rows_that_hold_formulas_keeps_each_formula_on_its_own_row() {
    let (mut sheet, values) = priced();
    assert_eq!(sheet.first_cross_row_formula_row(1), None);
    let original: Vec<String> = (1..=3)
        .map(|r| sheet.raw(at(r, 2)).unwrap().to_string())
        .collect();
    let range = CellRange::new(at(1, 0), at(3, 2));
    let order = RowOrder::sorted(range, 0, true, &values).expect("sort by quantity");
    order.apply(&mut sheet);
    // Quantities 1, 2, 3 and each total follows its row.
    let after = crate::evaluate(&sheet);
    let numbers = |col| -> Vec<f64> {
        (1..=3)
            .map(|r| match after.get(&at(r, col)) {
                Some(Value::Number(n)) => *n,
                other => panic!("row {r} col {col}: {other:?}"),
            })
            .collect()
    };
    assert_eq!(numbers(0), vec![1.0, 2.0, 3.0]);
    assert_eq!(numbers(2), vec![10.0, 10.0, 6.0], "A x B on the same row");
    assert_eq!(
        sheet.raw(at(1, 2)),
        Some("=A2*B2"),
        "the formula names its new row"
    );
    // Undo puts every formula back exactly.
    order.revert(&mut sheet);
    let restored: Vec<String> = (1..=3)
        .map(|r| sheet.raw(at(r, 2)).unwrap().to_string())
        .collect();
    assert_eq!(restored, original);
}

#[test]
fn a_row_with_a_total_over_other_rows_stops_the_sort() {
    let (mut sheet, _) = priced();
    sheet.set_raw(at(4, 2), "=SUM(C2:C4)");
    assert_eq!(sheet.first_cross_row_formula_row(1), Some(4));
}

#[test]
fn a_formula_that_only_points_at_a_blank_cell_is_zero() {
    let mut sheet = Sheet::new("Blank");
    sheet.set_raw(at(0, 0), "=C7");
    sheet.set_raw(at(1, 0), "=A1+1");
    let values = crate::evaluate(&sheet);
    assert_eq!(values.get(&at(0, 0)), Some(&Value::Number(0.0)));
    assert_eq!(values.get(&at(1, 0)), Some(&Value::Number(1.0)));
}
