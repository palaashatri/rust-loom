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

/// A 3x3 table with a bold header row, a centred Qty column and a bold body cell.
fn header_bold_and_centred_table() -> Table {
    let mut table = Table::new(3, 3);
    table.header_row = true;
    for (column, text) in ["Item", "Qty", "Price"].into_iter().enumerate() {
        table.set_cell_rich_text(0, column, vec![run(text, true, false)]);
    }
    table.set_cell_rich_text(1, 0, vec![run("Apples", true, false)]);
    table.set_cell_rich_text(1, 1, vec![run("50", false, false)]);
    table.set_cell_rich_text(1, 2, vec![run("1.50", false, false)]);
    table.set_cell_rich_text(2, 0, vec![run("Pears", false, false)]);
    table.set_cell_rich_text(2, 1, vec![run("7", false, false)]);
    table.set_cell_rich_text(2, 2, vec![run("2.00", false, false)]);
    for row in 0..3 {
        table.cell_mut(row, 1).expect("cell").style_override = Some(CellStyleOverride {
            horizontal_align: Some(HorizontalAlign::Center),
            ..Default::default()
        });
    }
    table
}

/// The byte offset where cell (`row`, `column`) of the last block starts.
fn cell_offset(document: &WriterDocument, row: usize, column: usize) -> usize {
    let index = document.blocks.len() - 1;
    let before: usize = document.blocks[..index]
        .iter()
        .map(|block| block.text.len_bytes() + 1)
        .sum();
    let grid = crate::tables::compute_cell_ranges(document.blocks[index].text.as_str());
    before + grid[row][column].0
}

fn alignment_of(table: &Table, row: usize, column: usize) -> Option<HorizontalAlign> {
    table
        .cell(row, column)
        .and_then(|cell| cell.style_override.as_ref())
        .and_then(|style| style.horizontal_align)
}

fn is_bold(table: &Table, row: usize, column: usize, text: &str) -> bool {
    table.cell(row, column).is_some_and(|cell| {
        cell.content
            .runs
            .iter()
            .any(|run| run.bold && run.text == text)
    })
}

/// The Word table cell (`<w:tc>` to `</w:tc>`) whose text is `text`.
fn cell_xml<'a>(body: &'a str, text: &str) -> &'a str {
    let at = body
        .find(&format!(">{text}<"))
        .unwrap_or_else(|| panic!("{text:?} is in the body"));
    let start = body[..at]
        .rfind("<w:tc>")
        .expect("a cell starts before its text");
    let end = at + body[at..].find("</w:tc>").expect("the cell ends");
    &body[start..end]
}

#[test]
fn inserting_a_row_keeps_the_bold_header_and_the_centred_column() {
    let mut document = paste(&header_bold_and_centred_table());
    let id = document.blocks.last().expect("table block").id;
    document
        .insert_table_row_at_offset(cell_offset(&document, 0, 0), true)
        .expect("row inserted");

    let read = document
        .loom_table_from_block(id)
        .expect("stored table parses");
    assert_eq!(read.row_count(), 4);
    assert!(is_bold(&read, 0, 0, "Item"), "the header cell stays bold");
    assert!(
        is_bold(&read, 2, 0, "Apples"),
        "the bold body cell moved down with its text"
    );
    for row in 0..read.row_count() {
        assert_eq!(
            alignment_of(&read, row, 1),
            Some(HorizontalAlign::Center),
            "row {row} keeps the centred Qty column"
        );
    }
}

#[test]
fn deleting_a_row_and_inserting_or_deleting_columns_keeps_emphasis_and_alignment() {
    let mut document = paste(&header_bold_and_centred_table());
    let id = document.blocks.last().expect("table block").id;
    document
        .delete_table_row_at_offset(cell_offset(&document, 1, 0))
        .expect("row deleted");
    document
        .insert_table_column_at_offset(cell_offset(&document, 0, 1), false)
        .expect("column inserted");
    document
        .delete_table_column_at_offset(cell_offset(&document, 0, 0))
        .expect("column deleted");

    let read = document
        .loom_table_from_block(id)
        .expect("stored table parses");
    assert_eq!((read.row_count(), read.col_count()), (2, 3));
    assert!(
        is_bold(&read, 0, 1, "Qty"),
        "the header moved with its column"
    );
    assert_eq!(alignment_of(&read, 0, 1), Some(HorizontalAlign::Center));
    assert_eq!(
        alignment_of(&read, 1, 1),
        Some(HorizontalAlign::Center),
        "the centred column keeps its alignment on every row"
    );
    assert_eq!(
        alignment_of(&read, 0, 0),
        Some(HorizontalAlign::Left),
        "the inserted column is left-aligned"
    );
}

