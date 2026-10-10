//! Line layout: paragraph-level bidi, caret affinity, multiple styled runs,
//! per-cluster fallback, selection rectangles and line-break opportunities.

mod common;

use common::{cmap_table, inter, rename_family, with_table, TempDir};
use loom_fonts::{
    line_break_opportunities, work, Affinity, FallbackPolicy, FontCatalog, FontRef, LineBreak,
    Paragraph, ScanConfig, SelectionRect, StyledRun, StyledSpan, TextDirection,
};
use std::fs;

const HEBREW: &str = "\u{5e9}\u{5dc}\u{5d5}\u{5dd}"; // 8 bytes

fn bundled() -> FontCatalog {
    let mut config = ScanConfig::with_dirs(Vec::new());
    config.policy = FallbackPolicy::new(Vec::<String>::new());
    FontCatalog::scan(&config)
}

fn inter_font(catalog: &FontCatalog, weight: u16, italic: bool) -> FontRef {
    catalog.resolve("Inter", weight, italic).unwrap()
}

/// A catalogue with Inter plus "Cjkfa", a font that covers only U+4E2D,
/// U+6587 and the space, so fallback decisions can be observed.
fn with_fallback(policy: &[&str]) -> (FontCatalog, TempDir) {
    let dir = TempDir::new("fallback");
    let font = with_table(
        &rename_family(&inter("Inter-Regular.ttf"), "Cjkfa"),
        b"cmap",
        cmap_table(&[(0x20, 3), (0x4E2D, 5), (0x6587, 6)]),
    );
    fs::write(dir.path().join("cjkfa.ttf"), font).unwrap();
    let mut config = ScanConfig::with_dirs(vec![dir.path().to_owned()]);
    config.policy = FallbackPolicy::new(policy.iter().copied());
    (FontCatalog::scan(&config), dir)
}

fn visual_ranges(layout: &loom_fonts::LineLayout) -> Vec<(usize, usize)> {
    layout
        .runs
        .iter()
        .map(|r| (r.range.start, r.range.end))
        .collect()
}

fn text_of<'a>(text: &'a str, layout: &loom_fonts::LineLayout, run: usize) -> &'a str {
    &text[layout.runs[run].range.clone()]
}

#[test]
fn a_wrapped_line_takes_its_bidi_context_from_the_whole_paragraph() {
    let catalog = bundled();
    let font = inter_font(&catalog, 400, false);
    // A Hebrew paragraph that wraps after "<hebrew> ": the second line is
    // "123 abc", whose numbers and Latin are right-to-left context material.
    let text = format!("{HEBREW} 123 abc");
    let second = HEBREW.len() + 1;
    let spans = [StyledSpan::new(0..text.len(), &font, 16.0)];
    let paragraph = Paragraph::new(&text, &spans, TextDirection::Auto);
    let line = catalog.layout_paragraph_line(&paragraph, second..text.len());

    assert_eq!(line.range, second..text.len());
    let pieces: Vec<&str> = (0..line.runs.len())
        .map(|i| text_of(&text, &line, i))
        .collect();
    // Visual order left to right is "abc", " ", "123": reversed, as the
    // paragraph's right-to-left level requires.
    assert_eq!(pieces.first().copied(), Some("abc"), "{pieces:?}");
    assert_eq!(pieces.last().copied(), Some("123"), "{pieces:?}");
    assert_eq!(pieces.concat(), "abc 123");

    // The same words laid out as a paragraph of their own are left to right.
    let isolated = catalog.layout_text("123 abc", &font, 16.0, TextDirection::Auto);
    assert_eq!(&"123 abc"[isolated.runs[0].range.clone()], "123 abc");
    assert!(!isolated.runs[0].rtl);
}

#[test]
fn wrapped_lines_tile_their_range_and_keep_absolute_offsets() {
    let catalog = bundled();
    let font = inter_font(&catalog, 400, false);
    let text = "The quick brown fox jumps over the lazy dog";
    let spans = [StyledSpan::new(0..text.len(), &font, 14.0)];
    let paragraph = Paragraph::new(text, &spans, TextDirection::Auto);
    let line = catalog.layout_paragraph_line(&paragraph, 10..25);
    let mut covered = visual_ranges(&line);
    covered.sort_unstable();
    assert_eq!(covered.first().map(|r| r.0), Some(10));
    assert_eq!(covered.last().map(|r| r.1), Some(25));
    let direct = catalog.text_width(&text[10..25], &font, 14.0);
    assert!((line.width - direct).abs() < 0.5);
    // Offsets passed to the queries are paragraph offsets.
    let x = line.x_at_offset(18, Affinity::Leading);
    assert_eq!(line.offset_at_x(x), (18, Affinity::Leading));
    assert_eq!(line.x_at_offset(10, Affinity::Leading), 0.0);
}

