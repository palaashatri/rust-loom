//! Cut and paste of plain text edit only the selected range or the caret
//! position; the rest of the document must stay as it was.

use crate::actions_tests::test_state;
use crate::*;
use loom_clipboard::{read_clipboard, set_mock_clipboard_enabled, write_clipboard, ClipboardData};
use loom_writer_core::TextSelection;
use std::sync::Mutex;

static MOCK_CLIPBOARD: Mutex<()> = Mutex::new(());

fn app_with(text: &str) -> (WriterApp, std::rc::Rc<GuiState>) {
    let dialogs = std::rc::Rc::new(loom_desktop::ScriptedFileDialogs::new([], [None]));
    let mut doc = WriterDocument::new("clip", "Clip");
    doc.replace_paragraphs(text);
    let (app, state) = test_state(doc, dialogs);
    wire_writer_shared_callbacks(&app, &state, None);
    (app, state)
}

fn put(text: &str) {
    let clip = ClipboardData {
        plain_text: Some(text.to_string()),
        ..Default::default()
    };
    write_clipboard(&clip).expect("write the mock clipboard");
}

fn select(state: &GuiState, anchor: usize, focus: usize) {
    let mut doc = state.current.borrow().clone();
    doc.set_selection(TextSelection {
        anchor,
        focus,
        ..TextSelection::caret(0)
    });
    *state.current.borrow_mut() = doc;
}

#[test]
fn cut_removes_only_the_selected_words_and_keeps_the_rest() {
    let _guard = MOCK_CLIPBOARD.lock().unwrap_or_else(|e| e.into_inner());
    set_mock_clipboard_enabled(true);
    let (app, state) = app_with("alpha beta gamma\nsecond paragraph");
    select(&state, 6, 10);
    app.invoke_cut();
    assert_eq!(
        state.current.borrow().editor_text(),
        "alpha  gamma\nsecond paragraph"
    );
    assert_eq!(
        read_clipboard().unwrap().plain_text.as_deref(),
        Some("beta")
    );
    app.invoke_undo();
    assert_eq!(
        state.current.borrow().editor_text(),
        "alpha beta gamma\nsecond paragraph",
        "one undo brings the cut words back"
    );
    set_mock_clipboard_enabled(false);
}

#[test]
fn paste_inserts_at_the_caret_and_keeps_every_other_paragraph() {
    let _guard = MOCK_CLIPBOARD.lock().unwrap_or_else(|e| e.into_inner());
    set_mock_clipboard_enabled(true);
    let (app, state) = app_with("first\nsecond");
    put(" more");
    select(&state, 5, 5);
    app.invoke_paste();
    assert_eq!(state.current.borrow().editor_text(), "first more\nsecond");
    assert_eq!(
        state.current.borrow().selection().focus,
        10,
        "the caret lands after the pasted text"
    );
    set_mock_clipboard_enabled(false);
}

#[test]
fn paste_over_a_selection_replaces_just_that_selection() {
    let _guard = MOCK_CLIPBOARD.lock().unwrap_or_else(|e| e.into_inner());
    set_mock_clipboard_enabled(true);
    let (app, state) = app_with("one two three");
    put("2");
    select(&state, 4, 7);
    app.invoke_paste();
    assert_eq!(state.current.borrow().editor_text(), "one 2 three");
    set_mock_clipboard_enabled(false);
}

#[test]
fn pasting_lines_makes_paragraphs_and_normalises_line_endings() {
    let _guard = MOCK_CLIPBOARD.lock().unwrap_or_else(|e| e.into_inner());
    set_mock_clipboard_enabled(true);
    let (app, state) = app_with("start end");
    put("a\r\nb");
    select(&state, 5, 5);
    app.invoke_paste();
    assert_eq!(state.current.borrow().editor_text(), "starta\nb end");
    set_mock_clipboard_enabled(false);
}
