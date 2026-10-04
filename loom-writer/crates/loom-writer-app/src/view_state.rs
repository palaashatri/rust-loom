//! The outline navigator toggle. The toolbar View menu, the in-window View
//! menu and the navigator's own close button all flip the same window
//! property; this file keeps the menu's check mark and the outline list in
//! step with it.

use std::rc::Rc;
use std::sync::Arc;

use slint::ComponentHandle;

use loom_desktop::{CommandStateProjection, NativeMenuBar};

use crate::{outline, sync_menu_state, GuiState, WriterApp};

pub(crate) const NAVIGATOR: &str = "view.navigator";

/// Handles the navigator command; false for any other id.
pub(crate) fn dispatch(app: &WriterApp, id: &str) -> bool {
    if id != NAVIGATOR {
        return false;
    }
    app.set_show_navigator(!app.get_show_navigator());
    // The window's own change notification arrives later; the toggle must be
    // visible to the menu and the outline immediately.
    app.invoke_view_state_changed();
    true
}

/// Sets the navigator's check mark from the window state.
pub(crate) fn project(projection: &mut CommandStateProjection, app: &WriterApp) {
    if let Some(mut item) = projection.get(NAVIGATOR).cloned() {
        item.checked = Some(app.get_show_navigator());
        projection.insert(item);
    }
}

/// Re-reads the outline and the menu whenever the navigator is toggled, from
/// any control.
pub(crate) fn wire(
    app: &WriterApp,
    state: &Rc<GuiState>,
    menu_service: Option<Arc<NativeMenuBar>>,
) {
    let state = state.clone();
    let app_ref = app.as_weak();
    app.on_view_state_changed(move || {
        let Some(app) = app_ref.upgrade() else { return };
        outline::publish(&app, &state.current.borrow());
        if let Some(menu_service) = &menu_service {
            sync_menu_state(menu_service, &app, &state);
        }
    });
}
