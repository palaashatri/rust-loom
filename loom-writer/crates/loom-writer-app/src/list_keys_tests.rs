//! Paragraph structure edits through the real callbacks: Enter splits keep
//! formatting, Enter on an empty list item and Backspace at the start of one
//! remove the marker, and a join keeps the first paragraph's kind.

use super::actions_tests::test_state;
use super::*;
use loom_text::Alignment;
use slint::platform::Key;

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

fn key(app: &WriterApp, key: Key) -> bool {
    app.invoke_handle_table_key(SharedString::from(key), false, false, false)
}

fn kinds(state: &GuiState) -> Vec<String> {
    state
        .current
        .borrow()
        .blocks
        .iter()
        .map(|b| b.kind.clone())
        .collect()
}

fn caret(state: &GuiState, at: usize) {
    state
        .current
        .borrow_mut()
        .set_selection(TextSelection::caret(at));
}

#[test]
fn enter_in_the_middle_of_a_centered_heading_keeps_both_halves_formatted() {
    let mut heading = RichBlock::new(1, "heading2", "Hello world");
    heading.style.alignment = Alignment::Center;
    let (app, state) = session(vec![heading]);
    app.invoke_document_edited("Hello\n world".into(), 6, 6);
    let doc = state.current.borrow();
    assert_eq!(doc.editor_text(), "Hello\n world");
    assert!(doc.blocks.iter().all(|b| b.kind == "heading2"));
    assert!(doc
        .blocks
        .iter()
        .all(|b| b.style.alignment == Alignment::Center));
}

#[test]
fn enter_at_the_end_of_a_list_item_adds_an_item_and_numbers_follow() {
    let (app, state) = session(vec![
        RichBlock::new(1, "list-numbered", "one"),
        RichBlock::new(2, "list-numbered", "two"),
    ]);
    app.invoke_document_edited("one\n\ntwo".into(), 4, 4);
    assert_eq!(kinds(&state), ["list-numbered"; 3]);
}

#[test]
fn enter_on_an_empty_list_item_leaves_the_list_as_one_undoable_step() {
    let (app, state) = session(vec![
        RichBlock::new(1, "list-numbered", "one"),
        RichBlock::new(2, "list-numbered", ""),
    ]);
    caret(&state, 4);
    assert!(key(&app, Key::Return));
    assert_eq!(kinds(&state), ["list-numbered", "paragraph"]);
    assert_eq!(state.current.borrow().editor_text(), "one\n");
    app.invoke_undo();
    assert_eq!(kinds(&state), ["list-numbered", "list-numbered"]);
}

#[test]
fn enter_on_a_non_empty_list_item_is_left_to_the_editor() {
    let (app, state) = session(vec![RichBlock::new(1, "list-bulleted", "item")]);
    caret(&state, 0);
    assert!(!key(&app, Key::Return));
    assert_eq!(kinds(&state), ["list-bulleted"]);
}

#[test]
fn backspace_at_the_start_of_a_list_item_removes_the_marker_before_joining() {
    let (app, state) = session(vec![
        RichBlock::new(1, "paragraph", "intro"),
        RichBlock::new(2, "list-bulleted", "item"),
    ]);
    caret(&state, 6);
    assert!(key(&app, Key::Backspace));
    assert_eq!(kinds(&state), ["paragraph", "paragraph"]);
    assert_eq!(state.current.borrow().editor_text(), "intro\nitem");
    // The next Backspace is an ordinary join, left to the editor.
    assert!(!key(&app, Key::Backspace));
    app.invoke_document_edited("introitem".into(), 5, 5);
    assert_eq!(state.current.borrow().editor_text(), "introitem");
    assert_eq!(kinds(&state), ["paragraph"]);
}

#[test]
fn backspace_inside_a_list_item_is_left_to_the_editor() {
    let (app, state) = session(vec![RichBlock::new(1, "list-bulleted", "item")]);
    caret(&state, 2);
    assert!(!key(&app, Key::Backspace));
    assert_eq!(kinds(&state), ["list-bulleted"]);
}

