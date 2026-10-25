//! Insert > Comment: the Document tab opens with the keyboard caret in the
//! new-comment field, typed text lands there, and the draft stays in the field
//! until Add Comment. Every step sends real key events through
//! `Window::dispatch_event`, so Slint's own focus handling decides the result.

use super::keyboard_flow_tests::{launched, Session};
use super::*;
use i_slint_backend_testing::{AccessibleRole, ElementHandle, ElementRoot};
use slint::platform::{Key, WindowEvent};
use slint::private_unstable_api::re_exports::{ItemWeak, WindowInner};

fn press(app: &WriterApp, key: Key) {
    tap(app, SharedString::from(key));
}

fn tap(app: &WriterApp, text: impl Into<SharedString>) {
    let text = text.into();
    app.window()
        .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    app.window()
        .dispatch_event(WindowEvent::KeyReleased { text });
    let _ = snapshot_component(app, 1280.0, 800.0, 1.0).expect("render after a key");
}

fn type_text(app: &WriterApp, text: &str) {
    for character in text.chars() {
        tap(app, character.to_string());
    }
}

fn focus_weak(app: &WriterApp) -> ItemWeak {
    WindowInner::from_pub(app.window())
        .focus_item
        .borrow()
        .clone()
}

/// The accessible name of the smallest named element under the keyboard focus.
fn focus_name(app: &WriterApp) -> String {
    let Some(item) = focus_weak(app).upgrade() else {
        return String::new();
    };
    let geometry = item.geometry();
    let origin = item.map_to_window(geometry.origin);
    let (cx, cy) = (
        origin.x + geometry.size.width / 2.0,
        origin.y + geometry.size.height / 2.0,
    );
    let mut best: Option<(f32, String)> = None;
    for element in app
        .root_element()
        .query_descendants()
        .match_predicate(|_| true)
        .find_all()
    {
        let Some(label) = element.accessible_label().filter(|label| !label.is_empty()) else {
            continue;
        };
        let (position, size) = (element.absolute_position(), element.size());
        let inside = cx >= position.x
            && cx <= position.x + size.width
            && cy >= position.y
            && cy <= position.y + size.height;
        let area = size.width * size.height;
        if inside && best.as_ref().is_none_or(|(smallest, _)| area < *smallest) {
            best = Some((area, label.to_string()));
        }
    }
    best.map(|(_, label)| label).unwrap_or_default()
}

/// Reaches the Insert toolbar item with Tab, opens its menu and runs the row at
/// `row` (Table is 0, Picture 1, Chart 2, Comment 3).
/// Insert menu rows, top to bottom: Page Break, Line Break, Table, Picture,
/// Chart from Clipboard, Comment. The menu highlight is moved with the arrow
/// keys, so Comment is the sixth row.
const INSERT_COMMENT_ROW: usize = 5;

fn run_insert_row(app: &WriterApp, row: usize) {
    app.invoke_focus_page();
    press(app, Key::F6);
    for _ in 0..12 {
        if focus_name(app) == "Insert" {
            break;
        }
        press(app, Key::Tab);
    }
    assert_eq!(focus_name(app), "Insert", "Tab reaches the Insert item");
    press(app, Key::Return);
    assert_eq!(app.get_toolbar_menu(), 3, "Enter opens the Insert menu");
    for _ in 0..row {
        press(app, Key::DownArrow);
    }
    press(app, Key::Return);
}

/// The text the new-comment field holds, read from its accessible value.
fn field_text(app: &WriterApp) -> String {
    ElementHandle::find_by_accessible_label(app, "New comment")
        .find(|element| element.accessible_role() == Some(AccessibleRole::TextInput))
        .and_then(|element| element.accessible_value())
        .map(|value| value.to_string())
        .unwrap_or_default()
}

fn assert_caret_in_new_comment_field(app: &WriterApp) {
    assert!(app.get_show_inspector(), "the inspector is open");
    assert_eq!(
        app.get_inspector_tab(),
        1,
        "comments live on the Document tab"
    );
    assert_eq!(
        focus_name(app),
        "New comment",
        "the keyboard caret is in the new-comment field, not on another control"
    );
}

#[test]
fn insert_comment_from_the_menu_puts_the_caret_in_the_new_comment_field() {
    let s: Session = launched("Hello world");
    run_insert_row(&s.app, INSERT_COMMENT_ROW);
    assert_caret_in_new_comment_field(&s.app);
    type_text(&s.app, "Note");
    assert_eq!(
        field_text(&s.app),
        "Note",
        "the typed text is kept in the new-comment field"
    );
}

#[test]
fn insert_comment_with_the_inspector_closed_opens_it_on_the_new_comment_field() {
    let s = launched("Hello world");
    s.app.set_show_inspector(false);
    let _ = snapshot_component(&s.app, 1280.0, 800.0, 1.0).expect("render");
    run_insert_row(&s.app, INSERT_COMMENT_ROW);
    assert_caret_in_new_comment_field(&s.app);
    type_text(&s.app, "Note");
    assert_eq!(field_text(&s.app), "Note");
}

