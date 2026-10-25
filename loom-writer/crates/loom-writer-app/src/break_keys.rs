//! Shift+Enter ends the line without ending the paragraph (a vertical tab in
//! the text); Ctrl+Enter ends the page: the paragraph closes with a form feed
//! and the text after the caret starts a new paragraph on the next page. Both
//! are ordinary text edits, so Backspace and Delete remove them like any
//! character and each insertion is one undo step. Enter with the caret in a
//! table cell does what Shift+Enter does there: it starts a new line in that
//! cell, because a table cell holds lines, not paragraphs.

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
    let soft = match (shift, ctrl) {
        (true, false) => true,
        (false, true) => false,
        (false, false) if caret_in_table_cell(state) => true,
        _ => return false,
    };
    insert_break(app, state, soft);
    true
}

/// Whether the caret is a single point inside a table cell (not a selection).
fn caret_in_table_cell(state: &GuiState) -> bool {
    let current = state.current.borrow();
    let selection = current.selection();
    selection.anchor == selection.focus && current.table_cell_at_offset(selection.anchor).is_some()
}

/// Puts a line break (`soft`) or a page break at the selection, as one undo
/// step. The keys and the menu and palette commands both come here.
pub(crate) fn insert_break(app: &WriterApp, state: &GuiState, soft: bool) {
    let insertion = if soft {
        SOFT_BREAK.to_string()
    } else {
        format!("{PAGE_BREAK}\n")
    };
    let selection = state.current.borrow().selection();
    let (low, high) = (
        selection.anchor.min(selection.focus),
        selection.anchor.max(selection.focus),
    );
    let mut next = state.current.borrow().clone();
    // A page break cannot sit inside a table cell. A line break can: it stays in
    // that cell, which draws it on a new line.
    if !soft && next.table_cell_at_offset(low).is_some() {
        app.set_status_right(SharedString::from(
            "A page break cannot go inside a table cell",
        ));
        return;
    }
    let text = next.editor_text();
    let (Some(before), Some(after)) = (text.get(..low), text.get(high..)) else {
        return;
    };
    let edited = format!("{before}{insertion}{after}");
    let caret = low + insertion.len();
    match next.replace_editor_text_at(&edited, Some(caret)) {
        Ok(_) => {
            next.set_selection(TextSelection::caret(caret));
            apply_with_history(app, state, next, HistoryKind::DocumentAction);
            app.set_status_right(SharedString::from(if soft {
                "Line break inserted"
            } else {
                "Page break inserted"
            }));
        }
        Err(error) => app.set_status_right(SharedString::from(format!("Not inserted: {error}"))),
    }
}

#[cfg(test)]
#[path = "break_keys_tests.rs"]
mod tests;
