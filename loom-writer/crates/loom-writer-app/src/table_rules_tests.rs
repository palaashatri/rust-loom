//! The divider row of a table is skipped by the caret and cannot be typed over.

use super::*;
use crate::actions_tests::{test_state, text_document};
use crate::table_rules::{divider_ranges, edit_touches_divider};
use loom_desktop::ScriptedFileDialogs;
use loom_writer_core::{RichBlock, TABLE_BLOCK_KIND};
use std::rc::Rc;

/// "Intro", a table with a divider, then "Outro". The divider occupies bytes
/// 16..29 of the editor text, with the header ending at byte 15 and the first
/// data row starting at byte 30.
const TEXT: &str = "Intro\n| A | B |\n| --- | --- |\n| 1 | 2 |\nOutro";

fn document_with_table() -> WriterDocument {
    let mut document = WriterDocument::new("tables", "Tables");
    document.push(RichBlock::new(1, "paragraph", "Intro"));
    document.push(RichBlock::new(
        2,
        TABLE_BLOCK_KIND,
        "| A | B |\n| --- | --- |\n| 1 | 2 |",
    ));
    document.push(RichBlock::new(3, "paragraph", "Outro"));
    document
}

fn session() -> (WriterApp, Rc<GuiState>) {
    let dialogs = Rc::new(ScriptedFileDialogs::new([], [None]));
    let (app, state) = test_state(document_with_table(), dialogs);
    crate::wire_writer_shared_callbacks(&app, &state, None);
    crate::apply_state(&app, &state);
    (app, state)
}

#[test]
fn the_divider_is_the_second_line_of_each_table() {
    assert_eq!(TEXT.len(), 45, "the fixture offsets used below");
    let dividers = divider_ranges(&document_with_table());
    assert_eq!(dividers.len(), 1, "one table, one divider");
    assert_eq!(dividers[0], 16..29);
    assert!(divider_ranges(&text_document("no table here")).is_empty());
}

#[test]
fn moving_down_onto_the_divider_lands_on_the_first_data_row() {
    let (app, state) = session();
    app.invoke_selection_changed(15, 15);
    // The text box moves the caret onto the divider; the caret must not stay there.
    app.invoke_selection_changed(16, 16);
    assert_eq!(
        state.current.borrow().selection(),
        TextSelection::caret(30),
        "the caret skips the divider going down"
    );
}

#[test]
fn moving_up_onto_the_divider_lands_on_the_header() {
    let (app, state) = session();
    app.invoke_selection_changed(30, 30);
    app.invoke_selection_changed(16, 16);
    assert_eq!(
        state.current.borrow().selection(),
        TextSelection::caret(15),
        "the caret skips the divider going up"
    );
}

#[test]
fn typing_into_the_divider_is_refused_and_the_divider_stays() {
    let (app, state) = session();
    app.invoke_selection_changed(16, 16);
    let typed = format!("{}x{}", &TEXT[..16], &TEXT[16..]);
    app.invoke_document_edited(typed.into(), 17, 17);
    let document = state.current.borrow();
    assert_eq!(document.editor_text(), TEXT, "the divider is unchanged");
    assert_eq!(
        app.get_status_left().as_str(),
        "The table divider row is not editable."
    );
}

#[test]
fn typing_in_a_header_cell_is_not_refused() {
    let (app, state) = session();
    // Byte 8 is just before the "A" of the header's first cell.
    app.invoke_selection_changed(8, 8);
    let typed = format!("{}x{}", &TEXT[..8], &TEXT[8..]);
    app.invoke_document_edited(typed.into(), 9, 9);
    assert!(
        state
            .current
            .borrow()
            .editor_text()
            .starts_with("Intro\n| xA | B |"),
        "cells above and below the divider still take text"
    );
}

#[test]
fn an_edit_that_touches_the_divider_is_refused_however_it_is_made() {
    let dividers = divider_ranges(&document_with_table());
    // Typed inside, at either edge, and by deleting the newline before the divider.
    assert!(edit_touches_divider(
        TEXT,
        &insert(TEXT, 16, "x"),
        &dividers
    ));
    assert!(edit_touches_divider(
        TEXT,
        &insert(TEXT, 29, "x"),
        &dividers
    ));
    assert!(edit_touches_divider(TEXT, &remove(TEXT, 15), &dividers));
    // Typing at the end of the header, or in the first data row, is not the divider.
    assert!(!edit_touches_divider(
        TEXT,
        &insert(TEXT, 15, "x"),
        &dividers
    ));
    assert!(!edit_touches_divider(
        TEXT,
        &insert(TEXT, 30, "x"),
        &dividers
    ));
}

const TABLE: &str = "| A | B |\n| --- | --- |\n| 1 | 2 |";

fn session_for(document: WriterDocument) -> (WriterApp, Rc<GuiState>) {
    let dialogs = Rc::new(ScriptedFileDialogs::new([], [None]));
    let (app, state) = test_state(document, dialogs);
    crate::wire_writer_shared_callbacks(&app, &state, None);
    crate::apply_state(&app, &state);
    (app, state)
}

#[test]
fn text_typed_right_after_a_table_starts_a_paragraph_that_counts() {
    let mut document = WriterDocument::new("table-end", "Tables");
    document.push(RichBlock::new(1, "paragraph", "Intro"));
    document.push(RichBlock::new(2, TABLE_BLOCK_KIND, TABLE));
    let (app, state) = session_for(document);
    let words_before = state.current.borrow().text_counts().0;
    // The caret sits at the end of the table's last row, the document's last text.
    let end = "Intro\n".len() + TABLE.len();
    app.invoke_selection_changed(end as i32, end as i32);
    let typed = format!("Intro\n{TABLE}x");
    app.invoke_document_edited(typed.into(), end as i32 + 1, end as i32 + 1);
    let document = state.current.borrow();
    assert_eq!(
        document.blocks.len(),
        3,
        "the typed text is a block of its own"
    );
    assert_eq!(
        document.blocks[1].text.as_str(),
        TABLE,
        "the table keeps its rows"
    );
    assert_eq!(document.blocks[2].kind.as_str(), "paragraph");
    assert_eq!(document.blocks[2].text.as_str(), "x");
    assert_eq!(
        document.text_counts().0,
        words_before + 1,
        "the typed word is counted"
    );
}

fn insert(text: &str, at: usize, typed: &str) -> String {
    format!("{}{typed}{}", &text[..at], &text[at..])
}

fn remove(text: &str, at: usize) -> String {
    format!("{}{}", &text[..at], &text[at + 1..])
}
