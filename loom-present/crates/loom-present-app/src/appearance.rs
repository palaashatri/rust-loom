//! Present's appearance choice (System, Light, Dark, High Contrast): the View
//! menu rows, the command-palette entries, the toolbar View menu and the saved
//! setting all go through the shared bindings in `loom-desktop`; this file only
//! names the application. The presenter window mirrors every change.

use crate::PresentApp;

/// The id under which Present remembers its settings.
pub(crate) const APPLICATION_ID: &str = "org.loom.present";

mod bindings {
    use crate::PresentApp;

    loom_desktop::appearance_bindings!(PresentApp, set_status_left);
}

pub(crate) use bindings::{apply_id, current, on_change, start};
#[cfg(test)]
pub(crate) use bindings::{install_store, start_with_store};

pub(crate) fn choose(app: &PresentApp, choice: loom_desktop::appearance::Appearance) {
    bindings::choose(app, choice);
    let status = app.get_status_left().to_string();
    crate::recovery_deferred::project_status(app, status);
}

pub(crate) fn dispatch(app: &PresentApp, id: &str) -> bool {
    let handled = bindings::dispatch(app, id);
    if handled {
        let status = app.get_status_left().to_string();
        crate::recovery_deferred::project_status(app, status);
    }
    handled
}

loom_desktop::ui_font_bindings!(PresentApp, crate::Theme);
