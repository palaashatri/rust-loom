//! Rules that decide when a Present deck counts as edited and which keystrokes
//! count as typing. The window code asks these; they never touch the window.

use std::rc::Rc;

use loom_present_core::{ElementType, PresentationDocument};
use slint::ComponentHandle;

use crate::{file_title, set_status, GuiState, PendingReplacement, PresentApp};

/// Connects the window's key hook. A typed letter that reaches the window, while
/// one text object is selected, moves keyboard focus to that object and says how
/// to edit it, so the letter is explained instead of silently dropped.
pub(crate) fn wire(app: &PresentApp, state: &Rc<GuiState>) {
    let state = state.clone();
    let app_ref = app.as_weak();
    app.on_window_key_unhandled(move |text| {
        let Some(app) = app_ref.upgrade() else {
            return;
        };
        if app.get_is_preview_mode() || !is_typed_character(&text) {
            return;
        }
        let editable = {
            let session = state.session.borrow();
            match session.selected_elements.as_slice() {
                [id] => session
                    .document
                    .active_slide()
                    .and_then(|slide| slide.elements.iter().find(|element| &element.id == id))
                    .is_some_and(|element| element.element_type != ElementType::Picture),
                _ => false,
            }
        };
        if editable {
            app.invoke_focus_editor();
            crate::set_status(&app, "Press Enter or F2 to edit the selected text");
        } else {
            // Nothing editable is selected, so the letter has nowhere to go: say so.
            crate::set_status(&app, "Select a text object, then press Enter or F2 to type");
        }
    });
}

/// Whether two documents hold the same deck content. The active slide is view
/// state, so moving between slides is not an edit and is not compared here.
pub(crate) fn content_matches(left: &PresentationDocument, right: &PresentationDocument) -> bool {
    left.id == right.id
        && left.title == right.title
        && left.author == right.author
        && left.theme == right.theme
        && left.slides.len() == right.slides.len()
        && left.slides.iter().zip(&right.slides).all(|(left, right)| {
            left.id == right.id
                && left.title == right.title
                && left.layout == right.layout
                && left.elements == right.elements
                && left.speaker_notes == right.speaker_notes
                && left.bg_color == right.bg_color
        })
}

/// Whether `text` is one character the user typed. Control keys (Tab, Return,
/// Escape) and the private-use codes Slint uses for arrows and F-keys are not.
pub(crate) fn is_typed_character(text: &str) -> bool {
    let mut chars = text.chars();
    match (chars.next(), chars.next()) {
        (Some(character), None) => {
            !character.is_control() && !('\u{e000}'..='\u{f8ff}').contains(&character)
        }
        _ => false,
    }
}

/// Selection and the open slide are view state, so only content counts as an edit.
pub(crate) fn deck_is_dirty(state: &GuiState) -> bool {
    let session = state.session.borrow();
    !content_matches(&session.document, &state.last_saved.borrow())
        || session.transitions != *state.last_saved_transitions.borrow()
}

/// Asks Save, Discard or Cancel before a dirty deck is replaced or closed. Returns
/// false when there is nothing to ask about, so the caller may go ahead.
pub(crate) fn request_deck_replacement(
    app: &PresentApp,
    state: &GuiState,
    operation: PendingReplacement,
) -> bool {
    if !deck_is_dirty(state) {
        return false;
    }
    state.pending_replacement.set(Some(operation));
    app.set_save_changes_document(
        file_title::display_title(
            state.save_path.borrow().as_deref(),
            &state.session.borrow().document.title,
        )
        .into(),
    );
    let closing = operation == PendingReplacement::CloseWindow;
    app.set_save_changes_closing(closing);
    app.set_save_changes_open(true);
    // The status names the same button the dialog shows for this question.
    set_status(
        app,
        if closing {
            "Unsaved changes — choose Save and close, Discard, or Cancel"
        } else {
            "Unsaved changes — choose Save, Discard, or Cancel"
        },
    );
    true
}

#[cfg(test)]
mod tests {
    use super::is_typed_character;

    #[test]
    fn a_letter_is_typed_but_control_and_navigation_keys_are_not() {
        assert!(is_typed_character("x"));
        assert!(is_typed_character(" "));
        assert!(!is_typed_character("\t"));
        assert!(!is_typed_character("\n"));
        assert!(!is_typed_character("\u{1b}"));
        assert!(!is_typed_character("\u{f700}"), "an arrow key");
        assert!(!is_typed_character("\u{f705}"), "F2");
        assert!(!is_typed_character(""));
        assert!(!is_typed_character("ab"));
    }
}
