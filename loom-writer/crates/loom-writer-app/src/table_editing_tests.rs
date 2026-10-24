//! Tests for real table editing in Loom Writer (Wave 1 W1).

#[cfg(test)]
mod tests {
    use crate::actions_tests::test_state;
    use crate::*;
    use loom_writer_core::{WriterTable, TABLE_BLOCK_KIND};
    use std::rc::Rc;

    fn make_test_app() -> (WriterApp, Rc<GuiState>) {
        let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], [None]));
        let mut doc = WriterDocument::new("test-table", "Test Table");
        doc.replace_paragraphs("Initial text");
        let (app, state) = test_state(doc, dialogs);
        wire_writer_shared_callbacks(&app, &state, None);
        (app, state)
    }

    fn make_app_with_active_table_range(
        caret_row: usize,
        caret_col: usize,
    ) -> (WriterApp, Rc<GuiState>, u64) {
        table_actions::clear_table_cell_selection_range();
        let (app, state) = make_test_app();
        let mut document = WriterDocument::new("test-range", "Test Range");
        document.replace_paragraphs("Intro");
        document.insert_table_block(usize::MAX, 3, 3).unwrap();
        let block_id = document
            .blocks
            .iter()
            .find(|block| block.kind == TABLE_BLOCK_KIND)
            .expect("inserted table block")
            .id;
        let mut table = document.table_from_block(block_id).unwrap();
        for row in 0..3 {
            for col in 0..3 {
                table.set(row, col, format!("R{row}C{col}"));
            }
        }
        document
            .blocks
            .iter_mut()
            .find(|block| block.id == block_id)
            .unwrap()
            .set_table(table.to_loom_table());
        let caret = (0..document.editor_text().len())
            .find(|offset| {
                document.table_cell_at_offset(*offset).is_some_and(|cell| {
                    cell.block_id == block_id && cell.row == caret_row && cell.col == caret_col
                })
            })
            .expect("caret offset in requested table cell");
        document.set_selection(TextSelection::caret(caret));
        *state.current.borrow_mut() = document;
        table_actions::set_table_cell_selection_range(Some(table_actions::TableCellRange {
            block_id,
            start_row: 0,
            start_col: 0,
            end_row: 1,
            end_col: 1,
        }));
        apply_state(&app, &state);

        let rects = app.get_table_cell_rects();
        let selected = (0..rects.row_count())
            .filter_map(|index| rects.row_data(index))
            .filter(|rect| rect.selected)
            .count();
        assert_eq!(selected, 4, "the fixture starts with A1:B2 highlighted");
        assert_eq!(
            app.get_document_accessible_label().as_str(),
            "Table 3 by 3, cells A1:B2 selected",
            "the active range is announced before the action"
        );
        (app, state, block_id)
    }

    fn assert_active_range_cleared_and_copy_is_full_table(
        app: &WriterApp,
        state: &GuiState,
        expected_cell: (usize, usize),
        expected_text: &str,
    ) {
        assert_eq!(table_actions::get_table_cell_selection_range(), None);
        let document = state.current.borrow();
        let caret = document
            .table_cell_at_offset(document.selection().anchor)
            .expect("caret remains in the table");
        assert_eq!((caret.row, caret.col), expected_cell);
        drop(document);

        let rects = app.get_table_cell_rects();
        assert!(
            (0..rects.row_count())
                .filter_map(|index| rects.row_data(index))
                .all(|rect| !rect.selected),
            "the stale range is no longer highlighted"
        );
        assert_eq!(
            app.get_document_accessible_label().as_str(),
            format!(
                "Table 3 by 3, cell {}{}: {expected_text}",
                (b'A' + expected_cell.1 as u8) as char,
                expected_cell.0 + 1
            ),
            "the accessible label follows the caret cell"
        );

        // A collapsed caret copies nothing, so no stale A1:B2 range can be copied.
        loom_clipboard::set_mock_clipboard_enabled(true);
        loom_clipboard::write_clipboard(&loom_clipboard::ClipboardData {
            plain_text: Some("sentinel".into()),
            ..Default::default()
        })
        .expect("write the sentinel");
        clipboard_actions::copy_action(app, state);
        let copied = loom_clipboard::read_clipboard()
            .expect("read the clipboard")
            .plain_text;
        assert_eq!(
            copied.as_deref(),
            Some("sentinel"),
            "Copy with a collapsed caret copies nothing"
        );
        loom_clipboard::set_mock_clipboard_enabled(false);
    }

    fn assert_range_cleared_and_copy_has_dimensions(
        app: &WriterApp,
        state: &GuiState,
        expected_rows: usize,
        expected_cols: usize,
    ) {
        assert_eq!(table_actions::get_table_cell_selection_range(), None);
        let rects = app.get_table_cell_rects();
        assert!(
            (0..rects.row_count())
                .filter_map(|index| rects.row_data(index))
                .all(|rect| !rect.selected),
            "a successful table mutation clears the painted cell range"
        );

        // The whole updated table is copied once it is selected: a collapsed caret copies nothing.
        let table_id = state
            .current
            .borrow()
            .blocks
            .iter()
            .find(|block| block.kind == loom_writer_core::TABLE_BLOCK_KIND)
            .map(|block| block.id)
            .expect("the updated table");
        table_actions::set_table_cell_selection_range(Some(table_actions::TableCellRange {
            block_id: table_id,
            start_row: 0,
            start_col: 0,
            end_row: expected_rows - 1,
            end_col: expected_cols - 1,
        }));
        loom_clipboard::set_mock_clipboard_enabled(true);
        clipboard_actions::copy_action(app, state);
        let copied = loom_clipboard::read_clipboard()
            .expect("Copy writes the selected table")
            .to_table()
            .expect("Copy preserves the table clipboard format");
        table_actions::clear_table_cell_selection_range();
        assert_eq!(copied.row_count(), expected_rows);
        assert_eq!(copied.col_count(), expected_cols);
        loom_clipboard::set_mock_clipboard_enabled(false);
    }

    #[test]
    fn table_cell_range_clears_on_tab_before_highlight_announcement_and_copy() {
        // User action: Tab moves from A1 to B1 while A1:B2 is selected.
        // Current failure: A1:B2 stays highlighted, announced, and copied.
        // Required behavior: clear the virtual range and use the caret cell.
        let (app, state, _) = make_app_with_active_table_range(0, 0);

        app.invoke_insert_tab();

        assert_active_range_cleared_and_copy_is_full_table(&app, &state, (0, 1), "R0C1");
    }

    #[test]
    fn table_cell_range_clears_on_shift_tab_before_highlight_announcement_and_copy() {
        // User action: Shift+Tab moves from B1 to A1 while A1:B2 is selected.
        // Current failure: A1:B2 stays highlighted, announced, and copied.
        // Required behavior: clear the virtual range and use the caret cell.
        let (app, state, _) = make_app_with_active_table_range(0, 1);

        app.invoke_table_navigate_backwards();

        assert_active_range_cleared_and_copy_is_full_table(&app, &state, (0, 0), "R0C0");
    }

    #[test]
    fn table_cell_range_clears_on_text_edit_before_highlight_announcement_and_copy() {
        // User action: type in A1 while A1:B2 is selected.
        // Current failure: A1:B2 stays highlighted, announced, and copied.
        // Required behavior: clear the virtual range and use the edited table.
        let (app, state, _) = make_app_with_active_table_range(0, 0);
        let (text, caret) = {
            let document = state.current.borrow();
            (document.editor_text(), document.selection().focus)
        };
        let mut edited = text;
        edited.insert(caret, 'X');
        let next_caret = (caret + 1) as i32;

        app.invoke_document_edited(edited.into(), next_caret, next_caret);

        assert_active_range_cleared_and_copy_is_full_table(&app, &state, (0, 0), "XR0C0");
    }

    fn assert_collapsed_navigation_clears_range(key: slint::platform::Key, ctrl: bool) {
        let (app, state, _) = make_app_with_active_table_range(0, 0);
        let key: SharedString = key.into();

        let handled =
            table_actions::handle_table_key(&app, &state, key.as_str(), false, ctrl, false);

        assert!(!handled, "normal caret navigation remains with the editor");
        assert_active_range_cleared_and_copy_is_full_table(&app, &state, (0, 0), "R0C0");
    }

    #[test]
    fn table_cell_range_clears_on_page_up_navigation() {
        assert_collapsed_navigation_clears_range(slint::platform::Key::PageUp, false);
    }

    #[test]
    fn table_cell_range_clears_on_page_down_navigation() {
        assert_collapsed_navigation_clears_range(slint::platform::Key::PageDown, false);
    }

    #[test]
    fn table_cell_range_clears_on_home_end_and_word_navigation() {
        for (key, ctrl) in [
            (slint::platform::Key::LeftArrow, false),
            (slint::platform::Key::RightArrow, false),
            (slint::platform::Key::UpArrow, false),
            (slint::platform::Key::DownArrow, false),
            (slint::platform::Key::Home, false),
            (slint::platform::Key::End, false),
            (slint::platform::Key::LeftArrow, true),
            (slint::platform::Key::RightArrow, true),
            (slint::platform::Key::UpArrow, true),
            (slint::platform::Key::DownArrow, true),
            (slint::platform::Key::PageUp, true),
            (slint::platform::Key::PageDown, true),
            (slint::platform::Key::Home, true),
            (slint::platform::Key::End, true),
        ] {
            assert_collapsed_navigation_clears_range(key, ctrl);
        }
    }

    #[test]
    fn table_cell_range_clears_after_successful_row_insertion_above() {
        let (app, state, _) = make_app_with_active_table_range(0, 0);
        table_actions::table_action(&app, &state, "insert-row-above");
        assert_range_cleared_and_copy_has_dimensions(&app, &state, 4, 3);
    }

    #[test]
    fn table_cell_range_clears_after_successful_row_insertion_below() {
        let (app, state, _) = make_app_with_active_table_range(0, 0);
        table_actions::table_action(&app, &state, "insert-row-below");
        assert_range_cleared_and_copy_has_dimensions(&app, &state, 4, 3);
    }

    #[test]
    fn table_cell_range_clears_after_successful_column_insertion_left() {
        let (app, state, _) = make_app_with_active_table_range(0, 0);
        table_actions::table_action(&app, &state, "insert-col-left");
        assert_range_cleared_and_copy_has_dimensions(&app, &state, 3, 4);
    }

    #[test]
    fn table_cell_range_clears_after_successful_column_insertion_right() {
        let (app, state, _) = make_app_with_active_table_range(0, 0);
        table_actions::table_action(&app, &state, "insert-col-right");
        assert_range_cleared_and_copy_has_dimensions(&app, &state, 3, 4);
    }

    #[test]
    fn merging_a_range_extended_up_and_left_starts_at_its_top_left_cell() {
        let (app, state, block_id) = make_app_with_active_table_range(2, 2);
        table_actions::clear_table_cell_selection_range();

        let left: slint::SharedString = slint::platform::Key::LeftArrow.into();
        let up: slint::SharedString = slint::platform::Key::UpArrow.into();
        assert!(table_actions::handle_table_key(
            &app,
            &state,
            left.as_str(),
            true,
            false,
            false,
        ));
        assert!(table_actions::handle_table_key(
            &app,
            &state,
            up.as_str(),
            true,
            false,
            false,
        ));

        let range = table_actions::get_table_cell_selection_range()
            .expect("Shift+Left and Shift+Up select a 2x2 range from C3 to B2");
        assert_eq!(
            (
                range.block_id,
                range.start_row,
                range.start_col,
                range.end_row,
                range.end_col,
            ),
            (block_id, 2, 2, 1, 1)
        );

        table_actions::table_action(&app, &state, "merge-cells");

        let document = state.current.borrow();
        let table = document
            .table_from_block(block_id)
            .expect("the merged table remains in the document");
        assert_eq!(
            table.merges,
            vec![(1, 1, 2, 2)],
            "the selected B2:C3 rectangle merges from its top-left cell"
        );
        let caret = document
            .table_cell_at_offset(document.selection().anchor)
            .expect("the caret remains inside the merged table");
        assert_eq!(
            (caret.row, caret.col),
            (1, 1),
            "the caret stays in merged B2 instead of landing in A3 after the rows shrink"
        );
    }

    #[test]
    fn merging_a_single_virtual_cell_reports_unavailable_without_mutation() {
        let (app, state, block_id) = make_app_with_active_table_range(0, 0);
        let range = table_actions::TableCellRange {
            block_id,
            start_row: 0,
            start_col: 0,
            end_row: 0,
            end_col: 0,
        };
        table_actions::set_table_cell_selection_range(Some(range));
        apply_state(&app, &state);
        let before = state.current.borrow().clone();
        let undo_before = state.history.borrow().undo_len();
        let redo_before = state.history.borrow().redo_len();
        app.set_status_right("Ready".into());

        table_actions::table_action(&app, &state, "merge-cells");

        assert_eq!(
            *state.current.borrow(),
            before,
            "a one-cell merge does not create a false document change"
        );
        assert_eq!(state.history.borrow().undo_len(), undo_before);
        assert_eq!(state.history.borrow().redo_len(), redo_before);
        assert_eq!(table_actions::get_table_cell_selection_range(), Some(range));
        assert_eq!(
            app.get_status_right().as_str(),
            "Table action failed: Select at least two cells to merge",
            "the status explains why the selected range could not be merged"
        );
        table_actions::clear_table_cell_selection_range();
    }

    #[test]
    fn test_insert_table_with_dimensions_creates_table_and_places_caret() {
        let (app, state) = make_test_app();
        // Invoke custom table insertion: 4 rows x 5 columns
        app.invoke_insert_table_size(4, 5);

        let current = state.current.borrow();
        let table_block = current
            .blocks
            .iter()
            .find(|b| b.kind == TABLE_BLOCK_KIND)
            .expect("Table block must be inserted");
        let table = current
            .table_from_block(table_block.id)
            .expect("Must parse table");
        assert_eq!(table.rows.len(), 4, "Must have 4 rows");
        assert_eq!(table.columns(), 5, "Must have 5 columns");

        // Verify caret is inside the first cell
        let caret = current.selection().anchor;
        let loc = current
            .table_cell_at_offset(caret)
            .expect("Caret must be in table");
        assert_eq!(loc.row, 0);
        assert_eq!(loc.col, 0);
    }

    #[test]
    fn the_caret_follows_typed_text_inside_a_table_cell_and_takes_the_cells_colour() {
        let (app, state) = make_test_app();
        app.invoke_insert_table_size(2, 2);
        let caret_x = |app: &WriterApp| {
            app.get_selection_rects()
                .iter()
                .find(|r| r.caret)
                .map(|r| (r.x, r.has_color))
                .expect("a caret")
        };
        let (empty_x, header_coloured) = caret_x(&app);
        assert!(
            header_coloured,
            "a header cell's caret uses the cell's light text colour"
        );

        let text = state.current.borrow().editor_text();
        let at = state.current.borrow().selection().anchor;
        let mut typed = text.clone();
        typed.insert_str(at, "Region");
        let caret = (at + 6) as i32;
        app.invoke_document_edited(typed.into(), caret, caret);
        let (after_x, _) = caret_x(&app);
        assert!(
            after_x > empty_x + 20.0,
            "caret must move right with six typed letters: {empty_x} -> {after_x}"
        );
    }

    #[test]
    fn header_cell_text_uses_the_table_styles_light_colour_on_its_dark_fill() {
        let (app, state) = make_test_app();
        app.invoke_insert_table_size(2, 2);
        let text = state.current.borrow().editor_text();
        let caret = state.current.borrow().selection().anchor;
        let mut typed = text.clone();
        typed.insert_str(caret, "Item");
        let caret = (caret + 4) as i32;
        app.invoke_document_edited(typed.into(), caret, caret);

        let blocks: Vec<_> = app.get_render_blocks().iter().collect();
        let first = blocks
            .iter()
            .filter(|b| b.has_color)
            .min_by(|a, b| a.y.partial_cmp(&b.y).unwrap())
            .expect("a header cell with its own text colour");
        let c = first.color;
        let luminance = 0.2126 * f32::from(c.red()) / 255.0
            + 0.7152 * f32::from(c.green()) / 255.0
            + 0.0722 * f32::from(c.blue()) / 255.0;
        assert!(
            luminance > 0.8,
            "header text must be light on the dark header fill, got {c:?}"
        );
    }

    #[test]
    fn test_table_cell_tab_and_arrow_navigation_in_app() {
        let (app, state) = make_test_app();
        app.invoke_insert_table_size(2, 3);

        // First cell (0, 0)
        let current = state.current.borrow();
        let loc = current
            .table_cell_at_offset(current.selection().anchor)
            .unwrap();
        assert_eq!((loc.row, loc.col), (0, 0));
        drop(current);

        // Tab to next cell (0, 1)
        app.invoke_insert_tab();
        let current = state.current.borrow();
        let loc = current
            .table_cell_at_offset(current.selection().anchor)
            .unwrap();
        assert_eq!((loc.row, loc.col), (0, 1));
        drop(current);

        // Tab to (0, 2)
        app.invoke_insert_tab();
        let current = state.current.borrow();
        let loc = current
            .table_cell_at_offset(current.selection().anchor)
            .unwrap();
        assert_eq!((loc.row, loc.col), (0, 2));
        drop(current);

        // Tab to (1, 0)
        app.invoke_insert_tab();
        let current = state.current.borrow();
        let loc = current
            .table_cell_at_offset(current.selection().anchor)
            .unwrap();
        assert_eq!((loc.row, loc.col), (1, 0));
        drop(current);

        // Backtab (Shift+Tab) to (0, 2)
        app.invoke_table_navigate_backwards();
        let current = state.current.borrow();
        let loc = current
            .table_cell_at_offset(current.selection().anchor)
            .unwrap();
        assert_eq!((loc.row, loc.col), (0, 2));
        drop(current);

        // Tab three times to reach last cell (1, 2)
        app.invoke_insert_tab();
        app.invoke_insert_tab();
        app.invoke_insert_tab();
        let current = state.current.borrow();
        let loc = current
            .table_cell_at_offset(current.selection().anchor)
            .unwrap();
        assert_eq!((loc.row, loc.col), (1, 2));
        drop(current);

        // Tab at the last cell appends a row! Now 3 rows, caret at (2, 0)
        app.invoke_insert_tab();
        let current = state.current.borrow();
        let table = current.table_from_block(current.blocks[1].id).unwrap();
        assert_eq!(table.rows.len(), 3, "Table must now have 3 rows");
        let loc = current
            .table_cell_at_offset(current.selection().anchor)
            .unwrap();
        assert_eq!((loc.row, loc.col), (2, 0));
    }

    #[test]
    fn test_table_mutations_and_undo_redo_in_app() {
        let (app, state) = make_test_app();
        app.invoke_insert_table_size(3, 3);

        // Insert row below
        app.invoke_table_action("insert-row-below".into());
        let current = state.current.borrow();
        let table = current.table_from_block(current.blocks[1].id).unwrap();
        assert_eq!(table.rows.len(), 4, "Must have 4 rows after insert");
        drop(current);

        // Undo row insertion
        app.invoke_undo();
        let current = state.current.borrow();
        let table = current.table_from_block(current.blocks[1].id).unwrap();
        assert_eq!(table.rows.len(), 3, "Must have 3 rows after undo");
        drop(current);

        // Insert column right
        app.invoke_table_action("insert-col-right".into());
        let current = state.current.borrow();
        let table = current.table_from_block(current.blocks[1].id).unwrap();
        assert_eq!(table.columns(), 4, "Must have 4 columns after insert");
        drop(current);

        // Delete row
        app.invoke_table_action("delete-row".into());
        let current = state.current.borrow();
        let table = current.table_from_block(current.blocks[1].id).unwrap();
        assert_eq!(table.rows.len(), 2, "Must have 2 rows after delete row");
        drop(current);

        // Delete column
        app.invoke_table_action("delete-col".into());
        let current = state.current.borrow();
        let table = current.table_from_block(current.blocks[1].id).unwrap();
        assert_eq!(table.columns(), 3, "Must have 3 columns after delete col");
    }

    #[test]
    fn deleting_the_last_row_clamps_caret_and_clears_range_only_after_success() {
        table_actions::clear_table_cell_selection_range();
        let (app, state) = make_test_app();
        app.invoke_insert_table_size(3, 2);

        let block_id = {
            let mut document = state.current.borrow().clone();
            let block_id = document
                .blocks
                .iter()
                .find(|block| block.kind == TABLE_BLOCK_KIND)
                .expect("inserted table")
                .id;
            let last_row_offset = (0..document.editor_text().len())
                .find(|offset| {
                    document
                        .table_cell_at_offset(*offset)
                        .is_some_and(|cell| cell.block_id == block_id && cell.row == 2)
                })
                .expect("an offset in the last table row");
            document.set_selection(TextSelection::caret(last_row_offset));
            *state.current.borrow_mut() = document;
            block_id
        };
        apply_state(&app, &state);
        table_actions::set_table_cell_selection_range(Some(table_actions::TableCellRange {
            block_id,
            start_row: 2,
            start_col: 0,
            end_row: 2,
            end_col: 1,
        }));

        app.invoke_table_action("delete-row".into());
        {
            let document = state.current.borrow();
            let table = document
                .table_from_block(block_id)
                .expect("remaining table");
            assert_eq!(table.rows.len(), 2);
            let caret = document
                .table_cell_at_offset(document.selection().anchor)
                .expect("caret remains in table");
            assert_eq!((caret.row, caret.col), (1, 0));
        }
        assert_eq!(table_actions::get_table_cell_selection_range(), None);

        {
            let mut document = state.current.borrow().clone();
            let offset = (0..document.editor_text().len())
                .find(|offset| {
                    document
                        .table_cell_at_offset(*offset)
                        .is_some_and(|cell| cell.block_id == block_id && cell.row == 1)
                })
                .expect("offset in the remaining last row");
            document.set_selection(TextSelection::caret(offset));
            *state.current.borrow_mut() = document;
        };
        apply_state(&app, &state);
        table_actions::set_table_cell_selection_range(Some(table_actions::TableCellRange {
            block_id,
            start_row: 1,
            start_col: 0,
            end_row: 1,
            end_col: 1,
        }));
        app.invoke_table_action("delete-row".into());
        {
            let document = state.current.borrow();
            assert_eq!(
                document.table_from_block(block_id).unwrap().rows.len(),
                1,
                "deleting the final position in a multi-row table leaves its one-row minimum"
            );
            let caret = document
                .table_cell_at_offset(document.selection().anchor)
                .expect("caret remains in the sole row");
            assert_eq!((caret.row, caret.col), (0, 0));
        }
        assert_eq!(table_actions::get_table_cell_selection_range(), None);

        let sole_row_offset = {
            let document = state.current.borrow();
            (0..document.editor_text().len())
                .find(|offset| {
                    document
                        .table_cell_at_offset(*offset)
                        .is_some_and(|cell| cell.block_id == block_id && cell.row == 0)
                })
                .expect("offset in the sole row")
        };
        state
            .current
            .borrow_mut()
            .set_selection(TextSelection::caret(sole_row_offset));
        apply_state(&app, &state);
        let sole_row_range = table_actions::TableCellRange {
            block_id,
            start_row: 0,
            start_col: 0,
            end_row: 0,
            end_col: 1,
        };
        table_actions::set_table_cell_selection_range(Some(sole_row_range));
        let before_noop = state.current.borrow().clone();
        let undo_depth = state.history.borrow().undo_len();

        app.invoke_table_action("delete-row".into());

        assert_eq!(*state.current.borrow(), before_noop);
        assert_eq!(state.history.borrow().undo_len(), undo_depth);
        assert_eq!(
            table_actions::get_table_cell_selection_range(),
            Some(sole_row_range)
        );
        table_actions::clear_table_cell_selection_range();
    }

    #[test]
    fn test_table_style_and_options_in_inspector() {
        let (app, state) = make_test_app();
        app.invoke_insert_table_size(3, 3);

        // Change table style to Dark 1
        app.invoke_set_table_style("table-style-dark-1".into());
        let current = state.current.borrow();
        let loom_table = current
            .table_from_block(current.blocks[1].id)
            .unwrap()
            .to_loom_table();
        assert_eq!(loom_table.style_id, "table-style-dark-1");
        drop(current);

        // Toggle banded rows
        app.invoke_toggle_table_option("banded_rows".into());
        let current = state.current.borrow();
        let loom_table = current
            .table_from_block(current.blocks[1].id)
            .unwrap()
            .to_loom_table();
        assert!(!loom_table.banded_rows, "Banded rows should be toggled off");
        drop(current);

        // Undo style change
        app.invoke_undo();
        let current = state.current.borrow();
        let loom_table = current
            .table_from_block(current.blocks[1].id)
            .unwrap()
            .to_loom_table();
        assert!(
            loom_table.banded_rows,
            "Banded rows should be restored to on"
        );
    }

    #[test]
    fn test_table_accessibility_semantics() {
        let (app, _state) = make_test_app();
        app.invoke_insert_table_size(3, 4);

        // Move to cell (1, 1) which is B2
        app.invoke_insert_tab(); // (0, 1)
        app.invoke_insert_tab(); // (0, 2)
        app.invoke_insert_tab(); // (0, 3)
        app.invoke_insert_tab(); // (1, 0)
        app.invoke_insert_tab(); // (1, 1)

        let a11y_name = app.get_document_accessible_label();
        assert!(
            a11y_name.contains("Table 3 by 4") && a11y_name.contains("cell B2"),
            "Accessible label must describe table dimensions and cell coordinates: got '{a11y_name}'"
        );
    }

    #[test]
    fn test_docx_export_contains_real_wtbl_and_shading() {
        let (app, state) = make_test_app();
        app.invoke_insert_table_size(3, 3);
        app.invoke_set_table_style("table-style-medium-1".into());

        // Populate cell text and merge cells (2, 0) and (2, 1)
        let tbl_id = state.current.borrow().blocks[1].id;
        let mut tbl = state.current.borrow().table_from_block(tbl_id).unwrap();
        tbl.set(0, 0, "Product");
        tbl.set(0, 1, "Qty");
        tbl.set(0, 2, "Price");
        tbl.set(1, 0, "Apples");
        tbl.set(1, 1, "50");
        tbl.set(1, 2, "$1.50");
        tbl.set(2, 0, "SpecialOffer");
        tbl.set(2, 2, "$0.99");
        let mut loom_tbl = tbl.to_loom_table();
        loom_tbl.merge_cells(2, 0, 1, 2).unwrap();
        let updated = WriterTable::from_loom_table(&loom_tbl);
        if let Some(b) = state
            .current
            .borrow_mut()
            .blocks
            .iter_mut()
            .find(|b| b.id == tbl_id)
        {
            b.set_table(updated.to_loom_table());
        }

        // Save to target for external COM verification using the APP's export path
        let out_dir = std::path::Path::new("target");
        std::fs::create_dir_all(out_dir).ok();
        let out_path = out_dir.join("w1-table-verification.docx");
        let (_path, export) =
            crate::docx_export::export_docx_file(&out_path, &state.current.borrow())
                .expect("Must export docx through app export path");

        let archive = loom_package::zip::PackageArchive::from_bytes(&export.bytes)
            .expect("Must parse docx zip");
        let doc_xml = String::from_utf8(
            archive
                .get("word/document.xml")
                .expect("document.xml")
                .to_vec(),
        )
        .unwrap();

        assert!(doc_xml.contains("<w:tbl>"), "DOCX must contain <w:tbl>");
        assert!(
            doc_xml.contains("<w:tblGrid>"),
            "DOCX must contain <w:tblGrid>"
        );
        assert!(
            doc_xml.contains("<w:gridCol"),
            "DOCX must contain grid columns"
        );
        assert!(doc_xml.contains("<w:shd"), "DOCX must contain cell shading");
        assert!(
            doc_xml.contains("<w:tblHeader/>"),
            "DOCX must contain header row marker"
        );
        assert!(
            doc_xml.contains("<w:gridSpan w:val=\"2\"/>"),
            "DOCX must contain horizontal merge gridSpan=2"
        );
    }

    #[test]
    fn test_property_zero_text_loss_around_tables() {
        let (_app, state) = make_test_app();
        let mut doc = state.current.borrow().clone();
        doc.replace_paragraphs("Introduction paragraph with unique words Alpha Beta Gamma.");
        doc.insert_table_block(usize::MAX, 2, 2).unwrap();
        // Populate table cells with unique words
        let tbl_id = doc
            .blocks
            .iter()
            .find(|b| b.kind == TABLE_BLOCK_KIND)
            .unwrap()
            .id;
        let mut tbl = doc.table_from_block(tbl_id).unwrap();
        tbl.set(0, 0, "CellOneOne");
        tbl.set(0, 1, "CellOneTwo");
        tbl.set(1, 0, "CellTwoOne");
        tbl.set(1, 1, "CellTwoTwo");
        if let Some(b) = doc.blocks.iter_mut().find(|b| b.id == tbl_id) {
            b.set_table(tbl.to_loom_table());
        }
        // Add paragraph after table
        doc.blocks.push(loom_writer_core::RichBlock::new(
            99,
            "paragraph",
            "Conclusion paragraph with Delta Epsilon Zeta.",
        ));

        let test_words = [
            "Alpha",
            "Beta",
            "Gamma",
            "CellOneOne",
            "CellOneTwo",
            "CellTwoOne",
            "CellTwoTwo",
            "Delta",
            "Epsilon",
            "Zeta",
        ];

        // 1. DOCX export
        let docx = loom_writer_core::export_docx(&doc).expect("Must export docx");
        let archive =
            loom_package::zip::PackageArchive::from_bytes(&docx.bytes).expect("Must parse docx");
        let docx_xml = String::from_utf8(
            archive
                .get("word/document.xml")
                .expect("document.xml")
                .to_vec(),
        )
        .unwrap();
        for word in &test_words {
            assert!(
                docx_xml.contains(word),
                "DOCX document.xml must contain word '{word}'"
            );
        }

        // 2. Markdown export
        let md = doc.to_markdown();
        for word in &test_words {
            assert!(
                md.contains(word),
                "Markdown export must contain word '{word}'"
            );
        }

        // 3. PDF export
        let pdf_bytes = loom_writer_core::export_pdf(&doc);
        let pdf_readable = loom_pdf::inspect::readable_content(&pdf_bytes).expect("readable PDF");
        for word in &test_words {
            assert!(
                pdf_readable.contains(word),
                "PDF export must contain word '{word}'"
            );
        }
    }

    #[test]
    fn test_typing_latency_in_100x20_table() {
        let (app, state) = make_test_app();
        app.invoke_insert_table_size(100, 20);

        let initial_text = state.current.borrow().editor_text();
        let caret = state.current.borrow().selection().anchor;

        let mut latencies_ms = Vec::with_capacity(20);
        let mut cur_text = initial_text;
        let mut cur_caret = caret;

        for i in 0..20 {
            let ch = (b'a' + (i % 26) as u8) as char;
            let mut next_text = cur_text.clone();
            next_text.insert(cur_caret, ch);
            cur_caret += 1;

            let start = std::time::Instant::now();
            app.invoke_document_edited(
                next_text.clone().into(),
                cur_caret as i32,
                cur_caret as i32,
            );
            let elapsed = start.elapsed();
            latencies_ms.push(elapsed.as_secs_f64() * 1000.0);
            cur_text = next_text;
        }

        latencies_ms.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let p95_idx = ((latencies_ms.len() as f64 * 0.95).ceil() as usize).saturating_sub(1);
        let p95 = latencies_ms[p95_idx];
        println!("100x20 table typing latency: p95 = {:.2} ms", p95);
        assert!(
            p95 < 250.0,
            "Typing latency in debug build must stay responsive: got {p95} ms"
        );
    }

    #[test]
    fn test_t4b_writer_table_shortcuts_menu_cell_selection_copy() {
        loom_clipboard::set_mock_clipboard_enabled(true);
        let (app, state) = make_test_app();
        app.invoke_insert_table_size(3, 3);

        // 1. Menu & Palette commands for row/col insert/delete & merge/split
        assert!(
            dispatch_command(&app, "table.insert_row_above"),
            "dispatch table.insert_row_above"
        );
        {
            let current = state.current.borrow();
            let table = current.table_from_block(current.blocks[1].id).unwrap();
            assert_eq!(table.rows.len(), 4, "Insert row above increased rows to 4");
        }

        assert!(
            dispatch_command(&app, "table.insert_row_below"),
            "dispatch table.insert_row_below"
        );
        {
            let current = state.current.borrow();
            let table = current.table_from_block(current.blocks[1].id).unwrap();
            assert_eq!(table.rows.len(), 5, "Insert row below increased rows to 5");
        }

        assert!(
            dispatch_command(&app, "table.delete_row"),
            "dispatch table.delete_row"
        );
        {
            let current = state.current.borrow();
            let table = current.table_from_block(current.blocks[1].id).unwrap();
            assert_eq!(table.rows.len(), 4, "Delete row decreased rows to 4");
        }

        assert!(
            dispatch_command(&app, "table.insert_col_left"),
            "dispatch table.insert_col_left"
        );
        {
            let current = state.current.borrow();
            let table = current.table_from_block(current.blocks[1].id).unwrap();
            assert_eq!(table.columns(), 4, "Insert col left increased cols to 4");
        }

        assert!(
            dispatch_command(&app, "table.insert_col_right"),
            "dispatch table.insert_col_right"
        );
        {
            let current = state.current.borrow();
            let table = current.table_from_block(current.blocks[1].id).unwrap();
            assert_eq!(table.columns(), 5, "Insert col right increased cols to 5");
        }

        assert!(
            dispatch_command(&app, "table.delete_column"),
            "dispatch table.delete_column"
        );
        {
            let current = state.current.borrow();
            let table = current.table_from_block(current.blocks[1].id).unwrap();
            assert_eq!(table.columns(), 4, "Delete col decreased cols to 4");
        }

        assert!(
            dispatch_command(&app, "table.merge_cells"),
            "dispatch table.merge_cells"
        );
        assert!(
            dispatch_command(&app, "table.split_cell"),
            "dispatch table.split_cell"
        );

        // Supported menu commands
        for cmd in [
            "edit.cut",
            "edit.copy",
            "edit.paste",
            "table.insert_row_above",
            "table.insert_row_below",
            "table.insert_col_left",
            "table.insert_col_right",
            "table.delete_row",
            "table.delete_column",
            "table.merge_cells",
            "table.split_cell",
        ] {
            assert!(
                local_menu::SUPPORTED_COMMANDS.contains(&cmd),
                "local_menu::SUPPORTED_COMMANDS must contain {cmd}"
            );
        }

        // Palette commands
        let palette = master_palette(&app);
        assert!(palette
            .iter()
            .any(|c| c.id == "table.insert_row_above" || c.id == "writer.table.insert_row_above"));
        assert!(palette
            .iter()
            .any(|c| c.id == "table.delete_row" || c.id == "writer.table.delete_row"));
        assert!(palette
            .iter()
            .any(|c| c.id == "table.merge_cells" || c.id == "writer.table.merge_cells"));

        // 2. Keyboard shortcuts handling
        let up_key: slint::SharedString = slint::platform::Key::UpArrow.into();
        assert!(table_actions::handle_table_key(
            &app,
            &state,
            up_key.as_str(),
            false,
            true,
            true
        )); // Ctrl+Alt+Up

        // 3. Populate a 3x3 table with known values for cell range selection & copy
        let mut doc = WriterDocument::new("test-range", "Test Range");
        doc.replace_paragraphs("Intro");
        doc.insert_table_block(usize::MAX, 3, 3).unwrap();
        let tbl_id = doc
            .blocks
            .iter()
            .find(|b| b.kind == TABLE_BLOCK_KIND)
            .unwrap()
            .id;
        let mut tbl = doc.table_from_block(tbl_id).unwrap();
        for r in 0..3 {
            for c in 0..3 {
                tbl.set(r, c, format!("R{r}C{c}"));
            }
        }
        if let Some(b) = doc.blocks.iter_mut().find(|b| b.id == tbl_id) {
            b.set_table(tbl.to_loom_table());
        }
        let first_cell_start = doc
            .table_cell_at_offset(doc.blocks[0].text.len_bytes() + 3)
            .unwrap()
            .cell_start;
        doc.set_selection(loom_writer_core::TextSelection::caret(first_cell_start));
        *state.current.borrow_mut() = doc;
        apply_state(&app, &state);

        // 4. Shift+Right to select R0C0:R0C1, then Shift+Down to select R0C0:R1C1 (2x2 sub-table)
        let right_key: slint::SharedString = slint::platform::Key::RightArrow.into();
        let down_key: slint::SharedString = slint::platform::Key::DownArrow.into();
        assert!(table_actions::handle_table_key(
            &app,
            &state,
            right_key.as_str(),
            true,
            false,
            false
        ));
        assert!(table_actions::handle_table_key(
            &app,
            &state,
            down_key.as_str(),
            true,
            false,
            false
        ));

        let sel_range = table_actions::get_table_cell_selection_range()
            .expect("Must have active table cell selection range");
        assert_eq!((sel_range.start_row, sel_range.start_col), (0, 0));
        assert_eq!((sel_range.end_row, sel_range.end_col), (1, 1));

        // 5. Copy selected cells only
        clipboard_actions::copy_action(&app, &state);
        let clip = loom_clipboard::read_clipboard().expect("Must read clipboard");
        let sub_tbl = clip.to_table().expect("Clipboard must contain table");
        assert_eq!(
            sub_tbl.row_count(),
            2,
            "Copied table must have exactly 2 rows"
        );
        assert_eq!(
            sub_tbl.col_count(),
            2,
            "Copied table must have exactly 2 columns"
        );
        assert_eq!(sub_tbl.cell_text(0, 0).as_deref(), Some("R0C0"));
        assert_eq!(sub_tbl.cell_text(0, 1).as_deref(), Some("R0C1"));
        assert_eq!(sub_tbl.cell_text(1, 0).as_deref(), Some("R1C0"));
        assert_eq!(sub_tbl.cell_text(1, 1).as_deref(), Some("R1C1"));
        assert!(
            !clip.tsv.as_ref().unwrap().contains("R2C2"),
            "Must NOT contain unselected cells like R2C2"
        );
        loom_clipboard::set_mock_clipboard_enabled(false);
    }

    /// The status line says what the table action did, in words. It never
    /// shows the internal action id the menu and palette dispatched.
    #[test]
    fn structural_table_actions_report_what_changed_in_words_not_ids() {
        for (action, expected) in [
            ("insert-row-above", "Inserted a row above"),
            ("insert-row-below", "Inserted a row below"),
            ("insert-col-left", "Inserted a column to the left"),
            ("insert-col-right", "Inserted a column to the right"),
            ("delete-row", "Deleted the row"),
            ("delete-col", "Deleted the column"),
        ] {
            let (app, state, _) = make_app_with_active_table_range(1, 1);
            table_actions::clear_table_cell_selection_range();
            app.set_status_right("Ready".into());
            table_actions::table_action(&app, &state, action);
            assert_eq!(
                app.get_status_right().as_str(),
                expected,
                "the status after {action}"
            );
        }
    }

    #[test]
    fn merging_the_selected_cells_reports_it_in_words() {
        let (app, state, _) = make_app_with_active_table_range(0, 0);
        table_actions::table_action(&app, &state, "merge-cells");
        assert_eq!(app.get_status_right().as_str(), "Merged the selected cells");
    }

    #[test]
    fn table_style_and_option_changes_name_the_style_and_option_in_words() {
        let (app, _state) = make_test_app();
        app.invoke_insert_table_size(3, 3);
        app.invoke_set_table_style("table-style-dark-1".into());
        assert_eq!(
            app.get_status_right().as_str(),
            "Table style set: Table Style Dark 1",
            "the style is named as the style gallery names it"
        );
        app.invoke_toggle_table_option("banded_rows".into());
        assert_eq!(
            app.get_status_right().as_str(),
            "Table option toggled: Banded rows",
            "the option is named as the inspector labels it"
        );
    }

    /// Insert Table opens no dialog, so its name has no ellipsis, in the palette
    /// and in the menu alike.
    #[test]
    fn the_table_insert_command_is_named_insert_table_without_an_ellipsis() {
        let (app, _state) = make_test_app();
        let row = master_palette(&app)
            .into_iter()
            .find(|command| command.id == "writer.table.insert")
            .expect("the palette offers the table insert");
        assert_eq!(row.label, "Insert Table");
        let spec = table_actions::table_command_specs()
            .into_iter()
            .find(|spec| spec.id.as_str() == "writer.table.insert")
            .expect("the registry lists the table insert");
        assert_eq!(spec.label, "Insert Table");
        let menu = local_menu::writer_menu_bar();
        assert_eq!(
            menu.find_item("insert.table").and_then(|item| item.label()),
            Some("Insert Table"),
            "the Table menu item opens nothing and is not marked with an ellipsis"
        );
    }
}
