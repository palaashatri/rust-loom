//! Insert Picture through the real callbacks: the block, the page canvas,
//! undo, cancellation and refusal.

use crate::actions_tests::test_state;
use crate::*;
use std::path::PathBuf;
use std::rc::Rc;

/// A PNG of the given size written to a fresh temporary file.
fn png_file(tag: &str, width: u32, height: u32) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("loom-picture-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("photo.png");
    image::RgbaImage::from_pixel(width, height, image::Rgba([200, 60, 40, 255]))
        .save(&path)
        .unwrap();
    path
}

fn app_choosing(choice: Option<PathBuf>) -> (WriterApp, Rc<GuiState>) {
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([choice], [None]));
    let mut doc = WriterDocument::new("pic", "Pictures");
    doc.replace_paragraphs("First paragraph\nSecond paragraph");
    let (app, state) = test_state(doc, dialogs);
    wire_writer_shared_callbacks(&app, &state, None);
    (app, state)
}

fn pictures(state: &GuiState) -> usize {
    state
        .current
        .borrow()
        .blocks
        .iter()
        .filter(|block| block.is_image())
        .count()
}

#[test]
fn inserting_a_picture_adds_one_block_the_page_draws_and_undo_removes_it() {
    let path = png_file("insert", 240, 120);
    let (app, state) = app_choosing(Some(path));
    app.invoke_insert_image();
    assert_eq!(pictures(&state), 1, "one picture block was inserted");
    assert!(
        app.get_status_left()
            .contains("Inserted picture: photo.png"),
        "{}",
        app.get_status_left()
    );
    let shown = app.global::<PageImages>().get_items();
    assert_eq!(slint::Model::row_count(&shown), 1, "the page canvas has it");
    let item = slint::Model::row_data(&shown, 0).unwrap();
    assert!(item.width > 0.0 && item.height > 0.0);
    assert!((item.width / item.height - 2.0).abs() < 0.01, "aspect kept");
    assert!(item.label.contains("photo.png"));
    assert_eq!(item.source.size().width, 240, "the real pixels are shown");

    app.invoke_undo();
    assert_eq!(pictures(&state), 0, "undo removes the picture");
    assert_eq!(
        slint::Model::row_count(&app.global::<PageImages>().get_items()),
        0,
        "and the page stops drawing it"
    );
    app.invoke_redo();
    assert_eq!(pictures(&state), 1, "redo brings it back");
}

#[test]
fn a_cancelled_chooser_changes_nothing() {
    let (app, state) = app_choosing(None);
    let before = state.current.borrow().clone();
    app.invoke_insert_image();
    assert_eq!(*state.current.borrow(), before);
}

#[test]
fn a_file_that_is_not_a_picture_is_refused_with_a_reason() {
    let dir = std::env::temp_dir().join(format!("loom-picture-bad-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("notes.png");
    std::fs::write(&path, b"this is text, not a picture").unwrap();
    let (app, state) = app_choosing(Some(path));
    let before = state.current.borrow().clone();
    app.invoke_insert_image();
    assert_eq!(*state.current.borrow(), before, "the document is untouched");
    assert!(
        app.get_status_left().contains("only PNG and JPEG")
            || app.get_status_left().contains("could not read"),
        "{}",
        app.get_status_left()
    );
}

#[test]
fn a_picture_lands_below_the_caret_paragraph_and_the_next_one_is_typeable() {
    let path = png_file("order", 60, 60);
    let (app, state) = app_choosing(Some(path));
    app.invoke_insert_image();
    let doc = state.current.borrow();
    let kinds: Vec<bool> = doc.blocks.iter().map(|b| b.is_image()).collect();
    let at = kinds.iter().position(|is| *is).unwrap();
    assert!(at <= 1, "inserted at the caret, not the end: {kinds:?}");
    let caret = doc.selection().anchor;
    let text = doc.editor_text();
    assert!(caret <= text.len() && text.is_char_boundary(caret));
}

/// Writes a document with two pictures for a visual check: set
/// `LOOM_PICTURE_DOC=<path>` and run with `--ignored`.
#[test]
#[ignore]
fn write_a_document_with_pictures() {
    let Ok(out) = std::env::var("LOOM_PICTURE_DOC") else {
        return;
    };
    let mut doc = WriterDocument::new("pic", "Pictures");
    doc.replace_paragraphs("A heading above the picture\nBody text before the first picture, long enough to wrap across the width of the page so the layout is clearly text.\nBody text after.");
    let wide = loom_writer_core::InlineImage::from_bytes(
        "wide.png",
        std::fs::read(png_file("vis-wide", 600, 220)).unwrap(),
    )
    .unwrap();
    let tall = loom_writer_core::InlineImage::from_bytes(
        "tall.png",
        std::fs::read(png_file("vis-tall", 200, 260)).unwrap(),
    )
    .unwrap();
    let first_end = doc.editor_text().find('\n').unwrap();
    doc.insert_image_block(first_end + 40, wide).unwrap();
    let end = doc.editor_text().len();
    doc.insert_image_block(end, tall).unwrap();
    std::fs::write(out, loom_writer_core::save_document(&doc).unwrap()).unwrap();
}

#[test]
fn a_picture_is_not_counted_as_a_paragraph_in_the_status_bar() {
    let path = png_file("count", 50, 50);
    let (app, state) = app_choosing(Some(path));
    app.invoke_insert_image();
    let text_blocks = state
        .current
        .borrow()
        .blocks
        .iter()
        .filter(|block| !block.is_image())
        .count();
    let total_blocks = state.current.borrow().blocks.len();
    assert_eq!(total_blocks, text_blocks + 1, "one block is the picture");
    // The status line shows the insertion message until the next edit; ask
    // for the counts the way every refresh does.
    apply_document(&app, &state.current.borrow().clone());
    let status = app.get_status_left().to_string();
    assert!(
        status.contains(&format!("{text_blocks} paragraph")),
        "{status} should count {text_blocks} paragraphs, not {total_blocks}"
    );
}

#[test]
fn picture_commands_resize_the_picture_under_the_caret_and_undo() {
    let (app, state) = app_choosing(Some(png_file("resize", 240, 120)));
    app.invoke_insert_image();
    let width = |state: &GuiState| {
        state
            .current
            .borrow()
            .blocks
            .iter()
            .find_map(|b| b.image.as_ref().map(|i| i.width_pt))
            .unwrap()
    };
    let natural = width(&state);
    assert!(crate::dispatch_command(&app, "picture.larger"));
    assert!((width(&state) - natural * 1.25).abs() < 0.01);
    assert!(crate::dispatch_command(&app, "picture.smaller"));
    assert!((width(&state) - natural).abs() < 0.01);
    assert!(crate::dispatch_command(&app, "picture.larger"));
    assert!(crate::dispatch_command(&app, "picture.reset_size"));
    assert!((width(&state) - natural).abs() < 0.01);
    app.invoke_undo();
    assert!(
        (width(&state) - natural * 1.25).abs() < 0.01,
        "undo restores the larger size"
    );
}