#[test]
fn affinity_tells_the_two_carets_at_a_bidi_boundary_apart() {
    let catalog = bundled();
    let font = inter_font(&catalog, 400, false);
    let text = format!("abc {HEBREW} def");
    let layout = catalog.layout_text(&text, &font, 16.0, TextDirection::Auto);
    let start = 4; // before the first Hebrew letter
    let end = 4 + HEBREW.len(); // after the last one
    let hebrew = layout.runs.iter().find(|r| r.rtl).expect("a Hebrew run");
    assert_eq!(hebrew.range, start..end);
    let (left, right) = (hebrew.x, hebrew.x + hebrew.shaped.width);

    // Before the Hebrew: trailing edge of "abc " is at the run's left edge;
    // leading edge of the first Hebrew letter is at the run's right edge.
    let trailing = layout.x_at_offset(start, Affinity::Trailing);
    let leading = layout.x_at_offset(start, Affinity::Leading);
    assert!((trailing - left).abs() < 1e-3, "{trailing} vs {left}");
    assert!((leading - right).abs() < 1e-3, "{leading} vs {right}");
    // After the Hebrew: the last letter's trailing edge is the run's left
    // edge, the following space's leading edge is the run's right edge.
    let after_trailing = layout.x_at_offset(end, Affinity::Trailing);
    let after_leading = layout.x_at_offset(end, Affinity::Leading);
    assert!((after_trailing - left).abs() < 1e-3);
    assert!((after_leading - right).abs() < 1e-3);
    // Inside a run the two coincide; letters advance leftwards in Hebrew.
    let mut previous = right;
    for offset in [start + 2, start + 4, start + 6] {
        let x = layout.x_at_offset(offset, Affinity::Leading);
        assert_eq!(x, layout.x_at_offset(offset, Affinity::Trailing));
        assert!(x < previous);
        previous = x;
    }
    // Plain left-to-right text has one caret per boundary.
    let at_a = layout.x_at_offset(1, Affinity::Leading);
    assert_eq!(at_a, layout.x_at_offset(1, Affinity::Trailing));
}

#[test]
fn every_boundary_round_trips_through_hit_testing() {
    let catalog = bundled();
    let font = inter_font(&catalog, 400, false);
    let text = format!("abc {HEBREW} def");
    let layout = catalog.layout_text(&text, &font, 16.0, TextDirection::Auto);
    let boundaries: Vec<usize> = text
        .char_indices()
        .map(|(i, _)| i)
        .chain([text.len()])
        .collect();

    // Every distinct visual caret position.
    let mut xs: Vec<f32> = Vec::new();
    for &offset in &boundaries {
        for affinity in [Affinity::Leading, Affinity::Trailing] {
            xs.push(layout.x_at_offset(offset, affinity));
        }
    }
    for &offset in &boundaries {
        for affinity in [Affinity::Leading, Affinity::Trailing] {
            let x = layout.x_at_offset(offset, affinity);
            let (found, found_affinity) = layout.offset_at_x(x);
            // The answer must sit at the very same place...
            let back = layout.x_at_offset(found, found_affinity);
            assert!(
                (back - x).abs() < 1e-3,
                "offset {offset} {affinity:?} at {x}: hit {found} {found_affinity:?} at {back}"
            );
            // ...and where the place belongs to one position only, it must
            // be that position (two logical positions can share a visual
            // spot at a bidi boundary; the lower offset is reported).
            let sharing: Vec<(usize, Affinity)> = boundaries
                .iter()
                .flat_map(|&o| [(o, Affinity::Leading), (o, Affinity::Trailing)])
                .filter(|&(o, a)| (layout.x_at_offset(o, a) - x).abs() < 1e-3)
                .collect();
            let distinct_offsets: std::collections::BTreeSet<usize> =
                sharing.iter().map(|(o, _)| *o).collect();
            if distinct_offsets.len() == 1 {
                assert_eq!(found, offset, "unique spot {x}");
            } else {
                assert_eq!(found, *distinct_offsets.first().unwrap(), "shared spot {x}");
            }
        }
    }
    assert!(xs.len() > 20);
}

