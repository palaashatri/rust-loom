use crate::{
    text_advance, visual_caret_move, CaretAffinity, PageViewport, RichBlock, TextSelection,
    VisualMove, WriterDocument,
};

fn justified_paragraph() -> WriterDocument {
    let text = format!("one two {}", "x".repeat(100));
    let mut document = WriterDocument::new("justification", "Justification");
    let mut paragraph = RichBlock::new(1, "paragraph", &text);
    paragraph.style.alignment = loom_text::Alignment::Justify;
    document.push(paragraph);
    document
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
fn justified_gap_is_shared_by_caret_selection_and_vertical_hit_test() {
    let document = justified_paragraph();
    let style = document.page.page_style();
    let column_width = style.width_pt - style.margin_left_pt - style.margin_right_pt;
    let viewport = PageViewport {
        width: style.width_pt,
        height: style.height_pt,
        zoom: 1.0,
        scroll_x: 0.0,
        scroll_y: 0.0,
    };
    let flow = document.flow(&style, viewport).expect("layout");
    let lines = flow.lines();
    assert!(
        lines.len() >= 2,
        "the long final word wraps after the first line"
    );
    let first = &lines[0];
    assert_eq!(first.start, 0);
    assert_eq!(
        document.blocks[0].text.as_str()[first.start..first.end].trim_end(),
        "one two"
    );

    let shaped = first
        .shaped_line
        .as_deref()
        .expect("the document flow keeps actual shaped geometry");
    assert!(!shaped.estimated(), "glyph geometry must be measured");
    assert!(
        (shaped.width_pt() - column_width).abs() < 0.1,
        "the justified shaped line should fill the text column"
    );
    assert!(
        shaped.natural_width_pt() < column_width,
        "the first line has room to justify"
    );
    let (space_left, space_right) = glyph_span(first, 3..4);
    let (two_left, two_right) = glyph_span(first, 4..7);
    assert!(
        (space_right - two_left).abs() < 0.01,
        "the shaped word starts at the end of its expanded space"
    );
    let expected_two_x = first.bounds.x + two_left;

    let caret = flow
        .caret_rect(&document, &TextSelection::caret(4))
        .expect("caret after the expanded word gap");
    assert!(
        (caret.rect.x - expected_two_x).abs() < 0.01,
        "caret at the start of 'two' should match its shaped glyph position: got {}, expected {expected_two_x}",
        caret.rect.x
    );

    let selected = flow
        .selection_rects(&document, &TextSelection::range(4, 7), 0..flow.page_count())
        .into_iter()
        .find(|rect| rect.start == 4 && rect.end == 7)
        .expect("selection of 'two'");
    assert!(
        (selected.rect.x - expected_two_x).abs() < 0.01,
        "selection should start at the shaped glyph position: got {}, expected {expected_two_x}",
        selected.rect.x
    );
    assert!(
        (selected.rect.width - (two_right - two_left)).abs() < 0.01,
        "selection width should cover the shaped advances for 'two'"
    );

    let gap_hit_x = first.bounds.x + space_left + (space_right - space_left) * 0.75;
    let direct_hit = shaped
        .hit_test(gap_hit_x - first.bounds.x)
        .expect("hit test in the expanded gap");
    assert_eq!(
        direct_hit.offset, 4,
        "a hit in the latter part of the expanded space is nearest the start of 'two'"
    );
    let landed = visual_caret_move(
        &document,
        first.end,
        CaretAffinity::Downstream,
        VisualMove::Up,
        Some(gap_hit_x),
    )
    .expect("vertical movement into the justified line");
    assert_eq!(
        landed.offset, direct_hit.offset,
        "vertical movement uses the retained DocumentFlow shaped hit geometry"
    );
    assert_eq!(
        landed.offset, 4,
        "a hit in the expanded gap should land at the start of 'two'"
    );

    let last = lines.last().expect("final line");
    assert_eq!(last.end, document.blocks[0].text.as_str().len());
    assert!(
        last.bounds.width < column_width,
        "the paragraph-final single-word line remains natural"
    );
}

#[test]
fn heading_default_and_explicit_bold_have_identical_wrapping_and_caret_geometry() {
    let text = "Wide headings keep their bold face while line geometry wraps words consistently across both forms";
    let mut plain = WriterDocument::new("heading-plain", "Heading");
    let mut heading = RichBlock::new(1, "heading2", text);
    heading.style.alignment = loom_text::Alignment::Justify;
    plain.push(heading);

    let mut explicit = WriterDocument::new("heading-explicit", "Heading");
    let mut heading = RichBlock::new(1, "heading2", text);
    heading.style.alignment = loom_text::Alignment::Justify;
    heading.runs.push(loom_text::StyleRun {
        start: 0,
        end: text.len(),
        style: loom_text::CharacterStyle {
            weight: loom_text::FontWeight::Bold,
            ..Default::default()
        },
    });
    explicit.push(heading);

    let style = plain.page.page_style();
    let font_size = style.font_size_for_kind("heading2");
    let explicit_bold_width = text_advance(text, 0, &explicit.blocks[0].runs, font_size);
    let unstyled_width = text_advance(text, 0, &[], font_size);
    assert!(
        (explicit_bold_width - unstyled_width).abs() > 0.5,
        "fixture must distinguish bold from regular measurement"
    );

    let viewport = PageViewport {
        width: style.width_pt,
        height: style.height_pt,
        zoom: 1.0,
        scroll_x: 0.0,
        scroll_y: 0.0,
    };
    let plain_flow = plain.flow(&style, viewport).expect("plain layout");
    let explicit_flow = explicit.flow(&style, viewport).expect("explicit layout");
    assert_eq!(
        plain_flow
            .lines()
            .iter()
            .map(|line| (line.start, line.end))
            .collect::<Vec<_>>(),
        explicit_flow
            .lines()
            .iter()
            .map(|line| (line.start, line.end))
            .collect::<Vec<_>>(),
        "a heading's implicit bold base and explicit bold run draw the same font"
    );
    for offset in (0..=text.len()).filter(|offset| text.is_char_boundary(*offset)) {
        let selection = TextSelection::caret(offset);
        let plain_caret = plain_flow
            .caret_rect(&plain, &selection)
            .map(|rect| rect.rect.x);
        let explicit_caret = explicit_flow
            .caret_rect(&explicit, &selection)
            .map(|rect| rect.rect.x);
        assert_eq!(plain_caret, explicit_caret, "caret at byte {offset}");
    }
}
