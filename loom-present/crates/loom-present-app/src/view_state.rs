//! View menu toggles for the Navigator and Speaker Notes panes. The toolbar,
//! the View menu and the notes drawer's own close button all flip the same
//! window properties; this file keeps the menu's checkmarks in step.

use std::rc::Rc;

use slint::ComponentHandle;

use loom_desktop::CommandStateProjection;

use crate::{sync_menu_state, GuiState, PresentApp};

pub(crate) const NAVIGATOR: &str = "view.navigator";
pub(crate) const NOTES: &str = "view.notes";

/// Handles the two pane commands; false for any other id.
pub(crate) fn dispatch(app: &PresentApp, id: &str) -> bool {
    match id {
        NAVIGATOR => app.set_show_navigator(!app.get_show_navigator()),
        NOTES => app.set_show_notes_drawer(!app.get_show_notes_drawer()),
        _ => return false,
    }
    true
}

/// Sets each pane command's checkmark from the window state.
pub(crate) fn project(projection: &mut CommandStateProjection, app: &PresentApp) {
    for (id, checked) in [
        (NAVIGATOR, app.get_show_navigator()),
        (NOTES, app.get_show_notes_drawer()),
    ] {
        if let Some(mut item) = projection.get(id).cloned() {
            item.checked = Some(checked);
            projection.insert(item);
        }
    }
}

/// Re-reads the menu whenever any pane toggle changes, from any control.
pub(crate) fn wire(app: &PresentApp, state: &Rc<GuiState>) {
    let state = state.clone();
    let app_ref = app.as_weak();
    app.on_view_state_changed(move || {
        if let (Some(app), Some(menu_service)) = (app_ref.upgrade(), &state.menu_service) {
            sync_menu_state(menu_service, &app, &state);
        }
    });
}
