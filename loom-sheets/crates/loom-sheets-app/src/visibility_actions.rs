//! Undoable Hide/Unhide Rows & Columns and Freeze Panes at Selection actions.

use loom_sheets_core::visibility::{hide_cols, hide_rows, unhide_cols, unhide_rows};
use loom_sheets_core::{CellRange, Sheet};

/// Undoable action record for row/column visibility and freeze-pane changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VisibilityUndo {
    RowsHidden(Vec<u32>),
    RowsUnhidden(Vec<u32>),
    ColsHidden(Vec<u32>),
    ColsUnhidden(Vec<u32>),
    FreezeChanged { prev_rows: u32, prev_cols: u32 },
}

/// Hides all rows in `range` and returns an undo record.
pub fn hide_selected_rows(sheet: &mut Sheet, range: CellRange) -> VisibilityUndo {
    VisibilityUndo::RowsHidden(hide_rows(sheet, range.start.row, range.end.row))
}

/// Unhides all rows in `range` (or 1..=1000 if single row) and returns an undo record.
pub fn unhide_selected_rows(sheet: &mut Sheet, range: CellRange) -> VisibilityUndo {
    VisibilityUndo::RowsUnhidden(unhide_rows(sheet, range.start.row, range.end.row))
}

/// Hides all columns in `range` and returns an undo record.
pub fn hide_selected_cols(sheet: &mut Sheet, range: CellRange) -> VisibilityUndo {
    VisibilityUndo::ColsHidden(hide_cols(sheet, range.start.col, range.end.col))
}

/// Unhides all columns in `range` and returns an undo record.
pub fn unhide_selected_cols(sheet: &mut Sheet, range: CellRange) -> VisibilityUndo {
    VisibilityUndo::ColsUnhidden(unhide_cols(sheet, range.start.col, range.end.col))
}

/// Freezes panes at the top-left of `selection` (rows above `start.row`, cols left of `start.col`).
pub fn freeze_panes_at_selection(sheet: &mut Sheet, selection: CellRange) -> VisibilityUndo {
    let prev_rows = sheet.freeze_rows;
    let prev_cols = sheet.freeze_cols;
    sheet.freeze_rows = selection.start.row.saturating_sub(1);
    sheet.freeze_cols = selection.start.col.saturating_sub(1);
    VisibilityUndo::FreezeChanged {
        prev_rows,
        prev_cols,
    }
}

/// Reverts a `VisibilityUndo` on `sheet`.
#[cfg(test)]
pub fn undo_visibility_action(sheet: &mut Sheet, undo: VisibilityUndo) {
    match undo {
        VisibilityUndo::RowsHidden(rows) => {
            for r in rows {
                sheet.hidden_rows.remove(&r);
            }
        }
        VisibilityUndo::RowsUnhidden(rows) => {
            for r in rows {
                sheet.hidden_rows.insert(r);
            }
        }
        VisibilityUndo::ColsHidden(cols) => {
            for c in cols {
                sheet.hidden_cols.remove(&c);
            }
        }
        VisibilityUndo::ColsUnhidden(cols) => {
            for c in cols {
                sheet.hidden_cols.insert(c);
            }
        }
        VisibilityUndo::FreezeChanged {
            prev_rows,
            prev_cols,
        } => {
            sheet.freeze_rows = prev_rows;
            sheet.freeze_cols = prev_cols;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loom_sheets_core::CellRef;

    #[test]
    fn hide_unhide_and_freeze_at_selection_undo() {
        let mut sheet = Sheet::new("Sheet1");
        let cr = |row, col| CellRef { row, col };
        let sel = CellRange::new(cr(3, 4), cr(5, 6));

        let u1 = hide_selected_rows(&mut sheet, sel);
        assert!(sheet.hidden_rows.contains(&3));
        assert!(sheet.hidden_rows.contains(&5));
        undo_visibility_action(&mut sheet, u1);
        assert!(!sheet.hidden_rows.contains(&3));

        let u2 = hide_selected_cols(&mut sheet, sel);
        assert!(sheet.hidden_cols.contains(&4));
        assert!(sheet.hidden_cols.contains(&6));
        undo_visibility_action(&mut sheet, u2);
        assert!(!sheet.hidden_cols.contains(&4));

        let u3 = freeze_panes_at_selection(&mut sheet, sel);
        assert_eq!(sheet.freeze_rows, 2);
        assert_eq!(sheet.freeze_cols, 3);
        undo_visibility_action(&mut sheet, u3);
        assert_eq!(sheet.freeze_rows, 0);
        assert_eq!(sheet.freeze_cols, 0);
    }
}
