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