#[test]
fn hit_testing_reports_which_side_of_a_boundary_was_hit() {
    let catalog = bundled();
    let font = inter_font(&catalog, 400, false);
    let text = format!("abc {HEBREW} def");
    let layout = catalog.layout_text(&text, &font, 16.0, TextDirection::Auto);
    let hebrew = layout.runs.iter().find(|r| r.rtl).unwrap();
    let (left, right) = (hebrew.x, hebrew.x + hebrew.shaped.width);
    // Just inside the Hebrew run's left edge the nearest caret is the
    // trailing edge of the text before / the last Hebrew letter.
    let (_, affinity) = layout.offset_at_x(left + 0.05);
    assert_eq!(affinity, Affinity::Trailing);
    // Just inside its right edge: the leading edge of the first Hebrew letter
    // (or of the text after the run).
    let (_, affinity) = layout.offset_at_x(right - 0.05);
    assert_eq!(affinity, Affinity::Leading);
    // Beyond the ends clamp to the first and last boundary.
    assert_eq!(layout.offset_at_x(-100.0).0, 0);
    assert_eq!(layout.offset_at_x(10_000.0).0, text.len());
}

#[test]
fn a_hit_exactly_between_two_carets_goes_to_the_left_one() {
    let catalog = bundled();
    let font = inter_font(&catalog, 400, false);
    let layout = catalog.layout_text("mm", &font, 16.0, TextDirection::Auto);
    let first = layout.x_at_offset(1, Affinity::Leading);
    // Halving a float is exact, so this x is equidistant from 0 and `first`.
    assert_eq!(layout.offset_at_x(first / 2.0), (0, Affinity::Leading));
    assert_eq!(layout.offset_at_x(first / 2.0 + 0.01).0, 1);
    assert_eq!(layout.offset_at_x(first / 2.0 - 0.01).0, 0);
}

#[test]
fn several_left_to_right_and_right_to_left_runs_are_reordered_as_blocks() {
    let catalog = bundled();
    let font = inter_font(&catalog, 400, false);
    // Right-to-left paragraph: Latin, Hebrew, Latin. Levels 2, 1, 2.
    let text = format!("ab cd {HEBREW} ef");
    let layout = catalog.layout_text(&text, &font, 16.0, TextDirection::RightToLeft);
    let block_of = |start: usize| -> usize {
        if start < 5 {
            0 // "ab cd"
        } else if start < text.len() - 2 {
            1 // " HEBREW "
        } else {
            2 // "ef"
        }
    };
    let order: Vec<usize> = layout
        .runs
        .iter()
        .map(|r| block_of(r.range.start))
        .collect();
    let mut compressed = order.clone();
    compressed.dedup();
    assert_eq!(compressed, [2, 1, 0], "blocks left to right: {order:?}");
    // Inside the middle block the pieces also run backwards.
    let middle: Vec<usize> = layout
        .runs
        .iter()
        .filter(|r| block_of(r.range.start) == 1)
        .map(|r| r.range.start)
        .collect();
    assert!(middle.windows(2).all(|w| w[0] > w[1]), "{middle:?}");
    // The Latin blocks are left to right inside, the Hebrew one is not.
    for run in &layout.runs {
        assert_eq!(run.rtl, block_of(run.range.start) == 1);
    }
    // Everything tiles and abuts.
    let mut x = 0.0;
    for run in &layout.runs {
        assert!((run.x - x).abs() < 1e-3);
        x += run.shaped.width;
    }
    assert!((layout.width - x).abs() < 1e-3);
    let mut sorted = visual_ranges(&layout);
    sorted.sort_unstable();
    assert_eq!(sorted.first().unwrap().0, 0);
    assert_eq!(sorted.last().unwrap().1, text.len());
    for pair in sorted.windows(2) {
        assert_eq!(pair[0].1, pair[1].0);
    }
}

