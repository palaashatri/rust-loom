//! Window and prompt title derived from the saved file name.

use std::path::Path;

/// The saved file's stem, or the deck's own title while it has never been
/// saved. The deck's internal title is never changed.
pub(crate) fn display_title(save_path: Option<&Path>, deck_title: &str) -> String {
    save_path
        .and_then(|path| path.file_stem())
        .map(|stem| stem.to_string_lossy().into_owned())
        .filter(|stem| !stem.is_empty())
        .unwrap_or_else(|| deck_title.to_owned())
}

/// The OS window title: the deck's name, a `*` while there are unsaved changes,
/// then the application name. The in-app title bar shows the same unsaved state.
pub(crate) fn window_title(name: &str, dirty: bool) -> String {
    if dirty {
        format!("{name} * - Loom Present")
    } else {
        format!("{name} - Loom Present")
    }
}

pub(crate) fn sync(app: &crate::PresentApp, state: &crate::GuiState) {
    let title = display_title(
        state.save_path.borrow().as_deref(),
        &state.session.borrow().document.title,
    );
    app.set_os_window_title(window_title(&title, crate::deck_is_dirty(state)).into());
    app.set_deck_title(title.into());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::*;

    /// Builds a wired app whose next Save dialog returns `save_to`.
    fn wired_app(save_to: PathBuf) -> (PresentApp, Rc<GuiState>) {
        set_platform();
        let app = PresentApp::new().expect("create PresentApp");
        let state = Rc::new(GuiState {
            session: RefCell::new(empty_session()),
            last_saved: RefCell::new(empty_session().document.clone()),
            last_saved_transitions: RefCell::default(),
            pending_replacement: Cell::new(None),
            selected_element: Cell::new(0),
            inspector_available: Cell::new(true),
            save_path: RefCell::new(None),
            dialogs: Rc::new(loom_desktop::ScriptedFileDialogs::new([], [Some(save_to)])),
            deck_filter: FileFilter::new("Deck", ["loomdeck"]).expect("filter"),
            pdf_filter: FileFilter::new("PDF", ["pdf"]).expect("filter"),
            menu_service: None,
            drag_state: RefCell::new(DragState::default()),
        });
        wire_app_callbacks(&app, &state);
        (app, state)
    }

    #[test]
    fn title_bar_dirty_marker_follows_unsaved_edits() {
        let dir = std::env::temp_dir().join(format!("loom-deck-dirty-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("dirty_deck.loomdeck");
        let (app, state) = wired_app(path);
        refresh(&app, &state);
        assert!(
            !app.get_deck_dirty(),
            "a fresh session must not be marked dirty"
        );

        app.invoke_add_slide();
        assert!(
            app.get_deck_dirty(),
            "an edit must set the title bar dirty marker"
        );

        app.invoke_save_deck();
        assert!(
            !app.get_deck_dirty(),
            "saving must clear the title bar dirty marker"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn saving_renames_the_window_and_close_prompt_without_touching_the_deck() {
        let dir = std::env::temp_dir().join(format!("loom-deck-title-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("qa_deck.loomdeck");
        let (app, state) = wired_app(path);
        refresh(&app, &state);
        assert_eq!(app.get_deck_title().as_str(), "Untitled Presentation");
        app.invoke_save_deck();
        assert_eq!(app.get_deck_title().as_str(), "qa_deck");
        assert_eq!(
            state.session.borrow().document.title,
            "Untitled Presentation"
        );
        assert_eq!(display_title(None, "Deck"), "Deck");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_os_window_title_names_the_deck_and_marks_unsaved_work() {
        let dir = std::env::temp_dir().join(format!("loom-deck-window-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("qa_deck.loomdeck");
        let (app, state) = wired_app(path);
        refresh(&app, &state);
        assert_eq!(
            app.get_os_window_title().as_str(),
            "Untitled Presentation - Loom Present",
            "a new deck is named by its title"
        );

        app.invoke_add_slide();
        assert_eq!(
            app.get_os_window_title().as_str(),
            "Untitled Presentation * - Loom Present",
            "unsaved work is marked in the window title"
        );

        app.invoke_save_deck();
        assert_eq!(
            app.get_os_window_title().as_str(),
            "qa_deck - Loom Present",
            "saving names the window after the file and clears the mark"
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
