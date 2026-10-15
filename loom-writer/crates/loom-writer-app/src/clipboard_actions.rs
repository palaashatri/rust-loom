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

    // 0. If a table cell selection range is active:
    if let Some(range) = crate::table_actions::get_table_cell_selection_range() {
        if let Some(writer_table) = doc.table_from_block(range.block_id) {
            let min_r = range.start_row.min(range.end_row);
            let max_r = range.start_row.max(range.end_row);
            let min_c = range.start_col.min(range.end_col);
            let max_c = range.start_col.max(range.end_col);
            let num_rows = max_r - min_r + 1;
            let num_cols = max_c - min_c + 1;
            let mut sub_table = Table::new(num_rows, num_cols);
            for r in 0..num_rows {
                for c in 0..num_cols {
                    let text = writer_table.get(min_r + r, min_c + c).unwrap_or("");
                    sub_table.set_cell_text(r, c, text);
                }
            }
            let clip = ClipboardData::from_table(&sub_table);
            if write_clipboard(&clip).is_ok() {
                app.set_status_left(slint::SharedString::from(format!(
                    "Copied table cells ({num_rows}x{num_cols})"
                )));
                return;
            }
        }
    }

    // 1. If inside a table cell, or table block is focused:
    if let Some(loc) = doc.table_cell_at_offset(caret) {
        if let Some(writer_table) = doc.table_from_block(loc.block_id) {
            let loom_tbl = writer_table.to_loom_table();
            let clip = ClipboardData::from_table(&loom_tbl);
            if write_clipboard(&clip).is_ok() {
                app.set_status_left(slint::SharedString::from(format!(
                    "Copied table ({}x{})",
                    loom_tbl.row_count(),
                    loom_tbl.col_count()
                )));
                return;
            }
        }
    }

    // 2. Regular text copy
    let text = doc.selected_text();
    if !text.is_empty() {
        let clip = ClipboardData {
            plain_text: Some(text.clone()),
            tsv: if text.contains('\t') {
                Some(text)
            } else {
                None
            },
            ..Default::default()
        };
        if write_clipboard(&clip).is_ok() {
            app.set_status_left("Copied text".into());
        }
    }
}

pub(crate) fn cut_action(app: &WriterApp, state: &GuiState) {
    copy_action(app, state);
    let mut next = state.current.borrow().clone();
    let selection = next.selection();
    let start_offset = selection.anchor.min(selection.focus);
    if !selection.is_collapsed() && next.replace_editor_text_at("", Some(start_offset)).is_ok() {
        apply_with_history(app, state, next, HistoryKind::DocumentAction);
        app.set_status_left("Cut".into());
    }
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
    if let Some(text) = clip.plain_text.or(clip.tsv) {
        if !text.is_empty() && next.replace_editor_text_at(&text, Some(caret)).is_ok() {
            apply_with_history(app, state, next, HistoryKind::DocumentAction);
            app.set_status_left("Pasted text".into());
        }
    }
}

/// Resolves a table from clipboard data following W2 rules:
/// - Private `loom_objects_json` with Table
/// - HTML with `<table>`
/// - TSV / plain text only if it has tabs on >= 2 rows!
pub(crate) fn resolve_table_from_clipboard(clip: &ClipboardData) -> Option<Table> {
    resolve_table_with_note(clip).map(|(table, _)| table)
}

/// Like [`resolve_table_from_clipboard`], plus a sentence for the status line
/// when the safety limits dropped part of an HTML table.
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
mod tests {
    use super::*;

    #[test]
    fn test_tsv_with_tabs_on_ge_2_rows_becomes_table() {
        let tsv_two_rows = "A\tB\nC\tD";
        let clip = ClipboardData {
            plain_text: Some(tsv_two_rows.into()),
            tsv: Some(tsv_two_rows.into()),
            ..Default::default()
        };
        let table = resolve_table_from_clipboard(&clip).expect("2 tab rows must become table");
        assert_eq!(table.row_count(), 2);
        assert_eq!(table.col_count(), 2);
    }

    #[test]
    fn test_tsv_with_tabs_on_single_row_does_not_become_table() {
        let tsv_one_row = "Single\tTab\nJust text";
        let clip = ClipboardData {
            plain_text: Some(tsv_one_row.into()),
            tsv: Some(tsv_one_row.into()),
            ..Default::default()
        };
        assert!(
            resolve_table_from_clipboard(&clip).is_none(),
            "1 tab row must NOT become table"
        );
    }

