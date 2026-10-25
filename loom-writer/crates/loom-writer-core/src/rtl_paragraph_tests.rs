//! Paragraph direction for right-to-left text.
//!
//! A paragraph's default alignment is `Start` (see `alignment_start_tests`): a
//! right-to-left paragraph that the user has not aligned starts at the right edge
//! of its column, as in every word processor, so its text ends at the column's
//! left edge and the caret at its start sits on the right. An explicit alignment
//! set by the user always wins, and left-to-right text stays flush left.

use crate::{
    text_advance, CaretAffinity, DocumentFlow, PageViewport, RichBlock, TextSelection,
    WriterDocument,
};

/// Shalom olam: right-to-left letters with one space, drawn as a single line.
const SHALOM_OLAM: &str = "\u{5E9}\u{5DC}\u{5D5}\u{5DD} \u{5E2}\u{5D5}\u{5DC}\u{5DD}";

fn document(text: &str, alignment: Option<loom_text::Alignment>) -> WriterDocument {
    let mut block = RichBlock::new(1, "paragraph", text);
    if let Some(alignment) = alignment {
        block.style.alignment = alignment;
    }
    let mut doc = WriterDocument::new("rtl-paragraph", "Right to left paragraph");
    doc.blocks = vec![block];
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

/// The x of the right edge of the first line's text column, in viewport points.
fn column_right(doc: &WriterDocument, flow: &DocumentFlow) -> f32 {
    let line = &flow.lines()[0];
    line.bounds.x + doc.blocks[0].text_column_width(&doc.page.page_style())
}

/// The left and right edges of the drawn text on the first line, in viewport
/// points, measured from the highlight of the whole text.
fn drawn_extent(doc: &WriterDocument, flow: &DocumentFlow) -> (f32, f32) {
    let len = doc.blocks[0].text.as_str().len();
    let rects = flow.selection_rects(
        doc,
        &selection(0, len, CaretAffinity::Downstream),
        0..flow.page_count(),
    );
    assert!(!rects.is_empty(), "the whole line is highlighted");
    let left = rects
        .iter()
        .map(|rect| rect.rect.x)
        .fold(f32::INFINITY, f32::min);
    let right = rects
        .iter()
        .map(|rect| rect.rect.x + rect.rect.width)
        .fold(f32::NEG_INFINITY, f32::max);
    (left, right)
}

#[test]
fn a_right_to_left_paragraph_without_alignment_ends_at_the_right_edge_of_its_column() {
    let doc = document(SHALOM_OLAM, None);
    let flow = flow_of(&doc);
    let column_right = column_right(&doc, &flow);
    let (_, right) = drawn_extent(&doc, &flow);
    assert!(
        (right - column_right).abs() < 1.0,
        "the text should start at the column's right edge {column_right}, but ends at {right}"
    );
}

#[test]
fn the_caret_at_the_start_of_a_right_to_left_paragraph_sits_at_the_right_edge() {
    let doc = document(SHALOM_OLAM, None);
    let flow = flow_of(&doc);
    let column_right = column_right(&doc, &flow);
    let caret = flow
        .caret_rect(&doc, &selection(0, 0, CaretAffinity::Downstream))
        .expect("a caret at the start");
    assert!(
        (caret.rect.x - column_right).abs() < 1.0,
        "the caret at offset 0 should be at the right edge {column_right}, but is at {}",
        caret.rect.x
    );
}

#[test]
fn the_caret_at_the_end_of_a_right_to_left_paragraph_sits_at_the_left_edge_of_its_text() {
    let doc = document(SHALOM_OLAM, None);
    let flow = flow_of(&doc);
    let column_right = column_right(&doc, &flow);
    let (text_left, text_right) = drawn_extent(&doc, &flow);
    let end = SHALOM_OLAM.len();
    let caret = flow
        .caret_rect(&doc, &selection(end, end, CaretAffinity::Upstream))
        .expect("a caret at the end");
    let expected = column_right - (text_right - text_left);
    assert!(
        (caret.rect.x - expected).abs() < 1.0,
        "the caret at the end should be at the text's left edge {expected}, but is at {}",
        caret.rect.x
    );
}

#[test]
fn an_explicit_right_alignment_on_a_right_to_left_paragraph_still_wins() {
    let doc = document(SHALOM_OLAM, Some(loom_text::Alignment::Right));
    let flow = flow_of(&doc);
    let column_right = column_right(&doc, &flow);
    let (_, right) = drawn_extent(&doc, &flow);
    assert!((right - column_right).abs() < 1.0, "right edge {right}");
    let caret = flow
        .caret_rect(&doc, &selection(0, 0, CaretAffinity::Downstream))
        .expect("a caret at the start");
    assert!(
        (caret.rect.x - column_right).abs() < 1.0,
        "{}",
        caret.rect.x
    );
}

#[test]
fn an_explicit_centre_alignment_on_a_right_to_left_paragraph_still_wins() {
    let doc = document(SHALOM_OLAM, Some(loom_text::Alignment::Center));
    let flow = flow_of(&doc);
    let column_right = column_right(&doc, &flow);
    let line_x = flow.lines()[0].bounds.x;
    let (left, right) = drawn_extent(&doc, &flow);
    let gap_before = left - line_x;
    let gap_after = column_right - right;
    assert!(
        (gap_before - gap_after).abs() < 1.0,
        "centred text has equal gaps, got {gap_before} and {gap_after}"
    );
    assert!(gap_before > 1.0, "the text is not flush left: {gap_before}");
}

#[test]
fn a_left_to_right_paragraph_without_alignment_keeps_its_flush_left_geometry() {
    let doc = document("Latin text", None);
    let flow = flow_of(&doc);
    let line_x = flow.lines()[0].bounds.x;
    let (left, right) = drawn_extent(&doc, &flow);
    assert!((left - line_x).abs() < 0.01, "left edge {left} at {line_x}");
    let size = doc.page.page_style().font_size_for_kind("paragraph");
    let advance = text_advance("Latin text", 0, &[], size);
    let caret = flow
        .caret_rect(&doc, &selection(10, 10, CaretAffinity::Upstream))
        .expect("a caret at the end");
    assert!(
        (caret.rect.x - (line_x + advance)).abs() < 0.01,
        "caret {}",
        caret.rect.x
    );
    assert!(
        (right - (line_x + advance)).abs() < 0.01,
        "right edge {right}"
    );
}
