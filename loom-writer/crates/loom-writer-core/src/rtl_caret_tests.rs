//! Caret, pointer and selection geometry for right-to-left text.
//!
//! Hebrew and Arabic are drawn in visual order, so the first logical letter sits
//! on the right. Carets, selection rectangles and pointer hits must follow the
//! drawn glyphs, not the logical advances.

use super::*;
use crate::{offset_at_x, DocumentFlow, TextSelection};

/// Alef then bet: two letters, two bytes each, right to left.
const HEBREW: &str = "\u{5D0}\u{5D1}";

fn hebrew_doc(text: &str) -> WriterDocument {
    let mut doc = WriterDocument::new("rtl", "Right to left");
    doc.blocks = vec![RichBlock::new(1, "paragraph", text)];
    doc
}

fn flow_of(doc: &WriterDocument) -> DocumentFlow {
    let style = doc.page.page_style();
    doc.flow(
        &style,
        PageViewport {
            width: style.width_pt,
            height: style.height_pt,
            zoom: 1.0,
            scroll_x: 0.0,
            scroll_y: 0.0,
        },
    )
    .expect("flow")
}

fn selection(anchor: usize, focus: usize, affinity: CaretAffinity) -> TextSelection {
    TextSelection {
        anchor,
        focus,
        affinity,
    }
}

#[test]
fn a_pointer_on_the_left_of_hebrew_text_lands_on_the_glyph_drawn_there() {
    // Bet is drawn on the left, so a point in its left half is the end of the text.
    let size = 12.0;
    assert_eq!(offset_at_x(HEBREW, 0, &[], size, 0.5), HEBREW.len());
    // Past the right edge is the start of the text, where alef is drawn.
    assert_eq!(offset_at_x(HEBREW, 0, &[], size, 10_000.0), 0);
}

#[test]
fn the_caret_at_the_start_of_a_hebrew_line_sits_at_its_right_edge() {
    let doc = hebrew_doc(HEBREW);
    let size = doc.page.page_style().font_size_for_kind("paragraph");
    let flow = flow_of(&doc);
    let line = flow.lines()[0].clone();
    let width = text_advance(HEBREW, 0, &[], size);
    let caret = flow
        .caret_rect(&doc, &selection(0, 0, CaretAffinity::Downstream))
        .expect("a caret");
    assert!(
        (caret.rect.x - (line.bounds.x + width)).abs() < 0.01,
        "the start of right-to-left text is at its right edge {width}, got {}",
        caret.rect.x - line.bounds.x
    );
}

#[test]
fn selecting_the_first_hebrew_letter_highlights_the_glyph_drawn_on_the_right() {
    let doc = hebrew_doc(HEBREW);
    let size = doc.page.page_style().font_size_for_kind("paragraph");
    let flow = flow_of(&doc);
    let line = flow.lines()[0].clone();
    let alef = text_advance(&HEBREW[..2], 0, &[], size);
    let bet = text_advance(&HEBREW[2..], 2, &[], size);
    let rects = flow.selection_rects(
        &doc,
        &selection(0, 2, CaretAffinity::Downstream),
        0..flow.page_count(),
    );
    assert_eq!(rects.len(), 1, "one highlight for alef: {rects:?}");
    let rect = rects[0].rect;
    assert!(
        (rect.x - (line.bounds.x + bet)).abs() < 0.01,
        "alef is drawn after bet, at {bet}; highlight starts at {}",
        rect.x - line.bounds.x
    );
    assert!(
        (rect.width - alef).abs() < 0.01,
        "highlight width {}",
        rect.width
    );
}

fn origin(doc: &WriterDocument) -> f32 {
    flow_of(doc).lines()[0].bounds.x
}

