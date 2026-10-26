use std::sync::Arc;

use loom_fonts::{FallbackPolicy, FontCatalog, Paragraph, ScanConfig, StyledSpan, TextDirection};
use loom_objects::table::{Table, TextRun};
use loom_pdf::inspect;
use loom_text::{CharacterStyle, StyleRun};

use crate::export::test_support::{drawn_text_runs, export_valid_pdf};
use crate::{fonts, test_fonts, RichBlock, WriterDocument};

struct ResetCatalog;

impl Drop for ResetCatalog {
    fn drop(&mut self) {
        fonts::reset_font_catalog();
    }
}

fn inter_catalog() -> Arc<FontCatalog> {
    let mut config = ScanConfig::with_dirs(Vec::new());
    config.policy = FallbackPolicy::new(["Inter"]);
    Arc::new(FontCatalog::scan(&config))
}

fn style_run(start: usize, end: usize, style: CharacterStyle) -> StyleRun {
    StyleRun { start, end, style }
}

#[test]
fn public_pdf_export_preserves_super_sub_size_and_baseline_geometry() {
    let _serial = test_fonts::serial();
    let catalog = inter_catalog();
    fonts::install_font_catalog(catalog);
    let _reset = ResetCatalog;

    let text = "A2B3";
    let size = 18.0;
    let base = CharacterStyle {
        font_family: "Inter".into(),
        font_size: size,
        explicit_size: true,
        ..CharacterStyle::default()
    };
    let mut superscript = base.clone();
    superscript.superscript = true;
    let mut subscript = base.clone();
    subscript.subscript = true;

    let mut document = WriterDocument::new("pdf-super-sub", "PDF super and subscript");
    let mut block = RichBlock::new(document.next_id(), "paragraph", text);
    block.runs = vec![
        style_run(0, 1, base.clone()),
        style_run(1, 2, superscript),
        style_run(2, 3, base),
        style_run(3, 4, subscript),
    ];
    document.push(block);

    let pdf = export_valid_pdf(&document);
    assert_eq!(inspect::page_text(&pdf).expect("read PDF text"), [text]);
    let runs = drawn_text_runs(&pdf).expect("read positioned PDF runs");
    let baseline = runs.iter().find(|run| run.text == "A").expect("base A");
    let superscript = runs.iter().find(|run| run.text == "2").expect("super 2");
    let second_base = runs.iter().find(|run| run.text == "B").expect("base B");
    let subscript = runs.iter().find(|run| run.text == "3").expect("sub 3");

    let expected_size = size * 0.7;
    let expected_shift = size * 0.35;
    assert!((baseline.size - size).abs() < 0.01, "{baseline:?}");
    assert!((second_base.size - size).abs() < 0.01, "{second_base:?}");
    assert!(
        (superscript.size - expected_size).abs() < 0.01,
        "superscript uses the effective font size: {superscript:?}"
    );
    assert!(
        (subscript.size - expected_size).abs() < 0.01,
        "subscript uses the effective font size: {subscript:?}"
    );
    assert!(
        (superscript.y - baseline.y - expected_shift).abs() < 0.02,
        "superscript baseline rises by 0.35 of requested size: {baseline:?}, {superscript:?}"
    );
    assert!(
        (subscript.y - baseline.y + expected_shift).abs() < 0.02,
        "subscript baseline falls by 0.35 of requested size: {baseline:?}, {subscript:?}"
    );
}

#[derive(Debug)]
struct Stroke {
    width: f32,
    from: (f32, f32),
    to: (f32, f32),
}

fn strokes(pdf: &[u8]) -> Vec<Stroke> {
    let content = inspect::readable_content(pdf).expect("read PDF operators");
    content
        .lines()
        .filter_map(|line| {
            let tokens: Vec<_> = line.split_whitespace().collect();
            if tokens.last() != Some(&"Q") {
                return None;
            }
            let rg = tokens.iter().position(|token| *token == "RG")?;
            let move_to = tokens.iter().position(|token| *token == "m")?;
            let line_to = tokens.iter().position(|token| *token == "l")?;
            if tokens.get(line_to + 1) != Some(&"S") {
                return None;
            }
            Some(Stroke {
                width: tokens.get(rg + 1)?.parse().ok()?,
                from: (
                    tokens.get(move_to.checked_sub(2)?)?.parse().ok()?,
                    tokens.get(move_to.checked_sub(1)?)?.parse().ok()?,
                ),
                to: (
                    tokens.get(line_to.checked_sub(2)?)?.parse().ok()?,
                    tokens.get(line_to.checked_sub(1)?)?.parse().ok()?,
                ),
            })
        })
        .collect()
}

