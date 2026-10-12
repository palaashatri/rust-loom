//! Real table editing actions and callbacks for Loom Writer (Wave 1).

use loom_writer_core::{TextSelection, WriterDocument};
use slint::{ComponentHandle, SharedString};
use std::rc::Rc;

use crate::{apply_state, apply_with_history, GuiState, HistoryKind, WriterApp};

/// Wires table-related callbacks into the Slint WriterApp.
pub(crate) fn wire(app: &WriterApp, state: &Rc<GuiState>) {
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
}

/// Inserts a new table block of given dimensions at the current caret.
pub(crate) fn insert_table_size(app: &WriterApp, state: &GuiState, rows: usize, cols: usize) {
    let mut next = state.current.borrow().clone();
    let caret = next.selection().anchor;
    match next.insert_table_block(caret, rows, cols) {
        Ok(_) => {
            apply_with_history(app, state, next, HistoryKind::DocumentAction);
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
    let caret = state.current.borrow().selection().anchor;
    let mut next = state.current.borrow().clone();
    let result = match action {
        "insert-row-above" => next.insert_table_row_at_offset(caret, false),
        "insert-row-below" => next.insert_table_row_at_offset(caret, true),
        "insert-col-left" => next.insert_table_column_at_offset(caret, false),
        "insert-col-right" => next.insert_table_column_at_offset(caret, true),
        "delete-row" => next.delete_table_row_at_offset(caret),
        "delete-col" => next.delete_table_column_at_offset(caret),
        "merge-cells" => next.merge_cells_at_offset(caret, 1, 2).map(|_| caret),
        "split-cell" => next.split_cell_at_offset(caret).map(|_| caret),
        _ => return,
    };
    match result {
        Ok(new_caret) => {
            next.set_selection(TextSelection::caret(new_caret));
            apply_with_history(app, state, next, HistoryKind::DocumentAction);
            app.set_status_right(SharedString::from(format!("Table updated: {action}")));
        }
        Err(err) => {
            app.set_status_right(SharedString::from(format!("Table action failed: {err}")));
        }
    }
}

/// Navigates backwards (Shift+Tab) within a table.
pub(crate) fn table_navigate_backwards(app: &WriterApp, state: &GuiState) {
    let caret = state.current.borrow().selection().anchor;
    let mut next = state.current.borrow().clone();
    if let Ok(new_caret) = next.tab_navigate_table(caret, true) {
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
            apply_with_history(app, state, next, HistoryKind::DocumentAction);
            app.set_status_right(SharedString::from(format!("Table style set: {style_id}")));
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
            apply_with_history(app, state, next, HistoryKind::DocumentAction);
            app.set_status_right(SharedString::from(format!(
                "Table option toggled: {option}"
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
    if let Some(loc) = doc.table_cell_at_offset(caret) {
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
            let label = format!(
                "Table {} by {}, cell {}{}: {}",
                table.rows.len(),
                table.columns(),
                col_letter,
                row_num,
                cell_text
            );
            app.set_document_accessible_label(SharedString::from(label));
        }
    } else {
        app.set_is_table_selected(false);
        app.set_document_accessible_label(SharedString::from("Document body"));
    }
}
