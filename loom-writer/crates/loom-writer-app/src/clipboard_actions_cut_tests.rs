use super::*;

#[test]
fn cut_preserves_selection_and_history_when_clipboard_write_fails() {
    let _clipboard_guard = MockClipboardGuard::new();
    write_clipboard(&ClipboardData {
        plain_text: Some("previous clipboard".into()),
        ..Default::default()
    })
    .expect("seed the clipboard before inducing a write failure");

    let mut document = loom_writer_core::WriterDocument::new("cut-failure", "Cut failure");
    document.replace_paragraphs("alpha beta gamma");
    let (app, state) = cut_test_app(document);

    let mut after_first_edit = state.current.borrow().clone();
    after_first_edit.replace_paragraphs("alpha beta gamma!");
    crate::apply_with_history(&app, &state, after_first_edit, HistoryKind::DocumentAction);
    let mut after_second_edit = state.current.borrow().clone();
    after_second_edit.replace_paragraphs("alpha beta gamma!!");
    crate::apply_with_history(&app, &state, after_second_edit, HistoryKind::DocumentAction);
    let restored = state
        .history
        .borrow_mut()
        .undo()
        .expect("undo the second setup edit to seed the redo history");
    *state.current.borrow_mut() = restored;
    crate::apply_state(&app, &state);

    app.invoke_selection_changed(6, 10);
    app.set_status_left("Ready".into());
    let before = state.current.borrow().clone();
    let undo_before = state.history.borrow().undo_len();
    let redo_before = state.history.borrow().redo_len();
    assert_eq!(
        (undo_before, redo_before),
        (1, 1),
        "the failure regression starts with both undo and redo entries"
    );

    loom_clipboard::set_mock_clipboard_fails(true);
    app.invoke_cut();
    loom_clipboard::set_mock_clipboard_fails(false);

    assert_eq!(
        *state.current.borrow(),
        before,
        "a failed clipboard write keeps the document and selected text"
    );
    assert_eq!(state.history.borrow().undo_len(), undo_before);
    assert_eq!(state.history.borrow().redo_len(), redo_before);
    assert_eq!(
        app.get_status_left().as_str(),
        "Cut failed: Clipboard is unavailable"
    );
    assert_eq!(
        read_clipboard()
            .expect("read the prior clipboard contents")
            .plain_text
            .as_deref(),
        Some("previous clipboard"),
        "failure leaves the old clipboard contents intact"
    );
}

#[test]
fn cut_preserves_a_picture_selection_when_its_clipboard_format_is_unsupported() {
    let _clipboard_guard = MockClipboardGuard::new();
    write_clipboard(&ClipboardData {
        plain_text: Some("previous clipboard".into()),
        ..Default::default()
    })
    .expect("seed the clipboard before cutting the picture");

    let mut document = loom_writer_core::WriterDocument::new("cut-picture", "Cut picture");
    document.replace_paragraphs("Before\nAfter");
    let image = loom_writer_core::InlineImage::from_bytes("photo.png", tiny_png())
        .expect("valid PNG fixture");
    let image_id = document
        .insert_image_block("Before".len(), image)
        .expect("insert the picture block");
    let (start, end) = block_range(&document, image_id);
    document.set_selection(loom_writer_core::TextSelection::range(start, end));
    let (app, state) = cut_test_app(document);
    let before = state.current.borrow().clone();
    let undo_before = state.history.borrow().undo_len();
    let redo_before = state.history.borrow().redo_len();
    app.set_status_left("Ready".into());

    app.invoke_cut();

    assert_eq!(
        *state.current.borrow(),
        before,
        "unsupported picture data stays in the document with its selection"
    );
    assert_eq!(state.history.borrow().undo_len(), undo_before);
    assert_eq!(state.history.borrow().redo_len(), redo_before);
    assert_eq!(
        app.get_status_left().as_str(),
        "Cut failed: selected object format is unsupported"
    );
    assert_eq!(
        read_clipboard()
            .expect("read the prior clipboard contents")
            .plain_text
            .as_deref(),
        Some("previous clipboard"),
        "unsupported object content does not overwrite the clipboard with markers"
    );
}

