use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use loom_fonts::{FontCatalog, ScanConfig, ShapeSettings, Shaped, TextDirection};
use ttf_parser::{Face, GlyphId, OutlineBuilder};

use crate::{inspect, PdfDocument, PdfFontProgram, PositionedShapedRun};

fn script_face(postscript_name: &str) -> Arc<loom_fonts::LoadedFace> {
    catalog_face(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../loom-fonts/assets/fonts"),
        postscript_name,
    )
}

fn inter_face() -> Arc<loom_fonts::LoadedFace> {
    catalog_face(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../loom-ui/ui/fonts"),
        "Inter-Regular",
    )
}

fn catalog_face(directory: impl AsRef<Path>, postscript_name: &str) -> Arc<loom_fonts::LoadedFace> {
    let catalog = FontCatalog::scan(&ScanConfig::with_dirs(vec![directory
        .as_ref()
        .to_path_buf()]));
    let (id, _) = catalog
        .faces_in_order()
        .find(|(_, info)| info.postscript_name == postscript_name)
        .unwrap_or_else(|| panic!("{postscript_name} is present in the test font directory"));
    catalog.load(id).expect("load test face")
}

fn register_script_face(pdf: &mut PdfDocument, face: &loom_fonts::LoadedFace) -> crate::PdfFontId {
    pdf.register_font_program(
        PdfFontProgram::new(Arc::<[u8]>::from(face.bytes()), face.collection_index())
            .expect("make an owned font program"),
    )
    .expect("register the source font")
}

fn shape_rtl(face: &loom_fonts::LoadedFace, text: &str, script: [u8; 4]) -> Shaped {
    face.shape(
        text,
        12.0,
        &ShapeSettings {
            direction: TextDirection::RightToLeft,
            script: Some(script),
            ..ShapeSettings::default()
        },
    )
}

#[derive(Debug, PartialEq, Eq)]
enum OutlineOp {
    Move(u32, u32),
    Line(u32, u32),
    Quad(u32, u32, u32, u32),
    Curve(u32, u32, u32, u32, u32, u32),
    Close,
}

#[derive(Default)]
struct Outline(Vec<OutlineOp>);

impl OutlineBuilder for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.push(OutlineOp::Move(x.to_bits(), y.to_bits()));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.0.push(OutlineOp::Line(x.to_bits(), y.to_bits()));
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.0.push(OutlineOp::Quad(
            x1.to_bits(),
            y1.to_bits(),
            x.to_bits(),
            y.to_bits(),
        ));
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.0.push(OutlineOp::Curve(
            x1.to_bits(),
            y1.to_bits(),
            x2.to_bits(),
            y2.to_bits(),
            x.to_bits(),
            y.to_bits(),
        ));
    }

    fn close(&mut self) {
        self.0.push(OutlineOp::Close);
    }
}

fn outline(face: &Face<'_>, glyph: u16) -> Vec<OutlineOp> {
    let mut outline = Outline::default();
    assert!(
        face.outline_glyph(GlyphId(glyph), &mut outline).is_some(),
        "glyph {glyph} has an outline"
    );
    outline.0
}

fn pdf_number(value: f32) -> f32 {
    let rounded = (value * 1000.0).round() / 1000.0;
    format!("{rounded:.3}")
        .parse()
        .expect("formatted PDF number")
}

/// Compares the font program's emitted subset outlines and text matrices with
/// the independent source face and HarfRust's actual positioned glyphs.
struct SourceGlyphRun<'a> {
    postscript_name: &'a str,
    source_bytes: &'a [u8],
    source_index: u32,
    text: &'a str,
    shaped: &'a Shaped,
    origin_x: f32,
    origin_y: f32,
}

