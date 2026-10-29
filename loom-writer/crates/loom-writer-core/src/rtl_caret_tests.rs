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

fn glyph_span(line: &crate::layout::FlowLine, source: std::ops::Range<usize>) -> (f32, f32) {
    let shaped = line
        .shaped_line
        .as_deref()
        .expect("document flow retains shaped glyph geometry");
    let mut left = f32::INFINITY;
    let mut right = f32::NEG_INFINITY;
    for run in shaped.painted_runs() {
        let mut pen_x = run.x_pt;
        for glyph in &run.shaped.glyphs {
            if source.contains(&(run.source_range.start + glyph.cluster)) {
                left = left.min(pen_x);
                right = right.max(pen_x + glyph.advance);
            }
            pen_x += glyph.advance;
        }
    }
    assert!(
        left.is_finite() && right.is_finite(),
        "no shaped glyphs in source range {source:?}"
    );
    (left, right)
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
    let flow = flow_of(&doc);
    let line = flow.lines()[0].clone();
    let caret = flow
        .caret_rect(&doc, &selection(0, 0, CaretAffinity::Downstream))
        .expect("a caret");
    // A paragraph with no alignment set starts at its reading start (Start),
    // which for Hebrew is the column's right edge; the caret at offset 0 sits there.
    assert!(
        (caret.rect.x - (line.bounds.x + right_edge(&doc))).abs() < 0.01,
        "the start of right-to-left text is at the column's right edge {}, got {}",
        right_edge(&doc),
        caret.rect.x - line.bounds.x
    );
}

#[test]
fn selecting_the_first_hebrew_letter_highlights_the_glyph_drawn_on_the_right() {
    let doc = hebrew_doc(HEBREW);
    let flow = flow_of(&doc);
    let line = &flow.lines()[0];
    let (alef_left, alef_right) = glyph_span(line, 0..2);
    let (_, bet_right) = glyph_span(line, 2..4);
    assert!(
        (alef_left - bet_right).abs() < 0.01,
        "the shaped bet and alef advances are adjacent"
    );

    let rects = flow.selection_rects(
        &doc,
        &selection(0, 2, CaretAffinity::Downstream),
        0..flow.page_count(),
    );
    assert_eq!(rects.len(), 1, "one highlight for alef: {rects:?}");
    let rect = rects[0].rect;
    assert!(
        (rect.x - (line.bounds.x + alef_left)).abs() < 0.01,
        "alef highlight starts at its shaped glyph position: {}",
        rect.x - line.bounds.x
    );
    assert!(
        (rect.width - (alef_right - alef_left)).abs() < 0.01,
        "highlight width {} must match alef's shaped advance",
        rect.width
    );

    let caret = flow
        .caret_rect(&doc, &selection(0, 0, CaretAffinity::Downstream))
        .expect("caret at the logical start");
    assert!(
        (caret.rect.x - (line.bounds.x + alef_right)).abs() < 0.01,
        "the logical start caret sits at the right edge of alef"
    );
}

/// The right edge of the column the Hebrew line is aligned in, relative to the
/// line's column origin.
fn right_edge(doc: &WriterDocument) -> f32 {
    doc.blocks[0].text_column_width(&doc.page.page_style())
}