#[test]
fn one_line_can_hold_several_styled_runs() {
    let catalog = bundled();
    let regular = inter_font(&catalog, 400, false);
    let bold = inter_font(&catalog, 700, false);
    let runs = [
        StyledRun::new("Hello ", &regular, 12.0),
        StyledRun::new("world", &bold, 20.0),
    ];
    let layout = catalog.layout_line(&runs, TextDirection::Auto);

    assert_eq!(layout.runs.len(), 2);
    assert_eq!(layout.runs[0].style, 0);
    assert_eq!(layout.runs[1].style, 1);
    assert_eq!((layout.runs[0].size, layout.runs[1].size), (12.0, 20.0));
    assert_eq!(layout.runs[0].range, 0..6);
    assert_eq!(layout.runs[1].range, 6..11);
    assert_ne!(layout.runs[0].face, layout.runs[1].face);
    assert_eq!(layout.runs[0].face, regular.primary());
    assert_eq!(layout.runs[1].face, bold.primary());

    // Width is the sum of two separate shapings (no kerning across styles).
    let a = catalog.text_width("Hello ", &regular, 12.0);
    let b = catalog.text_width("world", &bold, 20.0);
    assert!((layout.width - (a + b)).abs() < 1e-3);
    assert!((layout.runs[1].x - a).abs() < 1e-3);

    // Line metrics are the largest over the runs.
    let big = catalog.load(bold.primary()).unwrap().line_metrics(20.0);
    let small = catalog.load(regular.primary()).unwrap().line_metrics(12.0);
    assert!((layout.ascent - big.ascent).abs() < 1e-4);
    assert!((layout.descent - big.descent).abs() < 1e-4);
    assert!(layout.ascent > small.ascent);
    assert_eq!(layout.runs[0].ascent, small.ascent);
    assert!((layout.line_height() - (big.ascent + big.descent + big.line_gap)).abs() < 1e-4);

    // Carets cross the style boundary seamlessly.
    let boundary = layout.x_at_offset(6, Affinity::Leading);
    assert!((boundary - a).abs() < 1e-3);
    assert_eq!(layout.x_at_offset(6, Affinity::Trailing), boundary);
}

#[test]
fn a_run_carries_its_decoration_metrics() {
    let catalog = bundled();
    let font = inter_font(&catalog, 400, false);
    let layout = catalog.layout_text("Underlined", &font, 16.0, TextDirection::Auto);
    let face = catalog.load(font.primary()).unwrap();
    let metrics = face.line_metrics(16.0);
    let underline = metrics.underline.expect("Inter records an underline");
    assert!(underline.offset < 0.0 && underline.thickness > 0.0);
    let strike = metrics.strikeout.expect("Inter records a strikeout");
    assert!(strike.offset > 0.0 && strike.thickness > 0.0);
    assert_eq!(layout.runs[0].underline, Some(underline));
    assert_eq!(layout.runs[0].strikeout, Some(strike));
}

#[test]
fn synthesis_flags_are_judged_per_face_including_fallbacks() {
    // Cjkfa has only a regular face; the fallback Inter has a real bold.
    let (catalog, _dir) = with_fallback(&[]);
    let font = catalog.resolve("Cjkfa", 700, false).unwrap();
    assert!(font.synthetic_bold);
    assert_eq!(font.chain.len(), 2);
    assert!(font.chain_setup[0].synthetic_bold);
    assert!(!font.chain_setup[1].synthetic_bold);

    let layout = catalog.layout_text("\u{4e2d}a", &font, 16.0, TextDirection::Auto);
    assert_eq!(layout.runs.len(), 2);
    assert_eq!(layout.runs[0].face, font.chain[0]);
    assert!(layout.runs[0].synthetic_bold);
    assert_eq!(layout.runs[1].face, font.chain[1]);
    assert!(
        !layout.runs[1].synthetic_bold,
        "the fallback face is a true bold"
    );
    assert!(!layout.runs[0].synthetic_italic && !layout.runs[1].synthetic_italic);

    let italic = catalog.resolve("Cjkfa", 400, true).unwrap();
    let layout = catalog.layout_text("\u{4e2d}a", &italic, 16.0, TextDirection::Auto);
    assert!(layout.runs[0].synthetic_italic);
    assert!(!layout.runs[1].synthetic_italic);
}

