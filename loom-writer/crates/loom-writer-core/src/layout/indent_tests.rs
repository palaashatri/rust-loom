//! Left indent and line spacing of a paragraph must change the page the
//! editor draws, the caret that follows it, and the PDF export.

use crate::{PageStyle, PageViewport, RichBlock, TextSelection, WriterDocument};

fn viewport() -> PageViewport {
    PageViewport {
        width: 900.0,
        height: 900.0,
        zoom: 1.0,
        scroll_x: 0.0,
        scroll_y: 0.0,
    }
}

/// One paragraph of many words, so it wraps to several lines.
fn paragraph(indent: f32, spacing: f32) -> WriterDocument {
    let mut block = RichBlock::new(1, "paragraph", "word ".repeat(60).trim_end());
    block.style.left_indent = indent;
    block.style.line_spacing = spacing;
    let mut doc = WriterDocument::new("indent", "Indent");
    doc.blocks.push(block);
    doc
}

#[test]
fn left_indent_moves_every_line_and_narrows_its_wrap() {
    let style = PageStyle::default();
    let plain = paragraph(0.0, 1.15).flow(&style, viewport()).unwrap();
    let indented = paragraph(72.0, 1.15).flow(&style, viewport()).unwrap();
    let base_x = plain.lines()[0].bounds.x;
    let usable = style.width_pt - style.margin_left_pt - style.margin_right_pt;
    for line in indented.lines() {
        assert!(
            (line.bounds.x - (base_x + 72.0)).abs() < 0.01,
            "line starts 72 pt in: {} vs {}",
            line.bounds.x,
            base_x
        );
        // The wrap measure may let a trailing space hang by a fraction of a point,
        // exactly as it does for unindented text.
        assert!(
            line.bounds.width <= usable - 72.0 + 1.0,
            "line fits the narrower column: {}",
            line.bounds.width
        );
    }
    assert!(
        indented.lines().len() > plain.lines().len(),
        "the narrower column wraps to more lines"
    );
}

#[test]
fn the_caret_at_the_start_of_an_indented_paragraph_sits_at_the_indent() {
    let style = PageStyle::default();
    let plain = paragraph(0.0, 1.15).flow(&style, viewport()).unwrap();
    let indented = paragraph(72.0, 1.15).flow(&style, viewport()).unwrap();
    let doc = paragraph(72.0, 1.15);
    let base_x = plain.lines()[0].bounds.x;
    let caret = indented
        .caret_rect(&doc, &TextSelection::caret(0))
        .expect("a caret at the start of the paragraph");
    assert!(
        (caret.rect.x - (base_x + 72.0)).abs() < 0.01,
        "caret x {} vs indented start {}",
        caret.rect.x,
        base_x + 72.0
    );
}

#[test]
fn line_spacing_scales_the_line_box_and_the_default_is_unchanged() {
    let style = PageStyle::default();
    let default = paragraph(0.0, 1.15).flow(&style, viewport()).unwrap();
    let double = paragraph(0.0, 2.0).flow(&style, viewport()).unwrap();
    let default_height = default.lines()[0].bounds.height;
    assert!(
        (default_height - style.font_size_for_kind("paragraph") * style.line_height).abs() < 0.01,
        "the default line box is the page's own line height"
    );
    let ratio = double.lines()[0].bounds.height / default_height;
    assert!((ratio - 2.0 / 1.15).abs() < 0.001, "ratio {ratio}");
}

/// Text x position as read from the run's text matrix.
fn text_x(pdf: &[u8], text: &str) -> f32 {
    crate::export::test_support::drawn_text_runs(pdf)
        .expect("semantic text runs")
        .into_iter()
        .find(|run| run.text == text)
        .unwrap_or_else(|| panic!("{text:?} is drawn"))
        .x
}

#[test]
fn pdf_draws_an_indented_paragraph_further_in() {
    let style = PageStyle::default();
    let mut indented = WriterDocument::new("pdf", "Pdf");
    let mut block = RichBlock::new(1, "paragraph", "Indented line");
    block.style.left_indent = 72.0;
    indented.blocks.push(block);
    let mut plain = WriterDocument::new("pdf", "Pdf");
    plain
        .blocks
        .push(RichBlock::new(1, "paragraph", "Indented line"));

    let plain_x = text_x(&crate::export::export_pdf(&plain), "Indented line");
    let indented_x = text_x(&crate::export::export_pdf(&indented), "Indented line");
    assert!(
        (plain_x - style.margin_left_pt).abs() < 0.01,
        "plain text starts at the margin: {plain_x}"
    );
    assert!(
        (indented_x - (style.margin_left_pt + 72.0)).abs() < 0.01,
        "indented text starts 72 pt in: {indented_x}"
    );
}
