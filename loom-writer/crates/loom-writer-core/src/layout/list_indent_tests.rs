//! A list item's text starts to the right of the body column, with room on its
//! left for the marker at every list level. The caret and the wrap read the
//! same column as the drawn text, so they stay on the glyphs.

use crate::list_levels::{LIST_LEVEL_INDENT_PT, MAX_LIST_LEVEL};
use crate::text_metrics::block_measure;
use crate::{PageStyle, PageViewport, RichBlock, TextSelection, WriterDocument};

/// The gap the page draws between a list marker and the text after it.
const MARKER_GAP_PT: f32 = 4.0;

fn viewport() -> PageViewport {
    PageViewport {
        width: 900.0,
        height: 900.0,
        zoom: 1.0,
        scroll_x: 0.0,
        scroll_y: 0.0,
    }
}

/// A body paragraph followed by one short item for each `(kind, level)`.
fn document(items: &[(&str, u8)]) -> WriterDocument {
    let mut doc = WriterDocument::new("list", "List");
    doc.blocks.push(RichBlock::new(1, "paragraph", "Body"));
    for (index, (kind, level)) in items.iter().enumerate() {
        let mut block = RichBlock::new(index as u64 + 2, kind, "Item");
        block.style.list_level = *level;
        doc.blocks.push(block);
    }
    doc
}

/// The left edge of the line drawn for block `index` of `doc`.
fn text_x(doc: &WriterDocument, index: usize) -> f32 {
    let flow = doc
        .flow(&PageStyle::default(), viewport())
        .expect("a valid flow");
    flow.lines()
        .iter()
        .find(|line| line.block_index == index)
        .expect("the block has a line")
        .bounds
        .x
}

#[test]
fn list_text_starts_right_of_body_text_and_each_level_steps_in_by_its_indent() {
    let body = text_x(&document(&[]), 0);
    let doc = document(&[
        ("list-bulleted", 0),
        ("list-bulleted", 1),
        ("list-numbered", 2),
    ]);
    let top = text_x(&doc, 1);
    assert!(
        top > body,
        "list text at {top} must start right of body text at {body}"
    );
    let nested = text_x(&doc, 2);
    let deeper = text_x(&doc, 3);
    assert!(
        (nested - top - LIST_LEVEL_INDENT_PT).abs() < 0.01,
        "level 1 steps in one level indent: {nested} vs {top}"
    );
    assert!(
        (deeper - top - 2.0 * LIST_LEVEL_INDENT_PT).abs() < 0.01,
        "level 2 steps in two level indents: {deeper} vs {top}"
    );
}

#[test]
fn every_marker_sits_left_of_its_text_inside_its_own_level_column() {
    let style = PageStyle::default();
    let body = text_x(&document(&[]), 0);
    for (kind, number) in [("list-bulleted", 0), ("list-numbered", 999)] {
        for level in 0..=MAX_LIST_LEVEL {
            let doc = document(&[(kind, level)]);
            let marker = doc.blocks[1].list_marker(number);
            let width = block_measure(&marker, &[], style.font_size_for_kind(kind))
                .advance(0, marker.len());
            let text = text_x(&doc, 1);
            let right = text - MARKER_GAP_PT;
            let left = right - width;
            assert!(
                right <= text,
                "marker {marker:?} ends at or before its text"
            );
            let column = body + f32::from(level) * LIST_LEVEL_INDENT_PT;
            assert!(
                left >= column - 0.01,
                "marker {marker:?} at level {level} starts at {left}, left of its column {column}"
            );
        }
    }
}

#[test]
fn the_caret_at_the_start_of_a_list_item_sits_on_its_text() {
    let style = PageStyle::default();
    let doc = document(&[("list-numbered", 1)]);
    let flow = doc.flow(&style, viewport()).expect("a valid flow");
    // The body paragraph and its line break come first, so the item starts at byte 5.
    let caret = flow
        .caret_rect(&doc, &TextSelection::caret(5))
        .expect("a caret at the item's start");
    let text = text_x(&doc, 1);
    assert!(
        (caret.rect.x - text).abs() < 0.01,
        "caret at {} sits on the item text at {text}",
        caret.rect.x
    );
}

#[test]
fn every_line_of_a_wrapped_list_item_shares_the_text_column() {
    let style = PageStyle::default();
    let mut doc = WriterDocument::new("list", "List");
    doc.blocks
        .push(RichBlock::new(1, "list-bulleted", &"word ".repeat(60)));
    let flow = doc.flow(&style, viewport()).expect("a valid flow");
    let lines = flow.lines();
    assert!(lines.len() > 1, "the item wraps to several lines");
    let column = doc.blocks[0].text_column_width(&style);
    let first = lines[0].bounds.x;
    for line in lines {
        assert!(
            (line.bounds.x - first).abs() < 0.01,
            "line starts at {} not {first}",
            line.bounds.x
        );
        assert!(
            line.bounds.width <= column + 1.0,
            "line fits the item's text column: {} vs {column}",
            line.bounds.width
        );
    }
}