#[test]
fn public_pdf_export_draws_strikethrough_at_the_font_metric() {
    let _serial = test_fonts::serial();
    let catalog = inter_catalog();
    fonts::install_font_catalog(catalog.clone());
    let _reset = ResetCatalog;

    let size = 18.0;
    let text = "struck";
    let face_ref = catalog.resolve("Inter", 400, false).expect("Inter regular");
    let face = catalog.load(face_ref.primary()).expect("load Inter face");
    let metric = face
        .line_metrics(size)
        .strikeout
        .expect("Inter provides a strikeout metric");
    let paragraph = Paragraph::new(
        text,
        &[StyledSpan::new(0..text.len(), &face_ref, size)],
        TextDirection::LeftToRight,
    );
    let expected_width = catalog
        .layout_paragraph_line(&paragraph, 0..text.len())
        .width;

    let mut document = WriterDocument::new("pdf-strikethrough", "PDF strikethrough");
    let mut block = RichBlock::new(document.next_id(), "paragraph", text);
    block.runs.push(style_run(
        0,
        text.len(),
        CharacterStyle {
            font_family: "Inter".into(),
            font_size: size,
            explicit_size: true,
            strikethrough: true,
            ..CharacterStyle::default()
        },
    ));
    document.push(block);

    let pdf = export_valid_pdf(&document);
    assert_eq!(inspect::page_text(&pdf).expect("read PDF text"), [text]);
    let text_run = drawn_text_runs(&pdf)
        .expect("read positioned PDF runs")
        .into_iter()
        .find(|run| run.text == text)
        .expect("struck text run");
    assert!((text_run.size - size).abs() < 0.01, "{text_run:?}");

    let strokes = strokes(&pdf);
    assert_eq!(
        strokes.len(),
        1,
        "one strike line spans the text: {strokes:?}"
    );
    let stroke = &strokes[0];
    let expected_thickness = metric.thickness.max(0.25);
    let expected_y = text_run.y + metric.offset - expected_thickness / 2.0;
    assert!(
        (stroke.width - expected_thickness).abs() < 0.01,
        "{stroke:?}"
    );
    assert!((stroke.from.0 - text_run.x).abs() < 0.01, "{stroke:?}");
    assert!(
        (stroke.to.0 - text_run.x - expected_width).abs() < 0.02,
        "{stroke:?}"
    );
    assert!((stroke.from.1 - expected_y).abs() < 0.02, "{stroke:?}");
    assert!((stroke.to.1 - expected_y).abs() < 0.02, "{stroke:?}");
}

#[test]
fn public_pdf_export_keeps_style_runs_in_one_line_and_resets_alpha() {
    let _serial = test_fonts::serial();
    let catalog = inter_catalog();
    fonts::install_font_catalog(catalog);
    let _reset = ResetCatalog;

    let text = "fade solid";
    let mut document = WriterDocument::new("pdf-alpha-reset", "PDF alpha reset");
    let mut block = RichBlock::new(document.next_id(), "paragraph", text);
    block.runs = vec![
        style_run(
            0,
            4,
            CharacterStyle {
                color: Some("#11223380".into()),
                ..CharacterStyle::default()
            },
        ),
        style_run(
            5,
            text.len(),
            CharacterStyle {
                color: Some("#445566".into()),
                ..CharacterStyle::default()
            },
        ),
    ];
    document.push(block);

    let pdf = export_valid_pdf(&document);
    let operators = inspect::readable_content(&pdf).expect("read PDF operators");
    let line = operators
        .split_once("BT")
        .and_then(|(_, after)| after.split_once("ET").map(|(line, _)| line))
        .expect("one text object contains the complete line");
    let translucent = line.find("/GS128 gs").expect("translucent run state");
    let opaque = line.find("/GS255 gs").expect("opaque reset state");
    assert!(
        translucent < opaque,
        "states follow source-run order: {line}"
    );
    assert_eq!(operators.matches("BT").count(), 1, "one logical text line");
    assert_eq!(operators.matches("ET").count(), 1, "one logical text line");
    assert_eq!(inspect::page_text(&pdf).expect("extract text"), [text]);
    let objects = String::from_utf8_lossy(&pdf);
    assert!(objects.contains("/ca 0.502 /CA 0.502"));
    assert!(objects.contains("/ca 1.000 /CA 1.000"));
}

