//! Window and prompt title derived from the saved file name.

use std::path::Path;

/// The name shown to the user: the saved file's stem, or the document's own
/// title while it has never been saved. The document's internal title is never
/// changed.
pub(crate) fn display_title(save_path: Option<&Path>, document_title: &str) -> String {
    save_path
        .and_then(|path| path.file_stem())
        .map(|stem| stem.to_string_lossy().into_owned())
        .filter(|stem| !stem.is_empty())
        .unwrap_or_else(|| document_title.to_owned())
}

pub(crate) fn sync(app: &crate::WriterApp, state: &crate::GuiState) {
    let title = display_title(
        state.save_path.borrow().as_deref(),
        &state.current.borrow().title,
    );
    app.set_window_title(window_title(&title, crate::document_is_dirty(state)).into());
    app.set_doc_title(title.into());
}

/// The window title: the document's name, starred while it has unsaved
/// changes, then the application name.
pub(crate) fn window_title(name: &str, dirty: bool) -> String {
    if dirty {
        format!("{name} * - Loom Writer")
    } else {
        format!("{name} - Loom Writer")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions_tests::{test_state, text_document};

    #[test]
    fn saved_files_show_their_stem_and_unsaved_documents_their_title() {
        assert_eq!(display_title(None, "Untitled"), "Untitled");
        let path = std::env::temp_dir().join("Report.loomdoc");
        assert_eq!(display_title(Some(&path), "Untitled"), "Report");
    }

    #[test]
    fn the_window_title_names_the_document_and_marks_unsaved_work() {
        let dir = std::env::temp_dir().join(format!("loom-window-title-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("Report.loomdoc");
        let dialogs: std::rc::Rc<dyn loom_desktop::FileDialogService> = std::rc::Rc::new(
            loom_desktop::ScriptedFileDialogs::new([], [Some(path.clone())]),
        );
        let (app, state) = test_state(text_document("hello"), dialogs);
        crate::wire_writer_shared_callbacks(&app, &state, None);
        crate::apply_state(&app, &state);
        assert_eq!(app.get_window_title().as_str(), "Test - Loom Writer");
        state.current.borrow_mut().replace_paragraphs("changed");
        crate::apply_state(&app, &state);
        assert_eq!(app.get_window_title().as_str(), "Test * - Loom Writer");
        app.invoke_save_doc();
        assert_eq!(app.get_window_title().as_str(), "Report - Loom Writer");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn saving_renames_the_window_and_the_close_prompt_without_touching_the_document() {
        let dir = std::env::temp_dir().join(format!("loom-title-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("Report.loomdoc");
        let dialogs: std::rc::Rc<dyn loom_desktop::FileDialogService> = std::rc::Rc::new(
            loom_desktop::ScriptedFileDialogs::new([], [Some(path.clone())]),
        );
        let (app, state) = test_state(text_document("hello"), dialogs);
        crate::wire_writer_shared_callbacks(&app, &state, None);
        crate::apply_state(&app, &state);
        assert_eq!(app.get_doc_title().as_str(), "Test");
        app.invoke_save_doc();
        assert_eq!(app.get_doc_title().as_str(), "Report");
        assert_eq!(state.current.borrow().title, "Test");
        state.current.borrow_mut().replace_paragraphs("changed");
        assert!(crate::request_document_replacement(
            &app,
            &state,
            crate::PendingReplacement::CloseWindow
        ));
        assert_eq!(app.get_save_changes_document().as_str(), "Report");
        let _ = std::fs::remove_dir_all(dir);
    }
}
