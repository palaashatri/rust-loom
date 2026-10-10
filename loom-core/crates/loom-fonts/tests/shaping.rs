mod common;

use common::Lcg;
use loom_fonts::{
    Affinity, FallbackPolicy, FontCatalog, FontRef, LoadedFace, ScanConfig, ShapeSettings,
    TextDirection,
};
use std::sync::Arc;

fn catalog() -> FontCatalog {
    let mut config = ScanConfig::with_dirs(Vec::new());
    config.policy = FallbackPolicy::new(Vec::<String>::new());
    FontCatalog::scan(&config)
}

fn face(catalog: &FontCatalog, weight: u16, italic: bool) -> (Arc<LoadedFace>, FontRef) {
    let font = catalog.resolve("Inter", weight, italic).unwrap();
    (catalog.load(font.primary()).unwrap(), font)
}

fn no_kerning() -> ShapeSettings {
    ShapeSettings {
        kerning: false,
        ..ShapeSettings::default()
    }
}

#[test]
fn shaping_reports_glyphs_clusters_and_advances() {
    let catalog = catalog();
    let (face, _) = face(&catalog, 400, false);
    let shaped = face.shape("Hello", 16.0, &ShapeSettings::default());
    assert_eq!(shaped.glyphs.len(), 5);
    assert!(!shaped.rtl);
    assert!(shaped
        .glyphs
        .iter()
        .all(|g| g.glyph_id != 0 && g.advance > 0.0));
    let clusters: Vec<usize> = shaped.glyphs.iter().map(|g| g.cluster).collect();
    assert_eq!(clusters, [0, 1, 2, 3, 4]);
    let sum: f32 = shaped.glyphs.iter().map(|g| g.advance).sum();
    assert!((shaped.width - sum).abs() < 1e-4);
    assert!((face.text_width("Hello", 16.0) - shaped.width).abs() < 1e-6);
    // The glyph ids are the ones the character map gives.
    assert_eq!(Some(shaped.glyphs[0].glyph_id), face.glyph_id('H'));
}

#[test]
fn empty_text_shapes_to_nothing() {
    let catalog = catalog();
    let (face, _) = face(&catalog, 400, false);
    let shaped = face.shape("", 12.0, &ShapeSettings::default());
    assert!(shaped.glyphs.is_empty());
    assert_eq!(shaped.width, 0.0);
    assert_eq!(face.text_width("", 12.0), 0.0);
}

#[test]
fn clusters_are_byte_offsets_on_character_boundaries() {
    let catalog = catalog();
    let (face, _) = face(&catalog, 400, false);
    let text = "h\u{e9}llo w\u{f6}rld \u{3b1}\u{3b2}";
    let shaped = face.shape(text, 14.0, &ShapeSettings::default());
    let mut last = 0;
    for glyph in &shaped.glyphs {
        assert!(text.is_char_boundary(glyph.cluster));
        assert!(glyph.cluster >= last, "clusters never go backwards for LTR");
        last = glyph.cluster;
    }
    assert!(last > text.chars().count(), "offsets are bytes, not chars");
}

#[test]
fn a_combining_sequence_stays_in_one_cluster() {
    let catalog = catalog();
    let (face, _) = face(&catalog, 400, false);
    let text = "e\u{301}x";
    let shaped = face.shape(text, 20.0, &ShapeSettings::default());
    let last = shaped.glyphs.last().unwrap();
    assert_eq!(last.cluster, 3, "x follows the 3-byte e + U+0301");
    assert!(shaped
        .glyphs
        .iter()
        .take_while(|g| g.cluster != 3)
        .all(|g| g.cluster == 0));
    assert!((face.text_width("e\u{301}", 20.0) - face.text_width("\u{e9}", 20.0)).abs() < 0.5);
}

#[test]
fn kerning_narrows_pairs_and_can_be_switched_off() {
    let catalog = catalog();
    let (face, _) = face(&catalog, 400, false);
    let size = 100.0;
    for pair in ["AV", "To", "Wa", "Ty"] {
        let kerned = face.shape(pair, size, &ShapeSettings::default()).width;
        let plain = face.shape(pair, size, &no_kerning()).width;
        assert!(
            kerned < plain - 0.5,
            "{pair}: {kerned} should be under {plain}"
        );
        let nominal: f32 = pair
            .chars()
            .map(|c| face.glyph_advance(face.glyph_id(c).unwrap(), size))
            .sum();
        assert!(
            (plain - nominal).abs() < 0.01,
            "{pair}: kerning off is the sum of hmtx advances"
        );
    }
}

