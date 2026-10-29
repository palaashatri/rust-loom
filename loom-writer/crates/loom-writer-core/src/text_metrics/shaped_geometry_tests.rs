use std::sync::Arc;

use loom_fonts::{
    Affinity, FallbackPolicy, FontCatalog, Paragraph, ScanConfig, StyledSpan, TextDirection,
};
use loom_text::{CharacterStyle, FontWeight, ParagraphDirection, StyleRun};

use crate::export::test_support::export_valid_pdf as export_pdf;
use crate::{fonts, test_fonts, PageViewport, RichBlock, TextSelection, WriterDocument};

struct ResetCatalog;

impl Drop for ResetCatalog {
    fn drop(&mut self) {
        fonts::reset_font_catalog();
    }
}

fn deterministic_script_catalog() -> Arc<FontCatalog> {
    let mut config = ScanConfig::with_dirs(Vec::new());
    config.policy = FallbackPolicy::new(["Noto Naskh Arabic", "Noto Sans Hebrew"]);
    Arc::new(FontCatalog::scan(&config))
}

#[test]
fn default_document_ink_and_explicit_black_share_stable_black_paint() {
    let _serial = test_fonts::serial();
    let catalog = deterministic_script_catalog();
    fonts::install_font_catalog(catalog);
    let _reset = ResetCatalog;

    let text = "default ink";
    let inherited = crate::shape_text_line(
        text,
        &[],
        16.0,
        "paragraph",
        ParagraphDirection::LeftToRight,
        loom_text::Alignment::Left,
        0..text.len(),
        300.0,
    )
    .expect("the inherited foreground line shapes");
    assert!(
        inherited
            .painted_runs()
            .iter()
            .all(|run| { run.foreground_color_explicit && run.rgba == [0, 0, 0, 255] }),
        "default document text keeps black ink regardless of the application theme"
    );

    let explicit_black = StyleRun {
        start: 0,
        end: text.len(),
        style: CharacterStyle {
            color: Some("#000000FF".to_owned()),
            ..CharacterStyle::default()
        },
    };
    let explicit = crate::shape_text_line(
        text,
        &[explicit_black],
        16.0,
        "paragraph",
        ParagraphDirection::LeftToRight,
        loom_text::Alignment::Left,
        0..text.len(),
        300.0,
    )
    .expect("the explicitly colored line shapes");
    assert!(
        explicit
            .painted_runs()
            .iter()
            .all(|run| { run.foreground_color_explicit && run.rgba == [0, 0, 0, 255] }),
        "explicit black and default document ink have the same canonical paint color"
    );
    assert_eq!(
        inherited.painted_runs()[0].rgba,
        explicit.painted_runs()[0].rgba,
        "the shared painted-run API exposes the same resolved default ink to both consumers"
    );

    let mut document = WriterDocument::new("default-ink", "Default ink");
    document.push(RichBlock::new(document.next_id(), "paragraph", text));
    let pdf = export_pdf(&document);
    let operators = loom_pdf::inspect::readable_content(&pdf).expect("read PDF operators");
    assert!(
        operators.contains("0.000 0.000 0.000 rg"),
        "default document text is exported with the same black ink: {operators}"
    );
}

