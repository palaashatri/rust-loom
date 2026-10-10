//! Measurement against the shaper: widths and positions must be those of the
//! glyphs the page draws (Inter shaped with kerning), however the cache cuts
//! the text into words, and the PDF must space its runs the same way.

use super::faces;
use super::*;
use loom_text::CharacterStyle;

/// Text full of pairs Inter kerns, plus ordinary prose, digits, punctuation and
/// accented letters.
const SAMPLES: [&str; 8] = [
    "AVATAR Toyota Wave Type Yellow. AVAILABLE WAYS TO TRAVEL: To Tokyo, Vancouver.",
    "Typography: Ty Tw Te To Ta; P. P, F. F, T. T, V. V, W. W, Y. Y, \"AV\" 'Yo' (Wa) [Te].",
    "The quick brown fox jumps over the lazy dog, 12,345.67 times (about 98% of it).",
    "  leading and  double   spaces, trailing too  ",
    "Caf\u{e9} cr\u{e8}me br\u{fb}l\u{e9}e, na\u{ef}ve fa\u{e7}ade \u{2014} \u{201c}quoted\u{201d} \u{2026}",
    "Cafe\u{301} cre\u{300}me, A\u{30a}ngstro\u{308}m, n\u{303}",
    "\u{3a9}\u{3bc}\u{3ad}\u{3b3}\u{3b1} \u{41f}\u{440}\u{438}\u{432}\u{435}\u{442} AV\u{a0}AV",
    "https://example.com/AVATAR/Toyota?q=WAVE&r=Yoyo#TAWA",
];

/// The width of `text` shaped in one piece straight through `loom-fonts`.
fn shaped_whole(text: &str, size: f32, bold: bool, italic: bool) -> f32 {
    let catalog = loom_fonts::FontCatalog::bundled_only();
    let font = catalog
        .resolve("Inter", if bold { 700 } else { 400 }, italic)
        .expect("Inter");
    catalog
        .load(font.primary())
        .expect("face")
        .text_width(text, size)
}

fn run(start: usize, end: usize, bold: bool, italic: bool) -> StyleRun {
    StyleRun {
        start,
        end,
        style: CharacterStyle {
            weight: if bold {
                FontWeight::Bold
            } else {
                FontWeight::Regular
            },
            italic,
            ..Default::default()
        },
    }
}

#[test]
fn a_run_measures_like_the_whole_line_shaped_at_once() {
    // Words are shaped separately and remembered; the result must still be the
    // width of the line shaped in one piece, in every face.
    for (bold, italic) in [(false, false), (true, false), (false, true), (true, true)] {
        let runs = [run(0, usize::MAX, bold, italic)];
        for sample in SAMPLES {
            let size = 11.0;
            let measured = text_advance(sample, 0, &runs, size);
            let whole = shaped_whole(sample, size, bold, italic);
            assert!(
                (measured - whole).abs() < 0.01,
                "{sample:?} bold {bold} italic {italic}: measured {measured}, shaped {whole}"
            );
        }
    }
}

#[test]
fn nothing_kerns_against_a_space_that_precedes_it_in_any_face() {
    // The word cache shapes a word with the space that follows it and assumes
    // the next word starts clean. Check that for every character the faces
    // cover (Inter Bold kerns '.', ',' and the ellipsis against a *following*
    // space, which the cache's unit takes into account).
    let catalog = loom_fonts::FontCatalog::bundled_only();
    for (bold, italic) in [(false, false), (true, false), (false, true), (true, true)] {
        let font = catalog
            .resolve("Inter", if bold { 700 } else { 400 }, italic)
            .expect("Inter");
        let face = catalog.load(font.primary()).expect("face");
        let width = |text: &str| face.text_width(text, 1000.0);
        let space = width(" ");
        assert!((width("  ") - 2.0 * space).abs() < 0.01, "two spaces kern");
        let mut checked = 0;
        for code in 0x21u32..0x1_0000 {
            let Some(ch) = char::from_u32(code) else {
                continue;
            };
            if ch.is_control() || !face.covers(ch) {
                continue;
            }
            let alone = width(&ch.to_string());
            let after_space = width(&format!(" {ch}"));
            assert!(
                (after_space - space - alone).abs() < 0.01,
                "U+{code:04X} kerns against a preceding space (bold {bold}, italic {italic})"
            );
            checked += 1;
        }
        assert!(checked > 1500, "{checked} characters were checked");
    }
}

#[test]
fn kerned_pairs_make_a_line_narrower_than_the_sum_of_its_glyphs() {
    let text = "AVATAR Toyota Wave";
    let natural = loom_pdf::text_width_pt(
        text,
        &loom_pdf::TextStyle {
            size_pt: 12.0,
            ..Default::default()
        },
    );
    let kerned = text_advance(text, 0, &[], 12.0);
    assert!(
        kerned < natural * 0.985,
        "kerned {kerned} pt against {natural} pt unkerned"
    );
    // A pair that does not kern measures as the sum of its glyphs.
    let plain = "iiiiiiiiii";
    assert!(
        (text_advance(plain, 0, &[], 12.0)
            - loom_pdf::text_width_pt(
                plain,
                &loom_pdf::TextStyle {
                    size_pt: 12.0,
                    ..Default::default()
                }
            ))
        .abs()
            < 0.2,
        "ii does not kern much"
    );
}

