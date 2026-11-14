//! Shared command dispatch for menu, toolbar, and palette actions.

use loom_desktop::{CommandAction, DesktopError};
use slint::{ComponentHandle, Model, SharedString};

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
        "app.quit" | "loom sheets.quit" => app
            .window()
            .dispatch_event(slint::platform::WindowEvent::CloseRequested),
        "file.new" | "sheets.new" => app.invoke_new_sheet(),
        "file.new_template" | "sheets.new-template" => {
            app.set_template_chooser_open(true);
            app.invoke_focus_template_chooser();
            // The menu that dispatched this command hands focus back to the
            // grid as it closes; take it again once that has happened.
            let weak = slint::ComponentHandle::as_weak(app);
            slint::Timer::single_shot(std::time::Duration::from_millis(50), move || {
                if let Some(app) = weak.upgrade() {
                    if app.get_template_chooser_open() {
                        app.invoke_focus_template_chooser();
                    }
                }
            });
        }
        "file.open" | "sheets.open" => app.invoke_open_sheet(),
        "file.save" | "sheets.save" => app.invoke_save_sheet(),
        "file.save_as" | "sheets.save-as" => app.invoke_save_as_sheet(),
        "sheets.retry-recovery" => app.invoke_retry_recovery(),
        "file.export_csv" | "sheets.export-csv" => app.invoke_export_csv(),
        "file.export_xlsx" | "sheets.export-xlsx" => app.invoke_export_xlsx(),
        "file.export_pdf" | "sheets.export-pdf" => {
            app.global::<crate::SheetInspectorActions>()
                .invoke_export_pdf();
        }
        "arrange.bring_forward" => {
            app.global::<crate::SheetInspectorActions>()
                .invoke_arrange_object(0);
        }
        "arrange.send_backward" => {
            app.global::<crate::SheetInspectorActions>()
                .invoke_arrange_object(1);
        }
        "arrange.bring_to_front" => {
            app.global::<crate::SheetInspectorActions>()
                .invoke_arrange_object(2);
        }
        "arrange.send_to_back" => {
            app.global::<crate::SheetInspectorActions>()
                .invoke_arrange_object(3);
        }
        "edit.undo" | "sheets.undo" => app.invoke_undo(),
        "edit.redo" | "sheets.redo" => app.invoke_redo(),
        "edit.cut" | "sheets.cut" => app.invoke_cut_selection(),
        "edit.copy" | "sheets.copy" => app.invoke_copy_selection(),
        "edit.paste" | "sheets.paste" => app.invoke_paste_selection(),
        "edit.select_all" | "sheets.select-all" => app.invoke_select_all(),
        "edit.find" | "sheets.find" => crate::data_tools::open_find_replace(app, false),
        "edit.replace" | "sheets.replace" => crate::data_tools::open_find_replace(app, true),
        "data.toggle_filter" | "sheets.toggle-filter" => {
            app.global::<crate::DataToolsState>()
                .invoke_toggle_auto_filter();
        }
        "data.filter_column" | "sheets.filter-column" => {
            app.global::<crate::DataToolsState>()
                .invoke_open_filter_column();
        }
        "data.clear_filter" | "sheets.clear-filter" => {
            app.global::<crate::DataToolsState>()
                .invoke_clear_all_filters();
        }
        "data.named_ranges" | "sheets.named-ranges" => {
            app.global::<crate::DataToolsState>()
                .invoke_open_named_ranges();
        }
        "data.validation" | "sheets.validation" => {
            app.global::<crate::DataToolsState>().set_val_expanded(true);
            app.invoke_show_inspector_tab(1);
        }
        "app.palette" | "help.shortcuts" => app.invoke_open_palette(),
        "help.welcome" | "sheets.welcome" => crate::welcome_guide::open(app),
        "view.inspector" => app.invoke_toggle_inspector(),
        "view.zoom_in" => app.invoke_zoom_in(),
        "view.zoom_out" => app.invoke_zoom_out(),
        "view.zoom_actual" => app.invoke_zoom_actual(),
        id if crate::appearance::dispatch(app, id) => {}
        id if crate::window_chrome::dispatch(app, id) => {}
        "table.add_row" => app.invoke_add_row(),
        "table.delete_row" => app.invoke_delete_row(),
        "table.add_col" | "sheets.add-col" => app.invoke_add_table_col(),
        "table.delete_col" => app.invoke_delete_col(),
        "table.sort_asc" | "sheets.sort-asc" => app.invoke_sort_ascending(),
        "table.sort_desc" | "sheets.sort-desc" => app.invoke_sort_descending(),
        "table.freeze_header" | "sheets.freeze-header" => app.invoke_freeze_panes(),
        "table.unfreeze_panes" | "sheets.unfreeze-panes" => app.invoke_unfreeze_panes(),
        "sheets.delete_sheet" => app.invoke_delete_sheet(),
        "sheets.add_sheet" => app.invoke_add_sheet(),
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
        "sheets.worksheet-objects" => {
            if app.get_object_views().row_count() == 0 {
                return false;
            }
            app.invoke_focus_objects();
        }
        "sheets.cycle_chart_kind" => app.invoke_cycle_chart_kind(),
        "sheets.copy_chart" => app.invoke_copy_chart(),
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

