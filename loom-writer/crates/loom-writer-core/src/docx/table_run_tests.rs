use loom_objects::table::{Table, TextRun};
use loom_package::zip::PackageArchive;
use roxmltree::Document;

use super::{export_docx, import_docx};
use crate::{RichBlock, WriterDocument};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

fn document_xml(export: &[u8]) -> String {
    let archive = PackageArchive::from_bytes(export).expect("read DOCX package");
    String::from_utf8(
        archive
            .get("word/document.xml")
            .expect("document part exists")
            .to_vec(),
    )
    .expect("document XML is UTF-8")
}

fn run_has_property(
    document_xml: &str,
    text: &str,
    property_name: &str,
    attribute: Option<(&str, &str)>,
) -> bool {
    let document = Document::parse(document_xml).expect("document XML parses");
    let Some(properties) = document.descendants().find_map(|run| {
        if !run.is_element()
            || run.tag_name().name() != "r"
            || run.tag_name().namespace() != Some(W)
        {
            return None;
        }
        let run_text: String = run
            .descendants()
            .filter(|node| {
                node.is_element()
                    && node.tag_name().name() == "t"
                    && node.tag_name().namespace() == Some(W)
            })
            .filter_map(|node| node.text())
            .collect();
        if run_text != text {
            return None;
        }
        run.children().find(|node| {
            node.is_element()
                && node.tag_name().name() == "rPr"
                && node.tag_name().namespace() == Some(W)
        })
    }) else {
        return false;
    };
    let Some(property) = properties.descendants().find(|node| {
        node.is_element()
            && node.tag_name().name() == property_name
            && node.tag_name().namespace() == Some(W)
    }) else {
        return false;
    };
    match attribute {
        Some((name, value)) => property.attribute((W, name)) == Some(value),
        None => true,
    }
}

fn import_document_xml(xml: &str) -> WriterDocument {
    let mut archive = PackageArchive::new();
    archive
        .add("word/document.xml", xml.as_bytes().to_vec())
        .expect("add document part");
    import_docx(&archive.to_bytes().expect("write DOCX fixture")).expect("import DOCX")
}

fn run_start(runs: &[TextRun], text: &str) -> usize {
    let mut offset = 0;
    for run in runs {
        if run.text == text {
            return offset;
        }
        offset += run.text.len();
    }
    panic!("missing typed run {text:?}");
}

#[test]
fn typed_table_runs_export_strike_scripts_font_color_and_size() {
    let mut strike = TextRun::plain("strike");
    strike.bold = true;
    strike.italic = true;
    strike.underline = true;
    strike.strikethrough = true;
    strike.font_family = Some("Georgia".to_owned());
    strike.font_size_pt = Some(16.0);
    strike.color = Some("#123456FF".to_owned());
    let mut super_run = TextRun::plain("super");
    super_run.superscript = true;
    let mut sub_run = TextRun::plain("sub");
    sub_run.subscript = true;
    let mut table = Table::new(1, 1);
    table.header_row = false;
    table.set_cell_rich_text(0, 0, vec![strike, super_run, sub_run]);
    let mut source = WriterDocument::new("typed-table-runs", "Typed table runs");
    source.blocks.push(RichBlock::table_block(1, table));

    let export = export_docx(&source).expect("export typed table styles");
    let xml = document_xml(&export.bytes);
    assert!(run_has_property(&xml, "strike", "strike", None));
    assert!(run_has_property(&xml, "strike", "b", None));
    assert!(run_has_property(&xml, "strike", "i", None));
    assert!(run_has_property(
        &xml,
        "strike",
        "u",
        Some(("val", "single"))
    ));
    assert!(run_has_property(
        &xml,
        "strike",
        "rFonts",
        Some(("ascii", "Georgia"))
    ));
    assert!(run_has_property(&xml, "strike", "sz", Some(("val", "32"))));
    assert!(run_has_property(
        &xml,
        "strike",
        "color",
        Some(("val", "123456"))
    ));
    assert!(run_has_property(
        &xml,
        "super",
        "vertAlign",
        Some(("val", "superscript"))
    ));
    assert!(run_has_property(
        &xml,
        "sub",
        "vertAlign",
        Some(("val", "subscript"))
    ));
}

#[test]
fn typed_table_runs_export_default_foreground_as_automatic_color() {
    let mut table = Table::new(1, 1);
    table.header_row = false;
    table.set_cell_rich_text(0, 0, vec![TextRun::plain("plain")]);
    let mut source = WriterDocument::new("automatic-cell-color", "Automatic cell color");
    source.blocks.push(RichBlock::table_block(1, table));

    let export = export_docx(&source).expect("export plain table cell");
    let xml = document_xml(&export.bytes);
    assert!(run_has_property(
        &xml,
        "plain",
        "color",
        Some(("val", "auto"))
    ));
}