    #[test]
    fn test_html_table_over_the_row_limit_says_rows_were_dropped() {
        let mut html = String::from("<table>");
        for r in 0..10_030 {
            html.push_str(&format!("<tr><td>R{r}</td></tr>"));
        }
        html.push_str("</table>");
        let clip = ClipboardData {
            html: Some(html),
            ..Default::default()
        };
        let (table, note) = resolve_table_with_note(&clip).expect("table");
        assert_eq!(table.row_count(), 10_000);
        let note = note.expect("a capped paste must explain itself");
        assert!(note.contains("first 10000 rows"), "{note}");
    }

    #[test]
    fn test_html_table_resolves_to_table() {
        let html = "<table><tr><td>Header</td></tr><tr><td>Value</td></tr></table>";
        let clip = ClipboardData {
            html: Some(html.into()),
            plain_text: Some("Header\nValue".into()),
            ..Default::default()
        };
        let table = resolve_table_from_clipboard(&clip).expect("HTML table must resolve");
        assert_eq!(table.row_count(), 2);
    }

    #[test]
    fn test_writer_paste_of_sheets_table_inserts_table_block() {
        use loom_clipboard::set_mock_clipboard_enabled;
        set_mock_clipboard_enabled(true);
        loom_test_support::capture::set_platform();
        let dialogs = std::rc::Rc::new(loom_desktop::ScriptedFileDialogs::new([], [None]));
        let mut doc = loom_writer_core::WriterDocument::new("test", "Test");
        doc.replace_paragraphs("Intro");
        let (app, state) = crate::actions_tests::test_state(doc, dialogs);
        crate::wire_writer_shared_callbacks(&app, &state, None);

        let mut source_table = Table::new(2, 3);
        source_table.set_cell_text(0, 0, "Product");
        source_table.set_cell_text(0, 1, "Q1");
        source_table.set_cell_text(0, 2, "Q2");
        source_table.set_cell_text(1, 0, "Widget");
        source_table.set_cell_text(1, 1, "100");
        source_table.set_cell_text(1, 2, "150");

        let clip = ClipboardData::from_table(&source_table);
        write_clipboard(&clip).expect("write mock clipboard");

        app.invoke_paste();

        let doc = state.current.borrow();
        let table_blocks: Vec<_> = doc
            .blocks
            .iter()
            .filter(|b| b.kind == loom_writer_core::TABLE_BLOCK_KIND)
            .collect();
        assert_eq!(table_blocks.len(), 1, "must insert a table block");
        let parsed = doc
            .table_from_block(table_blocks[0].id)
            .expect("must parse table");
        assert_eq!(parsed.rows.len(), 2);
        assert_eq!(parsed.rows[0][0], "Product");
        assert_eq!(parsed.rows[1][1], "100");

        set_mock_clipboard_enabled(false);
    }

    #[test]
    fn test_writer_copy_table_writes_multiformat_clipboard_for_sheets() {
        use loom_clipboard::set_mock_clipboard_enabled;
        set_mock_clipboard_enabled(true);

        loom_test_support::capture::set_platform();
        let dialogs = std::rc::Rc::new(loom_desktop::ScriptedFileDialogs::new([], [None]));
        let mut doc = loom_writer_core::WriterDocument::new("test", "Test");
        doc.replace_paragraphs("Intro");
        let (app, state) = crate::actions_tests::test_state(doc, dialogs);
        crate::wire_writer_shared_callbacks(&app, &state, None);

        app.invoke_insert_table_size(2, 2);
        app.invoke_copy();

        let clip = read_clipboard().expect("read clipboard");
        assert!(
            clip.loom_objects_json.is_some(),
            "must write private format"
        );
        assert!(clip.html.is_some(), "must write HTML");
        assert!(clip.tsv.is_some(), "must write TSV");

        let sheets_table = clip.to_table().expect("sheets can parse back");
        assert_eq!(sheets_table.row_count(), 2);
        assert_eq!(sheets_table.col_count(), 2);

        set_mock_clipboard_enabled(false);
    }
}
