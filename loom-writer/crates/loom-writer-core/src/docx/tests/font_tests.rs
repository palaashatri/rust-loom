//! Font fidelity regressions for documents authored outside Writer.
//!
//! These cover explicit `w:rFonts` declarations, save/reopen wrapping and PDF
//! embedding, and the disclosed loss of complex-script-only run properties.

use loom_text::FontWeight;

use super::super::{export_docx, import_docx, inspect_docx_unsupported};
use super::{committed_fixture, parts};
use crate::{RichBlock, WriterDocument};

#[test]
fn externally_authored_arial_survives_import_save_and_pdf_export() {
    // The fixture's inherited document font is Georgia; only the first run
    // declares Arial. The fixture Arial has double advances, so wrapping in
    // Arial is visibly narrower than wrapping in Writer's inherited font.
    let _fonts = crate::test_fonts::install(&[("Arial", 200), ("Georgia", 100)]);
    let bytes = committed_fixture("writer_external_arial.docx");
    let imported = import_docx(&bytes).expect("import the external Arial fixture");

    let arial_block = imported
        .blocks
        .iter()
        .find(|block| block.text.as_str().starts_with("Arial stays Arial"))
        .expect("the Arial paragraph imports");
    assert_eq!(
        arial_block.runs.len(),
        1,
        "the Arial paragraph carries one explicit run: {:?}",
        arial_block.runs
    );
    assert_eq!(
        arial_block.runs[0].style.font_family, "Arial",
        "an explicit Arial declaration is authored text, not an inherited default"
    );
    let inherited_block = imported
        .blocks
        .iter()
        .find(|block| block.text.as_str().starts_with("This paragraph inherits"))
        .expect("the inherited paragraph imports");
    assert!(
        inherited_block.runs.is_empty(),
        "text with no explicit run font keeps the inherited default: {:?}",
        inherited_block.runs
    );

    // Saving and reopening keeps the explicit family.
    let saved = export_docx(&imported).expect("export the imported Arial fixture");
    let saved_xml = parts(&saved)["word/document.xml"].clone();
    assert!(
        saved_xml.contains("w:ascii=\"Arial\""),
        "the export states the explicit Arial family: {saved_xml}"
    );
    let reopened = import_docx(&saved.bytes).expect("reopen the exported Arial document");
    let reopened_block = reopened
        .blocks
        .iter()
        .find(|block| block.text.as_str().starts_with("Arial stays Arial"))
        .expect("the Arial paragraph reopens");
    assert_eq!(
        reopened_block.runs[0].style.font_family, "Arial",
        "save and reopen keeps explicit Arial"
    );
    assert_eq!(
        reopened.blocks, imported.blocks,
        "the save/reopen round trip keeps the imported run styles exactly"
    );

    // Editor wrapping uses Arial, and reopening reproduces it exactly.
    let style = crate::PageStyle::default();
    let viewport = || crate::PageViewport {
        width: style.width_pt,
        height: style.height_pt,
        ..crate::PageViewport::default()
    };
    let line_ranges = |doc: &WriterDocument, block_id: u64| {
        let block_index = doc
            .blocks
            .iter()
            .position(|block| block.id == block_id)
            .expect("the laid-out block exists");
        doc.flow(&style, viewport())
            .expect("layout the Arial document")
            .lines()
            .iter()
            .filter(|line| line.block_index == block_index)
            .map(|line| (line.start, line.end))
            .collect::<Vec<_>>()
    };
    let arial_lines = line_ranges(&imported, arial_block.id);
    assert!(
        arial_lines.len() > 1,
        "the fixture sentence wraps so the comparison is meaningful: {arial_lines:?}"
    );
    assert_eq!(
        line_ranges(&reopened, reopened_block.id),
        arial_lines,
        "save and reopen keeps Arial's line wrapping"
    );
    let mut control = WriterDocument::new("arial-control", "Arial control");
    control
        .blocks
        .push(RichBlock::new(1, "paragraph", arial_block.text.as_str()));
    assert_ne!(
        line_ranges(&control, control.blocks[0].id),
        arial_lines,
        "the Arial run wraps in Arial, not in Writer's inherited font"
    );

    // PDF export draws the run with the Arial face rather than the default.
    let pdf = crate::export_pdf(&reopened).expect("export the reopened Arial document");
    let faces = loom_pdf::inspect::embedded_fonts(&pdf)
        .expect("inspect the PDF fonts")
        .into_iter()
        .map(|font| font.base_font)
        .collect::<Vec<_>>();
    assert!(
        faces.iter().any(|face| face.ends_with("+Arial-Regular")),
        "the PDF embeds the authored Arial face: {faces:?}"
    );
}

