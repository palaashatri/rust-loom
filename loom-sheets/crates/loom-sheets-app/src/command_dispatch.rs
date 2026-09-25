//! Shared command dispatch for menu, toolbar, and palette actions.

use loom_desktop::{CommandAction, DesktopError};
use slint::SharedString;

use crate::SheetsApp;

/// Dispatch canonical command IDs through the same Slint callbacks used by
/// Sheets toolbar and palette controls.
pub(crate) fn dispatch_command(app: &SheetsApp, id: &str) -> bool {
    if app.get_close_draining() {
        return true;
    }
    // Keep every route that shares the canonical dispatcher behind the modal
    // decision. The global native menu does not obey the Slint overlay hitbox.
    if app.get_xlsx_import_warning_open() {
        return false;
    }

    match id {
        "file.new" | "sheets.new" => app.invoke_new_sheet(),
        "file.new_template" | "sheets.new-template" => {
            app.set_template_chooser_open(true);
        }
        "file.open" | "sheets.open" => app.invoke_open_sheet(),
        "file.save" | "sheets.save" => app.invoke_save_sheet(),
        "file.save_as" | "sheets.save-as" => app.invoke_save_as_sheet(),
        "file.export_csv" | "sheets.export-csv" => app.invoke_export_csv(),
        "file.export_xlsx" | "sheets.export-xlsx" => app.invoke_export_xlsx(),
        "edit.undo" | "sheets.undo" => app.invoke_undo(),
        "edit.redo" | "sheets.redo" => app.invoke_redo(),
        "edit.cut" | "sheets.cut" => app.invoke_cut_selection(),
        "edit.copy" | "sheets.copy" => app.invoke_copy_selection(),
        "edit.paste" | "sheets.paste" => app.invoke_paste_selection(),
        "edit.select_all" | "sheets.select-all" => app.invoke_select_all(),
        "app.palette" | "help.shortcuts" => app.invoke_open_palette(),
        "view.inspector" => app.invoke_toggle_inspector(),
        "view.zoom_in" => app.invoke_zoom_in(),
        "view.zoom_out" => app.invoke_zoom_out(),
        "view.zoom_actual" => app.invoke_zoom_actual(),
        "table.add_row" => app.invoke_add_row(),
        "table.delete_row" => app.invoke_delete_row(),
        "table.add_col" | "sheets.add-col" => app.invoke_add_table_col(),
        "table.delete_col" => app.invoke_delete_col(),
        "table.sort_asc" | "sheets.sort-asc" => app.invoke_sort_ascending(),
        "table.sort_desc" | "sheets.sort-desc" => app.invoke_sort_descending(),
        "table.freeze_header" | "sheets.freeze-header" => app.invoke_freeze_panes(),
        "table.unfreeze_panes" | "sheets.unfreeze-panes" => app.invoke_unfreeze_panes(),
        "sheets.delete_sheet" => app.invoke_delete_sheet(),
        "sheets.organize" => app.invoke_organize(),
        "format.bold" | "sheets.bold" => app.invoke_toggle_bold(),
        "format.italic" | "sheets.italic" => app.invoke_toggle_italic(),
        "format.underline" | "sheets.underline" => app.invoke_toggle_underline(),
        "format.align_left" | "sheets.align-left" => app.invoke_set_cell_alignment(0),
        "format.align_center" | "sheets.align-center" => app.invoke_set_cell_alignment(1),
        "format.align_right" | "sheets.align-right" => app.invoke_set_cell_alignment(2),
        "format.general" => app.invoke_set_cell_format(0),
        "format.currency" => app.invoke_set_cell_format(1),
        "format.percentage" => app.invoke_set_cell_format(2),
        "format.number" => app.invoke_set_cell_format(3),
        "table.adjust_decimals_increase" => app.invoke_adjust_decimals(1),
        "table.adjust_decimals_decrease" => app.invoke_adjust_decimals(-1),
        "format.borders" | "sheets.borders" => app.invoke_toggle_borders(),
        "format.fill_cycle" | "sheets.fill-cycle" => app.invoke_cycle_fill(),
        "format.font_increase" => app.invoke_adjust_font(1),
        "format.font_decrease" => app.invoke_adjust_font(-1),
        "table.fill_down" => app.invoke_fill_selection(),
        "sheets.insert_chart" => app.invoke_insert_chart(),
        "sheets.insert_shape" => app.invoke_insert_shape(),
        "sheets.insert_image" => app.invoke_insert_image(),
        "sheets.cycle_chart_kind" => app.invoke_cycle_chart_kind(),
        "table.pivot_sum" => app.invoke_pivot_summary(0),
        "table.pivot_count" => app.invoke_pivot_summary(1),
        "table.pivot_average" => app.invoke_pivot_summary(2),
        "table.pivot_min" => app.invoke_pivot_summary(3),
        "table.pivot_max" => app.invoke_pivot_summary(4),
        _ => return false,
    }
    true
}

pub(crate) fn schedule_menu_action(
    app_ref: &slint::Weak<SheetsApp>,
    action: CommandAction,
) -> Result<(), DesktopError> {
    let error_id = action.id.clone();
    app_ref
        .upgrade_in_event_loop(move |app| {
            // A native menu event can be queued immediately before an import
            // warning opens, so check again when the event reaches the UI.
            if app.get_xlsx_import_warning_open() {
                return;
            }
            let id = action.id.as_str();
            if !dispatch_command(&app, id) {
                app.set_status_left(SharedString::from(format!(
                    "Unsupported menu command: {id}"
                )));
            }
        })
        .map_err(|error| {
            DesktopError::InvalidRequest(format!(
                "failed to schedule Sheets menu command {error_id}: {error}"
            ))
        })
}
