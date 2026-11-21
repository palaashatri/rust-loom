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

fn picture_name(state: &GuiState) -> Option<String> {
    state
        .current
        .borrow()
        .blocks
        .iter()
        .find(|block| block.is_image())
        .and_then(|block| block.image.as_ref())
        .map(|image| image.name.clone())
}

/// Inserts one picture named `photo.png` and leaves the caret on it.
fn app_with_one_picture() -> (WriterApp, Rc<GuiState>) {
    let (app, state) = app_choosing(Some(png_file("alt", 80, 40)));
    app.invoke_insert_image();
    assert_eq!(pictures(&state), 1);
    (app, state)
}

#[test]
fn picture_description_dialog_is_prefilled_and_save_applies_one_undoable_step() {
    let (app, state) = app_with_one_picture();
    assert!(crate::dispatch_command(&app, "picture.alt_text"));
    let dialog = app.global::<PictureAlt>();
    assert!(dialog.get_open(), "the dialog opened");
    assert_eq!(dialog.get_value(), "photo.png", "prefilled");

    dialog.invoke_save("A red square".into());
    assert!(!dialog.get_open(), "applying closes the dialog");
    assert_eq!(picture_name(&state).as_deref(), Some("A red square"));
    assert!(app.get_can_undo());

    app.invoke_undo();
    assert_eq!(
        picture_name(&state).as_deref(),
        Some("photo.png"),
        "one undo restores the previous description"
    );
}

#[test]
fn cancelling_the_picture_description_changes_nothing() {
    let (app, state) = app_with_one_picture();
    let before = state.current.borrow().clone();
    assert!(crate::dispatch_command(&app, "picture.alt_text"));
    let dialog = app.global::<PictureAlt>();
    dialog.set_value("Changed but cancelled".into());
    dialog.invoke_cancel();
    assert!(!dialog.get_open(), "cancel closes the dialog");
    assert_eq!(*state.current.borrow(), before, "nothing changed");
}

#[test]
fn a_blank_picture_description_is_refused_and_the_dialog_stays_open() {
    let (app, state) = app_with_one_picture();
    let before = state.current.borrow().clone();
    assert!(crate::dispatch_command(&app, "picture.alt_text"));
    let dialog = app.global::<PictureAlt>();
    dialog.invoke_save("   ".into());
    assert_eq!(*state.current.borrow(), before, "the description is kept");
    assert!(dialog.get_open(), "the user can type a description");
    assert!(
        app.get_status_left().contains("cannot be empty"),
        "{}",
        app.get_status_left()
    );
}

#[test]
fn picture_description_needs_the_caret_on_a_picture() {
    let (app, _state) = app_choosing(None);
    assert!(crate::dispatch_command(&app, "picture.alt_text"));
    assert!(
        !app.global::<PictureAlt>().get_open(),
        "no picture, no dialog"
    );
    assert!(
        app.get_status_left()
            .contains("Place the caret on a picture"),
        "{}",
        app.get_status_left()
    );
}

#[test]
fn typing_at_the_end_of_a_picture_line_is_refused_and_the_picture_keeps_its_text() {
    let path = png_file("typing", 240, 120);
    let (app, state) = app_choosing(Some(path));
    app.invoke_insert_image();
    assert_eq!(pictures(&state), 1, "one picture block was inserted");
    let picture_text = state
        .current
        .borrow()
        .blocks
        .iter()
        .find(|block| block.is_image())
        .unwrap()
        .text
        .as_str()
        .to_string();
    let text = state.current.borrow().editor_text();
    let end = text.find(&picture_text).unwrap() + picture_text.len();
    let mut typed = text.clone();
    typed.insert(end, 'x');
    app.invoke_document_edited(typed.as_str().into(), end as i32 + 1, end as i32 + 1);

    assert_eq!(pictures(&state), 1, "the picture is still a picture");
    let kept = state
        .current
        .borrow()
        .blocks
        .iter()
        .find(|block| block.is_image())
        .unwrap()
        .text
        .as_str()
        .to_string();
    assert_eq!(
        kept, picture_text,
        "the picture keeps exactly its own characters"
    );
    assert_eq!(
        state.current.borrow().editor_text(),
        text,
        "the typed text was refused"
    );
    assert!(
        app.get_status_left().contains("keeps its own line"),
        "{}",
        app.get_status_left()
    );
}

fn png_with_corrupt_image_data() -> Vec<u8> {
    let mut bytes = Vec::new();
    image::RgbaImage::from_pixel(120, 80, image::Rgba([40, 80, 120, 255]))
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .expect("encode PNG fixture");
    let idat = bytes
        .windows(4)
        .position(|chunk| chunk == b"IDAT")
        .expect("PNG has IDAT chunk");
    bytes[idat + 4] ^= 0xff;
    bytes
}

