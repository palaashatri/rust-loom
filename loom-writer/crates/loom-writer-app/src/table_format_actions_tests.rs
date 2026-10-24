//! Table formatting in Loom Writer: Ctrl+B and Ctrl+I through the keyboard, the
//! paragraph guard, column alignment, and what the page draws for a cell (its
//! typed runs, alignment, column widths and caret).

use std::rc::Rc;

use loom_writer_core::loom_objects::table::{HorizontalAlign, Table, TextRun};
use loom_writer_core::{PageViewport, RichBlock, TextSelection, WriterDocument};

use crate::actions_tests::test_state;
use crate::projection;
use crate::table_actions::{self, table_format_actions, TableCellRange};
use crate::{writer_render_alignment, GuiState, WriterApp};

const BLOCK: u64 = 42;

/// A paragraph and a table of `rows` x `columns` filled from `texts`, row by
/// row. The table block is `BLOCK`.
fn table_document(rows: usize, columns: usize, texts: &[&str]) -> WriterDocument {
    let mut document = WriterDocument::new("table-format", "Table Format");
    document.replace_paragraphs("Intro");
    let mut table = Table::new(rows, columns);
    for (index, text) in texts.iter().enumerate() {
        table.set_cell_text(index / columns, index % columns, *text);
    }
    document.blocks.push(RichBlock::table_block(BLOCK, table));
    document
}

fn app_with_table(rows: usize, columns: usize, texts: &[&str]) -> (WriterApp, Rc<GuiState>) {
    table_actions::clear_table_cell_selection_range();
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], [None]));
    test_state(table_document(rows, columns, texts), dialogs)
}

/// The editor offset where the table block starts.
fn block_start(document: &WriterDocument) -> usize {
    document
        .blocks
        .iter()
        .take_while(|block| block.id != BLOCK)
        .map(|block| block.text.len_bytes() + 1)
        .sum()
}

/// The editor offset of byte `at` of a cell's text.
fn offset_in_cell(document: &WriterDocument, row: usize, column: usize, at: usize) -> usize {
    let (_, offsets) = document
        .table_cell_display_map(BLOCK, row, column)
        .expect("a table cell");
    block_start(document) + offsets[at]
}

fn cell_start(state: &GuiState, row: usize, column: usize) -> usize {
    offset_in_cell(&state.current.borrow(), row, column, 0)
}

fn place_caret(state: &GuiState, offset: usize) {
    state
        .current
        .borrow_mut()
        .set_selection(TextSelection::caret(offset));
}

fn select(state: &GuiState, start: usize, end: usize) {
    state
        .current
        .borrow_mut()
        .set_selection(TextSelection::range(start, end));
}

fn cell_range(start: (usize, usize), end: (usize, usize)) -> TableCellRange {
    TableCellRange {
        block_id: BLOCK,
        start_row: start.0,
        start_col: start.1,
        end_row: end.0,
        end_col: end.1,
    }
}

fn bold(run: &TextRun) -> bool {
    run.bold
}

fn italic(run: &TextRun) -> bool {
    run.italic
}

fn alignment_of(table: &Table, column: usize) -> Option<HorizontalAlign> {
    (0..table.row_count()).find_map(|row| {
        table
            .cell(row, column)
            .and_then(|cell| cell.style_override.as_ref())
            .and_then(|style| style.horizontal_align)
    })
}

/// The x of the caret the page draws, or `None` when no caret is drawn.
fn caret_x(document: &WriterDocument) -> Option<f32> {
    projection::project(document, PageViewport::default(), None)
        .selection_rects
        .iter()
        .find(|rect| rect.caret)
        .map(|rect| rect.x)
}

