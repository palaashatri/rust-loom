use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use loom_fonts::{FallbackPolicy, FontCatalog, Paragraph, ScanConfig, StyledSpan, TextDirection};
use loom_pdf::inspect;
use loom_text::{Alignment, CharacterStyle, ParagraphDirection, StyleRun};

use crate::export::test_support::export_valid_pdf as export_pdf;
use crate::{
    fonts, test_fonts, RichBlock, WriterDocument, WriterPaintedRun, WriterShapedVisualLine,
};

struct ResetCatalog;

impl Drop for ResetCatalog {
    fn drop(&mut self) {
        fonts::reset_font_catalog();
    }
}

fn script_catalog() -> Arc<FontCatalog> {
    let mut config = ScanConfig::with_dirs(Vec::new());
    config.policy = FallbackPolicy::new(["Noto Naskh Arabic", "Noto Sans Hebrew"]);
    Arc::new(FontCatalog::scan(&config))
}

#[derive(Debug)]
struct ExpectedGlyph {
    postscript_name: String,
    source_glyph: u16,
    cluster_text: String,
    x: f32,
    y_offset: f32,
    width_1000_em: f32,
}

fn expected_glyphs(
    catalog: &FontCatalog,
    text: &str,
    size: f32,
    left_origin: f32,
) -> (Vec<ExpectedGlyph>, f32) {
    let regular = catalog
        .resolve("Inter", 400, false)
        .expect("bundled document face");
    let paragraph = Paragraph::new(
        text,
        &[StyledSpan::new(0..text.len(), &regular, size)],
        TextDirection::RightToLeft,
    );
    let layout = catalog.layout_paragraph_line(&paragraph, 0..text.len());
    let mut expected = Vec::new();

    for run in &layout.runs {
        let info = catalog
            .face(run.face)
            .expect("layout face belongs to catalog");
        let face = catalog.load(run.face).expect("load source face");
        let source = &text[run.range.clone()];
        let cluster_starts: Vec<_> = run
            .shaped
            .glyphs
            .iter()
            .map(|glyph| glyph.cluster)
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        let mut pen = 0.0;
        for glyph in &run.shaped.glyphs {
            let next = cluster_starts
                .iter()
                .copied()
                .find(|start| *start > glyph.cluster)
                .unwrap_or(source.len());
            let cluster_text = source[glyph.cluster..next].to_owned();
            expected.push(ExpectedGlyph {
                postscript_name: info.postscript_name.clone(),
                source_glyph: glyph.glyph_id,
                cluster_text,
                x: left_origin + run.x + pen + glyph.x_offset,
                y_offset: glyph.y_offset,
                width_1000_em: face.glyph_advance(glyph.glyph_id, run.size) / run.size * 1000.0,
            });
            pen += glyph.advance;
        }
    }

    (expected, layout.ascent)
}

