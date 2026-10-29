use loom_package::zip::PackageArchive;
use roxmltree::{Document, Node};

use super::{export_docx, import_docx, inspect_docx_unsupported, DocxExport};
use crate::{FootnoteEntry, RichBlock, WriterDocument};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

fn document_with_footnotes() -> WriterDocument {
    let mut document = WriterDocument::new("footnotes", "Footnote interop");
    document
        .blocks
        .push(RichBlock::new(7, "paragraph", "A🙂e\u{301} & Z"));
    document
        .blocks
        .push(RichBlock::new(8, "paragraph", "before after"));
    let mut table = loom_objects::Table::new(1, 2);
    table.set_cell_text(0, 0, "cell🙂");
    table.set_cell_text(0, 1, "end");
    document.blocks.push(RichBlock::table_block(9, table));
    document.footnotes = vec![
        FootnoteEntry {
            id: 51,
            block_id: 7,
            offset: 1,
            body: "Use & <angle>\nline two".into(),
        },
        FootnoteEntry {
            id: 3,
            block_id: 7,
            offset: 5,
            body: "第二 note".into(),
        },
        FootnoteEntry {
            id: 60,
            block_id: 7,
            offset: 10,
            body: "entity anchor".into(),
        },
        FootnoteEntry {
            id: u64::MAX,
            block_id: 8,
            offset: 7,
            body: "high internal id".into(),
        },
        FootnoteEntry {
            id: 7,
            block_id: 9,
            offset: 4,
            body: "cell note".into(),
        },
    ];
    document
}

fn rewrite_part(export: &DocxExport, part: &str, rewrite: impl FnOnce(&str) -> String) -> Vec<u8> {
    let archive = PackageArchive::from_bytes(&export.bytes).expect("read DOCX package");
    let mut output = PackageArchive::new();
    let mut rewrite = Some(rewrite);
    for path in archive.paths() {
        let bytes = archive.get(path).expect("package part");
        let content = if path == part {
            let original = std::str::from_utf8(bytes).expect("part is UTF-8");
            rewrite.take().expect("one matching part")(original).into_bytes()
        } else {
            bytes.to_vec()
        };
        output.add(path, content).expect("copy package part");
    }
    output.to_bytes().expect("write DOCX package")
}

fn paragraph_events<'a, 'input>(paragraph: Node<'a, 'input>) -> Vec<String> {
    paragraph
        .children()
        .filter(|node| node.has_tag_name((W, "r")))
        .map(|run| {
            if let Some(reference) = run
                .descendants()
                .find(|node| node.has_tag_name((W, "footnoteReference")))
            {
                return format!(
                    "ref:{}",
                    reference.attribute((W, "id")).expect("reference ID")
                );
            }
            run.descendants()
                .filter(|node| node.has_tag_name((W, "t")))
                .filter_map(|node| node.text())
                .collect::<String>()
        })
        .filter(|event| !event.is_empty())
        .collect()
}

#[test]
fn docx_footnotes_export_import_in_document_order_with_utf8_and_table_anchors() {
    let document = document_with_footnotes();
    let saved = crate::save_document(&document).expect("save source document");
    let reopened = crate::load_document(&saved).expect("reopen source document");
    assert_eq!(reopened.footnotes, document.footnotes);
    let exported = export_docx(&reopened).expect("export footnote document");
    let repeated = export_docx(&reopened).expect("repeat export");
    assert_eq!(exported.bytes, repeated.bytes, "OOXML ID mapping is stable");

    let archive = PackageArchive::from_bytes(&exported.bytes).expect("read DOCX package");
    let footnotes_xml =
        std::str::from_utf8(archive.get("word/footnotes.xml").expect("footnotes part"))
            .expect("footnotes XML is UTF-8");
    let relationships = std::str::from_utf8(
        archive
            .get("word/_rels/document.xml.rels")
            .expect("document relationships"),
    )
    .expect("relationships are UTF-8");
    let content_types =
        std::str::from_utf8(archive.get("[Content_Types].xml").expect("content types"))
            .expect("content types are UTF-8");
    assert!(relationships.contains(&format!("Type=\"{}\"", super::footnotes::FOOTNOTES_REL)));
    assert!(relationships.contains("Target=\"footnotes.xml\""));
    assert!(content_types.contains(&format!(
        "ContentType=\"{}\"",
        super::footnotes::FOOTNOTES_CONTENT_TYPE
    )));
    assert!(content_types.contains("PartName=\"/word/footnotes.xml\""));

    let note_ids: Vec<_> = Document::parse(footnotes_xml)
        .expect("well-formed footnotes XML")
        .root_element()
        .children()
        .filter(|node| node.has_tag_name((W, "footnote")))
        .filter(|node| node.attribute((W, "type")).is_none())
        .map(|node| node.attribute((W, "id")).expect("footnote ID").to_string())
        .collect();
    assert_eq!(
        note_ids,
        vec![
            "51".to_string(),
            "3".to_string(),
            "60".to_string(),
            "1".to_string(),
            "7".to_string()
        ]
    );

    let document_xml = std::str::from_utf8(archive.get("word/document.xml").unwrap()).unwrap();
    let parsed_document = Document::parse(document_xml).expect("well-formed document XML");
    let paragraphs: Vec<_> = parsed_document
        .descendants()
        .filter(|node| node.has_tag_name((W, "p")))
        .collect();
    assert_eq!(
        paragraph_events(paragraphs[0]),
        ["A", "ref:51", "🙂", "ref:3", "e\u{301} &", "ref:60", " Z"]
    );
    assert_eq!(
        paragraph_events(paragraphs[1]),
        ["before ", "ref:1", "after"]
    );
    assert_eq!(
        paragraphs
            .iter()
            .find(|paragraph| paragraph
                .descendants()
                .any(|node| node.has_tag_name((W, "footnoteReference"))
                    && node.attribute((W, "id")) == Some("7")))
            .map(|paragraph| paragraph_events(*paragraph)),
        Some(vec!["cell".into(), "ref:7".into(), "🙂".into()])
    );

    let imported = import_docx(&exported.bytes).expect("import exported notes");
    assert_eq!(imported.footnotes.len(), 5);
    let first_block = imported
        .blocks
        .iter()
        .find(|block| block.text.as_str() == "A🙂e\u{301} & Z")
        .expect("imported first paragraph");
    let second_block = imported
        .blocks
        .iter()
        .find(|block| block.text.as_str() == "before after")
        .expect("imported second paragraph");
    let table_block = imported
        .blocks
        .iter()
        .find(|block| block.typed_table().is_some())
        .expect("imported typed table");
    assert_eq!(first_block.text.as_str(), "A🙂e\u{301} & Z");
    assert_eq!(
        imported
            .footnotes
            .iter()
            .map(|note| (note.id, note.block_id, note.offset, note.body.as_str()))
            .collect::<Vec<_>>(),
        [
            (51, first_block.id, 1, "Use & <angle>\nline two"),
            (3, first_block.id, 5, "第二 note"),
            (60, first_block.id, 10, "entity anchor"),
            (1, second_block.id, 7, "high internal id"),
            (7, table_block.id, 4, "cell note"),
        ]
    );
    let table = table_block.typed_table().expect("table remains typed");
    assert_eq!(table.cell_text(0, 0).as_deref(), Some("cell🙂"));
    assert_eq!(table.cell_text(0, 1).as_deref(), Some("end"));
}

