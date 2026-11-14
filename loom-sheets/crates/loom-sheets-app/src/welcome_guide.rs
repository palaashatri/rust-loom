//! Interactive first-run and on-demand Welcome Guide for Loom Sheets.

use std::rc::Rc;
use std::sync::Arc;

use loom_desktop::settings_file;
use loom_desktop::NativeMenuBar;
use slint::{ComponentHandle, SharedString};

use crate::appearance::APPLICATION_ID;
use crate::{
    apply_sheet, sync_menu_state, sync_sheet_tabs, sync_window_title, GuiState, SheetsApp,
};

const WELCOME_SEEN_KEY: &str = "welcome_guide_seen";

/// True when the user has not yet dismissed or completed the Welcome Guide.
pub(crate) fn should_show_on_first_run() -> bool {
    let path = settings_file::path_for_application(APPLICATION_ID);
    settings_file::read(&path, WELCOME_SEEN_KEY).as_deref() != Some("true")
}

/// Persists that the Welcome Guide has been shown to the user.
pub(crate) fn mark_seen() {
    let path = settings_file::path_for_application(APPLICATION_ID);
    let _ = settings_file::write(&path, WELCOME_SEEN_KEY, "true");
}

/// Opens the Welcome Guide at Step 1 and moves keyboard focus into it.
pub(crate) fn open(app: &SheetsApp) {
    app.set_welcome_step(0);
    app.set_welcome_open(true);
    app.invoke_focus_welcome();
}

/// Wires Welcome Guide close and interactive "Try" callbacks.
pub(crate) fn wire(app: &SheetsApp, state: &Rc<GuiState>, menu_service: &Arc<NativeMenuBar>) {
    {
        let app_ref = app.as_weak();
        app.on_welcome_close(move || {
            if let Some(app) = app_ref.upgrade() {
                app.set_welcome_open(false);
                mark_seen();
                app.invoke_focus_grid();
            }
        });
    }
    {
        let app_ref = app.as_weak();
        let state = Rc::clone(state);
        let menu_service = Arc::clone(menu_service);
        app.on_welcome_try_action(move |action| {
            let Some(app) = app_ref.upgrade() else {
                return;
            };
            app.set_welcome_open(false);
            mark_seen();
            match action.as_str() {
                "templates" => {
                    crate::command_dispatch::dispatch_command(&app, "file.new_template");
                }
                "sample-budget" => {
                    load_practice_sheet(&app, &state, &menu_service);
                }
                "add-sheet" => {
                    app.invoke_add_sheet();
                }
                "inspector" => {
                    if !app.get_show_inspector() {
                        crate::command_dispatch::dispatch_command(&app, "view.inspector");
                    }
                    app.set_status_left(
                        "Format Inspector opened — adjust cell formatting or switch to Organize"
                            .into(),
                    );
                }
                "table-menu" => {
                    app.set_toolbar_menu(3);
                }
                "export-menu" => {
                    app.set_toolbar_menu(5);
                }
                "palette" => {
                    app.invoke_open_palette();
                }
                _ => {
                    app.invoke_focus_grid();
                }
            }
        });
    }
}

fn load_practice_sheet(app: &SheetsApp, state: &GuiState, menu_service: &Arc<NativeMenuBar>) {
    if crate::mutation_guard::refused(app, state) {
        return;
    }
    if state.is_dirty() {
        app.set_status_left(SharedString::from(
            "Current workbook has unsaved edits — use New from Template to try an example",
        ));
        app.invoke_focus_grid();
        return;
    }
    let sheet = crate::starter_workbook();
    state.install_workbook(vec![sheet], 0);
    *state.save_path.borrow_mut() = None;
    *state.import_stem.borrow_mut() = None;
    apply_sheet(app, state);
    sync_sheet_tabs(app, state);
    state.mark_saved();
    sync_window_title(app, state);
    sync_menu_state(menu_service, app, state);
    app.set_status_left(
        "Loaded practice sheet — try editing B2:B4 or inspecting formulas in B5:B6".into(),
    );
    app.invoke_focus_grid();
}

#[cfg(test)]
mod tests {
    use super::*;
    use loom_desktop::{FileFilter, ScriptedFileDialogs};

    #[test]
    fn welcome_guide_seen_flag_persists_in_settings_file() {
        let dir = std::env::temp_dir().join(format!("loom-welcome-pref-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp config dir");
        let path = dir.join(APPLICATION_ID).join("settings.toml");
        assert_ne!(
            settings_file::read(&path, WELCOME_SEEN_KEY).as_deref(),
            Some("true")
        );
        settings_file::write(&path, WELCOME_SEEN_KEY, "true").expect("write welcome_guide_seen");
        assert_eq!(
            settings_file::read(&path, WELCOME_SEEN_KEY).as_deref(),
            Some("true")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn welcome_guide_opens_steps_and_runs_interactive_try_actions() {
        crate::set_platform();
        let app = SheetsApp::new().expect("create SheetsApp");
        let dialogs = Rc::new(ScriptedFileDialogs::new([], []));
        let state = Rc::new(GuiState::new(
            crate::blank_sheet(),
            None,
            dialogs,
            FileFilter::new("Workbook", ["loomtable"]).expect("filter"),
            FileFilter::new("CSV", ["csv"]).expect("filter"),
            FileFilter::new("CSV", ["csv"]).expect("filter"),
            FileFilter::new("Excel", ["xlsx"]).expect("filter"),
        ));
        let menu_service = Arc::new(NativeMenuBar::new());
        wire(&app, &state, &menu_service);

        open(&app);
        assert!(app.get_welcome_open());
        assert_eq!(app.get_welcome_step(), 0);

        app.invoke_welcome_try_action("palette".into());
        assert!(!app.get_welcome_open());
        assert!(app.get_palette_open());

        app.set_palette_open(false);
        open(&app);
        app.invoke_welcome_try_action("templates".into());
        assert!(!app.get_welcome_open());
        assert!(app.get_template_chooser_open());

        app.set_template_chooser_open(false);
        open(&app);
        app.invoke_welcome_try_action("table-menu".into());
        assert!(!app.get_welcome_open());
        assert_eq!(app.get_toolbar_menu(), 3);
    }
}
