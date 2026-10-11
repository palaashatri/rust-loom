//! Runs that name a font family are measured in that family: the widths,
//! caret positions and line breaks are those of the glyphs the page draws in
//! it, and the PDF export (which draws everything in Inter) is not affected.

use super::*;
use crate::test_fonts;
use loom_text::{CharacterStyle, StyleRun};

const TEXT: &str = "AVATAR Toyota Wave, quick brown fox.";

fn family_run(start: usize, end: usize, family: &str, bold: bool, italic: bool) -> StyleRun {
    StyleRun {
        start,
        end,
        style: CharacterStyle {
            font_family: family.to_owned(),
            weight: if bold {
                FontWeight::Bold
            } else {
                FontWeight::Regular
            },
            italic,
            ..CharacterStyle::default()
        },
    }
}

/// `text` shaped in one piece in `family`, straight through `loom-fonts`.
fn shaped_in(family: &str, text: &str, size: f32, bold: bool, italic: bool) -> f32 {
    let catalog = crate::fonts::font_catalog();
    let font = catalog
        .resolve(family, if bold { 700 } else { 400 }, italic)
        .expect("family");
    catalog
        .load(font.primary())
        .expect("face")
        .text_width(text, size)
}

#[test]
fn a_run_in_another_family_is_measured_in_that_family() {
    let _fonts = test_fonts::install(&[("Wider", 200)]);
    let size = 12.0;
    let inter = text_advance(TEXT, 0, &[], size);
    for (bold, italic) in [(false, false), (true, false), (false, true), (true, true)] {
        let runs = [family_run(0, TEXT.len(), "Wider", bold, italic)];
        let measured = text_advance(TEXT, 0, &runs, size);
        let expected = shaped_in("Wider", TEXT, size, bold, italic);
        assert!(
            (measured - expected).abs() < 0.02,
            "bold {bold} italic {italic}: measured {measured}, shaped {expected}"
        );
        assert!(
            measured > inter * 1.5,
            "the doubled advances are not in the measure: {measured} against Inter {inter}"
        );
    }
}

#[test]
fn only_the_run_that_names_the_family_changes_width() {
    let _fonts = test_fonts::install(&[("Wider", 200)]);
    let size = 12.0;
    let runs = [family_run(7, 13, "Wider", false, false)];
    let mixed = text_advance(TEXT, 0, &runs, size);
    // The pieces are shaped separately: before, inside and after the run.
    let expected = shaped_in("Inter", &TEXT[..7], size, false, false)
        + shaped_in("Wider", &TEXT[7..13], size, false, false)
        + shaped_in("Inter", &TEXT[13..], size, false, false);
    assert!(
        (mixed - expected).abs() < 0.02,
        "mixed line {mixed}, pieces {expected}"
    );
    // A caret after the run sits where the pieces end.
    let before_run = text_advance(&TEXT[..7], 0, &runs, size);
    let through_run = text_advance(&TEXT[..13], 0, &runs, size);
    assert!(
        (through_run - before_run - shaped_in("Wider", &TEXT[7..13], size, false, false)).abs()
            < 0.02
    );
    // Clicking in the middle of the wide run lands inside it.
    let middle = before_run + (through_run - before_run) / 2.0;
    let offset = offset_at_x(TEXT, 0, &runs, size, middle);
    assert!((9..=11).contains(&offset), "click landed at byte {offset}");
}

#[test]
fn the_old_generic_names_measure_as_documented() {
    let _fonts = test_fonts::install(&[("Georgia", 150)]);
    let size = 11.0;
    let plain = text_advance(TEXT, 0, &[], size);
    // "Sans" and the unnamed family are the document font.
    for name in ["Sans", "", "Inter"] {
        let runs = [family_run(0, TEXT.len(), name, false, false)];
        assert_eq!(text_advance(TEXT, 0, &runs, size), plain, "{name:?}");
    }
    // "Serif" is whatever concrete serif the catalogue offers: here Georgia.
    let runs = [family_run(0, TEXT.len(), "Serif", false, false)];
    let serif = text_advance(TEXT, 0, &runs, size);
    assert!((serif - shaped_in("Georgia", TEXT, size, false, false)).abs() < 0.02);
    assert!(serif > plain * 1.3);
}

#[test]
fn wrapping_uses_the_family_widths() {
    let _fonts = test_fonts::install(&[("Wider", 200)]);
    let text = "one two three four five six seven eight nine ten eleven twelve";
    let runs = [family_run(0, text.len(), "Wider", false, false)];
    let max = 160.0;
    let size = 11.0;
    let plain = wrap_by_width(text, &[], size, max);
    let wide = block_measure(text, &runs, size).wrap(text, max);
    assert!(
        wide.len() > plain.len(),
        "{} against {}",
        wide.len(),
        plain.len()
    );
    for (start, end) in &wide {
        let visible = text[*start..*end].trim_end();
        assert!(
            text_advance(visible, *start, &runs, size) <= max + 0.01,
            "line too wide: {visible:?}"
        );
    }
}

#[test]
fn the_pdf_ignores_the_families_runs_name() {
    let _fonts = test_fonts::install(&[("Wider", 200)]);
    let style = loom_pdf::TextStyle::default();
    let runs = [family_run(0, TEXT.len(), "Wider", false, false)];
    let named = crate::export_flow::line_width("", TEXT, &runs, &style, 0);
    let plain = crate::export_flow::line_width("", TEXT, &[], &style, 0);
    assert_eq!(named, plain, "the PDF draws Inter whatever the run names");
}

#[test]
fn replacing_the_catalogue_re_measures_runs_and_layout() {
    // Before the scan finishes "Wider" is not installed and is measured in the
    // stand-in; once the catalogue is replaced the same text is wider.
    let fonts = test_fonts::install(&[]);
    let size = 12.0;
    let runs = [family_run(0, TEXT.len(), "Wider", false, false)];
    let before = text_advance(TEXT, 0, &runs, size);

    let mut document = crate::WriterDocument::new("epoch", "Epoch");
    let mut block = crate::RichBlock::new(1, "paragraph", TEXT);
    block.runs = runs.to_vec();
    document.push(block);
    let style = document.page.page_style();
    let viewport = crate::PageViewport {
        width: style.width_pt,
        height: style.height_pt,
        zoom: 1.0,
        scroll_x: 0.0,
        scroll_y: 0.0,
    };
    let line_width = |document: &crate::WriterDocument| {
        let flow = document.flow(&style, viewport).expect("flow");
        flow.lines_on(0..1)[0].bounds.width
    };
    let line_before = line_width(&document);

    fonts.add(&[("Wider", 200)]);
    let after = text_advance(TEXT, 0, &runs, size);
    assert!(after > before * 1.5, "{after} against {before}");
    let line_after = line_width(&document);
    assert!(
        line_after > line_before * 1.5,
        "the cached layout kept its old widths: {line_after} against {line_before}"
    );
}
