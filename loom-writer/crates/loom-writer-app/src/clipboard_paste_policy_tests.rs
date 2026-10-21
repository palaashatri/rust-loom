//! What Paste turns into a table, what it keeps as text, and what Copy does
//! with a caret inside a table. Driven through the real callbacks.

use super::*;
use crate::actions_tests::test_state;
use std::sync::Mutex;

static MOCK_CLIPBOARD: Mutex<()> = Mutex::new(());

fn app_with(text: &str) -> (WriterApp, Rc<GuiState>) {
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], [None]));
    let mut doc = loom_writer_core::WriterDocument::new("paste", "Paste");
    doc.replace_paragraphs(text);
    let (app, state) = test_state(doc, dialogs);
    crate::wire_writer_shared_callbacks(&app, &state, None);
    (app, state)
}

fn put(clip: ClipboardData) {
    loom_clipboard::write_clipboard(&clip).expect("write the mock clipboard");
}

fn select(state: &GuiState, anchor: usize, focus: usize) {
    let mut doc = state.current.borrow().clone();
    doc.set_selection(loom_writer_core::TextSelection {
        anchor,
        focus,
        ..loom_writer_core::TextSelection::caret(0)
    });
    *state.current.borrow_mut() = doc;
}

fn has_table(state: &GuiState) -> bool {
    state
        .current
        .borrow()
        .blocks
        .iter()
        .any(|block| block.kind == loom_writer_core::TABLE_BLOCK_KIND)
}

/// Runs `body` with the mock clipboard on, serialised with the other tests
/// that use it.
fn with_mock_clipboard(body: impl FnOnce()) {
    let _guard = MOCK_CLIPBOARD.lock().unwrap_or_else(|e| e.into_inner());
    loom_clipboard::set_mock_clipboard_enabled(true);
    body();
    loom_clipboard::set_mock_clipboard_enabled(false);
}

#[test]
fn html_with_a_table_inside_running_text_pastes_the_text_not_only_the_table() {
    with_mock_clipboard(|| {
        put(ClipboardData {
            html: Some(
                "<html><body><p>Intro sentence.</p><table><tr><td>a</td><td>b</td></tr></table>\
                 <p>Closing words.</p></body></html>"
                    .into(),
            ),
            plain_text: Some("Intro sentence.\na\tb\nClosing words.".into()),
            ..Default::default()
        });
        let (app, state) = app_with("start");
        app.invoke_paste();
        assert!(
            !has_table(&state),
            "a table inside running text is not a table paste"
        );
        let text = state.current.borrow().editor_text();
        assert!(text.contains("Intro sentence."), "{text}");
        assert!(text.contains("Closing words."), "{text}");
    });
}

#[test]
fn tabbed_plain_text_without_a_spreadsheet_representation_is_text_not_a_table() {
    with_mock_clipboard(|| {
        put(ClipboardData {
            plain_text: Some("if\tx\nthen\ty".into()),
            ..Default::default()
        });
        let (app, state) = app_with("start");
        app.invoke_paste();
        assert!(
            !has_table(&state),
            "tabbed text with no table format is text"
        );
        assert!(state.current.borrow().editor_text().contains("then"));
    });
}

#[test]
fn tab_separated_rows_with_ragged_tab_counts_are_text_not_a_table() {
    with_mock_clipboard(|| {
        let tsv = "a\tb\tc\nd\te";
        put(ClipboardData {
            tsv: Some(tsv.into()),
            plain_text: Some(tsv.into()),
            ..Default::default()
        });
        let (app, state) = app_with("start");
        app.invoke_paste();
        assert!(!has_table(&state), "ragged rows are not a table");
    });
}

#[test]
fn indented_code_with_uniform_tabs_is_text_not_a_table() {
    with_mock_clipboard(|| {
        let code = "\tif x\n\treturn y";
        put(ClipboardData {
            tsv: Some(code.into()),
            plain_text: Some(code.into()),
            ..Default::default()
        });
        let (app, state) = app_with("start");
        app.invoke_paste();
        assert!(!has_table(&state), "indented code is not a table");
        assert!(state.current.borrow().editor_text().contains("return y"));
    });
}

#[test]
fn a_table_pasted_over_a_selection_replaces_the_selected_text() {
    with_mock_clipboard(|| {
        put(ClipboardData {
            html: Some("<table><tr><td>a</td><td>b</td></tr></table>".into()),
            plain_text: Some("a\tb".into()),
            ..Default::default()
        });
        let (app, state) = app_with("Intro word here");
        select(&state, 6, 10);
        app.invoke_paste();
        assert!(has_table(&state), "the table was pasted");
        assert!(
            !state.current.borrow().editor_text().contains("word"),
            "the selected text was replaced: {}",
            state.current.borrow().editor_text()
        );
    });
}

#[test]
fn ctrl_c_with_a_collapsed_caret_in_a_table_cell_copies_nothing() {
    with_mock_clipboard(|| {
        put(ClipboardData {
            plain_text: Some("previous clipboard".into()),
            ..Default::default()
        });
        let mut doc = loom_writer_core::WriterDocument::new("copy", "Copy");
        doc.replace_paragraphs("Intro\nAfter");
        doc.insert_table_block(0, 2, 2).unwrap();
        let cell = (0..=doc.editor_text().len())
            .find_map(|offset| doc.table_cell_at_offset(offset))
            .expect("a table cell offset");
        doc.set_selection(loom_writer_core::TextSelection::caret(cell.cell_start));
        let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], [None]));
        let (app, state) = test_state(doc, dialogs);
        crate::wire_writer_shared_callbacks(&app, &state, None);

        app.invoke_copy();

        assert_eq!(
            loom_clipboard::read_clipboard()
                .unwrap()
                .plain_text
                .as_deref(),
            Some("previous clipboard"),
            "a collapsed caret copies nothing"
        );
        assert_eq!(app.get_status_left(), "Nothing selected");
    });
}
