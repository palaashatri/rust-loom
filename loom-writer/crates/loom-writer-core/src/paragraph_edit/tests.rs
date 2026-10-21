use crate::TextSelection;
use crate::{RichBlock, WriterDocument};
use loom_text::{Alignment, CharacterStyle, FontWeight, StyleRun};

fn bold() -> CharacterStyle {
    CharacterStyle {
        weight: FontWeight::Bold,
        ..Default::default()
    }
}

fn block(id: u64, kind: &str, text: &str) -> RichBlock {
    RichBlock::new(id, kind, text)
}

fn doc_of(blocks: Vec<RichBlock>) -> WriterDocument {
    let mut doc = WriterDocument::new("d", "t");
    doc.blocks = blocks;
    doc
}

/// Types `inserted` at `at` the way the editor reports it: new text plus the
/// caret after the insertion.
fn type_at(doc: &mut WriterDocument, at: usize, inserted: &str) {
    let mut text = doc.editor_text();
    text.insert_str(at, inserted);
    doc.replace_editor_text_at(&text, Some(at + inserted.len()))
        .unwrap();
}

/// Deletes `start..end` the way the editor reports it.
fn delete(doc: &mut WriterDocument, start: usize, end: usize) {
    let mut text = doc.editor_text();
    text.replace_range(start..end, "");
    doc.replace_editor_text_at(&text, Some(start)).unwrap();
}

fn kinds(doc: &WriterDocument) -> Vec<&str> {
    doc.blocks.iter().map(|b| b.kind.as_str()).collect()
}

fn texts(doc: &WriterDocument) -> Vec<&str> {
    doc.blocks.iter().map(|b| b.text.as_str()).collect()
}

#[test]
fn enter_in_the_middle_of_a_heading_keeps_both_halves_headings_with_their_runs() {
    let mut heading = block(1, "heading2", "Hello world");
    heading.style.alignment = Alignment::Center;
    heading.runs = vec![StyleRun {
        start: 6,
        end: 11,
        style: bold(),
    }];
    let mut doc = doc_of(vec![heading]);
    type_at(&mut doc, 5, "\n");
    assert_eq!(texts(&doc), ["Hello", " world"]);
    assert_eq!(kinds(&doc), ["heading2", "heading2"]);
    assert!(doc
        .blocks
        .iter()
        .all(|b| b.style.alignment == Alignment::Center));
    assert_ne!(doc.blocks[0].id, doc.blocks[1].id);
    assert!(doc.blocks[0].runs.is_empty(), "{:?}", doc.blocks[0].runs);
    assert_eq!(doc.blocks[1].runs.len(), 1);
    assert_eq!(
        (doc.blocks[1].runs[0].start, doc.blocks[1].runs[0].end),
        (1, 6)
    );
}

#[test]
fn enter_in_a_paragraph_keeps_alignment_and_runs_on_the_right_characters() {
    let mut para = block(1, "paragraph", "aa");
    para.style.alignment = Alignment::Right;
    para.runs = vec![StyleRun {
        start: 0,
        end: 1,
        style: bold(),
    }];
    let mut doc = doc_of(vec![para]);
    type_at(&mut doc, 1, "\n");
    assert_eq!(texts(&doc), ["a", "a"]);
    assert!(doc
        .blocks
        .iter()
        .all(|b| b.style.alignment == Alignment::Right));
    assert_eq!(doc.blocks[0].runs.len(), 1);
    assert!(doc.blocks[1].runs.is_empty(), "{:?}", doc.blocks[1].runs);
}

#[test]
fn enter_at_the_end_of_a_heading_starts_a_normal_paragraph() {
    let mut heading = block(1, "heading1", "Title");
    heading.style.alignment = Alignment::Center;
    let mut doc = doc_of(vec![heading, block(2, "paragraph", "Body")]);
    type_at(&mut doc, 5, "\n");
    assert_eq!(texts(&doc), ["Title", "", "Body"]);
    assert_eq!(kinds(&doc), ["heading1", "paragraph", "paragraph"]);
    assert_eq!(doc.blocks[1].style.alignment, Alignment::Center);
    assert_eq!(doc.blocks[0].id, 1);
    assert_eq!(doc.blocks[2].id, 2);
}

