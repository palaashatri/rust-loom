//! Picture selection and resizing through the real pointer and key callbacks.
//! A press on a picture selects it, dragging a corner handle resizes it live
//! and one release is one undo step, Escape cancels a drag, and Alt+Shift+Left
//! and Alt+Shift+Right resize the selected picture by ten percent.

use crate::actions_tests::test_state;
use crate::*;
use loom_writer_core::{InlineImage, RichBlock, TextSelection, WriterDocument};
use slint::platform::Key;
use slint::{ComponentHandle, Model, SharedString};
use std::rc::Rc;

/// Where the picture's block starts: "Before" and one paragraph separator come first.
const PICTURE_START: usize = "Before".len() + 1;

/// A 400 x 200 pixel PNG, which is 300 x 150 points at 96 pixels to the inch.
fn png() -> Vec<u8> {
    let mut bytes = Vec::new();
    image::RgbaImage::from_pixel(400, 200, image::Rgba([30, 120, 200, 255]))
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .unwrap();
    bytes
}

/// "Before", then the picture, then the empty paragraph Insert Picture leaves.
fn document(alignment: loom_text::Alignment) -> WriterDocument {
    let mut doc = WriterDocument::new("pictures", "Pictures");
    doc.blocks.push(RichBlock::new(1, "paragraph", "Before"));
    let image = InlineImage::from_bytes("photo.png", png()).expect("a readable PNG");
    doc.insert_image_block(6, image)
        .expect("room for a picture");
    for block in doc.blocks.iter_mut().filter(|block| block.is_image()) {
        block.style.alignment = alignment;
    }
    doc
}

fn app_with(doc: WriterDocument) -> (WriterApp, Rc<GuiState>) {
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], []));
    let (app, state) = test_state(doc, dialogs);
    wire_writer_shared_callbacks(&app, &state, None);
    apply_state(&app, &state);
    (app, state)
}

/// The picture as the page canvas draws it right now.
fn shown(app: &WriterApp) -> WriterPageImage {
    app.global::<PageImages>()
        .get_items()
        .row_data(0)
        .expect("the picture is on the page")
}

fn centre(item: &WriterPageImage) -> (f32, f32) {
    (item.x + item.width / 2.0, item.y + item.height / 2.0)
}

/// A point inside the bottom-right corner, where that handle is drawn.
fn bottom_right(item: &WriterPageImage) -> (f32, f32) {
    (item.x + item.width - 1.0, item.y + item.height - 1.0)
}

fn select_picture(app: &WriterApp) {
    let (x, y) = centre(&shown(app));
    app.invoke_pointer_pressed(x, y, false);
    app.invoke_pointer_released(x, y);
}

fn stored_width(state: &GuiState) -> f32 {
    state
        .current
        .borrow()
        .blocks
        .iter()
        .find(|block| block.is_image())
        .and_then(|block| block.image.as_ref())
        .map(|image| image.width_pt)
        .expect("the document holds the picture")
}

fn usable_width(state: &GuiState) -> f32 {
    let style = state.current.borrow().page.page_style();
    style.width_pt - style.margin_left_pt - style.margin_right_pt
}

fn approx(actual: f32, expected: f32) -> bool {
    (actual - expected).abs() < 0.05
}

/// Presses the bottom-right handle of the selected picture, moves by (`dx`, `dy`)
/// and returns the pointer position where the drag ends.
fn drag_bottom_right(app: &WriterApp, dx: f32, dy: f32) -> (f32, f32) {
    let (hx, hy) = bottom_right(&shown(app));
    app.invoke_pointer_pressed(hx, hy, false);
    app.invoke_pointer_moved(hx + dx, hy + dy);
    (hx + dx, hy + dy)
}

#[test]
fn pressing_a_picture_selects_it_and_moves_the_caret_into_its_block() {
    let (app, state) = app_with(document(loom_text::Alignment::Left));
    select_picture(&app);
    assert_eq!(
        state.current.borrow().selection(),
        TextSelection::caret(PICTURE_START),
        "the caret goes to the picture's block"
    );
    assert!(
        shown(&app).selected,
        "the picture shows its outline and handles"
    );
    assert_eq!(
        state.history.borrow().undo_len(),
        0,
        "selecting a picture is not an edit"
    );
}

