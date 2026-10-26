//! Table formatting in Loom Writer: Ctrl+B and Ctrl+I through the keyboard, the
//! paragraph guard, column alignment, and what the page draws for a cell (its
//! typed runs, alignment, column widths and caret).

use std::rc::Rc;

use loom_writer_core::loom_objects::table::{HorizontalAlign, Table, TextRun};
use loom_writer_core::{PageViewport, RichBlock, TextSelection, WriterDocument};
use slint::ComponentHandle;

use crate::actions_tests::test_state;
use crate::projection;
use crate::table_actions::{self, table_format_actions, TableCellRange};
use crate::{writer_render_alignment, GuiState, WriterApp};

const BLOCK: u64 = 42;
const PENDING_TABLE_SCRIPT_RECOVERY_MODE: &str = "LOOM_WRITER_TABLE_SCRIPT_RECOVERY_TEST";
const PENDING_TABLE_SCRIPT_RECOVERY_TEST: &str =
    "table_actions::table_format_actions::tests::pending_table_script_formats_only_new_text_and_survives_save_undo_and_recovery";

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
fn font_family_choice_styles_typed_table_runs_and_is_undoable() {
    let (app, state) = app_with_table(1, 1, &["Cell text"]);
    let start = cell_start(&state, 0, 0);
    select(&state, start, start + "Cell text".len());
    crate::wire_writer_shared_callbacks(&app, &state, None);

    crate::font_picker::apply_family(&app, &state, "Serif");

    let document = state.current.borrow();
    let table = document.loom_table_from_block(BLOCK).expect("table");
    let cell = table.cell(0, 0).expect("cell");
    assert_eq!(cell.content.text(), "Cell text");
    assert_eq!(
        cell.content.runs[0].font_family.as_deref(),
        Some("Serif"),
        "font family belongs to the typed cell run, not the table block's unused paragraph runs"
    );
    assert!(document.blocks[1].runs.is_empty());
    drop(document);

    app.invoke_undo();
    let undone = state
        .current
        .borrow()
        .loom_table_from_block(BLOCK)
        .expect("table");
    assert_eq!(undone.cell(0, 0).unwrap().content.runs[0].font_family, None);
    app.invoke_redo();
    let redone = state
        .current
        .borrow()
        .loom_table_from_block(BLOCK)
        .expect("table");
    assert_eq!(
        redone.cell(0, 0).unwrap().content.runs[0]
            .font_family
            .as_deref(),
        Some("Serif")
    );
}

#[test]
fn font_size_choice_styles_typed_table_runs_and_is_undoable() {
    let (app, state) = app_with_table(1, 1, &["Cell text"]);
    let start = cell_start(&state, 0, 0);
    select(&state, start, start + "Cell text".len());
    crate::wire_writer_shared_callbacks(&app, &state, None);
    crate::apply_state(&app, &state);
    assert_eq!(app.get_font_size_pt(), 12, "table text inherits 12 pt");

    app.invoke_increase_font_size();
    assert_eq!(
        app.get_font_size_pt(),
        13,
        "the size control reflects the cell run"
    );

    let document = state.current.borrow();
    let table = document.loom_table_from_block(BLOCK).expect("table");
    assert_eq!(
        table.cell(0, 0).unwrap().content.runs[0].font_size_pt,
        Some(13.0),
        "font size belongs to the typed cell run"
    );
    assert!(document.blocks[1].runs.is_empty());
    drop(document);

    app.invoke_undo();
    let undone = state
        .current
        .borrow()
        .loom_table_from_block(BLOCK)
        .expect("table");
    assert_eq!(
        undone.cell(0, 0).unwrap().content.runs[0].font_size_pt,
        None
    );
    app.invoke_redo();
    let redone = state
        .current
        .borrow()
        .loom_table_from_block(BLOCK)
        .expect("table");
    assert_eq!(
        redone.cell(0, 0).unwrap().content.runs[0].font_size_pt,
        Some(13.0)
    );
}

