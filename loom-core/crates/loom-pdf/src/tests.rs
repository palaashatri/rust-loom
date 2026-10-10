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

fn width_at_1000(text: &str) -> f32 {
    text_width_pt(
        text,
        &TextStyle {
            size_pt: 1000.0,
            ..Default::default()
        },
    )
}

fn embedded_glyph_count(bytes: &[u8]) -> usize {
    inspect::embedded_fonts(bytes)
        .unwrap()
        .iter()
        .map(|font| font.mapped_glyphs)
        .sum()
}

#[test]
fn invisible_format_characters_are_neither_drawn_nor_measured() {
    // Soft hyphen, zero-width space, directional marks, BOM, line and
    // paragraph separators, a variation selector and a bidi isolate.
    let noisy = "a\u{ad}b\u{200b}c\u{200e}d\u{feff}e\u{2028}f\u{2029}g\u{fe0f}h\u{2066}i\u{2069}";
    let bytes = one_line(noisy, &TextStyle::default());
    assert_eq!(inspect::page_text(&bytes).unwrap(), ["abcdefghi"]);
    assert_eq!(embedded_glyph_count(&bytes), 9, "no glyph for a control");
    assert_eq!(width_at_1000(noisy), width_at_1000("abcdefghi"));
    for ch in [
        '\u{ad}',
        '\u{200b}',
        '\u{200d}',
        '\u{2028}',
        '\u{2060}',
        '\u{fe0f}',
        '\u{feff}',
        '\u{e0100}',
    ] {
        assert_eq!(width_at_1000(&ch.to_string()), 0.0, "U+{:04X}", ch as u32);
    }
    // Nothing visible left: no text operator and no font.
    let blank = one_line("\u{200b}\u{ad}", &TextStyle::default());
    assert!(inspect::embedded_fonts(&blank).unwrap().is_empty());
    assert_eq!(inspect::page_text(&blank).unwrap(), [""]);
}

#[test]
fn accent_sequences_draw_as_one_precomposed_glyph() {
    let bytes = one_line("e\u{301}", &TextStyle::default());
    assert_eq!(inspect::page_text(&bytes).unwrap(), ["\u{e9}"]);
    assert_eq!(embedded_glyph_count(&bytes), 1);
    assert_eq!(width_at_1000("e\u{301}"), width_at_1000("\u{e9}"));
    // The same word spelled with combining marks and precomposed is the same
    // drawing: identical bytes.
    let combining = one_line("Cafe\u{301} A\u{30a} n\u{303}", &TextStyle::default());
    let precomposed = one_line("Caf\u{e9} \u{c5} \u{f1}", &TextStyle::default());
    assert_eq!(combining, precomposed);
    // A mark with no precomposed form stays after its base and keeps its width.
    let odd = one_line("x\u{301}", &TextStyle::default());
    assert_eq!(inspect::page_text(&odd).unwrap(), ["x\u{301}"]);
}

#[test]
fn emoji_sequences_draw_one_box_not_a_row_of_them() {
    let family = "x\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}y";
    let tone = "x\u{1f44d}\u{1f3fd}y";
    let flag = "x\u{1f1ee}\u{1f1f3}y";
    for sample in [family, tone, flag] {
        let bytes = one_line(sample, &TextStyle::default());
        assert_eq!(
            inspect::page_text(&bytes).unwrap(),
            ["x\u{fffd}y"],
            "{sample:?}: one notdef between the letters"
        );
        assert_eq!(
            width_at_1000(sample),
            width_at_1000("x\u{1f468}y"),
            "{sample:?}"
        );
    }
}

#[test]
fn measuring_a_huge_run_does_not_overflow() {
    // 2,000,000 capital Ws are about 5e9 font units: more than a u32 holds.
    let text = "W".repeat(2_000_000);
    let width = width_at_1000(&text);
    let one = width_at_1000("W");
    assert!(width.is_finite());
    assert!((width - one * 2_000_000.0).abs() / width < 1e-4, "{width}");
}

