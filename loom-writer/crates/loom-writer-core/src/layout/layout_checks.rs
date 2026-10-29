//! Cached and windowed layout against the original implementation in
//! `layout_tests`, plus the cache and flooring shortcuts that make edits cheap.

use super::selection::floor_in_blocks;
use super::*;
use crate::{floor_grapheme_boundary, grapheme_boundaries, CaretAffinity};

fn sample_document(blocks: usize) -> WriterDocument {
    let mut doc = WriterDocument::new("layout", "Layout");
    let words = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu";
    let text = (0..blocks)
        .map(|n| match n % 9 {
            4 => String::new(),
            7 => "supercalifragilisticexpialidocious".repeat(9),
            _ => format!("{n} {}", words.repeat(1 + n % 5)),
        })
        .collect::<Vec<_>>()
        .join("\n");
    doc.replace_paragraphs(&text);
    let mut offset = 0usize;
    let mut starts = Vec::new();
    for block in &doc.blocks {
        starts.push(offset);
        offset += block.text.len_bytes() + 1;
    }
    for (n, &start) in starts.iter().enumerate() {
        let selection = TextSelection::caret(start);
        match n % 11 {
            0 => crate::set_selection_heading(&mut doc, selection, 1),
            3 => crate::set_selection_heading(&mut doc, selection, 3),
            5 => crate::set_selection_alignment(&mut doc, selection, 1),
            8 => crate::set_selection_alignment(&mut doc, selection, 2),
            _ => {}
        }
        if n % 6 == 0 && doc.blocks[n].text.len_bytes() > 14 {
            crate::set_selection_bold(&mut doc, TextSelection::range(start + 2, start + 12), true);
        }
    }
    doc
}

fn viewport(zoom: f32, scroll_y: f32) -> PageViewport {
    let style = PageStyle::default();
    PageViewport {
        width: style.width_pt,
        height: style.height_pt,
        zoom,
        scroll_x: 0.0,
        scroll_y,
    }
}

#[test]
fn layout_and_pagination_match_the_reference_implementation() {
    let doc = sample_document(160);
    let style = doc.page.page_style();
    assert!(
        doc.reference_paginate(&style).unwrap().len() > 3,
        "several pages"
    );
    assert_eq!(doc.paginate(&style), doc.reference_paginate(&style));
    for (zoom, scroll) in [(1.0, 0.0), (1.5, 700.0), (0.5, 90.0)] {
        let mut doc = doc.clone();
        for caret in [0usize, 57, 4000, 9000] {
            doc.set_selection(TextSelection::caret(caret));
            let want = doc
                .reference_layout(&style, viewport(zoom, scroll))
                .unwrap();
            let got = doc.layout(&style, viewport(zoom, scroll)).unwrap();
            assert_eq!(got, want, "zoom {zoom} scroll {scroll} caret {caret}");
        }
        doc.set_selection(TextSelection::range(30, 6000));
        let want = doc
            .reference_layout(&style, viewport(zoom, scroll))
            .unwrap();
        assert_eq!(doc.layout(&style, viewport(zoom, scroll)).unwrap(), want);
    }
    let mut invalid = style.clone();
    invalid.line_height = 0.0;
    assert_eq!(doc.paginate(&invalid), doc.reference_paginate(&invalid));
    let mut bad_view = viewport(1.0, 0.0);
    bad_view.zoom = 0.0;
    assert_eq!(
        doc.layout(&style, bad_view).unwrap_err(),
        doc.reference_layout(&style, bad_view).unwrap_err()
    );
}

#[test]
fn layout_bounds_match_the_shaped_visual_line_width() {
    let doc = sample_document(1);
    let style = doc.page.page_style();
    let flow = doc.flow(&style, viewport(1.0, 0.0)).unwrap();
    let line = flow
        .lines()
        .iter()
        .find(|line| line.shaped_line.is_some())
        .expect("the fixture has a shaped line");
    assert_eq!(
        line.bounds.width,
        line.shaped_line.as_ref().unwrap().width_pt(),
        "the line box follows the width used to place its shaped glyphs"
    );
}