#[test]
fn selected_table_scripts_are_undoable_saved_and_recovery_recorded() {
    let (app, state) = app_with_table(1, 1, &["Cell text"]);
    let start = cell_start(&state, 0, 0);
    select(&state, start, start + "Cell text".len());
    crate::wire_writer_shared_callbacks(&app, &state, None);
    crate::apply_state(&app, &state);
    let format = app.global::<crate::WriterFormatState>();
    let writes_before = crate::recovery::WRITES.with(|writes| writes.get());

    format.invoke_toggle_superscript();
    let superscript = state
        .current
        .borrow()
        .loom_table_from_block(BLOCK)
        .unwrap()
        .cell(0, 0)
        .unwrap()
        .content
        .runs[0]
        .clone();
    assert!(superscript.superscript);
    assert!(!superscript.subscript);
    assert!(
        crate::recovery::WRITES.with(|writes| writes.get()) > writes_before,
        "a selected table script edit must be recorded for recovery"
    );
    let saved = loom_writer_core::save_document(&state.current.borrow()).expect("save document");
    let restored = loom_writer_core::load_document(&saved).expect("reload document");
    assert!(
        restored
            .loom_table_from_block(BLOCK)
            .unwrap()
            .cell(0, 0)
            .unwrap()
            .content
            .runs[0]
            .superscript
    );

    app.invoke_undo();
    let undone = state
        .current
        .borrow()
        .loom_table_from_block(BLOCK)
        .unwrap()
        .cell(0, 0)
        .unwrap()
        .content
        .runs[0]
        .clone();
    assert!(!undone.superscript && !undone.subscript);
    app.invoke_redo();
    format.invoke_toggle_subscript();
    let subscript = state
        .current
        .borrow()
        .loom_table_from_block(BLOCK)
        .unwrap()
        .cell(0, 0)
        .unwrap()
        .content
        .runs[0]
        .clone();
    assert!(!subscript.superscript);
    assert!(subscript.subscript);
    app.invoke_undo();
    let restored_super = state
        .current
        .borrow()
        .loom_table_from_block(BLOCK)
        .unwrap()
        .cell(0, 0)
        .unwrap()
        .content
        .runs[0]
        .clone();
    assert!(restored_super.superscript && !restored_super.subscript);
}

