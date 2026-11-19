use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use loom_desktop::{settings_file, NativeMenuBar};
use slint::{ComponentHandle, SharedString};

use crate::{
    GuiState, PendingReplacement, WelcomeGuideState, WriterApp, WriterHeaderFooterState,
    WriterReviewState,
};

const WELCOME_SEEN_KEY: &str = "welcome_guide_seen";

fn settings_path() -> PathBuf {
    settings_file::path_for_application(crate::appearance::APPLICATION_ID)
}

fn should_show_at(path: &Path) -> bool {
    settings_file::read(path, WELCOME_SEEN_KEY).as_deref() != Some("true")
}

fn mark_seen_at(path: &Path) -> io::Result<()> {
    settings_file::write(path, WELCOME_SEEN_KEY, "true")
}

fn mark_seen() {
    if cfg!(test) {
        return;
    }
    let _ = mark_seen_at(&settings_path());
}

/// Returns true until the user dismisses or acts on the first-run welcome guide.
pub(crate) fn should_show_on_first_run() -> bool {
    should_show_at(&settings_path())
}

/// Opens the 7-step Loom Writer Welcome Guide at Step 1 and syncs platform shortcut labels.
pub(crate) fn open(app: &WriterApp) {
    let guide = app.global::<WelcomeGuideState>();
    let mod_key = format!("{}+", app.get_shortcut_modifier());
    guide.set_mod_key(SharedString::from(mod_key));
    guide.set_step(0);
    guide.set_open(true);
    let toggled_focus = !guide.get_focus_request();
    guide.set_focus_request(toggled_focus);
    app.set_status_left("Opened Welcome Guide".into());
}

fn close_without_refocus(app: &WriterApp) {
    let guide = app.global::<WelcomeGuideState>();
    guide.set_open(false);
    mark_seen();
}

/// Replaces the current document with the practice document when clean, or prompts Save/Discard/Cancel when dirty.
pub(crate) fn load_practice_document(
    app: &WriterApp,
    state: &GuiState,
    menu_service: Option<&NativeMenuBar>,
) -> bool {
    if crate::document_is_dirty(state)
        && crate::request_document_replacement(app, state, PendingReplacement::QuickStartSample)
    {
        return false;
    }
    crate::open_quick_start_sample(app, state);
    if let Some(menu) = menu_service {
        crate::sync_menu_state(menu, app, state);
    }
    true
}

fn run_try_action(
    app: &WriterApp,
    state: &GuiState,
    menu_service: Option<&NativeMenuBar>,
    action: &str,
) {
    close_without_refocus(app);
    match action {
        "templates" => crate::begin_new_document(app),
        "sample-doc" => {
            let _ = load_practice_document(app, state, menu_service);
        }
        "insert-table" => {
            let _ = crate::dispatch_command(app, "writer.table.insert");
            app.invoke_focus_page();
        }
        "headers-footers" => {
            app.global::<WriterHeaderFooterState>().invoke_open_editor();
        }
        "review-panel" => {
            let review = app.global::<WriterReviewState>();
            review.set_panel_open(true);
            review.invoke_navigate(0);
        }
        "print-preview" => {
            crate::print_preview::open(app, state);
        }
        "palette" => {
            app.invoke_open_palette();
        }
        _ => {
            app.invoke_focus_page();
        }
    }
}

