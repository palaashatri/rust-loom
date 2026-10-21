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

/// The picture the caret or selection is on: the block it sits in, or the one
/// just above the caret (where Insert Picture leaves it).
fn picture_at_caret(document: &loom_writer_core::WriterDocument) -> Option<u64> {
    let (start, end) = document.selection().normalized_range();
    let mut block_start = 0usize;
    let mut previous: Option<&loom_writer_core::RichBlock> = None;
    for block in &document.blocks {
        let block_end = block_start + block.text.len_bytes();
        if block.is_image()
            && start <= block_end
            && end >= block_start
            && (start < block_end || end > block_start || start == block_start)
        {
            return Some(block.id);
        }
        if start == block_start && end == start {
            if let Some(p) = previous.filter(|p| p.is_image()) {
                return Some(p.id);
            }
        }
        previous = Some(block);
        block_start = block_end + 1;
    }
    None
}

/// Picture size and description commands. `false` when `id` is not one of them.
pub(crate) fn run_picture_command(app: &WriterApp, state: &GuiState, id: &str) -> bool {
    let apply: fn(&mut loom_writer_core::WriterDocument, u64) -> bool = match id {
        "picture.larger" => |d, b| d.scale_image_block(b, 1.25),
        "picture.smaller" => |d, b| d.scale_image_block(b, 0.8),
        "picture.reset_size" => |d, b| d.reset_image_block_size(b),
        _ => return false,
    };
    let mut next = state.current.borrow().clone();
    let Some(block) = picture_at_caret(&next) else {
        app.set_status_left("Place the caret on a picture first".into());
        return true;
    };
    if apply(&mut next, block) {
        apply_with_history(app, state, next, HistoryKind::DocumentAction);
        app.set_status_left("Picture resized".into());
    } else {
        app.set_status_left("The picture is already at that size limit".into());
    }
    true
}

std::thread_local! {
    static STATE: std::cell::RefCell<Option<std::rc::Weak<GuiState>>> =
        const { std::cell::RefCell::new(None) };
}

/// Runs a picture command from the shared command dispatcher.
pub(crate) fn dispatch(app: &WriterApp, id: &str) {
    let state = STATE.with(|slot| slot.borrow().as_ref().and_then(std::rc::Weak::upgrade));
    if let Some(state) = state {
        run_picture_command(app, &state, id);
    }
}

/// Wires the Insert Picture command.
pub(crate) fn wire(app: &WriterApp, state: &Rc<GuiState>) {
    STATE.with(|slot| *slot.borrow_mut() = Some(Rc::downgrade(state)));
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

#[cfg(test)]
mod tests {
    use super::*;

    fn png() -> Vec<u8> {
        let mut bytes = Vec::new();
        image::RgbaImage::from_pixel(40, 20, image::Rgba([1, 2, 3, 255]))
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        bytes
    }

    #[test]
    fn picture_at_caret_finds_the_picture_above_or_under_the_caret() {
        let mut doc = loom_writer_core::WriterDocument::new("d", "D");
        doc.blocks
            .push(loom_writer_core::RichBlock::new(1, "paragraph", "a"));
        let image = InlineImage::from_bytes("p.png", png()).unwrap();
        let id = doc.insert_image_block(0, image).unwrap();
        // The picture lands after paragraph "a"; the caret goes to the empty
        // paragraph below it, which is where Insert Picture leaves it.
        let after = 1
            + 1
            + doc
                .blocks
                .iter()
                .find(|b| b.id == id)
                .unwrap()
                .text
                .len_bytes()
            + 1;
        doc.set_selection(TextSelection::caret(after));
        assert_eq!(picture_at_caret(&doc), Some(id));
        doc.set_selection(TextSelection::caret(1));
        assert_eq!(
            picture_at_caret(&doc),
            None,
            "caret at the end of the text above"
        );
        doc.set_selection(TextSelection::caret(3));
        assert_eq!(picture_at_caret(&doc), Some(id), "caret inside the picture");
    }
}
