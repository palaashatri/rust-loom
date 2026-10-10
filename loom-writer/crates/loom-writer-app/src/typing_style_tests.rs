//! Ctrl+B, Ctrl+I and Ctrl+U at a collapsed caret set the style of the text typed
//! next, at that caret only.

use super::*;
use crate::actions_tests::{test_state, text_document};
use std::rc::Rc;

/// A window whose document is `text`, with the caret not yet moved.
fn session(text: &str) -> (WriterApp, Rc<GuiState>) {
    crate::typing_style::clear();
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], [None]));
    let (app, state) = test_state(text_document(text), dialogs);
    crate::wire_writer_shared_callbacks(&app, &state, None);
    crate::apply_state(&app, &state);
    (app, state)
}

/// Whether the byte at `offset` of the first paragraph is italic.
fn italic_at(document: &WriterDocument, offset: usize) -> bool {
    document.blocks[0]
        .runs
        .iter()
        .any(|run| run.start <= offset && offset < run.end && run.style.italic)
}

#[test]
fn ctrl_i_at_a_collapsed_caret_italicises_the_text_typed_next() {
    let (app, state) = session("Hello");
    app.invoke_selection_changed(5, 5);
    app.invoke_toggle_italic();
    assert_eq!(
        app.get_status_right().as_str(),
        "Italic on for new text",
        "a collapsed caret sets the style instead of refusing"
    );
    app.invoke_document_edited("HelloX".into(), 6, 6);
    let document = state.current.borrow();
    assert_eq!(document.plain_text(), "HelloX");
    assert!(
        italic_at(&document, 5),
        "the typed X takes the pending italic"
    );
    assert!(
        !italic_at(&document, 4),
        "the text before the caret is unchanged"
    );
}

#[test]
fn a_second_toggle_turns_the_pending_style_back_off() {
    let (app, state) = session("Hello");
    app.invoke_selection_changed(5, 5);
    app.invoke_toggle_italic();
    app.invoke_toggle_italic();
    assert_eq!(app.get_status_right().as_str(), "Italic off for new text");
    app.invoke_document_edited("HelloY".into(), 6, 6);
    assert!(!italic_at(&state.current.borrow(), 5));
}

#[test]
fn moving_the_caret_drops_the_pending_style() {
    let (app, state) = session("Hello");
    app.invoke_selection_changed(5, 5);
    app.invoke_toggle_italic();
    app.invoke_selection_changed(2, 2);
    app.invoke_document_edited("HeXllo".into(), 3, 3);
    let document = state.current.borrow();
    assert_eq!(document.plain_text(), "HeXllo");
    assert!(
        !italic_at(&document, 2),
        "text typed where the caret moved to is not italic"
    );
}
