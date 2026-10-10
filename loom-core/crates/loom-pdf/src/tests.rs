use super::*;
use crate::inspect;

fn parse_xref(bytes: &[u8]) -> (usize, Vec<usize>) {
    // Operate on raw bytes: xref offsets are byte offsets, and a UTF-8
    // lossy conversion of the binary header would shift indices.
    let start = bytes
        .windows(b"startxref".len())
        .position(|w| w == b"startxref")
        .expect("startxref present");
    let after = &bytes[start + b"startxref".len()..];
    let start_pos: usize = std::str::from_utf8(after)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .parse()
        .unwrap();
    let xref = std::str::from_utf8(&bytes[start_pos..]).unwrap();
    let count_line = xref.lines().nth(1).unwrap();
    let count: usize = count_line
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    let entries: Vec<usize> = xref
        .lines()
        .skip(3)
        .take(count - 1)
        .map(|l| l.split_whitespace().next().unwrap().parse().unwrap())
        .collect();
    (start_pos, entries)
}

fn one_line(text: &str, style: &TextStyle) -> Vec<u8> {
    let mut doc = PdfDocument::new();
    let p = doc.add_page(400.0, 200.0);
    doc.draw_text(p, 10.0, 10.0, text, style);
    doc.serialize()
}

#[test]
fn serializes_valid_pdf() {
    let mut doc = PdfDocument::new();
    let p = doc.add_page(612.0, 792.0);
    doc.draw_text(p, 72.0, 720.0, "Hello, Loom!", &TextStyle::default());
    doc.draw_rect(
        p,
        72.0,
        400.0,
        100.0,
        50.0,
        PathStyle::filled((0.7, 0.3, 0.1)),
    );
    doc.draw_line(
        p,
        0.0,
        0.0,
        612.0,
        792.0,
        PathStyle::stroked((0.0, 0.0, 0.0), 1.0),
    );
    let bytes = doc.serialize();
    assert!(bytes.starts_with(b"%PDF-1.4"));
    assert!(bytes.ends_with(b"%%EOF\n"));

    let (xref_pos, entries) = parse_xref(&bytes);
    assert_eq!(
        entries.len(),
        10,
        "catalog + page + pagetree + stream + info + five objects for the one face"
    );
    assert_eq!(&bytes[entries[0]..entries[0] + 7], b"1 0 obj");
    for (i, off) in entries.iter().enumerate() {
        let header = format!("{} 0 obj", i + 1);
        assert_eq!(&bytes[*off..*off + header.len()], header.as_bytes());
    }
    assert_eq!(&bytes[xref_pos..xref_pos + 4], b"xref");
    let content = inspect::readable_content(&bytes).unwrap();
    assert!(content.contains("(Hello, Loom!) Tj"), "{content}");
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("/Type /Catalog"));
    assert!(text.contains("/MediaBox [0 0 612.00 792.00]"));
}

#[test]
fn a_page_without_text_embeds_no_font() {
    let mut doc = PdfDocument::new();
    let p = doc.add_page(100.0, 100.0);
    doc.draw_rect(p, 1.0, 1.0, 5.0, 5.0, PathStyle::filled((0.0, 0.0, 0.0)));
    let bytes = doc.serialize();
    assert!(inspect::embedded_fonts(&bytes).unwrap().is_empty());
    assert!(!String::from_utf8_lossy(&bytes).contains("/FontFile2"));
}

#[test]
fn deterministic_output() {
    let build = || {
        let mut doc = PdfDocument::new();
        let page = doc.add_page(100.0, 100.0);
        doc.draw_text(
            page,
            10.0,
            50.0,
            "same \u{3a9}\u{43f}",
            &TextStyle::default(),
        );
        let bold = TextStyle {
            bold: true,
            italic: true,
            ..Default::default()
        };
        doc.draw_text(page, 10.0, 30.0, "also same", &bold);
        doc.serialize()
    };
    assert_eq!(build(), build(), "identical input -> identical bytes");
}

#[test]
fn special_characters_round_trip_and_controls_are_dropped() {
    let bytes = one_line("a(b)\\c\nd\te", &TextStyle::default());
    assert_eq!(inspect::page_text(&bytes).unwrap(), ["a(b)\\cde"]);
    let content = inspect::readable_content(&bytes).unwrap();
    assert!(content.contains(r"(a\(b\)\\cde) Tj"), "{content}");
}