#[test]
fn public_pdf_export_uses_source_glyphs_for_mixed_rtl_scripts() {
    let _serial = test_fonts::serial();
    let catalog = script_catalog();
    fonts::install_font_catalog(catalog.clone());
    let _reset = ResetCatalog;

    let text = "A اللّٰه שָׁלוֹם Z";
    let mut document = WriterDocument::new("export-shaped-scripts", "Shaped scripts");
    let mut block = RichBlock::new(document.next_id(), "paragraph", text);
    block.style.direction = ParagraphDirection::RightToLeft;
    block.style.alignment = Alignment::Left;
    document.push(block);

    let page_style = document.page.page_style();
    let (expected, ascent) = expected_glyphs(
        &catalog,
        text,
        page_style.body_font_size_pt,
        page_style.margin_left_pt,
    );
    let expected_arabic: Vec<_> = expected
        .iter()
        .filter(|glyph| glyph.postscript_name == "NotoNaskhArabic-Regular")
        .collect();
    let expected_hebrew: Vec<_> = expected
        .iter()
        .filter(|glyph| glyph.postscript_name == "NotoSansHebrew-Regular")
        .collect();
    assert!(
        !expected_arabic.is_empty(),
        "fixture shapes Arabic with Noto"
    );
    assert!(
        !expected_hebrew.is_empty(),
        "fixture shapes Hebrew with Noto"
    );

    let pdf = export_pdf(&document);
    let embedded = inspect::embedded_fonts(&pdf).expect("inspect embedded PDF fonts");
    let embedded_names: Vec<_> = embedded
        .iter()
        .map(|font| format!("{} ({})", font.resource, font.base_font))
        .collect();

    let arabic = embedded
        .iter()
        .find(|font| font.base_font.ends_with("NotoNaskhArabic-Regular"))
        .unwrap_or_else(|| panic!("PDF embeds the shaped Arabic source face: {embedded_names:?}"));
    let hebrew = embedded
        .iter()
        .find(|font| font.base_font.ends_with("NotoSansHebrew-Regular"))
        .unwrap_or_else(|| panic!("PDF embeds the shaped Hebrew source face: {embedded_names:?}"));

    assert_eq!(inspect::page_text(&pdf).expect("extract PDF text"), [text]);
    inspect::verify(&pdf).expect("PDF streams, maps and embedded fonts are consistent");

    let draws = inspect::positioned_glyph_draws(&pdf).expect("read positioned glyphs");
    assert_eq!(
        draws.len(),
        expected.len(),
        "every shaped glyph is drawn once"
    );

    let baseline = page_style.height_pt - (page_style.margin_top_pt + ascent);

    for font in [arabic, hebrew] {
        let mut source = expected
            .iter()
            .filter(|glyph| font.base_font.ends_with(&glyph.postscript_name))
            .collect::<Vec<_>>();
        source.sort_by(|left, right| {
            left.x
                .total_cmp(&right.x)
                .then(left.y_offset.total_cmp(&right.y_offset))
        });
        let mut drawn = draws
            .iter()
            .filter(|draw| draw.resource == font.resource)
            .collect::<Vec<_>>();
        drawn.sort_by(|left, right| left.x.total_cmp(&right.x).then(left.y.total_cmp(&right.y)));
        assert_eq!(drawn.len(), source.len(), "all source glyphs use {font:?}");

        let mut clusters_by_gid: HashMap<u16, HashSet<String>> = HashMap::new();
        for expected in &source {
            clusters_by_gid
                .entry(expected.source_glyph)
                .or_default()
                .insert(expected.cluster_text.clone());
        }
        for (actual, expected) in drawn.iter().zip(&source) {
            assert!(
                (actual.x - expected.x).abs() < 0.01,
                "glyph x: {actual:?} vs {expected:?}"
            );
            assert!(
                (actual.y - expected.y_offset - baseline).abs() < 0.01,
                "glyph baseline and mark offset: {actual:?} vs {expected:?}"
            );

            let width = font
                .widths
                .get(usize::from(actual.cid))
                .expect("PDF subset width exists for every drawn CID");
            assert!(
                (width - expected.width_1000_em).abs() < 0.1,
                "subset CID {} preserves source GID {} advance: {width} vs {}",
                actual.cid,
                expected.source_glyph,
                expected.width_1000_em
            );
            let mapped = font
                .to_unicode
                .get(&actual.cid)
                .expect("drawn CID has a ToUnicode mapping");
            assert!(
                clusters_by_gid
                    .get(&expected.source_glyph)
                    .is_some_and(|clusters| clusters.contains(mapped)),
                "CID {} keeps its source logical cluster: {mapped:?}",
                actual.cid,
            );
        }
    }
}

fn painted_run<'a>(
    shaped: &'a WriterShapedVisualLine,
    text: &str,
    sample: &str,
) -> &'a WriterPaintedRun {
    let start = text.find(sample).expect("the text contains the styled run");
    shaped
        .painted_runs()
        .iter()
        .find(|run| run.source_range.start == start)
        .expect("the shaped line retains the inline style boundary")
}