#[test]
fn characters_that_share_a_glyph_copy_out_as_themselves() {
    // Inter draws U+2019 and U+02BC with one glyph, so the ToUnicode map can
    // name only one of them: the first drawn. The other carries ActualText.
    let both = "\u{2019}\u{2bc}\u{2019}\u{2bc}";
    let bytes = one_line(both, &TextStyle::default());
    inspect::verify(&bytes).unwrap();
    assert_eq!(inspect::page_text(&bytes).unwrap(), [both]);
    assert_eq!(embedded_glyph_count(&bytes), 1, "one glyph, one entry");
    let content = inspect::readable_content(&bytes).unwrap();
    assert!(content.contains("/ActualText"), "{content}");
    // The first character drawn owns the entry: reversing the text moves it.
    let reversed = one_line("\u{2bc}\u{2019}", &TextStyle::default());
    assert_eq!(inspect::page_text(&reversed).unwrap(), ["\u{2bc}\u{2019}"]);
    let first = inspect::readable_content(&reversed).unwrap();
    assert_eq!(first.matches("/ActualText").count(), 1, "{first}");
    // Canonical equivalents are one character: ohm sign copies as Omega,
    // with no span, because the text is normalised before it is drawn.
    let ohm = one_line("\u{2126}\u{3a9}", &TextStyle::default());
    assert_eq!(inspect::page_text(&ohm).unwrap(), ["\u{3a9}\u{3a9}"]);
    assert!(!inspect::readable_content(&ohm)
        .unwrap()
        .contains("/ActualText"));
}

#[test]
fn a_ligature_code_point_inter_lacks_draws_as_its_letters() {
    assert!(fonts::inter(0).glyph('\u{fb01}').is_none(), "premise");
    let bytes = one_line("of\u{fb01}ce \u{fb03}", &TextStyle::default());
    assert_eq!(inspect::page_text(&bytes).unwrap(), ["office ffi"]);
    assert_eq!(width_at_1000("\u{fb01}"), width_at_1000("fi"));
    assert_eq!(width_at_1000("\u{fb03}"), width_at_1000("ffi"));
    // A fallback hook that cannot cover it does not turn it into a box.
    let hook = Arc::new(CountingFallback {
        asked: Default::default(),
        answer: None,
    });
    let mut doc = PdfDocument::new();
    doc.set_font_fallback(hook.clone());
    let page = doc.add_page(100.0, 100.0);
    doc.draw_text(page, 0.0, 0.0, "\u{fb02}", &TextStyle::default());
    assert_eq!(inspect::page_text(&doc.serialize()).unwrap(), ["fl"]);
}

#[test]
fn the_fallback_hook_runs_once_per_character_and_style() {
    let hook = Arc::new(CountingFallback {
        asked: Default::default(),
        answer: None,
    });
    let mut doc = PdfDocument::new();
    doc.set_font_fallback(hook.clone());
    let page = doc.add_page(100.0, 100.0);
    let bold = TextStyle {
        bold: true,
        ..Default::default()
    };
    doc.draw_text(
        page,
        0.0,
        0.0,
        "\u{65e5}\u{65e5}\u{672c}\u{65e5}",
        &TextStyle::default(),
    );
    doc.draw_text(page, 0.0, 20.0, "\u{65e5}\u{672c}", &TextStyle::default());
    assert_eq!(*hook.asked.lock().unwrap(), ['\u{65e5}', '\u{672c}']);
    doc.draw_text(page, 0.0, 40.0, "\u{65e5}\u{65e5}", &bold);
    assert_eq!(
        *hook.asked.lock().unwrap(),
        ['\u{65e5}', '\u{672c}', '\u{65e5}'],
        "a different style asks again, once"
    );
}

