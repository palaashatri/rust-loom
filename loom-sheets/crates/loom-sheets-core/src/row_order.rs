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
        let mut keyed: Vec<(u32, &Value)> = (range.start.row..=range.end.row)
            .map(|row| {
                let value = values
                    .get(&CellRef {
                        row,
                        col: sort_column,
                    })
                    .unwrap_or(&empty);
                (row - range.start.row, value)
            })
            .collect();
        keyed.sort_by(|left, right| {
            let ordering = compare_values(left.1, right.1);
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
            order: keyed.into_iter().map(|(offset, _)| offset).collect(),
        })
    }

    /// Move the cells of the range's rows into the sorted order. Cells outside
    /// the range's columns, and everything else in the sheet, stay where they are.
    pub fn apply(&self, sheet: &mut Sheet) {
        sheet
            .cells
            .permute_rows(self.start_row, self.first_col, self.last_col, &self.order);
        self.follow_moved_formulas(sheet, &self.order);
    }

    /// Restore the order that was in place before [`Self::apply`].
    pub fn revert(&self, sheet: &mut Sheet) {
        let mut source_offsets = vec![0u32; self.order.len()];
        for (new_offset, old_offset) in self.order.iter().enumerate() {
            source_offsets[*old_offset as usize] = new_offset as u32;
        }
        sheet.cells.permute_rows(
            self.start_row,
            self.first_col,
            self.last_col,
            &source_offsets,
        );
        self.follow_moved_formulas(sheet, &source_offsets);
    }

    /// A formula that moved with its row keeps pointing at its own row: its
    /// relative row references shift by how far the row moved. (A sort only
    /// moves rows whose formulas refer to nothing but their own row, see
    /// [`Sheet::first_cross_row_formula_row`], so this is exact.)
    fn follow_moved_formulas(&self, sheet: &mut Sheet, source_offsets: &[u32]) {
        let last_row = self.start_row + source_offsets.len().saturating_sub(1) as u32;
        let Some(first_formula_row) = sheet.cells.first_formula_row_from(self.start_row) else {
            return;
        };
        if first_formula_row > last_row {
            return;
        }
        let start_offset = (first_formula_row - self.start_row) as usize;
        for (dest_offset, source_offset) in source_offsets.iter().enumerate().skip(start_offset) {
            let delta = dest_offset as i64 - i64::from(*source_offset);
            if delta == 0 {
                continue;
            }
            let row = self.start_row + dest_offset as u32;
            sheet
                .cells
                .update_row_formulas_in_range(row, self.first_col, self.last_col, |cell| {
                    let shifted = crate::shift_formula_references(&cell.raw, 0, delta as i32);
                    if shifted == cell.raw {
                        false
                    } else {
                        cell.raw = shifted;
                        true
                    }
                });
        }
    }

    /// Bytes this entry keeps alive.
    pub fn memory_bytes(&self) -> usize {
        std::mem::size_of::<Self>() + self.order.capacity() * std::mem::size_of::<u32>()
    }
}

impl Sheet {
    /// The first row at or after `from_row` holding a formula that refers to
    /// anything but its own row (a total, a running sum, a lookup into other
    /// rows). Such a row is where a sort stops, because moving it would change
    /// what it adds up. `None` when every formula is about its own row only.
    pub fn first_cross_row_formula_row(&self, from_row: u32) -> Option<u32> {
        let mut row = from_row;
        loop {
            let found = self.cells.first_formula_row_from(row)?;
            let crosses = self
                .cells
                .row_entries(found)
                .any(|(_, cell)| cell.is_formula() && !is_row_local_formula(&cell.raw, found));
            if crosses {
                return Some(found);
            }
            row = found + 1;
        }
    }
}

/// Whether every cell reference in `formula` (written in the cell at 0-based
/// `row`) is to that same row with a relative row, so the formula stays correct
/// when its row moves and its references shift with it. A formula with no
/// references qualifies; anything qualified by a sheet name does not.
pub fn is_row_local_formula(formula: &str, row: u32) -> bool {
    let bytes = formula.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'"' {
            // A string literal is opaque.
            i += 1;
            while i < bytes.len() {
                if bytes[i] == b'"' {
                    if bytes.get(i + 1) == Some(&b'"') {
                        i += 2;
                        continue;
                    }
                    break;
                }
                i += 1;
            }
            i += 1;
            continue;
        }
        if b == b'!' || b == b'\'' {
            return false;
        }
        if b == b'$' || b.is_ascii_alphabetic() {
            // A token of ASCII letters, `$`, digits, underscores, and dots.
            let start = i;
            while i < bytes.len()
                && (bytes[i].is_ascii_alphanumeric()
                    || bytes[i] == b'$'
                    || bytes[i] == b'_'
                    || bytes[i] == b'.')
            {
                i += 1;
            }
            if bytes.get(i) == Some(&b'(') {
                continue;
            }
            if let Some(token_row) = reference_row(&formula[start..i]) {
                match token_row {
                    RowPart::Relative(r) if r == row => {}
                    _ => return false,
                }
            }
            continue;
        }
        i += 1;
    }
    true
}

enum RowPart {
    Relative(u32),
    Absolute,
}

/// The row of an A1-style reference token (`$B7`, `C12`, `D$3`), or `None`
/// when the token is not a cell reference (a name, a number, a function).
fn reference_row(token: &str) -> Option<RowPart> {
    let bytes = token.as_bytes();
    let mut i = 0;
    if bytes.get(i) == Some(&b'$') {
        i += 1;
    }
    let letters = i;
    while bytes.get(i).is_some_and(u8::is_ascii_alphabetic) {
        i += 1;
    }
    if i == letters || i - letters > 3 {
        return None;
    }
    let absolute = bytes.get(i) == Some(&b'$');
    if absolute {
        i += 1;
    }
    let digits = &token[i..];
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if absolute {
        return Some(RowPart::Absolute);
    }
    let number: u32 = digits.parse().ok()?;
    Some(RowPart::Relative(number.checked_sub(1)?))
}

#[cfg(test)]
#[path = "row_order_tests.rs"]
mod tests;
