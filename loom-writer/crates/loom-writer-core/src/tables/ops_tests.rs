//! Table operations over typed blocks: insertion, Tab and arrow navigation,
//! row and column edits, merges, styles, and the export and count contracts.
//! These were written against Markdown text before typed storage; they now
//! assert typed data, and the Markdown fixtures go through the migration.

use loom_objects::table::{CellStyleOverride, HorizontalAlign, Table, TextRun};

use super::{markdown_table_block, TableArrowDirection, TABLE_BLOCK_KIND, TABLE_CELL_SEPARATOR};
use crate::{RichBlock, TextSelection, WriterDocument};

fn document() -> WriterDocument {
    let mut document = WriterDocument::new("doc", "Doc");
    document
        .blocks
        .push(RichBlock::new(1, "paragraph", "First"));
    document
        .blocks
        .push(RichBlock::new(2, "paragraph", "Second"));
    document
}

/// A paragraph, a 3x2 table and a paragraph after it.
fn document_with_table_between_paragraphs() -> WriterDocument {
    let mut document = WriterDocument::new("doc", "Doc");
    document
        .blocks
        .push(RichBlock::new(1, "paragraph", "Intro"));
    document.blocks.push(markdown_table_block(
        2,
        "| Region | Sales |\n| --- | --- |\n| North | 10 |\n| South | 7 |",
    ));
    document
        .blocks
        .push(RichBlock::new(3, "paragraph", "Outro"));
    document
}

fn kinds(document: &WriterDocument) -> Vec<String> {
    document
        .blocks
        .iter()
        .map(|b| b.kind.as_str().to_string())
        .collect()
}

fn table_of(document: &WriterDocument, id: u64) -> Table {
    document
        .loom_table_from_block(id)
        .expect("a typed table block")
}

#[test]
fn insert_table_appends_block_and_moves_caret() {
    let mut document = document();
    let id = document
        .insert_table_block(usize::MAX, 3, 3)
        .expect("insert succeeds");
    assert_eq!(id, 3);
    let table = table_of(&document, id);
    assert_eq!((table.row_count(), table.col_count()), (3, 3));
    assert!(table.header_row);
    assert_eq!(
        document.blocks.last().expect("block").kind,
        TABLE_BLOCK_KIND
    );
    assert!(document.selection().anchor > 0);
}

#[test]
fn insert_table_after_offset_places_block_between_paragraphs() {
    let mut document = document();
    let first_len = document.blocks[0].text.len_bytes();
    document
        .insert_table_block(first_len, 2, 2)
        .expect("insert succeeds");
    assert_eq!(document.blocks[1].kind, TABLE_BLOCK_KIND);
    assert_eq!(document.blocks[2].text.as_str(), "Second");
}

#[test]
fn insert_rejects_empty_geometry() {
    let mut document = document();
    assert!(document.insert_table_block(0, 0, 3).is_err());
    assert!(document.insert_table_block(0, 3, 0).is_err());
}

#[test]
fn a_table_exports_as_markdown_and_reads_back_as_the_same_table() {
    let mut document = document();
    let id = document.insert_table_block(usize::MAX, 2, 2).unwrap();
    let typed = table_of(&document, id);
    let markdown = crate::tables::table_markdown(&typed);
    let reread = markdown_table_block(id, &markdown);
    let reread = reread.typed_table().expect("typed");
    // The read-back writes each column's explicit alignment, so compare the grid and text.
    assert_eq!(
        (reread.row_count(), reread.col_count()),
        (typed.row_count(), typed.col_count())
    );
    assert_eq!(
        crate::tables::flat_table_text(reread),
        crate::tables::flat_table_text(&typed)
    );
}

#[test]
fn table_blocks_export_empty_cells_as_markdown_gaps() {
    let mut document = document();
    document
        .insert_table_block(usize::MAX, 2, 2)
        .expect("insert succeeds");
    assert!(document.to_markdown().contains("|  |  |"));
}