#[test]
fn kerning_does_not_cross_a_change_of_style() {
    // "AV" is kerned inside one run; with V bold the page shapes "A" and "V"
    // separately, in different faces, so nothing is kerned between them.
    let text = "AV";
    let size = 20.0;
    let split = [run(1, 2, true, false)];
    let measured = text_advance(text, 0, &split, size);
    let expected = shaped_whole("A", size, false, false) + shaped_whole("V", size, true, false);
    assert!(
        (measured - expected).abs() < 1e-3,
        "{measured} vs {expected}"
    );
    let one_run = text_advance(text, 0, &[], size);
    assert!(
        one_run < shaped_whole("A", size, false, false) + shaped_whole("V", size, false, false),
        "AV kerns within one run"
    );
}

#[test]
fn italic_runs_are_measured_in_the_italic_face() {
    let text = "Quick fox";
    let upright = text_advance(text, 0, &[], 14.0);
    let italic = text_advance(text, 0, &[run(0, text.len(), false, true)], 14.0);
    assert!((italic - shaped_whole(text, 14.0, false, true)).abs() < 1e-3);
    assert!((upright - shaped_whole(text, 14.0, false, false)).abs() < 1e-3);
    assert!(
        (shaped_whole(text, 14.0, false, true) - shaped_whole(text, 14.0, true, false)).abs()
            > 1e-3,
        "the faces differ"
    );
}

#[test]
fn positions_rise_to_the_width_and_every_boundary_maps_back_to_itself() {
    for sample in SAMPLES {
        let measure = block_measure(sample, &[], 11.0);
        let mut previous = -1.0_f32;
        let mut boundaries = vec![0usize];
        boundaries.extend(
            unicode_segmentation::UnicodeSegmentation::grapheme_indices(sample, true)
                .map(|(start, grapheme)| start + grapheme.len()),
        );
        for &boundary in &boundaries {
            let x = measure.x_at(boundary);
            assert!(x >= previous, "{sample:?}: x runs backwards at {boundary}");
            previous = x;
        }
        assert!((measure.x_at(sample.len()) - measure.width()).abs() < 1e-4);
        for &boundary in &boundaries {
            let x = measure.x_at(boundary);
            let hit = measure.offset_at_x(x, sample.len());
            // Boundaries that share an x (zero-width graphemes) map to the
            // first; every other boundary maps to itself.
            assert_eq!(
                measure.x_at(hit),
                x,
                "{sample:?}: the point at boundary {boundary} lands at {hit}"
            );
            if boundaries
                .iter()
                .filter(|other| (measure.x_at(**other) - x).abs() < 1e-4)
                .count()
                == 1
            {
                assert_eq!(hit, boundary, "{sample:?}");
            }
        }
    }
}

#[test]
fn a_selection_edge_inside_a_kerned_pair_is_where_the_glyph_is_drawn() {
    // The caret between A and V sits at the start of V as drawn: the kerning
    // of the pair has already been taken, which the width of "A" shaped on its
    // own would miss.
    let text = "AVATAR";
    let size = 40.0;
    let measure = block_measure(text, &[], size);
    let alone = shaped_whole("A", size, false, false);
    let drawn_start_of_v = measure.x_at(1);
    assert!(
        drawn_start_of_v < alone - 0.5,
        "V starts at {drawn_start_of_v}; A alone is {alone}"
    );
    assert!(
        (drawn_start_of_v
            - (shaped_whole("AV", size, false, false) - shaped_whole("V", size, false, false)))
        .abs()
            < 1e-3
    );
}

#[test]
fn combining_sequences_measure_as_their_composed_letters() {
    for (composed, decomposed) in [
        ("Caf\u{e9}", "Cafe\u{301}"),
        ("\u{c5}ngstr\u{f6}m", "A\u{30a}ngstro\u{308}m"),
        ("se\u{f1}or", "sen\u{303}or"),
    ] {
        let a = text_advance(composed, 0, &[], 16.0);
        let b = text_advance(decomposed, 0, &[], 16.0);
        assert!(
            (a - b).abs() < 0.01,
            "{composed:?} {a} vs {decomposed:?} {b}"
        );
    }
}

#[test]
fn characters_inter_lacks_are_estimated_in_the_editor_and_drawn_by_the_pdf() {
    let text = "\u{65e5}\u{672c}";
    let size = 10.0;
    // The editor assumes the window's fallback font sets them one em wide.
    assert!((text_advance(text, 0, &[], size) - 2.0 * size).abs() < 1e-3);
    // The PDF writes them with the advance of the notdef box.
    let style = loom_pdf::TextStyle {
        size_pt: size,
        ..Default::default()
    };
    assert!(
        (pdf_text_width(text, &style) - loom_pdf::text_width_pt(text, &style)).abs() < 1e-3,
        "the PDF measures what it draws"
    );
    // A covered word next to them is still shaped and kerned.
    let mixed = "AVATAR \u{65e5}\u{672c} Toyota";
    let covered = text_advance("AVATAR ", 0, &[], size) + text_advance(" Toyota", 0, &[], size);
    let whole = text_advance(mixed, 0, &[], size);
    assert!(
        (whole - covered - 2.0 * size).abs() < 0.05,
        "{whole} {covered}"
    );
}

