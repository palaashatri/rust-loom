//! Memory accounting for undo and redo entries.
//!
//! An entry is charged for the bytes it keeps alive. Cell bands that several
//! entries share are charged once, so a whole-sheet snapshot that changed only
//! its column widths costs almost nothing.

use std::collections::HashSet;

use loom_sheets_core::{CellAlignment, CellRef, CellStyle, RangeEdit, Sheet};

use crate::{SheetTransaction, WorkbookUndoState};

/// Bytes a stack keeps alive, counting each cell band once across all entries.
pub(crate) fn history_bytes(stack: &[SheetTransaction]) -> usize {
    let mut counted = HashSet::new();
    stack
        .iter()
        .map(|tx| transaction_bytes(tx, &mut counted))
        .sum()
}

fn transaction_bytes(tx: &SheetTransaction, counted: &mut HashSet<usize>) -> usize {
    match tx {
        SheetTransaction::Range(edit) => edit.memory_bytes(),
        SheetTransaction::Batch(edits) => edits.iter().map(RangeEdit::memory_bytes).sum(),
        SheetTransaction::Alignment { before, .. } => {
            std::mem::size_of_val(tx)
                + before.capacity() * std::mem::size_of::<(CellRef, CellAlignment)>()
        }
        SheetTransaction::Style { before, after } => {
            std::mem::size_of_val(tx)
                + (before.capacity() + after.capacity())
                    * std::mem::size_of::<(CellRef, CellStyle)>()
        }
        SheetTransaction::Snapshot { before, after } => {
            std::mem::size_of_val(tx)
                + sheet_history_bytes(before, counted)
                + sheet_history_bytes(after, counted)
        }
        SheetTransaction::Rows(order) => order.memory_bytes(),
        SheetTransaction::Workbook { before, after } => {
            std::mem::size_of_val(tx)
                + workbook_state_bytes(before, counted)
                + workbook_state_bytes(after, counted)
        }
    }
}

fn workbook_state_bytes(state: &WorkbookUndoState, counted: &mut HashSet<usize>) -> usize {
    std::mem::size_of_val(state)
        + state
            .sheets
            .iter()
            .map(|sheet| sheet_history_bytes(sheet, counted))
            .sum::<usize>()
}

/// Bytes one sheet keeps alive. Its cell bands are counted through `counted`,
/// so a band that another entry in the same history already holds is not
/// counted again.
fn sheet_history_bytes(sheet: &Sheet, counted: &mut HashSet<usize>) -> usize {
    let maps = unique_band_bytes(sheet.cells.band_costs(), counted)
        + unique_band_bytes(sheet.styles.band_costs(), counted)
        + unique_band_bytes(sheet.alignments.band_costs(), counted);
    let objects = sheet
        .objects
        .iter()
        .map(|object| {
            object.label.capacity()
                + object.path.capacity()
                + object.embedded.as_ref().map_or(0, Vec::capacity)
                + object.asset.as_ref().map_or(0, String::capacity)
        })
        .sum::<usize>();
    std::mem::size_of_val(sheet)
        + sheet.name.capacity()
        + maps
        + objects
        + sheet.col_widths.len() * std::mem::size_of::<(u32, f32)>()
        + sheet.row_heights.len() * std::mem::size_of::<(u32, f32)>()
}

/// The bytes of the bands not yet counted, recording each band as counted.
fn unique_band_bytes(
    bands: impl Iterator<Item = (usize, usize)>,
    counted: &mut HashSet<usize>,
) -> usize {
    bands
        .filter(|(identity, _)| counted.insert(*identity))
        .map(|(_, bytes)| bytes)
        .sum()
}