fn assert_emitted_glyphs_match_source(pdf: &[u8], run: SourceGlyphRun<'_>) {
    let SourceGlyphRun {
        postscript_name,
        source_bytes,
        source_index,
        text,
        shaped,
        origin_x,
        origin_y,
    } = run;
    let embedded = inspect::embedded_fonts(pdf)
        .expect("read embedded font")
        .into_iter()
        .find(|font| font.base_font.ends_with(postscript_name))
        .unwrap_or_else(|| panic!("PDF embeds {postscript_name}"));
    assert!(embedded.subset, "test face is subsettable");
    let source = Face::parse(source_bytes, source_index).expect("parse original source face");
    let subset = Face::parse(&embedded.program, 0).expect("parse emitted subset face");

    let mut by_cluster: BTreeMap<usize, Vec<(u16, f32, f32)>> = BTreeMap::new();
    let mut pen_x = f64::from(origin_x);
    for glyph in &shaped.glyphs {
        let x = (pen_x + f64::from(glyph.x_offset)) as f32;
        let y = (f64::from(origin_y) + f64::from(glyph.y_offset)) as f32;
        by_cluster.entry(glyph.cluster).or_default().push((
            glyph.glyph_id,
            pdf_number(x),
            pdf_number(y),
        ));
        pen_x += f64::from(glyph.advance);
    }
    let starts: Vec<_> = by_cluster.keys().copied().collect();
    let mut expected = Vec::new();
    for (index, start) in starts.iter().copied().enumerate() {
        let end = starts.get(index + 1).copied().unwrap_or(text.len());
        let cluster_text = &text[start..end];
        for (source_gid, x, y) in by_cluster.remove(&start).unwrap_or_default() {
            expected.push((source_gid, x, y, cluster_text.to_string()));
        }
    }
    let mut first_cluster_by_gid = std::collections::HashMap::new();
    for (source_gid, _, _, cluster_text) in &expected {
        first_cluster_by_gid
            .entry(*source_gid)
            .or_insert_with(|| cluster_text.clone());
    }
    let draws: Vec<_> = inspect::positioned_glyph_draws(pdf)
        .expect("read positioned glyph operations")
        .into_iter()
        .filter(|draw| draw.resource == embedded.resource)
        .collect();
    assert!(
        draws.len() >= expected.len(),
        "each expected glyph is drawn"
    );

    let mut remaining = draws;
    for (source_gid, x, y, _) in expected {
        let at = remaining
            .iter()
            .position(|draw| (draw.x, draw.y) == (x, y))
            .unwrap_or_else(|| panic!("source glyph at ({x}, {y}) has a positioned draw"));
        let draw = remaining.remove(at);
        assert_eq!(
            embedded.to_unicode.get(&draw.cid),
            first_cluster_by_gid.get(&source_gid),
            "ToUnicode maps emitted CID {} to its first logical cluster",
            draw.cid
        );
        let source_outline = outline(&source, source_gid);
        let subset_outline = outline(&subset, draw.cid);
        assert_eq!(
            subset_outline, source_outline,
            "emitted CID {} at ({}, {}) preserves source GID {} outline",
            draw.cid, draw.x, draw.y, source_gid
        );
    }
}

#[test]
fn one_actualtext_span_replaces_every_glyph_until_emc() {
    let mut pdf = PdfDocument::new();
    let page = pdf.add_page(100.0, 100.0);
    let slot = pdf.inter_slot(0);
    let first = pdf.faces[slot].encode('A');
    let second = pdf.faces[slot].encode('B');
    pdf.page_mut(page).ops.push(format!(
        "/Span << /ActualText <FEFF00660069> >> BDC BT /F1 12 Tf \
         1 0 0 1 10 10 Tm <{first:04X}> Tj 1 0 0 1 20 10 Tm <{second:04X}> Tj ET EMC"
    ));

    let extracted = inspect::page_text(&pdf.serialize()).expect("read PDF text");
    assert_eq!(
        extracted,
        ["fi"],
        "one marked-content span has one logical replacement"
    );
}