#[test]
fn enter_at_the_start_of_a_block_keeps_the_text_block_identity() {
    let mut doc = doc_of(vec![
        block(1, "heading1", "Title"),
        block(2, "list-bulleted", "x"),
    ]);
    type_at(&mut doc, 6, "\n");
    assert_eq!(texts(&doc), ["Title", "", "x"]);
    assert_eq!(kinds(&doc), ["heading1", "list-bulleted", "list-bulleted"]);
    assert_eq!(doc.blocks[2].id, 2);
}

#[test]
fn enter_at_the_end_of_a_list_item_makes_a_new_item_of_the_same_kind() {
    for kind in ["list-bulleted", "list-numbered"] {
        let mut doc = doc_of(vec![block(1, kind, "one"), block(2, kind, "two")]);
        type_at(&mut doc, 3, "\n");
        assert_eq!(texts(&doc), ["one", "", "two"]);
        assert_eq!(kinds(&doc), [kind, kind, kind]);
    }
}

#[test]
fn enter_on_an_empty_list_item_exits_the_list() {
    let mut doc = doc_of(vec![
        block(1, "list-numbered", "one"),
        block(2, "list-numbered", ""),
        block(3, "list-numbered", "two"),
    ]);
    type_at(&mut doc, 4, "\n");
    assert_eq!(texts(&doc), ["one", "", "two"]);
    assert_eq!(kinds(&doc), ["list-numbered", "paragraph", "list-numbered"]);
    assert_eq!(doc.blocks[1].id, 2);
    assert_eq!(doc.selection(), TextSelection::caret(4));
}

#[test]
fn removing_a_list_marker_turns_the_item_into_a_paragraph_first() {
    let mut doc = doc_of(vec![
        block(1, "paragraph", "intro"),
        block(2, "list-bulleted", "item"),
    ]);
    assert!(doc.unlist_block_at(6));
    assert_eq!(kinds(&doc), ["paragraph", "paragraph"]);
    assert_eq!(texts(&doc), ["intro", "item"]);
    assert!(!doc.unlist_block_at(6), "a plain paragraph has no marker");
}

#[test]
fn joining_two_paragraphs_keeps_the_first_ones_kind_style_and_runs() {
    let mut first = block(1, "heading1", "Head");
    first.style.alignment = Alignment::Center;
    let mut second = block(2, "paragraph", "Headline tail");
    second.runs = vec![StyleRun {
        start: 9,
        end: 13,
        style: bold(),
    }];
    let mut doc = doc_of(vec![first, second]);
    delete(&mut doc, 4, 5);
    assert_eq!(texts(&doc), ["HeadHeadline tail"]);
    assert_eq!(kinds(&doc), ["heading1"]);
    assert_eq!(doc.blocks[0].style.alignment, Alignment::Center);
    assert_eq!(doc.blocks[0].id, 1);
    assert_eq!(doc.blocks[0].runs.len(), 1);
    assert_eq!(
        (doc.blocks[0].runs[0].start, doc.blocks[0].runs[0].end),
        (13, 17)
    );
}

#[test]
fn joining_a_paragraph_with_a_following_heading_stays_a_paragraph() {
    let mut doc = doc_of(vec![
        block(1, "paragraph", "ab"),
        block(2, "heading1", "abcd"),
    ]);
    delete(&mut doc, 2, 3);
    assert_eq!(texts(&doc), ["ababcd"]);
    assert_eq!(kinds(&doc), ["paragraph"]);
}

#[test]
fn deleting_across_paragraphs_keeps_the_first_paragraphs_formatting() {
    let mut first = block(1, "heading1", "First");
    first.style.alignment = Alignment::Center;
    let mut doc = doc_of(vec![
        first,
        block(2, "paragraph", "Second"),
        block(3, "list-bulleted", "Third"),
    ]);
    // From "Fi|rst" to "Th|ird".
    delete(&mut doc, 2, 15);
    assert_eq!(texts(&doc), ["Fiird"]);
    assert_eq!(kinds(&doc), ["heading1"]);
    assert_eq!(doc.blocks[0].style.alignment, Alignment::Center);
}

#[test]
fn numbered_items_stay_in_one_list_after_enter_and_join() {
    let mut doc = doc_of(vec![
        block(1, "list-numbered", "a"),
        block(2, "list-numbered", "b"),
    ]);
    type_at(&mut doc, 1, "\n");
    assert_eq!(kinds(&doc), ["list-numbered"; 3]);
    delete(&mut doc, 1, 2);
    assert_eq!(texts(&doc), ["a", "b"]);
    assert_eq!(kinds(&doc), ["list-numbered"; 2]);
}