#[test]
fn a_word_is_shaped_once_and_remembered() {
    faces::clear_caches();
    let text = "Tavares Toyota Tavares Toyota Tavares";
    let first = text_advance(text, 0, &[], 12.0);
    let remembered = faces::cached_word_count();
    // "Tavares " and "Toyota " (a word is held with the space after it) and
    // the last "Tavares", which has none.
    assert_eq!(remembered, 3, "three distinct words are held");
    let again = text_advance(text, 0, &[], 12.0);
    assert_eq!(first, again);
    assert_eq!(faces::cached_word_count(), remembered);
    // Another size reuses the same word advances (they are in em).
    let doubled = text_advance(text, 0, &[], 24.0);
    assert!((doubled - 2.0 * first).abs() < 1e-3);
    assert_eq!(faces::cached_word_count(), remembered);
}

/// Cost of measuring a long document of varied prose from a cold cache: 400
/// pages (8,000 paragraphs of 25 words) drawn from a vocabulary of 30,000
/// distinct words, so most words are shaped once. `LOOM_FRAME_BENCH=1` prints
/// the numbers; `enforce` also fails above the open-time budget share of 1 s.
#[test]
fn a_long_document_of_varied_words_is_measured_within_budget() {
    let Ok(mode) = std::env::var("LOOM_FRAME_BENCH") else {
        return;
    };
    let mut seed = 0x2545_F491_4F6C_DD1Du64;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let syllables = [
        "ta", "vo", "re", "li", "mon", "ar", "ex", "qui", "Wa", "Yo", "ne", "ph", "str", "ing",
        "ous", "ly", "ed", "Av", "To", "ba",
    ];
    let vocabulary: Vec<String> = (0..30_000)
        .map(|_| {
            let count = 2 + (next() % 3) as usize;
            let mut word = String::new();
            for _ in 0..count {
                word.push_str(syllables[(next() % syllables.len() as u64) as usize]);
            }
            word
        })
        .collect();
    let paragraphs: Vec<String> = (0..8_000)
        .map(|_| {
            (0..25)
                .map(|_| vocabulary[(next() % vocabulary.len() as u64) as usize].as_str())
                .collect::<Vec<_>>()
                .join(" ")
                + "."
        })
        .collect();
    faces::clear_caches();
    let started = std::time::Instant::now();
    let mut lines = 0usize;
    for paragraph in &paragraphs {
        lines += wrap_by_width(paragraph, &[], 11.0, 451.0).len();
    }
    let cold = started.elapsed().as_secs_f64() * 1e3;
    let started = std::time::Instant::now();
    for paragraph in &paragraphs {
        lines += wrap_by_width(paragraph, &[], 11.0, 451.0).len();
    }
    let warm = started.elapsed().as_secs_f64() * 1e3;
    println!(
        "MEASURE_BENCH paragraphs=8000 distinct_words={} lines={lines} cold_ms={cold:.0} warm_ms={warm:.0}",
        faces::cached_word_count()
    );
    if mode == "enforce" {
        assert!(cold < 1000.0, "cold measure took {cold} ms");
    }
}

#[test]
fn the_pdf_spaces_runs_by_the_widths_it_wraps_with() {
    use crate::{export_pdf, RichBlock, WriterDocument};

    let mut document = WriterDocument::new("kerned", "Kerned");
    for text in SAMPLES.iter().take(3) {
        document.push(RichBlock::new(document.next_id(), "paragraph", text));
    }
    let pdf = export_pdf(&document);
    let moved: f32 = loom_pdf::inspect::spacing_adjustments(&pdf)
        .expect("adjustments")
        .iter()
        .sum();
    assert!(moved > 0.0, "kerned runs are spaced");

    // Every drawn run is one line here; its natural width less its measured
    // width is what the file moved the pen back by.
    let size = document.page.page_style().body_font_size_pt;
    let style = loom_pdf::TextStyle {
        size_pt: size,
        ..Default::default()
    };
    let lines = loom_pdf::inspect::page_text(&pdf).expect("text");
    let expected: f32 = lines
        .iter()
        .flat_map(|page| page.split('\n'))
        .map(|line| loom_pdf::text_width_pt(line, &style) - pdf_text_width(line, &style))
        .sum();
    let in_points = moved * size / 1000.0;
    // Runs end with their last glyph unadjusted, and the rounding is a tenth
    // of a thousandth of an em per kerned pair.
    assert!(
        (in_points - expected).abs() < 0.05 * lines.len() as f32 + 0.5,
        "the file moves the pen by {in_points} pt, the measure says {expected} pt"
    );
}
