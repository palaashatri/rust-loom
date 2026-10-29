//! Gate 11 fixture producer for Writer: builds a representative document
//! through the real model and exporters and writes `writer.docx`,
//! `writer.pdf` and `writer.md` where Word and LibreOffice can open them.
//!
//! Run with:
//! `LOOM_INTEROP_OUT=<dir> cargo test -p loom-writer-core --test interop_fixtures -- --ignored`
//!
//! The scripts under `loom-writer/tests/interop/` and the LibreOffice
//! conversion check read these files; the expected text is fixed there.

use loom_text::{Alignment, CharacterStyle, FontWeight, StyleRun};
use loom_writer_core::{
    export_docx, export_pdf, CommentThread, PageMarginsPreset, PageOrientation, PageSetup,
    PaperSize, RichBlock, WriterDocument, TABLE_BLOCK_KIND,
};
use std::path::PathBuf;

fn styled(f: impl FnOnce(&mut CharacterStyle)) -> CharacterStyle {
    let mut style = CharacterStyle::default();
    f(&mut style);
    style
}

fn run(start: usize, end: usize, style: CharacterStyle) -> StyleRun {
    StyleRun { start, end, style }
}

fn fixture_document() -> WriterDocument {
    let mut doc = WriterDocument::new("interop", "Interop Report");
    doc.page = PageSetup {
        paper: PaperSize::Letter,
        orientation: PageOrientation::Landscape,
        margins: PageMarginsPreset::Moderate,
        ..PageSetup::default()
    };
    doc.blocks
        .push(RichBlock::new(1, "heading1", "Quarterly Overview"));
    // "Plain bold italic underline mix": bold 6..10, italic 11..17, underline 18..27.
    let mut styled_paragraph = RichBlock::new(2, "paragraph", "Plain bold italic underline mix");
    styled_paragraph.runs = vec![
        run(6, 10, styled(|s| s.weight = FontWeight::Bold)),
        run(11, 17, styled(|s| s.italic = true)),
        run(18, 27, styled(|s| s.underline = true)),
    ];
    doc.blocks.push(styled_paragraph);
    doc.blocks.push(RichBlock::new(3, "heading2", "Details"));
    doc.blocks
        .push(RichBlock::new(4, "list-bulleted", "Apples"));
    doc.blocks.push(RichBlock::new(5, "list-bulleted", "Pears"));
    doc.blocks
        .push(RichBlock::new(6, "list-numbered", "First step"));
    doc.blocks
        .push(RichBlock::new(7, "list-numbered", "Second step"));
    doc.blocks
        .push(RichBlock::new(8, "list-numbered", "Third step"));
    doc.blocks.push(loom_writer_core::markdown_table_block(
        9,
        "| Name | Qty |\n| --- | --- |\n| Tea | 2 |\n| Coffee | 3 |",
    ));
    doc.blocks.push(RichBlock::new(
        10,
        "paragraph",
        "Caf\u{e9} Zo\u{eb} \u{2013} \u{201c}quoted\u{201d} \u{2014} \u{20ac}5 \u{2026} D\u{fc}sseldorf",
    ));
    let mut centered = RichBlock::new(11, "paragraph", "Centered closing line");
    centered.style.alignment = Alignment::Center;
    doc.blocks.push(centered);
    doc.comments.push(CommentThread {
        id: "c1".into(),
        author: "Ada Lovelace".into(),
        block_id: 2,
        start: 6,
        end: 10,
        body: "Check this word".into(),
        resolved: false,
        orphaned: false,
    });
    doc
}

#[test]
#[ignore = "writes interop fixtures; set LOOM_INTEROP_OUT and run with --ignored"]
fn write_writer_interop_fixtures() {
    let dir = PathBuf::from(std::env::var("LOOM_INTEROP_OUT").expect("set LOOM_INTEROP_OUT"));
    std::fs::create_dir_all(&dir).unwrap();
    let doc = fixture_document();

    let docx = export_docx(&doc).expect("docx export");
    assert_eq!(
        docx.comments_exported, 1,
        "the anchored comment is exported"
    );
    std::fs::write(dir.join("writer.docx"), &docx.bytes).unwrap();
    std::fs::write(
        dir.join("writer.pdf"),
        export_pdf(&doc).expect("valid PDF fixture"),
    )
    .unwrap();
    std::fs::write(dir.join("writer.md"), doc.to_markdown().as_bytes()).unwrap();
}