#[test]
fn the_outline_follows_the_caret_off_the_picture() {
    let (app, _state) = app_with(document(loom_text::Alignment::Left));
    select_picture(&app);
    assert!(shown(&app).selected);
    app.invoke_selection_changed(2, 2);
    assert!(!shown(&app).selected, "the caret left the picture");
}

#[test]
fn dragging_the_bottom_right_handle_resizes_live_and_keeps_the_aspect_ratio() {
    let (app, state) = app_with(document(loom_text::Alignment::Left));
    select_picture(&app);
    let (ex, ey) = drag_bottom_right(&app, 60.0, 0.0);
    let live = shown(&app);
    assert!(approx(live.width, 360.0), "live width {}", live.width);
    assert!(approx(live.height, 180.0), "live height {}", live.height);
    assert!(
        approx(stored_width(&state), 300.0),
        "the document is unchanged until the release"
    );
    app.invoke_pointer_released(ex, ey);
    assert!(
        approx(stored_width(&state), 360.0),
        "release commits the width"
    );
    assert!(
        approx(shown(&app).height, 180.0),
        "the page redraws at the committed size"
    );
    assert_eq!(
        state.history.borrow().undo_len(),
        1,
        "one drag is one undo step"
    );
}

#[test]
fn undo_after_a_resize_restores_the_picture_size() {
    let (app, state) = app_with(document(loom_text::Alignment::Left));
    select_picture(&app);
    let (ex, ey) = drag_bottom_right(&app, 60.0, 0.0);
    app.invoke_pointer_released(ex, ey);
    app.invoke_undo();
    assert!(
        approx(stored_width(&state), 300.0),
        "undo restores the size"
    );
    assert_eq!(state.history.borrow().undo_len(), 0);
}

#[test]
fn a_press_and_release_on_a_handle_without_moving_adds_no_history() {
    let (app, state) = app_with(document(loom_text::Alignment::Left));
    select_picture(&app);
    let (hx, hy) = bottom_right(&shown(&app));
    app.invoke_pointer_pressed(hx, hy, false);
    app.invoke_pointer_released(hx, hy);
    assert!(approx(stored_width(&state), 300.0));
    assert_eq!(
        state.history.borrow().undo_len(),
        0,
        "a zero-length drag records nothing"
    );
}

#[test]
fn a_drag_past_the_text_width_stops_at_the_text_width_and_inward_at_sixteen_points() {
    let (app, state) = app_with(document(loom_text::Alignment::Left));
    let usable = usable_width(&state);
    select_picture(&app);
    let (ex, ey) = drag_bottom_right(&app, 2000.0, 0.0);
    assert!(
        approx(shown(&app).width, usable),
        "live width {} stops at the text width {usable}",
        shown(&app).width
    );
    app.invoke_pointer_released(ex, ey);
    assert!(
        approx(stored_width(&state), usable),
        "the commit clamps too"
    );

    let (app, state) = app_with(document(loom_text::Alignment::Left));
    select_picture(&app);
    let (ex, ey) = drag_bottom_right(&app, -2000.0, 0.0);
    let small = shown(&app);
    assert!(approx(small.width, 32.0), "minimum width {}", small.width);
    assert!(
        approx(small.height, 16.0),
        "minimum height {}",
        small.height
    );
    app.invoke_pointer_released(ex, ey);
    assert!(approx(stored_width(&state), 32.0));
}

#[test]
fn escape_during_a_handle_drag_restores_the_size_and_a_later_release_adds_nothing() {
    let (app, state) = app_with(document(loom_text::Alignment::Left));
    select_picture(&app);
    let (ex, ey) = drag_bottom_right(&app, 60.0, 0.0);
    let escape: SharedString = Key::Escape.into();
    assert!(
        app.invoke_handle_table_key(escape, false, false, false),
        "Escape is consumed while a drag is running"
    );
    assert!(
        approx(shown(&app).width, 300.0),
        "Escape restores the on-screen size, now {}",
        shown(&app).width
    );
    app.invoke_pointer_released(ex, ey);
    assert!(
        approx(stored_width(&state), 300.0),
        "the cancelled drag never committed"
    );
    assert_eq!(state.history.borrow().undo_len(), 0);
}