/// Platform modifier label shown in menus and the command palette.
pub(crate) const PRIMARY_MOD: &str = if cfg!(target_os = "macos") {
    "Cmd+"
} else {
    "Ctrl+"
};

const fn sc(mac: &'static str, other: &'static str) -> &'static str {
    if cfg!(target_os = "macos") {
        mac
    } else {
        other
    }
}

/// Canonical keyboard shortcut label for a Sheets command identifier.
pub(crate) fn shortcut_for_id(id: &str) -> &'static str {
    match id {
        "file.new" | "sheets.new" => sc("Cmd+N", "Ctrl+N"),
        "file.new_template" | "sheets.new-template" => sc("Cmd+Shift+N", "Ctrl+Shift+N"),
        "file.open" | "sheets.open" => sc("Cmd+O", "Ctrl+O"),
        "file.save" | "sheets.save" => sc("Cmd+S", "Ctrl+S"),
        "file.save_as" | "sheets.save-as" => sc("Cmd+Shift+S", "Ctrl+Shift+S"),
        "sheets.retry-recovery" => sc("Cmd+Alt+Z", "Ctrl+Alt+Z"),
        "file.export_csv" | "sheets.export-csv" => sc("Cmd+E", "Ctrl+E"),
        "file.export_xlsx" | "sheets.export-xlsx" => sc("Cmd+Shift+E", "Ctrl+Shift+E"),
        "file.export_pdf" | "sheets.export-pdf" => sc("Cmd+P", "Ctrl+P"),
        "arrange.bring_forward" => sc("Cmd+Alt+]", "Ctrl+Alt+]"),
        "arrange.send_backward" => sc("Cmd+Alt+[", "Ctrl+Alt+["),
        "arrange.bring_to_front" => sc("Cmd+Shift+Alt+]", "Ctrl+Shift+Alt+]"),
        "arrange.send_to_back" => sc("Cmd+Shift+Alt+[", "Ctrl+Shift+Alt+["),
        "edit.undo" | "sheets.undo" => sc("Cmd+Z", "Ctrl+Z"),
        "edit.redo" | "sheets.redo" => sc("Cmd+Shift+Z", "Ctrl+Shift+Z"),
        "edit.cut" | "sheets.cut" => sc("Cmd+X", "Ctrl+X"),
        "edit.copy" | "sheets.copy" => sc("Cmd+C", "Ctrl+C"),
        "edit.paste" | "sheets.paste" => sc("Cmd+V", "Ctrl+V"),
        "edit.select_all" | "sheets.select-all" => sc("Cmd+A", "Ctrl+A"),
        "edit.find" | "sheets.find" => sc("Cmd+F", "Ctrl+F"),
        "edit.replace" | "sheets.replace" => sc("Cmd+H", "Ctrl+H"),
        "data.toggle_filter" | "sheets.toggle-filter" => sc("Cmd+Alt+A", "Ctrl+Alt+A"),
        "data.filter_column" | "sheets.filter-column" => sc("Cmd+Alt+Q", "Ctrl+Alt+Q"),
        "data.clear_filter" | "sheets.clear-filter" => sc("Cmd+Alt+E", "Ctrl+Alt+E"),
        "data.named_ranges" | "sheets.named-ranges" => sc("Cmd+Alt+H", "Ctrl+Alt+H"),
        "data.validation" | "sheets.validation" => sc("Cmd+Alt+V", "Ctrl+Alt+V"),
        "table.fill_down" => sc("Cmd+D", "Ctrl+D"),
        "app.palette" => sc("Cmd+K", "Ctrl+K"),
        "help.shortcuts" => sc("Cmd+Shift+P", "Ctrl+Shift+P"),
        "help.welcome" | "sheets.welcome" => sc("Cmd+Shift+/", "Ctrl+Shift+/"),
        "view.inspector" => sc("Cmd+Alt+I", "Ctrl+Alt+I"),
        "view.zoom_in" => sc("Cmd+=", "Ctrl+="),
        "view.zoom_out" => sc("Cmd+-", "Ctrl+-"),
        "view.zoom_actual" => sc("Cmd+0", "Ctrl+0"),
        "sheets.add_sheet" => sc("Cmd+Alt+N", "Ctrl+Alt+N"),
        "sheets.delete_sheet" => sc("Cmd+Alt+X", "Ctrl+Alt+X"),
        "table.add_row" => sc("Cmd+Alt+R", "Ctrl+Alt+R"),
        "table.delete_row" => sc("Cmd+Alt+W", "Ctrl+Alt+W"),
        "table.add_col" | "sheets.add-col" => sc("Cmd+Alt+K", "Ctrl+Alt+K"),
        "table.delete_col" => sc("Cmd+Alt+L", "Ctrl+Alt+L"),
        "table.sort_asc" | "sheets.sort-asc" | "sheets.organize" => sc("Cmd+Alt+S", "Ctrl+Alt+S"),
        "table.sort_desc" | "sheets.sort-desc" => sc("Cmd+Alt+D", "Ctrl+Alt+D"),
        "table.freeze_header" | "sheets.freeze-header" => sc("Cmd+Alt+F", "Ctrl+Alt+F"),
        "table.unfreeze_panes" | "sheets.unfreeze-panes" => sc("Cmd+Alt+U", "Ctrl+Alt+U"),
        "format.bold" | "sheets.bold" => sc("Cmd+B", "Ctrl+B"),
        "format.italic" | "sheets.italic" => sc("Cmd+I", "Ctrl+I"),
        "format.underline" | "sheets.underline" => sc("Cmd+U", "Ctrl+U"),
        "format.align_left" | "sheets.align-left" => sc("Cmd+Shift+L", "Ctrl+Shift+L"),
        "format.align_center" | "sheets.align-center" => sc("Cmd+Shift+M", "Ctrl+Shift+M"),
        "format.align_right" | "sheets.align-right" => sc("Cmd+Shift+R", "Ctrl+Shift+R"),
        "format.general" => sc("Cmd+Alt+0", "Ctrl+Alt+0"),
        "format.number" => sc("Cmd+Alt+1", "Ctrl+Alt+1"),
        "format.currency" => sc("Cmd+Alt+4", "Ctrl+Alt+4"),
        "format.percentage" => sc("Cmd+Alt+5", "Ctrl+Alt+5"),
        "table.adjust_decimals_increase" => sc("Cmd+Alt+.", "Ctrl+Alt+."),
        "table.adjust_decimals_decrease" => sc("Cmd+Alt+,", "Ctrl+Alt+,"),
        "format.borders" | "sheets.borders" => sc("Cmd+Alt+B", "Ctrl+Alt+B"),
        "format.fill_cycle" | "sheets.fill-cycle" => sc("Cmd+Alt+G", "Ctrl+Alt+G"),
        "format.font_increase" => sc("Cmd+Shift+.", "Ctrl+Shift+."),
        "format.font_decrease" => sc("Cmd+Shift+,", "Ctrl+Shift+,"),
        "sheets.insert_chart" => sc("Cmd+Alt+C", "Ctrl+Alt+C"),
        "sheets.insert_shape" => sc("Cmd+Alt+J", "Ctrl+Alt+J"),
        "sheets.insert_image" => sc("Cmd+Alt+M", "Ctrl+Alt+M"),
        "sheets.cycle_chart_kind" => sc("Cmd+Alt+T", "Ctrl+Alt+T"),
        "sheets.copy_chart" => sc("Cmd+Alt+Y", "Ctrl+Alt+Y"),
        "sheets.worksheet-objects" => "F6",
        "table.pivot_sum" => sc("Cmd+Alt+P", "Ctrl+Alt+P"),
        "table.pivot_count" => sc("Cmd+Alt+2", "Ctrl+Alt+2"),
        "table.pivot_average" => sc("Cmd+Alt+3", "Ctrl+Alt+3"),
        "table.pivot_min" => sc("Cmd+Alt+8", "Ctrl+Alt+8"),
        "table.pivot_max" => sc("Cmd+Alt+9", "Ctrl+Alt+9"),
        "view.appearance.system" => sc("Cmd+Alt+6", "Ctrl+Alt+6"),
        "view.appearance.light" => sc("Cmd+Alt+7", "Ctrl+Alt+7"),
        "view.appearance.dark" => sc("Cmd+Shift+D", "Ctrl+Shift+D"),
        "view.appearance.high_contrast" => sc("Cmd+Shift+H", "Ctrl+Shift+H"),
        "view.window-frame.system" => sc("Cmd+Alt+[", "Ctrl+Alt+["),
        "view.window-frame.loom" => sc("Cmd+Alt+]", "Ctrl+Alt+]"),
        _ => "",
    }
}

