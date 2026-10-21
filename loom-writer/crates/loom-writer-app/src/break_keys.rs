//! Shift+Enter ends the line without ending the paragraph (a vertical tab in
//! the text); Ctrl+Enter ends the page: the paragraph closes with a form feed
//! and the text after the caret starts a new paragraph on the next page. Both
//! are ordinary text edits, so Backspace and Delete remove them like any
//! character and each insertion is one undo step.

use loom_writer_core::TextSelection;
use slint::SharedString;

use crate::{apply_with_history, GuiState, HistoryKind, WriterApp};

/// The line-break character stored in a paragraph's text.
pub(crate) const SOFT_BREAK: char = '\u{b}';
/// The page-break character stored at the end of a paragraph's text.
pub(crate) const PAGE_BREAK: char = '\u{c}';

/// Handles the key when it is Shift+Enter or Ctrl+Enter; returns whether the
/// key was consumed.
pub(crate) fn handle(
    app: &WriterApp,
    state: &GuiState,
    key: &str,
    shift: bool,
    ctrl: bool,
    alt: bool,
) -> bool {
    let return_key: SharedString = slint::platform::Key::Return.into();
    if alt || (key != return_key.as_str() && key != "\n") {
        return false;
    }
    let insertion = match (shift, ctrl) {
        (true, false) => SOFT_BREAK.to_string(),
        (false, true) => format!("{PAGE_BREAK}\n"),
        _ => return false,
    };
    let selection = state.current.borrow().selection();
    let (low, high) = (
        selection.anchor.min(selection.focus),
        selection.anchor.max(selection.focus),
    );
    let mut next = state.current.borrow().clone();
    if next.table_cell_at_offset(low).is_some() {
        app.set_status_right(SharedString::from(
            "Line and page breaks cannot go inside a table cell",
        ));
        return true;
    }
    let text = next.editor_text();
    let (Some(before), Some(after)) = (text.get(..low), text.get(high..)) else {
        return false;
    };
    let edited = format!("{before}{insertion}{after}");
    let caret = low + insertion.len();
    match next.replace_editor_text_at(&edited, Some(caret)) {
        Ok(_) => {
            next.set_selection(TextSelection::caret(caret));
            apply_with_history(app, state, next, HistoryKind::DocumentAction);
            app.set_status_right(SharedString::from(if shift {
                "Line break inserted"
            } else {
                "Page break inserted"
            }));
        }
        Err(error) => app.set_status_right(SharedString::from(format!("Not inserted: {error}"))),
    }
    true
}

/// The markup text without the break characters, which draw as nothing.
pub(crate) fn without_breaks(markup: String) -> String {
    if markup.contains([SOFT_BREAK, PAGE_BREAK]) {
        markup.replace([SOFT_BREAK, PAGE_BREAK], "")
    } else {
        markup
    }
}

#[cfg(test)]
#[path = "break_keys_tests.rs"]
mod tests;