/// The widest single-junction deviation from additivity over all letter
/// pairs: the kerning bound the property tests use.
fn kerning_bound(face: &LoadedFace, size: f32) -> f32 {
    let letters: Vec<char> = ('a'..='z').chain('A'..='Z').collect();
    let mut bound = 0.0_f32;
    for &a in &letters {
        for &b in &letters {
            let pair: String = [a, b].iter().collect();
            let deviation = (face.text_width(&pair, size)
                - face.text_width(&a.to_string(), size)
                - face.text_width(&b.to_string(), size))
            .abs();
            bound = bound.max(deviation);
        }
    }
    bound
}

#[test]
fn width_of_a_concatenation_is_the_sum_within_the_kerning_bound() {
    let catalog = catalog();
    let (face, _) = face(&catalog, 400, false);
    let size = 20.0;
    let bound = kerning_bound(&face, size);
    println!(
        "largest letter-pair kerning at {size}: {bound:.3} ({:.1}% of an em)",
        bound / size * 100.0
    );
    assert!(bound > 0.0, "Inter kerns letter pairs");
    assert!(bound < 0.25 * size, "and by well under a quarter em");

    let alphabet: Vec<char> = ('a'..='z').chain('A'..='Z').chain([' ']).collect();
    let mut rng = Lcg(0xC0FFEE);
    for case in 0..300 {
        let (len_a, len_b) = (1 + rng.below(24) as usize, 1 + rng.below(24) as usize);
        let a = rng.string(&alphabet, len_a);
        let b = rng.string(&alphabet, len_b);
        let whole = face.text_width(&format!("{a}{b}"), size);
        let parts = face.text_width(&a, size) + face.text_width(&b, size);
        assert!(
            (whole - parts).abs() <= bound + 1e-3,
            "case {case}: {a:?}+{b:?}: {whole} vs {parts} (bound {bound})"
        );
        // Without kerning the concatenation is exact.
        let plain = |s: &str| face.shape(s, size, &no_kerning()).width;
        assert!((plain(&format!("{a}{b}")) - plain(&a) - plain(&b)).abs() < 1e-3);
    }
}

#[test]
fn width_scales_linearly_with_size() {
    let catalog = catalog();
    let (face, _) = face(&catalog, 700, false);
    let text = "The quick brown fox jumps over the lazy dog";
    let base = face.text_width(text, 10.0);
    for size in [7.0_f32, 12.0, 24.0, 96.0] {
        let scaled = face.text_width(text, size);
        assert!(
            (scaled - base * size / 10.0).abs() < 0.02 * size,
            "size {size}: {scaled} vs {}",
            base * size / 10.0
        );
    }
}

#[test]
fn bold_is_wider_than_regular_and_italic_differs() {
    let catalog = catalog();
    let text = "Wonderful typography";
    let regular = face(&catalog, 400, false).0.text_width(text, 16.0);
    let bold = face(&catalog, 700, false).0.text_width(text, 16.0);
    let italic = face(&catalog, 400, true).0.text_width(text, 16.0);
    assert!(bold > regular);
    assert!((italic - regular).abs() > 0.01);
}

#[test]
fn line_breaks_and_tabs_have_no_advance() {
    let catalog = catalog();
    let (face, _) = face(&catalog, 400, false);
    let with = face.text_width("a\nb\tc", 16.0);
    let without = face.text_width("abc", 16.0);
    assert!((with - without).abs() < 0.5, "{with} vs {without}");
}

