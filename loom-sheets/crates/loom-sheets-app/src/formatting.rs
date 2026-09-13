//! Cell and range formatting operations with full undo/redo transaction recording.

use loom_sheets_core::style::FillColor;
use loom_sheets_core::{CellRange, NumberFormat, Sheet};

use crate::{commit_transaction, SheetTransaction};

/// Default font size in px when stepping from theme-following text.
const DEFAULT_FONT_SIZE: u8 = 14;
/// Font size step and bounds in px.
const FONT_STEP: i32 = 1;
const FONT_MIN: i32 = 9;
const FONT_MAX: i32 = 32;

/// Toggle bold on the given range. If all cells are bold, clears bold; otherwise sets bold.
pub(crate) fn toggle_selection_bold(
    sheet: &mut Sheet,
    undo_stack: &mut Vec<SheetTransaction>,
    redo_stack: &mut Vec<SheetTransaction>,
    range: CellRange,
) -> bool {
    let cells = range.cells();
    if cells.is_empty() {
        return false;
    }
    let all_bold = cells.iter().all(|&c| sheet.cell_style(c).bold);
    let target = !all_bold;

    let mut before = Vec::with_capacity(cells.len());
    let mut after = Vec::with_capacity(cells.len());
    for &cell in &cells {
        let prev = sheet.cell_style(cell);
        before.push((cell, prev));
        let mut next = prev;
        next.bold = target;
        after.push((cell, next));
    }
    let tx = SheetTransaction::Style { before, after };
    commit_transaction(sheet, undo_stack, redo_stack, tx)
}

/// Toggle italic on the given range. If all cells are italic, clears italic; otherwise sets italic.
pub(crate) fn toggle_selection_italic(
    sheet: &mut Sheet,
    undo_stack: &mut Vec<SheetTransaction>,
    redo_stack: &mut Vec<SheetTransaction>,
    range: CellRange,
) -> bool {
    let cells = range.cells();
    if cells.is_empty() {
        return false;
    }
    let all_italic = cells.iter().all(|&c| sheet.cell_style(c).italic);
    let target = !all_italic;

    let mut before = Vec::with_capacity(cells.len());
    let mut after = Vec::with_capacity(cells.len());
    for &cell in &cells {
        let prev = sheet.cell_style(cell);
        before.push((cell, prev));
        let mut next = prev;
        next.italic = target;
        after.push((cell, next));
    }
    let tx = SheetTransaction::Style { before, after };
    commit_transaction(sheet, undo_stack, redo_stack, tx)
}

/// Toggle underline on the given range. If all cells are underlined, clears underline; otherwise sets underline.
pub(crate) fn toggle_selection_underline(
    sheet: &mut Sheet,
    undo_stack: &mut Vec<SheetTransaction>,
    redo_stack: &mut Vec<SheetTransaction>,
    range: CellRange,
) -> bool {
    let cells = range.cells();
    if cells.is_empty() {
        return false;
    }
    let all_underline = cells.iter().all(|&c| sheet.cell_style(c).underline);
    let target = !all_underline;

    let mut before = Vec::with_capacity(cells.len());
    let mut after = Vec::with_capacity(cells.len());
    for &cell in &cells {
        let prev = sheet.cell_style(cell);
        before.push((cell, prev));
        let mut next = prev;
        next.underline = target;
        after.push((cell, next));
    }
    let tx = SheetTransaction::Style { before, after };
    commit_transaction(sheet, undo_stack, redo_stack, tx)
}

/// Set the number format (General, Currency, Percentage, Number) across the selection.
pub(crate) fn set_selection_number_format(
    sheet: &mut Sheet,
    undo_stack: &mut Vec<SheetTransaction>,
    redo_stack: &mut Vec<SheetTransaction>,
    range: CellRange,
    format: NumberFormat,
) -> bool {
    let cells = range.cells();
    if cells.is_empty() {
        return false;
    }
    let mut before = Vec::with_capacity(cells.len());
    let mut after = Vec::with_capacity(cells.len());
    let mut any_change = false;
    for &cell in &cells {
        let prev = sheet.cell_style(cell);
        if prev.number_format != format {
            any_change = true;
        }
        before.push((cell, prev));
        let mut next = prev;
        next.number_format = format;
        after.push((cell, next));
    }
    if !any_change {
        return false;
    }
    let tx = SheetTransaction::Style { before, after };
    commit_transaction(sheet, undo_stack, redo_stack, tx)
}

/// Adjust decimal places count across the selection by delta (+1 or -1).
pub(crate) fn set_selection_decimal_places(
    sheet: &mut Sheet,
    undo_stack: &mut Vec<SheetTransaction>,
    redo_stack: &mut Vec<SheetTransaction>,
    range: CellRange,
    delta: i32,
) -> bool {
    let cells = range.cells();
    if cells.is_empty() {
        return false;
    }
    let mut before = Vec::with_capacity(cells.len());
    let mut after = Vec::with_capacity(cells.len());
    let mut any_change = false;
    for &cell in &cells {
        let prev = sheet.cell_style(cell);
        let current = prev.decimal_places.unwrap_or(2) as i32;
        let new_decimals = (current + delta).clamp(0, 10) as u8;
        if prev.decimal_places != Some(new_decimals) {
            any_change = true;
        }
        before.push((cell, prev));
        let mut next = prev;
        next.decimal_places = Some(new_decimals);
        after.push((cell, next));
    }
    if !any_change {
        return false;
    }
    let tx = SheetTransaction::Style { before, after };
    commit_transaction(sheet, undo_stack, redo_stack, tx)
}