#[test]
fn corrupted_picture_surfaces_accessible_error_label_and_visible_fallback() {
    picture_view::reset_cache_for_tests(picture_view::default_cache_budget_bytes());
    let broken =
        loom_writer_core::InlineImage::from_bytes("broken.png", png_with_corrupt_image_data())
            .expect("valid PNG header passes initial insertion validation");
    assert!(broken.preview_rgba(1600).is_err());

    let mut doc = WriterDocument::new("broken-pic", "Broken picture");
    doc.replace_paragraphs("Above the picture");
    doc.insert_image_block(usize::MAX, broken)
        .expect("insert broken picture block");

    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], [None]));
    let (app, state) = test_state(doc, dialogs);
    wire_writer_shared_callbacks(&app, &state, None);
    apply_state(&app, &state);
    let _ = loom_test_support::capture::snapshot_component(&app, 1280.0, 800.0, 1.0)
        .expect("render document with corrupted picture fallback");

    let shown = app.global::<PageImages>().get_items();
    assert_eq!(slint::Model::row_count(&shown), 1);
    let item = slint::Model::row_data(&shown, 0).unwrap();
    assert_eq!(
        item.source.size().width,
        0,
        "failed decode emits empty image"
    );
    assert!(
        item.label.contains("broken.png") && item.label.contains("image unavailable"),
        "accessible label reports unreadable picture: {}",
        item.label
    );
    assert!(
        app.get_status_right().contains("Picture unavailable:"),
        "status surfaces the decode failure: {}",
        app.get_status_right()
    );

    let fallback_node =
        i_slint_backend_testing::ElementHandle::find_by_accessible_label(&app, item.label.as_str())
            .find(|element| {
                element.accessible_role() == Some(i_slint_backend_testing::AccessibleRole::Image)
            })
            .expect("fallback rectangle exposes accessible image role and error label");
    assert!(
        fallback_node.size().width > 10.0 && fallback_node.size().height > 10.0,
        "fallback placeholder keeps a visible bounding box"
    );
}

#[test]
fn oversized_picture_is_refused_with_accessible_error_label_and_fallback() {
    picture_view::reset_cache_for_tests(picture_view::default_cache_budget_bytes());
    let mut bytes = Vec::new();
    image::RgbaImage::from_pixel(4, 4, image::Rgba([10, 20, 30, 255]))
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .unwrap();
    let oversized = loom_writer_core::InlineImage {
        name: "huge.png".into(),
        format: loom_writer_core::ImageFormat::Png,
        width_px: 10_000,
        height_px: 10_000,
        width_pt: 240.0,
        height_pt: 180.0,
        bytes: std::sync::Arc::new(bytes),
    };

    let mut doc = WriterDocument::new("oversized-pic", "Oversized picture");
    doc.insert_image_block(0, oversized)
        .expect("insert oversized picture block");

    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], [None]));
    let (app, state) = test_state(doc, dialogs);
    wire_writer_shared_callbacks(&app, &state, None);
    apply_state(&app, &state);
    let _ = loom_test_support::capture::snapshot_component(&app, 1280.0, 800.0, 1.0)
        .expect("render oversized picture fallback");

    let shown = app.global::<PageImages>().get_items();
    let item = slint::Model::row_data(&shown, 0).unwrap();
    assert_eq!(item.source.size().width, 0);
    assert!(
        item.label.contains("huge.png") && item.label.contains("pixel limit"),
        "accessible label reports pixel limit refusal: {}",
        item.label
    );
    assert!(
        app.get_status_right().contains("Picture unavailable:")
            && app.get_status_right().contains("pixel limit"),
        "status reports pixel limit refusal: {}",
        app.get_status_right()
    );
}

#[test]
fn decoded_picture_cache_enforces_byte_budget_by_evicting_oldest_entries() {
    assert_eq!(
        picture_view::default_cache_budget_bytes(),
        32 * 1024 * 1024,
        "default decoded RGBA budget must be 32 MiB"
    );

    // Budget fits exactly two 64x64 RGBA images (2 * 64 * 64 * 4 = 32768 bytes).
    let two_images_budget = 2 * 64 * 64 * 4;
    picture_view::reset_cache_for_tests(two_images_budget);

    let make_image = |tag: &str| {
        let path = png_file(tag, 64, 64);
        loom_writer_core::InlineImage::from_bytes(
            &format!("{tag}.png"),
            std::fs::read(path).unwrap(),
        )
        .unwrap()
    };
    let img_a = make_image("cache-a");
    let img_b = make_image("cache-b");
    let img_c = make_image("cache-c");

    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], [None]));
    let (app, _state) = test_state(WriterDocument::new("cache", "Cache"), dialogs);

    let mut doc_ab = WriterDocument::new("cache-ab", "Cache AB");
    doc_ab.insert_image_block(0, img_a.clone()).unwrap();
    doc_ab
        .insert_image_block(usize::MAX, img_b.clone())
        .unwrap();
    apply_document(&app, &doc_ab);
    assert_eq!(picture_view::cached_entries_for_tests(), 2);
    assert_eq!(picture_view::cached_bytes_for_tests(), two_images_budget);

    // Touch img_a so img_b becomes the oldest (least recently used) entry.
    let mut doc_a = WriterDocument::new("cache-a", "Cache A");
    doc_a.insert_image_block(0, img_a.clone()).unwrap();
    apply_document(&app, &doc_a);

    // Adding img_c must evict img_b while retaining img_a and img_c within budget.
    let mut doc_c = WriterDocument::new("cache-c", "Cache C");
    doc_c.insert_image_block(0, img_c.clone()).unwrap();
    apply_document(&app, &doc_c);

    assert_eq!(picture_view::cached_entries_for_tests(), 2);
    assert_eq!(picture_view::cached_bytes_for_tests(), two_images_budget);
    assert!(
        picture_view::is_cached_for_tests(&img_a),
        "recently accessed entry stays cached"
    );
    assert!(
        !picture_view::is_cached_for_tests(&img_b),
        "oldest unused entry was evicted"
    );
    assert!(
        picture_view::is_cached_for_tests(&img_c),
        "newly decoded entry is cached"
    );
}