#[test]
fn line_metrics_are_positive_and_scale() {
    let catalog = catalog();
    let (face, _) = face(&catalog, 400, false);
    let m = face.line_metrics(16.0);
    assert!(m.ascent > 0.0 && m.descent > 0.0 && m.line_gap >= 0.0);
    assert!((m.line_height - (m.ascent + m.descent + m.line_gap)).abs() < 1e-4);
    let ratio = m.line_height / 16.0;
    assert!((1.0..1.5).contains(&ratio), "line height {ratio} em");
    assert!(m.cap_height.is_some_and(|c| c > 0.0 && c < m.ascent));
    let double = face.line_metrics(32.0);
    assert!((double.line_height - 2.0 * m.line_height).abs() < 0.01);
}

#[test]
fn coverage_distinguishes_latin_from_cjk() {
    let catalog = catalog();
    let (face, _) = face(&catalog, 400, false);
    for ch in [
        'A', 'z', '7', '\u{e9}', '\u{3b1}', '\u{416}', ' ', '\u{20ac}',
    ] {
        assert!(face.covers(ch), "{ch:?}");
    }
    assert!(!face.covers('\u{4e2d}'));
    // Controls and joiners never force a fallback.
    assert!(face.covers('\n') && face.covers('\u{200d}'));
    assert_eq!(face.first_uncovered("ab\u{4e2d}c"), Some('\u{4e2d}'));
    assert_eq!(face.first_uncovered("abc"), None);
    assert!(catalog.covers(catalog.faces("Inter")[0].0, 'q'));
    assert!(!catalog.covers(catalog.faces("Inter")[0].0, '\u{4e2d}'));
}

#[test]
fn caret_stops_round_trip_through_hit_testing() {
    let catalog = catalog();
    let (face, _) = face(&catalog, 400, false);
    let text = "Hello, w\u{f6}rld";
    let shaped = face.shape(text, 18.0, &ShapeSettings::default());
    let stops = shaped.caret_stops(text);
    assert_eq!(stops.first().map(|s| (s.offset, s.x)), Some((0, 0.0)));
    let last = stops.last().unwrap();
    assert_eq!(last.offset, text.len());
    assert!((last.x - shaped.width).abs() < 1e-3);
    assert_eq!(stops.len(), text.chars().count() + 1);
    for pair in stops.windows(2) {
        assert!(pair[1].offset > pair[0].offset);
        assert!(pair[1].x > pair[0].x, "LTR caret moves right");
    }
    for stop in &stops {
        assert_eq!(shaped.offset_at_x(text, stop.x), stop.offset);
        assert!((shaped.x_at_offset(text, stop.offset) - stop.x).abs() < 1e-4);
    }
    // Between two stops the nearer one wins; beyond the ends clamp.
    let mid = (stops[3].x + stops[4].x) / 2.0;
    assert_eq!(shaped.offset_at_x(text, mid - 0.01), stops[3].offset);
    assert_eq!(shaped.offset_at_x(text, mid + 0.01), stops[4].offset);
    assert_eq!(shaped.offset_at_x(text, -50.0), 0);
    assert_eq!(shaped.offset_at_x(text, 10_000.0), text.len());
}

#[test]
fn caret_stops_skip_inside_a_combining_sequence() {
    let catalog = catalog();
    let (face, _) = face(&catalog, 400, false);
    let text = "ae\u{301}b";
    let shaped = face.shape(text, 18.0, &ShapeSettings::default());
    let offsets: Vec<usize> = shaped.caret_stops(text).iter().map(|s| s.offset).collect();
    assert_eq!(offsets, [0, 1, 4, 5], "no caret between e and its accent");
}

#[test]
fn right_to_left_runs_place_the_start_of_text_at_the_right() {
    let catalog = catalog();
    let (face, _) = face(&catalog, 400, false);
    let text = "abc";
    let settings = ShapeSettings {
        direction: TextDirection::RightToLeft,
        ..ShapeSettings::default()
    };
    let shaped = face.shape(text, 20.0, &settings);
    assert!(shaped.rtl);
    // Glyphs are in visual order, so clusters run backwards.
    let clusters: Vec<usize> = shaped.glyphs.iter().map(|g| g.cluster).collect();
    assert_eq!(clusters, [2, 1, 0]);
    let stops = shaped.caret_stops(text);
    assert_eq!(stops.first().unwrap().offset, 0);
    assert!((stops.first().unwrap().x - shaped.width).abs() < 1e-3);
    assert_eq!(stops.last().unwrap().x, 0.0);
    for pair in stops.windows(2) {
        assert!(pair[1].x < pair[0].x, "later characters sit further left");
    }
    assert_eq!(shaped.offset_at_x(text, shaped.width), 0);
    assert_eq!(shaped.offset_at_x(text, 0.0), 3);
}