#[test]
fn caret_selection_stays_normalized() {
    let mut document = document();
    document
        .insert_table_block(usize::MAX, 2, 2)
        .expect("insert succeeds");
    let selection = document.selection();
    assert!(selection.anchor <= document.editor_text().len());
    let _ = TextSelection::caret(0);
}

#[test]
fn typing_in_a_table_cell_keeps_the_table_one_block() {
    let mut document = document_with_table_between_paragraphs();
    let text = document.editor_text();
    let at = text.find("North").expect("cell text") + "North".len();
    let mut edited = text.clone();
    edited.insert(at, 'X');
    document
        .replace_editor_text_at(&edited, Some(at + 1))
        .expect("cell edit");

    assert_eq!(
        kinds(&document),
        ["paragraph", TABLE_BLOCK_KIND, "paragraph"],
        "the table stays one table block"
    );
    assert_eq!(document.blocks[0].text.as_str(), "Intro");
    assert_eq!(document.blocks[2].text.as_str(), "Outro");
    let table = table_of(&document, 2);
    assert_eq!(table.cell_text(1, 0).as_deref(), Some("NorthX"));
    assert_eq!(
        table.cell_text(1, 1).as_deref(),
        Some("10"),
        "other cells are untouched"
    );

    let pdf = crate::export::test_support::export_valid_pdf(&document);
    let text = loom_pdf::inspect::page_text(&pdf)
        .expect("extracted PDF text")
        .join("\n");
    assert!(!text.contains('|'), "no pipe characters are printed");
    assert!(text.contains("NorthX"), "the edited cell is printed");
}

#[test]
fn enter_after_a_table_starts_a_paragraph_and_keeps_the_table() {
    let mut document = document_with_table_between_paragraphs();
    let table_text = document.blocks[1].text.as_str().to_string();
    let text = document.editor_text();
    let table_end = text.find("Outro").expect("next paragraph") - 1;
    let mut edited = text.clone();
    edited.insert(table_end, '\n');
    document
        .replace_editor_text_at(&edited, Some(table_end + 1))
        .expect("enter");

    assert_eq!(
        kinds(&document),
        ["paragraph", TABLE_BLOCK_KIND, "paragraph", "paragraph"]
    );
    assert_eq!(document.blocks[1].text.as_str(), table_text);
    assert_eq!(document.blocks[2].text.as_str(), "");
    assert_eq!(document.blocks[3].text.as_str(), "Outro");
}

#[test]
fn table_cells_count_as_words_and_characters_not_as_markdown() {
    let mut document = WriterDocument::new("counts", "Counts");
    document
        .blocks
        .push(RichBlock::new(1, "paragraph", "Intro words here"));
    document.blocks.push(markdown_table_block(
        2,
        "| Item | Qty |\n| --- | --- |\n| Apples | 3 |",
    ));

    // The paragraph has 3 words and 16 characters. The table's cells are
    // Item, Qty, Apples and 3: 4 words and 14 characters.
    let stats = document.statistics();
    assert_eq!(stats.word_count, 7);
    assert_eq!(stats.char_count, 30);
    assert_eq!(stats.char_count_no_spaces, 28);
    // The status bar also counts the one character between the two blocks.
    assert_eq!(document.text_counts(), (7, 31));
    let pagination = document.estimate_pagination();
    assert_eq!((pagination.words, pagination.characters), (7, 31));
}

#[test]
fn insert_table_places_caret_inside_first_cell() {
    let mut document = document();
    let _ = document.insert_table_block(usize::MAX, 2, 2).unwrap();
    let block_offset = document
        .blocks
        .iter()
        .take(document.blocks.len() - 1)
        .map(|b| b.text.len_bytes() + 1)
        .sum::<usize>();
    assert_eq!(document.selection().anchor, block_offset);
}