#[test]
fn pdf_emits_naskh_contextual_diacritic_glyphs_and_positions() {
    let face = script_face("NotoNaskhArabic-Regular");
    let text = "اللّٰه";
    let shaped = shape_rtl(&face, text, *b"Arab");
    assert!(
        shaped.glyphs.len() < text.chars().count(),
        "the Arabic contextual/diacritic fixture uses fewer glyphs than scalars"
    );
    let cluster_count = shaped
        .glyphs
        .iter()
        .map(|glyph| glyph.cluster)
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    assert!(
        cluster_count < shaped.glyphs.len(),
        "fixture has a multi-glyph cluster"
    );

    let mut pdf = PdfDocument::new();
    let page = pdf.add_page(300.0, 100.0);
    let font = register_script_face(&mut pdf, &face);
    let run = PositionedShapedRun {
        x: 10.0,
        y: 50.0,
        text,
        shaped: &shaped,
        font,
        size_pt: 12.0,
        fill_rgb: (0.0, 0.0, 0.0),
    };
    pdf.draw_shaped_text_line(page, &[run])
        .expect("draw the shaped glyph sequence");
    let bytes = pdf.serialize();
    inspect::verify(&bytes).expect("emitted PDF is internally consistent");
    assert_eq!(inspect::page_text(&bytes).unwrap(), [text]);
    assert_eq!(
        inspect::readable_content(&bytes)
            .unwrap()
            .matches("/ActualText")
            .count(),
        cluster_count,
        "one logical replacement span per text cluster"
    );
    assert_eq!(
        inspect::positioned_glyph_draws(&bytes).unwrap().len(),
        shaped.glyphs.len()
    );
    assert_emitted_glyphs_match_source(
        &bytes,
        SourceGlyphRun {
            postscript_name: "NotoNaskhArabic-Regular",
            source_bytes: face.bytes(),
            source_index: face.collection_index(),
            text,
            shaped: &shaped,
            origin_x: 10.0,
            origin_y: 50.0,
        },
    );
}

#[test]
fn pdf_preserves_hebrew_mark_offsets_and_subset_outlines() {
    let face = script_face("NotoSansHebrew-Regular");
    let text = "שָׁלוֹם";
    let shaped = shape_rtl(&face, text, *b"Hebr");
    assert!(
        shaped
            .glyphs
            .iter()
            .any(|glyph| glyph.x_offset != 0.0 || glyph.y_offset != 0.0),
        "Hebrew mark placement comes from nonzero shaper offsets"
    );

    let mut pdf = PdfDocument::new();
    let page = pdf.add_page(300.0, 100.0);
    let font = register_script_face(&mut pdf, &face);
    pdf.draw_shaped_text_line(
        page,
        &[PositionedShapedRun {
            x: 10.0,
            y: 50.0,
            text,
            shaped: &shaped,
            font,
            size_pt: 12.0,
            fill_rgb: (0.0, 0.0, 0.0),
        }],
    )
    .expect("draw Hebrew glyphs");
    let bytes = pdf.serialize();
    assert_eq!(inspect::page_text(&bytes).unwrap(), [text]);
    assert_eq!(
        inspect::positioned_glyph_draws(&bytes).unwrap().len(),
        shaped.glyphs.len()
    );
    assert_emitted_glyphs_match_source(
        &bytes,
        SourceGlyphRun {
            postscript_name: "NotoSansHebrew-Regular",
            source_bytes: face.bytes(),
            source_index: face.collection_index(),
            text,
            shaped: &shaped,
            origin_x: 10.0,
            origin_y: 50.0,
        },
    );
}