#[test]
fn a_base_and_its_combining_mark_stay_in_one_face() {
    let (catalog, _dir) = with_fallback(&["Cjkfa"]);
    let font = catalog.resolve("Inter", 400, false).unwrap();
    let cjk = catalog.faces("Cjkfa")[0].0;
    assert_eq!(font.chain, [font.primary(), cjk]);
    // U+4E2D is only in Cjkfa, U+0301 only in Inter: the cluster is drawn
    // by the face that has its base letter, in one run.
    let text = "\u{4e2d}\u{301}";
    let layout = catalog.layout_text(text, &font, 16.0, TextDirection::Auto);
    assert_eq!(layout.runs.len(), 1, "{:?}", visual_ranges(&layout));
    assert_eq!(layout.runs[0].range, 0..text.len());
    assert_eq!(layout.runs[0].face, cjk);
    // No caret stop falls between the base and its mark.
    let offsets: Vec<usize> = layout.runs[0]
        .caret_stops
        .iter()
        .map(|s| s.offset)
        .collect();
    assert_eq!(offsets, [0, text.len()]);
}

#[test]
fn spaces_adopt_the_surrounding_face_and_script() {
    let (catalog, _dir) = with_fallback(&["Cjkfa"]);
    let font = catalog.resolve("Inter", 400, false).unwrap();
    let cjk = catalog.faces("Cjkfa")[0].0;
    let ranges = |text: &str| -> Vec<(usize, usize, bool)> {
        catalog
            .layout_text(text, &font, 16.0, TextDirection::Auto)
            .runs
            .iter()
            .map(|r| (r.range.start, r.range.end, r.face == cjk))
            .collect()
    };
    // A space between two fallback characters stays in the fallback run.
    assert_eq!(ranges("\u{4e2d} \u{6587}"), [(0, 7, true)]);
    // A space after Latin text stays with the Latin run.
    assert_eq!(ranges("a \u{4e2d}"), [(0, 2, false), (2, 5, true)]);
    // A space after fallback text joins it (the fallback covers a space).
    assert_eq!(ranges("\u{4e2d} a"), [(0, 4, true), (4, 5, false)]);
}

#[test]
fn selection_rectangles_follow_the_text_across_bidi_boundaries() {
    let catalog = bundled();
    let font = inter_font(&catalog, 400, false);
    let plain = catalog.layout_text("Hello world", &font, 16.0, TextDirection::Auto);
    let rects: Vec<SelectionRect> = plain.selection_rects(0..5);
    assert_eq!(rects.len(), 1);
    let expected = plain.x_at_offset(5, Affinity::Leading);
    assert!(rects[0].x.abs() < 1e-3 && (rects[0].width - expected).abs() < 1e-3);
    assert!(plain.selection_rects(3..3).is_empty());
    assert!(plain.selection_rects(0..0).is_empty());

    let text = format!("abc {HEBREW} def");
    let mixed = catalog.layout_text(&text, &font, 16.0, TextDirection::Auto);
    let hebrew = mixed.runs.iter().find(|r| r.rtl).unwrap();
    // A selection from inside "abc" to inside "def" is one visual block.
    let across = mixed.selection_rects(2..text.len() - 1);
    assert_eq!(across.len(), 1, "{across:?}");
    let start_x = mixed.x_at_offset(2, Affinity::Leading);
    let end_x = mixed.x_at_offset(text.len() - 1, Affinity::Leading);
    assert!((across[0].x - start_x).abs() < 1e-3);
    assert!((across[0].x + across[0].width - end_x).abs() < 1e-3);
    // The tail of "abc " plus only the first Hebrew letter is two blocks:
    // the letter is drawn at the Hebrew run's far right.
    let split = mixed.selection_rects(3..6);
    assert_eq!(split.len(), 2, "{split:?}");
    let first_letter = split.last().unwrap();
    assert!((first_letter.x + first_letter.width - (hebrew.x + hebrew.shaped.width)).abs() < 1e-3);
    assert!(first_letter.x > hebrew.x);
}