#[test]
fn pending_table_script_formats_only_new_text_and_survives_save_undo_and_recovery() {
    if std::env::var_os(PENDING_TABLE_SCRIPT_RECOVERY_MODE).is_none() {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let sequence = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "loom-writer-table-script-recovery-{}-{sequence}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).expect("create isolated table-script recovery root");
        let state_home = root.join("state");
        let state_home_env = if cfg!(windows) {
            "LOCALAPPDATA"
        } else if cfg!(target_os = "macos") {
            "HOME"
        } else {
            "XDG_STATE_HOME"
        };
        let output = std::process::Command::new(
            std::env::current_exe().expect("current app test executable"),
        )
        .args([
            "--exact",
            PENDING_TABLE_SCRIPT_RECOVERY_TEST,
            "--nocapture",
            "--test-threads=1",
        ])
        .env(PENDING_TABLE_SCRIPT_RECOVERY_MODE, "child")
        .env(state_home_env, &state_home)
        .output()
        .expect("run isolated table-script recovery child");
        let cleanup = std::fs::remove_dir_all(&root);
        assert!(
            output.status.success(),
            "isolated recovery child failed: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        cleanup.expect("remove isolated table-script recovery root");
        return;
    }

    crate::typing_style::clear();
    assert!(crate::recovery::initialize_editing_session(None)
        .expect("initialize isolated table-script recovery")
        .draft
        .is_none());
    let recovery_worker = crate::recovery::DeferredWriteTestHarness::install();
    let (app, state) = app_with_table(1, 1, &["AB"]);
    let before = state.current.borrow().editor_text();
    let caret = offset_in_cell(&state.current.borrow(), 0, 0, 2);
    place_caret(&state, caret);
    crate::wire_writer_shared_callbacks(&app, &state, None);
    crate::apply_state(&app, &state);
    let writes_before = crate::recovery::WRITES.with(|writes| writes.get());
    app.global::<crate::WriterFormatState>()
        .invoke_toggle_superscript();

    let mut typed = before.clone();
    typed.insert(caret, 'X');
    app.invoke_document_edited(typed.into(), (caret + 1) as i32, (caret + 1) as i32);

    let document = state.current.borrow();
    let table = document.loom_table_from_block(BLOCK).unwrap();
    let cell = table.cell(0, 0).unwrap();
    assert_eq!(cell.content.text(), "ABX");
    assert!(!cell.content.runs[0].superscript);
    let inserted = cell
        .content
        .runs
        .iter()
        .find(|run| run.text.contains('X'))
        .expect("new script run");
    assert_eq!(inserted.text, "X");
    assert!(inserted.superscript && !inserted.subscript);
    let saved = loom_writer_core::save_document(&document).expect("save document");
    let restored = loom_writer_core::load_document(&saved).expect("reload document");
    assert!(restored
        .loom_table_from_block(BLOCK)
        .unwrap()
        .cell(0, 0)
        .unwrap()
        .content
        .runs
        .iter()
        .any(|run| run.text == "X" && run.superscript));
    assert!(crate::recovery::DEFERRED.with(crate::recovery::DeferredWrite::is_stale));
    drop(document);
    crate::flush_deferred_recovery(&app, &state);
    assert_eq!(
        recovery_worker
            .started
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("deferred table-script recovery serializer starts"),
        0
    );
    recovery_worker.release_first();
    recovery_worker
        .completed
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("deferred table-script recovery serializer completes")();
    let recovered = crate::recovery::initialize_editing_session(None)
        .expect("reopen isolated recovery draft")
        .draft
        .expect("table script is restored from recovery")
        .document;
    assert!(
        recovered
            .loom_table_from_block(BLOCK)
            .unwrap()
            .cell(0, 0)
            .unwrap()
            .content
            .runs
            .iter()
            .any(|run| run.text == "X" && run.superscript),
        "the recovered canonical table run keeps the pending script style"
    );
    assert!(
        crate::recovery::WRITES.with(|writes| writes.get()) > writes_before,
        "typed table script text must enter the production recovery write path"
    );
    crate::recovery::discard_document_recovery().expect("clear isolated recovery draft");

    app.invoke_undo();
    assert_eq!(state.current.borrow().editor_text(), before);
    app.invoke_redo();
    assert!(state
        .current
        .borrow()
        .loom_table_from_block(BLOCK)
        .unwrap()
        .cell(0, 0)
        .unwrap()
        .content
        .runs
        .iter()
        .any(|run| run.text == "X" && run.superscript));
    crate::typing_style::clear();
}

#[test]
fn strikethrough_action_formats_typed_table_runs_and_is_undoable() {
    let (app, state) = app_with_table(1, 1, &["Cell text"]);
    let start = cell_start(&state, 0, 0);
    select(&state, start, start + "Cell text".len());
    crate::wire_writer_shared_callbacks(&app, &state, None);
    crate::apply_state(&app, &state);
    let writes_before = crate::recovery::WRITES.with(|writes| writes.get());

    app.invoke_toggle_strikethrough();
    let run = state
        .current
        .borrow()
        .loom_table_from_block(BLOCK)
        .unwrap()
        .cell(0, 0)
        .unwrap()
        .content
        .runs[0]
        .clone();
    assert!(run.strikethrough);
    assert!(
        crate::recovery::WRITES.with(|writes| writes.get()) > writes_before,
        "a selected table strike edit must be recorded for recovery"
    );
    let saved = loom_writer_core::save_document(&state.current.borrow()).expect("save document");
    let restored = loom_writer_core::load_document(&saved).expect("reload document");
    assert!(
        restored
            .loom_table_from_block(BLOCK)
            .unwrap()
            .cell(0, 0)
            .unwrap()
            .content
            .runs[0]
            .strikethrough
    );

    app.invoke_undo();
    assert!(
        !state
            .current
            .borrow()
            .loom_table_from_block(BLOCK)
            .unwrap()
            .cell(0, 0)
            .unwrap()
            .content
            .runs[0]
            .strikethrough
    );
    app.invoke_redo();
    assert!(
        state
            .current
            .borrow()
            .loom_table_from_block(BLOCK)
            .unwrap()
            .cell(0, 0)
            .unwrap()
            .content
            .runs[0]
            .strikethrough
    );
}

#[test]
fn pending_font_family_at_a_table_caret_styles_only_new_text() {
    crate::typing_style::clear();
    let (app, state) = app_with_table(1, 1, &["Cell text"]);
    let caret = offset_in_cell(&state.current.borrow(), 0, 0, "Cell text".len());
    place_caret(&state, caret);
    crate::wire_writer_shared_callbacks(&app, &state, None);
    crate::apply_state(&app, &state);

    crate::font_picker::apply_family(&app, &state, "Serif");
    let before = state.current.borrow().editor_text();
    let mut typed = before.clone();
    typed.insert(caret, 'X');
    app.invoke_document_edited(typed.into(), (caret + 1) as i32, (caret + 1) as i32);

    let document = state.current.borrow();
    let table = document.loom_table_from_block(BLOCK).expect("table");
    let cell = table.cell(0, 0).expect("cell");
    assert_eq!(cell.content.text(), "Cell textX");
    let inserted = cell
        .content
        .runs
        .iter()
        .find(|run| run.text.contains('X'))
        .expect("inserted run");
    assert_eq!(inserted.text, "X");
    assert_eq!(inserted.font_family.as_deref(), Some("Serif"));
    assert!(cell.content.runs[0].font_family.is_none());
    let saved = loom_writer_core::save_document(&document).expect("save document");
    let reloaded = loom_writer_core::load_document(&saved).expect("reload document");
    assert_eq!(
        reloaded
            .loom_table_from_block(BLOCK)
            .unwrap()
            .cell(0, 0)
            .unwrap()
            .content
            .runs
            .iter()
            .find(|run| run.text.contains('X'))
            .unwrap()
            .font_family
            .as_deref(),
        Some("Serif")
    );
    drop(document);

    app.invoke_undo();
    assert_eq!(state.current.borrow().editor_text(), before);
    app.invoke_redo();
    assert_eq!(
        state
            .current
            .borrow()
            .loom_table_from_block(BLOCK)
            .unwrap()
            .cell(0, 0)
            .unwrap()
            .content
            .runs
            .iter()
            .find(|run| run.text.contains('X'))
            .unwrap()
            .font_family
            .as_deref(),
        Some("Serif")
    );
    crate::typing_style::clear();
}

#[test]
fn pending_foreground_color_at_a_table_caret_styles_only_new_text() {
    crate::typing_style::clear();
    let (app, state) = app_with_table(1, 1, &["Cell text"]);
    let caret = offset_in_cell(&state.current.borrow(), 0, 0, "Cell text".len());
    place_caret(&state, caret);
    crate::wire_writer_shared_callbacks(&app, &state, None);
    crate::apply_state(&app, &state);
    let global = app.global::<crate::WriterFormatState>();
    global.invoke_open_foreground_color();
    global.set_color_input("#336699".into());
    global.invoke_apply_foreground_color();

    let before = state.current.borrow().editor_text();
    let mut typed = before.clone();
    typed.insert(caret, 'X');
    app.invoke_document_edited(typed.into(), (caret + 1) as i32, (caret + 1) as i32);

    let document = state.current.borrow();
    let table = document.loom_table_from_block(BLOCK).expect("table");
    let cell = table.cell(0, 0).expect("cell");
    let inserted = cell
        .content
        .runs
        .iter()
        .find(|run| run.text.contains('X'))
        .expect("inserted run");
    assert_eq!(inserted.text, "X");
    assert_eq!(inserted.color.as_deref(), Some("#336699FF"));
    assert!(cell.content.runs[0].color.is_none());
    let saved = loom_writer_core::save_document(&document).expect("save document");
    let reloaded = loom_writer_core::load_document(&saved).expect("reload document");
    assert_eq!(
        reloaded
            .loom_table_from_block(BLOCK)
            .unwrap()
            .cell(0, 0)
            .unwrap()
            .content
            .runs
            .iter()
            .find(|run| run.text.contains('X'))
            .unwrap()
            .color
            .as_deref(),
        Some("#336699FF")
    );
    drop(document);

    app.invoke_undo();
    assert_eq!(state.current.borrow().editor_text(), before);
    app.invoke_redo();
    assert_eq!(
        state
            .current
            .borrow()
            .loom_table_from_block(BLOCK)
            .unwrap()
            .cell(0, 0)
            .unwrap()
            .content
            .runs
            .iter()
            .find(|run| run.text.contains('X'))
            .unwrap()
            .color
            .as_deref(),
        Some("#336699FF")
    );
    crate::typing_style::clear();
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