#[test]
fn mixed_logical_runs_extract_in_order_while_using_each_source_font() {
    let inter = inter_face();
    let hebrew = script_face("NotoSansHebrew-Regular");
    let arabic = script_face("NotoNaskhArabic-Regular");
    let hebrew_text = "שָׁלוֹם";
    let arabic_text = "اللّٰه";
    let settings = ShapeSettings::default();
    let left = "A";
    let right = "Z";
    let left_shaped = inter.shape(left, 12.0, &settings);
    let right_shaped = inter.shape(right, 12.0, &settings);
    let hebrew_shaped = shape_rtl(&hebrew, hebrew_text, *b"Hebr");
    let arabic_shaped = shape_rtl(&arabic, arabic_text, *b"Arab");

    let mut pdf = PdfDocument::new();
    let page = pdf.add_page(500.0, 100.0);
    let inter_font = pdf.inter_font_id(false, false);
    let hebrew_font = register_script_face(&mut pdf, &hebrew);
    let arabic_font = register_script_face(&mut pdf, &arabic);
    let x0 = 10.0;
    let x_hebrew = x0 + left_shaped.width;
    let x_arabic = x_hebrew + hebrew_shaped.width;
    let x_right = x_arabic + arabic_shaped.width;
    let runs = [
        PositionedShapedRun {
            x: x0,
            y: 50.0,
            text: left,
            shaped: &left_shaped,
            font: inter_font,
            size_pt: 12.0,
            fill_rgb: (0.0, 0.0, 0.0),
        },
        PositionedShapedRun {
            x: x_hebrew,
            y: 50.0,
            text: hebrew_text,
            shaped: &hebrew_shaped,
            font: hebrew_font,
            size_pt: 12.0,
            fill_rgb: (0.0, 0.0, 0.0),
        },
        PositionedShapedRun {
            x: x_arabic,
            y: 50.0,
            text: arabic_text,
            shaped: &arabic_shaped,
            font: arabic_font,
            size_pt: 12.0,
            fill_rgb: (0.0, 0.0, 0.0),
        },
        PositionedShapedRun {
            x: x_right,
            y: 50.0,
            text: right,
            shaped: &right_shaped,
            font: inter_font,
            size_pt: 12.0,
            fill_rgb: (0.0, 0.0, 0.0),
        },
    ];
    pdf.draw_shaped_text_line(page, &runs)
        .expect("draw mixed-direction logical runs");
    let bytes = pdf.serialize();
    assert_eq!(
        inspect::page_text(&bytes).unwrap(),
        [format!("{left}{hebrew_text}{arabic_text}{right}")]
    );
    assert_eq!(
        inspect::positioned_glyph_draws(&bytes).unwrap().len(),
        left_shaped.glyphs.len()
            + hebrew_shaped.glyphs.len()
            + arabic_shaped.glyphs.len()
            + right_shaped.glyphs.len()
    );
    assert_emitted_glyphs_match_source(
        &bytes,
        SourceGlyphRun {
            postscript_name: "Inter-Regular",
            source_bytes: inter.bytes(),
            source_index: inter.collection_index(),
            text: left,
            shaped: &left_shaped,
            origin_x: x0,
            origin_y: 50.0,
        },
    );
    assert_emitted_glyphs_match_source(
        &bytes,
        SourceGlyphRun {
            postscript_name: "Inter-Regular",
            source_bytes: inter.bytes(),
            source_index: inter.collection_index(),
            text: right,
            shaped: &right_shaped,
            origin_x: x_right,
            origin_y: 50.0,
        },
    );
    assert_emitted_glyphs_match_source(
        &bytes,
        SourceGlyphRun {
            postscript_name: "NotoSansHebrew-Regular",
            source_bytes: hebrew.bytes(),
            source_index: hebrew.collection_index(),
            text: hebrew_text,
            shaped: &hebrew_shaped,
            origin_x: x_hebrew,
            origin_y: 50.0,
        },
    );
    assert_emitted_glyphs_match_source(
        &bytes,
        SourceGlyphRun {
            postscript_name: "NotoNaskhArabic-Regular",
            source_bytes: arabic.bytes(),
            source_index: arabic.collection_index(),
            text: arabic_text,
            shaped: &arabic_shaped,
            origin_x: x_arabic,
            origin_y: 50.0,
        },
    );
}