#[test]
fn complex_script_only_formatting_is_disclosed_and_does_not_restyle_latin_text() {
    let cases: [(&str, &str, &[&str]); 2] = [
        (
            "writer_complex_script_arabic.docx",
            "\u{627}\u{644}\u{639}\u{631}\u{628}\u{64a}\u{629}",
            &["w:bCs", "w:szCs", "w:cs"],
        ),
        (
            "writer_complex_script_hebrew.docx",
            "\u{5e2}\u{5d1}\u{5e8}\u{5d9}\u{5ea}",
            &["w:iCs"],
        ),
    ];
    for (fixture, script_text, properties) in cases {
        let bytes = committed_fixture(fixture);
        let warnings = inspect_docx_unsupported(&bytes).expect("inspect the fixture");
        let joined = warnings.join(" | ").to_lowercase();
        assert!(
            joined.contains("complex-script"),
            "{fixture} discloses that complex-script formatting cannot be imported: {warnings:?}"
        );
        for property in properties {
            assert!(
                joined.contains(&property.to_lowercase()),
                "{fixture} names the dropped {property}: {warnings:?}"
            );
        }

        let imported = import_docx(&bytes).expect("import the complex-script fixture");
        let all_text = imported
            .blocks
            .iter()
            .map(|block| block.text.as_str())
            .collect::<String>();
        assert!(
            all_text.contains(script_text),
            "{fixture} imports the affected script text instead of dropping it"
        );
        let latin = imported
            .blocks
            .iter()
            .find(|block| block.text.as_str().starts_with("Latin neighbour"))
            .expect("the Latin neighbour imports");
        assert!(
            latin.runs.iter().all(
                |run| run.style.weight.numeric() < FontWeight::Bold.numeric() && !run.style.italic
            ),
            "{fixture} does not apply complex-script-only styling to its Latin run: {:?}",
            latin.runs
        );

        // The affected script text itself is imported plainly rather than
        // being given the Latin run's style or silently dropped.
        let script_blocks = imported
            .blocks
            .iter()
            .filter(|block| block.text.as_str().contains(script_text))
            .collect::<Vec<_>>();
        assert!(
            !script_blocks.is_empty(),
            "{fixture} keeps the script paragraphs"
        );
        for block in script_blocks {
            for run in &block.runs {
                assert!(
                    run.style.weight.numeric() < FontWeight::Bold.numeric(),
                    "{fixture} never invents complex-script bold: {:?}",
                    run.style
                );
            }
        }
    }

    // An explicit Latin italic with complex-script italic switched off keeps
    // the run's single italic style, which also covers the script text. The
    // import is not silent about that difference: the fixture's warning names
    // `w:iCs`, so the limitation is disclosed rather than accepted.
    let bytes = committed_fixture("writer_complex_script_hebrew.docx");
    let imported = import_docx(&bytes).expect("import the Hebrew fixture");
    let latin_italic = imported
        .blocks
        .iter()
        .find(|block| {
            block.text.as_str().contains('\u{5e2}') && block.runs.iter().any(|run| run.style.italic)
        })
        .expect("the w:i run imports its italic style");
    assert!(
        latin_italic
            .runs
            .iter()
            .any(|run| run.style.italic && run.style.font_family.is_empty()),
        "the run's w:i declaration still imports: {:?}",
        latin_italic.runs
    );
}
