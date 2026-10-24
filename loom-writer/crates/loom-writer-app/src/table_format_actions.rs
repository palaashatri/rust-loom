//! Typed table formatting for Loom Writer: bold, italic and underline on the
//! cells of a selection, and column alignment. The formatting is stored on each
//! cell's typed runs and style override, so the page, PDF and DOCX draw the same
//! thing. Each command is one undo step.

use std::ops::Range;

use loom_writer_core::loom_objects::table::{HorizontalAlign, TextRun};
use loom_writer_core::{TextSelection, WriterDocument};
use slint::SharedString;

use crate::table_actions::{get_table_cell_selection_range, TableCellRange};
use crate::{apply_with_history, GuiState, HistoryKind, WriterApp};

/// A character emphasis that a table cell's typed runs can carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Emphasis {
    Bold,
    Italic,
    Underline,
}

impl Emphasis {
    /// The emphasis of a Ctrl+letter shortcut, for either letter case.
    pub(crate) fn from_shortcut(key: &str) -> Option<Self> {
        match key {
            "b" | "B" => Some(Self::Bold),
            "i" | "I" => Some(Self::Italic),
            "u" | "U" => Some(Self::Underline),
            _ => None,
        }
    }

    fn has(self, run: &TextRun) -> bool {
        match self {
            Self::Bold => run.bold,
            Self::Italic => run.italic,
            Self::Underline => run.underline,
        }
    }

    fn set(self, run: &mut TextRun, on: bool) {
        match self {
            Self::Bold => run.bold = on,
            Self::Italic => run.italic = on,
            Self::Underline => run.underline = on,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Bold => "Bold",
            Self::Italic => "Italic",
            Self::Underline => "Underline",
        }
    }
}

/// What an emphasis command acts on, inside one table.
enum Target {
    /// The cells of the active cell range.
    Cells {
        block_id: u64,
        rows: Range<usize>,
        columns: Range<usize>,
    },
    /// Selected text (global editor offsets) that lies inside one table block.
    Text { range: Range<usize> },
}

/// The rows and columns a cell range covers, end exclusive.
fn cell_span(range: TableCellRange) -> (Range<usize>, Range<usize>) {
    let rows = range.start_row.min(range.end_row)..range.start_row.max(range.end_row) + 1;
    let columns = range.start_col.min(range.end_col)..range.start_col.max(range.end_col) + 1;
    (rows, columns)
}

/// The target of an emphasis command: the active cell range when there is one,
/// else a text selection inside one table. `None` for anything else.
fn emphasis_target(doc: &WriterDocument, selection: &TextSelection) -> Option<Target> {
    if let Some(range) = get_table_cell_selection_range() {
        if doc.loom_table_from_block(range.block_id).is_some() {
            let (rows, columns) = cell_span(range);
            return Some(Target::Cells {
                block_id: range.block_id,
                rows,
                columns,
            });
        }
    }
    if selection.is_collapsed() {
        return None;
    }
    let range = selection.anchor.min(selection.focus)..selection.anchor.max(selection.focus);
    doc.table_block_id_for_range(range.clone())
        .map(|_| Target::Text { range })
}

/// Shows a message in the status line and announces it to assistive technology.
fn announce(app: &WriterApp, message: String) {
    app.set_status_right(SharedString::from(message.clone()));
    app.set_selection_announcement(SharedString::from(message));
}

