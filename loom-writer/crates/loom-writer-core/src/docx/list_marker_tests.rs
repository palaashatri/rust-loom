use loom_package::zip::PackageArchive;
use loom_text::ListMarkerStyle;

use super::{export_docx, import_docx};
use crate::{RichBlock, WriterDocument};

#[test]
fn round_trip_preserves_all_list_marker_styles_levels_and_restarts() {
    let mut doc = WriterDocument::new("doc-1", "List markers");
    doc.blocks.clear();

    let items: Vec<(&str, ListMarkerStyle, u8, &str)> = vec![
        ("list-bulleted", ListMarkerStyle::Disc, 0, "Bullet disc"),
        (
            "list-bulleted",
            ListMarkerStyle::Circle,
            1,
            "Bullet circle nested",
        ),
        ("list-bulleted", ListMarkerStyle::Square, 0, "Bullet square"),
        (
            "list-bulleted",
            ListMarkerStyle::Dash,
            2,
            "Bullet dash nested",
        ),
        (
            "paragraph",
            ListMarkerStyle::Default,
            0,
            "Separator paragraph",
        ),
        (
            "list-numbered",
            ListMarkerStyle::Decimal,
            0,
            "Ordered decimal 1",
        ),
        (
            "list-numbered",
            ListMarkerStyle::Decimal,
            1,
            "Ordered decimal nested",
        ),
        (
            "list-numbered",
            ListMarkerStyle::LowerAlpha,
            0,
            "Ordered lower-alpha",
        ),
        (
            "list-numbered",
            ListMarkerStyle::UpperAlpha,
            0,
            "Ordered upper-alpha",
        ),
        (
            "list-numbered",
            ListMarkerStyle::LowerRoman,
            0,
            "Ordered lower-roman",
        ),
        (
            "list-numbered",
            ListMarkerStyle::UpperRoman,
            0,
            "Ordered upper-roman",
        ),
    ];

    for (idx, (kind, marker, level, text)) in items.iter().enumerate() {
        let mut block = RichBlock::new((idx + 1) as u64, kind, text);
        block.style.list_marker = *marker;
        block.style.list_level = *level;
        doc.blocks.push(block);
    }

    let exported = export_docx(&doc).expect("export_docx should succeed");
    let archive = PackageArchive::from_bytes(&exported.bytes).expect("valid docx zip");
    let numbering_xml = std::str::from_utf8(
        archive
            .get("word/numbering.xml")
            .expect("word/numbering.xml"),
    )
    .expect("utf8 numbering.xml");
    for fmt in [
        "bullet",
        "decimal",
        "lowerLetter",
        "upperLetter",
        "lowerRoman",
        "upperRoman",
    ] {
        assert!(
            numbering_xml.contains(&format!("w:numFmt w:val=\"{fmt}\"")),
            "missing w:numFmt={fmt} in word/numbering.xml"
        );
    }
    for glyph in ["\u{2022}", "\u{25e6}", "\u{25aa}", "\u{2013}"] {
        assert!(
            numbering_xml.contains(&format!("w:lvlText w:val=\"{glyph}\"")),
            "missing bullet glyph {glyph} in word/numbering.xml"
        );
    }
    // 5 distinct ordered runs (Decimal, LowerAlpha, UpperAlpha, LowerRoman, UpperRoman)
    assert_eq!(numbering_xml.matches("startOverride").count(), 5);

    let reimported = import_docx(&exported.bytes).expect("import_docx should succeed");
    assert_eq!(reimported.blocks.len(), items.len());
    for (idx, (expected_kind, expected_marker, expected_level, expected_text)) in
        items.iter().enumerate()
    {
        let block = &reimported.blocks[idx];
        assert_eq!(block.kind, *expected_kind, "block {idx} kind mismatch");
        assert_eq!(
            block.style.list_marker, *expected_marker,
            "block {idx} ({expected_text}) marker mismatch"
        );
        assert_eq!(
            block.style.list_level, *expected_level,
            "block {idx} level mismatch"
        );
        assert_eq!(
            block.text.as_str(),
            *expected_text,
            "block {idx} text mismatch"
        );
    }
}