#[test]
fn selection_rectangles_match_shaped_flow_for_every_caret_and_many_ranges() {
    let doc = sample_document(40);
    let style = doc.page.page_style();
    let layout = doc.layout(&style, viewport(1.0, 0.0)).unwrap();
    let flow = doc.flow(&style, viewport(1.0, 0.0)).unwrap();
    let total = doc.editor_text().len();
    for affinity in [CaretAffinity::Upstream, CaretAffinity::Downstream] {
        for offset in (0..=total).step_by(3) {
            let mut selection = TextSelection::caret(offset);
            selection.affinity = affinity;
            assert_eq!(
                doc.selection_rectangles_for(&layout, &style, &selection),
                flow.selection_rects(&doc, &selection, 0..flow.page_count()),
                "caret {offset} {affinity:?}"
            );
        }
    }
    for (a, b) in [
        (0, total),
        (5, 5),
        (10, 400),
        (400, 10),
        (total - 3, total + 50),
    ] {
        let selection = TextSelection::range(a, b);
        assert_eq!(
            doc.selection_rectangles_for(&layout, &style, &selection),
            flow.selection_rects(&doc, &selection, 0..flow.page_count()),
            "range {a}..{b}"
        );
    }
}

#[test]
fn windowed_rectangles_are_the_full_ones_on_the_window_pages() {
    let doc = sample_document(200);
    let style = doc.page.page_style();
    let flow = doc.flow(&style, viewport(1.0, 0.0)).unwrap();
    assert!(flow.page_count() > 6);
    let selection = TextSelection::range(100, doc.editor_text().len() - 100);
    let all = flow.selection_rects(&doc, &selection, 0..flow.page_count());
    for window in [0..1, 2..4, flow.page_count() - 1..flow.page_count()] {
        let part = flow.selection_rects(&doc, &selection, window.clone());
        let expected: Vec<_> = all
            .iter()
            .filter(|rect| window.contains(&rect.page_index))
            .cloned()
            .collect();
        assert!(!expected.is_empty());
        assert_eq!(part, expected, "pages {window:?}");
    }
    let caret = TextSelection::caret(5000);
    let rect = flow.caret_rect(&doc, &caret).expect("caret rect");
    let reference = doc.reference_layout(&style, viewport(1.0, 0.0)).unwrap();
    assert_eq!(
        Some(&rect),
        doc.reference_rects_for(&reference, &style, &caret).first()
    );
}

#[test]
fn the_window_lists_pages_near_the_viewport_and_is_never_empty() {
    let doc = sample_document(200);
    let style = doc.page.page_style();
    let scroll = 3.0 * (style.height_pt + PAGE_GAP_PT);
    let flow = doc.flow(&style, viewport(1.0, scroll)).unwrap();
    let pages = flow.visible_pages(0.0, 800.0);
    assert!(pages.contains(&3), "{pages:?}");
    assert!(pages.len() <= 2, "{pages:?}");
    assert!(flow
        .lines_on(pages.clone())
        .iter()
        .all(|line| pages.contains(&line.page)));
    assert_eq!(
        flow.visible_pages(1.0e9, 2.0e9),
        flow.page_count() - 1..flow.page_count()
    );
    assert_eq!(flow.visible_pages(-2.0e9, -1.0e9), 0..1);
}

#[test]
fn an_edit_wraps_only_the_changed_block_and_counts_follow() {
    // Replacing the font catalogue (another test's fixture) re-wraps everything.
    let _serial = crate::test_fonts::serial();
    let mut doc = sample_document(600);
    let style = doc.page.page_style();
    let view = viewport(1.0, 0.0);
    let _ = doc.flow(&style, view).unwrap();
    let before = WRAP_MISSES.with(std::cell::Cell::get);
    let _ = doc.flow(&style, view).unwrap();
    assert_eq!(
        WRAP_MISSES.with(std::cell::Cell::get),
        before,
        "an unchanged document re-wraps nothing"
    );
    let (words, chars) = doc.text_counts();
    let text = doc.editor_text();
    assert_eq!(words, text.split_whitespace().count());
    assert_eq!(chars, text.chars().count());

    let mut edited = text.clone();
    edited.insert_str(edited.len() / 2, "ZZ");
    doc.replace_editor_text(&edited).unwrap();
    let mid = WRAP_MISSES.with(std::cell::Cell::get);
    let _ = doc.flow(&style, view).unwrap();
    assert_eq!(
        WRAP_MISSES.with(std::cell::Cell::get) - mid,
        1,
        "one block changed"
    );
    let (words, chars) = doc.text_counts();
    assert_eq!(words, edited.split_whitespace().count());
    assert_eq!(chars, edited.chars().count());
    assert_eq!(doc.reference_paginate(&style), doc.paginate(&style));
}