fn chart_document() -> WriterDocument {
    use loom_objects::chart::{Chart, ChartSeries, ChartType};
    let mut doc = WriterDocument::new("doc-chart", "Chart interop");
    doc.push(RichBlock::new(1, "paragraph", "Before the chart"));
    let chart = Chart::from_data(
        "chart-1",
        Some("Quarterly Growth".to_string()),
        ChartType::Column,
        vec!["Q1".to_string(), "Q2".to_string(), "Q3".to_string()],
        vec![ChartSeries {
            name: "Revenue".to_string(),
            values: vec![50.0, 75.0, 120.0],
            color: None,
        }],
    );
    doc.insert_chart_block(100, chart).expect("insert chart");
    doc
}

/// A chart block leaves Writer as a real DOCX chart part, not as its JSON text.
#[test]
fn a_chart_block_exports_as_a_chart_part_and_never_as_json_text() {
    let export = export_docx(&chart_document()).expect("export");
    let archive = loom_package::zip::PackageArchive::from_bytes(&export.bytes).expect("zip");
    let names: Vec<&str> = archive.paths();
    assert!(
        names.iter().any(|n| n.starts_with("word/charts/chart")),
        "no chart part in {names:?}"
    );
    let body = String::from_utf8(archive.get("word/document.xml").expect("body").to_vec()).unwrap();
    assert!(
        body.contains("c:chart"),
        "the body does not reference the chart"
    );
    assert!(
        !body.contains("chartType"),
        "the chart's JSON leaked into the text"
    );
    if let Ok(dir) = std::env::var("LOOM_INTEROP_OUT") {
        std::fs::write(PathBuf::from(dir).join("writer-chart.docx"), &export.bytes).unwrap();
    }
}

#[test]
#[ignore = "reads writer-lo-authored.docx from LOOM_INTEROP_OUT and verifies Loom imports LibreOffice-authored DOCX"]
fn import_libreoffice_authored_docx() {
    let dir = PathBuf::from(std::env::var("LOOM_INTEROP_OUT").expect("set LOOM_INTEROP_OUT"));
    let path = dir.join("writer-lo-authored.docx");
    if !path.exists() {
        println!(
            "SKIP import_libreoffice_authored_docx: {} not present yet",
            path.display()
        );
        return;
    }
    let bytes = std::fs::read(&path).expect("read writer-lo-authored.docx");
    let doc =
        loom_writer_core::docx::import_docx(&bytes).expect("import LibreOffice-authored DOCX");
    let texts: Vec<&str> = doc.blocks.iter().map(|b| b.text.as_str()).collect();
    assert!(
        doc.blocks
            .iter()
            .any(|b| b.kind == "heading1" && b.text.as_str() == "Quarterly Overview"),
        "Heading 1 preserved: {:?}",
        doc.blocks
    );
    assert!(
        doc.blocks
            .iter()
            .any(|b| b.kind == "heading2" && b.text.as_str() == "Details"),
        "Heading 2 preserved: {:?}",
        doc.blocks
    );
    assert!(
        doc.blocks
            .iter()
            .any(|b| b.kind == "list-bulleted" && b.text.as_str() == "Apples"),
        "Bulleted list preserved: {:?}",
        doc.blocks
    );
    assert!(
        doc.blocks.iter().any(|b| b.kind == TABLE_BLOCK_KIND
            && b.text.as_str().contains("Tea")
            && b.text.as_str().contains("Coffee")),
        "Table preserved: {:?}",
        doc.blocks
    );
    assert!(
        texts.iter().any(|t| t.contains("Caf\u{e9} Zo\u{eb}")),
        "Accented text preserved: {texts:?}"
    );
    println!(
        "PASS import_libreoffice_authored_docx: blocks={}, kinds={:?}",
        doc.blocks.len(),
        doc.blocks.iter().map(|b| &b.kind).collect::<Vec<_>>()
    );
}