#[test]
fn an_empty_line_still_has_height_for_its_caret() {
    let catalog = bundled();
    let font = inter_font(&catalog, 400, false);
    let layout = catalog.layout_text("", &font, 20.0, TextDirection::Auto);
    assert!(layout.runs.is_empty());
    assert_eq!(layout.width, 0.0);
    assert!(!layout.estimated);
    let face = catalog.load(font.primary()).unwrap().line_metrics(20.0);
    assert!((layout.ascent - face.ascent).abs() < 1e-4);
    assert!((layout.descent - face.descent).abs() < 1e-4);
    assert_eq!(layout.x_at_offset(0, Affinity::Leading), 0.0);
    assert_eq!(layout.offset_at_x(50.0), (0, Affinity::Leading));
}

#[test]
fn an_estimated_line_still_places_a_caret() {
    let (catalog, dir) = with_fallback(&[]);
    let font = catalog.resolve("Cjkfa", 400, false).unwrap();
    // Take the font file away and drop the bundled fallback from the chain:
    // nothing can be loaded, so the layout is an estimate of half an em.
    let mut only_file = font.clone();
    only_file.chain.truncate(1);
    only_file.chain_setup.truncate(1);
    fs::remove_file(dir.path().join("cjkfa.ttf")).unwrap();
    let layout = catalog.layout_text("abcd", &only_file, 10.0, TextDirection::Auto);
    assert!(layout.estimated);
    assert!((layout.width - 20.0).abs() < 1e-4);
    assert!((layout.x_at_offset(2, Affinity::Leading) - 10.0).abs() < 1e-4);
    assert_eq!(layout.offset_at_x(14.0).0, 3);
}

#[test]
fn caret_work_stays_near_linear_and_queries_use_the_cached_stops() {
    let catalog = bundled();
    let font = inter_font(&catalog, 400, false);
    let words = ["alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta"];
    let mut text = String::new();
    let mut n = 0;
    while text.len() < 8_000 {
        text.push_str(words[n % words.len()]);
        text.push(' ');
        n += 1;
    }
    let len = text.len();
    let log = (len as f64).log2().ceil() as u64;

    work::reset();
    let layout = catalog.layout_text(&text, &font, 12.0, TextDirection::Auto);
    let build = work::caret_steps();
    assert!(layout.runs.iter().map(|r| r.range.len()).sum::<usize>() == len);
    assert!(work::caret_builds() >= 1);
    assert!(
        build <= 6 * len as u64 * log,
        "building stops took {build} steps for {len} characters"
    );
    // Quadratic work would be about len^2 / 2; far above the bound.
    assert!(build < (len as u64) * (len as u64) / 16);

    let builds_before = work::caret_builds();
    let steps_before = work::caret_steps();
    for offset in 0..=len {
        let x = layout.x_at_offset(offset, Affinity::Leading);
        let (hit, _) = layout.offset_at_x(x);
        assert_eq!(hit, offset);
    }
    let queries = (len as u64 + 1) * 2;
    assert_eq!(
        work::caret_builds(),
        builds_before,
        "queries must not rebuild the stops"
    );
    let per_query = (work::caret_steps() - steps_before) / queries;
    assert!(per_query <= log + 4, "{per_query} steps per query");
}

#[test]
fn line_break_opportunities_follow_uax_14() {
    let text = "Hello world-wide\nNext";
    let breaks = line_break_opportunities(text);
    let simple: Vec<(usize, bool)> = breaks.iter().map(|b| (b.offset, b.mandatory)).collect();
    assert_eq!(
        simple,
        [(6, false), (12, false), (17, true), (21, true)],
        "after the space, after the hyphen, after the newline, at the end"
    );
    // Break opportunities fall on character boundaries.
    for b in line_break_opportunities("\u{4e2d}\u{6587}\u{5b57} a") {
        assert!("\u{4e2d}\u{6587}\u{5b57} a".is_char_boundary(b.offset));
    }
    let cjk = line_break_opportunities("\u{4e2d}\u{6587}\u{5b57}");
    assert_eq!(
        cjk,
        [
            LineBreak {
                offset: 3,
                mandatory: false
            },
            LineBreak {
                offset: 6,
                mandatory: false
            },
            LineBreak {
                offset: 9,
                mandatory: true
            },
        ]
    );
    // A no-break space forbids the break.
    let glued = line_break_opportunities("a\u{a0}b");
    assert_eq!(
        glued,
        [LineBreak {
            offset: 4,
            mandatory: true
        }]
    );
    assert_eq!(line_break_opportunities(""), []);
}