#[test]
fn writer_bidi_geometry_matches_one_contextually_shaped_styled_paragraph() {
    let _serial = test_fonts::serial();
    let catalog = deterministic_script_catalog();
    fonts::install_font_catalog(catalog.clone());
    let _reset = ResetCatalog;

    let text = "مرحبا office שלום";
    let arabic_end = "مرحبا".len();
    let latin_start = arabic_end + 1;
    let latin_end = latin_start + "office".len();
    let hebrew_start = latin_end + 1;
    let latin_size = 19.0;
    let latin = StyleRun {
        start: latin_start,
        end: latin_end,
        style: CharacterStyle {
            font_size: latin_size,
            explicit_size: true,
            weight: FontWeight::Bold,
            ..CharacterStyle::default()
        },
    };
    let mut block = RichBlock::new(1, "paragraph", text);
    block.style.direction = ParagraphDirection::RightToLeft;
    block.runs.push(latin.clone());

    let regular = catalog.resolve("Inter", 400, false).expect("bundled Inter");
    let bold = catalog
        .resolve("Inter", 700, false)
        .expect("bundled Inter Bold");
    let spans = [
        StyledSpan::new(0..latin_start, &regular, 16.0),
        StyledSpan::new(latin_start..latin_end, &bold, latin_size),
        StyledSpan::new(latin_end..text.len(), &regular, 16.0),
    ];
    let paragraph = Paragraph::new(text, &spans, TextDirection::RightToLeft);
    let shaped = catalog.layout_paragraph_line(&paragraph, 0..text.len());

    let families: Vec<_> = shaped
        .runs
        .iter()
        .filter_map(|run| catalog.face(run.face).map(|face| face.family.as_str()))
        .collect();
    assert!(families.contains(&"Noto Naskh Arabic"));
    assert!(families.contains(&"Noto Sans Hebrew"));
    assert!(shaped.runs.iter().any(|run| {
        run.range == (latin_start..latin_end)
            && run.size == latin_size
            && !run.synthetic_bold
            && !run.rtl
    }));

    let writer = crate::shape_text_line(
        text,
        &block.runs,
        16.0,
        "paragraph",
        ParagraphDirection::RightToLeft,
        loom_text::Alignment::Left,
        0..text.len(),
        800.0,
    )
    .expect("Writer's paragraph pipeline should return shaped glyph geometry");
    let width_delta = (writer.width_pt() - shaped.width).abs();
    let caret_deltas: Vec<_> = text
        .char_indices()
        .filter(|(offset, _)| {
            ![
                0,
                arabic_end,
                latin_start,
                latin_end,
                hebrew_start,
                text.len(),
            ]
            .contains(offset)
        })
        .map(|(offset, _)| {
            let writer_x = writer.caret_x(offset, Affinity::Leading);
            let shaped_x = shaped.x_at_offset(offset, Affinity::Leading);
            (offset, (writer_x - shaped_x).abs())
        })
        .chain(std::iter::once((text.len(), {
            let writer_x = writer.caret_x(text.len(), Affinity::Trailing);
            let shaped_x = shaped.x_at_offset(text.len(), Affinity::Trailing);
            (writer_x - shaped_x).abs()
        })))
        .collect();
    let largest_caret_delta = caret_deltas
        .iter()
        .map(|(_, delta)| *delta)
        .fold(0.0_f32, f32::max);

    assert!(
        width_delta < 0.05 && largest_caret_delta < 0.05,
        "Writer's paragraph geometry diverges from the shared shaped paragraph: width delta={width_delta:.3}pt, largest caret delta={largest_caret_delta:.3}pt, caret deltas={caret_deltas:?}"
    );
    let selected = latin_start..latin_end;
    let expected = shaped.selection_rects(selected.clone());
    let actual = writer.selection_rects(selected);
    assert_eq!(
        actual, expected,
        "selection extents must use the shaped glyph positions"
    );
    for selection in [
        0..text.len(),
        0..arabic_end,
        latin_start..latin_end,
        latin_end..text.len(),
    ] {
        let expected = shaped.selection_rects(selection.clone());
        let actual = writer.selection_rects(selection);
        assert_eq!(
            actual, expected,
            "mixed-direction selection must retain visual spans"
        );
    }
}

