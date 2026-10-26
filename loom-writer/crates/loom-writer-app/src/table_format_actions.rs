//! Typed table formatting for Loom Writer: bold, italic and underline on the
//! cells of a selection, and column alignment. The formatting is stored on each
//! cell's typed runs and style override, so the page, PDF and DOCX draw the same
//! thing. Each command is one undo step.

use std::ops::Range;

use loom_text::{CharacterStyle, FontWeight};
use loom_writer_core::loom_objects::table::{HorizontalAlign, TextRun};
use loom_writer_core::{TextSelection, WriterDocument, TABLE_CELL_BASE_FONT_SIZE_PT};
use slint::SharedString;

use crate::table_actions::{get_table_cell_selection_range, TableCellRange};
use crate::{apply_with_history, GuiState, HistoryKind, WriterApp};

/// A character emphasis that a table cell's typed runs can carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Emphasis {
    Bold,
    Italic,
    Underline,
    Strikethrough,
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
            Self::Strikethrough => run.strikethrough,
        }
    }

    fn set(self, run: &mut TextRun, on: bool) {
        match self {
            Self::Bold => run.bold = on,
            Self::Italic => run.italic = on,
            Self::Underline => run.underline = on,
            Self::Strikethrough => run.strikethrough = on,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Bold => "Bold",
            Self::Italic => "Italic",
            Self::Underline => "Underline",
            Self::Strikethrough => "Strikethrough",
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

pub(crate) fn can_format_selection(doc: &WriterDocument, selection: TextSelection) -> bool {
    if get_table_cell_selection_range()
        .is_some_and(|range| doc.loom_table_from_block(range.block_id).is_some())
    {
        return true;
    }
    if selection.is_collapsed() {
        doc.table_cell_at_offset(selection.focus).is_some()
    } else {
        let start = selection.anchor.min(selection.focus);
        let end = selection.anchor.max(selection.focus);
        let mut block_start = 0usize;
        doc.blocks.iter().any(|block| {
            let block_end = block_start + block.text.len_bytes();
            let overlaps = block.typed_table().is_some() && start < block_end && end > block_start;
            block_start = block_end + 1;
            overlaps
        })
    }
}

pub(crate) fn selection_is_table_only(doc: &WriterDocument, selection: TextSelection) -> bool {
    if get_table_cell_selection_range()
        .is_some_and(|range| doc.loom_table_from_block(range.block_id).is_some())
    {
        return true;
    }
    if selection.is_collapsed() {
        doc.table_cell_at_offset(selection.focus).is_some()
    } else {
        doc.table_block_id_for_range(
            selection.anchor.min(selection.focus)..selection.anchor.max(selection.focus),
        )
        .is_some()
    }
}

fn mutate_target(
    document: &mut WriterDocument,
    selection: &TextSelection,
    change: impl Fn(&mut TextRun),
) -> bool {
    match emphasis_target(document, selection) {
        Some(Target::Cells {
            block_id,
            rows,
            columns,
        }) => document
            .set_table_cells_runs(block_id, rows, columns, change)
            .is_ok(),
        Some(Target::Text { range }) => {
            document.set_table_text_runs(range, change).unwrap_or(false)
        }
        None if !selection.is_collapsed() => {
            mutate_selected_table_blocks(document, selection, &change)
        }
        None => false,
    }
}

fn mutate_selected_table_blocks(
    document: &mut WriterDocument,
    selection: &TextSelection,
    change: &impl Fn(&mut TextRun),
) -> bool {
    let start = selection.anchor.min(selection.focus);
    let end = selection.anchor.max(selection.focus);
    let mut block_start = 0usize;
    let mut segments = Vec::new();
    for block in &document.blocks {
        let block_end = block_start + block.text.len_bytes();
        if block.typed_table().is_some() {
            let part_start = start.max(block_start);
            let part_end = end.min(block_end);
            if part_start < part_end {
                segments.push(part_start..part_end);
            }
        }
        block_start = block_end + 1;
    }
    segments.into_iter().fold(false, |changed, range| {
        document.set_table_text_runs(range, change).unwrap_or(false) || changed
    })
}

pub(crate) fn set_selection_superscript(
    document: &mut WriterDocument,
    selection: TextSelection,
    enabled: bool,
) -> bool {
    mutate_target(document, &selection, |run| {
        run.superscript = enabled;
        if enabled {
            run.subscript = false;
        }
    })
}

pub(crate) fn set_selection_emphasis(
    document: &mut WriterDocument,
    selection: TextSelection,
    emphasis: Emphasis,
    enabled: bool,
) -> bool {
    mutate_target(document, &selection, |run| emphasis.set(run, enabled))
}

pub(crate) fn set_selection_subscript(
    document: &mut WriterDocument,
    selection: TextSelection,
    enabled: bool,
) -> bool {
    mutate_target(document, &selection, |run| {
        run.subscript = enabled;
        if enabled {
            run.superscript = false;
        }
    })
}

pub(crate) fn set_selection_font_family(
    document: &mut WriterDocument,
    selection: TextSelection,
    family: &str,
) -> bool {
    let family = family.trim();
    mutate_target(document, &selection, |run| {
        run.font_family = (!family.is_empty()).then(|| family.to_owned());
    })
}

pub(crate) fn set_selection_font_size(
    document: &mut WriterDocument,
    selection: TextSelection,
    size_pt: f32,
) -> bool {
    let size_pt = size_pt.clamp(6.0, 72.0);
    mutate_target(document, &selection, |run| {
        run.font_size_pt = Some(size_pt);
    })
}

pub(crate) fn set_selection_foreground_color(
    document: &mut WriterDocument,
    selection: TextSelection,
    color: &str,
) -> bool {
    mutate_target(document, &selection, |run| {
        run.color = Some(color.to_owned());
    })
}

/// Character styles stored on the table runs covered by the active cell range
/// or text selection. A collapsed table caret reports the run under it.
pub(crate) fn selected_character_styles(
    document: &WriterDocument,
    selection: TextSelection,
) -> Vec<CharacterStyle> {
    if let Some(range) = get_table_cell_selection_range() {
        let Some(table) = document.loom_table_from_block(range.block_id) else {
            return Vec::new();
        };
        let mut styles = Vec::new();
        for row in range.start_row.min(range.end_row)..=range.start_row.max(range.end_row) {
            for column in range.start_col.min(range.end_col)..=range.start_col.max(range.end_col) {
                if let Some(cell) = table.cell(row, column).filter(|cell| !cell.is_covered) {
                    styles.extend(cell.content.runs.iter().map(character_style));
                }
            }
        }
        return styles;
    }

    if selection.is_collapsed() {
        let Some(location) = document.table_cell_at_offset(selection.focus) else {
            return Vec::new();
        };
        let Some(table) = document.loom_table_from_block(location.block_id) else {
            return Vec::new();
        };
        let Some(cell) = table.cell(location.row, location.col) else {
            return Vec::new();
        };
        let at = selection.focus.saturating_sub(location.cell_start);
        let mut cursor = 0usize;
        if let Some(run) = cell.content.runs.iter().find(|run| {
            let end = cursor + run.text.len();
            let contains = cursor <= at.saturating_sub(1) && at.saturating_sub(1) < end;
            cursor = end;
            contains
        }) {
            return vec![character_style(run)];
        }
        return cell
            .content
            .runs
            .first()
            .map(character_style)
            .into_iter()
            .collect();
    }

    let start = selection.anchor.min(selection.focus);
    let end = selection.anchor.max(selection.focus);
    let mut styles = Vec::new();
    let mut block_start = 0usize;
    for block in &document.blocks {
        let block_end = block_start + block.text.len_bytes();
        if let Some(table) = block.typed_table() {
            let part_start = start.max(block_start);
            let part_end = end.min(block_end);
            if part_start < part_end {
                let ranges = document.table_cell_ranges(block.id).unwrap_or_default();
                for (row, cells) in ranges.iter().enumerate() {
                    for (column, &(cell_start, cell_end)) in cells.iter().enumerate() {
                        let Some(cell) = table.cell(row, column).filter(|cell| !cell.is_covered)
                        else {
                            continue;
                        };
                        let cell_start = block_start + cell_start;
                        let cell_end = block_start + cell_end;
                        let local_start = part_start.max(cell_start).saturating_sub(cell_start);
                        let local_end = part_end.min(cell_end).saturating_sub(cell_start);
                        if local_start >= local_end {
                            continue;
                        }
                        let mut cursor = 0usize;
                        for run in &cell.content.runs {
                            let run_start = cursor;
                            let run_end = cursor + run.text.len();
                            if run_start < local_end && run_end > local_start {
                                styles.push(character_style(run));
                            }
                            cursor = run_end;
                        }
                    }
                }
            }
        }
        block_start = block_end + 1;
    }
    styles
}

fn character_style(run: &TextRun) -> CharacterStyle {
    let mut style = CharacterStyle::default();
    style.weight = if run.bold {
        FontWeight::Bold
    } else {
        FontWeight::Regular
    };
    style.italic = run.italic;
    style.underline = run.underline;
    style.strikethrough = run.strikethrough;
    style.superscript = run.superscript;
    style.subscript = run.subscript;
    style.font_size = run.font_size_pt.unwrap_or(TABLE_CELL_BASE_FONT_SIZE_PT);
    style.explicit_size = run.font_size_pt.is_some();
    style.font_family = run.font_family.clone().unwrap_or_default();
    style.color = run.color.clone();
    style
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