#[test]
fn full_unicode_text_round_trips_through_the_tounicode_map() {
    // Latin-1, typographic marks, Greek and Cyrillic, plus a supplementary
    // plane emoji that only the notdef box can show.
    let sample = "Caf\u{e9} \u{2014} \u{201c}quotes\u{201d} \u{2026} \
                  \u{395}\u{3bb}\u{3bb}\u{3b7}\u{3bd}\u{3b9}\u{3ba}\u{3ac} \
                  \u{41f}\u{440}\u{438}\u{432}\u{435}\u{442}";
    let bytes = one_line(sample, &TextStyle::default());
    assert_eq!(inspect::page_text(&bytes).unwrap(), [sample]);
    // Not UTF-8, and not WinAnsi bytes either: the words are glyph ids.
    assert!(!String::from_utf8_lossy(&bytes).contains("Caf"));
}

#[test]
fn characters_without_a_glyph_show_notdef_not_a_question_mark() {
    let bytes = one_line("a\u{65e5}\u{672c}b", &TextStyle::default());
    // Unmapped glyph ids read back as U+FFFD; the glyph itself is `.notdef`.
    assert_eq!(inspect::page_text(&bytes).unwrap(), ["a\u{fffd}\u{fffd}b"]);
    let wide = text_width_pt("\u{65e5}", &TextStyle::default());
    assert!(wide > 0.0, "a missing glyph still takes its notdef width");
}

#[test]
fn bold_italic_run_uses_the_bold_italic_face_and_nothing_else() {
    let style = TextStyle {
        bold: true,
        italic: true,
        ..Default::default()
    };
    let bytes = one_line("x", &style);
    let fonts = inspect::embedded_fonts(&bytes).unwrap();
    assert_eq!(fonts.len(), 1, "only the face that was used is embedded");
    assert_eq!(fonts[0].resource, "F4");
    assert!(
        fonts[0].base_font.ends_with("+Inter-BoldItalic"),
        "{}",
        fonts[0].base_font
    );
    assert!(inspect::readable_content(&bytes)
        .unwrap()
        .contains("/F4 12.00 Tf BT 10.00 10.00 Td (x) Tj ET"));
}

#[test]
fn font_is_embedded_as_a_small_subset() {
    let bytes = one_line("Hello, Loom! \u{3a9}", &TextStyle::default());
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("/FontFile2"), "font program is embedded");
    assert!(text.contains("/Subtype /CIDFontType2"));
    assert!(text.contains("/Encoding /Identity-H"));
    let fonts = inspect::embedded_fonts(&bytes).unwrap();
    assert_eq!(fonts.len(), 1);
    let font = &fonts[0];
    assert!(font.subset, "{}", font.base_font);
    assert_eq!(font.base_font.len(), "ABCDEF+Inter-Regular".len());
    // Inter Regular is about 400 KB; a dozen glyphs need a few KB.
    assert!(font.inflated_program_bytes < 12_000, "{font:?}");
    assert!(font.program_bytes < 8_000, "{font:?}");
    assert!(bytes.len() < 16_000, "whole file is {} bytes", bytes.len());
    // The ToUnicode map names every distinct character that was drawn.
    let distinct: std::collections::BTreeSet<char> = "Hello, Loom! \u{3a9}".chars().collect();
    assert_eq!(font.mapped_glyphs, distinct.len());
}

