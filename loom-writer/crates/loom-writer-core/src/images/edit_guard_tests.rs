//! Typing next to a picture must never change the picture's own characters.
//! Edits go through the same entry point the editor uses.

use super::*;
use crate::{RichBlock, WriterDocument};
use std::io::Cursor;

fn png() -> Vec<u8> {
    let mut bytes = Vec::new();
    image::RgbaImage::from_pixel(8, 4, image::Rgba([10, 90, 200, 255]))
        .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
        .unwrap();
    bytes
}

/// Blocks "before", a picture, "after": the picture sits on its own line.
fn document_with_picture() -> (WriterDocument, u64) {
    let mut doc = WriterDocument::new("doc", "Doc");
    doc.blocks.push(RichBlock::new(1, "paragraph", "before"));
    doc.blocks.push(RichBlock::new(2, "paragraph", "after"));
    let image = InlineImage::from_bytes("pic.png", png()).unwrap();
    let id = doc.insert_image_block(6, image).unwrap();
    (doc, id)
}

/// Byte range of block `id` in the editor text.
fn span(doc: &WriterDocument, id: u64) -> (usize, usize) {
    let mut start = 0;
    for block in &doc.blocks {
        let end = start + block.text.len_bytes();
        if block.id == id {
            return (start, end);
        }
        start = end + 1;
    }
    panic!("block {id} is in the document");
}

fn picture_text(doc: &WriterDocument, id: u64) -> String {
    doc.blocks
        .iter()
        .find(|block| block.id == id)
        .unwrap()
        .text
        .as_str()
        .to_string()
}

/// Edits `text` at `at` by inserting `what`, then applies it the way the
/// editor does. Returns the result and the text before the edit.
fn type_at(doc: &mut WriterDocument, at: usize, what: &str) -> Result<bool, String> {
    let mut text = doc.editor_text();
    text.insert_str(at, what);
    doc.replace_editor_text_at(&text, Some(at + what.len()))
        .map_err(|error| error.to_string())
}

/// After a refused edit the document is what it was; after an accepted one
/// the picture still has exactly its own text and the typed text is a
/// paragraph of its own (never inside the picture).
fn assert_picture_kept(doc: &WriterDocument, id: u64, original_text: &str, typed: &str) {
    let block = doc.blocks.iter().find(|block| block.id == id).unwrap();
    assert!(block.is_image(), "the picture is still a picture");
    assert_eq!(
        picture_text(doc, id),
        original_text,
        "the picture keeps exactly its two characters"
    );
    assert!(
        doc.blocks
            .iter()
            .any(|block| !block.is_image() && block.text.as_str().contains(typed)),
        "typed text goes into a paragraph beside the picture"
    );
}

#[test]
fn typing_at_the_end_of_a_picture_line_never_adds_text_to_the_picture() {
    let (mut doc, id) = document_with_picture();
    let original = picture_text(&doc, id);
    let (_, end) = span(&doc, id);
    let before = doc.editor_text();
    match type_at(&mut doc, end, "x") {
        Ok(_) => assert_picture_kept(&doc, id, &original, "x"),
        Err(_) => assert_eq!(doc.editor_text(), before, "a refused edit changes nothing"),
    }
    assert_eq!(picture_text(&doc, id), original);
}

#[test]
fn typing_between_the_picture_characters_is_refused_or_lands_beside_it() {
    let (mut doc, id) = document_with_picture();
    let original = picture_text(&doc, id);
    let (start, _) = span(&doc, id);
    // After the object character (3 bytes), before the marker.
    let inside = start + '\u{FFFC}'.len_utf8();
    let before = doc.editor_text();
    match type_at(&mut doc, inside, "x") {
        Ok(_) => assert_picture_kept(&doc, id, &original, "x"),
        Err(_) => assert_eq!(doc.editor_text(), before, "a refused edit changes nothing"),
    }
    assert_eq!(picture_text(&doc, id), original);
}

#[test]
fn typing_before_the_picture_never_becomes_part_of_it() {
    let (mut doc, id) = document_with_picture();
    let original = picture_text(&doc, id);
    let (start, _) = span(&doc, id);
    let before = doc.editor_text();
    match type_at(&mut doc, start, "x") {
        Ok(_) => assert_picture_kept(&doc, id, &original, "x"),
        Err(_) => assert_eq!(doc.editor_text(), before, "a refused edit changes nothing"),
    }
    assert_eq!(picture_text(&doc, id), original);
}

#[test]
fn deleting_the_whole_picture_line_still_removes_the_picture() {
    let (mut doc, id) = document_with_picture();
    let (start, end) = span(&doc, id);
    // The separator before the picture plus the picture itself.
    let mut text = doc.editor_text();
    text.replace_range(start - 1..end, "");
    doc.replace_editor_text_at(&text, Some(start - 1))
        .expect("removing a whole picture is an edit the editor allows");
    assert!(
        doc.blocks.iter().all(|block| !block.is_image()),
        "the picture is gone"
    );
    assert_eq!(doc.editor_text(), "before\nafter");
}

#[test]
fn typing_in_the_paragraph_before_a_picture_still_works() {
    let (mut doc, _) = document_with_picture();
    let result = type_at(&mut doc, 6, " more");
    assert_eq!(result, Ok(true));
    assert!(doc.editor_text().starts_with("before more\n"));
    assert!(doc.blocks.iter().any(|block| block.is_image()));
}
