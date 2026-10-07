//! Reordering slides. The Slide menu, the command palette, Ctrl+Alt+Up and
//! Ctrl+Alt+Down, the strip's buttons, Ctrl+Up and Ctrl+Down on a focused
//! thumbnail and dragging a thumbnail all end in [`move_to`], so each move is
//! one undoable edit, selection follows the moved slide, and the same
//! announcement is made.

use std::rc::Rc;

use slint::ComponentHandle;

use loom_desktop::CommandStateProjection;
use loom_present_core::PresentationSession;

use crate::{refresh, set_status, GuiState, PresentApp};

pub(crate) const MOVE_UP: &str = "slide.move_up";
pub(crate) const MOVE_DOWN: &str = "slide.move_down";

/// Whether `id` is one of the two reorder commands.
pub(crate) fn is_command(id: &str) -> bool {
    matches!(id, MOVE_UP | MOVE_DOWN)
}

/// Runs a reorder command; false for any other id.
pub(crate) fn dispatch(app: &PresentApp, id: &str) -> bool {
    match id {
        MOVE_UP => app.invoke_move_slide_up(),
        MOVE_DOWN => app.invoke_move_slide_down(),
        _ => return false,
    }
    true
}

/// Enables each reorder command only when the selected slide can move that way.
pub(crate) fn project(projection: &mut CommandStateProjection, session: &PresentationSession) {
    for (id, enabled) in [
        (MOVE_UP, session.can_move_active_slide_up()),
        (MOVE_DOWN, session.can_move_active_slide_down()),
    ] {
        if let Some(mut item) = projection.get(id).cloned() {
            item.enabled = enabled;
            projection.insert(item);
        }
    }
}

/// Mirrors the session's movability into the window (strip buttons, palette).
pub(crate) fn sync(app: &PresentApp, session: &PresentationSession) {
    app.set_can_move_slide_up(session.can_move_active_slide_up());
    app.set_can_move_slide_down(session.can_move_active_slide_down());
}

/// What is said, and shown in the status bar, after a move.
pub(crate) fn moved_text(from: usize, to: usize) -> String {
    format!("Moved slide {} to position {}", from + 1, to + 1)
}

/// Moves slide `from` to position `to` as one undoable edit and selects it.
/// Returns false, changing nothing, when the move would do nothing.
pub(crate) fn move_to(app: &PresentApp, state: &GuiState, from: usize, to: usize) -> bool {
    let was_active = state.session.borrow().document.active_index == from;
    {
        let mut session = state.session.borrow_mut();
        if !session.move_slide(from, to) {
            return false;
        }
        // Elements selected on another slide do not belong to the moved one.
        if was_active {
            session.prune_selection();
        } else {
            session.clear_selection();
        }
    }
    if !was_active {
        state.selected_element.set(0);
    }
    refresh(app, state);
    let text = moved_text(from, to);
    app.set_slide_announcement(text.as_str().into());
    set_status(app, text);
    true
}

/// Moves the selected slide one place (`-1` earlier, `1` later), or says why not.
fn step(app: &PresentApp, state: &GuiState, delta: isize) {
    let (from, total) = {
        let session = state.session.borrow();
        (session.document.active_index, session.document.len())
    };
    let target = from.checked_add_signed(delta).filter(|to| *to < total);
    match target {
        Some(to) => {
            move_to(app, state, from, to);
        }
        None => {
            let text = if total < 2 {
                "There is only one slide".to_string()
            } else if delta < 0 {
                format!("Slide {} is already first", from + 1)
            } else {
                format!("Slide {} is already last", from + 1)
            };
            app.set_slide_announcement(text.as_str().into());
            set_status(app, text);
        }
    }
}

pub(crate) fn wire(app: &PresentApp, state: &Rc<GuiState>) {
    for delta in [-1_isize, 1] {
        let state = state.clone();
        let app_ref = app.as_weak();
        let handler = move || {
            if let Some(app) = app_ref.upgrade() {
                step(&app, &state, delta);
            }
        };
        if delta < 0 {
            app.on_move_slide_up(handler);
        } else {
            app.on_move_slide_down(handler);
        }
    }
    let state = state.clone();
    let app_ref = app.as_weak();
    app.on_reorder_slide(move |from, to| {
        let (Ok(from), Ok(to)) = (usize::try_from(from), usize::try_from(to)) else {
            return;
        };
        if let Some(app) = app_ref.upgrade() {
            move_to(&app, &state, from, to);
        }
    });
}
