//! Format Painter state machine wired to `loom_clipboard::StyleClipboard<CellStyle>`.

use loom_clipboard::StyleClipboard;
use loom_sheets_core::style::CellStyle;
use loom_sheets_core::{CellRange, CellRef, Sheet};

/// Undoable batch record for applying a painted style to a range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatPainterUndo {
    /// Previous styles for each cell in the target range.
    pub previous: Vec<(CellRef, CellStyle)>,
}

/// Wrapper around `loom_clipboard::StyleClipboard<CellStyle>` for Sheets.
#[derive(Debug, Clone, Default)]
pub struct SheetsFormatPainter {
    clipboard: StyleClipboard<CellStyle>,
}

impl SheetsFormatPainter {
    /// Creates an inactive Format Painter.
    pub fn new() -> Self {
        Self {
            clipboard: StyleClipboard::new(),
        }
    }

    /// Captures the style of `cell` for a single application.
    pub fn pick_from_cell(&mut self, sheet: &Sheet, cell: CellRef) {
        let style = sheet.cell_style(cell);
        self.clipboard.pick(style);
    }

    /// Captures the style of `cell` for repeated (locked) applications.
    pub fn pick_locked_from_cell(&mut self, sheet: &Sheet, cell: CellRef) {
        let style = sheet.cell_style(cell);
        self.clipboard.pick_locked(style);
    }

    /// Applies the armed style to every cell in `target`, returning an undo record
    /// if Format Painter was active.
    pub fn apply_to_range(
        &mut self,
        sheet: &mut Sheet,
        target: CellRange,
    ) -> Option<FormatPainterUndo> {
        let style = self.clipboard.apply_take()?;
        let mut previous = Vec::new();
        for row in target.start.row..=target.end.row {
            for col in target.start.col..=target.end.col {
                let coord = CellRef { row, col };
                previous.push((coord, sheet.cell_style(coord)));
                sheet.set_cell_style(coord, style);
            }
        }
        Some(FormatPainterUndo { previous })
    }

    /// Clears the armed style and returns to `Inactive`.
    #[cfg(test)]
    pub fn cancel(&mut self) {
        self.clipboard.cancel();
    }

    /// Whether Format Painter is armed (`SingleShot` or `Locked`).
    pub fn is_active(&self) -> bool {
        self.clipboard.is_active()
    }

    /// Whether Format Painter is in `Locked` mode.
    pub fn is_locked(&self) -> bool {
        self.clipboard.is_locked()
    }
}

/// Reverts a `FormatPainterUndo` on `sheet`.
#[cfg(test)]
pub fn undo_format_painter(sheet: &mut Sheet, undo: FormatPainterUndo) {
    for (coord, prev_style) in undo.previous {
        sheet.set_cell_style(coord, prev_style);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_shot_and_locked_format_painter_with_undo() {
        let mut sheet = Sheet::new("Sheet1");
        let cr = |row, col| CellRef { row, col };
        let mut src_style = CellStyle::new();
        src_style.bold = true;
        src_style.indent = 2;
        sheet.set_cell_style(cr(1, 1), src_style);

        let mut fp = SheetsFormatPainter::new();
        assert!(!fp.is_active());

        // Single-shot
        fp.pick_from_cell(&sheet, cr(1, 1));
        assert!(fp.is_active());
        let target = CellRange::new(cr(2, 1), cr(2, 2));
        let undo = fp.apply_to_range(&mut sheet, target).unwrap();
        assert!(!fp.is_active());
        assert!(sheet.cell_style(cr(2, 1)).bold);
        assert_eq!(sheet.cell_style(cr(2, 2)).indent, 2);

        undo_format_painter(&mut sheet, undo);
        assert!(!sheet.cell_style(cr(2, 1)).bold);

        // Locked
        fp.pick_locked_from_cell(&sheet, cr(1, 1));
        assert!(fp.is_locked());
        assert!(fp.apply_to_range(&mut sheet, target).is_some());
        assert!(fp.is_locked());
        fp.cancel();
        assert!(!fp.is_active());
    }
}
