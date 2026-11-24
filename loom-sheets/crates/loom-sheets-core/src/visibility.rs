//! Row and column visibility helpers and visible-index mapping.

use crate::Sheet;

/// Hides all rows in `[start_row, end_row]` (1-based) and returns the rows that were newly hidden.
pub fn hide_rows(sheet: &mut Sheet, start_row: u32, end_row: u32) -> Vec<u32> {
    let (lo, hi) = (start_row.min(end_row), start_row.max(end_row));
    let mut changed = Vec::new();
    for r in lo..=hi {
        if r >= 1 && sheet.hidden_rows.insert(r) {
            changed.push(r);
        }
    }
    changed
}

/// Unhides all rows in `[start_row, end_row]` (1-based) and returns the rows that were newly unhidden.
pub fn unhide_rows(sheet: &mut Sheet, start_row: u32, end_row: u32) -> Vec<u32> {
    let (lo, hi) = (start_row.min(end_row), start_row.max(end_row));
    let mut changed = Vec::new();
    for r in lo..=hi {
        if sheet.hidden_rows.remove(&r) {
            changed.push(r);
        }
    }
    changed
}

/// Hides all columns in `[start_col, end_col]` (1-based) and returns the columns that were newly hidden.
pub fn hide_cols(sheet: &mut Sheet, start_col: u32, end_col: u32) -> Vec<u32> {
    let (lo, hi) = (start_col.min(end_col), start_col.max(end_col));
    let mut changed = Vec::new();
    for c in lo..=hi {
        if c >= 1 && sheet.hidden_cols.insert(c) {
            changed.push(c);
        }
    }
    changed
}

/// Unhides all columns in `[start_col, end_col]` (1-based) and returns the columns that were newly unhidden.
pub fn unhide_cols(sheet: &mut Sheet, start_col: u32, end_col: u32) -> Vec<u32> {
    let (lo, hi) = (start_col.min(end_col), start_col.max(end_col));
    let mut changed = Vec::new();
    for c in lo..=hi {
        if sheet.hidden_cols.remove(&c) {
            changed.push(c);
        }
    }
    changed
}

/// Returns `true` if `row` is hidden.
pub fn is_row_hidden(sheet: &Sheet, row: u32) -> bool {
    sheet.hidden_rows.contains(&row)
}

/// Returns `true` if `col` is hidden.
pub fn is_col_hidden(sheet: &Sheet, col: u32) -> bool {
    sheet.hidden_cols.contains(&col)
}

/// Returns the next visible row `>= from_row` up to `max_row`.
pub fn next_visible_row(sheet: &Sheet, from_row: u32, max_row: u32) -> Option<u32> {
    (from_row.max(1)..=max_row).find(|r| !sheet.hidden_rows.contains(r))
}

/// Returns the previous visible row `<= from_row` down to 1.
pub fn prev_visible_row(sheet: &Sheet, from_row: u32) -> Option<u32> {
    (1..=from_row)
        .rev()
        .find(|r| !sheet.hidden_rows.contains(r))
}

/// Returns the next visible column `>= from_col` up to `max_col`.
pub fn next_visible_col(sheet: &Sheet, from_col: u32, max_col: u32) -> Option<u32> {
    (from_col.max(1)..=max_col).find(|c| !sheet.hidden_cols.contains(c))
}

/// Returns the previous visible column `<= from_col` down to 1.
pub fn prev_visible_col(sheet: &Sheet, from_col: u32) -> Option<u32> {
    (1..=from_col)
        .rev()
        .find(|c| !sheet.hidden_cols.contains(c))
}

/// Returns all visible row indices in `[start_row, end_row]`.
pub fn visible_row_indices(sheet: &Sheet, start_row: u32, end_row: u32) -> Vec<u32> {
    (start_row..=end_row)
        .filter(|r| !sheet.hidden_rows.contains(r))
        .collect()
}

/// Returns all visible column indices in `[start_col, end_col]`.
pub fn visible_col_indices(sheet: &Sheet, start_col: u32, end_col: u32) -> Vec<u32> {
    (start_col..=end_col)
        .filter(|c| !sheet.hidden_cols.contains(c))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hide_unhide_rows_and_cols_and_navigation() {
        let mut sheet = Sheet::new("Sheet1");
        assert_eq!(hide_rows(&mut sheet, 2, 3), vec![2, 3]);
        assert!(is_row_hidden(&sheet, 2));
        assert_eq!(next_visible_row(&sheet, 2, 10), Some(4));
        assert_eq!(prev_visible_row(&sheet, 3), Some(1));
        assert_eq!(visible_row_indices(&sheet, 1, 5), vec![1, 4, 5]);
        assert_eq!(unhide_rows(&mut sheet, 1, 5), vec![2, 3]);

        assert_eq!(hide_cols(&mut sheet, 2, 4), vec![2, 3, 4]);
        assert!(is_col_hidden(&sheet, 3));
        assert_eq!(next_visible_col(&sheet, 2, 10), Some(5));
        assert_eq!(prev_visible_col(&sheet, 4), Some(1));
        assert_eq!(visible_col_indices(&sheet, 1, 6), vec![1, 5, 6]);
        assert_eq!(unhide_cols(&mut sheet, 2, 4), vec![2, 3, 4]);
    }
}