fn pdf_glyph_positions_for_run(
    run: &WriterPaintedRun,
    text_origin_x: f32,
    baseline: f32,
) -> Vec<(f32, f32)> {
    let mut pen = 0.0;
    run.shaped
        .glyphs
        .iter()
        .map(|glyph| {
            let position = (
                text_origin_x + run.x_pt + pen + glyph.x_offset,
                baseline + run.baseline_shift_pt + glyph.y_offset,
            );
            pen += glyph.advance;
            position
        })
        .collect()
}

fn assert_pdf_run_positions(
    run: &WriterPaintedRun,
    font_suffix: &str,
    embedded: &[inspect::EmbeddedFont],
    draws: &[inspect::GlyphDraw],
    text_origin_x: f32,
    baseline: f32,
) {
    let font = embedded
        .iter()
        .find(|font| font.base_font.ends_with(font_suffix))
        .unwrap_or_else(|| panic!("PDF embeds {font_suffix}: {embedded:?}"));
    let mut expected = pdf_glyph_positions_for_run(run, text_origin_x, baseline);
    let mut actual: Vec<_> = draws
        .iter()
        .filter(|draw| draw.resource == font.resource)
        .map(|draw| (draw.x, draw.y))
        .collect();
    let sort_positions = |positions: &mut [(f32, f32)]| {
        positions
            .sort_by(|left, right| left.0.total_cmp(&right.0).then(left.1.total_cmp(&right.1)));
    };
    sort_positions(&mut expected);
    sort_positions(&mut actual);
    assert_eq!(
        actual.len(),
        expected.len(),
        "glyph count for {font_suffix}"
    );
    for (actual, expected) in actual.iter().zip(expected.iter()) {
        assert!(
            (actual.0 - expected.0).abs() < 0.01,
            "{actual:?} vs {expected:?}"
        );
        assert!(
            (actual.1 - expected.1).abs() < 0.01,
            "{actual:?} vs {expected:?}"
        );
    }
}

fn mixed_inline_style_document() -> WriterDocument {
    let text = "Latin שלום مرحبا";
    let mut document = WriterDocument::new("export-shaped-styles", "Shaped styled scripts");
    let mut block = RichBlock::new(document.next_id(), "paragraph", text);
    block.style.direction = ParagraphDirection::RightToLeft;
    block.style.alignment = Alignment::Left;
    for (sample, style) in [
        (
            "Latin",
            CharacterStyle {
                font_size: 19.0,
                explicit_size: true,
                weight: loom_text::FontWeight::Bold,
                italic: true,
                color: Some("#167A35FF".into()),
                ..CharacterStyle::default()
            },
        ),
        (
            "שלום",
            CharacterStyle {
                font_size: 17.0,
                explicit_size: true,
                superscript: true,
                color: Some("#145AC8FF".into()),
                ..CharacterStyle::default()
            },
        ),
        (
            "مرحبا",
            CharacterStyle {
                font_size: 18.0,
                explicit_size: true,
                subscript: true,
                color: Some("#C33218FF".into()),
                ..CharacterStyle::default()
            },
        ),
    ] {
        let start = text.find(sample).expect("the fixture contains each script");
        block.runs.push(StyleRun {
            start,
            end: start + sample.len(),
            style,
        });
    }
    document.push(block);
    document
}