#[test]
fn unusable_fallback_faces_are_refused_with_a_reason() {
    static MEDIUM: &[u8] = include_bytes!("../../loom-ui/ui/fonts/Inter-Medium.ttf");
    let leaked = |bytes: Vec<u8>| -> &'static [u8] { Box::leak(bytes.into_boxed_slice()) };
    let mut doc = PdfDocument::new();

    // A name that is a bundled face would be merged with it.
    assert!(doc
        .fallback_font(FallbackFont {
            name: "Inter-Regular",
            data: MEDIUM,
        })
        .is_none());
    // A font collection must never be embedded whole as FontFile2.
    assert!(doc
        .fallback_font(FallbackFont {
            name: "Collection-Regular",
            data: leaked(b"ttcf\0\x01\0\0\0\0\0\x01".to_vec()),
        })
        .is_none());
    // CFF-flavoured OpenType cannot be written as CIDFontType2/FontFile2.
    let mut cff = MEDIUM.to_vec();
    cff[..4].copy_from_slice(b"OTTO");
    assert!(doc
        .fallback_font(FallbackFont {
            name: "Cff-Regular",
            data: leaked(cff),
        })
        .is_none());
    // Not a font at all.
    assert!(doc
        .fallback_font(FallbackFont {
            name: "Junk-Regular",
            data: b"not a font",
        })
        .is_none());
    let problems = doc.fallback_problems().join("\n");
    for expected in ["bundled", "collection", "CFF", "usable"] {
        assert!(problems.contains(expected), "{expected}: {problems}");
    }

    // A good face is accepted once per name; the same name for different
    // bytes is refused rather than silently merged.
    let good = FallbackFont {
        name: "Medium-Fallback",
        data: MEDIUM,
    };
    assert!(doc.fallback_font(good).is_some());
    assert!(doc.fallback_font(good).is_some());
    assert_eq!(doc.fallback_fonts.len(), 5, "parsed once per name");
    let other = FallbackFont {
        name: "Medium-Fallback",
        data: include_bytes!("../../loom-ui/ui/fonts/Inter-SemiBold.ttf"),
    };
    assert!(doc.fallback_font(other).is_none());
    assert!(doc
        .fallback_problems()
        .iter()
        .any(|p| p.contains("two different font programs")));
}

#[test]
fn fallback_faces_get_unique_resource_names_after_the_inter_faces() {
    static MEDIUM: &[u8] = include_bytes!("../../loom-ui/ui/fonts/Inter-Medium.ttf");
    static SEMIBOLD: &[u8] = include_bytes!("../../loom-ui/ui/fonts/Inter-SemiBold.ttf");
    let mut doc = PdfDocument::new();
    let first = doc
        .fallback_font(FallbackFont {
            name: "Medium-Fallback",
            data: MEDIUM,
        })
        .expect("accepted");
    let second = doc
        .fallback_font(FallbackFont {
            name: "SemiBold-Fallback",
            data: SEMIBOLD,
        })
        .expect("accepted");
    let a = doc.fallback_slot(first.clone());
    let b = doc.fallback_slot(second);
    assert_eq!(doc.fallback_slot(first), a, "the same face reuses its slot");
    let regular = doc.inter_slot(0);
    let resources: Vec<&str> = [regular, a, b]
        .iter()
        .map(|slot| doc.faces[*slot].resource.as_str())
        .collect();
    assert_eq!(resources, ["F1", "F5", "F6"]);
}

#[test]
fn a_document_without_pages_is_written_with_one_blank_page() {
    let bytes = PdfDocument::new().serialize();
    inspect::verify(&bytes).unwrap();
    assert_eq!(inspect::page_text(&bytes).unwrap(), [""]);
    assert!(String::from_utf8_lossy(&bytes).contains("/Count 1"));
}