/// Toggles `emphasis` on the typed runs of the table target. It turns off when
/// every character already has it and on otherwise, as one undo step. Returns
/// `false`, changing nothing, when the selection is not inside a table, so the
/// caller keeps its paragraph behaviour.
pub(crate) fn toggle_emphasis(app: &WriterApp, state: &GuiState, emphasis: Emphasis) -> bool {
    let selection = state.current.borrow().selection();
    let Some(target) = emphasis_target(&state.current.borrow(), &selection) else {
        return false;
    };
    let mut next = state.current.borrow().clone();
    let all_have = match &target {
        Target::Cells {
            block_id,
            rows,
            columns,
        } => next.table_cells_have(*block_id, rows.clone(), columns.clone(), |run| {
            emphasis.has(run)
        }),
        Target::Text { range } => next.table_text_has(range.clone(), |run| emphasis.has(run)),
    };
    let on = !all_have.unwrap_or(false);
    let (applied, what) = match &target {
        Target::Cells {
            block_id,
            rows,
            columns,
        } => {
            let count = rows.len() * columns.len();
            let what = if count == 1 {
                "1 cell".to_string()
            } else {
                format!("{count} cells")
            };
            let applied =
                next.set_table_cells_runs(*block_id, rows.clone(), columns.clone(), |run| {
                    emphasis.set(run, on)
                });
            (applied, what)
        }
        Target::Text { range } => {
            let applied = next
                .set_table_text_runs(range.clone(), |run| emphasis.set(run, on))
                .map(|_| ());
            (applied, "the selected table text".to_string())
        }
    };
    match applied {
        Ok(()) => {
            next.set_selection(selection);
            apply_with_history(app, state, next, HistoryKind::DocumentAction);
            let verb = if on { "applied to" } else { "removed from" };
            announce(app, format!("{} {verb} {what}", emphasis.name()));
        }
        Err(err) => announce(app, format!("Formatting failed: {err}")),
    }
    true
}

/// The horizontal alignment of a Text alignment command index: 0 left, 1 centre,
/// 2 right. Justify is not offered: the page cannot justify a line.
fn alignment_for_index(index: i32) -> Option<HorizontalAlign> {
    match index {
        0 => Some(HorizontalAlign::Left),
        1 => Some(HorizontalAlign::Center),
        2 => Some(HorizontalAlign::Right),
        _ => None,
    }
}

fn alignment_label(align: HorizontalAlign) -> &'static str {
    match align {
        HorizontalAlign::Left => "Left",
        HorizontalAlign::Center => "Center",
        HorizontalAlign::Right => "Right",
        HorizontalAlign::Justify => "Justify",
    }
}

/// The table and columns that an alignment command acts on: the active cell
/// range's columns, else the columns from the anchor cell to the focus cell.
fn column_scope(doc: &WriterDocument, selection: &TextSelection) -> Option<(u64, Range<usize>)> {
    if let Some(range) = get_table_cell_selection_range() {
        if doc.loom_table_from_block(range.block_id).is_some() {
            let (_, columns) = cell_span(range);
            return Some((range.block_id, columns));
        }
    }
    let start = doc.table_cell_at_offset(selection.anchor)?;
    let end = doc.table_cell_at_offset(selection.focus)?;
    (start.block_id == end.block_id).then(|| {
        (
            start.block_id,
            start.col.min(end.col)..start.col.max(end.col) + 1,
        )
    })
}

/// Sets the alignment of the columns the caret or selection is in, when they
/// are inside one table. `index` is the Text alignment command's index. Returns
/// `false`, changing nothing, when the selection is not in a table.
pub(crate) fn align_table_columns_for_index(app: &WriterApp, state: &GuiState, index: i32) -> bool {
    let Some(align) = alignment_for_index(index) else {
        return false;
    };
    let selection = state.current.borrow().selection();
    let Some((block_id, columns)) = column_scope(&state.current.borrow(), &selection) else {
        return false;
    };
    let count = columns.len();
    let mut next = state.current.borrow().clone();
    match next.set_table_column_alignment(block_id, columns, align) {
        Ok(()) => {
            next.set_selection(selection);
            apply_with_history(app, state, next, HistoryKind::DocumentAction);
            let noun = if count == 1 { "column" } else { "columns" };
            announce(
                app,
                format!(
                    "{} alignment applied to {count} {noun}",
                    alignment_label(align)
                ),
            );
        }
        Err(err) => announce(app, format!("Alignment failed: {err}")),
    }
    true
}

/// The Text alignment index of a table column action name, such as
/// `align-column-center`.
pub(crate) fn column_alignment_index(action: &str) -> Option<i32> {
    match action {
        "align-column-left" => Some(0),
        "align-column-center" => Some(1),
        "align-column-right" => Some(2),
        _ => None,
    }
}

#[cfg(test)]
#[path = "table_format_actions_tests.rs"]
mod tests;
