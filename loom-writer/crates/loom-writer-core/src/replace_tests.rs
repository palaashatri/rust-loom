//! Behaviour of find and replace across blocks: non-overlapping matches, case
//! handling, comment anchors and the structure of Markdown tables.

use crate::{WriterDocument, TABLE_BLOCK_KIND};

fn document_with(blocks: &[(&str, &str)]) -> WriterDocument {
    let mut doc = WriterDocument::new("r", "R");
    for (kind, text) in blocks {
        doc.push(crate::RichBlock::new(doc.next_id(), kind, text));
    }
    doc
}

fn paragraph(text: &str) -> WriterDocument {
    document_with(&[("paragraph", text)])
}

#[test]
fn overlapping_occurrences_are_one_match_read_left_to_right() {
    let doc = paragraph("aaa");
    let hits: Vec<_> = doc
        .find_all("aa", false)
        .into_iter()
        .map(|hit| (hit.start, hit.end))
        .collect();
    assert_eq!(hits, vec![(0, 2)]);
    assert_eq!(paragraph("aaaa").count_matches("aa", false), 2);
    assert_eq!(paragraph("aaaa").count_matches("aa", true), 2);
}

#[test]
fn replace_all_does_not_consume_its_own_replacements() {
    let mut doc = paragraph("aaa");
    assert_eq!(doc.replace_all("aa", "x", false), 1);
    assert_eq!(doc.editor_text(), "xa");
}

#[test]
fn replace_all_with_a_multibyte_replacement_stays_on_char_boundaries() {
    let mut doc = paragraph("aaa");
    assert_eq!(doc.replace_all("aa", "\u{e9}", false), 1);
    assert_eq!(doc.editor_text(), "\u{e9}a");

    let mut doc = paragraph("aaaa");
    assert_eq!(doc.replace_all("aa", "\u{e9}", false), 2);
    assert_eq!(doc.editor_text(), "\u{e9}\u{e9}");

    let mut doc = paragraph("AAAA");
    assert_eq!(doc.replace_all("aa", "\u{e9}", false), 2);
    assert_eq!(doc.editor_text(), "\u{e9}\u{e9}");
}

#[test]
fn match_counts_and_replacements_agree() {
    let text = "aaaaa and Aa";
    let counted = paragraph(text).count_matches("aa", false);
    let mut doc = paragraph(text);
    assert_eq!(doc.replace_all("aa", "b", false), counted);
    assert_eq!(counted, 3);
}

#[test]
fn unicode_matches_ignore_case_and_stay_non_overlapping() {
    let mut doc = paragraph("\u{c9}\u{e9}\u{c9}");
    assert_eq!(doc.count_matches("\u{e9}", false), 3);
    assert_eq!(doc.replace_all("\u{e9}\u{e9}", "x", false), 1);
    assert_eq!(doc.editor_text(), "x\u{c9}");
}

#[test]
fn replace_all_honours_case_sensitivity() {
    let mut doc = paragraph("Cat cat");
    assert_eq!(doc.replace_all("cat", "dog", true), 1);
    assert_eq!(doc.editor_text(), "Cat dog");
}

#[test]
fn replace_all_rebases_comment_anchors_onto_the_same_words() {
    let mut doc = paragraph("hello world");
    let block_id = doc.blocks[0].id;
    doc.add_comment_thread(block_id, 6, 11, "about the world")
        .unwrap();
    assert_eq!(doc.replace_all("hello", "hi", false), 1);
    assert_eq!(doc.editor_text(), "hi world");
    let thread = &doc.comments[0];
    assert!(!thread.orphaned);
    assert_eq!(thread.block_id, block_id);
    assert_eq!(&doc.editor_text()[thread.start..thread.end], "world");
}

#[test]
fn replace_all_keeps_a_comment_on_a_replaced_word_attached_to_its_replacement() {
    let mut doc = paragraph("one cat two");
    let block_id = doc.blocks[0].id;
    doc.add_comment_thread(block_id, 4, 7, "the cat").unwrap();
    doc.replace_all("cat", "lion", false);
    assert_eq!(doc.editor_text(), "one lion two");
    let thread = &doc.comments[0];
    assert!(!thread.orphaned);
    assert_eq!(&doc.editor_text()[thread.start..thread.end], "lion");
}

#[test]
fn replace_all_leaves_a_table_divider_row_alone() {
    let mut doc = document_with(&[(TABLE_BLOCK_KIND, "| a | b |\n| --- | --- |\n| - | 2 |")]);
    assert_eq!(doc.replace_all("-", "x", false), 1, "only the cell text");
    assert_eq!(doc.editor_text(), "| a | b |\n| --- | --- |\n| x | 2 |");
}

#[test]
fn replace_all_never_changes_table_pipes() {
    let mut doc = document_with(&[(TABLE_BLOCK_KIND, "| a | b |\n| --- | --- |\n| 1 | 2 |")]);
    assert_eq!(doc.replace_all("|", "/", false), 0);
    assert_eq!(doc.replace_all("1 | 2", "x", false), 0, "spans a pipe");
    assert_eq!(doc.editor_text(), "| a | b |\n| --- | --- |\n| 1 | 2 |");
}

#[test]
fn a_replacement_containing_a_pipe_is_escaped_inside_a_table() {
    let mut doc = document_with(&[(TABLE_BLOCK_KIND, "| a | b |\n| --- | --- |\n| 1 | 2 |")]);
    assert_eq!(doc.replace_all("2", "x|y", false), 1);
    let table = crate::parse_table_markdown(doc.blocks[0].text.as_str());
    assert_eq!(table.columns(), 2, "the pipe stayed inside its cell");
    assert_eq!(table.rows.len(), 2);
    assert_eq!(table.get(1, 1), Some("x|y"));
}

#[test]
fn find_counts_the_same_matches_that_replace_all_replaces_in_a_table() {
    let mut doc = document_with(&[(TABLE_BLOCK_KIND, "| - | - |\n| --- | --- |\n| - - | 2 |")]);
    let counted = doc.count_matches("-", false);
    assert_eq!(
        counted, 4,
        "two header cells and two data dashes; the divider is not text"
    );
    assert_eq!(doc.replace_all("-", "x", false), counted);
}
