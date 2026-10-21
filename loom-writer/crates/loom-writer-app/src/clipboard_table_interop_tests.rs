use super::*;

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

    let (point, target_offset) = pointer_position_for_cell(&state.current.borrow(), table_id, 2, 2);
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