#[test]
fn justified_mixed_paragraph_defers_every_wrapped_line_with_spaces() {
    let text = format!(
        "אבג {}",
        "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu ".repeat(20)
    );
    let mut document = WriterDocument::new("bidi-justify", "Bidi justification");
    let mut paragraph = RichBlock::new(1, "paragraph", &text);
    paragraph.style.alignment = loom_text::Alignment::Justify;
    document.push(paragraph);

    let style = document.page.page_style();
    let viewport = viewport(1.0, 0.0);
    let flow = document
        .flow(&style, viewport)
        .expect("mixed paragraph layout");
    let font_size = style.font_size_for_kind("paragraph");
    let width = document.blocks[0].text_column_width(&style);
    let line = flow
        .lines()
        .iter()
        .find(|line| {
            let source = &text[line.start..line.end];
            line.end < text.len()
                && source.contains(' ')
                && !source
                    .chars()
                    .any(|ch| ('\u{0590}'..='\u{08ff}').contains(&ch))
        })
        .expect("a later wrapped line contains only left-to-right words");

    let (spaces, deferred) =
        document.blocks[0].justification_spaces_for_line(line.start..line.end, font_size, width);
    assert!(deferred, "RTL paragraph context applies to this whole line");
    assert!(
        spaces.is_empty(),
        "the line stays at natural width until bidi spacing is supported"
    );
}

#[test]
fn fallback_caret_keeps_explicit_rtl_justification_alignment() {
    let _serial = crate::test_fonts::serial();
    let mut block = crate::RichBlock::new(89, "paragraph", "Latin");
    block.style.direction = loom_text::ParagraphDirection::RightToLeft;
    block.style.alignment = loom_text::Alignment::Justify;
    let mut doc = WriterDocument::new("rtl-fallback-caret", "RTL fallback caret");
    doc.blocks.push(block);
    let mut style = doc.page.page_style();
    style.width_pt = 300.0;
    style.margin_left_pt = 0.0;
    style.margin_right_pt = 0.0;
    style.body_font_size_pt = 16.0;
    let mut flow = doc.flow(&style, viewport(1.0, 0.0)).unwrap();
    let line = flow.lines().first().expect("one line is laid out");
    let shaped = line
        .shaped_line
        .as_ref()
        .expect("the valid paragraph first has shaped geometry");
    let expected_x = line.bounds.x + shaped.caret_x(line.start, loom_fonts::Affinity::Leading);
    let selection = TextSelection::caret(line.global_start);
    let shaped_caret = flow
        .caret_rect(&doc, &selection)
        .expect("the shaped line has a caret");
    assert!((shaped_caret.rect.x - expected_x).abs() < 0.05);

    // Simulate a valid flow whose transient shaping result was unavailable;
    // exercise the actual fallback branch without changing document content.
    flow.lines[0].shaped_line = None;
    let fallback_caret = flow
        .caret_rect(&doc, &selection)
        .expect("the fallback line still has a caret");
    assert!(
        (fallback_caret.rect.x - expected_x).abs() < 0.05,
        "fallback caret x {} should preserve the shaped explicit-RTL position {expected_x}",
        fallback_caret.rect.x
    );
}

#[test]
fn cached_flow_measurements_are_reused_for_caret_selection_and_projection_geometry() {
    let text = format!(
        "אבג {}",
        "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu ".repeat(12)
    );
    let mut document = WriterDocument::new("cached-justify", "Cached justification");
    let mut paragraph = RichBlock::new(1, "paragraph", &text);
    paragraph.style.alignment = loom_text::Alignment::Justify;
    document.push(paragraph);

    let style = document.page.page_style();
    let viewport = viewport(1.0, 0.0);
    let flow = document.flow(&style, viewport).expect("initial layout");
    let measure_builds = crate::text_metrics::justification::measure_build_count();
    let bidi_scans = crate::text_metrics::bidi::paragraph_scan_count();
    let block = &document.blocks[0];
    let font_size = style.font_size_for_kind("paragraph");
    let width = block.text_column_width(&style);

    for line in flow.lines() {
        block.justification_spaces_for_line(line.start..line.end, font_size, width);
        block.line_offset_at_x_with_justification(line.start..line.end, font_size, width, 80.0);
        block.measured_advance(line.start, line.end, font_size);
    }
    let first = flow.lines().first().expect("wrapped line");
    flow.caret_rect(&document, &TextSelection::caret(first.global_start + 1))
        .expect("caret geometry");
    flow.selection_rects(
        &document,
        &TextSelection::range(first.global_start, first.global_start + 3),
        0..flow.page_count(),
    );
    document.flow(&style, viewport).expect("cached relayout");

    assert_eq!(
        crate::text_metrics::justification::measure_build_count(),
        measure_builds,
        "caret, selection, projection line geometry, and unchanged flow reuse the cached measure"
    );
    assert_eq!(
        crate::text_metrics::bidi::paragraph_scan_count(),
        bidi_scans,
        "mixed-paragraph bidi classification is reused after layout"
    );
}

