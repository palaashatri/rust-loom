//! Enter and Backspace on list items: Enter on an empty item leaves the list,
//! and Backspace at the start of an item removes its marker before it joins
//! the paragraph above (the text editor alone would do neither).

use slint::SharedString;

use crate::{apply_with_history, GuiState, HistoryKind, WriterApp};

/// Handles the key when it is one of those two list gestures; returns whether
/// the key was consumed. Modified keys and non-collapsed selections are left to
/// the editor.
pub(crate) fn handle(app: &WriterApp, state: &GuiState, key: &str, ctrl: bool, alt: bool) -> bool {
    if ctrl || alt {
        return false;
    }
    let is = |code: slint::platform::Key| {
        let text: SharedString = code.into();
        key == text.as_str()
    };
    let enter = is(slint::platform::Key::Return) || key == "\n";
    let backspace = is(slint::platform::Key::Backspace);
    if !enter && !backspace {
        return false;
    }
    let selection = state.current.borrow().selection();
    if selection.anchor != selection.focus {
        return false;
    }
    let caret = selection.focus;
    let mut next = state.current.borrow().clone();
    let done = if enter {
        next.exit_empty_list_item_at(caret)
    } else {
        next.unlist_block_at(caret)
    };
    if !done {
        return false;
    }
    apply_with_history(app, state, next, HistoryKind::DocumentAction);
    app.set_status_right(SharedString::from(if enter {
        "Left the list"
    } else {
        "List marker removed"
    }));
    true
}
