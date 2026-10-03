//! Pictures through the real window callbacks: insert, move/resize, delete, undo,
//! save and reopen, export, and the strip thumbnails.

use super::*;
use loom_desktop::ScriptedFileDialogs;
use loom_package::zip::PackageArchive;
use slint::Model;

fn png_bytes(width: u32, height: u32, seed: u8) -> Vec<u8> {
    let mut image = image::RgbaImage::new(width, height);
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        *pixel = image::Rgba([seed, (x % 251) as u8, (y % 251) as u8, 255]);
    }
    let mut out = Vec::new();
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .expect("encode png");
    out
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("loom-present-pic-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn state_with(dialogs: ScriptedFileDialogs) -> Rc<GuiState> {
    Rc::new(GuiState {
        session: RefCell::new(empty_session()),
        last_saved: RefCell::new(empty_session().document.clone()),
        last_saved_transitions: RefCell::default(),
        pending_replacement: Cell::new(None),
        selected_element: Cell::new(0),
        inspector_available: Cell::new(true),
        save_path: RefCell::new(None),
        dialogs: Rc::new(dialogs),
        deck_filter: FileFilter::new("Deck", ["loomdeck"]).expect("filter"),
        pdf_filter: FileFilter::new("PDF", ["pdf"]).expect("filter"),
        menu_service: None,
        drag_state: RefCell::new(DragState::default()),
    })
}

fn element_count(state: &GuiState) -> usize {
    state
        .session
        .borrow()
        .document
        .active_slide()
        .expect("slide")
        .elements
        .len()
}

fn picture_geometry(state: &GuiState) -> (f32, f32, f32, f32) {
    let session = state.session.borrow();
    let element = session
        .document
        .active_slide()
        .expect("slide")
        .elements
        .iter()
        .find(|element| element.element_type == ElementType::Picture)
        .expect("a picture");
    (element.x, element.y, element.width, element.height)
}

fn picture_index(state: &GuiState) -> i32 {
    state
        .session
        .borrow()
        .document
        .active_slide()
        .expect("slide")
        .elements
        .iter()
        .position(|element| element.element_type == ElementType::Picture)
        .expect("a picture") as i32
}

fn with_picture(name: &str, width: u32, height: u32) -> (PresentApp, Rc<GuiState>, PathBuf) {
    set_platform();
    let app = PresentApp::new().expect("create PresentApp");
    let dir = temp_dir(name);
    let source = dir.join("photo.png");
    std::fs::write(&source, png_bytes(width, height, 7)).expect("write png");
    let state = state_with(ScriptedFileDialogs::new(
        [Some(source.clone())],
        [
            Some(dir.join("deck.loomdeck")),
            Some(dir.join("out.pdf")),
            Some(dir.join("out.pptx")),
        ],
    ));
    wire_app_callbacks(&app, &state);
    refresh_without_recovery(&app, &state);
    app.invoke_add_picture();
    (app, state, source)
}

#[test]
fn insert_image_places_selects_and_draws_the_picture() {
    let (app, state, _) = with_picture("insert", 400, 200);
    assert!(
        app.get_status_left().starts_with("Inserted photo.png"),
        "{}",
        app.get_status_left()
    );
    let (x, y, width, height) = picture_geometry(&state);
    assert!(
        (width / height - 2.0).abs() < 0.01,
        "aspect kept: {width}x{height}"
    );
    assert!(width <= 1000.0 * 0.6 + 0.5 && height <= 562.5 * 0.6 + 0.5);
    assert!(
        (x + width / 2.0 - 500.0).abs() < 1.0 && (y + height / 2.0 - 281.25).abs() < 1.0,
        "centered"
    );
    let index = picture_index(&state) as usize;
    assert_eq!(state.session.borrow().selected_elements.len(), 1);
    assert_eq!(app.get_selection_count(), 1);
    assert_eq!(app.get_element_types().row_data(index), Some(6));
    let image = app.get_element_images().row_data(index).expect("image row");
    assert_eq!(
        (image.size().width, image.size().height),
        (400, 200),
        "the canvas gets the decoded picture"
    );
    assert_eq!(
        app.get_element_contents().row_data(index).unwrap(),
        "photo.png",
        "no asset id leaks into the UI"
    );
    assert!(!app.get_active_element_content_editable());
    assert_eq!(app.get_status_right(), "Edited");
}

