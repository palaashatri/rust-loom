//! Sheets' appearance choice (System, Light, Dark, High Contrast): the View
//! menu rows, the command-palette entries, the toolbar View menu and the saved
//! setting all go through the shared bindings in `loom-desktop`; this file only
//! names the application.

use crate::SheetsApp;

/// The id under which Sheets remembers its settings.
pub(crate) const APPLICATION_ID: &str = "org.loom.sheets";

loom_desktop::appearance_bindings!(SheetsApp, set_status_left);
