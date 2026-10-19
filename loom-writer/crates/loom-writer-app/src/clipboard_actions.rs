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
            return;
        }
    }

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
                app.set_status_left(slint::SharedString::from(with_rich_note(format!(
                    "Copied table cells ({num_rows}x{num_cols})"
                ))));
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

/// Appends the plain-text-only warning on systems without rich clipboard formats.
fn with_rich_note(status: String) -> String {
    match loom_clipboard::rich_format_note() {
        Some(note) => format!("{status} - {note}"),
        None => status,
    }
}

pub(crate) fn cut_action(app: &WriterApp, state: &GuiState) {
    copy_action(app, state);
    let mut next = state.current.borrow().clone();
    let selection = next.selection();
    if !selection.is_collapsed() && replace_selection(&mut next, "") {
        apply_with_history(app, state, next, HistoryKind::DocumentAction);
        app.set_status_left("Cut".into());
    }
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
    if let Some(text) = clip.plain_text.or(clip.tsv) {
        if !text.is_empty() && replace_selection(&mut next, &text) {
            apply_with_history(app, state, next, HistoryKind::DocumentAction);
            app.set_status_left("Pasted text".into());
        }
    }
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
mod tests {
    use super::*;

    fn populated_document_with_table(
        document_id: &str,
        title: &str,
        rows: usize,
        columns: usize,
        body_text: &str,
        cell_prefix: &str,
    ) -> (loom_writer_core::WriterDocument, u64, usize, usize) {
        let mut document = loom_writer_core::WriterDocument::new(document_id, title);
        document.replace_paragraphs(&format!("Intro\n{body_text}"));
        let table_id = document.insert_table_block(0, rows, columns).unwrap();
        let mut table = document.table_from_block(table_id).unwrap();
        for row in 0..rows {
            for column in 0..columns {
                table.set(row, column, format!("{cell_prefix}-{row}-{column}"));
            }
        }
        document
            .blocks
            .iter_mut()
            .find(|block| block.id == table_id)
            .unwrap()
            .text = loom_writer_core::Text::from_str(&table.to_markdown());

        let editor_text = document.editor_text();
        let first_cell_offset = (0..=editor_text.len())
            .find(|offset| document.table_cell_at_offset(*offset).is_some())
            .expect("first table cell offset");
        let body_end = editor_text
            .find(body_text)
            .map(|start| start + body_text.len())
            .expect("body paragraph follows the table");
        document.set_selection(loom_writer_core::TextSelection::caret(first_cell_offset));
        (document, table_id, first_cell_offset, body_end)
    }

    fn table_cell_offset(
        document: &loom_writer_core::WriterDocument,
        row: usize,
        column: usize,
    ) -> usize {
        let text = document.editor_text();
        (0..=text.len())
            .find(|offset| {
                document
                    .table_cell_at_offset(*offset)
                    .is_some_and(|location| location.row == row && location.col == column)
            })
            .unwrap_or_else(|| panic!("table cell ({row}, {column}) has an editor offset"))
    }

    fn select_top_left_two_by_two(
        app: &WriterApp,
        state: &GuiState,
        table_id: u64,
        table_offset: usize,
    ) {
        app.invoke_selection_changed(table_offset as i32, table_offset as i32);
        let right: slint::SharedString = slint::platform::Key::RightArrow.into();
        let down: slint::SharedString = slint::platform::Key::DownArrow.into();
        assert!(crate::table_actions::handle_table_key(
            app,
            state,
            right.as_str(),
            true,
            false,
            false
        ));
        assert!(crate::table_actions::handle_table_key(
            app,
            state,
            down.as_str(),
            true,
            false,
            false
        ));
        assert_eq!(
            crate::table_actions::get_table_cell_selection_range(),
            Some(crate::table_actions::TableCellRange {
                block_id: table_id,
                start_row: 0,
                start_col: 0,
                end_row: 1,
                end_col: 1,
            }),
            "the intended 2x2 cell range is established before the pointer event"
        );
    }

    fn pointer_position_for_offset(
        document: &loom_writer_core::WriterDocument,
        target_offset: usize,
    ) -> (f32, f32) {
        let style = document.page.page_style();
        let flow = document
            .flow(
                &style,
                crate::PageViewport {
                    width: style.width_pt,
                    height: style.height_pt,
                    zoom: 1.0,
                    scroll_x: 0.0,
                    scroll_y: 0.0,
                },
            )
            .expect("layout table for pointer hit testing");
        let base_page = flow.page_bounds().first().copied().expect("first page");
        let line = flow
            .lines()
            .iter()
            .find(|line| {
                let end = line.global_start + (line.end - line.start);
                line.global_start <= target_offset && target_offset <= end
            })
            .expect("target cell is on a rendered line");
        let block = &document.blocks[line.block_index];
        let target_local_offset = line.start + target_offset - line.global_start;
        let content_width =
            (style.width_pt - style.margin_left_pt - style.margin_right_pt).max(1.0);
        let alignment_offset = match block.style.alignment {
            loom_text::Alignment::Center => ((content_width - line.bounds.width) / 2.0).max(0.0),
            loom_text::Alignment::Right => (content_width - line.bounds.width).max(0.0),
            loom_text::Alignment::Left | loom_text::Alignment::Justify => 0.0,
        };
        let line_x = line.bounds.x - base_page.x - style.margin_left_pt + alignment_offset;
        let line_y = line.bounds.y - base_page.y - style.margin_top_pt + line.bounds.height / 2.0;
        let line_text = &block.text.as_str()[line.start..line.end];
        let font_size = style.font_size_for_kind(block.kind.as_str());
        let local_x = (0..=(line.bounds.width.max(0.0) * 4.0).ceil() as usize)
            .map(|step| step as f32 / 4.0)
            .find(|x| {
                loom_writer_core::offset_at_x(line_text, line.start, &block.runs, font_size, *x)
                    == target_local_offset
            })
            .expect("target cell offset has a pointer position");
        let point = (line_x + local_x, line_y);
        assert_eq!(
            crate::writer_pointer_offset(
                document,
                crate::PageViewport::default(),
                point.0,
                point.1
            ),
            Some(target_offset),
            "the test pointer position resolves to the intended cell offset"
        );
        point
    }

    fn pointer_position_for_cell(
        document: &loom_writer_core::WriterDocument,
        table_id: u64,
        target_row: usize,
        target_column: usize,
    ) -> ((f32, f32), usize) {
        let target_offset = table_cell_offset(document, target_row, target_column);
        let target_location = document
            .table_cell_at_offset(target_offset)
            .expect("target cell has an editor offset");
        assert_eq!(
            (
                target_location.block_id,
                target_location.row,
                target_location.col
            ),
            (table_id, target_row, target_column),
            "the target offset belongs to the intended table cell"
        );

        let style = document.page.page_style();
        let flow = document
            .flow(
                &style,
                crate::PageViewport {
                    width: style.width_pt,
                    height: style.height_pt,
                    zoom: 1.0,
                    scroll_x: 0.0,
                    scroll_y: 0.0,
                },
            )
            .expect("layout table for pointer hit testing");
        let base_page = flow.page_bounds().first().copied().expect("first page");
        let content_width =
            (style.width_pt - style.margin_left_pt - style.margin_right_pt).max(1.0);

        for line in flow.lines() {
            let block = &document.blocks[line.block_index];
            if block.id != table_id {
                continue;
            }
            let alignment_offset = match block.style.alignment {
                loom_text::Alignment::Center => {
                    ((content_width - line.bounds.width) / 2.0).max(0.0)
                }
                loom_text::Alignment::Right => (content_width - line.bounds.width).max(0.0),
                loom_text::Alignment::Left | loom_text::Alignment::Justify => 0.0,
            };
            let line_x = line.bounds.x - base_page.x - style.margin_left_pt + alignment_offset;
            let line_y =
                line.bounds.y - base_page.y - style.margin_top_pt + line.bounds.height / 2.0;
            let line_text = &block.text.as_str()[line.start..line.end];
            let font_size = style.font_size_for_kind(block.kind.as_str());
            let max_step = (line.bounds.width.max(0.0) * 4.0).ceil() as usize;
            for step in 0..=max_step {
                let local_x = step as f32 / 4.0;
                let local_byte = loom_writer_core::offset_at_x(
                    line_text,
                    line.start,
                    &block.runs,
                    font_size,
                    local_x,
                );
                let candidate = line.global_start + local_byte;
                let Some(location) = document.table_cell_at_offset(candidate) else {
                    continue;
                };
                if (location.block_id, location.row, location.col)
                    != (table_id, target_row, target_column)
                {
                    continue;
                }
                let point = (line_x + local_x, line_y);
                if let Some(actual_offset) = crate::writer_pointer_offset(
                    document,
                    crate::PageViewport::default(),
                    point.0,
                    point.1,
                ) {
                    if document
                        .table_cell_at_offset(actual_offset)
                        .is_some_and(|actual| {
                            (actual.block_id, actual.row, actual.col)
                                == (table_id, target_row, target_column)
                        })
                    {
                        return (point, actual_offset);
                    }
                }
            }
        }

        panic!(
            "the Writer pointer hit-test has a real position inside table cell ({target_row}, {target_column})"
        );
    }

    #[test]
    fn test_tsv_with_tabs_on_ge_2_rows_becomes_table() {
        let tsv_two_rows = "A\tB\nC\tD";
        let clip = ClipboardData {
            plain_text: Some(tsv_two_rows.into()),
            tsv: Some(tsv_two_rows.into()),
            ..Default::default()
        };
        let table = resolve_table_with_note(&clip)
            .map(|(table, _)| table)
            .expect("2 tab rows must become table");
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
            resolve_table_with_note(&clip)
                .map(|(table, _)| table)
                .is_none(),
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
        let table = resolve_table_with_note(&clip)
            .map(|(table, _)| table)
            .expect("HTML table must resolve");
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

    #[test]
    fn copying_body_text_after_leaving_selected_table_cells_copies_only_body_text() {
        loom_clipboard::set_mock_clipboard_enabled(true);
        crate::table_actions::clear_table_cell_selection_range();

        let body_text = "Selected body text";
        let mut document = loom_writer_core::WriterDocument::new("copy-body", "Copy Body");
        document.replace_paragraphs(body_text);
        document.insert_table_block(usize::MAX, 2, 2).unwrap();
        let table_block_id = document
            .blocks
            .iter()
            .find(|block| block.kind == loom_writer_core::TABLE_BLOCK_KIND)
            .expect("table block")
            .id;
        let mut table = document
            .table_from_block(table_block_id)
            .expect("inserted table parses");
        table.set(0, 0, "Cell A1");
        table.set(0, 1, "Cell B1");
        table.set(1, 0, "Cell A2");
        table.set(1, 1, "Cell B2");
        document
            .blocks
            .iter_mut()
            .find(|block| block.id == table_block_id)
            .expect("table block remains present")
            .text = loom_writer_core::Text::from_str(&table.to_markdown());
        let table_offset = (0..=document.editor_text().len())
            .find(|offset| document.table_cell_at_offset(*offset).is_some())
            .expect("offset inside inserted table");
        document.set_selection(loom_writer_core::TextSelection::caret(table_offset));

        let dialogs = std::rc::Rc::new(loom_desktop::ScriptedFileDialogs::new([], [None]));
        let (app, state) = crate::actions_tests::test_state(document, dialogs);
        crate::wire_writer_shared_callbacks(&app, &state, None);
        crate::apply_state(&app, &state);

        app.invoke_selection_changed(table_offset as i32, table_offset as i32);
        let right: slint::SharedString = slint::platform::Key::RightArrow.into();
        let down: slint::SharedString = slint::platform::Key::DownArrow.into();
        assert!(crate::table_actions::handle_table_key(
            &app,
            &state,
            right.as_str(),
            true,
            false,
            false
        ));
        assert!(crate::table_actions::handle_table_key(
            &app,
            &state,
            down.as_str(),
            true,
            false,
            false
        ));
        assert_eq!(
            crate::table_actions::get_table_cell_selection_range(),
            Some(crate::table_actions::TableCellRange {
                block_id: table_block_id,
                start_row: 0,
                start_col: 0,
                end_row: 1,
                end_col: 1,
            }),
            "the intended 2x2 table range exists before moving to body text"
        );

        app.invoke_selection_changed(0, body_text.len() as i32);
        assert_eq!(state.current.borrow().selected_text(), body_text);
        app.invoke_copy();

        let clipboard = read_clipboard().expect("read mock clipboard");
        let remaining_table_range = crate::table_actions::get_table_cell_selection_range();
        crate::table_actions::clear_table_cell_selection_range();
        assert_eq!(
            clipboard.plain_text.as_deref(),
            Some(body_text),
            "Copy follows the selected body text after leaving the table"
        );
        assert_eq!(
            remaining_table_range, None,
            "leaving the table clears its active cell range"
        );

        loom_clipboard::set_mock_clipboard_enabled(false);
    }

    #[test]
    fn stale_table_range_does_not_override_text_selection_extended_from_table_into_body() {
        loom_clipboard::set_mock_clipboard_enabled(true);
        crate::table_actions::clear_table_cell_selection_range();

        let body_text = "Body after table";
        let (document, table_id, table_offset, body_end) = populated_document_with_table(
            "copy-table-to-body",
            "Copy Table To Body",
            2,
            2,
            body_text,
            "cell",
        );
        let dialogs = std::rc::Rc::new(loom_desktop::ScriptedFileDialogs::new([], [None]));
        let (app, state) = crate::actions_tests::test_state(document, dialogs);
        crate::wire_writer_shared_callbacks(&app, &state, None);
        crate::apply_state(&app, &state);

        app.invoke_selection_changed(table_offset as i32, table_offset as i32);
        let right: slint::SharedString = slint::platform::Key::RightArrow.into();
        let down: slint::SharedString = slint::platform::Key::DownArrow.into();
        assert!(crate::table_actions::handle_table_key(
            &app,
            &state,
            right.as_str(),
            true,
            false,
            false
        ));
        assert!(crate::table_actions::handle_table_key(
            &app,
            &state,
            down.as_str(),
            true,
            false,
            false
        ));

        assert_eq!(
            crate::table_actions::get_table_cell_selection_range(),
            Some(crate::table_actions::TableCellRange {
                block_id: table_id,
                start_row: 0,
                start_col: 0,
                end_row: 1,
                end_col: 1,
            }),
            "the intended 2x2 table range exists before extending into body text"
        );
        app.invoke_selection_changed(table_offset as i32, body_end as i32);
        let selected_text = state.current.borrow().selected_text();
        assert!(
            selected_text.contains(body_text),
            "selection: {selected_text:?}"
        );
        app.invoke_copy();

        let clipboard = read_clipboard().expect("read mock clipboard");
        let remaining_range = crate::table_actions::get_table_cell_selection_range();
        crate::table_actions::clear_table_cell_selection_range();
        assert_eq!(
            clipboard.plain_text.as_deref(),
            Some(selected_text.as_str()),
            "Copy follows the explicit text selection even when its anchor is in a table"
        );
        assert_eq!(
            remaining_range, None,
            "extending the selection out of the table clears the active cell range"
        );

        loom_clipboard::set_mock_clipboard_enabled(false);
    }

    #[test]
    fn explicit_text_selection_ending_inside_table_is_copied_as_text() {
        loom_clipboard::set_mock_clipboard_enabled(true);
        crate::table_actions::clear_table_cell_selection_range();

        let body_text = "Body before table";
        let mut document =
            loom_writer_core::WriterDocument::new("copy-body-to-table", "Copy Body To Table");
        document.replace_paragraphs(body_text);
        let table_id = document.insert_table_block(usize::MAX, 2, 2).unwrap();
        let mut table = document.table_from_block(table_id).unwrap();
        table.set(0, 0, "table-0-0");
        table.set(0, 1, "table-0-1");
        table.set(1, 0, "table-1-0");
        table.set(1, 1, "table-1-1");
        document
            .blocks
            .iter_mut()
            .find(|block| block.id == table_id)
            .unwrap()
            .text = loom_writer_core::Text::from_str(&table.to_markdown());
        let table_offset = (0..=document.editor_text().len())
            .find(|offset| document.table_cell_at_offset(*offset).is_some())
            .expect("first table cell offset");
        document.set_selection(loom_writer_core::TextSelection::caret(table_offset));

        let dialogs = std::rc::Rc::new(loom_desktop::ScriptedFileDialogs::new([], [None]));
        let (app, state) = crate::actions_tests::test_state(document, dialogs);
        crate::wire_writer_shared_callbacks(&app, &state, None);
        crate::apply_state(&app, &state);

        app.invoke_selection_changed(table_offset as i32, table_offset as i32);
        let right: slint::SharedString = slint::platform::Key::RightArrow.into();
        let down: slint::SharedString = slint::platform::Key::DownArrow.into();
        assert!(crate::table_actions::handle_table_key(
            &app,
            &state,
            right.as_str(),
            true,
            false,
            false
        ));
        assert!(crate::table_actions::handle_table_key(
            &app,
            &state,
            down.as_str(),
            true,
            false,
            false
        ));

        assert_eq!(
            crate::table_actions::get_table_cell_selection_range(),
            Some(crate::table_actions::TableCellRange {
                block_id: table_id,
                start_row: 0,
                start_col: 0,
                end_row: 1,
                end_col: 1,
            }),
            "the intended 2x2 table range exists before selecting text into the table"
        );
        app.invoke_selection_changed(0, table_offset as i32);
        let selected_text = state.current.borrow().selected_text();
        assert!(
            selected_text.contains(body_text),
            "selection: {selected_text:?}"
        );
        app.invoke_copy();

        let clipboard = read_clipboard().expect("read mock clipboard");
        let remaining_range = crate::table_actions::get_table_cell_selection_range();
        crate::table_actions::clear_table_cell_selection_range();
        assert_eq!(
            clipboard.plain_text.as_deref(),
            Some(selected_text.as_str()),
            "Copy follows selected text when focus ends inside a table"
        );
        assert_eq!(
            remaining_range, None,
            "an explicit text selection clears the active table cell range"
        );

        loom_clipboard::set_mock_clipboard_enabled(false);
    }

    #[test]
    fn stale_table_range_is_cleared_when_replacing_document_with_same_table_id() {
        loom_clipboard::set_mock_clipboard_enabled(true);
        crate::table_actions::clear_table_cell_selection_range();

        let (document, old_table_id, old_table_offset, _) = populated_document_with_table(
            "old-table-document",
            "Old Table Document",
            2,
            2,
            "Old body",
            "old",
        );
        let dialogs = std::rc::Rc::new(loom_desktop::ScriptedFileDialogs::new([], [None]));
        let (app, state) = crate::actions_tests::test_state(document, dialogs);
        crate::wire_writer_shared_callbacks(&app, &state, None);
        crate::apply_state(&app, &state);

        app.invoke_selection_changed(old_table_offset as i32, old_table_offset as i32);
        let right: slint::SharedString = slint::platform::Key::RightArrow.into();
        let down: slint::SharedString = slint::platform::Key::DownArrow.into();
        assert!(crate::table_actions::handle_table_key(
            &app,
            &state,
            right.as_str(),
            true,
            false,
            false
        ));
        assert!(crate::table_actions::handle_table_key(
            &app,
            &state,
            down.as_str(),
            true,
            false,
            false
        ));

        let (replacement, replacement_table_id, replacement_table_offset, _) =
            populated_document_with_table(
                "new-table-document",
                "New Table Document",
                3,
                3,
                "New body",
                "replacement",
            );
        assert_eq!(
            replacement_table_id, old_table_id,
            "the regression requires a same-id table in the replacement document"
        );
        let replacement = loom_writer_core::load_document(
            &loom_writer_core::save_document(&replacement).expect("serialize replacement"),
        )
        .expect("reopen replacement with its saved caret");
        assert_eq!(
            replacement.selection(),
            loom_writer_core::TextSelection::caret(replacement_table_offset),
            "the persisted replacement caret remains inside its table"
        );
        assert!(replacement
            .table_cell_at_offset(replacement_table_offset)
            .is_some());

        assert_eq!(
            crate::table_actions::get_table_cell_selection_range(),
            Some(crate::table_actions::TableCellRange {
                block_id: old_table_id,
                start_row: 0,
                start_col: 0,
                end_row: 1,
                end_col: 1,
            }),
            "the old document's selected cells still exist immediately before replacement"
        );
        crate::replace_opened_document(
            &app,
            &state,
            std::path::PathBuf::from("replacement.loomdoc"),
            replacement,
        );
        app.invoke_copy();

        let clipboard = read_clipboard().expect("read mock clipboard");
        let copied_table = clipboard
            .to_table()
            .expect("decode copied replacement table");
        let remaining_range = crate::table_actions::get_table_cell_selection_range();
        crate::table_actions::clear_table_cell_selection_range();
        assert_eq!(
            copied_table.row_count(),
            3,
            "copy the full replacement table"
        );
        assert_eq!(
            copied_table.col_count(),
            3,
            "copy the full replacement table"
        );
        assert_eq!(
            copied_table.cell_text(0, 0).as_deref(),
            Some("replacement-0-0")
        );
        assert_eq!(
            copied_table.cell_text(2, 2).as_deref(),
            Some("replacement-2-2")
        );
        assert_eq!(
            remaining_range, None,
            "document replacement clears the old table cell range"
        );

        loom_clipboard::set_mock_clipboard_enabled(false);
    }

    #[test]
    fn pointer_selecting_another_cell_clears_table_range_before_copy() {
        loom_clipboard::set_mock_clipboard_enabled(true);
        crate::table_actions::clear_table_cell_selection_range();

        let (document, table_id, table_offset, _) = populated_document_with_table(
            "pointer-table-range",
            "Pointer Table Range",
            3,
            3,
            "Body after table",
            "cell",
        );
        let dialogs = std::rc::Rc::new(loom_desktop::ScriptedFileDialogs::new([], [None]));
        let (app, state) = crate::actions_tests::test_state(document, dialogs);
        crate::wire_writer_shared_callbacks(&app, &state, None);
        crate::apply_state(&app, &state);
        select_top_left_two_by_two(&app, &state, table_id, table_offset);
        assert_eq!(
            app.get_document_accessible_label().as_str(),
            "Table 3 by 3, cells A1:B2 selected",
            "the selected cell range is reflected in the table's accessible label"
        );

        let (point, target_offset) =
            pointer_position_for_cell(&state.current.borrow(), table_id, 2, 2);
        assert_ne!(target_offset, table_offset, "click a different cell");
        app.invoke_pointer_pressed(point.0, point.1, false);
        app.invoke_pointer_released(point.0, point.1);
        let selection = state.current.borrow().selection();
        assert!(selection.is_collapsed(), "the pointer places a caret");
        assert_eq!(
            selection.focus, target_offset,
            "the pointer's actual caret offset"
        );
        let location = state
            .current
            .borrow()
            .table_cell_at_offset(selection.focus)
            .expect("the pointer caret is in the table");
        assert_eq!(
            (location.block_id, location.row, location.col),
            (table_id, 2, 2),
            "the pointer selected the intended table cell"
        );
        let label_after_pointer = app.get_document_accessible_label().to_string();
        let range_after_pointer = crate::table_actions::get_table_cell_selection_range();
        app.invoke_copy();

        let clipboard = read_clipboard().expect("read mock clipboard");
        let copied_table = clipboard.to_table().expect("decode copied table");
        let copied_size = (copied_table.row_count(), copied_table.col_count());
        let copied_last_cell = copied_table.cell_text(2, 2);
        crate::table_actions::clear_table_cell_selection_range();
        loom_clipboard::set_mock_clipboard_enabled(false);

        assert_eq!(
            (
                label_after_pointer,
                range_after_pointer,
                copied_size,
                copied_last_cell,
            ),
            (
                "Table 3 by 3, cell C3: cell-2-2".to_owned(),
                None,
                (3, 3),
                Some("cell-2-2".to_owned()),
            ),
            "a pointer selection clears the stale range, refreshes its label, and copies the full table"
        );
    }

    #[test]
    fn pointer_reselecting_the_anchor_clears_table_range_when_caret_is_unchanged() {
        loom_clipboard::set_mock_clipboard_enabled(true);
        crate::table_actions::clear_table_cell_selection_range();

        let (document, table_id, table_offset, _) = populated_document_with_table(
            "pointer-anchor-table-range",
            "Pointer Anchor Table Range",
            3,
            3,
            "Body after table",
            "cell",
        );
        let dialogs = std::rc::Rc::new(loom_desktop::ScriptedFileDialogs::new([], [None]));
        let (app, state) = crate::actions_tests::test_state(document, dialogs);
        crate::wire_writer_shared_callbacks(&app, &state, None);
        crate::apply_state(&app, &state);
        select_top_left_two_by_two(&app, &state, table_id, table_offset);
        assert_eq!(
            app.get_document_accessible_label().as_str(),
            "Table 3 by 3, cells A1:B2 selected",
            "the selected cell range is reflected in the table's accessible label"
        );
        assert_eq!(
            state.current.borrow().selection(),
            loom_writer_core::TextSelection::caret(table_offset),
            "Shift+Arrow retains the anchor caret while extending the cell range"
        );

        let point = pointer_position_for_offset(&state.current.borrow(), table_offset);
        app.invoke_pointer_pressed(point.0, point.1, false);
        app.invoke_pointer_released(point.0, point.1);
        assert_eq!(
            state.current.borrow().selection(),
            loom_writer_core::TextSelection::caret(table_offset),
            "the click reselects the unchanged caret cell"
        );
        let label_after_pointer = app.get_document_accessible_label().to_string();
        let range_after_pointer = crate::table_actions::get_table_cell_selection_range();
        app.invoke_copy();

        let clipboard = read_clipboard().expect("read mock clipboard");
        let copied_table = clipboard.to_table().expect("decode copied table");
        let copied_size = (copied_table.row_count(), copied_table.col_count());
        let copied_last_cell = copied_table.cell_text(2, 2);
        crate::table_actions::clear_table_cell_selection_range();
        loom_clipboard::set_mock_clipboard_enabled(false);

        assert_eq!(
            (
                label_after_pointer,
                range_after_pointer,
                copied_size,
                copied_last_cell,
            ),
            (
                "Table 3 by 3, cell A1: cell-0-0".to_owned(),
                None,
                (3, 3),
                Some("cell-2-2".to_owned()),
            ),
            "reselecting the unchanged caret clears the stale range, refreshes its label, and copies the full table"
        );
    }
}
