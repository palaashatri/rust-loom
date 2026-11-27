//! Interactive Fill Handle state, hit-testing, and undoable batch auto-fill.

use loom_sheets_core::autofill::{double_click_fill_extent, generate_fill_values};
use loom_sheets_core::{Cell, CellRange, CellRef, Sheet};

/// State for an active fill-handle drag gesture.
#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FillHandleState {
    /// Whether the user is currently dragging the fill handle.
    pub dragging: bool,
    /// Original selection range at drag start.
    pub source_range: Option<CellRange>,
    /// Current target range during drag.
    pub target_range: Option<CellRange>,
}

#[cfg(test)]
impl FillHandleState {
    /// Returns `true` if `(px, py)` hits the 6x6px bottom-right corner handle
    /// centered at `(corner_x, corner_y)` (with a 10px hit slop).
    pub fn hit_test_corner(px: f32, py: f32, corner_x: f32, corner_y: f32) -> bool {
        (px - corner_x).abs() <= 6.0 && (py - corner_y).abs() <= 6.0
    }

    /// Begins a fill-handle drag from `source`.
    pub fn begin_drag(&mut self, source: CellRange) {
        self.dragging = true;
        self.source_range = Some(source);
        self.target_range = Some(source);
    }

    /// Updates the drag target range to include `hovered_cell`, constrained to
    /// either vertical or horizontal extension from `source_range`.
    pub fn update_drag(&mut self, hovered_cell: CellRef) {
        let Some(src) = self.source_range else {
            return;
        };
        let row_delta = if hovered_cell.row > src.end.row {
            hovered_cell.row - src.end.row
        } else {
            src.start.row.saturating_sub(hovered_cell.row)
        };
        let col_delta = if hovered_cell.col > src.end.col {
            hovered_cell.col - src.end.col
        } else {
            src.start.col.saturating_sub(hovered_cell.col)
        };

        let target = if row_delta >= col_delta {
            CellRange::new(
                CellRef {
                    row: src.start.row.min(hovered_cell.row),
                    col: src.start.col,
                },
                CellRef {
                    row: src.end.row.max(hovered_cell.row),
                    col: src.end.col,
                },
            )
        } else {
            CellRange::new(
                CellRef {
                    row: src.start.row,
                    col: src.start.col.min(hovered_cell.col),
                },
                CellRef {
                    row: src.end.row,
                    col: src.end.col.max(hovered_cell.col),
                },
            )
        };
        self.target_range = Some(target);
    }

    /// Finishes the drag and returns `(source, target)` if it extended beyond `source`.
    pub fn finish_drag(&mut self) -> Option<(CellRange, CellRange)> {
        self.dragging = false;
        let src = self.source_range.take()?;
        let tgt = self.target_range.take()?;
        if tgt != src {
            Some((src, tgt))
        } else {
            None
        }
    }
}

/// Undoable batch record for an auto-fill operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FillBatchUndo {
    /// Previous cell contents for each modified cell (`None` if cell was empty).
    pub previous: Vec<(CellRef, Option<Cell>)>,
}

/// Applies auto-fill from `source` to `target` on `sheet`, returning an undo record.
pub fn apply_fill_range(sheet: &mut Sheet, source: CellRange, target: CellRange) -> FillBatchUndo {
    let generated = generate_fill_values(source, target, sheet);
    let mut previous = Vec::with_capacity(generated.len());
    for (coord, new_cell) in generated {
        let old = sheet.cells.get(&coord).cloned();
        previous.push((coord, old));
        sheet.cells.insert(coord, new_cell);
    }
    FillBatchUndo { previous }
}

/// Reverts a previously applied `FillBatchUndo` on `sheet`.
#[cfg(test)]
pub fn undo_fill_range(sheet: &mut Sheet, undo: FillBatchUndo) {
    for (coord, old_cell) in undo.previous {
        match old_cell {
            Some(c) => {
                sheet.cells.insert(coord, c);
            }
            None => {
                sheet.cells.remove(&coord);
            }
        }
    }
}

/// Performs double-click fill-down from `source` if an adjacent column has contiguous data.
pub fn apply_double_click_fill(
    sheet: &mut Sheet,
    source: CellRange,
) -> Option<(CellRange, FillBatchUndo)> {
    let target = double_click_fill_extent(source, sheet)?;
    let undo = apply_fill_range(sheet, source, target);
    Some((target, undo))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_handle_drag_and_undo_round_trip() {
        let mut sheet = Sheet::new("Sheet1");
        let cr = |row, col| CellRef { row, col };
        sheet.cells.insert(cr(1, 1), Cell::new("5"));
        sheet.cells.insert(cr(2, 1), Cell::new("10"));

        let mut state = FillHandleState::default();
        assert!(FillHandleState::hit_test_corner(102.0, 203.0, 100.0, 200.0));
        let src = CellRange::new(cr(1, 1), cr(2, 1));
        state.begin_drag(src);
        state.update_drag(cr(4, 1));
        let (s, t) = state.finish_drag().unwrap();

        let undo = apply_fill_range(&mut sheet, s, t);
        assert_eq!(sheet.cells.get(&cr(3, 1)).unwrap().raw, "15");
        assert_eq!(sheet.cells.get(&cr(4, 1)).unwrap().raw, "20");

        undo_fill_range(&mut sheet, undo);
        assert!(sheet.cells.get(&cr(3, 1)).is_none());
        assert!(sheet.cells.get(&cr(4, 1)).is_none());
    }
}