#[test]
fn ctrl_b_on_a_cell_range_bolds_every_selected_cell_as_one_undo_step() {
    let (app, state) = app_with_table(3, 3, &["A", "B", "C", "D", "E", "F", "G", "H", "I"]);
    table_actions::set_table_cell_selection_range(Some(cell_range((0, 0), (1, 1))));
    place_caret(&state, cell_start(&state, 0, 0));
    let undo_before = state.history.borrow().undo_len();

    assert!(
        table_actions::handle_table_key(&app, &state, "b", false, true, false),
        "Ctrl+B is handled by the table range, not by typing style"
    );

    assert_eq!(
        state
            .current
            .borrow()
            .table_cells_have(BLOCK, 0..2, 0..2, bold),
        Some(true),
        "every cell of the 2 x 2 range is bold in its typed runs"
    );
    assert_eq!(
        state
            .current
            .borrow()
            .table_cells_have(BLOCK, 2..3, 0..3, bold),
        Some(false),
        "cells outside the range keep their formatting"
    );
    assert_eq!(
        state.history.borrow().undo_len(),
        undo_before + 1,
        "the whole range is one undo step"
    );
    assert_eq!(app.get_status_right().as_str(), "Bold applied to 4 cells");

    assert!(table_actions::handle_table_key(
        &app, &state, "b", false, true, false
    ));
    assert_eq!(
        state
            .current
            .borrow()
            .table_cells_have(BLOCK, 0..2, 0..2, bold),
        Some(false),
        "a second Ctrl+B takes the bold off again"
    );
    assert_eq!(app.get_status_right().as_str(), "Bold removed from 4 cells");
}

#[test]
fn ctrl_i_inside_one_cell_italicises_only_the_selected_text_of_that_cell() {
    let (app, state) = app_with_table(2, 2, &["Hello", "World", "Third", "Fourth"]);
    let start = cell_start(&state, 0, 0);
    select(&state, start + 1, start + 4);

    assert!(table_actions::handle_table_key(
        &app, &state, "i", false, true, false
    ));

    let doc = state.current.borrow();
    assert_eq!(
        doc.table_text_has(start + 1..start + 4, italic),
        Some(true),
        "the selected text is italic in the cell's typed runs"
    );
    assert_eq!(
        doc.table_text_has(start..start + 5, italic),
        Some(false),
        "the rest of the cell is not italic"
    );
    assert_eq!(
        doc.table_cells_have(BLOCK, 0..2, 0..2, bold),
        Some(false),
        "italic does not make bold"
    );
    assert_eq!(
        app.get_status_right().as_str(),
        "Italic applied to the selected table text"
    );
}

#[test]
fn ctrl_b_outside_a_table_or_with_a_collapsed_caret_is_left_to_the_paragraph_command() {
    let (app, state) = app_with_table(2, 2, &["A", "B", "C", "D"]);
    place_caret(&state, 2);
    let undo_before = state.history.borrow().undo_len();
    assert!(
        !table_actions::handle_table_key(&app, &state, "b", false, true, false),
        "a paragraph caret is not a table command"
    );
    place_caret(&state, cell_start(&state, 0, 1));
    assert!(
        !table_actions::handle_table_key(&app, &state, "b", false, true, false),
        "a collapsed caret in a cell keeps the typing style"
    );
    assert_eq!(
        state.history.borrow().undo_len(),
        undo_before,
        "nothing changed"
    );
}

#[test]
fn centre_on_a_table_caret_centres_only_that_column() {
    let (app, state) = app_with_table(3, 3, &["1", "2", "3", "4", "5", "6", "7", "8", "9"]);
    place_caret(&state, cell_start(&state, 1, 1));

    assert!(table_format_actions::align_table_columns_for_index(
        &app, &state, 1
    ));

    let doc = state.current.borrow();
    let table = doc.loom_table_from_block(BLOCK).expect("table");
    assert_eq!(alignment_of(&table, 1), Some(HorizontalAlign::Center));
    assert_eq!(
        alignment_of(&table, 0),
        None,
        "column 1 keeps its alignment"
    );
    assert_eq!(
        alignment_of(&table, 2),
        None,
        "column 3 keeps its alignment"
    );
    assert_eq!(
        app.get_status_right().as_str(),
        "Center alignment applied to 1 column"
    );
}