#[test]
fn document_flow_keeps_the_same_shaped_line_for_width_caret_and_selection() {
    let _serial = test_fonts::serial();
    let catalog = deterministic_script_catalog();
    fonts::install_font_catalog(catalog.clone());
    let _reset = ResetCatalog;

    let text = "مرحبا office שלום";
    let latin_start = "مرحبا ".len();
    let latin_end = latin_start + "office".len();
    let mut block = RichBlock::new(1, "paragraph", text);
    block.style.direction = ParagraphDirection::RightToLeft;
    block.style.alignment = loom_text::Alignment::Left;
    block.runs.push(StyleRun {
        start: latin_start,
        end: latin_end,
        style: CharacterStyle {
            font_size: 19.0,
            explicit_size: true,
            weight: FontWeight::Bold,
            ..CharacterStyle::default()
        },
    });
    let mut doc = WriterDocument::new("mixed", "Mixed direction");
    doc.blocks.push(block);
    let mut style = doc.page.page_style();
    style.body_font_size_pt = 16.0;
    let flow = doc
        .flow(
            &style,
            PageViewport {
                width: style.width_pt,
                height: style.height_pt,
                zoom: 1.0,
                scroll_x: 0.0,
                scroll_y: 0.0,
            },
        )
        .expect("valid page layout");
    let line = flow.lines().first().expect("one unwrapped line");
    let shaped = line
        .shaped_line
        .as_ref()
        .expect("flow must retain its authoritative shaped line");
    let independent = catalog.resolve("Inter", 400, false).expect("Inter");
    let independent_bold = catalog.resolve("Inter", 700, false).expect("Inter Bold");
    let spans = [
        StyledSpan::new(0..latin_start, &independent, 16.0),
        StyledSpan::new(latin_start..latin_end, &independent_bold, 19.0),
        StyledSpan::new(latin_end..text.len(), &independent, 16.0),
    ];
    let paragraph = Paragraph::new(text, &spans, TextDirection::RightToLeft);
    let expected = catalog.layout_paragraph_line(&paragraph, 0..text.len());
    assert!((shaped.width_pt() - expected.width).abs() < 0.05);
    let selection = doc.selection_rectangles_for(
        &doc.layout(
            &style,
            PageViewport {
                width: style.width_pt,
                height: style.height_pt,
                zoom: 1.0,
                scroll_x: 0.0,
                scroll_y: 0.0,
            },
        )
        .expect("page layout"),
        &style,
        &loom_document::TextSelection::range(latin_start, latin_end),
    );
    assert!(
        !selection.is_empty(),
        "flow selection uses shaped run geometry"
    );
    for offset in [latin_start, latin_start + 1, latin_end - 1, latin_end] {
        let actual = shaped.caret_x(offset, Affinity::Leading);
        let expected_x = expected.x_at_offset(offset, Affinity::Leading);
        assert!((actual - expected_x).abs() < 0.05, "caret at {offset}");
    }
    for stop in expected.caret_positions() {
        assert!(
            (shaped.caret_x(stop.offset, stop.affinity) - stop.x).abs() < 0.05,
            "Writer must preserve {:?} affinity at byte {}",
            stop.affinity,
            stop.offset
        );
    }
}

#[test]
fn script_fallback_wrapping_uses_shaped_widths_and_preserves_graphemes() {
    let _serial = test_fonts::serial();
    let catalog = deterministic_script_catalog();
    fonts::install_font_catalog(catalog.clone());
    let _reset = ResetCatalog;

    let text = "مرحباالسلامعليكم";
    let base_size = 20.0;
    let mut block = RichBlock::new(1, "paragraph", text);
    block.style.direction = ParagraphDirection::RightToLeft;
    let regular = catalog.resolve("Inter", 400, false).expect("Inter");
    let spans = [StyledSpan::new(0..text.len(), &regular, base_size)];
    let paragraph = Paragraph::new(text, &spans, TextDirection::RightToLeft);
    let width = 75.0;

    let wrapped = crate::shape_wrapped_block_lines(&block, base_size, width)
        .expect("the installed script faces shape this paragraph");
    assert!(wrapped.len() > 1, "the narrow column wraps the Arabic word");
    let mut next = 0;
    for (range, shaped) in &wrapped {
        assert_eq!(range.start, next, "line ranges cover the source in order");
        assert!(text.is_char_boundary(range.start) && text.is_char_boundary(range.end));
        let line = shaped.as_ref().expect("nonempty line has shaped geometry");
        let independent = catalog.layout_paragraph_line(&paragraph, range.clone());
        assert!(
            (line.natural_width_pt() - independent.width).abs() < 0.05,
            "Writer retains the independent shaped width for {:?}",
            &text[range.clone()]
        );
        assert!(
            line.natural_width_pt() <= width + 0.1,
            "shaped line {:?} fits its {width} pt column: {} pt",
            &text[range.clone()],
            line.natural_width_pt()
        );
        next = range.end;
    }
    assert_eq!(next, text.len(), "line ranges retain every source byte");

    let style = crate::PageStyle {
        width_pt: width,
        margin_left_pt: 0.0,
        margin_right_pt: 0.0,
        body_font_size_pt: base_size,
        ..crate::PageStyle::default()
    };
    let mut document = WriterDocument::new("wrapped", "Wrapped");
    document.blocks.push(block);
    let flow = document
        .flow(
            &style,
            PageViewport {
                width: style.width_pt,
                height: style.height_pt,
                zoom: 1.0,
                scroll_x: 0.0,
                scroll_y: 0.0,
            },
        )
        .expect("valid narrow page geometry");
    assert_eq!(
        flow.lines()
            .iter()
            .map(|line| line.start..line.end)
            .collect::<Vec<_>>(),
        wrapped
            .iter()
            .map(|(range, _)| range.clone())
            .collect::<Vec<_>>(),
        "pagination, screen geometry, caret and hit-test share the shaped wraps"
    );
}