#[test]
fn writer_table_converts_to_and_from_loom_objects_table() {
    let mut writer_table = crate::WriterTable::new("tbl-1", 2, 3);
    writer_table.rows[0] = vec!["H1".into(), "H2".into(), "H3".into()];
    writer_table.rows[1] = vec!["A".into(), "B".into(), "C".into()];
    writer_table.header_row = true;
    let loom_table = writer_table.to_loom_table();
    assert_eq!(loom_table.row_count(), 2);
    assert_eq!(loom_table.col_count(), 3);
    assert_eq!(loom_table.cell_text(0, 0).as_deref(), Some("H1"));
    assert_eq!(loom_table.cell_text(1, 2).as_deref(), Some("C"));
    let converted_back = crate::WriterTable::from_loom_table(&loom_table);
    assert_eq!(writer_table, converted_back);
}

#[test]
fn a_pipe_in_legacy_text_before_the_first_pipe_stays_in_its_cell() {
    let block = markdown_table_block(
        1,
        "TypedBefore| Col1 | Col2 |\n| --- | --- |\n| Val1 | Val2 |",
    );
    let table = block.typed_table().expect("typed");
    assert_eq!(table.row_count(), 2);
    assert!(table
        .cell_text(0, 0)
        .unwrap_or_default()
        .contains("TypedBefore"));
}

#[test]
fn table_tab_navigates_and_appends_row_at_end() {
    let mut document = document();
    let _ = document.insert_table_block(usize::MAX, 2, 2).unwrap();
    let block_id = document.blocks.last().expect("table").id;
    let initial_caret = document.selection().anchor;
    let loc = document.table_cell_at_offset(initial_caret).unwrap();
    assert_eq!((loc.row, loc.col), (0, 0));

    let c1 = document.tab_navigate_table(initial_caret, false).unwrap();
    let loc1 = document.table_cell_at_offset(c1).unwrap();
    assert_eq!((loc1.row, loc1.col), (0, 1));
    let c2 = document.tab_navigate_table(c1, false).unwrap();
    let loc2 = document.table_cell_at_offset(c2).unwrap();
    assert_eq!((loc2.row, loc2.col), (1, 0));
    let c3 = document.tab_navigate_table(c2, false).unwrap();
    let loc3 = document.table_cell_at_offset(c3).unwrap();
    assert_eq!((loc3.row, loc3.col), (1, 1));
    // Tab from the final cell appends a row and places the caret at (2, 0).
    let c4 = document.tab_navigate_table(c3, false).unwrap();
    let loc4 = document.table_cell_at_offset(c4).unwrap();
    assert_eq!((loc4.row, loc4.col), (2, 0));
    assert_eq!(table_of(&document, block_id).row_count(), 3);
}

#[test]
fn table_arrow_navigation_moves_between_cells() {
    let mut document = document();
    let _ = document.insert_table_block(usize::MAX, 3, 3).unwrap();
    let initial_caret = document.selection().anchor;
    let down1 = document
        .arrow_navigate_table(initial_caret, TableArrowDirection::Down)
        .unwrap();
    let loc_down1 = document.table_cell_at_offset(down1).unwrap();
    assert_eq!((loc_down1.row, loc_down1.col), (1, 0));
    let right1 = document
        .arrow_navigate_table(down1, TableArrowDirection::Right)
        .unwrap();
    let loc_r1 = document.table_cell_at_offset(right1).unwrap();
    assert_eq!((loc_r1.row, loc_r1.col), (1, 1));
    let up1 = document
        .arrow_navigate_table(right1, TableArrowDirection::Up)
        .unwrap();
    let loc_u1 = document.table_cell_at_offset(up1).unwrap();
    assert_eq!((loc_u1.row, loc_u1.col), (0, 1));
}

