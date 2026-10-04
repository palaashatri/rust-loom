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

pub(crate) fn sync(app: &crate::PresentApp, state: &crate::GuiState) {
    let title = display_title(
        state.save_path.borrow().as_deref(),
        &state.session.borrow().document.title,
    );
    app.set_deck_title(title.into());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::*;

    #[test]
    fn saving_renames_the_window_and_close_prompt_without_touching_the_deck() {
        set_platform();
        let dir = std::env::temp_dir().join(format!("loom-deck-title-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("qa_deck.loomdeck");
        let app = PresentApp::new().expect("create PresentApp");
        let state = Rc::new(GuiState {
            session: RefCell::new(empty_session()),
            last_saved: RefCell::new(empty_session().document.clone()),
            last_saved_transitions: RefCell::default(),
            pending_replacement: Cell::new(None),
            selected_element: Cell::new(0),
            inspector_available: Cell::new(true),
            save_path: RefCell::new(None),
            dialogs: Rc::new(loom_desktop::ScriptedFileDialogs::new(
                [],
                [Some(path.clone())],
            )),
            deck_filter: FileFilter::new("Deck", ["loomdeck"]).expect("filter"),
            pdf_filter: FileFilter::new("PDF", ["pdf"]).expect("filter"),
            menu_service: None,
            drag_state: RefCell::new(DragState::default()),
        });
        wire_app_callbacks(&app, &state);
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
}
