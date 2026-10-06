//! The one admission check every workbook-changing callback goes through.
//!
//! While durable recovery is paused (see `recovery_pause`) Sheets must not
//! accept any change it cannot keep safe. Instead of each handler deciding for
//! itself, every callback that changes the workbook lists itself in
//! [`MUTATING`] and starts with [`refused`]. Every other callback must be
//! listed in [`ALLOWED`], with the reason it is safe. A test walks the Rust
//! sources and the Slint declarations, so a callback that is in neither list,
//! or a mutating callback that skips the check, fails the build instead of
//! silently editing a workbook recovery cannot protect.

use crate::{GuiState, SheetsApp};

/// Callbacks that change the workbook (cells, formats, sizes, tabs, objects,
/// charts, history). Names are the Rust `on_<name>` spelling.
#[cfg(test)]
pub(crate) const MUTATING: &[&str] = &[
    "add_row",
    "add_sheet",
    "add_table_col",
    "adjust_col_width",
    "adjust_decimals",
    "adjust_font",
    "adjust_row_height",
    "clear_selected_cells",
    "commit_selected_cell",
    "create_template",
    "cut_selection",
    "cycle_chart_kind",
    "cycle_fill",
    "delete_col",
    "delete_row",
    "delete_sheet",
    "fill_selection",
    "freeze_panes",
    "header_autofit",
    "header_resized",
    "insert_chart",
    "insert_image",
    "insert_shape",
    "object_keyboard_action",
    "object_move_ended",
    "object_move_started",
    "object_resize_ended",
    "object_resize_started",
    "organize",
    "paste_selection",
    "pivot_summary",
    "quick_formula",
    "redo",
    "rename_sheet",
    "set_cell_alignment",
    "set_cell_format",
    "set_fill",
    "sort_ascending",
    "sort_descending",
    "toggle_bold",
    "toggle_borders",
    "toggle_italic",
    "toggle_underline",
    "undo",
    "unfreeze_panes",
];

/// Callbacks that never change the workbook, or that are separately protected.
/// Each group says why.
#[cfg(test)]
pub(crate) const ALLOWED: &[&str] = &[
    // Navigation, selection, scrolling, zoom, and view layout.
    "begin_edit",
    "cancel_selected_cell",
    "cancel_template",
    "cell_clicked",
    "cell_dragged",
    "cell_pressed",
    "corner_pressed",
    "cycle_zoom",
    "drag_ended",
    "extend_selection",
    "goto_cell",
    "grid_scrolled",
    "grid_viewport_changed",
    "header_pressed",
    "inspector_context_changed",
    "inspector_search_edited",
    "jump",
    "navigate_selection",
    "navigate_template_selection",
    "reveal_selected_requested",
    "select_all",
    "select_sheet",
    "toggle_inspector",
    "window_resized",
    "zoom_actual",
    "zoom_in",
    "zoom_out",
    // Preview-only gesture updates and cancels; the commit is gated at the
    // matching `*_ended` callback, and a cancel restores the old geometry.
    "object_move_cancelled",
    "object_moved",
    "object_resize_cancelled",
    "object_resized",
    // Reads the workbook; it never changes it.
    "copy_selection",
    "close_chart",
    // Saving, exporting, retrying recovery, and closing are how the user keeps
    // work safe, so they must stay available while recovery is paused.
    "export_csv",
    "export_xlsx",
    "retry_recovery",
    "save_as_sheet",
    "save_changes_cancel",
    "save_changes_discard",
    "save_changes_save",
    "save_sheet",
    "close_requested",
    // Replacing the whole workbook has its own unsaved-work decision.
    "new_sheet",
    "open_sheet",
    "xlsx_import_cancel",
    "xlsx_import_continue",
    // Menu, toolbar, and palette routing. They only dispatch to the callbacks
    // above, so the guard applies to whatever they invoke.
    "local_menu_action",
    "local_menu_move",
    "local_menu_opened",
    "palette_backspace",
    "palette_close",
    "palette_invoked",
    "palette_key_text",
    "palette_move",
    "palette_query_changed",
    "toolbar_command",
    // Window chrome.
    "window_close",
    "window_drag",
    "window_resize",
];

/// True, with the visible paused status set, when a workbook change must be
/// refused because recovery cannot keep it safe. Call before touching the
/// workbook so a refused change leaves no trace.
pub(crate) fn refused(app: &SheetsApp, state: &GuiState) -> bool {
    crate::recovery_pause::reject_edit(app, state)
}

/// [`refused`] for the callback that would commit an object move or resize:
/// a refused commit also drops the on-screen preview so the object returns to
/// where the workbook still has it.
pub(crate) fn refused_ending_gesture(app: &SheetsApp, state: &GuiState) -> bool {
    if !refused(app, state) {
        return false;
    }
    crate::object_actions::cancel_active_gesture(app, state);
    true
}