#[test]
fn table_insert_and_delete_rows_and_columns() {
    let mut document = document();
    let _ = document.insert_table_block(usize::MAX, 2, 2).unwrap();
    let block_id = document.blocks.last().expect("table").id;
    let caret = document.selection().anchor;

    let c_row = document.insert_table_row_at_offset(caret, true).unwrap();
    assert_eq!(table_of(&document, block_id).row_count(), 3);
    let c_del_row = document.delete_table_row_at_offset(c_row).unwrap();
    assert_eq!(table_of(&document, block_id).row_count(), 2);

    let c_col = document
        .insert_table_column_at_offset(c_del_row, true)
        .unwrap();
    assert_eq!(table_of(&document, block_id).col_count(), 3);
    let _ = document.delete_table_column_at_offset(c_col).unwrap();
    assert_eq!(table_of(&document, block_id).col_count(), 2);
}

/// A 2x2 table whose (0,0) cell is bold and whose (1,1) cell is centred.
fn styled_document() -> (WriterDocument, u64) {
    let mut table = Table::new(2, 2);
    table.id = "styled".to_string();
    table.set_cell_rich_text(0, 0, vec![TextRun::bold("Head")]);
    table.set_cell_text(0, 1, "B");
    table.set_cell_text(1, 0, "C");
    table.set_cell_text(1, 1, "D");
    table.cell_mut(1, 1).expect("cell").style_override = Some(CellStyleOverride {
        horizontal_align: Some(HorizontalAlign::Center),
        ..Default::default()
    });
    let mut document = WriterDocument::new("doc", "Doc");
    document.push(RichBlock::table_block(1, table));
    (document, 1)
}

#[test]
fn structural_edits_keep_bold_and_alignment() {
    let (mut document, id) = styled_document();
    let caret = document.table_cell_ranges(id).expect("ranges")[0][0].0;
    document
        .insert_table_row_at_offset(caret, true)
        .expect("row");
    document
        .insert_table_column_at_offset(caret, true)
        .expect("column");
    let table = table_of(&document, id);
    assert_eq!(
        table.cell(0, 0).expect("cell").content.runs,
        vec![TextRun::bold("Head")],
        "bold survives a row and a column insert"
    );
    let centred = table
        .cell(2, 2)
        .and_then(|cell| cell.style_override.as_ref())
        .and_then(|style| style.horizontal_align);
    assert_eq!(
        centred,
        Some(HorizontalAlign::Center),
        "alignment moves with its cell"
    );
}

#[test]
fn merge_and_split_keep_the_grid_and_the_origin() {
    let (mut document, id) = styled_document();
    let caret = document.table_cell_ranges(id).expect("ranges")[0][0].0;
    document.merge_cells_at_offset(caret, 1, 2).expect("merge");
    let merged = table_of(&document, id);
    assert_eq!(merged.col_count(), 2, "a merge keeps the grid");
    assert!(merged.is_covered(0, 1));
    document.split_cell_at_offset(caret).expect("split");
    assert!(!table_of(&document, id).is_covered(0, 1));
}

#[test]
fn table_style_and_options_are_stored_on_the_typed_table() {
    let (mut document, id) = styled_document();
    let caret = document.table_cell_ranges(id).expect("ranges")[0][0].0;
    document
        .set_table_style_at_offset(caret, "table-style-light-1")
        .expect("style");
    document
        .toggle_table_option_at_offset(caret, "banded_rows")
        .expect("option");
    let table = table_of(&document, id);
    assert_eq!(table.style_id, "table-style-light-1");
    assert!(!table.banded_rows, "the option toggled off");
    assert!(document
        .toggle_table_option_at_offset(caret, "not_an_option")
        .is_err());
}

// Typed cell formatting: bold, italic and underline live on each cell's runs,
// and column alignment lives on each cell's style override.

fn is_bold(run: &TextRun) -> bool {
    run.bold
}

fn make_bold(run: &mut TextRun) {
    run.bold = true;
}

fn make_plain(run: &mut TextRun) {
    run.bold = false;
}