#[test]
fn insert_comment_from_the_toolbar_command_path_focuses_the_field() {
    let s = launched("Hello world");
    s.app.invoke_toolbar_command("insert.comment".into());
    let _ = snapshot_component(&s.app, 1280.0, 800.0, 1.0).expect("render");
    assert_caret_in_new_comment_field(&s.app);
}

#[test]
fn the_draft_stays_in_the_field_until_enter_adds_the_comment() {
    let s = launched("Hello world");
    run_insert_row(&s.app, INSERT_COMMENT_ROW);
    type_text(&s.app, "Check this");
    let _ = snapshot_component(&s.app, 1280.0, 800.0, 1.0).expect("render");
    assert_eq!(
        field_text(&s.app),
        "Check this",
        "the draft is still in the field after a render"
    );
    press(&s.app, Key::Return);
    assert_eq!(
        s.state.current.borrow().comments.len(),
        1,
        "Enter in the field adds the comment"
    );
    assert_eq!(
        field_text(&s.app),
        "",
        "the field is cleared once the comment is added"
    );
}

#[test]
fn a_comment_with_a_bare_caret_says_it_covers_the_whole_paragraph() {
    let s = launched("Hello world");
    s.state
        .current
        .borrow_mut()
        .set_selection(loom_writer_core::TextSelection::caret(2));
    s.app.invoke_add_comment(SharedString::from("Check this"));
    assert_eq!(
        s.state.current.borrow().comments.len(),
        1,
        "the comment is still added to the paragraph"
    );
    assert_eq!(
        s.app.get_status_right().to_string(),
        "Comment added to the whole paragraph; select text to anchor it to a phrase",
        "the status names the text the comment covers"
    );
}

#[test]
fn a_comment_on_selected_text_says_it_is_anchored_to_that_text() {
    let s = launched("Hello world");
    s.state
        .current
        .borrow_mut()
        .set_selection(loom_writer_core::TextSelection::range(0, 5));
    s.app.invoke_add_comment(SharedString::from("Check this"));
    assert_eq!(s.state.current.borrow().comments.len(), 1);
    assert_eq!(
        s.app.get_status_right().to_string(),
        "Comment added to the selected text",
        "the status names the text the comment covers"
    );
}

#[test]
fn a_selection_across_paragraphs_says_only_its_first_paragraph_is_commented() {
    let s = launched("Hello world\nSecond para");
    s.state
        .current
        .borrow_mut()
        .set_selection(loom_writer_core::TextSelection::range(6, 18));
    s.app.invoke_add_comment(SharedString::from("Check this"));
    let document = s.state.current.borrow();
    assert_eq!(document.comments.len(), 1, "one thread is added");
    let first = document.blocks[0].id;
    let comment = &document.comments[0];
    assert_eq!(
        (comment.block_id, comment.start, comment.end),
        (first, 6, 11),
        "the thread covers 'world' in the first paragraph and nothing after it"
    );
    assert_eq!(
        s.app.get_status_right().to_string(),
        "Comment added to the first paragraph of the selection; the rest is not covered",
        "the status says the rest of the selection is not commented"
    );
}

#[test]
fn a_comment_with_the_caret_in_a_table_names_the_table() {
    let s = launched("Lead");
    let table_id = {
        let mut document = s.state.current.borrow_mut();
        document.insert_table_block(0, 2, 2).expect("insert table");
        let mut start = 0usize;
        let mut caret = None;
        let mut table_id = None;
        for block in &document.blocks {
            if block.kind == loom_writer_core::TABLE_BLOCK_KIND {
                caret = Some(start);
                table_id = Some(block.id);
                break;
            }
            start += block.text.len_bytes() + 1;
        }
        document.set_selection(loom_writer_core::TextSelection::caret(
            caret.expect("the table block is in the document"),
        ));
        table_id.expect("the table block has an id")
    };
    s.app.invoke_add_comment(SharedString::from("Check this"));
    let document = s.state.current.borrow();
    assert_eq!(
        document.comments.len(),
        1,
        "the comment is added to the table"
    );
    assert_eq!(
        document.comments[0].block_id, table_id,
        "the thread is anchored to the table block"
    );
    assert_eq!(
        s.app.get_status_right().to_string(),
        "Comment added to the whole table",
        "the status names the table rather than a paragraph"
    );
}

#[test]
fn whole_block_comment_status_names_the_kind_of_block_it_covers() {
    assert_eq!(
        crate::whole_block_comment_status(loom_writer_core::IMAGE_BLOCK_KIND),
        "Comment added to the whole picture"
    );
    assert_eq!(
        crate::whole_block_comment_status(loom_writer_core::CHART_BLOCK_KIND),
        "Comment added to the whole chart"
    );
    assert_eq!(
        crate::whole_block_comment_status("paragraph"),
        "Comment added to the whole paragraph; select text to anchor it to a phrase"
    );
}