#[test]
fn left_and_right_move_through_hebrew_in_the_order_it_is_drawn() {
    // Drawn left to right: bet (logical 2..4), then alef (0..2). The end of the
    // text, offset 4, is drawn on the left.
    let doc = hebrew_doc(HEBREW);
    let size = doc.page.page_style().font_size_for_kind("paragraph");
    let bet = text_advance(&HEBREW[2..], 2, &[], size);
    let alef = text_advance(&HEBREW[..2], 0, &[], size);
    let origin = origin(&doc);

    let first = visual_caret_move(&doc, 4, CaretAffinity::Upstream, VisualMove::Right, None)
        .expect("Right moves over bet");
    assert_eq!(
        (first.offset, first.affinity),
        (2, CaretAffinity::Downstream)
    );
    assert!((first.x - (origin + bet)).abs() < 0.01, "at {}", first.x);

    let second = visual_caret_move(&doc, 2, CaretAffinity::Downstream, VisualMove::Right, None)
        .expect("Right moves over alef");
    assert_eq!(
        (second.offset, second.affinity),
        (0, CaretAffinity::Downstream)
    );
    assert!(
        (second.x - (origin + bet + alef)).abs() < 0.01,
        "at {}",
        second.x
    );

    assert!(
        visual_caret_move(&doc, 0, CaretAffinity::Downstream, VisualMove::Right, None).is_none(),
        "the line's right edge falls back to the input's own move"
    );

    let back = visual_caret_move(&doc, 0, CaretAffinity::Downstream, VisualMove::Left, None)
        .expect("Left moves back over alef");
    assert_eq!((back.offset, back.affinity), (2, CaretAffinity::Downstream));
    let start = visual_caret_move(&doc, 2, CaretAffinity::Downstream, VisualMove::Left, None)
        .expect("Left moves back over bet");
    assert_eq!((start.offset, start.affinity), (4, CaretAffinity::Upstream));
    assert!(
        visual_caret_move(&doc, 4, CaretAffinity::Upstream, VisualMove::Left, None).is_none(),
        "the line's left edge falls back too"
    );
}

#[test]
fn left_to_right_text_is_left_to_the_input_so_its_moves_do_not_change() {
    let doc = hebrew_doc("abc");
    assert!(
        visual_caret_move(&doc, 1, CaretAffinity::Downstream, VisualMove::Right, None).is_none()
    );
    assert!(
        visual_caret_move(&doc, 1, CaretAffinity::Downstream, VisualMove::Left, None).is_none()
    );
    let size = doc.page.page_style().font_size_for_kind("paragraph");
    assert_eq!(offset_at_x("abc", 0, &[], size, 0.5), 0);
    assert_eq!(doc.blocks[0].line_offset_at_x(size, 0..3, 0.5), 0);
}

#[test]
fn a_selection_across_a_right_to_left_run_highlights_each_drawn_piece_without_overlap() {
    // "ab " then alef and bet (drawn reversed), then " cd". Logical 1..5 selects
    // b, the space and alef; bet sits between the space and alef on screen.
    let text = "ab \u{5D0}\u{5D1} cd";
    let doc = hebrew_doc(text);
    let size = doc.page.page_style().font_size_for_kind("paragraph");
    let flow = flow_of(&doc);
    let rects = flow.selection_rects(
        &doc,
        &selection(1, 5, CaretAffinity::Downstream),
        0..flow.page_count(),
    );
    assert_eq!(rects.len(), 2, "two drawn pieces: {rects:?}");
    let (first, second) = (&rects[0], &rects[1]);
    assert_eq!((first.start, first.end), (1, 3), "b and the space");
    assert_eq!((second.start, second.end), (3, 5), "alef");
    let gap_from = first.rect.x + first.rect.width;
    assert!(
        gap_from <= second.rect.x + 0.01,
        "highlights overlap: {rects:?}"
    );
    let bet = text_advance(&text[5..7], 5, &[], size);
    assert!(
        (second.rect.x - gap_from - bet).abs() < 0.01,
        "bet is drawn between the two pieces: gap {}",
        second.rect.x - gap_from
    );
}

#[test]
fn a_hebrew_hit_inside_a_paragraph_is_relative_to_the_line_start() {
    let doc = hebrew_doc(HEBREW);
    let size = doc.page.page_style().font_size_for_kind("paragraph");
    let block = &doc.blocks[0];
    assert_eq!(block.line_offset_at_x(size, 0..4, 0.5), 4);
    assert_eq!(block.line_offset_at_x(size, 0..4, 10_000.0), 0);
}
