//! Present's appearance choice (System, Light, Dark, High Contrast): the View
//! menu rows, the command-palette entries, the toolbar View menu and the saved
//! setting all go through the shared bindings in `loom-desktop`; this file only
//! names the application. The presenter window mirrors every change.

use crate::PresentApp;

/// The id under which Present remembers its settings.
pub(crate) const APPLICATION_ID: &str = "org.loom.present";

loom_desktop::appearance_bindings!(PresentApp, set_status_left);