#[test]
fn slide_strip_thumbnails_draw_the_real_slide_and_follow_edits() {
    let (app, state, _) = with_picture("thumbs", 120, 60);
    let thumbs = app.get_slide_thumbs();
    assert_eq!(thumbs.row_count(), state.session.borrow().document.len());
    let row = thumbs.row_data(0).expect("first thumbnail");
    let index = picture_index(&state) as usize;
    assert_eq!(row.types.row_data(index), Some(6));
    assert_eq!(row.images.row_data(index).expect("image").size().width, 120);
    assert_eq!(
        row.xs.row_count(),
        element_count(&state),
        "every element, not placeholder lines"
    );

    // A text edit shows up in the thumbnail without rebuilding unrelated slides.
    app.invoke_add_slide();
    assert_eq!(app.get_slide_thumbs().row_count(), 2);
    app.invoke_select_slide(0);
    app.invoke_select_element(0);
    app.invoke_update_element_content("Real title".into());
    let row = app.get_slide_thumbs().row_data(0).expect("first thumbnail");
    assert_eq!(row.contents.row_data(0).unwrap(), "Real title");
}

#[test]
fn cancelled_missing_and_corrupt_files_change_nothing_and_say_why() {
    set_platform();
    let app = PresentApp::new().expect("app");
    let dir = temp_dir("corrupt");
    let bad = dir.join("notes.png");
    std::fs::write(&bad, b"this is not an image").expect("write");
    let text = dir.join("notes.txt");
    std::fs::write(&text, "hello").expect("write");
    let state = state_with(ScriptedFileDialogs::new(
        [None, Some(bad), Some(dir.join("missing.png")), Some(text)],
        [],
    ));
    wire_app_callbacks(&app, &state);
    refresh_without_recovery(&app, &state);
    let before = state.session.borrow().document.clone();
    for expected in [
        "Insert image cancelled",
        "Could not insert image",
        "Could not insert image",
        "Could not insert image",
    ] {
        app.invoke_add_picture();
        assert!(
            app.get_status_left().starts_with(expected),
            "{}",
            app.get_status_left()
        );
        assert!(presentation_documents_match(
            &state.session.borrow().document,
            &before
        ));
        assert!(!state.session.borrow().can_undo());
        assert_eq!(state.session.borrow().document.picture_count(), 0);
    }
    assert!(state.session.borrow().document.assets.is_empty());
}

#[test]
fn resizing_a_picture_keeps_its_aspect_and_undo_redo_round_trips() {
    let (app, state, _) = with_picture("resize", 300, 100);
    let index = picture_index(&state);
    let (x0, y0, w0, h0) = picture_geometry(&state);
    app.invoke_handle_pressed(index, "se".into(), false);
    app.invoke_handle_moved(index, "se".into(), 90.0, 4.0);
    app.invoke_handle_released(index, "se".into());
    let (x1, y1, w1, h1) = picture_geometry(&state);
    assert!(w1 > w0 + 50.0, "grew: {w0} -> {w1}");
    assert!((w1 / h1 - w0 / h0).abs() < 0.02, "aspect kept: {w1}x{h1}");
    assert!(
        (x1 - x0).abs() < 0.01 && (y1 - y0).abs() < 0.01,
        "top-left stays for a south-east drag"
    );

    app.invoke_handle_pressed(index, "nw".into(), false);
    app.invoke_handle_moved(index, "nw".into(), 30.0, 0.0);
    app.invoke_handle_released(index, "nw".into());
    let (x2, y2, w2, h2) = picture_geometry(&state);
    assert!(
        (w2 / h2 - w0 / h0).abs() < 0.02,
        "aspect kept from the other corner"
    );
    assert!(
        ((x2 + w2) - (x1 + w1)).abs() < 0.01 && ((y2 + h2) - (y1 + h1)).abs() < 0.01,
        "opposite corner stays"
    );

    app.invoke_element_pressed(index, false);
    app.invoke_element_moved(index, 25.0, 15.0);
    app.invoke_element_released(index);
    let (x3, y3, w3, h3) = picture_geometry(&state);
    assert!((x3 - x2).abs() > 1.0 && (y3 - y2).abs() > 1.0);
    assert!(
        (w3 - w2).abs() < 0.01 && (h3 - h2).abs() < 0.01,
        "a move does not resize"
    );

    app.invoke_undo();
    assert_eq!(picture_geometry(&state), (x2, y2, w2, h2));
    app.invoke_undo();
    assert_eq!(picture_geometry(&state), (x1, y1, w1, h1));
    app.invoke_redo();
    assert_eq!(picture_geometry(&state), (x2, y2, w2, h2));
}