#[test]
fn docx_footnote_import_rejects_dangling_duplicate_and_malformed_references() {
    let exported = export_docx(&document_with_footnotes()).expect("export seed");
    let dangling = rewrite_part(&exported, "word/document.xml", |xml| {
        xml.replacen(
            "<w:footnoteReference w:id=\"51\"/>",
            "<w:footnoteReference w:id=\"999\"/>",
            1,
        )
    });
    assert!(import_docx(&dangling)
        .unwrap_err()
        .contains("has no matching footnote"));

    let duplicate = rewrite_part(&exported, "word/document.xml", |xml| {
        xml.replacen(
            "<w:footnoteReference w:id=\"51\"/>",
            "<w:footnoteReference w:id=\"51\"/><w:footnoteReference w:id=\"51\"/>",
            1,
        )
    });
    assert!(import_docx(&duplicate)
        .unwrap_err()
        .contains("referenced more than once"));

    let missing_id = rewrite_part(&exported, "word/document.xml", |xml| {
        xml.replacen(
            "<w:footnoteReference w:id=\"51\"/>",
            "<w:footnoteReference/>",
            1,
        )
    });
    assert!(import_docx(&missing_id)
        .unwrap_err()
        .contains("missing w:id"));
}

#[test]
fn docx_footnote_import_rejects_duplicate_definitions_and_oversized_bodies() {
    let exported = export_docx(&document_with_footnotes()).expect("export seed");
    let duplicate = rewrite_part(&exported, "word/footnotes.xml", |xml| {
        let definition =
            "<w:footnote w:id=\"51\"><w:p><w:r><w:t>duplicate</w:t></w:r></w:p></w:footnote>";
        xml.replacen("</w:footnotes>", &format!("{definition}</w:footnotes>"), 1)
    });
    assert!(import_docx(&duplicate)
        .unwrap_err()
        .contains("footnote ID 51 is duplicated"));

    let oversized_body = "x".repeat(crate::footnotes::MAX_FOOTNOTE_BODY_BYTES + 1);
    let oversized = rewrite_part(&exported, "word/footnotes.xml", |_| {
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><w:footnotes xmlns:w=\"{W}\">\
             <w:footnote w:id=\"51\"><w:p><w:r><w:t>{oversized_body}</w:t></w:r></w:p></w:footnote>\
             </w:footnotes>"
        )
    });
    assert!(import_docx(&oversized).unwrap_err().contains("body limit"));
}

#[test]
fn docx_import_reports_endnotes_but_not_supported_footnotes_as_lost() {
    let exported = export_docx(&document_with_footnotes()).expect("export seed");
    let fixture = rewrite_part(&exported, "word/document.xml", |xml| {
        xml.replace(
            "<w:footnoteReference w:id=\"51\"/>",
            "<w:footnoteReference w:id=\"51\"/><w:endnoteReference w:id=\"1\"/>",
        )
    });
    let warnings = inspect_docx_unsupported(&fixture).expect("inspect unsupported content");
    assert!(warnings.iter().any(|warning| warning.contains("Endnotes")));
    assert!(!warnings.iter().any(|warning| warning.contains("Footnote")));
}