#[test]
fn non_finite_numbers_never_reach_the_file() {
    let mut doc = PdfDocument::new();
    assert!(doc.try_add_page(f32::NAN, 100.0).is_err());
    assert!(doc.try_add_page(100.0, f32::INFINITY).is_err());
    assert!(doc.try_add_page(0.0, 100.0).is_err());
    assert!(doc.try_add_page(100.0, 20_000.0).is_err());
    assert!(doc.try_add_page(595.0, 842.0).is_ok());
    // `add_page` cannot fail: an unusable size becomes US Letter.
    let page = doc.add_page(f32::NAN, f32::NAN);
    doc.draw_text(
        page,
        f32::NAN,
        f32::INFINITY,
        "text",
        &TextStyle {
            size_pt: f32::NAN,
            fill_rgb: (f32::NAN, 0.0, 0.0),
            ..Default::default()
        },
    );
    doc.draw_rect(
        page,
        f32::NAN,
        0.0,
        f32::INFINITY,
        1.0,
        PathStyle::filled((f32::NAN, 0.0, 0.0)),
    );
    doc.draw_line(
        page,
        0.0,
        f32::NAN,
        1.0,
        1.0,
        PathStyle::stroked((0.0, 0.0, 0.0), f32::NAN),
    );
    doc.draw_rect_with_transform(
        page,
        (0.0, 0.0, 1.0, 1.0),
        PathStyle::filled((0.0, 0.0, 0.0)),
        [f32::NAN, 0.0, 0.0, 1.0, 0.0, 0.0],
    );
    let bytes = doc.serialize();
    inspect::verify(&bytes).unwrap();
    let all = format!(
        "{}\n{}",
        String::from_utf8_lossy(&bytes),
        inspect::readable_content(&bytes).unwrap()
    );
    for bad in ["NaN", "inf"] {
        assert!(!all.contains(bad), "{bad} in the file");
    }
    assert!(all.contains("/MediaBox [0 0 612.00 792.00]"));
    assert_eq!(inspect::page_text(&bytes).unwrap().len(), 2);
}

/// Byte offset of the first occurrence of `needle`.
fn find_bytes(haystack: &[u8], needle: &[u8]) -> usize {
    haystack
        .windows(needle.len())
        .position(|w| w == needle)
        .unwrap_or_else(|| panic!("{} not found", String::from_utf8_lossy(needle)))
}

/// Replace the first occurrence of `from` with `to` (same length).
fn patched(bytes: &[u8], from: &[u8], to: &[u8]) -> Vec<u8> {
    assert_eq!(from.len(), to.len());
    let at = bytes
        .windows(from.len())
        .position(|w| w == from)
        .unwrap_or_else(|| panic!("{} not found", String::from_utf8_lossy(from)));
    let mut out = bytes.to_vec();
    out[at..at + to.len()].copy_from_slice(to);
    out
}