#[test]
fn delete_key_removes_the_picture_and_undo_redo_restore_it() {
    let (app, state, _) = with_picture("delete", 80, 80);
    let before = element_count(&state);
    let geometry = picture_geometry(&state);
    app.invoke_canvas_key_pressed(slint::platform::Key::Delete.into(), false, false);
    assert_eq!(element_count(&state), before - 1);
    assert_eq!(state.session.borrow().document.picture_count(), 0);
    assert_eq!(app.get_selection_count(), 0);
    app.invoke_undo();
    assert_eq!(element_count(&state), before);
    assert_eq!(picture_geometry(&state), geometry);
    assert!(
        app.get_element_images()
            .row_data(picture_index(&state) as usize)
            .unwrap()
            .size()
            .width
            > 0
    );
    app.invoke_redo();
    assert_eq!(element_count(&state), before - 1);
}

#[test]
fn pictures_survive_save_and_reopen_after_the_source_is_deleted() {
    let (app, state, source) = with_picture("save", 160, 90);
    let geometry = picture_geometry(&state);
    assert_eq!(save_current_deck(&app, &state, true), Ok(true));
    assert_eq!(app.get_status_right(), "Saved", "the saved state is shown");
    let deck = state.save_path.borrow().clone().expect("saved path");
    std::fs::remove_file(&source).expect("delete the source image");

    let reopened = load_session(&deck).expect("reopen");
    let element = reopened
        .document
        .slides
        .iter()
        .flat_map(|slide| &slide.elements)
        .find(|element| element.element_type == ElementType::Picture)
        .expect("picture element");
    assert_eq!(
        (element.x, element.y, element.width, element.height),
        geometry
    );
    let asset = reopened
        .document
        .asset_for(element)
        .expect("embedded bytes");
    assert_eq!((asset.width, asset.height), (160, 90));

    // The reopened deck draws it.
    let app2 = PresentApp::new().expect("app");
    let state2 = state_with(ScriptedFileDialogs::new([], []));
    *state2.session.borrow_mut() = reopened;
    wire_app_callbacks(&app2, &state2);
    refresh_without_recovery(&app2, &state2);
    let index = picture_index(&state2) as usize;
    assert_eq!(
        app2.get_element_images()
            .row_data(index)
            .unwrap()
            .size()
            .width,
        160
    );

    // Edited again after the next change.
    app.invoke_canvas_key_pressed(slint::platform::Key::Delete.into(), false, false);
    assert_eq!(app.get_status_right(), "Edited");
}

#[test]
fn pdf_and_pptx_exports_through_the_app_include_the_picture() {
    let (app, state, _) = with_picture("export", 40, 20);
    let _ = state;
    // The scripted save results are ordered: deck, pdf, pptx. Skip the deck.
    assert_eq!(save_current_deck(&app, &state, true), Ok(true));
    app.invoke_export_pdf();
    assert!(
        app.get_status_left().starts_with("Exported "),
        "{}",
        app.get_status_left()
    );
    app.invoke_export_pptx();
    assert!(
        app.get_status_left().starts_with("Exported "),
        "{}",
        app.get_status_left()
    );
    let dir = state
        .save_path
        .borrow()
        .clone()
        .expect("path")
        .parent()
        .expect("dir")
        .to_path_buf();
    let pdf = std::fs::read(dir.join("out.pdf")).expect("pdf written");
    let needle: &[u8] = b"/Subtype /Image /Width 40 /Height 20";
    assert!(pdf.windows(needle.len()).any(|w| w == needle));
    let pptx = std::fs::read(dir.join("out.pptx")).expect("pptx written");
    let archive = PackageArchive::from_bytes(&pptx).expect("pptx zip");
    let slide = String::from_utf8(
        archive
            .get("ppt/slides/slide1.xml")
            .expect("slide")
            .to_vec(),
    )
    .expect("utf8");
    assert!(slide.contains("<p:pic>"));
}

#[test]
fn a_fresh_start_is_a_blank_deck_and_the_sample_stays_reachable() {
    set_platform();
    let (fresh, baseline) = startup_sessions(None, None).expect("fresh start");
    assert_eq!(fresh.document.title, "Untitled Presentation");
    assert_eq!(fresh.document.len(), 1);
    assert!(fresh.document.slides[0]
        .elements
        .iter()
        .all(|e| e.content.is_empty()));
    assert!(presentation_documents_match(
        &fresh.document,
        &baseline.document
    ));

    let app = PresentApp::new().expect("app");
    let state = state_with(ScriptedFileDialogs::new([], []));
    wire_app_callbacks(&app, &state);
    refresh_without_recovery(&app, &state);
    app.invoke_new_sample_deck();
    assert_eq!(
        state.session.borrow().document.title,
        "Loom for Local Creators"
    );
    assert!(
        app.get_status_left().contains("sample"),
        "{}",
        app.get_status_left()
    );
    assert!(dispatch_command(&app, "file.new_sample"));
    assert!(is_present_menu_command("slide.insert_image"));
}
