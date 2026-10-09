//! Live command-palette wiring backed by the shared `CommandRegistry`.
//!
//! The registry is a `std::sync::Mutex`, which is not re-entrant. Dispatched
//! palette actions (New Document, Open, ...) call back into handlers that lock
//! the registry again, so the lock must never be held while dispatching.

use super::*;

pub(super) fn wire(app: &WriterApp, state: &Rc<GuiState>) {
    // Live GUI palette wiring overrides the headless `wire_palette` handlers so
    // query filtering and dispatch go through the shared `CommandRegistry`.
    // This ensures menu/toolbar/palette/shortcut/a11y all use one `search`
    // ranking and one `invoke` guard.
    {
        let state_for_palette = state.clone();
        let app_ref = app.as_weak();
        app.on_palette_query_changed(move |query| {
            if let Some(app) = app_ref.upgrade() {
                let registry = state_for_palette.registry.lock().unwrap();
                rebuild_palette_with_registry(&app, &registry, query.as_str());
                app.set_palette_selected(0);
            }
        });
    }
    // Typing and Backspace go through the same shared registry as the rebuilt
    // query, so commands whose enablement depends on the document (Bold, Italic,
    // headings) are found while typing, not only in the empty-query list.
    {
        let state_for_palette = state.clone();
        let app_ref = app.as_weak();
        app.on_palette_key_text(move |text| {
            if let Some(app) = app_ref.upgrade() {
                // After Ctrl+A the search text is selected, so typing replaces it.
                let base = if app.get_palette_query_selected() {
                    String::new()
                } else {
                    app.get_palette_query().to_string()
                };
                app.set_palette_query_selected(false);
                let query = format!("{base}{text}");
                app.set_palette_query(query.as_str().into());
                let registry = state_for_palette.registry.lock().unwrap();
                rebuild_palette_with_registry(&app, &registry, &query);
                app.set_palette_selected(0);
            }
        });
    }
    {
        let state_for_palette = state.clone();
        let app_ref = app.as_weak();
        app.on_palette_backspace(move || {
            if let Some(app) = app_ref.upgrade() {
                // After Ctrl+A, Backspace clears the whole selected search text.
                let mut query = app.get_palette_query().to_string();
                if app.get_palette_query_selected() {
                    query.clear();
                } else {
                    query.pop();
                }
                app.set_palette_query_selected(false);
                app.set_palette_query(query.as_str().into());
                let registry = state_for_palette.registry.lock().unwrap();
                rebuild_palette_with_registry(&app, &registry, &query);
                app.set_palette_selected(0);
            }
        });
    }
    {
        let state_for_palette = state.clone();
        let app_ref = app.as_weak();
        app.on_palette_invoked(move |index| {
            if let Some(app) = app_ref.upgrade() {
                // Resolve the command and run the registry guard while the lock is
                // held, then release it BEFORE dispatching: the dispatched callback
                // (for example New Document) locks the registry again, and
                // `std::sync::Mutex` is not re-entrant.
                let q = app.get_palette_query().trim().to_string();
                let command = {
                    let registry = state_for_palette.registry.lock().unwrap();
                    let command = if q.is_empty() {
                        let mut specs: Vec<&CommandSpec> =
                            registry.commands().filter(|s| s.enabled).collect();
                        specs.sort_by(|a, b| a.order.cmp(&b.order).then(a.id.cmp(&b.id)));
                        specs
                            .into_iter()
                            .filter_map(|spec| {
                                master_palette(&app)
                                    .into_iter()
                                    .find(|c| c.id == spec.id.as_str())
                            })
                            .nth(index as usize)
                    } else {
                        registry
                            .search(&q)
                            .into_iter()
                            .filter(|(spec, _)| spec.enabled)
                            .filter_map(|(spec, _)| {
                                master_palette(&app)
                                    .into_iter()
                                    .find(|c| c.id == spec.id.as_str())
                            })
                            .nth(index as usize)
                    };
                    if let Some(command) = &command {
                        if command.id != "writer.new-sample" {
                            let _ = registry.invoke(&CommandInvocation::new(
                                command.id,
                                InvocationSource::Palette,
                            ));
                        }
                    }
                    command
                };
                if let Some(command) = command {
                    app.set_palette_open(false);
                    if command.id == "writer.new-sample" {
                        if !request_document_replacement(
                            &app,
                            &state_for_palette,
                            PendingReplacement::QuickStartSample,
                        ) {
                            open_quick_start_sample(&app, &state_for_palette);
                        }
                        return;
                    }
                    let _ = dispatch_palette_action(&app, command.action);
                    let registry = state_for_palette.registry.lock().unwrap();
                    let _ = registry.invoke(&CommandInvocation::new(
                        command.id,
                        InvocationSource::Accessibility,
                    ));
                    let _ = registry.invoke(&CommandInvocation::new(
                        command.id,
                        InvocationSource::Shortcut,
                    ));
                }
            }
        });
    }
}