#[test]
fn invalid_shaped_input_is_rejected_before_page_or_subset_mutation() {
    let face = script_face("NotoNaskhArabic-Regular");
    let text = "سلام";
    let shaped = shape_rtl(&face, text, *b"Arab");
    let mut pdf = PdfDocument::new();
    let page = pdf.add_page(300.0, 100.0);
    let font = register_script_face(&mut pdf, &face);

    let mut invalid_gid = shaped.clone();
    invalid_gid.glyphs[0].glyph_id = u16::MAX;
    let mut invalid_cluster = shaped.clone();
    invalid_cluster.glyphs[0].cluster = 1;
    let mut invalid_geometry = shaped.clone();
    invalid_geometry.glyphs[0].advance = 2_000_000_000.0;
    let invalid = [invalid_gid, invalid_cluster, invalid_geometry];
    for broken in &invalid {
        let valid_run = PositionedShapedRun {
            x: 10.0,
            y: 50.0,
            text,
            shaped: &shaped,
            font,
            size_pt: 12.0,
            fill_rgb: (0.0, 0.0, 0.0),
        };
        let broken_run = PositionedShapedRun {
            x: 100.0,
            y: 50.0,
            text,
            shaped: broken,
            font,
            size_pt: 12.0,
            fill_rgb: (0.0, 0.0, 0.0),
        };
        let result = pdf.draw_shaped_text_line(page, &[valid_run, broken_run]);
        assert!(result.is_err());
        assert!(pdf.faces.is_empty(), "no remapper slot was created");
        assert!(
            pdf.pages[page.0].ops.is_empty(),
            "no page operation was added"
        );
    }
}

#[test]
fn malformed_font_bytes_and_face_indices_are_refused() {
    assert!(PdfFontProgram::new(Arc::<[u8]>::from(&b"not a font"[..]), 0).is_err());
    let face = script_face("NotoNaskhArabic-Regular");
    assert!(
        PdfFontProgram::new(Arc::<[u8]>::from(face.bytes()), u32::MAX).is_err(),
        "out-of-range collection index is refused"
    );
}

fn draw_registered_inter(
    pdf: &mut PdfDocument,
    face: &loom_fonts::LoadedFace,
    text: &str,
    shaped: &Shaped,
) -> Vec<u8> {
    let page = pdf.add_page(100.0, 100.0);
    let font = pdf
        .register_font_face(face.bytes(), face.collection_index())
        .expect("register Inter source bytes");
    pdf.draw_shaped_text_line(
        page,
        &[PositionedShapedRun {
            x: 10.0,
            y: 50.0,
            text,
            shaped,
            font,
            size_pt: 12.0,
            fill_rgb: (0.0, 0.0, 0.0),
        }],
    )
    .expect("draw shaped Inter text");
    pdf.serialize()
}

#[test]
fn registering_inter_face_bytes_returns_a_drawable_program_handle() {
    let face = inter_face();
    let mut pdf = PdfDocument::new();
    let text = "A";
    let shaped = face.shape(text, 12.0, &ShapeSettings::default());
    let bytes = draw_registered_inter(&mut pdf, &face, text, &shaped);

    assert_eq!(inspect::page_text(&bytes).expect("extract text"), ["A"]);
    assert_emitted_glyphs_match_source(
        &bytes,
        SourceGlyphRun {
            postscript_name: "Inter-Regular",
            source_bytes: face.bytes(),
            source_index: face.collection_index(),
            text,
            shaped: &shaped,
            origin_x: 10.0,
            origin_y: 50.0,
        },
    );
}

#[test]
fn inter_face_handle_does_not_alias_an_earlier_custom_program() {
    let custom = script_face("NotoNaskhArabic-Regular");
    let face = inter_face();
    let mut pdf = PdfDocument::new();
    register_script_face(&mut pdf, &custom);
    let text = "A";
    let shaped = face.shape(text, 12.0, &ShapeSettings::default());
    let bytes = draw_registered_inter(&mut pdf, &face, text, &shaped);

    assert_eq!(inspect::page_text(&bytes).expect("extract text"), ["A"]);
    assert_emitted_glyphs_match_source(
        &bytes,
        SourceGlyphRun {
            postscript_name: "Inter-Regular",
            source_bytes: face.bytes(),
            source_index: face.collection_index(),
            text,
            shaped: &shaped,
            origin_x: 10.0,
            origin_y: 50.0,
        },
    );
}

#[test]
fn font_program_size_limit_is_shared_and_inclusive() {
    let limit = usize::try_from(loom_fonts::MAX_FONT_FILE_BYTES)
        .expect("font size limit fits the target address space");
    assert!(crate::fonts::validate_font_file_size(limit).is_ok());
    let error = crate::fonts::validate_font_file_size(limit + 1)
        .expect_err("one byte over the shared limit is refused");
    assert!(error.contains(&loom_fonts::MAX_FONT_FILE_BYTES.to_string()));
}