#[test]
fn layout_line_matches_a_single_shape_for_plain_text() {
    let catalog = catalog();
    let (face, font) = face(&catalog, 400, false);
    let text = "Plain left to right text, with numbers 12345.";
    let layout = catalog.layout_text(text, &font, 15.0, TextDirection::Auto);
    assert!(!layout.estimated);
    let direct = face.text_width(text, 15.0);
    assert!((layout.width - direct).abs() < 0.5);
    let tiled: usize = layout.runs.iter().map(|r| r.range.len()).sum();
    assert_eq!(tiled, text.len());
    assert!((catalog.text_width(text, &font, 15.0) - layout.width).abs() < 1e-6);
    // Caret mapping through the layout agrees with the run's own stops.
    for offset in [0, 5, 12, text.len()] {
        let x = layout.x_at_offset(offset, Affinity::Leading);
        assert_eq!(layout.offset_at_x(x), (offset, Affinity::Leading));
    }
}

#[test]
fn layout_line_orders_bidi_runs_visually_and_tiles_the_text() {
    let catalog = catalog();
    let (_, font) = face(&catalog, 400, false);
    let text = "abc \u{5e9}\u{5dc}\u{5d5}\u{5dd} def";
    let layout = catalog.layout_text(text, &font, 16.0, TextDirection::Auto);
    assert!(layout.runs.iter().any(|r| r.rtl));
    let mut covered: Vec<(usize, usize)> = layout
        .runs
        .iter()
        .map(|r| (r.range.start, r.range.end))
        .collect();
    covered.sort_unstable();
    let mut at = 0;
    for (start, end) in covered {
        assert_eq!(start, at);
        at = end;
    }
    assert_eq!(at, text.len());
    let mut x = 0.0;
    for run in &layout.runs {
        assert!((run.x - x).abs() < 1e-3, "runs abut left to right");
        x += run.shaped.width;
    }
    assert!((layout.width - x).abs() < 1e-3);
    // Logical start of the Hebrew run is at its right edge.
    let hebrew = text.find('\u{5e9}').unwrap();
    let run = layout.runs.iter().find(|r| r.rtl).unwrap();
    let at_start = layout.x_at_offset(hebrew, Affinity::Leading);
    assert!((at_start - (run.x + run.shaped.width)).abs() < 1e-3);
}

#[test]
fn layout_line_of_empty_text_is_empty() {
    let catalog = catalog();
    let (_, font) = face(&catalog, 400, false);
    let layout = catalog.layout_text("", &font, 16.0, TextDirection::Auto);
    assert!(layout.runs.is_empty() && layout.width == 0.0 && !layout.estimated);
    assert_eq!(layout.offset_at_x(5.0), (0, Affinity::Leading));
}

#[test]
fn layout_estimates_when_no_face_can_be_loaded() {
    let dir = common::TempDir::new("gone");
    let paths = common::write_family_variants(dir.path(), 1);
    let mut config = ScanConfig::with_dirs(vec![dir.path().to_owned()]);
    config.include_bundled = false;
    config.policy = FallbackPolicy::new(Vec::<String>::new());
    let catalog = FontCatalog::scan(&config);
    let font = catalog.resolve("F0000", 400, false).unwrap();
    std::fs::remove_file(&paths[0]).unwrap();
    let layout = catalog.layout_text("abcd", &font, 10.0, TextDirection::Auto);
    assert!(layout.estimated);
    assert!((layout.width - 20.0).abs() < 1e-4);
}

#[test]
fn shaping_is_deterministic_across_loads() {
    let text = "Deterministic shaping, kerning AV To.";
    let a = {
        let catalog = catalog();
        face(&catalog, 400, false)
            .0
            .shape(text, 13.0, &ShapeSettings::default())
    };
    let b = {
        let catalog = catalog();
        face(&catalog, 400, false)
            .0
            .shape(text, 13.0, &ShapeSettings::default())
    };
    assert_eq!(a, b);
}