#[test]
fn the_docx_body_keeps_the_bold_run_and_the_centred_column_after_a_row_insert() {
    let mut document = paste(&header_bold_and_centred_table());
    document
        .insert_table_row_at_offset(cell_offset(&document, 0, 0), true)
        .expect("row inserted");
    let docx = export_docx(&document).expect("docx exports").bytes;
    let archive = PackageArchive::from_bytes(&docx).expect("zip reads back");
    let body =
        String::from_utf8_lossy(archive.get("word/document.xml").expect("body part")).into_owned();

    assert!(cell_xml(&body, "Item").contains("<w:b/>"), "header is bold");
    assert!(
        cell_xml(&body, "Apples").contains("<w:b/>"),
        "a bold body cell is bold in Word"
    );
    assert!(
        cell_xml(&body, "50").contains("<w:jc w:val=\"center\"/>"),
        "the centred column is centred in Word"
    );
    assert!(
        !body.contains("**") && !body.contains("\\*") && !body.contains('|'),
        "markup leaked into the DOCX body"
    );
}

#[test]
fn changing_the_table_style_keeps_the_bold_header_and_the_centred_column() {
    let mut document = paste(&header_bold_and_centred_table());
    let id = document.blocks.last().expect("table block").id;
    document
        .set_table_style_at_offset(cell_offset(&document, 0, 0), "table-style-dark-1")
        .expect("the style is applied");

    let read = document
        .loom_table_from_block(id)
        .expect("stored table parses");
    assert!(is_bold(&read, 0, 0, "Item"), "the header stays bold");
    assert!(
        is_bold(&read, 1, 0, "Apples"),
        "a bold body cell stays bold"
    );
    for row in 0..read.row_count() {
        assert_eq!(
            alignment_of(&read, row, 1),
            Some(HorizontalAlign::Center),
            "row {row} keeps the centred Qty column"
        );
    }
    assert_eq!(
        document
            .table_from_block(id)
            .expect("table parses")
            .style_id,
        "table-style-dark-1"
    );
}

#[test]
fn toggling_the_header_option_keeps_the_bold_header_and_the_centred_column() {
    let mut document = paste(&header_bold_and_centred_table());
    let id = document.blocks.last().expect("table block").id;
    for state in ["off", "on"] {
        document
            .toggle_table_option_at_offset(cell_offset(&document, 0, 0), "header_row")
            .expect("the option toggles");

        let read = document
            .loom_table_from_block(id)
            .expect("stored table parses");
        assert!(
            is_bold(&read, 0, 0, "Item"),
            "the header stays bold with the header option {state}"
        );
        assert!(
            is_bold(&read, 1, 0, "Apples"),
            "a bold body cell stays bold with the header option {state}"
        );
        for row in 0..read.row_count() {
            assert_eq!(
                alignment_of(&read, row, 1),
                Some(HorizontalAlign::Center),
                "row {row} keeps the centred Qty column with the header option {state}"
            );
        }
    }
}

#[test]
fn merging_cells_keeps_the_surviving_cell_bold_and_writes_no_markup_into_its_text() {
    let mut document = paste(&header_bold_and_centred_table());
    let id = document.blocks.last().expect("table block").id;
    document
        .merge_cells_at_offset(cell_offset(&document, 1, 0), 1, 2)
        .expect("the two body cells merge");

    let read = document
        .loom_table_from_block(id)
        .expect("stored table parses");
    assert!(
        is_bold(&read, 1, 0, "Apples"),
        "the surviving cell keeps its bold text"
    );
    let shown = document.table_from_block(id).expect("table parses");
    let text = shown.get(1, 0).expect("the surviving cell");
    assert_eq!(text, "Apples 50");
    assert!(
        !text.contains('*') && !text.contains('|'),
        "markup leaked into the merged cell text: {text:?}"
    );
}

#[test]
fn splitting_a_merged_cell_keeps_the_neighbouring_bold_and_centred_cells() {
    let mut model = header_bold_and_centred_table();
    model
        .merge_cells(1, 0, 1, 2)
        .expect("the model merges the body cells");
    let mut document = paste(&model);
    let id = document.blocks.last().expect("table block").id;
    assert_eq!(
        document.table_from_block(id).expect("table parses").merges,
        vec![(1, 0, 1, 2)],
        "the merge is stored before the split"
    );

    document
        .split_cell_at_offset(cell_offset(&document, 1, 0))
        .expect("the merged cell splits");

    let read = document
        .loom_table_from_block(id)
        .expect("stored table parses");
    assert!(is_bold(&read, 0, 0, "Item"), "the header keeps its bold");
    assert!(
        is_bold(&read, 0, 2, "Price"),
        "the header keeps its bold on the right"
    );
    for row in 0..read.row_count() {
        assert_eq!(
            alignment_of(&read, row, 1),
            Some(HorizontalAlign::Center),
            "row {row} keeps the centred Qty column after the split"
        );
    }
    assert!(document
        .table_from_block(id)
        .expect("table parses")
        .merges
        .is_empty());
}