fn assert_mixed_script_style_geometry(shaped: &WriterShapedVisualLine, text: &str) {
    let latin = painted_run(shaped, text, "Latin");
    let hebrew = painted_run(shaped, text, "שלום");
    let arabic = painted_run(shaped, text, "مرحبا");
    assert_eq!(latin.rgba, [22, 122, 53, 255]);
    assert_eq!(hebrew.rgba, [20, 90, 200, 255]);
    assert_eq!(arabic.rgba, [195, 50, 24, 255]);
    assert!((hebrew.size_pt - 17.0 * 0.7).abs() < 0.01);
    assert!((arabic.size_pt - 18.0 * 0.7).abs() < 0.01);
    assert!((hebrew.baseline_shift_pt - 17.0 * 0.35).abs() < 0.01);
    assert!((arabic.baseline_shift_pt + 18.0 * 0.35).abs() < 0.01);
}

fn assert_pdf_matches_mixed_shaped_line(
    document: &WriterDocument,
    shaped: &WriterShapedVisualLine,
    page_style: &crate::PageStyle,
    text: &str,
) {
    let latin = painted_run(shaped, text, "Latin");
    let hebrew = painted_run(shaped, text, "שלום");
    let arabic = painted_run(shaped, text, "مرحبا");
    let pdf = export_pdf(document);
    assert_eq!(inspect::page_text(&pdf).expect("extract PDF text"), [text]);
    inspect::verify(&pdf).expect("PDF glyph maps and embedded fonts are consistent");
    let embedded = inspect::embedded_fonts(&pdf).expect("inspect embedded PDF fonts");
    let draws = inspect::positioned_glyph_draws(&pdf).expect("inspect positioned glyphs");
    let total_shaped_glyphs: usize = shaped
        .painted_runs()
        .iter()
        .map(|run| run.shaped.glyphs.len())
        .sum();
    assert_eq!(draws.len(), total_shaped_glyphs);
    let text_origin_x = page_style.margin_left_pt + document.blocks[0].text_start_pt(page_style);
    let line_top_pdf = page_style.height_pt - page_style.margin_top_pt;
    let baseline = line_top_pdf - shaped.ascent_pt();
    assert_pdf_run_positions(
        latin,
        "Inter-BoldItalic",
        &embedded,
        &draws,
        text_origin_x,
        baseline,
    );
    assert_pdf_run_positions(
        hebrew,
        "NotoSansHebrew-Regular",
        &embedded,
        &draws,
        text_origin_x,
        baseline,
    );
    assert_pdf_run_positions(
        arabic,
        "NotoNaskhArabic-Regular",
        &embedded,
        &draws,
        text_origin_x,
        baseline,
    );

    let operators = inspect::readable_content(&pdf).expect("read PDF paint operators");
    for color in [
        "0.086 0.478 0.208 rg",
        "0.078 0.353 0.784 rg",
        "0.765 0.196 0.094 rg",
    ] {
        assert!(operators.contains(color), "PDF preserves run paint {color}");
    }
    for size in ["19.00 Tf", "11.90 Tf", "12.60 Tf"] {
        assert!(
            operators.contains(size),
            "PDF emits effective style size {size}"
        );
    }
}

#[test]
fn public_pdf_export_uses_shared_geometry_for_mixed_inline_script_styles() {
    let _serial = test_fonts::serial();
    let catalog = script_catalog();
    fonts::install_font_catalog(catalog);
    let _reset = ResetCatalog;

    let document = mixed_inline_style_document();
    let text = document.blocks[0].text.as_str();
    let page_style = document.page.page_style();
    let flow = document
        .flow(&page_style, crate::PageViewport::default())
        .expect("the mixed styled paragraph lays out");
    assert_eq!(
        flow.lines().len(),
        1,
        "the fixture stays on one visual line"
    );
    let shaped = flow.lines()[0]
        .shaped_line
        .as_deref()
        .expect("the editor uses shared shaped geometry");
    assert_mixed_script_style_geometry(shaped, text);
    assert_pdf_matches_mixed_shaped_line(&document, shaped, &page_style, text);
}