/// A 2x2 table `7` with A B / C D, after a paragraph.
fn two_by_two_document() -> WriterDocument {
    let mut document = WriterDocument::new("doc", "Doc");
    document
        .blocks
        .push(RichBlock::new(1, "paragraph", "Intro"));
    let mut table = Table::new(2, 2);
    table.set_cell_text(0, 0, "A");
    table.set_cell_text(0, 1, "B");
    table.set_cell_text(1, 0, "C");
    table.set_cell_text(1, 1, "D");
    document.blocks.push(RichBlock::table_block(7, table));
    document
}

fn cell_pieces(table: &Table, row: usize, column: usize) -> Vec<(String, bool)> {
    table
        .cell(row, column)
        .expect("cell")
        .content
        .runs
        .iter()
        .map(|run| (run.text.clone(), run.bold))
        .collect()
}

#[test]
fn bold_on_a_cell_range_is_stored_on_each_selected_cell_only() {
    let mut document = two_by_two_document();
    assert_eq!(
        document.table_cells_have(7, 0..2, 0..2, is_bold),
        Some(false)
    );
    document
        .set_table_cells_runs(7, 0..1, 0..2, make_bold)
        .expect("bold the top row");
    let table = table_of(&document, 7);
    assert_eq!(cell_pieces(&table, 0, 0), vec![("A".to_string(), true)]);
    assert_eq!(cell_pieces(&table, 0, 1), vec![("B".to_string(), true)]);
    assert_eq!(cell_pieces(&table, 1, 0), vec![("C".to_string(), false)]);
    assert_eq!(
        table.cell_text(0, 1).as_deref(),
        Some("B"),
        "text unchanged"
    );
    assert_eq!(
        document.table_cells_have(7, 0..1, 0..2, is_bold),
        Some(true)
    );
    assert_eq!(
        document.table_cells_have(7, 0..2, 0..2, is_bold),
        Some(false)
    );
}

#[test]
fn bold_on_selected_text_inside_one_cell_changes_only_that_text() {
    let mut document = WriterDocument::new("doc", "Doc");
    let mut table = Table::new(1, 2);
    table.set_cell_text(0, 0, "Hello");
    table.set_cell_text(0, 1, "World");
    document.blocks.push(RichBlock::table_block(3, table));
    // Flat text is "Hello", a separator at 5, then "World". Select "ell".
    assert_eq!(document.table_block_id_for_range(1..4), Some(3));
    assert_eq!(document.set_table_text_runs(1..4, make_bold), Ok(true));
    let table = table_of(&document, 3);
    assert_eq!(
        cell_pieces(&table, 0, 0),
        vec![
            ("H".to_string(), false),
            ("ell".to_string(), true),
            ("o".to_string(), false),
        ]
    );
    assert_eq!(
        cell_pieces(&table, 0, 1),
        vec![("World".to_string(), false)]
    );
    assert_eq!(document.table_text_has(1..4, is_bold), Some(true));
    assert_eq!(document.table_text_has(0..5, is_bold), Some(false));
}

#[test]
fn restyling_table_text_preserves_strike_and_script_run_boundaries() {
    for (name, feature) in [("strikethrough", 0), ("superscript", 1), ("subscript", 2)] {
        let first = TextRun::plain("A");
        let mut second = TextRun::plain("B");
        match feature {
            0 => second.strikethrough = true,
            1 => second.superscript = true,
            _ => second.subscript = true,
        }
        let has_feature = |run: &TextRun| match feature {
            0 => run.strikethrough,
            1 => run.superscript,
            _ => run.subscript,
        };
        let mut table = Table::new(1, 1);
        table.set_cell_rich_text(0, 0, vec![first, second]);
        let mut document = WriterDocument::new("typed-table-style", "Typed table style");
        document.blocks.push(RichBlock::table_block(3, table));

        assert_eq!(
            document.set_table_text_runs(0..2, |run| run.underline = true),
            Ok(true),
            "underline both runs while retaining the {name} distinction"
        );

        let table = table_of(&document, 3);
        let runs = &table.cell(0, 0).expect("cell").content.runs;
        assert_eq!(runs.len(), 2, "the {name} boundary must remain separate");
        assert_eq!((runs[0].text.as_str(), runs[1].text.as_str()), ("A", "B"));
        assert!(runs.iter().all(|run| run.underline));
        assert!(!has_feature(&runs[0]), "A should remain without {name}");
        assert!(has_feature(&runs[1]), "B should keep {name}");
    }
}