#[test]
fn the_column_action_aligns_every_column_of_the_active_cell_range() {
    let (app, state) = app_with_table(2, 3, &["a", "b", "c", "d", "e", "f"]);
    table_actions::set_table_cell_selection_range(Some(cell_range((0, 1), (1, 2))));
    place_caret(&state, cell_start(&state, 0, 1));

    table_actions::table_action(&app, &state, "align-column-right");

    let doc = state.current.borrow();
    let table = doc.loom_table_from_block(BLOCK).expect("table");
    assert_eq!(alignment_of(&table, 1), Some(HorizontalAlign::Right));
    assert_eq!(alignment_of(&table, 2), Some(HorizontalAlign::Right));
    assert_eq!(
        alignment_of(&table, 0),
        None,
        "column 1 is outside the range"
    );
}

#[test]
fn alignment_outside_a_table_is_left_to_the_paragraph_command() {
    let (app, state) = app_with_table(2, 2, &["A", "B", "C", "D"]);
    place_caret(&state, 2);
    assert!(!table_format_actions::align_table_columns_for_index(
        &app, &state, 1
    ));
    let doc = state.current.borrow();
    let table = doc.loom_table_from_block(BLOCK).expect("table");
    assert_eq!(alignment_of(&table, 0), None, "the table is untouched");
    assert_eq!(alignment_of(&table, 1), None, "the table is untouched");
}

#[test]
fn a_centred_column_is_drawn_centred_and_its_caret_moves_with_the_text() {
    let mut document = table_document(2, 2, &["Name", "Qty", "Tea", "2"]);
    let end_of_two = offset_in_cell(&document, 1, 1, 1);
    document.set_selection(TextSelection::caret(end_of_two));
    let left = caret_x(&document).expect("a caret in the cell");

    assert!(document
        .set_table_column_alignment(BLOCK, 1..2, HorizontalAlign::Center)
        .is_ok());
    let centred_page = projection::project(&document, PageViewport::default(), None);
    let cells: Vec<_> = centred_page
        .rows
        .iter()
        .filter(|row| row.table_cell)
        .collect();
    assert_eq!(cells.len(), 4, "every non-empty cell is drawn");
    assert_eq!(
        cells[3].alignment,
        writer_render_alignment(loom_text::Alignment::Center),
        "the cell of column 2 carries the centre alignment"
    );

    let centred = caret_x(&document).expect("a caret in the cell");
    assert!(
        centred > left + 10.0,
        "the caret moves to the centred text: {left} -> {centred}"
    );
}

#[test]
fn a_cell_is_drawn_from_its_typed_runs_and_plain_cells_carry_none() {
    let mut document = table_document(1, 2, &["Plain", "Bold"]);
    document
        .set_table_cells_runs(BLOCK, 0..1, 1..2, |run| {
            run.bold = true;
            run.italic = true;
            run.underline = true;
        })
        .expect("style the second cell");
    let table = document.loom_table_from_block(BLOCK).expect("table");

    let styled = projection::cell_text_block(BLOCK, &table, 0, 1, false).expect("text");
    assert_eq!(styled.text.as_str(), "Bold");
    assert_eq!(styled.runs.len(), 1, "one run covers the whole text");
    let style = &styled.runs[0];
    assert_eq!((style.start, style.end), (0, 4));
    assert_eq!(style.style.weight, loom_text::FontWeight::Bold);
    assert!(style.style.italic && style.style.underline);

    let plain = projection::cell_text_block(BLOCK, &table, 0, 0, false).expect("text");
    assert!(plain.runs.is_empty(), "plain cells carry no style runs");
}

#[test]
fn typed_column_widths_are_the_drawn_column_geometry() {
    let mut document = table_document(1, 2, &["A", "B"]);
    let mut table = document.loom_table_from_block(BLOCK).expect("table");
    table.columns[0].width_pt *= 3.0;
    document
        .blocks
        .iter_mut()
        .find(|block| block.id == BLOCK)
        .expect("table block")
        .set_table(table);

    let page = projection::project(&document, PageViewport::default(), None);
    let cells: Vec<_> = page
        .table_cell_rects
        .iter()
        .filter(|rect| rect.table_cell)
        .collect();
    assert_eq!(cells.len(), 2);
    let ratio = cells[0].width / cells[1].width;
    assert!(
        (ratio - 3.0).abs() < 0.01,
        "column A is three times column B: {ratio}"
    );
    assert!(
        (cells[1].x - (cells[0].x + cells[0].width)).abs() < 0.01,
        "column B starts where column A ends"
    );
}