fn mixed_document() -> Vec<u8> {
    let mut doc = PdfDocument::new();
    let page = doc.add_page(300.0, 200.0);
    doc.draw_text(
        page,
        10.0,
        150.0,
        "WAVE Caf\u{e9} \u{3a9}",
        &TextStyle::default(),
    );
    let bold = TextStyle {
        bold: true,
        ..Default::default()
    };
    doc.draw_text(page, 10.0, 120.0, "Heading \u{2019}\u{2bc}", &bold);
    let id = doc.add_image(PdfImage::rgb(1, 1, vec![1, 2, 3], Some(vec![200])).unwrap());
    doc.draw_image_with_transform(page, id, (5.0, 5.0), [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    doc.add_page(100.0, 100.0);
    doc.serialize()
}

#[test]
fn verify_accepts_a_document_with_text_images_and_several_faces() {
    inspect::verify(&mixed_document()).unwrap();
}

#[test]
fn inspect_rejects_a_stream_whose_declared_length_is_wrong() {
    let good = mixed_document();
    // The first /Length belongs to a page content stream. Keep the digit
    // count so the xref offsets stay valid.
    let at = good
        .windows(b"/Length ".len())
        .position(|w| w == b"/Length ")
        .unwrap()
        + b"/Length ".len();
    let digits: String = good[at..]
        .iter()
        .take_while(|b| b.is_ascii_digit())
        .map(|b| char::from(*b))
        .collect();
    let value: u32 = digits.parse().unwrap();
    let wrong = if value % 10 == 9 {
        value - 1
    } else {
        value + 1
    };
    let mut bytes = good.clone();
    bytes[at..at + digits.len()].copy_from_slice(wrong.to_string().as_bytes());
    let error = inspect::verify(&bytes).unwrap_err();
    assert!(error.contains("/Length"), "{error}");
}

#[test]
fn inspect_rejects_widths_out_of_glyph_order() {
    let good = mixed_document();
    let text = String::from_utf8_lossy(&good).into_owned();
    let at = text.find("/W [0 [").unwrap() + "/W [0 [".len();
    let list: Vec<&str> = text[at..].split(']').next().unwrap().split(' ').collect();
    assert!(list.len() >= 3, "{list:?}");
    // Swap two different widths in place (the byte length is unchanged).
    let (i, j) = (1..list.len())
        .flat_map(|i| (i + 1..list.len()).map(move |j| (i, j)))
        .find(|(i, j)| list[*i] != list[*j] && list[*i].len() == list[*j].len())
        .expect("two different widths of the same length");
    let mut swapped: Vec<&str> = list.clone();
    swapped.swap(i, j);
    let before = list.join(" ");
    let after = swapped.join(" ");
    let bytes = patched(&good, before.as_bytes(), after.as_bytes());
    let error = inspect::verify(&bytes).unwrap_err();
    assert!(error.contains("/W"), "{error}");
}

#[test]
fn inspect_rejects_a_bfchar_block_with_the_wrong_count() {
    let good = mixed_document();
    let at = find_bytes(&good, b" beginbfchar");
    let digit = good[at - 1];
    let mut bytes = good.clone();
    bytes[at - 1] = if digit == b'9' { b'8' } else { digit + 1 };
    let error = inspect::verify(&bytes).unwrap_err();
    assert!(error.contains("bfchar"), "{error}");
}

#[test]
fn inspect_rejects_an_xref_offset_that_misses_its_object() {
    let good = mixed_document();
    let table = find_bytes(&good, b"0000000000 65535 f");
    // The first in-use entry: bump its last digit.
    let entry = table + "0000000000 65535 f \n".len();
    let mut bytes = good.clone();
    let digit = &mut bytes[entry + 9];
    *digit = if *digit == b'9' { b'8' } else { *digit + 1 };
    let error = inspect::verify(&bytes).unwrap_err();
    assert!(error.contains("xref"), "{error}");
}

#[test]
fn inspect_rejects_a_trailer_size_that_disagrees_with_the_xref() {
    let good = mixed_document();
    let size = find_bytes(&good, b"/Size ") + "/Size ".len();
    let mut bytes = good.clone();
    bytes[size] = if bytes[size] == b'9' {
        b'8'
    } else {
        bytes[size] + 1
    };
    assert!(inspect::verify(&bytes).unwrap_err().contains("/Size"));
}

#[test]
fn inspect_rejects_a_shown_glyph_the_tounicode_map_does_not_name() {
    let good = mixed_document();
    // Rename glyph 1 of the first font in its map; the page still shows it.
    let bytes = patched(&good, b"<0001> <", b"<0009> <");
    let error = inspect::verify(&bytes).unwrap_err();
    assert!(error.contains("ToUnicode"), "{error}");
}

#[test]
fn inspect_rejects_a_page_count_that_disagrees_with_the_kids() {
    let good = mixed_document();
    let bytes = patched(&good, b"/Count 2", b"/Count 3");
    assert!(inspect::verify(&bytes).unwrap_err().contains("/Count"));
}

/// Runs `qpdf --check` over some documents when qpdf is installed; an
/// absent qpdf skips the test, because the repository does not require it.
#[test]
fn qpdf_accepts_the_files_when_it_is_installed() {
    let probe = std::process::Command::new("qpdf").arg("--version").output();
    if probe.is_err() {
        eprintln!("qpdf not installed: skipping the external structure check");
        return;
    }
    let dir = std::env::temp_dir().join(format!("loom-pdf-qpdf-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let samples = [
        ("mixed", mixed_document()),
        ("empty", PdfDocument::new().serialize()),
        (
            "shared",
            one_line("\u{2019}\u{2bc} fi\u{fb01} e\u{301}", &TextStyle::default()),
        ),
    ];
    for (name, bytes) in samples {
        let path = dir.join(format!("{name}.pdf"));
        std::fs::write(&path, &bytes).unwrap();
        let out = std::process::Command::new("qpdf")
            .arg("--check")
            .arg(&path)
            .output()
            .expect("qpdf runs");
        // 0 = clean, 3 = warnings only; 2 = the file is damaged.
        assert!(
            matches!(out.status.code(), Some(0 | 3)),
            "{name}: {}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}