/// New Document callback shared by the toolbar, menu and palette.
pub(super) fn wire_new_document(app: &WriterApp, state: &Rc<GuiState>) {
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_new_doc(move || {
            if let Some(app) = app_ref.upgrade() {
                let guard = state
                    .registry
                    .lock()
                    .unwrap()
                    .invoke(&CommandInvocation::new(
                        "file.new",
                        InvocationSource::Toolbar,
                    ));
                if guard.is_err() {
                    return;
                }
                if request_document_replacement(&app, &state, PendingReplacement::NewDocument) {
                    return;
                }
                begin_new_document(&app);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions_tests::{test_state, text_document};

    /// Fails the whole test binary instead of hanging forever if the palette
    /// handler deadlocks on the non-re-entrant registry mutex.
    struct Watchdog(std::sync::mpsc::Sender<()>);

    fn watchdog() -> Watchdog {
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        std::thread::spawn(move || {
            if rx.recv_timeout(std::time::Duration::from_secs(20)).is_err() {
                eprintln!("DEADLOCK: palette dispatch did not return within 20 s");
                std::process::exit(101);
            }
        });
        Watchdog(tx)
    }

    impl Drop for Watchdog {
        fn drop(&mut self) {
            let _ = self.0.send(());
        }
    }

    fn palette_app(document: WriterDocument, clean: bool) -> (WriterApp, Rc<GuiState>) {
        let dialogs: Rc<dyn FileDialogService> =
            Rc::new(loom_desktop::ScriptedFileDialogs::new([None], [None]));
        let (app, state) = test_state(document.clone(), dialogs);
        if !clean {
            *state.last_saved.borrow_mut() = text_document("saved baseline");
        }
        wire_writer_shared_callbacks(&app, &state, None);
        wire(&app, &state);
        wire_new_document(&app, &state);
        apply_state(&app, &state);
        (app, state)
    }

    fn run_palette(app: &WriterApp, query: &str) {
        app.invoke_open_palette();
        app.set_palette_query(query.into());
        app.invoke_palette_query_changed(query.into());
        app.invoke_palette_invoked(0);
    }

    #[test]
    fn palette_new_document_on_dirty_document_opens_prompt_and_closes_palette() {
        let _guard = watchdog();
        let (app, state) = palette_app(text_document("unsaved work"), false);
        run_palette(&app, "new document");
        assert!(!app.get_palette_open(), "palette must close");
        assert!(app.get_save_changes_open(), "Save changes prompt must show");
        assert_eq!(
            state.pending_replacement.get(),
            Some(PendingReplacement::NewDocument)
        );
        assert_eq!(state.current.borrow().editor_text(), "unsaved work");
    }

    #[test]
    fn palette_new_document_on_clean_document_opens_template_chooser() {
        let _guard = watchdog();
        let (app, state) = palette_app(text_document("saved text"), true);
        run_palette(&app, "new document");
        assert!(!app.get_palette_open());
        assert!(!app.get_save_changes_open());
        assert!(app.get_template_chooser_open(), "chooser must open");
        assert_eq!(state.current.borrow().editor_text(), "saved text");
    }

    #[test]
    fn palette_registry_lock_is_free_after_every_modal_command() {
        let _guard = watchdog();
        for query in [
            "open",
            "new document",
            "export pdf",
            "export docx",
            "save as",
        ] {
            let (app, state) = palette_app(text_document("unsaved work"), false);
            run_palette(&app, query);
            assert!(state.registry.try_lock().is_ok(), "{query}: lock leaked");
        }
    }

    #[test]
    fn palette_search_finds_the_markdown_export_command() {
        use slint::Model;
        let _guard = watchdog();
        let (app, _state) = palette_app(text_document("unsaved work"), true);
        app.invoke_open_palette();
        app.invoke_palette_query_changed("Markdown".into());
        let commands = app.get_palette_commands();
        let labels: Vec<String> = (0..commands.row_count())
            .filter_map(|row| commands.row_data(row))
            .map(|item| item.label.to_string())
            .collect();
        assert!(
            labels.iter().any(|label| label == "Export Markdown (.md)"),
            "searching Markdown lists the export command, got {labels:?}"
        );
    }

    #[test]
    fn menu_new_document_on_dirty_document_does_not_hold_the_registry() {
        let _guard = watchdog();
        let (app, state) = palette_app(text_document("unsaved work"), false);
        assert!(dispatch_command(&app, "file.new"));
        assert!(app.get_save_changes_open());
        assert!(state.registry.try_lock().is_ok());
    }
}