#[test]
fn shaped_cell_wrapping_keeps_consecutive_and_trailing_explicit_lines() {
    let _serial = test_fonts::serial();
    let catalog = deterministic_script_catalog();
    fonts::install_font_catalog(catalog);
    let _reset = ResetCatalog;

    let block = RichBlock::new(3, "paragraph", "one\n\ntwo\n");
    let lines = crate::shape_wrapped_block_lines(&block, 16.0, 300.0)
        .expect("explicit line breaks remain valid layout");
    assert_eq!(
        lines
            .iter()
            .map(|(range, _)| range.clone())
            .collect::<Vec<_>>(),
        [0..4, 4..5, 5..9, 9..9],
        "each explicit break, including the trailing one, preserves a visual line"
    );
}

#[test]
fn crlf_is_one_mandatory_break_even_when_the_line_would_fit() {
    let _serial = test_fonts::serial();
    let catalog = deterministic_script_catalog();
    fonts::install_font_catalog(catalog);
    let _reset = ResetCatalog;

    let text = "first\r\nsecond";
    let break_start = "first".len();
    let break_end = break_start + "\r\n".len();
    let block = RichBlock::new(9, "paragraph", text);
    let wrapped = crate::shape_wrapped_block_lines(&block, 16.0, 300.0)
        .expect("the wide column shapes both CRLF-separated lines");

    assert_eq!(
        wrapped
            .iter()
            .map(|(range, _)| range.clone())
            .collect::<Vec<_>>(),
        [0..break_end, break_end..text.len()],
        "CRLF is consumed as one mandatory break, not painted or wrapped as content"
    );
    for (range, shaped) in &wrapped {
        let line = shaped
            .as_ref()
            .expect("each nonempty visual line has shaped glyph geometry");
        assert!(
            line.painted_runs()
                .iter()
                .all(|run| run.shaped.glyphs.iter().all(|glyph| {
                    let source = run.source_range.start + glyph.cluster;
                    !(break_start..break_end).contains(&source)
                })),
            "visual line {range:?} contains no CR or LF glyph"
        );
    }
}

#[test]
fn soft_break_remains_in_source_range_without_becoming_a_painted_glyph() {
    let _serial = test_fonts::serial();
    let catalog = deterministic_script_catalog();
    fonts::install_font_catalog(catalog);
    let _reset = ResetCatalog;

    for (id, text, before, marker) in [
        (12, "One\u{b}Two", "One", "\u{b}"),
        (13, "One\nTwo", "One", "\n"),
    ] {
        let marker_start = before.len();
        let marker_end = marker_start + marker.len();
        let block = RichBlock::new(id, "paragraph", text);
        let wrapped = crate::shape_wrapped_block_lines(&block, 16.0, 300.0)
            .expect("the hard-break paragraph shapes");
        assert_eq!(wrapped.len(), 2);
        assert_eq!(wrapped[0].0, 0..marker_end);
        let line = wrapped[0]
            .1
            .as_ref()
            .expect("the first visual line has shaped geometry");
        assert_eq!(line.source_range(), 0..marker_end);
        assert!(
            line.painted_runs()
                .iter()
                .all(|run| run.shaped.glyphs.iter().all(|glyph| {
                    let source = run.source_range.start + glyph.cluster;
                    !(marker_start..marker_end).contains(&source)
                })),
            "the {marker:?} source marker stays in the line range but is not rasterized"
        );
    }
}