#[test]
fn subset_glyphs_keep_their_outlines_including_composites() {
    use ttf_parser::{Face, GlyphId, OutlineBuilder};

    #[derive(Default)]
    struct Bounds {
        points: Vec<(f32, f32)>,
    }
    impl OutlineBuilder for Bounds {
        fn move_to(&mut self, x: f32, y: f32) {
            self.points.push((x, y));
        }
        fn line_to(&mut self, x: f32, y: f32) {
            self.points.push((x, y));
        }
        fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
            self.points.extend([(x1, y1), (x, y)]);
        }
        fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
            self.points.extend([(x1, y1), (x2, y2), (x, y)]);
        }
        fn close(&mut self) {}
    }
    fn outline(face: &Face, gid: u16) -> Vec<(i32, i32)> {
        let mut b = Bounds::default();
        face.outline_glyph(GlyphId(gid), &mut b).expect("outline");
        b.points
            .iter()
            .map(|(x, y)| (*x as i32, *y as i32))
            .collect()
    }

    // Accented letters are composite glyphs in Inter: base + mark components.
    let sample = "Caf\u{e9}\u{c5}\u{f1}\u{3ac}\u{439}";
    let chars: Vec<char> = sample.chars().collect();
    let bytes = one_line(sample, &TextStyle::default());
    let font = &inspect::embedded_fonts(&bytes).unwrap()[0];
    let subset = Face::parse(&font.program, 0).expect("subset parses as a font");
    let original = Face::parse(
        include_bytes!("../../loom-ui/ui/fonts/Inter-Regular.ttf"),
        0,
    )
    .expect("original parses");
    // New ids are handed out in first-use order starting after .notdef.
    for (i, ch) in chars.iter().enumerate() {
        let old = original
            .glyph_index(*ch)
            .expect("Inter covers the sample")
            .0;
        let new = (i + 1) as u16;
        assert_eq!(
            outline(&subset, new),
            outline(&original, old),
            "outline of {ch:?} changed in the subset"
        );
        assert_eq!(
            subset.glyph_hor_advance(GlyphId(new)),
            original.glyph_hor_advance(GlyphId(old)),
            "advance of {ch:?}"
        );
    }
    assert!(
        subset.number_of_glyphs() < 40,
        "only the used glyphs and their components remain: {}",
        subset.number_of_glyphs()
    );
}

#[test]
fn text_width_uses_the_embedded_inter_advances() {
    let at = |bold: bool, text: &str| {
        text_width_pt(
            text,
            &TextStyle {
                size_pt: 1000.0,
                bold,
                ..Default::default()
            },
        )
    };
    // Advances read from Inter-Regular.ttf / Inter-Bold.ttf (1/1000 em),
    // matching the tables the Writer editor measures with.
    assert!((at(false, "H") - 743.0).abs() < 1.0, "{}", at(false, "H"));
    assert!((at(true, "H") - 747.0).abs() < 1.0, "{}", at(true, "H"));
    assert!((at(false, " ") - 281.0).abs() < 1.0);
    assert!((at(false, "Hello") - (743.0 + 583.0 + 242.0 + 242.0 + 600.0)).abs() < 3.0);
    assert_eq!(at(false, ""), 0.0);
    assert!((at(false, "\u{e9}") - at(false, "e")).abs() < 0.5);
    assert_eq!(at(false, "\n\t"), 0.0, "controls take no space");
    // The drawn run and the measured run agree because both read the font.
    let style = TextStyle {
        size_pt: 10.0,
        ..Default::default()
    };
    assert!((text_width_pt("Hello", &style) - at(false, "Hello") / 100.0).abs() < 1e-3);
}

#[test]
fn glyph_widths_in_the_pdf_equal_the_measured_advances() {
    let bytes = one_line("W", &TextStyle::default());
    let text = String::from_utf8_lossy(&bytes);
    let at = text.find("/W [0 [").expect("width array");
    let list: Vec<f32> = text[at + 7..]
        .split(']')
        .next()
        .unwrap()
        .split_whitespace()
        .map(|w| w.parse().unwrap())
        .collect();
    let expected = text_width_pt(
        "W",
        &TextStyle {
            size_pt: 1000.0,
            ..Default::default()
        },
    );
    assert_eq!(list.len(), 2, "notdef and W");
    assert!((list[1] - expected).abs() < 0.01, "{list:?} vs {expected}");
}

