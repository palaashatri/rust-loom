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
    doc.blocks.push(RichBlock::new(
        9,
        TABLE_BLOCK_KIND,
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
    std::fs::write(dir.join("writer.pdf"), export_pdf(&doc)).unwrap();
    std::fs::write(dir.join("writer.md"), doc.to_markdown().as_bytes()).unwrap();
}