#[test]
fn joining_paragraphs_with_backspace_keeps_the_first_ones_kind() {
    let mut first = RichBlock::new(1, "heading1", "Head");
    first.style.alignment = Alignment::Center;
    let (app, state) = session(vec![first, RichBlock::new(2, "paragraph", "tail")]);
    app.invoke_document_edited("Headtail".into(), 4, 4);
    let doc = state.current.borrow();
    assert_eq!(doc.blocks.len(), 1);
    assert_eq!(doc.blocks[0].kind, "heading1");
    assert_eq!(doc.blocks[0].style.alignment, Alignment::Center);
}

/// Tab or Shift+Tab through the same callback the editor uses.
fn tab(app: &WriterApp, shift: bool) -> bool {
    app.invoke_handle_table_key(SharedString::from(Key::Tab), shift, false, false)
}

fn levels(state: &GuiState) -> Vec<u8> {
    state
        .current
        .borrow()
        .blocks
        .iter()
        .map(|b| b.style.list_level)
        .collect()
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
fn tab_nests_a_list_item_from_any_caret_position_and_shift_tab_lifts_it() {
    let (app, state) = session(vec![RichBlock::new(1, "list-bulleted", "item")]);
    caret(&state, 2);
    assert!(tab(&app, false), "Tab in a list item is taken by the list");
    assert_eq!(levels(&state), [1]);
    assert_eq!(texts(&state), ["item"], "no spaces are typed");
    assert!(tab(&app, true));
    assert_eq!(levels(&state), [0]);
    assert_eq!(kinds(&state), ["list-bulleted"]);
}

#[test]
fn tab_stops_at_the_deepest_level_and_each_change_is_one_undo_step() {
    let mut item = RichBlock::new(1, "list-numbered", "item");
    item.style.list_level = 5;
    let (app, state) = session(vec![item]);
    caret(&state, 0);
    assert!(tab(&app, false));
    assert_eq!(levels(&state), [5], "level 5 is the deepest");
    assert!(tab(&app, true));
    assert_eq!(levels(&state), [4]);
    app.invoke_undo();
    assert_eq!(levels(&state), [5]);
}

#[test]
fn tab_in_a_paragraph_is_left_to_the_editor_which_inserts_spaces() {
    let (app, state) = session(vec![RichBlock::new(1, "paragraph", "text")]);
    caret(&state, 0);
    assert!(!tab(&app, false));
    assert_eq!(levels(&state), [0]);
    assert_eq!(texts(&state), ["text"]);
}

#[test]
fn tab_over_a_selection_changes_each_list_item_in_it_and_leaves_other_blocks() {
    let (app, state) = session(vec![
        RichBlock::new(1, "list-bulleted", "alpha"),
        RichBlock::new(2, "paragraph", "middle"),
        RichBlock::new(3, "list-numbered", "omega"),
    ]);
    let end = "alpha\nmiddle\nomega".len();
    state
        .current
        .borrow_mut()
        .set_selection(TextSelection::range(0, end));
    assert!(tab(&app, false));
    assert_eq!(levels(&state), [1, 0, 1]);
}

#[test]
fn enter_on_an_empty_nested_item_moves_it_up_then_leaves_the_list() {
    let mut nested = RichBlock::new(2, "list-bulleted", "");
    nested.style.list_level = 1;
    let (app, state) = session(vec![RichBlock::new(1, "list-bulleted", "top"), nested]);
    caret(&state, 4);
    assert!(key(&app, Key::Return));
    assert_eq!(levels(&state), [0, 0]);
    assert_eq!(kinds(&state), ["list-bulleted", "list-bulleted"]);
    assert!(key(&app, Key::Return));
    assert_eq!(kinds(&state), ["list-bulleted", "paragraph"]);
}

#[test]
fn backspace_at_the_start_of_a_nested_item_moves_it_up_before_removing_its_marker() {
    let mut nested = RichBlock::new(2, "list-bulleted", "item");
    nested.style.list_level = 1;
    let (app, state) = session(vec![RichBlock::new(1, "paragraph", "intro"), nested]);
    caret(&state, 6);
    assert!(key(&app, Key::Backspace));
    assert_eq!(kinds(&state), ["paragraph", "list-bulleted"]);
    assert_eq!(levels(&state), [0, 0]);
    assert_eq!(texts(&state), ["intro", "item"]);
    assert!(key(&app, Key::Backspace));
    assert_eq!(kinds(&state), ["paragraph", "paragraph"]);
    assert_eq!(texts(&state), ["intro", "item"]);
}