/// Connects Welcome Guide close and interactive Try callbacks to Writer application handlers.
pub(crate) fn wire(
    app: &WriterApp,
    state: &Rc<GuiState>,
    menu_service: &Option<Arc<NativeMenuBar>>,
) {
    let guide = app.global::<WelcomeGuideState>();
    {
        let app_ref = app.as_weak();
        guide.on_close_guide(move || {
            let Some(app) = app_ref.upgrade() else {
                return;
            };
            close_without_refocus(&app);
            app.invoke_focus_page();
        });
    }
    {
        let app_ref = app.as_weak();
        let state = state.clone();
        let menu_service = menu_service.clone();
        guide.on_try_action(move |action| {
            let Some(app) = app_ref.upgrade() else {
                return;
            };
            run_try_action(&app, &state, menu_service.as_deref(), action.as_str());
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions_tests::{test_state, text_document};
    use crate::wire_writer_shared_callbacks;
    use loom_desktop::ScriptedFileDialogs;
    use loom_writer_core::WriterDocument;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!(
            "loom-writer-welcome-test-{label}-{}-{nanos}",
            std::process::id()
        ))
    }

    #[test]
    fn welcome_guide_seen_flag_persists_without_erasing_other_settings() {
        let dir = unique_temp_dir("seen");
        let path = dir.join("settings.toml");
        settings_file::write(&path, "theme", "dark").expect("write unrelated setting");

        assert!(should_show_at(&path));
        mark_seen_at(&path).expect("persist welcome_guide_seen");
        assert!(!should_show_at(&path));
        assert_eq!(settings_file::read(&path, "theme").as_deref(), Some("dark"));
        assert_eq!(
            settings_file::read(&path, WELCOME_SEEN_KEY).as_deref(),
            Some("true")
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn open_initializes_step_modifier_and_close_hides_guide() {
        let dialogs: Rc<dyn crate::FileDialogService> = Rc::new(ScriptedFileDialogs::new([], []));
        let (app, state) = test_state(text_document("Draft"), dialogs);
        wire(&app, &state, &None);

        open(&app);
        let guide = app.global::<WelcomeGuideState>();
        assert!(guide.get_open());
        assert_eq!(guide.get_step(), 0);
        assert!(guide.get_mod_key().ends_with('+'));

        guide.set_step(6);
        guide.invoke_close_guide();
        assert!(!guide.get_open());
    }

    #[test]
    fn load_practice_document_guards_dirty_document_before_replacing() {
        let dialogs: Rc<dyn crate::FileDialogService> = Rc::new(ScriptedFileDialogs::new([], []));
        let (app, state) = test_state(text_document("Initial"), dialogs);
        {
            let mut current = state.current.borrow_mut();
            current.replace_paragraphs("Unsaved edits");
        }
        assert!(crate::document_is_dirty(&state));

        let replaced = load_practice_document(&app, &state, None);
        assert!(!replaced);
        assert!(app.get_save_changes_open());
        assert_eq!(
            state.pending_replacement.get(),
            Some(PendingReplacement::QuickStartSample)
        );
        assert_ne!(state.current.borrow().id, "quick-start");
    }

    #[test]
    fn load_practice_document_replaces_clean_document_with_quick_start() {
        let dialogs: Rc<dyn crate::FileDialogService> = Rc::new(ScriptedFileDialogs::new([], []));
        let (app, state) = test_state(WriterDocument::new("blank", "Blank"), dialogs);
        assert!(!crate::document_is_dirty(&state));

        let replaced = load_practice_document(&app, &state, None);
        assert!(replaced);
        assert!(!app.get_save_changes_open());
        assert_eq!(state.current.borrow().id, "quick-start");
        assert!(!crate::document_is_dirty(&state));
    }

    #[test]
    fn try_actions_dispatch_across_all_seven_writer_surfaces() {
        let dialogs: Rc<dyn crate::FileDialogService> = Rc::new(ScriptedFileDialogs::new([], []));
        let (app, state) = test_state(WriterDocument::new("doc-1", "Test Doc"), dialogs);
        wire_writer_shared_callbacks(&app, &state, None);
        crate::wire_palette(&app);
        wire(&app, &state, &None);

        let guide = app.global::<WelcomeGuideState>();

        open(&app);
        guide.invoke_try_action("templates".into());
        assert!(!guide.get_open());
        assert!(app.get_template_chooser_open());
        app.set_template_chooser_open(false);

        open(&app);
        guide.invoke_try_action("headers-footers".into());
        assert!(!guide.get_open());
        assert!(app.global::<WriterHeaderFooterState>().get_dialog_open());

        open(&app);
        guide.invoke_try_action("review-panel".into());
        assert!(!guide.get_open());
        assert!(app.global::<WriterReviewState>().get_panel_open());

        open(&app);
        guide.invoke_try_action("print-preview".into());
        assert!(!guide.get_open());
        assert!(app.global::<crate::PrintPreviewState>().get_open());
        app.global::<crate::PrintPreviewState>().set_open(false);

        open(&app);
        guide.invoke_try_action("palette".into());
        assert!(!guide.get_open());
        assert!(app.get_palette_open());
        app.set_palette_open(false);

        open(&app);
        let blocks_before = state.current.borrow().blocks.len();
        guide.invoke_try_action("insert-table".into());
        assert!(!guide.get_open());
        assert!(state.current.borrow().blocks.len() > blocks_before);

        open(&app);
        *state.last_saved.borrow_mut() = state.current.borrow().clone();
        guide.invoke_try_action("sample-doc".into());
        assert!(!guide.get_open());
        assert_eq!(state.current.borrow().id, "quick-start");
    }
}
