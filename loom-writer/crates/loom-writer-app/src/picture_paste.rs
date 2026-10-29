//! Paste a picture from the system clipboard. A copy that holds no text (a
//! screenshot, an image from a paint program or a browser) lands as a picture
//! block at the caret, placed the way Insert Picture places one.

use loom_writer_core::{InlineImage, TextSelection};
use slint::SharedString;

use crate::{apply_with_history, GuiState, HistoryKind, WriterApp};

/// The name a pasted picture carries, as Insert Picture names its file.
pub(crate) const PASTED_PICTURE_NAME: &str = "Pasted picture";

/// Inserts the clipboard's picture, if it has one. `false` when the clipboard
/// holds no picture, so the caller can report nothing to paste. A picture that
/// is refused (undecodable, too large, or no room in the document) is reported
/// on the status line and leaves the document unchanged.
pub(crate) fn paste_picture(app: &WriterApp, state: &GuiState) -> bool {
    let bytes = match loom_clipboard::read_clipboard_image() {
        Ok(Some(bytes)) => bytes,
        _ => return false,
    };
    if let Err(message) = insert_pasted_picture(app, state, bytes) {
        app.set_status_left(SharedString::from(format!(
            "Pasted picture refused: {message}"
        )));
    }
    true
}

/// Validates the bytes, then inserts them at the caret as one undoable edit,
/// with the caret after the picture as Insert Picture leaves it.
fn insert_pasted_picture(app: &WriterApp, state: &GuiState, bytes: Vec<u8>) -> Result<(), String> {
    let image = InlineImage::from_bytes(PASTED_PICTURE_NAME, bytes)?;
    let mut next = state.current.borrow().clone();
    let caret = next.selection().anchor;
    let id = next
        .insert_image_block(caret, image)
        .ok_or("this document already holds the most pictures it can")?;
    let mut after = 0usize;
    for block in &next.blocks {
        after += block.text.len_bytes() + 1;
        if block.id == id {
            break;
        }
    }
    next.set_selection(TextSelection::caret(after.min(next.editor_text().len())));
    if !apply_with_history(app, state, next, HistoryKind::DocumentAction) {
        return Err(app.get_status_left().to_string());
    }
    app.set_status_left(SharedString::from("Pasted picture"));
    Ok(())
}

#[cfg(test)]
#[path = "picture_paste_tests.rs"]
mod tests;
