//! A run spaced by a [`RunShaper`]: kerning reaches the file as `TJ`
//! adjustments equal to the shaper's difference from the font's own advances,
//! and nothing else about the page changes.

use std::sync::Arc;

use loom_fonts::{FontCatalog, ShapeSettings};

use crate::{inspect, ClusterAdvance, PdfDocument, RunShaper, TextStyle};

/// Shapes with `loom-fonts` (kerning on) one character per cluster. Only
/// valid for text that shapes one glyph per character, which plain ASCII does
/// in Inter.
#[derive(Debug)]
struct KerningShaper;

impl RunShaper for KerningShaper {
    fn clusters(&self, text: &str, bold: bool, italic: bool) -> Option<Vec<ClusterAdvance>> {
        let catalog = FontCatalog::bundled_only();
        let font = catalog
            .resolve("Inter", if bold { 700 } else { 400 }, italic)
            .ok()?;
        let face = catalog.load(font.primary()).ok()?;
        let shaped = face.shape(text, 1000.0, &ShapeSettings::default());
        let stops = shaped.caret_stops(text);
        if stops.len() != text.chars().count() + 1 {
            return None;
        }
        Some(
            stops
                .windows(2)
                .map(|pair| ClusterAdvance {
                    chars: 1,
                    advance: Some(pair[1].x - pair[0].x),
                })
                .collect(),
        )
    }
}

fn draw(text: &str, style: &TextStyle, shaper: Option<Arc<dyn RunShaper>>) -> Vec<u8> {
    let mut doc = PdfDocument::new();
    if let Some(shaper) = shaper {
        doc.set_run_shaper(shaper);
    }
    let page = doc.add_page(400.0, 200.0);
    doc.draw_text(page, 10.0, 100.0, text, style);
    doc.serialize()
}

fn shaper(shaper: impl RunShaper + 'static) -> Option<Arc<dyn RunShaper>> {
    Some(Arc::new(shaper))
}

fn kerning() -> Option<Arc<dyn RunShaper>> {
    shaper(KerningShaper)
}

#[test]
fn kerned_pairs_are_written_as_adjustments_that_add_up_to_the_shaped_width() {
    let style = TextStyle {
        size_pt: 12.0,
        ..TextStyle::default()
    };
    let text = "AVATAR Toyota Wave";
    let pdf = draw(text, &style, kerning());
    let adjustments = inspect::spacing_adjustments(&pdf).expect("adjustments");
    assert!(
        adjustments.iter().filter(|a| **a > 0.0).count() >= 4,
        "AV, VA, AT, TA... tighten: {adjustments:?}"
    );

    let catalog = FontCatalog::bundled_only();
    let font = catalog.resolve("Inter", 400, false).expect("Inter");
    let face = catalog.load(font.primary()).expect("face");
    let kerned = face.text_width(text, style.size_pt);
    let natural = crate::text_width_pt(text, &style);
    let moved: f32 = adjustments.iter().sum::<f32>() * style.size_pt / 1000.0;
    assert!(natural - kerned > 0.5, "the sample really kerns");
    assert!(
        (natural - kerned - moved).abs() < 0.01,
        "the file moves the pen by {moved} pt; the shaper says {} pt",
        natural - kerned
    );
}

#[test]
fn a_spaced_run_reads_back_as_the_same_text_and_a_valid_file() {
    let style = TextStyle {
        bold: true,
        italic: true,
        ..TextStyle::default()
    };
    let spaced = draw("AVATAR Toyota", &style, kerning());
    let plain = draw("AVATAR Toyota", &style, None);
    assert_eq!(
        inspect::page_text(&spaced).expect("text"),
        inspect::page_text(&plain).expect("text")
    );
    assert_eq!(inspect::page_text(&spaced).unwrap(), ["AVATAR Toyota"]);
    inspect::verify(&spaced).expect("a spaced run is a consistent file");
    // The readable form shows the run as a plain literal and `Tj`.
    let content = inspect::readable_content(&spaced).expect("content");
    assert!(content.contains("(AVATAR Toyota) Tj"), "{content}");
    assert!(inspect::spacing_adjustments(&plain).unwrap().is_empty());
    assert!(!String::from_utf8_lossy(&plain).contains(" TJ"));
}

#[test]
fn without_a_shaper_the_bytes_are_those_of_a_plain_run() {
    // The same document drawn twice: no shaper anywhere, identical bytes.
    let style = TextStyle::default();
    assert_eq!(
        draw("AVATAR Toyota Wave", &style, None),
        draw("AVATAR Toyota Wave", &style, None)
    );
    let content = inspect::readable_content(&draw("AVATAR", &style, None)).unwrap();
    assert!(content.contains("(AVATAR) Tj"), "{content}");
}

#[test]
fn characters_inter_lacks_and_shared_glyph_spans_keep_their_text() {
    // A character drawn through an /ActualText span (U+02BC shares U+2019's
    // glyph), a notdef box and kerned Latin around them.
    let style = TextStyle::default();
    let text = "AV\u{2019}\u{2bc}AV \u{4e2d}AV";
    let natural = draw(text, &style, None);
    let spaced = draw(text, &style, shaper(Narrow));
    assert_eq!(
        inspect::page_text(&spaced).unwrap(),
        inspect::page_text(&natural).unwrap()
    );
    // The shared-glyph characters keep their own text through /ActualText;
    // the box for U+4E2D carries none.
    assert_eq!(
        inspect::page_text(&spaced).unwrap(),
        ["AV\u{2019}\u{2bc}AV \u{fffd}AV"]
    );
    inspect::verify(&spaced).expect("consistent");
    // Nine characters are set in Inter; the box for U+4E2D is left alone and
    // nothing trails the last glyph.
    assert_eq!(inspect::spacing_adjustments(&spaced).unwrap().len(), 8);
}

/// A shaper that sets every character 300/1000 em wide.
#[derive(Debug)]
struct Narrow;

impl RunShaper for Narrow {
    fn clusters(&self, text: &str, _bold: bool, _italic: bool) -> Option<Vec<ClusterAdvance>> {
        Some(
            text.chars()
                .map(|_| ClusterAdvance {
                    chars: 1,
                    advance: Some(300.0),
                })
                .collect(),
        )
    }
}

#[test]
fn a_shaper_that_does_not_describe_the_run_is_ignored() {
    #[derive(Debug)]
    struct Wrong;
    impl RunShaper for Wrong {
        fn clusters(&self, _: &str, _: bool, _: bool) -> Option<Vec<ClusterAdvance>> {
            Some(vec![ClusterAdvance {
                chars: 2,
                advance: Some(10.0),
            }])
        }
    }
    let style = TextStyle::default();
    let ignored = draw("ABCDEF", &style, shaper(Wrong));
    assert_eq!(ignored, draw("ABCDEF", &style, None));
}
