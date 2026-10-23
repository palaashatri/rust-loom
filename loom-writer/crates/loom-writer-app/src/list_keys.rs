//! Tab and Shift+Tab on list items change the item's level. Enter and
//! Backspace at the start of a list item first move a nested item up one level,
//! and then remove its marker (Enter on an empty top-level item leaves the
//! list). The text editor alone would do none of this.

use slint::SharedString;

use crate::list_level_actions;
use crate::{apply_with_history, GuiState, HistoryKind, WriterApp};

/// Handles the key when it is one of those list gestures; returns whether the
/// key was consumed. Other keys, Tab outside a list item, and non-collapsed
/// selections for Enter and Backspace are left to the editor.
pub(crate) fn handle(
    app: &WriterApp,
    state: &GuiState,
    key: &str,
    shift: bool,
    ctrl: bool,
    alt: bool,
) -> bool {
    if ctrl || alt {
        return false;
    }
    let is = |code: slint::platform::Key| {
        let text: SharedString = code.into();
        key == text.as_str()
    };
    if is(slint::platform::Key::Tab) {
        return list_level_actions::change_level(app, state, !shift);
    }
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
    let nested = state
        .current
        .borrow()
        .list_level_at(caret)
        .is_some_and(|level| level > 0);
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
    app.set_status_right(SharedString::from(if nested {
        "List level decreased"
    } else if enter {
        "Left the list"
    } else {
        "List marker removed"
    }));
    true
}
