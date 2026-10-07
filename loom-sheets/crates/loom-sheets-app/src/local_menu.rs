//! The in-window menu for Sheets. The projection, keyboard navigation, and
//! dispatch logic are shared (`loom_desktop::local_menu_bindings!`); this file
//! only says which commands Sheets handles.

use crate::SheetsApp;

pub(crate) const SUPPORTED_COMMANDS: &[&str] = &[
    "file.new",
    "file.new_template",
    "file.open",
    "file.save",
    "file.save_as",
    "file.export_csv",
    "file.export_xlsx",
    "edit.undo",
    "edit.redo",
    "edit.cut",
    "edit.copy",
    "edit.paste",
    "edit.select_all",
    "app.palette",
    "help.shortcuts",
    "view.inspector",
    "view.zoom_in",
    "view.zoom_out",
    "view.zoom_actual",
    "view.appearance.system",
    "view.appearance.light",
    "view.appearance.dark",
    "view.appearance.high_contrast",
    "table.add_row",
    "table.delete_row",
    "table.add_col",
    "table.delete_col",
    "table.sort_asc",
    "table.sort_desc",
    "table.freeze_header",
    "table.unfreeze_panes",
    "table.pivot_sum",
    "sheets.insert_shape",
    "sheets.insert_image",
    "sheets.delete_sheet",
];

loom_desktop::local_menu_bindings!(SheetsApp, SUPPORTED_COMMANDS, set_status_left);

/// Sheets' menu bar with only the commands Sheets handles enabled. Help >
/// Keyboard Shortcuts opens the command palette.
pub(crate) fn sheets_menu_bar() -> loom_desktop::MenuBar {
    use loom_desktop::{build_standard_menu_bar, Menu, MenuItem, MenuShortcut};
    let mut menu_bar = build_standard_menu_bar(
        "Loom Sheets",
        vec![
            MenuItem::action("file.new_template", "New from Template..."),
            MenuItem::action_with_shortcut(
                "file.export_csv",
                "Export to CSV...",
                MenuShortcut::primary("E"),
            ),
            MenuItem::action("file.export_xlsx", "Export to Excel (.xlsx)..."),
        ],
        vec![],
        [
            vec![MenuItem::check("view.inspector", "Format Inspector", false)],
            loom_desktop::appearance::menu_items(),
        ]
        .concat(),
        vec![Menu::new(
            "Table",
            vec![
                MenuItem::action("table.add_row", "Add Row"),
                MenuItem::action("table.delete_row", "Delete Row"),
                MenuItem::action("table.add_col", "Add Column"),
                MenuItem::action("table.delete_col", "Delete Column"),
                MenuItem::action("table.sort_asc", "Sort Ascending"),
                MenuItem::action("table.sort_desc", "Sort Descending"),
                MenuItem::action("table.freeze_header", "Freeze Header Row"),
                MenuItem::action("table.unfreeze_panes", "Unfreeze Panes"),
                MenuItem::action("table.pivot_sum", "Pivot Summary (Sum)"),
                MenuItem::action("sheets.insert_shape", "Insert Shape"),
                MenuItem::action("sheets.insert_image", "Insert Image"),
                MenuItem::action("sheets.delete_sheet", "Delete Sheet"),
            ],
        )],
    );
    menu_bar.disable_items_except(SUPPORTED_COMMANDS);
    menu_bar
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use loom_desktop::{build_standard_menu_bar, MenuBarService, NativeMenuBar};

    use super::*;

    #[test]
    fn local_menu_action_uses_the_installed_menu_guard_and_sink() {
        i_slint_backend_testing::init_no_event_loop();
        let app = SheetsApp::new().expect("create SheetsApp");
        let menu_service = Arc::new(NativeMenuBar::new());
        let mut menu = build_standard_menu_bar("Loom Sheets", vec![], vec![], vec![], vec![]);
        menu.disable_items_except(["file.open"]);
        menu_service.install_menu_bar(&menu).expect("install menu");

        let dispatched = Arc::new(Mutex::new(Vec::new()));
        let dispatched_sink = dispatched.clone();
        menu_service
            .register_action_sink(Arc::new(move |action| {
                dispatched_sink.lock().unwrap().push(action.id);
                Ok(())
            }))
            .expect("register action sink");
        wire_action(&app, menu_service);

        app.invoke_local_menu_action("file.open".into());
        app.invoke_local_menu_action("file.save".into());
        assert_eq!(*dispatched.lock().unwrap(), ["file.open"]);
    }
}