#[test]
fn public_pdf_export_preserves_inline_rgba_and_background_paint() {
    let _serial = test_fonts::serial();
    let catalog = script_catalog();
    fonts::install_font_catalog(catalog);
    let _reset = ResetCatalog;

    let text = "auto colored";
    let mut document = WriterDocument::new("export-painted-text", "Painted text");
    document.page.appearance = crate::PageAppearance::Black;
    let mut block = RichBlock::new(document.next_id(), "paragraph", text);
    block.runs.push(StyleRun {
        start: "auto ".len(),
        end: text.len(),
        style: CharacterStyle {
            color: Some("#11223380".into()),
            background: Some("#AABBCC40".into()),
            ..CharacterStyle::default()
        },
    });
    document.push(block);

    let pdf = export_pdf(&document);
    let operators = inspect::readable_content(&pdf).expect("read PDF drawing operators");
    let objects = String::from_utf8_lossy(&pdf);
    assert!(
        objects.contains("/ExtGState"),
        "semi-transparent paints have page graphics-state resources"
    );
    assert!(
        objects.contains("/ca 0.251") && objects.contains("/CA 0.502"),
        "PDF resources preserve independent background and text opacity"
    );
    assert!(
        operators.contains("/GS") && operators.contains(" gs "),
        "paint operators select scoped graphics states: {operators}"
    );
    assert!(
        operators.contains("0.667 0.733 0.800 rg") && operators.contains(" re f"),
        "the run's background RGB is filled behind its glyphs: {operators}"
    );
    assert!(
        operators.contains("0.000 0.000 0.000 rg 0.00 0.00 "),
        "the black page background is painted: {operators}"
    );
    assert!(
        operators.contains("1.000 1.000 1.000 rg"),
        "runs without explicit foreground RGB use white ink: {operators}"
    );
    assert!(
        operators.contains("0.067 0.133 0.200 rg"),
        "the run's foreground RGB is preserved: {operators}"
    );
    assert_eq!(inspect::page_text(&pdf).expect("extract text"), [text]);
}

#[test]
fn public_pdf_export_reproduces_fallback_synthetic_weight_and_slant() {
    let _serial = test_fonts::serial();
    let catalog = script_catalog();
    fonts::install_font_catalog(catalog.clone());
    let _reset = ResetCatalog;

    let text = "اللّٰه";
    let mut document = WriterDocument::new("export-synthetic-script", "Synthetic style");
    let mut block = RichBlock::new(document.next_id(), "paragraph", text);
    block.runs.push(StyleRun {
        start: 0,
        end: text.len(),
        style: CharacterStyle {
            weight: loom_text::FontWeight::Bold,
            italic: true,
            ..CharacterStyle::default()
        },
    });
    document.push(block);

    let size = document.page.page_style().body_font_size_pt;
    let bold_italic = catalog
        .resolve("Inter", 700, true)
        .expect("bundled bold italic request face");
    let paragraph = Paragraph::new(
        text,
        &[StyledSpan::new(0..text.len(), &bold_italic, size)],
        TextDirection::Auto,
    );
    let expected = catalog.layout_paragraph_line(&paragraph, 0..text.len());
    assert!(
        expected.runs.iter().any(|run| {
            catalog
                .face(run.face)
                .is_some_and(|face| face.postscript_name == "NotoNaskhArabic-Regular")
                && run.synthetic_bold
                && run.synthetic_italic
        }),
        "fixture requires both synthetic fallback effects"
    );

    let pdf = export_pdf(&document);
    let operators = inspect::readable_content(&pdf).expect("read PDF drawing operators");
    let expected_stroke_width = format!("{:.3} w", (size / 24.0_f32).max(0.25));
    assert_eq!(inspect::page_text(&pdf).expect("extract text"), [text]);
    assert!(
        operators.contains("2 Tr") && operators.contains(&expected_stroke_width),
        "synthetic bold strokes glyphs without changing advances: {operators}"
    );
    assert!(
        operators.contains("1 0 0.208 1 "),
        "synthetic italic applies the raster pipeline's outline shear: {operators}"
    );
}