#[test]
fn justified_selection_and_upstream_caret_do_not_include_hanging_break_spaces() {
    let mut failures = Vec::new();
    for line_break in ["\n", "\u{b}", "\u{c}"] {
        let text = format!("前置{line_break}one two {}", "x".repeat(180));
        let mut document = WriterDocument::new("hanging-space", "Hanging space");
        let mut paragraph = RichBlock::new(1, "paragraph", &text);
        paragraph.style.alignment = loom_text::Alignment::Justify;
        document.push(paragraph);

        let style = document.page.page_style();
        let flow = document
            .flow(&style, viewport(1.0, 0.0))
            .expect("layout with an explicit line break");
        let break_end = "前置".len() + line_break.len();
        let line = flow
            .lines()
            .iter()
            .find(|line| {
                line.start >= break_end
                    && line.end < text.len()
                    && text[line.start..line.end].ends_with(' ')
                    && text[line.start..line.end].contains("one two")
            })
            .expect("an automatic line ends with its hanging break space");
        assert!(line.start > 0, "the source range has a UTF-8 nonzero start");
        assert!(text.is_char_boundary(line.start));
        assert!(text.is_char_boundary(line.end));

        let global_end = line.global_start + line.end - line.start;
        let selection = TextSelection::range(line.global_start, global_end);
        let highlight = flow
            .selection_rects(&document, &selection, 0..flow.page_count())
            .into_iter()
            .find(|rect| rect.start == line.start && rect.end == line.end)
            .expect("selected wrapped line");
        let mut caret = TextSelection::caret(global_end);
        caret.affinity = CaretAffinity::Upstream;
        let caret_rect = flow.caret_rect(&document, &caret).expect("upstream caret");
        let right = line.bounds.x + line.bounds.width;

        if !(highlight.rect.x + highlight.rect.width <= right + 0.1
            && caret_rect.rect.x <= right + 0.1)
        {
            failures.push(format!(
                "separator {line_break:?}: line end {right}, selection right {}, upstream caret x {}",
                highlight.rect.x + highlight.rect.width,
                caret_rect.rect.x,
            ));
        }

        let hard_break_line = flow
            .lines()
            .iter()
            .find(|line| text[line.start..line.end].ends_with(line_break))
            .expect("layout keeps the explicit break on its preceding line");
        let visible = crate::text_metrics::bidi::visible_end(
            &text,
            hard_break_line.start..hard_break_line.end,
        );
        if visible != hard_break_line.end - line_break.len() {
            failures.push(format!(
                "separator {line_break:?}: visible endpoint {visible}, expected {}",
                hard_break_line.end - line_break.len(),
            ));
        }

        let final_break = format!("final{line_break}");
        let final_visible =
            crate::text_metrics::bidi::visible_end(&final_break, 0..final_break.len());
        if final_visible != "final".len() {
            failures.push(format!(
                "final separator {line_break:?}: visible endpoint {final_visible}, expected {}",
                "final".len(),
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "justified geometry must stop at visible content:\n{}",
        failures.join("\n")
    );
}

#[test]
fn natural_wrapped_line_keeps_its_trailing_space_caret_stop() {
    let text = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu ".repeat(12);
    let mut document = WriterDocument::new("natural-hanging-space", "Natural hanging space");
    document.push(RichBlock::new(1, "paragraph", &text));

    let style = document.page.page_style();
    let flow = document
        .flow(&style, viewport(1.0, 0.0))
        .expect("natural paragraph layout");
    let line = flow
        .lines()
        .iter()
        .find(|line| {
            line.start > 0 && line.end < text.len() && text[line.start..line.end].ends_with(' ')
        })
        .expect("a noninitial automatic line with a hanging space");
    assert!(text.is_char_boundary(line.start));
    assert!(text.is_char_boundary(line.end));

    let block = &document.blocks[line.block_index];
    let font_size = style.font_size_for_kind(block.kind.as_str());
    let width = block.text_column_width(&style);
    let local_width = block.measured_advance(line.start, line.end, font_size);
    let hit = block.line_offset_at_x_with_justification(
        line.start..line.end,
        font_size,
        width,
        local_width,
    );
    let global_end = line.global_start + line.end - line.start;
    let selection = TextSelection::range(line.global_start, global_end);
    let highlight = flow
        .selection_rects(&document, &selection, 0..flow.page_count())
        .into_iter()
        .find(|rect| rect.start == line.start && rect.end == line.end)
        .expect("selected natural line");
    let mut caret = TextSelection::caret(global_end);
    caret.affinity = CaretAffinity::Upstream;
    let caret_rect = flow.caret_rect(&document, &caret).expect("upstream caret");
    let right = line.bounds.x + line.bounds.width;
    let mut failures = Vec::new();
    if hit != line.end - line.start {
        failures.push(format!(
            "hit offset {hit}, expected trailing-space boundary {}",
            line.end - line.start
        ));
    }

    let mut previous = 0;
    for step in 0..=32 {
        let x = -8.0 + (local_width + 16.0) * step as f32 / 32.0;
        let offset =
            block.line_offset_at_x_with_justification(line.start..line.end, font_size, width, x);
        if offset < previous {
            failures.push(format!(
                "natural hit offsets are nonmonotonic at x {x}: {offset} followed {previous}"
            ));
            break;
        }
        previous = offset;
    }
    if (highlight.rect.x + highlight.rect.width - right).abs() > 0.1 {
        failures.push(format!(
            "selection right {}, line right {right}",
            highlight.rect.x + highlight.rect.width
        ));
    }
    if (caret_rect.rect.x - right).abs() > 0.1 {
        failures.push(format!(
            "upstream caret x {}, line right {right}",
            caret_rect.rect.x
        ));
    }
    assert!(
        failures.is_empty(),
        "natural hanging-space geometry:\n{}",
        failures.join("\n")
    );
}

#[test]
fn counts_of_empty_and_single_blocks_match_the_joined_text() {
    let mut doc = WriterDocument::new("c", "C");
    assert_eq!(doc.text_counts(), (0, 0));
    doc.replace_paragraphs("héllo wörld");
    assert_eq!(doc.text_counts(), (2, 11));
    doc.replace_paragraphs("a\n\nb c\n");
    let text = doc.editor_text();
    assert_eq!(
        doc.text_counts(),
        (text.split_whitespace().count(), text.chars().count())
    );
}

fn whole_text_floor(text: &str, offset: usize) -> usize {
    let offset = offset.min(text.len());
    grapheme_boundaries(text)
        .into_iter()
        .take_while(|boundary| *boundary <= offset)
        .last()
        .unwrap_or(0)
}

#[test]
fn flooring_to_a_grapheme_boundary_matches_whole_text_segmentation() {
    let samples = [
        "",
        "plain ascii\nsecond line\n\nfourth",
        "e\u{301}clair\nna\u{ef}ve\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467} family\n",
        "\u{1F1FA}\u{1F1F8}\u{1F1EB}\u{1F1F7}\nflags",
        "crlf\r\nline\r\nend",
        "\n\n\n",
        "trailing newline\n",
    ];
    for text in samples {
        for offset in 0..=text.len() + 2 {
            assert_eq!(
                floor_grapheme_boundary(text, offset),
                whole_text_floor(text, offset),
                "{text:?} at {offset}"
            );
        }
    }
}

#[test]
fn flooring_inside_blocks_equals_flooring_in_the_joined_text() {
    let mut doc = WriterDocument::new("f", "F");
    doc.replace_paragraphs("e\u{301}x\n\n\u{1F468}\u{200D}\u{1F469} y\nlast");
    let text = doc.editor_text();
    let starts = doc.block_starts();
    for offset in 0..=text.len() + 3 {
        assert_eq!(
            floor_in_blocks(&doc, &starts, offset),
            whole_text_floor(&text, offset),
            "offset {offset}"
        );
    }
}

#[test]
fn grapheme_positions_equal_counting_the_joined_text() {
    let mut doc = WriterDocument::new("g", "G");
    doc.replace_paragraphs("e\u{301}x y\n\n\u{1F468}\u{200D}\u{1F469} z\nlast line");
    let text = doc.editor_text();
    for offset in 0..=text.len() + 2 {
        let floored = whole_text_floor(&text, offset);
        assert_eq!(
            doc.grapheme_position(offset),
            crate::grapheme_count(&text[..floored]),
            "offset {offset}"
        );
    }
    assert_eq!(WriterDocument::new("e", "E").grapheme_position(7), 0);
}