/// Toggle all-edges borders on the given range.
pub(crate) fn toggle_selection_borders(
    sheet: &mut Sheet,
    undo_stack: &mut Vec<SheetTransaction>,
    redo_stack: &mut Vec<SheetTransaction>,
    range: CellRange,
) -> bool {
    let cells = range.cells();
    if cells.is_empty() {
        return false;
    }
    let all_bordered = cells.iter().all(|&c| sheet.cell_style(c).border);
    let target = !all_bordered;

    let mut before = Vec::with_capacity(cells.len());
    let mut after = Vec::with_capacity(cells.len());
    for &cell in &cells {
        let prev = sheet.cell_style(cell);
        before.push((cell, prev));
        let mut next = prev;
        next.border = target;
        after.push((cell, next));
    }
    let tx = SheetTransaction::Style { before, after };
    commit_transaction(sheet, undo_stack, redo_stack, tx)
}

/// Cycle the fill swatch on the given range through the palette to none.
pub(crate) fn cycle_selection_fill(
    sheet: &mut Sheet,
    undo_stack: &mut Vec<SheetTransaction>,
    redo_stack: &mut Vec<SheetTransaction>,
    range: CellRange,
) -> bool {
    let cells = range.cells();
    if cells.is_empty() {
        return false;
    }
    let first = sheet.cell_style(cells[0]).fill;
    let target = if cells.iter().all(|&c| sheet.cell_style(c).fill == first) {
        first.cycle()
    } else {
        first
    };

    let mut before = Vec::with_capacity(cells.len());
    let mut after = Vec::with_capacity(cells.len());
    for &cell in &cells {
        let prev = sheet.cell_style(cell);
        before.push((cell, prev));
        let mut next = prev;
        next.fill = target;
        after.push((cell, next));
    }
    let tx = SheetTransaction::Style { before, after };
    commit_transaction(sheet, undo_stack, redo_stack, tx)
}

/// Set the fill swatch on the given range (inspector swatches).
pub(crate) fn set_selection_fill(
    sheet: &mut Sheet,
    undo_stack: &mut Vec<SheetTransaction>,
    redo_stack: &mut Vec<SheetTransaction>,
    range: CellRange,
    fill: FillColor,
) -> bool {
    let cells = range.cells();
    if cells.is_empty() {
        return false;
    }
    let mut before = Vec::with_capacity(cells.len());
    let mut after = Vec::with_capacity(cells.len());
    let mut any_change = false;
    for &cell in &cells {
        let prev = sheet.cell_style(cell);
        if prev.fill != fill {
            any_change = true;
        }
        before.push((cell, prev));
        let mut next = prev;
        next.fill = fill;
        after.push((cell, next));
    }
    if !any_change {
        return false;
    }
    let tx = SheetTransaction::Style { before, after };
    commit_transaction(sheet, undo_stack, redo_stack, tx)
}
/// Step the font size across the selection by `delta` px from the theme size.
pub(crate) fn adjust_selection_font_size(
    sheet: &mut Sheet,
    undo_stack: &mut Vec<SheetTransaction>,
    redo_stack: &mut Vec<SheetTransaction>,
    range: CellRange,
    delta: i32,
) -> bool {
    let cells = range.cells();
    if cells.is_empty() {
        return false;
    }
    let mut before = Vec::with_capacity(cells.len());
    let mut after = Vec::with_capacity(cells.len());
    let mut any_change = false;
    for &cell in &cells {
        let prev = sheet.cell_style(cell);
        let current = prev.font_size.unwrap_or(DEFAULT_FONT_SIZE) as i32;
        let stepped = if delta == 0 {
            DEFAULT_FONT_SIZE
        } else {
            (current + delta * FONT_STEP).clamp(FONT_MIN, FONT_MAX) as u8
        };
        let new_size = if stepped == DEFAULT_FONT_SIZE && prev.font_size.is_some() {
            // Stepping back onto the theme size clears the override so the
            // cell follows future theme changes again.
            None
        } else {
            Some(stepped)
        };
        if prev.font_size != new_size {
            any_change = true;
        }
        before.push((cell, prev));
        let mut next = prev;
        next.font_size = new_size;
        after.push((cell, next));
    }
    if !any_change {
        return false;
    }
    let tx = SheetTransaction::Style { before, after };
    commit_transaction(sheet, undo_stack, redo_stack, tx)
}

/// Resolve the effective font size in px for display.
pub(crate) fn effective_font_size(style: loom_sheets_core::style::CellStyle) -> u8 {
    style.font_size.unwrap_or(DEFAULT_FONT_SIZE)
}
