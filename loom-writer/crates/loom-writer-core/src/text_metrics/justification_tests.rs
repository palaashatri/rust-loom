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

#[test]
fn justified_gap_is_shared_by_caret_selection_and_vertical_hit_test() {
    let document = justified_paragraph();
    let style = document.page.page_style();
    let column_width = style.width_pt - style.margin_left_pt - style.margin_right_pt;
    let font_size = style.font_size_for_kind("paragraph");
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

    let natural_word = text_advance("one two", 0, &[], font_size);
    let natural_prefix = text_advance("one ", 0, &[], font_size);
    let extra = column_width - natural_word;
    assert!(extra > 1.0, "the first line has room to justify");
    let expected_two_x = first.bounds.x + natural_prefix + extra;
    let natural_two = text_advance("two", 4, &[], font_size);
    assert!(
        (expected_two_x + natural_two - first.bounds.x - column_width).abs() < 0.1,
        "the last word on the justified line should reach the column edge"
    );

    let caret = flow
        .caret_rect(&document, &TextSelection::caret(4))
        .expect("caret after the expanded word gap");
    assert!(
        (caret.rect.x - expected_two_x).abs() < 0.1,
        "caret at the start of 'two' should include the expanded gap: got {}, expected {expected_two_x}",
        caret.rect.x
    );

    let selected = flow
        .selection_rects(&document, &TextSelection::range(4, 7), 0..flow.page_count())
        .into_iter()
        .find(|rect| rect.start == 4 && rect.end == 7)
        .expect("selection of 'two'");
    assert!(
        (selected.rect.x - expected_two_x).abs() < 0.1,
        "selection should start after the expanded gap: got {}, expected {expected_two_x}",
        selected.rect.x
    );

    let gap_hit_x = first.bounds.x + natural_prefix + extra * 0.75;
    let landed = visual_caret_move(
        &document,
        first.end,
        CaretAffinity::Downstream,
        VisualMove::Up,
        Some(gap_hit_x),
    )
    .expect("vertical movement into the justified line");
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
