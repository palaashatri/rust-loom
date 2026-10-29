//! Real table editing actions and callbacks for Loom Writer (Wave 1).

use loom_writer_core::{TextSelection, WriterDocument};
use slint::{ComponentHandle, SharedString};
use std::rc::Rc;

use crate::{apply_state, apply_with_history, GuiState, HistoryKind, WriterApp};

#[path = "break_keys.rs"]
pub(crate) mod break_keys;

#[path = "table_format_actions.rs"]
pub(crate) mod table_format_actions;

/// Wires table-related callbacks into the Slint WriterApp.
pub(crate) fn wire(app: &WriterApp, state: &Rc<GuiState>) {
    crate::list_level_actions::wire(state);
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_insert_table_size(move |rows, cols| {
            let Some(app) = app_ref.upgrade() else { return };
            insert_table_size(&app, &state, rows.max(1) as usize, cols.max(1) as usize);
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_table_action(move |act| {
            let Some(app) = app_ref.upgrade() else { return };
            table_action(&app, &state, act.as_str());
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_table_navigate_backwards(move || {
            let Some(app) = app_ref.upgrade() else { return };
            table_navigate_backwards(&app, &state);
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_set_table_style(move |style_id| {
            let Some(app) = app_ref.upgrade() else { return };
            set_table_style(&app, &state, style_id.as_str());
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_toggle_table_option(move |opt| {
            let Some(app) = app_ref.upgrade() else { return };
            toggle_table_option(&app, &state, opt.as_str());
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_handle_table_key(move |key, shift, ctrl, alt| {
            let Some(app) = app_ref.upgrade() else {
                return false;
            };
            if crate::picture_drag::handle_key(&app, &state, key.as_str(), shift, ctrl, alt) {
                return true;
            }
            if crate::caret_keys::handle(&app, &state, key.as_str(), shift, ctrl, alt) {
                return true;
            }
            if break_keys::handle(&app, &state, key.as_str(), shift, ctrl, alt)
                || crate::list_keys::handle(&app, &state, key.as_str(), shift, ctrl, alt)
            {
                return true;
            }
            handle_table_key(&app, &state, key.as_str(), shift, ctrl, alt)
        });
    }
}

/// Inserts a new table block of given dimensions at the current caret.
pub(crate) fn insert_table_size(app: &WriterApp, state: &GuiState, rows: usize, cols: usize) {
    let mut next = state.current.borrow().clone();
    let caret = next.selection().anchor;
    match next.insert_table_block(caret, rows, cols) {
        Ok(_) => {
            if !apply_with_history(app, state, next, HistoryKind::DocumentAction) {
                return;
            }
            app.set_status_right(SharedString::from(format!(
                "Table inserted ({rows}x{cols})"
            )));
        }
        Err(err) => {
            app.set_status_right(SharedString::from(format!("Table insertion failed: {err}")));
        }
    }
}

/// Dispatches a structural table action (insert/delete row/col, merge, split).
pub(crate) fn table_action(app: &WriterApp, state: &GuiState, action: &str) {
    if let Some(index) = table_format_actions::column_alignment_index(action) {
        table_format_actions::align_table_columns_for_index(app, state, index);
        return;
    }
    let caret = state.current.borrow().selection().anchor;
    let mut next = state.current.borrow().clone();
    let result = match action {
        "insert-row-above" => next.insert_table_row_at_offset(caret, false),
        "insert-row-below" => next.insert_table_row_at_offset(caret, true),
        "insert-col-left" => next.insert_table_column_at_offset(caret, false),
        "insert-col-right" => next.insert_table_column_at_offset(caret, true),
        "delete-row" => next.delete_table_row_at_offset(caret),
        "delete-col" => next.delete_table_column_at_offset(caret),
        "merge-cells" => {
            if let Some(range) = get_table_cell_selection_range() {
                let min_r = range.start_row.min(range.end_row);
                let max_r = range.start_row.max(range.end_row);
                let min_c = range.start_col.min(range.end_col);
                let max_c = range.start_col.max(range.end_col);
                let row_span = max_r - min_r + 1;
                let col_span = max_c - min_c + 1;
                let merge_offset = next
                    .blocks
                    .iter()
                    .position(|block| block.id == range.block_id)
                    .and_then(|block_index| {
                        let cell_start = next
                            .table_cell_ranges(range.block_id)?
                            .get(min_r)
                            .and_then(|row| row.get(min_c))
                            .map(|(start, _)| *start)?;
                        let block_start = next
                            .blocks
                            .iter()
                            .take(block_index)
                            .map(|block| block.text.len_bytes() + 1)
                            .sum::<usize>();
                        Some(block_start + cell_start)
                    });
                match merge_offset {
                    Some(_) if row_span == 1 && col_span == 1 => {
                        Err("Select at least two cells to merge".to_string())
                    }
                    Some(offset) => next
                        .merge_cells_at_offset(offset, row_span, col_span)
                        .map(|_| offset),
                    None => Err("Selected table cells no longer exist".to_string()),
                }
            } else {
                next.merge_cells_at_offset(caret, 1, 2).map(|_| caret)
            }
        }
        "split-cell" => next.split_cell_at_offset(caret).map(|_| caret),
        _ => return,
    };
    match result {
        Ok(new_caret) => {
            next.set_selection(TextSelection::caret(new_caret));
            if !crate::apply_table_action_with_history(
                app,
                state,
                next,
                HistoryKind::DocumentAction,
            ) {
                return;
            }
            app.set_status_right(SharedString::from(table_action_status(action)));
        }
        Err(err) => {
            app.set_status_right(SharedString::from(format!("Table action failed: {err}")));
        }
    }
}

/// The status line's words for a structural table action, given its dispatch id.
fn table_action_status(action: &str) -> &'static str {
    match action {
        "insert-row-above" => "Inserted a row above",
        "insert-row-below" => "Inserted a row below",
        "insert-col-left" => "Inserted a column to the left",
        "insert-col-right" => "Inserted a column to the right",
        "delete-row" => "Deleted the row",
        "delete-col" => "Deleted the column",
        "merge-cells" => "Merged the selected cells",
        "split-cell" => "Split the cell",
        _ => "Table updated",
    }
}

/// The inspector's name for a table option id.
fn table_option_label(option: &str) -> &'static str {
    match option {
        "header_row" => "Header row",
        "banded_rows" => "Banded rows",
        "first_column" => "First column",
        _ => "Table option",
    }
}

/// Navigates backwards (Shift+Tab) within a table.
pub(crate) fn table_navigate_backwards(app: &WriterApp, state: &GuiState) {
    let caret = state.current.borrow().selection().anchor;
    let mut next = state.current.borrow().clone();
    if let Ok(new_caret) = next.tab_navigate_table(caret, true) {
        clear_table_cell_selection_range();
        state
            .current
            .borrow_mut()
            .set_selection(TextSelection::caret(new_caret));
        apply_state(app, state);
    }
}

/// Sets the table style for the table enclosing caret.
pub(crate) fn set_table_style(app: &WriterApp, state: &GuiState, style_id: &str) {
    let caret = state.current.borrow().selection().anchor;
    let mut next = state.current.borrow().clone();
    match next.set_table_style_at_offset(caret, style_id) {
        Ok(()) => {
            if !apply_with_history(app, state, next, HistoryKind::DocumentAction) {
                return;
            }
            let status = match loom_objects::TableStyleCatalog::get(style_id) {
                Some(style) => format!("Table style set: {}", style.name),
                None => "Table style set".to_string(),
            };
            app.set_status_right(SharedString::from(status));
        }
        Err(err) => {
            app.set_status_right(SharedString::from(format!("Style failed: {err}")));
        }
    }
}

/// Toggles a table option (e.g. banded_rows, first_column) for the table enclosing caret.
pub(crate) fn toggle_table_option(app: &WriterApp, state: &GuiState, option: &str) {
    let caret = state.current.borrow().selection().anchor;
    let mut next = state.current.borrow().clone();
    match next.toggle_table_option_at_offset(caret, option) {
        Ok(()) => {
            if !apply_with_history(app, state, next, HistoryKind::DocumentAction) {
                return;
            }
            app.set_status_right(SharedString::from(format!(
                "Table option toggled: {}",
                table_option_label(option)
            )));
        }
        Err(err) => {
            app.set_status_right(SharedString::from(format!("Option failed: {err}")));
        }
    }
}

/// Synchronizes table inspector state and accessibility label with the document caret.
pub(crate) fn update_table_selection_properties(
    app: &WriterApp,
    doc: &WriterDocument,
    caret: usize,
) {
    let location = doc.table_cell_at_offset(caret);
    if get_table_cell_selection_range()
        .is_some_and(|range| location.map_or(true, |location| location.block_id != range.block_id))
    {
        clear_table_cell_selection_range();
    }

    if let Some(loc) = location {
        app.set_is_table_selected(true);
        if let Some(table) = doc.table_from_block(loc.block_id) {
            let loom_table = table.to_loom_table();
            app.set_table_style_id(SharedString::from(loom_table.style_id.as_str()));
            app.set_table_header_row(loom_table.header_row);
            app.set_table_banded_rows(loom_table.banded_rows);
            app.set_table_first_column(loom_table.first_column);
            let cell_text = table.get(loc.row, loc.col).unwrap_or("").trim();
            let col_letter = (b'A' + (loc.col as u8 % 26)) as char;
            let row_num = loc.row + 1;

            let label = if let Some(range) = get_table_cell_selection_range() {
                if range.block_id == loc.block_id
                    && (range.start_row != range.end_row || range.start_col != range.end_col)
                {
                    let min_r = range.start_row.min(range.end_row) + 1;
                    let max_r = range.start_row.max(range.end_row) + 1;
                    let min_c = (b'A' + (range.start_col.min(range.end_col) as u8 % 26)) as char;
                    let max_c = (b'A' + (range.start_col.max(range.end_col) as u8 % 26)) as char;
                    format!(
                        "Table {} by {}, cells {}{}:{}{} selected",
                        table.rows.len(),
                        table.columns(),
                        min_c,
                        min_r,
                        max_c,
                        max_r
                    )
                } else {
                    format!(
                        "Table {} by {}, cell {}{}: {}",
                        table.rows.len(),
                        table.columns(),
                        col_letter,
                        row_num,
                        cell_text
                    )
                }
            } else {
                format!(
                    "Table {} by {}, cell {}{}: {}",
                    table.rows.len(),
                    table.columns(),
                    col_letter,
                    row_num,
                    cell_text
                )
            };
            app.set_document_accessible_label(SharedString::from(label));
        }
    } else {
        app.set_is_table_selected(false);
        app.set_document_accessible_label(SharedString::from("Document body"));
    }
}

/// A valid, ordinary pointer press starts a new selection and cancels any
/// keyboard-extended cell range. The label also needs refreshing when the
/// click leaves the caret unchanged and selection projection returns early.
pub(crate) fn clear_pointer(
    app: &WriterApp,
    doc: &WriterDocument,
    caret: usize,
    extend: bool,
) -> bool {
    if extend || get_table_cell_selection_range().is_none() {
        return false;
    }
    clear_table_cell_selection_range();
    update_table_selection_properties(app, doc, caret);
    true
}

/// Refreshes the painted cells after the caller releases its document borrow.
pub(crate) fn refresh_cells(app: &WriterApp, state: &GuiState, cleared: bool) {
    if cleared {
        crate::refresh_render_projection_of(app, state);
    }
}

/// Wires page presses here so clearing the virtual cell range is followed by
/// a projection refresh after the document borrow has been released.
pub(crate) fn wire_pointer_pressed(app: &WriterApp, state: &Rc<GuiState>) {
    let state = state.clone();
    let app_ref = app.as_weak();
    app.on_pointer_pressed(move |x, y, extend| {
        let Some(app) = app_ref.upgrade() else {
            return;
        };
        if crate::picture_drag::pointer_pressed(&app, &state, x, y) {
            return;
        }
        let viewport = *state.viewport.borrow();
        let current = state.current.borrow();
        let Some(offset) = crate::writer_pointer_offset_with_app(&app, &current, viewport, x, y)
        else {
            state.pointer_active.set(false);
            state.pointer_anchor.set(None);
            return;
        };
        let cleared = clear_pointer(&app, &current, offset, extend);
        let clicks = if extend {
            1
        } else {
            crate::multi_click::register_press(x, y)
        };
        if clicks >= 2 {
            let text = current.editor_text();
            let (start, end) = if clicks == 2 {
                crate::multi_click::word_range(&text, offset)
            } else {
                crate::multi_click::paragraph_range(&text, offset)
            };
            drop(current);
            refresh_cells(&app, &state, cleared);
            state.pointer_active.set(false);
            state.pointer_anchor.set(None);
            crate::press_reveal::during_press(|| {
                app.invoke_selection_changed(
                    start.min(i32::MAX as usize) as i32,
                    end.min(i32::MAX as usize) as i32,
                );
            });
            return;
        }
        let anchor = if extend {
            current.selection().anchor
        } else {
            offset
        };
        drop(current);
        refresh_cells(&app, &state, cleared);
        state.pointer_anchor.set(Some(anchor));
        state.pointer_active.set(true);
        crate::press_reveal::during_press(|| {
            app.invoke_selection_changed(
                anchor.min(i32::MAX as usize) as i32,
                offset.min(i32::MAX as usize) as i32,
            );
        });
    });
}

/// A selected range of cells in a table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TableCellRange {
    pub block_id: u64,
    pub start_row: usize,
    pub start_col: usize,
    pub end_row: usize,
    pub end_col: usize,
}

impl TableCellRange {
    pub(crate) fn contains_cell(self, block_id: u64, row: usize, col: usize) -> bool {
        self.block_id == block_id
            && (self.start_row.min(self.end_row)..=self.start_row.max(self.end_row)).contains(&row)
            && (self.start_col.min(self.end_col)..=self.start_col.max(self.end_col)).contains(&col)
    }
}

thread_local! {
    static ACTIVE_CELL_RANGE: std::cell::RefCell<Option<TableCellRange>> = const { std::cell::RefCell::new(None) };
}

/// Returns the currently active table cell selection range, if any.
pub(crate) fn get_table_cell_selection_range() -> Option<TableCellRange> {
    ACTIVE_CELL_RANGE.with(|r| *r.borrow())
}

/// Sets the currently active table cell selection range.
pub(crate) fn set_table_cell_selection_range(range: Option<TableCellRange>) {
    ACTIVE_CELL_RANGE.with(|r| *r.borrow_mut() = range);
}

/// Clears the active table cell selection range.
pub(crate) fn clear_table_cell_selection_range() {
    ACTIVE_CELL_RANGE.with(|r| *r.borrow_mut() = None);
}

/// Returns command specifications for all table commands.
pub(crate) fn table_command_specs() -> Vec<loom_command::CommandSpec> {
    use loom_command::CommandSpec;
    vec![
        CommandSpec::new("insert.table", "Insert Table")
            .with_undo_label("Insert Table")
            .with_description("Insert a table at the caret; its cells are edited as text")
            .with_category("insert")
            .with_order(20),
        CommandSpec::new("writer.table.insert", "Insert Table")
            .with_undo_label("Insert Table")
            .with_description("Insert a Markdown table at the caret; its cells are edited as text")
            .with_category("insert")
            .with_order(21),
        CommandSpec::new("insert.chart", "Insert Chart from Clipboard")
            .with_undo_label("Insert Chart")
            .with_description("Insert an embedded chart from clipboard payload at the caret")
            .with_category("insert")
            .with_order(25),
        CommandSpec::new("writer.chart.insert", "Insert Chart (from Clipboard)")
            .with_undo_label("Insert Chart")
            .with_description("Insert an embedded chart from clipboard payload at the caret")
            .with_category("insert")
            .with_order(26),
        CommandSpec::new("insert.image", "Insert Picture...")
            .with_undo_label("Insert Picture")
            .with_description("Insert an image from a file at the caret")
            .with_category("insert")
            .with_order(27),
        CommandSpec::new("writer.image.insert", "Insert Picture...")
            .with_undo_label("Insert Picture")
            .with_description("Insert an image from a file at the caret")
            .with_category("insert")
            .with_order(28),
        CommandSpec::new("picture.larger", "Make Picture Larger")
            .with_undo_label("Resize Picture")
            .with_description("Enlarge the picture at the caret by 25%")
            .with_category("insert")
            .with_order(29),
        CommandSpec::new("picture.smaller", "Make Picture Smaller")
            .with_undo_label("Resize Picture")
            .with_description("Shrink the picture at the caret by 20%")
            .with_category("insert")
            .with_order(30),
        CommandSpec::new("picture.reset_size", "Reset Picture Size")
            .with_undo_label("Resize Picture")
            .with_description("Return the picture at the caret to its natural size")
            .with_category("insert")
            .with_order(31),
        CommandSpec::new("picture.alt_text", "Picture Description...")
            .with_undo_label("Picture Description")
            .with_description("Edit the description of the picture at the caret")
            .with_category("insert")
            .with_order(38),
        CommandSpec::new("insert.page_break", "Insert Page Break")
            .with_undo_label("Insert Page Break")
            .with_description("End the page at the caret; the text after it starts the next page")
            .with_category("insert")
            .with_order(39)
            .with_shortcut("Ctrl+Enter"),
        CommandSpec::new("insert.line_break", "Insert Line Break")
            .with_undo_label("Insert Line Break")
            .with_description("End the line at the caret without starting a new paragraph")
            .with_category("insert")
            .with_order(40)
            .with_shortcut("Shift+Enter"),
        CommandSpec::new("list.level.increase", "Increase List Level")
            .with_undo_label("Increase List Level")
            .with_description("Indent the list item at the caret one level deeper")
            .with_category("format")
            .with_order(41)
            .with_shortcut("Tab"),
        CommandSpec::new("list.level.decrease", "Decrease List Level")
            .with_undo_label("Decrease List Level")
            .with_description("Move the list item at the caret one level shallower")
            .with_category("format")
            .with_order(42)
            .with_shortcut("Shift+Tab"),
        CommandSpec::new("table.insert_row_above", "Insert Row Above")
            .with_undo_label("Insert Row Above")
            .with_description("Insert a new table row above current row")
            .with_category("table")
            .with_order(30)
            .with_shortcut("Ctrl+Alt+Up")
            .with_enabled(false),
        CommandSpec::new("table.insert_row_below", "Insert Row Below")
            .with_undo_label("Insert Row Below")
            .with_description("Insert a new table row below current row")
            .with_category("table")
            .with_order(31)
            .with_shortcut("Ctrl+Alt+Down")
            .with_enabled(false),
        CommandSpec::new("table.insert_col_left", "Insert Column Left")
            .with_undo_label("Insert Column Left")
            .with_description("Insert a new table column to the left")
            .with_category("table")
            .with_order(32)
            .with_shortcut("Ctrl+Alt+Left")
            .with_enabled(false),
        CommandSpec::new("table.insert_col_right", "Insert Column Right")
            .with_undo_label("Insert Column Right")
            .with_description("Insert a new table column to the right")
            .with_category("table")
            .with_order(33)
            .with_shortcut("Ctrl+Alt+Right")
            .with_enabled(false),
        CommandSpec::new("table.delete_row", "Delete Row")
            .with_undo_label("Delete Row")
            .with_description("Delete current table row")
            .with_category("table")
            .with_order(34)
            .with_shortcut("Ctrl+Alt+Backspace")
            .with_enabled(false),
        CommandSpec::new("table.delete_column", "Delete Column")
            .with_undo_label("Delete Column")
            .with_description("Delete current table column")
            .with_category("table")
            .with_order(35)
            .with_enabled(false),
        CommandSpec::new("table.delete_col", "Delete Column")
            .with_undo_label("Delete Column")
            .with_description("Delete current table column")
            .with_category("table")
            .with_order(35)
            .with_enabled(false),
        CommandSpec::new("table.merge_cells", "Merge Cells")
            .with_undo_label("Merge Cells")
            .with_description("Merge selected table cells")
            .with_category("table")
            .with_order(36)
            .with_shortcut("Ctrl+Alt+M")
            .with_enabled(false),
        CommandSpec::new("table.split_cell", "Split Cell")
            .with_undo_label("Split Cell")
            .with_description("Split current table cell")
            .with_category("table")
            .with_order(37)
            .with_shortcut("Ctrl+Alt+S")
            .with_enabled(false),
    ]
}

/// Updates enablement for table commands in the registry.
pub(crate) fn sync_table_registry_enablement(
    registry: &mut loom_command::CommandRegistry,
    enabled: bool,
) {
    use loom_command::CommandId;
    for id in [
        "insert.table",
        "writer.table.insert",
        "insert.chart",
        "writer.chart.insert",
        "insert.image",
        "writer.image.insert",
        "picture.larger",
        "picture.smaller",
        "picture.reset_size",
        "picture.alt_text",
    ] {
        registry.set_enabled(&CommandId::new(id), true);
    }
    for id in [
        "table.insert_row_above",
        "table.insert_row_below",
        "table.insert_col_left",
        "table.insert_col_right",
        "table.delete_row",
        "table.delete_column",
        "table.delete_col",
        "table.merge_cells",
        "table.split_cell",
    ] {
        registry.set_enabled(&CommandId::new(id), enabled);
    }
}

/// Handles keyboard events within tables: Ctrl+Alt structural commands and Shift+arrows cell range selection.
pub(crate) fn handle_table_key(
    app: &WriterApp,
    state: &GuiState,
    key: &str,
    shift: bool,
    ctrl: bool,
    alt: bool,
) -> bool {
    let caret = state.current.borrow().selection().anchor;
    let doc = state.current.borrow();
    let Some(loc) = doc.table_cell_at_offset(caret) else {
        return false;
    };
    let Some(table) = doc.table_from_block(loc.block_id) else {
        return false;
    };
    drop(doc);

    let matches_key = |k: slint::platform::Key, name: &str| -> bool {
        let s: slint::SharedString = k.into();
        key == s.as_str() || key == name
    };

    // A collapsed caret-navigation key leaves the virtual cell range and
    // continues through the editor's normal navigation path. This includes
    // page and word/line navigation, not only the four plain arrows.
    let navigates_caret = [
        (slint::platform::Key::RightArrow, "Right"),
        (slint::platform::Key::LeftArrow, "Left"),
        (slint::platform::Key::DownArrow, "Down"),
        (slint::platform::Key::UpArrow, "Up"),
        (slint::platform::Key::PageUp, "PageUp"),
        (slint::platform::Key::PageDown, "PageDown"),
        (slint::platform::Key::Home, "Home"),
        (slint::platform::Key::End, "End"),
    ]
    .into_iter()
    .any(|(key_code, name)| matches_key(key_code, name));
    if !shift && !alt && navigates_caret && get_table_cell_selection_range().is_some() {
        clear_table_cell_selection_range();
        let doc = state.current.borrow();
        update_table_selection_properties(app, &doc, caret);
        drop(doc);
        crate::refresh_render_projection_of(app, state);
        return false;
    }

    // Ctrl+B, Ctrl+I and Ctrl+U format the typed runs of the selected cells or
    // of the selected text inside one cell; other selections keep their path.
    if ctrl && !alt && !shift {
        if let Some(emphasis) = table_format_actions::Emphasis::from_shortcut(key) {
            if table_format_actions::toggle_emphasis(app, state, emphasis) {
                return true;
            }
        }
    }

    // Ctrl+Alt shortcuts
    if ctrl && alt {
        let action = if matches_key(slint::platform::Key::UpArrow, "Up") {
            "insert-row-above"
        } else if matches_key(slint::platform::Key::DownArrow, "Down") {
            "insert-row-below"
        } else if matches_key(slint::platform::Key::LeftArrow, "Left") {
            "insert-col-left"
        } else if matches_key(slint::platform::Key::RightArrow, "Right") {
            "insert-col-right"
        } else if matches_key(slint::platform::Key::Backspace, "Backspace") {
            "delete-row"
        } else if matches_key(slint::platform::Key::Delete, "Delete") {
            "delete-col"
        } else if key == "m" || key == "M" {
            "merge-cells"
        } else if key == "s" || key == "S" {
            "split-cell"
        } else {
            return false;
        };
        table_action(app, state, action);
        return true;
    }

    // Shift + arrows: 2D cell range selection
    if shift && !ctrl && !alt {
        let rows = table.rows.len();
        let cols = table.columns();
        let mut range = get_table_cell_selection_range().unwrap_or(TableCellRange {
            block_id: loc.block_id,
            start_row: loc.row,
            start_col: loc.col,
            end_row: loc.row,
            end_col: loc.col,
        });
        if range.block_id != loc.block_id {
            range = TableCellRange {
                block_id: loc.block_id,
                start_row: loc.row,
                start_col: loc.col,
                end_row: loc.row,
                end_col: loc.col,
            };
        }

        let is_arrow = if matches_key(slint::platform::Key::RightArrow, "Right") {
            range.end_col = (range.end_col + 1).min(cols - 1);
            true
        } else if matches_key(slint::platform::Key::LeftArrow, "Left") {
            range.end_col = range.end_col.saturating_sub(1);
            true
        } else if matches_key(slint::platform::Key::DownArrow, "Down") {
            range.end_row = (range.end_row + 1).min(rows - 1);
            true
        } else if matches_key(slint::platform::Key::UpArrow, "Up") {
            range.end_row = range.end_row.saturating_sub(1);
            true
        } else {
            false
        };

        if is_arrow {
            set_table_cell_selection_range(Some(range));
            let doc = state.current.borrow();
            update_table_selection_properties(app, &doc, caret);
            drop(doc);
            crate::refresh_render_projection_of(app, state);
            return true;
        }
    }

    false
}