fn table_run(
    text: &str,
    size: f32,
    superscript: bool,
    subscript: bool,
    strikethrough: bool,
) -> TextRun {
    TextRun {
        text: text.to_owned(),
        font_size_pt: Some(size),
        font_family: Some("Inter".to_owned()),
        superscript,
        subscript,
        strikethrough,
        ..TextRun::plain("")
    }
}

#[test]
fn public_pdf_export_preserves_super_sub_and_strike_in_typed_cells() {
    let _serial = test_fonts::serial();
    let catalog = inter_catalog();
    fonts::install_font_catalog(catalog.clone());
    let _reset = ResetCatalog;

    let size = 18.0;
    let mut table = Table::new(1, 1);
    table.header_row = false;
    table.cell_mut(0, 0).expect("cell").content.runs = vec![
        table_run("A", size, false, false, false),
        table_run("2", size, true, false, false),
        table_run("B", size, false, false, false),
        table_run("3", size, false, true, false),
        table_run("!", size, false, false, true),
    ];

    let mut document = WriterDocument::new("pdf-table-styles", "PDF table styles");
    let table_id = document.next_id();
    document.push(RichBlock::table_block(table_id, table));
    let pdf = export_valid_pdf(&document);
    assert_eq!(inspect::page_text(&pdf).expect("read PDF text"), ["A2B3!"]);

    let runs = drawn_text_runs(&pdf).expect("read positioned PDF runs");
    let baseline = runs.iter().find(|run| run.text == "A").expect("base A");
    let superscript = runs.iter().find(|run| run.text == "2").expect("super 2");
    let subscript = runs.iter().find(|run| run.text == "3").expect("sub 3");
    let struck = runs.iter().find(|run| run.text == "!").expect("strike run");
    let expected_size = size * 0.7;
    let expected_shift = size * 0.35;
    assert!(
        (superscript.size - expected_size).abs() < 0.01,
        "{superscript:?}"
    );
    assert!(
        (subscript.size - expected_size).abs() < 0.01,
        "{subscript:?}"
    );
    assert!((superscript.y - baseline.y - expected_shift).abs() < 0.02);
    assert!((subscript.y - baseline.y + expected_shift).abs() < 0.02);

    let face_ref = catalog.resolve("Inter", 400, false).expect("Inter regular");
    let face = catalog.load(face_ref.primary()).expect("load Inter face");
    let metric = face
        .line_metrics(size)
        .strikeout
        .expect("Inter provides a strikeout metric");
    let paragraph = Paragraph::new(
        "!",
        &[StyledSpan::new(0..1, &face_ref, size)],
        TextDirection::LeftToRight,
    );
    let expected_width = catalog.layout_paragraph_line(&paragraph, 0..1).width;
    let expected_thickness = metric.thickness.max(0.25);
    let expected_y = struck.y + metric.offset - expected_thickness / 2.0;
    let matching_strike = strokes(&pdf)
        .into_iter()
        .find(|stroke| {
            (stroke.from.0 - struck.x).abs() < 0.01
                && (stroke.to.0 - struck.x - expected_width).abs() < 0.02
                && (stroke.from.1 - expected_y).abs() < 0.02
                && (stroke.to.1 - expected_y).abs() < 0.02
                && (stroke.width - expected_thickness).abs() < 0.01
        })
        .expect("PDF strikes exactly the typed cell run's measured glyph span");
    assert!(matching_strike.width > 0.0);
}
