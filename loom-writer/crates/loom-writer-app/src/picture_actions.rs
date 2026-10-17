//! Insert Picture: choose a PNG or JPEG and place it as its own block at the
//! caret, as one undoable edit.

use std::path::Path;
use std::rc::Rc;

use loom_desktop::{FileFilter, OpenFileRequest};
use loom_writer_core::{InlineImage, TextSelection};
use slint::{ComponentHandle, SharedString};

use crate::{apply_with_history, GuiState, HistoryKind, WriterApp};

fn request(state: &GuiState) -> OpenFileRequest {
    OpenFileRequest {
        title: "Insert Picture".into(),
        initial_directory: state
            .save_path
            .borrow()
            .as_deref()
            .and_then(Path::parent)
            .filter(|parent| !parent.as_os_str().is_empty())
            .map(Path::to_path_buf),
        suggested_name: None,
        filters: FileFilter::new("Pictures", ["png", "jpg", "jpeg"])
            .into_iter()
            .collect(),
    }
}

/// Reads `path` and inserts it at the caret. The error is what the status line
/// says; nothing changes when it is returned.
pub(crate) fn insert_picture(app: &WriterApp, state: &GuiState, path: &Path) -> Result<(), String> {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "picture".into());
    let bytes = std::fs::read(path).map_err(|error| format!("Could not read {name}: {error}"))?;
    let image = InlineImage::from_bytes(&name, bytes)?;
    let mut next = state.current.borrow().clone();
    let caret = next.selection().anchor;
    let id = next
        .insert_image_block(caret, image)
        .ok_or("This document already holds the most pictures it can")?;
    // The caret goes to the start of the paragraph after the picture.
    let mut after = 0usize;
    for block in &next.blocks {
        after += block.text.len_bytes() + 1;
        if block.id == id {
            break;
        }
    }
    next.set_selection(TextSelection::caret(after.min(next.editor_text().len())));
    apply_with_history(app, state, next, HistoryKind::DocumentAction);
    app.set_status_left(SharedString::from(format!("Inserted picture: {name}")));
    Ok(())
}

/// Wires the Insert Picture command.
pub(crate) fn wire(app: &WriterApp, state: &Rc<GuiState>) {
    let state = state.clone();
    let app_ref = app.as_weak();
    app.on_insert_image(move || {
        let Some(app) = app_ref.upgrade() else { return };
        match state.dialogs.open_file(&request(&state)) {
            Ok(Some(path)) => {
                if let Err(message) = insert_picture(&app, &state, &path) {
                    app.set_status_left(SharedString::from(message));
                }
            }
            Ok(None) => {}
            Err(error) => app.set_status_left(SharedString::from(format!(
                "Could not open the picture chooser: {error}"
            ))),
        }
    });
}
