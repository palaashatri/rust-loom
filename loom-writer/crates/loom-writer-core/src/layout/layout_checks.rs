//! Cached and windowed layout against the original implementation in
//! `layout_tests`, plus the cache and flooring shortcuts that make edits cheap.

use super::*;
use crate::{grapheme_boundaries, CaretAffinity};

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
fn selection_rectangles_match_the_reference_for_every_caret_and_many_ranges() {
    let doc = sample_document(40);
    let style = doc.page.page_style();
    let layout = doc.layout(&style, viewport(1.0, 0.0)).unwrap();
    let total = doc.editor_text().len();
    for affinity in [CaretAffinity::Upstream, CaretAffinity::Downstream] {
        for offset in (0..=total).step_by(3) {
            let mut selection = TextSelection::caret(offset);
            selection.affinity = affinity;
            assert_eq!(
                doc.selection_rectangles_for(&layout, &style, &selection),
                doc.reference_rects_for(&layout, &style, &selection),
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
            doc.reference_rects_for(&layout, &style, &selection),
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
