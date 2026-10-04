//! The in-window menu for Writer (Windows and Linux). Projection, keyboard
//! navigation, and dispatch are shared (`loom_desktop::local_menu_bindings!`);
//! this file only says which commands Writer handles.

use crate::WriterApp;

/// Commands Writer's menu bar enables; the same list gates the native menu.
pub(crate) const SUPPORTED_COMMANDS: &[&str] = &[
    "file.new",
    "file.open",
    "file.save",
    "file.save_as",
    "file.export_pdf",
    "file.export_docx",
    "file.export_md",
    "edit.undo",
    "edit.redo",
    "app.palette",
    "view.inspector",
    "view.navigator",
    "format.bold",
    "format.italic",
    "format.underline",
];

loom_desktop::local_menu_bindings!(WriterApp, SUPPORTED_COMMANDS, set_status_right);
