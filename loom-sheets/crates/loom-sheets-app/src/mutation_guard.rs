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
    "apply_border_preset",
    "apply_column_filter",
    "apply_pivot_builder",
    "apply_validation",
    "arrange_object",
    "auto_text_color",
    "automatic_requested",
    "cf_add_rule",
    "cf_clear_selection",
    "cf_remove_rule",
    "clear_all_filters",
    "clear_column_filter",
    "clear_selected_cells",
    "clear_validation",
    "color_hex_requested",
    "color_requested",
    "commit_selected_cell",
    "create_template",
    "cut_selection",
    "cycle_chart_kind",
    "cycle_fill",
    "cycle_text_color",
    "delete_col",
    "delete_named_range",
    "delete_row",
    "delete_sheet",
    "fill_selection",
    "freeze_panes",
    "header_autofit",
    "header_resized",
    "insert_chart",
    "insert_image",
    "insert_shape",
    "merge_cells",
    "object_keyboard_action",
    "object_move_ended",
    "object_move_started",
    "object_resize_ended",
    "object_resize_started",
    "organize",
    "paste_formats_only",
    "paste_selection",
    "paste_values_only",
    "pivot_summary",
    "quick_formula",
    "redo",
    "rename_sheet",
    "replace_all",
    "replace_current",
    "save_named_range",
    "set_cell_alignment",
    "set_cell_format",
    "set_chart_kind",
    "set_chart_title",
    "set_col_width",
    "set_decimals",
    "set_object_field",
    "set_fill",
    "set_font_size",
    "set_row_height",
    "set_series_name",
    "set_x_axis_label",
    "set_y_axis_label",
    "sort_ascending",
    "sort_descending",
    "toggle_auto_filter",
    "toggle_bold",
    "toggle_borders",
    "toggle_chart_legend",
    "toggle_italic",
    "toggle_strikethrough",
    "toggle_underline",
    "toggle_wrap_text",
    "undo",
    "unfreeze_panes",
    "unmerge_cells",
    "wrap_toggled",
    // Canvas table mutations (guarded by `mutation_guard::refused`).
    "delete_active_table",
    "insert_table",
    "rename_active_table",
    "set_active_table_style",
    "toggle_active_table_alternating",
    "toggle_active_table_title",
    // P0 parity mutations (guarded by `mutation_guard::refused`).
    "autofill_series",
    "format_painter_apply",
];

/// Callbacks that never change the workbook, or that are separately protected.
/// Each group says why.
#[cfg(test)]
pub(crate) const ALLOWED: &[&str] = &[
    // Formula bar editing-assist draft updates (do not mutate the committed workbook).
    "accept_suggestion",
    "caret_moved",
    "cycle_f4",
    "dismiss_popup",
    "editing_state_changed",
    "formula_edited",
    // Navigation, selection, scrolling, zoom, and view layout.
    "begin_edit",
    "cancel_selected_cell",
    "cancel_template",
    "cell_clicked",
    "cell_dragged",
    "cell_pressed",
    "cf_close",
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
    "open_conditional_formatting",
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
    "clear_audit",
    "format_painter_copy",
    "format_painter_lock",
    "toggle_show_formulas",
    "trace_dependents",
    "trace_precedents",
    "copy_chart",
    // Saving, exporting, retrying recovery, and closing are how the user keeps
    // work safe, so they must stay available while recovery is paused.
    "export_csv",
    "export_pdf",
    "export_xlsx",
    // Print Preview & Page Setup modal navigation, options, and print dispatch.
    "close_preview",
    "next_page",
    "open_preview",
    "prev_page",
    "print_document",
    "settings_changed",
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
    "shortcut_pressed",
    "toolbar_command",
    "welcome_close",
    "welcome_try_action",
    // Window chrome and native window-event synchronization.
    "window_close",
    "window_drag",
    "window_resize",
    "winit_window_event",
    // Data tools UI navigation, search, and modal/filter-popup controls.
    "close_column_filter",
    "close_find",
    "close_named_ranges",
    "filter_matches",
    "find_next",
    "find_prev",
    "goto_named_range",
    "open_column_filter",
    "open_filter_column",
    "open_named_ranges",
    "select_all_filter_values",
    "toggle_filter_value",
    // Step 1 Start Hub & canvas table selection (no direct workbook mutation).
    "close_hub",
    "new_document",
    "open_file",
    "open_recent",
    "search_edited",
    "select_table",
    "tab_changed",
    // Pivot Builder draft modal controls (only `apply_pivot_builder` mutates the workbook).
    "close_builder",
    "cycle_val_agg",
    "open_builder",
    "select_filter_field",
    "select_filter_value",
    "set_dest_cell",
    "set_dest_new_sheet",
    "set_source_range",
    "set_title",
    "toggle_col_field",
    "toggle_row_field",
    "toggle_val_field",
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
