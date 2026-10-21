//! Behaviour of pasted tables: bold, italic and column alignment must survive
//! storage in the table block, read-back into a `loom_objects::Table`, and the
//! DOCX and PDF exports must not show the storage markers as text.

use crate::{export_docx, export_pdf, RichBlock, WriterDocument, WriterTable, TABLE_BLOCK_KIND};
use loom_objects::table::{CellStyleOverride, HorizontalAlign, Table, TextRun};
use loom_package::zip::PackageArchive;

fn run(text: &str, bold: bool, italic: bool) -> TextRun {
    TextRun {
        text: text.into(),
        bold,
        italic,
        ..TextRun::plain("")
    }
}

fn styled_table() -> Table {
    let mut table = Table::new(2, 3);
    table.header_row = true;
    table.set_cell_rich_text(0, 0, vec![run("Item", true, false)]);
    table.set_cell_rich_text(0, 1, vec![run("Qty", true, false)]);
    table.set_cell_rich_text(1, 0, vec![run("Apples", false, false)]);
    table.set_cell_rich_text(1, 1, vec![run("50", false, false)]);
    table.set_cell_rich_text(1, 2, vec![run("Note", false, true)]);
    for row in 0..2 {
        table.cell_mut(row, 1).expect("cell").style_override = Some(CellStyleOverride {
            horizontal_align: Some(HorizontalAlign::Right),
            ..Default::default()
        });
    }
    table
}

fn paste(table: &Table) -> WriterDocument {
    let mut document = WriterDocument::new("doc", "Doc");
    document
        .blocks
        .push(RichBlock::new(1, "paragraph", "Before"));
    document
        .insert_table_object(usize::MAX, table)
        .expect("paste succeeds");
    document
}

#[test]
fn pasted_bold_and_italic_runs_survive_storage_and_read_back() {
    let document = paste(&styled_table());
    let read = document
        .loom_table_from_block(document.blocks.last().expect("table block").id)
        .expect("stored table parses");

    let header = &read.cell(0, 0).expect("header cell").content.runs;
    assert!(
        header.iter().any(|r| r.bold && r.text == "Item"),
        "{header:?}"
    );
    let note = &read.cell(1, 2).expect("note cell").content.runs;
    assert!(
        note.iter().any(|r| r.italic && r.text == "Note"),
        "{note:?}"
    );
    let plain = &read.cell(1, 0).expect("plain cell").content.runs;
    assert!(plain.iter().all(|r| !r.bold && !r.italic), "{plain:?}");
}

#[test]
fn pasted_column_alignment_is_written_into_the_stored_divider_row() {
    let document = paste(&styled_table());
    let block = document.blocks.last().expect("table block");
    let read = loom_objects::markdown_to_table(block.text.as_str());
    let aligned = read.cell(1, 1).expect("qty cell").style_override.clone();
    assert_eq!(
        aligned.and_then(|o| o.horizontal_align),
        Some(HorizontalAlign::Right),
        "stored text: {}",
        block.text.as_str()
    );
}

#[test]
fn formatting_markers_never_reach_the_cell_text_the_editor_shows() {
    let document = paste(&styled_table());
    let table = document
        .table_from_block(document.blocks.last().expect("table block").id)
        .expect("table parses");
    assert_eq!(table.rows[0][0], "Item");
    assert_eq!(table.rows[1][2], "Note");
    assert!(table.rows.iter().flatten().all(|cell| !cell.contains('*')));
}

#[test]
fn a_pasted_table_exports_without_markup_characters_in_docx_or_pdf() {
    let document = paste(&styled_table());
    let docx = export_docx(&document).expect("docx exports").bytes;
    let archive = PackageArchive::from_bytes(&docx).expect("zip reads back");
    let body = String::from_utf8_lossy(archive.get("word/document.xml").expect("body part"));
    assert!(body.contains("Apples") && body.contains("Note"));
    assert!(!body.contains('*'), "markup leaked into the DOCX body");

    let pdf = loom_pdf::inspect::readable_content(&export_pdf(&document)).expect("pdf reads");
    assert!(pdf.contains("Apples") && pdf.contains("Note"));
    assert!(!pdf.contains('*'), "markup leaked into the PDF text");
}

#[test]
fn literal_asterisks_in_pasted_text_stay_literal_and_are_not_styled() {
    let mut table = Table::new(1, 1);
    table.set_cell_text(0, 0, "2*3*4");
    let document = paste(&table);
    let read = document
        .loom_table_from_block(document.blocks.last().expect("table block").id)
        .expect("table parses");
    let runs = &read.cell(0, 0).expect("cell").content.runs;
    assert_eq!(read.cell_text(0, 0).as_deref(), Some("2*3*4"), "{runs:?}");
    assert!(runs.iter().all(|r| !r.bold && !r.italic), "{runs:?}");
}

#[test]
fn writer_table_round_trip_through_loom_keeps_runs() {
    let table = styled_table();
    let converted = WriterTable::from_loom_table(&table).to_loom_table();
    assert_eq!(
        converted.cell(0, 0).expect("header").content.runs,
        vec![run("Item", true, false)]
    );
    assert_eq!(
        converted.cell(1, 2).expect("note").content.runs,
        vec![run("Note", false, true)]
    );
}

#[test]
fn table_block_kind_is_unchanged_by_paste() {
    let document = paste(&styled_table());
    assert_eq!(
        document.blocks.last().expect("block").kind,
        TABLE_BLOCK_KIND
    );
}