#[test]
fn cut_of_a_virtual_table_range_copies_then_clears_only_the_selected_cells() {
    let _clipboard_guard = MockClipboardGuard::new();
    let (document, table_id, _, _) = populated_document_with_table(
        "cut-table-range",
        "Cut table range",
        3,
        3,
        "Body after table",
        "cell",
    );
    let (app, state) = cut_test_app(document);
    let range = crate::table_actions::TableCellRange {
        block_id: table_id,
        start_row: 0,
        start_col: 0,
        end_row: 1,
        end_col: 1,
    };
    crate::table_actions::set_table_cell_selection_range(Some(range));
    crate::apply_state(&app, &state);
    let before = state.current.borrow().clone();
    let undo_before = state.history.borrow().undo_len();

    app.invoke_cut();

    let copied = read_clipboard()
        .expect("cut writes the selected cells before editing the document")
        .to_table()
        .expect("the cut range remains table clipboard data");
    assert_eq!((copied.row_count(), copied.col_count()), (2, 2));
    assert_eq!(copied.cell_text(0, 0).as_deref(), Some("cell-0-0"));
    assert_eq!(copied.cell_text(0, 1).as_deref(), Some("cell-0-1"));
    assert_eq!(copied.cell_text(1, 0).as_deref(), Some("cell-1-0"));
    assert_eq!(copied.cell_text(1, 1).as_deref(), Some("cell-1-1"));

    assert_eq!(
        crate::table_actions::get_table_cell_selection_range(),
        None,
        "successful cut clears the virtual range"
    );
    let after = state.current.borrow();
    let table = after
        .table_from_block(table_id)
        .expect("the table remains in the document");
    assert_eq!((table.rows.len(), table.columns()), (3, 3));
    assert_eq!(table.get(0, 0), Some(""));
    assert_eq!(table.get(0, 1), Some(""));
    assert_eq!(table.get(1, 0), Some(""));
    assert_eq!(table.get(1, 1), Some(""));
    assert_eq!(table.get(0, 2), Some("cell-0-2"));
    assert_eq!(table.get(1, 2), Some("cell-1-2"));
    assert_eq!(table.get(2, 0), Some("cell-2-0"));
    assert_eq!(table.get(2, 1), Some("cell-2-1"));
    assert_eq!(table.get(2, 2), Some("cell-2-2"));
    let caret = after
        .table_cell_at_offset(after.selection().anchor)
        .expect("the caret remains in the table after cutting");
    assert_eq!((caret.block_id, caret.row, caret.col), (table_id, 0, 0));
    drop(after);

    assert_eq!(state.history.borrow().undo_len(), undo_before + 1);
    assert_eq!(
        state.history.borrow_mut().undo(),
        Some(before),
        "one undo restores the exact pre-cut document"
    );
    crate::table_actions::clear_table_cell_selection_range();
}

#[test]
fn failed_virtual_table_range_cut_preserves_cells_range_history_and_clipboard() {
    let _clipboard_guard = MockClipboardGuard::new();
    write_clipboard(&ClipboardData {
        plain_text: Some("previous clipboard".into()),
        ..Default::default()
    })
    .expect("seed the clipboard before inducing a write failure");
    let (document, table_id, _, _) = populated_document_with_table(
        "cut-table-range-failure",
        "Cut table range failure",
        3,
        3,
        "Body after table",
        "cell",
    );
    let (app, state) = cut_test_app(document);
    let range = crate::table_actions::TableCellRange {
        block_id: table_id,
        start_row: 0,
        start_col: 0,
        end_row: 1,
        end_col: 1,
    };
    crate::table_actions::set_table_cell_selection_range(Some(range));
    let before = state.current.borrow().clone();
    let undo_before = state.history.borrow().undo_len();
    let redo_before = state.history.borrow().redo_len();
    app.set_status_left("Ready".into());
    loom_clipboard::set_mock_clipboard_fails(true);

    app.invoke_cut();

    loom_clipboard::set_mock_clipboard_fails(false);
    assert_eq!(
        *state.current.borrow(),
        before,
        "failed clipboard write leaves every table cell and the caret unchanged"
    );
    assert_eq!(
        crate::table_actions::get_table_cell_selection_range(),
        Some(range),
        "failed clipboard write keeps the virtual selection available for retry"
    );
    assert_eq!(state.history.borrow().undo_len(), undo_before);
    assert_eq!(state.history.borrow().redo_len(), redo_before);
    assert_eq!(
        app.get_status_left().as_str(),
        "Cut failed: Clipboard is unavailable"
    );
    assert_eq!(
        read_clipboard()
            .expect("read the prior clipboard contents")
            .plain_text
            .as_deref(),
        Some("previous clipboard"),
        "failure leaves the old clipboard contents intact"
    );
    crate::table_actions::clear_table_cell_selection_range();
}
