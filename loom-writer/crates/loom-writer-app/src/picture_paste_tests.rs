//! Pasting a picture through the real Paste command: a picture-only clipboard
//! becomes one undoable picture; text keeps priority; bad images change nothing.

use crate::actions_tests::test_state;
use crate::*;
use std::rc::Rc;

/// Switches the in-memory clipboard on for one test and off again afterwards.
struct MockClipboard;

impl MockClipboard {
    fn new() -> Self {
        loom_clipboard::set_mock_clipboard_enabled(true);
        loom_clipboard::set_mock_clipboard_fails(false);
        Self
    }
}

impl Drop for MockClipboard {
    fn drop(&mut self) {
        loom_clipboard::set_mock_clipboard_image(None);
        loom_clipboard::set_mock_clipboard_fails(false);
        loom_clipboard::set_mock_clipboard_enabled(false);
    }
}

fn png() -> Vec<u8> {
    let mut bytes = Vec::new();
    image::RgbaImage::from_pixel(40, 20, image::Rgba([30, 90, 160, 255]))
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .expect("encode a small picture");
    bytes
}

fn writer_app(text: &str) -> (WriterApp, Rc<GuiState>) {
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], [None]));
    let mut doc = WriterDocument::new("picture-paste", "Paste");
    doc.replace_paragraphs(text);
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

fn text(state: &GuiState) -> String {
    state.current.borrow().editor_text().to_string()
}

#[test]
fn a_picture_only_clipboard_pastes_one_picture_and_undo_removes_it() {
    let _clipboard = MockClipboard::new();
    loom_clipboard::set_mock_clipboard_image(Some(png()));
    let (app, state) = writer_app("First paragraph");

    app.invoke_paste();

    assert_eq!(pictures(&state), 1, "the copied picture is in the document");
    assert!(
        text(&state).starts_with("First paragraph"),
        "the text is untouched: {:?}",
        text(&state)
    );
    let inserted = state
        .current
        .borrow()
        .blocks
        .iter()
        .find(|block| block.is_image())
        .and_then(|block| block.image.clone())
        .expect("the picture block carries its image");
    assert_eq!(inserted.name, "Pasted picture");
    assert_eq!(app.get_status_left().as_str(), "Pasted picture");

    app.invoke_undo();
    assert_eq!(pictures(&state), 0, "one undo removes the pasted picture");
    assert_eq!(text(&state), "First paragraph");
}

#[test]
fn a_clipboard_with_text_pastes_the_text_and_not_the_picture() {
    let _clipboard = MockClipboard::new();
    loom_clipboard::write_clipboard(&loom_clipboard::ClipboardData {
        plain_text: Some("Copied words".into()),
        ..Default::default()
    })
    .expect("mock write");
    loom_clipboard::set_mock_clipboard_image(Some(png()));
    let (app, state) = writer_app("First");

    app.invoke_paste();

    assert_eq!(pictures(&state), 0, "text keeps priority over a picture");
    assert!(text(&state).contains("Copied words"), "{}", text(&state));
    assert_eq!(app.get_status_left().as_str(), "Pasted text");
}

#[test]
fn an_undecodable_picture_is_refused_and_the_document_is_unchanged() {
    let _clipboard = MockClipboard::new();
    loom_clipboard::set_mock_clipboard_image(Some(b"definitely not a picture".to_vec()));
    let (app, state) = writer_app("Keep this");
    let before = text(&state);

    app.invoke_paste();

    assert_eq!(pictures(&state), 0);
    assert_eq!(text(&state), before);
    assert!(
        app.get_status_left().starts_with("Pasted picture refused"),
        "{}",
        app.get_status_left()
    );
}

#[test]
fn an_oversized_picture_is_refused_before_it_is_decoded() {
    let _clipboard = MockClipboard::new();
    loom_clipboard::set_mock_clipboard_image(Some(vec![0u8; 32 * 1024 * 1024 + 1]));
    let (app, state) = writer_app("Keep this");

    app.invoke_paste();

    assert_eq!(pictures(&state), 0);
    assert!(
        app.get_status_left().contains("larger than 32 MB"),
        "{}",
        app.get_status_left()
    );
}

#[test]
fn an_empty_clipboard_changes_nothing() {
    let _clipboard = MockClipboard::new();
    let (app, state) = writer_app("Keep this");
    let before = text(&state);

    app.invoke_paste();

    assert_eq!(pictures(&state), 0);
    assert_eq!(text(&state), before);
}
