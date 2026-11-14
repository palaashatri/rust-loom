//! The in-window menu for Sheets. The projection, keyboard navigation, and
//! dispatch logic are shared (`loom_desktop::local_menu_bindings!`); this file
//! only says which commands Sheets handles.

use crate::SheetsApp;

/// Canonical Quit / Exit command id installed in the platform menu bar.
#[cfg(test)]
pub(crate) const QUIT_COMMAND_ID: &str = if cfg!(target_os = "macos") {
    "loom sheets.quit"
} else {
    "app.quit"
};

pub(crate) const SUPPORTED_COMMANDS: &[&str] = &[
    "app.quit",
    "loom sheets.quit",
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
    "view.window-frame.system",
    "view.window-frame.loom",
    "table.add_row",
    "table.delete_row",
    "table.add_col",
    "table.delete_col",
    "table.sort_asc",
    "table.sort_desc",
    "table.freeze_header",
    "table.unfreeze_panes",
    "table.pivot_sum",
    "sheets.add_sheet",
    "sheets.insert_chart",
    "sheets.insert_shape",
    "sheets.insert_image",
    "sheets.delete_sheet",
];

loom_desktop::local_menu_bindings!(SheetsApp, SUPPORTED_COMMANDS, set_status_left);

/// Sheets' menu bar with only the commands Sheets handles enabled. Help >
/// Keyboard Shortcuts opens the command palette.
pub(crate) fn sheets_menu_bar() -> loom_desktop::MenuBar {
    use loom_desktop::{build_standard_menu_bar, CommandState, Menu, MenuItem, MenuShortcut};
    let mut menu_bar = build_standard_menu_bar(
        "Loom Sheets",
        vec![
            MenuItem::action_with_shortcut(
                "file.new_template",
                "New from Template...",
                MenuShortcut::primary_shift("N"),
            ),
            MenuItem::action_with_shortcut(
                "file.export_csv",
                "Export to CSV...",
                MenuShortcut::primary("E"),
            ),
            MenuItem::action_with_shortcut(
                "file.export_xlsx",
                "Export to Excel (.xlsx)...",
                MenuShortcut::primary_shift("E"),
            ),
        ],
        vec![],
        [
            vec![
                CommandState::check("view.inspector", "Format Inspector", false)
                    .with_shortcut(MenuShortcut::primary_alt("I"))
                    .menu_item(),
            ],
            loom_desktop::appearance::menu_items(),
            loom_desktop::window_frame::menu_items(),
        ]
        .concat(),
        vec![Menu::new(
            "Table",
            vec![
                MenuItem::action_with_shortcut(
                    "table.add_row",
                    "Add Row",
                    MenuShortcut::primary_alt("R"),
                ),
                MenuItem::action_with_shortcut(
                    "table.delete_row",
                    "Delete Row",
                    MenuShortcut::primary_alt("W"),
                ),
                MenuItem::action_with_shortcut(
                    "table.add_col",
                    "Add Column",
                    MenuShortcut::primary_alt("K"),
                ),
                MenuItem::action_with_shortcut(
                    "table.delete_col",
                    "Delete Column",
                    MenuShortcut::primary_alt("L"),
                ),
                MenuItem::action_with_shortcut(
                    "table.sort_asc",
                    "Sort Ascending",
                    MenuShortcut::primary_alt("S"),
                ),
                MenuItem::action_with_shortcut(
                    "table.sort_desc",
                    "Sort Descending",
                    MenuShortcut::primary_alt("D"),
                ),
                MenuItem::action_with_shortcut(
                    "table.freeze_header",
                    "Freeze Header Row",
                    MenuShortcut::primary_alt("F"),
                ),
                MenuItem::action_with_shortcut(
                    "table.unfreeze_panes",
                    "Unfreeze Panes",
                    MenuShortcut::primary_alt("U"),
                ),
                MenuItem::action_with_shortcut(
                    "table.pivot_sum",
                    "Pivot Summary (Sum)",
                    MenuShortcut::primary_alt("P"),
                ),
                MenuItem::action_with_shortcut(
                    "sheets.insert_chart",
                    "Insert Chart",
                    MenuShortcut::primary_alt("C"),
                ),
                MenuItem::action_with_shortcut(
                    "sheets.insert_shape",
                    "Insert Shape",
                    MenuShortcut::primary_alt("J"),
                ),
                MenuItem::action_with_shortcut(
                    "sheets.insert_image",
                    "Insert Image",
                    MenuShortcut::primary_alt("M"),
                ),
                MenuItem::action_with_shortcut(
                    "sheets.add_sheet",
                    "Add Sheet",
                    MenuShortcut::primary_alt("N"),
                ),
                MenuItem::action_with_shortcut(
                    "sheets.delete_sheet",
                    "Delete Sheet",
                    MenuShortcut::primary_alt("X"),
                ),
            ],
        )],
    );
    menu_bar.apply_command_state(
        &CommandState::action("help.shortcuts", "Keyboard Shortcuts")
            .with_shortcut(MenuShortcut::primary_shift("P")),
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
