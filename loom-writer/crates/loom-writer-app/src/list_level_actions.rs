//! The menu and palette commands for list levels and the two breaks. The keys
//! reach the same logic through the editor's key chain (`list_keys.rs` for
//! Tab, `break_keys.rs` for Enter); a command runs it against the window's
//! state the way `picture_actions` does.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use slint::SharedString;

use crate::table_actions::break_keys;
use crate::{apply_with_history, GuiState, HistoryKind, WriterApp};

/// Increases the level of the list items the selection touches.
pub(crate) const INCREASE_LEVEL: &str = "list.level.increase";
/// Decreases the level of the list items the selection touches.
pub(crate) const DECREASE_LEVEL: &str = "list.level.decrease";
/// Ends the page at the caret, as Ctrl+Enter does.
pub(crate) const PAGE_BREAK: &str = "insert.page_break";
/// Ends the line at the caret, as Shift+Enter does.
pub(crate) const LINE_BREAK: &str = "insert.line_break";

std::thread_local! {
    /// The window whose menu or palette issued a command.
    static STATE: RefCell<Option<Weak<GuiState>>> = const { RefCell::new(None) };
}

/// Remembers the window the commands act on. Called once when it is wired.
pub(crate) fn wire(state: &Rc<GuiState>) {
    STATE.with(|slot| *slot.borrow_mut() = Some(Rc::downgrade(state)));
}

/// Runs `id` if it is one of this module's commands; returns whether it was.
pub(crate) fn dispatch(app: &WriterApp, id: &str) -> bool {
    if !matches!(
        id,
        INCREASE_LEVEL | DECREASE_LEVEL | PAGE_BREAK | LINE_BREAK
    ) {
        return false;
    }
    let Some(state) = STATE.with(|slot| slot.borrow().as_ref().and_then(Weak::upgrade)) else {
        return false;
    };
    match id {
        INCREASE_LEVEL | DECREASE_LEVEL => {
            let deeper = id == INCREASE_LEVEL;
            if !change_level(app, &state, deeper) {
                app.set_status_right(SharedString::from(
                    "Put the caret in a list item to change its level",
                ));
            }
        }
        PAGE_BREAK => break_keys::insert_break(app, &state, false),
        _ => break_keys::insert_break(app, &state, true),
    }
    true
}

/// Moves the list items the selection touches one level deeper or shallower,
/// as one undo step. Returns whether the selection touches a list item at all:
/// that is when Tab and Shift+Tab belong to the list rather than the editor.
pub(crate) fn change_level(app: &WriterApp, state: &GuiState, deeper: bool) -> bool {
    let selection = state.current.borrow().selection();
    let mut next = state.current.borrow().clone();
    let shift = next.shift_list_levels(selection.anchor, selection.focus, deeper);
    if shift.items == 0 {
        return false;
    }
    if shift.changed == 0 {
        app.set_status_right(SharedString::from(if deeper {
            "The list is already at its deepest level"
        } else {
            "The list is already at its top level"
        }));
        return true;
    }
    if !apply_with_history(app, state, next, HistoryKind::DocumentAction) {
        return true;
    }
    let items = shift.changed;
    app.set_status_right(SharedString::from(format!(
        "List level changed for {items} list {}",
        if items == 1 { "item" } else { "items" }
    )));
    true
}
