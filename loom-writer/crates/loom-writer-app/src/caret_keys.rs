//! Up, Down, Home and End along the lines the page shows.
//!
//! The hidden text input behind the page lays text out at one size, left
//! aligned and regular weight, so on the visible page it skips wrapped heading
//! lines and lands in the wrong column. These keys are taken here first and
//! resolved with the document flow; the input never sees them. Table cells keep
//! their own navigation, so a caret inside a table is left alone.

use std::cell::Cell;

use loom_writer_core::{visual_caret_move, TextSelection, VisualMove};
use slint::platform::Key;
use slint::SharedString;

use crate::{GuiState, WriterApp};

thread_local! {
    /// The column a run of vertical moves aims for: the caret offset the last
    /// move ended on and its x position. Valid only while the caret is still
    /// there, so any other action forgets it.
    static GOAL_COLUMN: Cell<Option<(usize, f32)>> = const { Cell::new(None) };
}

/// Handles Up, Down, Home and End (Shift extends), and Left and Right inside
/// right-to-left text; returns whether the key was consumed. Left and Right in
/// left-to-right text, and a plain Left or Right over a selection, are left to
/// the input. Ctrl and Alt combinations belong to the editor.
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
    let is = |code: Key| {
        let text: SharedString = code.into();
        key == text.as_str()
    };
    let kind = if is(Key::UpArrow) {
        VisualMove::Up
    } else if is(Key::DownArrow) {
        VisualMove::Down
    } else if is(Key::Home) {
        VisualMove::LineStart
    } else if is(Key::End) {
        VisualMove::LineEnd
    } else if is(Key::LeftArrow) {
        VisualMove::Left
    } else if is(Key::RightArrow) {
        VisualMove::Right
    } else {
        return false;
    };
    if crate::table_actions::get_table_cell_selection_range().is_some() {
        return false;
    }
    let vertical = matches!(kind, VisualMove::Up | VisualMove::Down);
    let (selection, target) = {
        let doc = state.current.borrow();
        let selection = doc.selection();
        if doc.table_cell_at_offset(selection.focus).is_some() {
            return false;
        }
        // A plain Left or Right over a selection collapses it; the input does that.
        if matches!(kind, VisualMove::Left | VisualMove::Right)
            && !shift
            && selection.anchor != selection.focus
        {
            return false;
        }
        let hint = GOAL_COLUMN
            .with(Cell::get)
            .filter(|(offset, _)| vertical && *offset == selection.focus)
            .map(|(_, x)| x);
        let Some(target) = visual_caret_move(&doc, selection.focus, selection.affinity, kind, hint)
        else {
            return false;
        };
        (selection, target)
    };
    GOAL_COLUMN.with(|goal| goal.set(vertical.then_some((target.offset, target.x))));

    let anchor = if shift {
        selection.anchor
    } else {
        target.offset
    };
    // The selection event keeps the document's affinity, so set the one this
    // move ends with first.
    state.current.borrow_mut().set_selection(TextSelection {
        anchor: selection.anchor,
        focus: selection.focus,
        affinity: target.affinity,
    });
    app.invoke_selection_changed(
        anchor.min(i32::MAX as usize) as i32,
        target.offset.min(i32::MAX as usize) as i32,
    );
    if (anchor, target.offset) == (selection.anchor, selection.focus) {
        // Only the affinity changed: the event saw no difference.
        crate::refresh_render_projection_of(app, state);
    }
    true
}

#[cfg(test)]
#[path = "rtl_caret_keys_tests.rs"]
mod rtl_caret_keys_tests;