#[test]
fn typed_table_run_colors_are_validated_before_export_without_mutating_source() {
    for color in ["not-a-color", "#12345680"] {
        let mut run = TextRun::plain("colored");
        run.color = Some(color.to_owned());
        let mut table = Table::new(1, 1);
        table.header_row = false;
        table.set_cell_rich_text(0, 0, vec![run]);
        let mut source = WriterDocument::new("invalid-cell-color", "Invalid cell color");
        source.blocks.push(RichBlock::table_block(1, table));

        let error = match export_docx(&source) {
            Err(error) => error,
            Ok(_) => panic!("invalid and translucent cell colors were accepted"),
        };
        let diagnostic = error.to_string();
        assert!(diagnostic.contains("invalid inline color"), "{diagnostic}");
        assert!(
            diagnostic.len() < 160,
            "diagnostic is bounded: {diagnostic}"
        );
        let table = source
            .loom_table_from_block(1)
            .expect("source table remains");
        assert_eq!(
            table.cell(0, 0).unwrap().content.runs[0].color.as_deref(),
            Some(color),
            "export refusal does not change the source color"
        );
    }
}

#[test]
fn typed_table_import_keeps_run_styles_offsets_breaks_and_header_cells() {
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
        <w:document xmlns:w="{W}"><w:body><w:tbl>
          <w:tr><w:trPr><w:tblHeader/></w:trPr>
            <w:tc><w:p>
              <w:r><w:t>A </w:t></w:r>
              <w:r><w:rPr><w:b/><w:i/><w:u w:val="single"/><w:strike/>
                <w:rFonts w:ascii="Georgia"/><w:sz w:val="32"/><w:color w:val="123456"/>
              </w:rPr><w:t>B</w:t></w:r>
            </w:p><w:p>
              <w:r><w:rPr><w:vertAlign w:val="superscript"/></w:rPr><w:t>C</w:t></w:r>
              <w:r><w:rPr><w:vertAlign w:val="subscript"/></w:rPr><w:br/><w:t>D</w:t></w:r>
            </w:p></w:tc>
            <w:tc><w:p><w:r><w:t>Header</w:t></w:r></w:p></w:tc>
          </w:tr>
        </w:tbl></w:body></w:document>"#
    );
    let imported = import_document_xml(&xml);
    let table = imported.loom_table_from_block(1).expect("typed table");
    assert!(table.header_row, "header row metadata is retained");
    let cell = table.cell(0, 0).expect("styled cell");
    assert_eq!(cell.content.text(), "A B C\u{b}D");
    let runs = &cell.content.runs;

    let strike = runs.iter().find(|run| run.text == "B").expect("strike run");
    assert_eq!(run_start(runs, "B"), 2, "offset follows the plain prefix");
    assert!(strike.bold && strike.italic && strike.underline && strike.strikethrough);
    assert_eq!(strike.font_family.as_deref(), Some("Georgia"));
    assert_eq!(strike.font_size_pt, Some(16.0));
    assert_eq!(strike.color.as_deref(), Some("#123456FF"));

    let superscript = runs.iter().find(|run| run.text == "C").expect("super run");
    assert_eq!(
        run_start(runs, "C"),
        4,
        "paragraph separator shifts the run"
    );
    assert!(superscript.superscript && !superscript.subscript);
    let subscript = runs
        .iter()
        .find(|run| run.text == "\u{b}D")
        .expect("soft-break subscript run");
    assert_eq!(run_start(runs, "\u{b}D"), 5, "soft break is in the run");
    assert!(subscript.subscript && !subscript.superscript);
    assert_eq!(
        table.cell(0, 1).expect("second header cell").content.text(),
        "Header",
        "the adjacent header cell remains intact"
    );
}

#[test]
fn typed_cell_styles_survive_the_public_docx_export_import_round_trip() {
    let mut strike = TextRun::plain("strike ");
    strike.bold = true;
    strike.italic = true;
    strike.underline = true;
    strike.strikethrough = true;
    strike.font_family = Some("Georgia".to_owned());
    strike.font_size_pt = Some(16.0);
    strike.color = Some("#123456FF".to_owned());
    let mut super_run = TextRun::plain("super ");
    super_run.superscript = true;
    let mut sub_run = TextRun::plain("sub");
    sub_run.subscript = true;
    let mut table = Table::new(1, 1);
    table.header_row = false;
    table.set_cell_rich_text(
        0,
        0,
        vec![TextRun::plain("plain "), strike, super_run, sub_run],
    );
    let mut source = WriterDocument::new("typed-cell-roundtrip", "Typed cell round trip");
    source.blocks.push(RichBlock::table_block(1, table));

    let export = export_docx(&source).expect("export typed cell styles");
    let imported = import_docx(&export.bytes).expect("import exported typed cell styles");
    let table = imported.loom_table_from_block(1).expect("typed table");
    let cell = table.cell(0, 0).expect("styled cell");
    assert_eq!(cell.content.text(), "plain strike super sub");

    let strike = cell
        .content
        .runs
        .iter()
        .find(|run| run.text == "strike ")
        .expect("strike run");
    assert!(strike.bold && strike.italic && strike.underline && strike.strikethrough);
    assert_eq!(strike.font_family.as_deref(), Some("Georgia"));
    assert_eq!(strike.font_size_pt, Some(16.0));
    assert_eq!(strike.color.as_deref(), Some("#123456FF"));
    assert!(cell
        .content
        .runs
        .iter()
        .any(|run| run.text == "super " && run.superscript && !run.subscript));
    assert!(cell
        .content
        .runs
        .iter()
        .any(|run| run.text == "sub" && run.subscript && !run.superscript));
}