#[test]
fn mixed_styles_on_one_page_keep_their_own_fonts() {
    let mut doc = PdfDocument::new();
    let p = doc.add_page(200.0, 100.0);
    let regular = TextStyle::default();
    let bold = TextStyle {
        bold: true,
        ..Default::default()
    };
    let italic = TextStyle {
        italic: true,
        ..Default::default()
    };
    doc.draw_text(p, 10.0, 10.0, "plain", &regular);
    doc.draw_text(p, 10.0, 30.0, "heavy", &bold);
    doc.draw_text(p, 10.0, 50.0, "slanted", &italic);
    let bytes = doc.serialize();
    let content = inspect::readable_content(&bytes).unwrap();
    assert!(content.contains("/F1 12.00 Tf BT 10.00 10.00 Td (plain)"));
    assert!(content.contains("/F2 12.00 Tf BT 10.00 30.00 Td (heavy)"));
    assert!(content.contains("/F3 12.00 Tf BT 10.00 50.00 Td (slanted)"));
    let fonts = inspect::embedded_fonts(&bytes).unwrap();
    let names: Vec<&str> = fonts
        .iter()
        .map(|f| f.base_font.split('+').nth(1).unwrap())
        .collect();
    assert_eq!(names, ["Inter-Regular", "Inter-Bold", "Inter-Italic"]);
}

#[test]
fn transformed_text_keeps_glyph_y_axis_upright() {
    let mut doc = PdfDocument::new();
    let p = doc.add_page(100.0, 100.0);
    doc.draw_text_with_transform(
        p,
        0.0,
        0.0,
        "upright",
        &TextStyle::default(),
        [1.0, 0.0, 0.0, -1.0, 10.0, 20.0],
    );
    let content = inspect::readable_content(&doc.serialize()).unwrap();
    assert!(
        content.contains("BT 1 0 0 -1 0.00 0.00 Tm (upright) Tj"),
        "transformed text must invert the local text y-axis exactly once: {content}"
    );
}

#[derive(Debug)]
struct CountingFallback {
    asked: std::sync::Mutex<Vec<char>>,
    answer: Option<FallbackFont>,
}

impl FontFallback for CountingFallback {
    fn font_for(&self, ch: char, _bold: bool, _italic: bool) -> Option<FallbackFont> {
        self.asked.lock().unwrap().push(ch);
        self.answer
    }
}

#[test]
fn fallback_hook_is_asked_only_for_characters_inter_lacks() {
    let hook = Arc::new(CountingFallback {
        asked: Default::default(),
        // A face that does not cover the character is rejected too.
        answer: Some(FallbackFont {
            name: "Inter-Medium",
            data: include_bytes!("../../loom-ui/ui/fonts/Inter-Medium.ttf"),
        }),
    });
    let mut doc = PdfDocument::new();
    doc.set_font_fallback(hook.clone());
    let p = doc.add_page(100.0, 100.0);
    doc.draw_text(p, 0.0, 0.0, "ab\u{65e5}", &TextStyle::default());
    assert_eq!(*hook.asked.lock().unwrap(), ['\u{65e5}']);
    let fonts = inspect::embedded_fonts(&doc.serialize()).unwrap();
    assert_eq!(fonts.len(), 1, "an unsuitable fallback is not embedded");
}

#[test]
fn fifty_page_document_stays_small_and_fast() {
    let paragraph = "The quick brown fox jumps over the lazy dog, \u{201c}twice\u{201d} \u{2014} \
                     caf\u{e9} na\u{ef}ve r\u{e9}sum\u{e9} \u{395}\u{3bb}\u{3bb}\u{3ac}\u{3b4}\u{3b1} \
                     \u{41f}\u{440}\u{438}\u{432}\u{435}\u{442} 0123456789.";
    let start = std::time::Instant::now();
    let mut doc = PdfDocument::new();
    let regular = TextStyle::default();
    let bold = TextStyle {
        bold: true,
        ..Default::default()
    };
    for page_no in 0..50 {
        let page = doc.add_page(612.0, 792.0);
        doc.draw_text(page, 72.0, 740.0, &format!("Heading {page_no}"), &bold);
        for line in 0..46 {
            let y = 700.0 - line as f32 * 14.0;
            doc.draw_text(page, 72.0, y, paragraph, &regular);
        }
    }
    let bytes = doc.serialize();
    let elapsed = start.elapsed();
    eprintln!(
        "50 pages x 47 lines: {} bytes ({:.0} KB), {:?}",
        bytes.len(),
        bytes.len() as f64 / 1024.0,
        elapsed
    );
    assert!(bytes.len() < 400_000, "{} bytes", bytes.len());
    assert!(elapsed.as_secs() < 10, "{elapsed:?}");
    let pages = inspect::page_text(&bytes).unwrap();
    assert_eq!(pages.len(), 50);
    assert!(pages[49].starts_with("Heading 49\n"));
}