#[test]
fn rtl_justification_expands_only_nonfinal_lines_and_right_aligns_the_rest() {
    let _serial = test_fonts::serial();
    let catalog = deterministic_script_catalog();
    fonts::install_font_catalog(catalog.clone());
    let _reset = ResetCatalog;

    let wrapped_text = "مرحبا بالعالم مرحبا";
    let pair_end = "مرحبا بالعالم".len();
    let regular = catalog.resolve("Inter", 400, false).expect("bundled Inter");
    let spans = [StyledSpan::new(0..wrapped_text.len(), &regular, 16.0)];
    let paragraph = Paragraph::new(wrapped_text, &spans, TextDirection::RightToLeft);
    let pair_width = catalog.layout_paragraph_line(&paragraph, 0..pair_end).width;
    let available_width = pair_width + 12.0;
    let mut block = RichBlock::new(10, "paragraph", wrapped_text);
    block.style.direction = ParagraphDirection::RightToLeft;
    block.style.alignment = loom_text::Alignment::Justify;
    let wrapped = crate::shape_wrapped_block_lines(&block, 16.0, available_width)
        .expect("the mixed-script paragraph wraps with its resolved script faces");
    assert_eq!(wrapped.len(), 2, "the fixture ends with one final word");
    let first = wrapped[0]
        .1
        .as_ref()
        .expect("the justified first line has shaped geometry");
    let final_line = wrapped[1]
        .1
        .as_ref()
        .expect("the final word has shaped geometry");
    assert_eq!(
        wrapped_text[wrapped[0].0.clone()].trim_end(),
        "مرحبا بالعالم",
        "the first visual line contains the two-word Arabic phrase"
    );
    assert!(
        (first.width_pt() - available_width).abs() < 0.05,
        "the nonfinal RTL line expands its eligible word gap to the column edge"
    );
    assert!(
        final_line.alignment_shift_pt() > 1.0,
        "the final single-word RTL line aligns to the paragraph start at the right edge"
    );
    assert!(
        (final_line.width_pt() + final_line.alignment_shift_pt() - available_width).abs() < 0.05,
        "the final line's natural width plus its right-alignment shift reaches the column edge"
    );

    let mut document = WriterDocument::new("rtl-justify-flow", "RTL justification flow");
    document.blocks.push(block.clone());
    let mut style = document.page.page_style();
    style.width_pt = available_width;
    style.margin_left_pt = 0.0;
    style.margin_right_pt = 0.0;
    style.body_font_size_pt = 16.0;
    let viewport = PageViewport {
        width: style.width_pt,
        height: style.height_pt,
        zoom: 1.0,
        scroll_x: 0.0,
        scroll_y: 0.0,
    };
    let flow = document
        .flow(&style, viewport)
        .expect("the RTL justified block flows at the same column width");
    let flow_final = flow.lines().last().expect("the final visual line");
    let flow_shaped = flow_final
        .shaped_line
        .as_ref()
        .expect("the final line retains shaped geometry");
    let expected_start_x =
        flow_final.bounds.x + flow_shaped.caret_x(flow_final.start, Affinity::Leading);
    let mut downstream_caret = TextSelection::caret(flow_final.global_start);
    downstream_caret.affinity = crate::CaretAffinity::Downstream;
    let caret = flow
        .caret_rect(&document, &downstream_caret)
        .expect("caret geometry for the final RTL paragraph line");
    assert!(
        (caret.rect.x - expected_start_x).abs() < 0.05,
        "the final RTL caret follows the right-aligned shaped start: {} vs {expected_start_x}",
        caret.rect.x
    );

    let explicit_text = "مرحبا\n";
    let mut explicit = RichBlock::new(11, "paragraph", explicit_text);
    explicit.style.direction = ParagraphDirection::RightToLeft;
    explicit.style.alignment = loom_text::Alignment::Justify;
    let explicit_lines = crate::shape_wrapped_block_lines(&explicit, 16.0, available_width)
        .expect("the explicitly broken RTL paragraph shapes");
    let explicit_line = explicit_lines[0]
        .1
        .as_ref()
        .expect("the explicit-break line has glyph geometry");
    assert!(
        explicit_line.alignment_shift_pt() > 1.0,
        "an explicit-break RTL line without eligible gaps aligns right"
    );

    let single_text = "مرحبا";
    let single_width = 300.0;
    let single = crate::shape_text_line(
        single_text,
        &[],
        16.0,
        "paragraph",
        ParagraphDirection::RightToLeft,
        loom_text::Alignment::Justify,
        0..single_text.len(),
        single_width,
    )
    .expect("the single RTL word shapes");
    assert!(
        single.alignment_shift_pt() > 1.0,
        "a single-word RTL justified paragraph aligns right"
    );

    let left = crate::shape_text_line(
        single_text,
        &[],
        16.0,
        "paragraph",
        ParagraphDirection::RightToLeft,
        loom_text::Alignment::Left,
        0..single_text.len(),
        single_width,
    )
    .expect("the physical-left RTL line shapes");
    let right = crate::shape_text_line(
        single_text,
        &[],
        16.0,
        "paragraph",
        ParagraphDirection::RightToLeft,
        loom_text::Alignment::Right,
        0..single_text.len(),
        single_width,
    )
    .expect("the physical-right RTL line shapes");
    assert_eq!(left.alignment_shift_pt(), 0.0, "Left remains physical Left");
    assert!(
        (right.alignment_shift_pt() - (single_width - right.width_pt())).abs() < 0.05,
        "Right remains physical Right in an RTL paragraph"
    );
}
