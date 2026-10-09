//! Sorting rows as a permutation, so an undo entry is not a copy of the sheet.
//!
//! A sort reorders whole rows of a range. [`RowOrder`] records the new order
//! as one `u32` per row. Applying it moves the cells of those rows, and
//! reverting it restores the previous order exactly.

use std::collections::HashMap;

use crate::{compare_values, CellRange, CellRef, Sheet, Value};

/// The rows of one sort range in their new order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowOrder {
    start_row: u32,
    first_col: u32,
    last_col: u32,
    /// `order[d]` is the offset from `start_row` of the row that moves to offset `d`.
    order: Vec<u32>,
}

impl RowOrder {
    /// The order that sorts the rows of `range` by the column at
    /// `range.start.col + relative_column`.
    ///
    /// `values` are the evaluated results that serve as sort keys. Ties keep
    /// their original order, as the sort always has.
    pub fn sorted(
        range: CellRange,
        relative_column: u32,
        ascending: bool,
        values: &HashMap<CellRef, Value>,
    ) -> Result<Self, String> {
        let sort_column = range
            .start
            .col
            .checked_add(relative_column)
            .ok_or_else(|| "sort column overflow".to_string())?;
        if sort_column > range.end.col {
            return Err("sort column is outside the range".into());
        }
        let empty = Value::Empty;
        let key = |row: u32| {
            values
                .get(&CellRef {
                    row,
                    col: sort_column,
                })
                .unwrap_or(&empty)
        };
        let mut rows: Vec<u32> = (range.start.row..=range.end.row).collect();
        rows.sort_by(|left, right| {
            let ordering = compare_values(key(*left), key(*right));
            if ascending {
                ordering
            } else {
                ordering.reverse()
            }
        });
        Ok(Self {
            start_row: range.start.row,
            first_col: range.start.col,
            last_col: range.end.col,
            order: rows.into_iter().map(|row| row - range.start.row).collect(),
        })
    }

    /// Move the cells of the range's rows into the sorted order. Cells outside
    /// the range's columns, and everything else in the sheet, stay where they are.
    pub fn apply(&self, sheet: &mut Sheet) {
        let mut destination = vec![0u32; self.order.len()];
        for (new_offset, old_offset) in self.order.iter().enumerate() {
            destination[*old_offset as usize] = new_offset as u32;
        }
        self.move_cells(sheet, &destination);
    }

    /// Restore the order that was in place before [`Self::apply`].
    pub fn revert(&self, sheet: &mut Sheet) {
        self.move_cells(sheet, &self.order);
    }

    /// Bytes this entry keeps alive.
    pub fn memory_bytes(&self) -> usize {
        std::mem::size_of::<Self>() + self.order.capacity() * std::mem::size_of::<u32>()
    }

    /// Rebuild the cell map with each cell in the range moved to the row given
    /// by `target[offset]`, where `offset` is its current row offset.
    fn move_cells(&self, sheet: &mut Sheet, target: &[u32]) {
        let len = self.order.len() as u32;
        let cells = std::mem::take(&mut sheet.cells);
        sheet.cells = cells
            .into_entries()
            .into_iter()
            .map(|(at, cell)| {
                let inside = at.row >= self.start_row
                    && at.row - self.start_row < len
                    && (self.first_col..=self.last_col).contains(&at.col);
                if inside {
                    let offset = (at.row - self.start_row) as usize;
                    let row = self.start_row + target[offset];
                    (CellRef { row, col: at.col }, cell)
                } else {
                    (at, cell)
                }
            })
            .collect();
    }
}

#[cfg(test)]
#[path = "row_order_tests.rs"]
mod tests;