#[test]
fn text_outside_a_table_and_text_crossing_into_one_is_not_a_table_range() {
    let document = document_with_table_between_paragraphs();
    let mut document = document;
    assert_eq!(
        document.table_block_id_for_range(0..5),
        None,
        "the intro paragraph"
    );
    assert_eq!(document.table_text_has(0..5, is_bold), None);
    assert_eq!(document.set_table_text_runs(0..5, make_bold), Ok(false));
    // "Intro" ends at 5 and the table block starts at 6: 0..8 crosses both.
    assert_eq!(document.table_block_id_for_range(0..8), None);
    assert_eq!(document.set_table_text_runs(0..8, make_bold), Ok(false));
    assert!(document
        .blocks
        .iter()
        .filter(|block| block.kind == TABLE_BLOCK_KIND)
        .all(|block| block.typed_table().is_some_and(|table| table
            .rows
            .iter()
            .flat_map(|row| row.cells.iter())
            .all(|cell| cell.content.runs.iter().all(|run| !run.bold)))));
}

#[test]
fn column_alignment_is_written_to_every_cell_of_that_column_only() {
    let mut document = two_by_two_document();
    document
        .set_table_column_alignment(7, 1..2, HorizontalAlign::Center)
        .expect("align column B");
    let table = table_of(&document, 7);
    for row in 0..2 {
        let aligned = table
            .cell(row, 1)
            .and_then(|cell| cell.style_override.as_ref())
            .and_then(|style| style.horizontal_align);
        assert_eq!(aligned, Some(HorizontalAlign::Center), "row {row} column B");
        let untouched = table
            .cell(row, 0)
            .and_then(|cell| cell.style_override.as_ref())
            .and_then(|style| style.horizontal_align);
        assert_eq!(untouched, None, "row {row} column A keeps its alignment");
    }
}

#[test]
fn typing_into_a_bold_empty_cell_produces_bold_text() {
    let mut document = WriterDocument::new("doc", "Doc");
    document
        .blocks
        .push(RichBlock::table_block(4, Table::new(1, 2)));
    document
        .set_table_cells_runs(4, 0..1, 0..1, make_bold)
        .expect("bold the empty cell");
    assert_eq!(
        document.table_cells_have(4, 0..1, 0..1, is_bold),
        Some(true)
    );
    document
        .edit_table_text(4, &format!("x{TABLE_CELL_SEPARATOR}"))
        .expect("type into the cell");
    let table = table_of(&document, 4);
    assert_eq!(cell_pieces(&table, 0, 0), vec![("x".to_string(), true)]);
}

#[test]
fn bold_on_a_merged_cell_skips_the_cells_it_covers() {
    let mut document = WriterDocument::new("doc", "Doc");
    let mut table = Table::new(1, 2);
    table.set_cell_text(0, 0, "Hi");
    table.merge_cells(0, 0, 1, 2).expect("merge");
    document.blocks.push(RichBlock::table_block(5, table));
    document
        .set_table_cells_runs(5, 0..1, 0..2, make_bold)
        .expect("bold the merged row");
    assert_eq!(
        document.table_cells_have(5, 0..1, 0..2, is_bold),
        Some(true)
    );
    document
        .set_table_cells_runs(5, 0..1, 0..2, make_plain)
        .expect("plain again");
    assert_eq!(
        document.table_cells_have(5, 0..1, 0..2, is_bold),
        Some(false)
    );
    assert_eq!(
        table_of(&document, 5).cell_text(0, 0).as_deref(),
        Some("Hi")
    );
}
