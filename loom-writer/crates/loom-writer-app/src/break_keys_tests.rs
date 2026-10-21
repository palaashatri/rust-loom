//! Shift+Enter and Ctrl+Enter through the real key callback.

use std::rc::Rc;

use loom_writer_core::{PageStyle, RichBlock, TextSelection, WriterDocument};
use slint::platform::Key;
use slint::SharedString;

use crate::actions_tests::test_state;
use crate::{wire_writer_shared_callbacks, GuiState, WriterApp};

fn session(blocks: Vec<RichBlock>) -> (WriterApp, Rc<GuiState>) {
    let mut document = WriterDocument::new("test", "Test");
    document.blocks = blocks;
    let (app, state) = test_state(
        document,
        Rc::new(loom_desktop::ScriptedFileDialogs::new([], [])),
    );
    wire_writer_shared_callbacks(&app, &state, None);
    (app, state)
}

fn press(app: &WriterApp, shift: bool, ctrl: bool) -> bool {
    app.invoke_handle_table_key(SharedString::from(Key::Return), shift, ctrl, false)
}

fn caret(state: &GuiState, at: usize) {
    state
        .current
        .borrow_mut()
        .set_selection(TextSelection::caret(at));
}

fn texts(state: &GuiState) -> Vec<String> {
    state
        .current
        .borrow()
        .blocks
        .iter()
        .map(|b| b.text.as_str().to_string())
        .collect()
}

#[test]
fn shift_enter_breaks_the_line_inside_the_paragraph_as_one_undo_step() {
    let (app, state) = session(vec![RichBlock::new(1, "paragraph", "AB")]);
    caret(&state, 1);
    assert!(press(&app, true, false));
    assert_eq!(texts(&state), ["A\u{b}B"], "still one paragraph");
    assert_eq!(state.current.borrow().selection(), TextSelection::caret(2));
    let doc = state.current.borrow().clone();
    let lines = doc
        .flow(&PageStyle::default(), Default::default())
        .unwrap()
        .lines()
        .len();
    assert_eq!(lines, 2, "the break starts a second line");
    app.invoke_undo();
    assert_eq!(texts(&state), ["AB"]);
    // Backspace removes it like a character.
    app.invoke_redo();
    app.invoke_document_edited("AB".into(), 1, 1);
    assert_eq!(texts(&state), ["AB"]);
}

#[test]
fn shift_enter_replaces_a_selection() {
    let (app, state) = session(vec![RichBlock::new(1, "paragraph", "A--B")]);
    state
        .current
        .borrow_mut()
        .set_selection(TextSelection::range(1, 3));
    assert!(press(&app, true, false));
    assert_eq!(texts(&state), ["A\u{b}B"]);
}

#[test]
fn ctrl_enter_starts_the_text_after_the_caret_on_a_new_page() {
    let (app, state) = session(vec![RichBlock::new(1, "paragraph", "AB")]);
    caret(&state, 1);
    assert!(press(&app, false, true));
    assert_eq!(texts(&state), ["A\u{c}", "B"]);
    assert_eq!(state.current.borrow().selection(), TextSelection::caret(3));
    let pages = state
        .current
        .borrow()
        .paginate(&PageStyle::default())
        .unwrap();
    assert_eq!(pages.len(), 2);
    assert_eq!(pages[1].fragments[0].text, "B");
    // One undo removes the whole break.
    app.invoke_undo();
    assert_eq!(texts(&state), ["AB"]);
    app.invoke_redo();
    // Backspace at the start of the new page joins, then removes the break.
    app.invoke_document_edited("A\u{c}B".into(), 2, 2);
    assert_eq!(texts(&state), ["A\u{c}B"]);
    app.invoke_document_edited("AB".into(), 1, 1);
    assert_eq!(texts(&state), ["AB"]);
}

#[test]
fn ctrl_enter_at_the_end_of_the_document_opens_an_empty_next_page() {
    let (app, state) = session(vec![RichBlock::new(1, "paragraph", "Last")]);
    caret(&state, 4);
    assert!(press(&app, false, true));
    assert_eq!(texts(&state), ["Last\u{c}", ""]);
    let pages = state
        .current
        .borrow()
        .paginate(&PageStyle::default())
        .unwrap();
    assert_eq!(pages.len(), 2);
}

#[test]
fn plain_enter_and_other_chords_are_left_to_the_editor() {
    let (app, state) = session(vec![RichBlock::new(1, "paragraph", "AB")]);
    caret(&state, 1);
    assert!(!press(&app, false, false));
    assert!(!press(&app, true, true));
    assert_eq!(texts(&state), ["AB"]);
}

#[test]
fn breaks_do_not_go_inside_a_table_cell() {
    let mut doc = WriterDocument::new("t", "T");
    doc.insert_table_block(0, 2, 2).unwrap();
    let before = doc.editor_text();
    let (app, state) = session(doc.blocks.clone());
    caret(&state, 2);
    assert!(press(&app, true, false));
    assert!(press(&app, false, true));
    assert_eq!(state.current.borrow().editor_text(), before);
}

#[test]
fn break_characters_are_not_drawn() {
    let markup = super::without_breaks("a\u{b}b\u{c}".to_string());
    assert_eq!(markup, "ab");
}