#[test]
fn left_and_right_move_through_hebrew_in_the_order_it_is_drawn() {
    // Drawn left to right: bet (logical 2..4), then alef (0..2). The end of the
    // text, offset 4, is drawn on the left. The shaped glyph advances are the
    // geometry oracle for every navigation and caret assertion.
    let doc = hebrew_doc(HEBREW);
    let flow = flow_of(&doc);
    let line = &flow.lines()[0];
    let (bet_left, bet_right) = glyph_span(line, 2..4);
    let (alef_left, alef_right) = glyph_span(line, 0..2);
    assert!(
        (bet_right - alef_left).abs() < 0.01,
        "adjacent RTL glyph advances share their boundary"
    );

    let first = visual_caret_move(&doc, 4, CaretAffinity::Upstream, VisualMove::Right, None)
        .expect("Right moves over bet");
    assert_eq!(
        (first.offset, first.affinity),
        (2, CaretAffinity::Downstream)
    );
    assert!(
        (first.x - (line.bounds.x + bet_right)).abs() < 0.01,
        "after bet at {}",
        first.x
    );

    let second = visual_caret_move(&doc, 2, CaretAffinity::Downstream, VisualMove::Right, None)
        .expect("Right moves over alef");
    assert_eq!(
        (second.offset, second.affinity),
        (0, CaretAffinity::Downstream)
    );
    assert!(
        (second.x - (line.bounds.x + alef_right)).abs() < 0.01,
        "after alef at {}",
        second.x
    );

    let caret_x = |offset, affinity| {
        flow.caret_rect(&doc, &selection(offset, offset, affinity))
            .expect("caret for visual stop")
            .rect
            .x
    };
    for target in [&first, &second] {
        assert!(
            (caret_x(target.offset, target.affinity) - target.x).abs() < 0.01,
            "keyboard move and painted caret use the same shaped stop"
        );
    }

    assert!(
        visual_caret_move(&doc, 0, CaretAffinity::Downstream, VisualMove::Right, None).is_none(),
        "the line's right edge falls back to the input's own move"
    );

    let back = visual_caret_move(&doc, 0, CaretAffinity::Downstream, VisualMove::Left, None)
        .expect("Left moves back over alef");
    assert_eq!((back.offset, back.affinity), (2, CaretAffinity::Downstream));
    assert!(
        (back.x - (line.bounds.x + bet_right)).abs() < 0.01,
        "back at the shared shaped boundary"
    );
    assert!(
        (caret_x(back.offset, back.affinity) - back.x).abs() < 0.01,
        "reverse movement and painted caret use the same shaped stop"
    );

    let start = visual_caret_move(&doc, 2, CaretAffinity::Downstream, VisualMove::Left, None)
        .expect("Left moves back over bet");
    assert_eq!((start.offset, start.affinity), (4, CaretAffinity::Upstream));
    assert!(
        (start.x - (line.bounds.x + bet_left)).abs() < 0.01,
        "at the left edge of the drawn text"
    );
    assert!(
        (caret_x(start.offset, start.affinity) - start.x).abs() < 0.01,
        "line-edge caret and visual movement use the same shaped stop"
    );
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
    let flow = flow_of(&doc);
    let line = &flow.lines()[0];
    let rects = flow.selection_rects(
        &doc,
        &selection(1, 5, CaretAffinity::Downstream),
        0..flow.page_count(),
    );
    assert_eq!(rects.len(), 2, "two drawn pieces: {rects:?}");
    let (first, second) = (&rects[0], &rects[1]);
    assert_eq!((first.start, first.end), (1, 3), "b and the space");
    assert_eq!((second.start, second.end), (3, 5), "alef");

    let (selected_left, selected_right) = glyph_span(line, 1..3);
    assert!(
        (first.rect.x - (line.bounds.x + selected_left)).abs() < 0.01,
        "the LTR piece starts on its shaped glyph boundary"
    );
    assert!(
        (first.rect.width - (selected_right - selected_left)).abs() < 0.01,
        "the LTR highlight covers the selected shaped advances"
    );
    let (alef_left, alef_right) = glyph_span(line, 3..5);
    assert!(
        (second.rect.x - (line.bounds.x + alef_left)).abs() < 0.01,
        "the RTL piece starts on alef's shaped boundary"
    );
    assert!(
        (second.rect.width - (alef_right - alef_left)).abs() < 0.01,
        "the RTL highlight covers alef's shaped advance"
    );

    let (bet_left, bet_right) = glyph_span(line, 5..7);
    let first_right = first.rect.x + first.rect.width;
    assert!(
        first_right <= second.rect.x + 0.01,
        "highlights overlap: {rects:?}"
    );
    assert!(
        (first_right - (line.bounds.x + bet_left)).abs() < 0.01
            && (second.rect.x - (line.bounds.x + bet_right)).abs() < 0.01,
        "the unselected bet occupies the gap between selected pieces"
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