#[test]
fn alt_shift_arrows_resize_the_selected_picture_by_ten_percent_without_moving_the_caret() {
    let (app, state) = app_with(document(loom_text::Alignment::Left));
    select_picture(&app);
    let caret = state.current.borrow().selection();

    let right: SharedString = Key::RightArrow.into();
    assert!(
        app.invoke_handle_table_key(right, true, false, true),
        "Alt+Shift+Right is consumed on a picture"
    );
    assert!(
        approx(stored_width(&state), 330.0),
        "grew to {}",
        stored_width(&state)
    );
    assert_eq!(
        state.current.borrow().selection(),
        caret,
        "the caret does not move"
    );
    assert_eq!(app.get_status_left().as_str(), "Picture 330 x 165 pt");

    let left: SharedString = Key::LeftArrow.into();
    assert!(app.invoke_handle_table_key(left, true, false, true));
    assert!(
        approx(stored_width(&state), 297.0),
        "shrank to {}",
        stored_width(&state)
    );
    assert!(app.get_status_left().as_str().starts_with("Picture 297 x "));
}

#[test]
fn alt_shift_arrows_with_the_caret_in_text_are_left_to_the_editor() {
    let (app, state) = app_with(document(loom_text::Alignment::Left));
    app.invoke_selection_changed(2, 2);
    let right: SharedString = Key::RightArrow.into();
    assert!(
        !app.invoke_handle_table_key(right, true, false, true),
        "the editor keeps Alt+Shift+Right when no picture is selected"
    );
    assert!(approx(stored_width(&state), 300.0));
    assert_eq!(state.history.borrow().undo_len(), 0);
}

#[test]
fn a_right_aligned_picture_previews_where_the_committed_layout_puts_it() {
    let (app, state) = app_with(document(loom_text::Alignment::Right));
    select_picture(&app);
    let (ex, ey) = drag_bottom_right(&app, 60.0, 0.0);
    let live = shown(&app);
    app.invoke_pointer_released(ex, ey);
    let committed = shown(&app);
    assert!(
        approx(live.x, committed.x),
        "preview x {} matches the committed x {}",
        live.x,
        committed.x
    );
    assert!(approx(live.width, committed.width));
    assert!(
        approx(committed.x + committed.width, usable_width(&state)),
        "a right-aligned picture ends at the text column"
    );
}

#[test]
fn a_press_on_text_still_places_the_caret_in_the_text() {
    let (app, state) = app_with(document(loom_text::Alignment::Left));
    let row = app
        .get_render_blocks()
        .row_data(0)
        .expect("the first paragraph has a row");
    app.invoke_pointer_pressed(row.x + 1.0, row.y + 1.0, false);
    app.invoke_pointer_released(row.x + 1.0, row.y + 1.0);
    let focus = state.current.borrow().selection().focus;
    assert!(
        focus <= "Before".len(),
        "caret {focus} stays in the first paragraph"
    );
    assert!(!shown(&app).selected);
}

/// Review captures, not a gate: writes the document as a `.loomdoc` and the
/// selected picture as PNGs to `LOOM_PICTURE_DUMP`. Run with
/// `cargo test -p loom-writer-app writes_selected_picture -- --ignored`.
#[test]
#[ignore = "writes review captures to LOOM_PICTURE_DUMP"]
fn writes_selected_picture_captures_for_review() {
    let Ok(dir) = std::env::var("LOOM_PICTURE_DUMP") else {
        return;
    };
    let (app, state) = app_with(document(loom_text::Alignment::Left));
    let bytes = loom_writer_core::save_document(&state.current.borrow()).expect("save");
    std::fs::write(format!("{dir}/picture.loomdoc"), bytes).expect("write the document");
    select_picture(&app);
    let light = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render");
    light
        .save(format!("{dir}/picture-selected-light-1x.png"))
        .expect("save light");
    let close = snapshot_component(&app, 1280.0, 800.0, 2.0).expect("render at 2x");
    close
        .save(format!("{dir}/picture-selected-light-2x.png"))
        .expect("save 2x");
    crate::apply_theme(&app, "high-contrast");
    let contrast = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render contrast");
    contrast
        .save(format!("{dir}/picture-selected-high-contrast.png"))
        .expect("save contrast");
}
