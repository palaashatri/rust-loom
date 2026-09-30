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
