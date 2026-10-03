//! The in-window menu for Present (Windows and Linux). Projection, keyboard
//! navigation, and dispatch are shared (`loom_desktop::local_menu_bindings!`);
//! this file only says which commands Present handles.

use crate::PresentApp;

/// Commands Present's menu bar enables; the same list gates the native menu.
pub(crate) const SUPPORTED_COMMANDS: &[&str] = &[
    "file.new",
    "file.new_sample",
    "file.open",
    "file.save",
    "file.save_as",
    "file.export_pdf",
    "file.export_pptx",
    "edit.undo",
    "edit.redo",
    "slide.new",
    "slide.insert_image",
    "slide.duplicate",
    "slide.delete",
    "slide.prev",
    "slide.next",
    "view.inspector",
    "app.palette",
];

loom_desktop::local_menu_bindings!(PresentApp, SUPPORTED_COMMANDS, set_status_right);