/// Dispatch a Ctrl/Cmd chord from the Slint root focus handler.
pub(crate) fn dispatch_shortcut(app: &SheetsApp, key: &str, shift: bool, alt: bool) -> bool {
    if app.get_close_draining() {
        return true;
    }
    if app.get_xlsx_import_warning_open() {
        return false;
    }
    let ch = key.chars().next();
    if shift && !alt && ch == Some(char::from(slint::platform::Key::Backspace)) {
        return dispatch_command(app, "sheets.delete_sheet");
    }
    if !shift && alt {
        if let Some(c) = ch {
            if c == char::from(slint::platform::Key::DownArrow) {
                return dispatch_command(app, "table.add_row");
            }
            if c == char::from(slint::platform::Key::UpArrow) {
                return dispatch_command(app, "table.delete_row");
            }
            if c == char::from(slint::platform::Key::RightArrow) {
                return dispatch_command(app, "table.add_col");
            }
            if c == char::from(slint::platform::Key::LeftArrow) {
                return dispatch_command(app, "table.delete_col");
            }
        }
    }
    let lower = key.to_ascii_lowercase();
    if !alt && lower == "q" && !shift {
        app.invoke_window_close();
        return true;
    }
    if !alt && (lower == "=" || lower == "+") {
        return dispatch_command(app, "view.zoom_in");
    }
    let command_id = match (lower.as_str(), shift, alt) {
        ("k", _, false) | ("p", true, false) => "app.palette",
        ("/", true, false) | ("?", _, false) => "help.welcome",
        ("n", false, false) => "file.new",
        ("n", true, false) => "file.new_template",
        ("o", false, false) => "file.open",
        ("s", false, false) => "file.save",
        ("s", true, false) => "file.save_as",
        ("e", false, false) => "file.export_csv",
        ("e", true, false) => "file.export_xlsx",
        ("z", false, false) => "edit.undo",
        ("z", true, false) | ("y", false, false) => "edit.redo",
        ("x", false, false) => "edit.cut",
        ("c", false, false) => "edit.copy",
        ("v", false, false) => "edit.paste",
        ("a", false, false) => "edit.select_all",
        ("f", false, false) => "edit.find",
        ("h", false, false) => "edit.replace",
        ("a", false, true) => "data.toggle_filter",
        ("q", false, true) => "data.filter_column",
        ("e", false, true) => "data.clear_filter",
        ("h", false, true) => "data.named_ranges",
        ("v", false, true) => "data.validation",
        ("d", false, false) => "table.fill_down",
        ("b", false, false) => "format.bold",
        ("i", false, false) => "format.italic",
        ("u", false, false) => "format.underline",
        ("-", false, false) | ("_", true, false) => "view.zoom_out",
        ("0", false, false) => "view.zoom_actual",
        ("l", true, false) => "format.align_left",
        ("m", true, false) => "format.align_center",
        ("r", true, false) => {
            if app.get_recovery_paused() {
                "sheets.retry-recovery"
            } else {
                "format.align_right"
            }
        }
        ("." | ">", true, false) => "format.font_increase",
        ("," | "<", true, false) => "format.font_decrease",
        ("d", true, false) => "view.appearance.dark",
        ("h", true, false) => "view.appearance.high_contrast",
        ("i", false, true) => "view.inspector",
        ("n", false, true) => "sheets.add_sheet",
        ("x", false, true) => "sheets.delete_sheet",
        ("r", false, true) => "table.add_row",
        ("w", false, true) => "table.delete_row",
        ("k", false, true) => "table.add_col",
        ("l", false, true) => "table.delete_col",
        ("s", false, true) => "table.sort_asc",
        ("d", false, true) => "table.sort_desc",
        ("f", false, true) => "table.freeze_header",
        ("u", false, true) => "table.unfreeze_panes",
        ("b", false, true) => "format.borders",
        ("g", false, true) => "format.fill_cycle",
        ("0", false, true) => "format.general",
        ("1", false, true) => "format.number",
        ("4", false, true) => "format.currency",
        ("5", false, true) => "format.percentage",
        (".", false, true) => "table.adjust_decimals_increase",
        (",", false, true) => "table.adjust_decimals_decrease",
        ("c", false, true) => "sheets.insert_chart",
        ("j", false, true) => "sheets.insert_shape",
        ("m", false, true) => "sheets.insert_image",
        ("o", false, true) => "sheets.worksheet-objects",
        ("t", false, true) => "sheets.cycle_chart_kind",
        ("y", false, true) => "sheets.copy_chart",
        ("p", false, true) => "table.pivot_sum",
        ("2", false, true) => "table.pivot_count",
        ("3", false, true) => "table.pivot_average",
        ("8", false, true) => "table.pivot_min",
        ("9", false, true) => "table.pivot_max",
        ("6", false, true) => "view.appearance.system",
        ("7", false, true) => "view.appearance.light",
        ("[", false, true) => "view.window-frame.system",
        ("]", false, true) => "view.window-frame.loom",
        ("z", false, true) if app.get_recovery_paused() => "sheets.retry-recovery",
        _ => return false,
    };
    dispatch_command(app, command_id)
}

/// Wire platform shortcut labels and root Ctrl/Cmd shortcut dispatch.
pub(crate) fn wire_shortcuts(app: &SheetsApp) {
    app.set_mod_key(PRIMARY_MOD.into());
    let app_ref = app.as_weak();
    app.on_shortcut_pressed(move |key, shift, alt| {
        app_ref
            .upgrade()
            .is_some_and(|app| dispatch_shortcut(&app, key.as_str(), shift, alt))
    });
}
