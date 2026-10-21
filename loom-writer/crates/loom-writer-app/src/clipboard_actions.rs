//! Multi-format clipboard actions for Writer (W2 interop).

use loom_clipboard::{
    html_to_table_with_report, read_clipboard, tsv_to_table, write_clipboard, ClipboardData,
};
use loom_objects::Table;
use slint::ComponentHandle;
use std::rc::Rc;

use crate::{apply_with_history, GuiState, HistoryKind, WriterApp};

pub(crate) fn wire(app: &WriterApp, state: &Rc<GuiState>) {
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_copy(move || {
            if let Some(app) = app_ref.upgrade() {
                copy_action(&app, &state);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_cut(move || {
            if let Some(app) = app_ref.upgrade() {
                cut_action(&app, &state);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_paste(move || {
            if let Some(app) = app_ref.upgrade() {
                paste_action(&app, &state);
            }
        });
    }
}

pub(crate) fn copy_action(app: &WriterApp, state: &GuiState) {
    let doc = state.current.borrow();
    let selection = doc.selection();
    let caret = selection.focus;

    // A noncollapsed editor selection is explicit text intent, even when its
    // focus is still inside the table that previously held a cell range.
    if selection.anchor != selection.focus {
        let text = doc.selected_text();
        if !text.is_empty() {
            let clip = text_clipboard_data(text);
            if write_clipboard(&clip).is_ok() {
                app.set_status_left("Copied text".into());
            }
            return;
        }
    }

    // 0. If a table cell selection range is active:
    if let Some(range) = crate::table_actions::get_table_cell_selection_range() {
        if let Some((clip, num_rows, num_cols, _)) = table_range_clipboard_data(&doc, range) {
            if write_clipboard(&clip).is_ok() {
                app.set_status_left(slint::SharedString::from(with_rich_note(format!(
                    "Copied table cells ({num_rows}x{num_cols})"
                ))));
                return;
            }
        }
    }

    // 1. If inside a table cell, or table block is focused:
    if let Some(loc) = doc.table_cell_at_offset(caret) {
        if let Some(loom_tbl) = doc.loom_table_from_block(loc.block_id) {
            let clip = ClipboardData::from_table(&loom_tbl);
            if write_clipboard(&clip).is_ok() {
                app.set_status_left(slint::SharedString::from(with_rich_note(format!(
                    "Copied table ({}x{})",
                    loom_tbl.row_count(),
                    loom_tbl.col_count()
                ))));
                return;
            }
        }
    }

    // 2. Regular text copy
    let text = doc.selected_text();
    if !text.is_empty() {
        let clip = text_clipboard_data(text);
        if write_clipboard(&clip).is_ok() {
            app.set_status_left("Copied text".into());
        }
    }
}

fn text_clipboard_data(text: String) -> ClipboardData {
    ClipboardData {
        plain_text: Some(text.clone()),
        tsv: if text.contains('\t') {
            Some(text)
        } else {
            None
        },
        ..Default::default()
    }
}

/// Appends the plain-text-only warning on systems without rich clipboard formats.
fn with_rich_note(status: String) -> String {
    match loom_clipboard::rich_format_note() {
        Some(note) => format!("{status} - {note}"),
        None => status,
    }
}

fn table_range_clipboard_data(
    document: &loom_writer_core::WriterDocument,
    range: crate::table_actions::TableCellRange,
) -> Option<(ClipboardData, usize, usize, usize)> {
    let writer_table = document.table_from_block(range.block_id)?;
    let min_row = range.start_row.min(range.end_row);
    let max_row = range.start_row.max(range.end_row);
    let min_col = range.start_col.min(range.end_col);
    let max_col = range.start_col.max(range.end_col);
    if max_row >= writer_table.rows.len() || max_col >= writer_table.columns() {
        return None;
    }
    let row_count = max_row.checked_sub(min_row)?.checked_add(1)?;
    let col_count = max_col.checked_sub(min_col)?.checked_add(1)?;
    let mut sub_table = Table::new(row_count, col_count);
    for row in 0..row_count {
        for col in 0..col_count {
            let text = writer_table.get(min_row + row, min_col + col)?;
            sub_table.set_cell_text(row, col, text);
        }
    }

    let block_index = document
        .blocks
        .iter()
        .position(|block| block.id == range.block_id)?;
    let cell_start =
        loom_writer_core::compute_cell_ranges(document.blocks.get(block_index)?.text.as_str())
            .get(min_row)?
            .get(min_col)?
            .0;
    let block_start = document
        .blocks
        .iter()
        .take(block_index)
        .map(|block| block.text.len_bytes() + 1)
        .sum::<usize>();

    Some((
        ClipboardData::from_table(&sub_table),
        row_count,
        col_count,
        block_start + cell_start,
    ))
}

pub(crate) fn cut_action(app: &WriterApp, state: &GuiState) {
    let current = state.current.borrow();
    let selection = current.selection();
    if selection.is_collapsed() {
        if let Some(range) = crate::table_actions::get_table_cell_selection_range() {
            if !current
                .table_cell_at_offset(selection.anchor)
                .is_some_and(|cell| cell.block_id == range.block_id)
            {
                drop(current);
                app.set_status_left("Cut failed: selected table range is unavailable".into());
                return;
            }
            let Some((clipboard_data, num_rows, num_cols, top_left_offset)) =
                table_range_clipboard_data(&current, range)
            else {
                drop(current);
                app.set_status_left("Cut failed: selected table range is unavailable".into());
                return;
            };

            let before = current.clone();
            let mut next = before.clone();
            let min_row = range.start_row.min(range.end_row);
            let max_row = range.start_row.max(range.end_row);
            let min_col = range.start_col.min(range.end_col);
            let max_col = range.start_col.max(range.end_col);
            let Some(mut table) = next.table_from_block(range.block_id) else {
                drop(current);
                app.set_status_left("Cut failed: selected table range is unavailable".into());
                return;
            };
            let mut valid_range = true;
            for row in min_row..=max_row {
                for col in min_col..=max_col {
                    valid_range &= table.set(row, col, "");
                }
            }
            if !valid_range {
                drop(current);
                app.set_status_left("Cut failed: selected table range is unavailable".into());
                return;
            }
            let Some(block) = next
                .blocks
                .iter_mut()
                .find(|block| block.id == range.block_id)
            else {
                drop(current);
                app.set_status_left("Cut failed: selected table range is unavailable".into());
                return;
            };
            let updated = table.to_markdown();
            let table_changed = block.text.as_str() != updated.as_str();
            block.text = loom_writer_core::Text::from_str(&updated);
            next.set_selection(loom_writer_core::TextSelection::caret(top_left_offset));
            drop(current);

            if let Err(error) = write_clipboard(&clipboard_data) {
                app.set_status_left(format!("Cut failed: {error}").into());
                return;
            }
            if *state.current.borrow() != before
                || crate::table_actions::get_table_cell_selection_range() != Some(range)
            {
                app.set_status_left(
                    "Copied table cells; cut cancelled because the document changed".into(),
                );
                return;
            }

            crate::table_actions::clear_table_cell_selection_range();
            if table_changed {
                apply_with_history(app, state, next, HistoryKind::DocumentAction);
            } else {
                state
                    .current
                    .borrow_mut()
                    .set_selection(loom_writer_core::TextSelection::caret(top_left_offset));
                crate::apply_state(app, state);
            }
            app.set_status_left(slint::SharedString::from(with_rich_note(format!(
                "Cut table cells ({num_rows}x{num_cols})"
            ))));
            return;
        }
        drop(current);
        copy_action(app, state);
        return;
    }
    if selection_contains_unsupported_object(&current) {
        drop(current);
        app.set_status_left("Cut failed: selected object format is unsupported".into());
        return;
    }

    let text = current.selected_text();
    if text.is_empty() {
        return;
    }
    drop(current);
    let clipboard_data = text_clipboard_data(text);
    if let Err(error) = write_clipboard(&clipboard_data) {
        app.set_status_left(format!("Cut failed: {error}").into());
        return;
    }

    let mut next = state.current.borrow().clone();
    if replace_selection(&mut next, "") {
        apply_with_history(app, state, next, HistoryKind::DocumentAction);
        app.set_status_left("Cut".into());
    }
}

fn selection_contains_unsupported_object(document: &loom_writer_core::WriterDocument) -> bool {
    let selection = document.selection();
    let (start, end) = selection.normalized_range();
    let mut block_start = 0;
    document.blocks.iter().any(|block| {
        let block_end = block_start + block.text.as_str().len();
        let selected = start < block_end && end > block_start;
        let unsupported = block.kind == loom_writer_core::IMAGE_BLOCK_KIND
            || block.kind == loom_writer_core::CHART_BLOCK_KIND;
        block_start = block_end.saturating_add(1);
        selected && unsupported
    })
}

/// Replaces the selected range (or inserts at the caret) with `insert` and puts
/// the caret after it, leaving the rest of the document alone. `false` when the
/// edit did not apply.
fn replace_selection(doc: &mut loom_writer_core::WriterDocument, insert: &str) -> bool {
    let selection = doc.selection();
    let old = doc.editor_text();
    let start = selection.anchor.min(selection.focus).min(old.len());
    let end = selection.anchor.max(selection.focus).min(old.len());
    if !old.is_char_boundary(start) || !old.is_char_boundary(end) {
        return false;
    }
    // Line endings become paragraph breaks and tabs become spaces, so the
    // caret offset below is the one the document will have.
    let insert = insert
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\t', "    ");
    let mut new_text = String::with_capacity(old.len() - (end - start) + insert.len());
    new_text.push_str(&old[..start]);
    new_text.push_str(&insert);
    new_text.push_str(&old[end..]);
    let caret = start + insert.len();
    if doc.replace_editor_text_at(&new_text, Some(caret)).is_err() {
        return false;
    }
    doc.set_selection(loom_writer_core::TextSelection::caret(caret));
    true
}

pub(crate) fn paste_action(app: &WriterApp, state: &GuiState) {
    let Ok(clip) = read_clipboard() else {
        app.set_status_left("Clipboard unavailable".into());
        return;
    };

    let mut next = state.current.borrow().clone();
    let caret = next.selection().focus;

    // Check if clipboard contains a table
    let table_opt = resolve_table_with_note(&clip);

    if let Some((table, note)) = table_opt {
        // Insert table block
        match next.insert_table_object(caret, &table) {
            Ok(_) => {
                apply_with_history(app, state, next, HistoryKind::DocumentAction);
                let mut status =
                    format!("Pasted table ({}x{})", table.row_count(), table.col_count());
                if let Some(note) = note {
                    status.push_str(&format!(" - {note}"));
                }
                app.set_status_left(slint::SharedString::from(status));
            }
            Err(e) => {
                app.set_status_left(slint::SharedString::from(format!("Paste failed: {e}")));
            }
        }
        return;
    }

    // Otherwise paste as plain text
    if let Some(text) = clip.plain_text.or(clip.tsv).filter(|text| !text.is_empty()) {
        if replace_selection(&mut next, &text) {
            apply_with_history(app, state, next, HistoryKind::DocumentAction);
            app.set_status_left("Pasted text".into());
        }
        return;
    }

    // A copy with no text may hold a picture (a screenshot or an image copied
    // from a paint program or browser): it pastes as a picture block.
    crate::picture_paste::paste_picture(app, state);
}

/// Resolves a table from clipboard data following W2 rules:
/// - Private `loom_objects_json` with Table
/// - HTML with `<table>`
/// - TSV / plain text only if it has tabs on >= 2 rows!
///
/// Also returns a sentence for the status line when the safety limits dropped
/// part of an HTML table.
pub(crate) fn resolve_table_with_note(clip: &ClipboardData) -> Option<(Table, Option<String>)> {
    // 1. Private loom objects JSON
    if let Some(json) = &clip.loom_objects_json {
        if let Ok(table) = Table::from_json(json) {
            return Some((table, None));
        }
    }

    // 2. HTML table
    if let Some(html) = &clip.html {
        if html.contains("<table") || html.contains("<TABLE") {
            if let Some((table, report)) = html_to_table_with_report(html) {
                return Some((table, report.message()));
            }
        }
    }

    // 3. TSV / plain text: MUST have tabs on >= 2 rows to become a table!
    if let Some(tsv_or_text) = clip.tsv.as_deref().or(clip.plain_text.as_deref()) {
        let tabbed_rows = tsv_or_text
            .lines()
            .filter(|line| line.contains('\t'))
            .count();
        if tabbed_rows >= 2 {
            return Some((tsv_to_table(tsv_or_text), None));
        }
    }

    None
}

#[cfg(test)]
#[path = "clipboard_actions_tests.rs"]
mod tests;
