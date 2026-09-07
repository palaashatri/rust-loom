use super::*;
use loom_desktop::ScriptedFileDialogs;

fn create_audit_state() -> GuiState {
    let mut canvas = blank_canvas().expect("blank canvas");
    let w = canvas.document.width;
    let h = canvas.document.height;
    let mut source = RgbaImage::transparent(w, h).expect("source image");
    for y in 0..10.min(h) {
        for x in 0..10.min(w) {
            source.set_pixel(x, y, [(x * 20) as u8, (y * 20) as u8, 128, 255]);
        }
    }
    canvas.set_layer_image("layer-bg", source).expect("set source image");
    new_gui_state(
        PhotoSession::new(canvas),
        None,
        Rc::new(ScriptedFileDialogs::new(
            [
                Some(PathBuf::from("audit-opened.loomphoto")),
                Some(PathBuf::from("audit-source.png")),
            ],
            [
                Some(PathBuf::from("audit-saved.loomphoto")),
                Some(PathBuf::from("audit-export.png")),
                Some(PathBuf::from("audit-export.jpg")),
            ],
        )),
    )
    .expect("create state")
}

#[test]
fn test_deep_audit_layer_stack_lifecycle_and_reordering() {
    set_platform();
    let app = PhotoApp::new().expect("create PhotoApp");
    let state = Rc::new(create_audit_state());
    wire_add_layer_callback(&app, &state);
    wire_add_adjustment_callback(&app, &state);
    wire_move_layer_callback(&app, &state);
    refresh_photo_with_state(&app, &state).expect("refresh initial state");

    assert_eq!(state.session.borrow().canvas.document.layers.len(), 1);
    assert!(!app.get_can_move_up());
    assert!(!app.get_can_move_down());

    // Add pixel layer
    app.invoke_add_layer();
    refresh_photo_with_state(&app, &state).expect("refresh after add layer");
    assert_eq!(state.session.borrow().canvas.document.layers.len(), 2);
    assert_eq!(state.session.borrow().canvas.document.active_layer_index, 1);
    assert!(!app.get_can_move_up());
    assert!(app.get_can_move_down());

    // Add adjustment layer
    app.invoke_add_adjustment();
    refresh_photo_with_state(&app, &state).expect("refresh after add adjustment");
    assert_eq!(state.session.borrow().canvas.document.layers.len(), 3);
    assert_eq!(state.session.borrow().canvas.document.active_layer_index, 2);

    // Reorder: move adjustment layer down
    app.invoke_move_layer(-1);
    refresh_photo_with_state(&app, &state).expect("refresh after move down");
    assert_eq!(state.session.borrow().canvas.document.active_layer_index, 1);
    assert!(app.get_can_move_up());
    assert!(app.get_can_move_down());

    // Toggle layer visibility
    state.session.borrow_mut().canvas.document.layers[1].visible = false;
    refresh_photo_with_state(&app, &state).expect("refresh visibility toggle");
    assert!(!state.session.borrow().canvas.document.layers[1].visible);

    // Delete active layer
    assert!(state.session.borrow_mut().remove_layer(1));
    refresh_photo_with_state(&app, &state).expect("refresh after delete layer");
    assert_eq!(state.session.borrow().canvas.document.layers.len(), 2);
}

#[test]
fn test_deep_audit_blend_modes_opacity_and_adjustments() {
    set_platform();
    let app = PhotoApp::new().expect("create PhotoApp");
    let state = Rc::new(create_audit_state());
    wire_adjustment_callbacks(&app, &state);
    refresh_photo_with_state(&app, &state).expect("initial refresh");

    // Add adjustment layers for brightness and contrast
    {
        let mut session = state.session.borrow_mut();
        session.add_adjustment("adj-bright", "Brightness Adj", "brightness", 0.0);
        session.add_adjustment("adj-contrast", "Contrast Adj", "contrast", 0.0);
        session.canvas.document.select_layer(1);
    }
    refresh_photo_with_state(&app, &state).expect("refresh adjustments");

    app.invoke_brightness_changed(35.0);
    assert!(
        (state.session.borrow().canvas.document.layers[1].adjustment_value - 0.35).abs() < 0.001
    );

    // Test blend mode and opacity mutations
    state.session.borrow_mut().canvas.document.layers[0].blend_mode = BlendMode::Multiply;
    state.session.borrow_mut().canvas.document.layers[0].opacity = 0.75;
    assert_eq!(
        state.session.borrow().canvas.document.layers[0].blend_mode,
        BlendMode::Multiply
    );
    assert_eq!(
        state.session.borrow().canvas.document.layers[0].opacity,
        0.75
    );

    // Undo should restore previous values
    assert!(state.session.borrow_mut().undo());
    assert!(
        (state.session.borrow().canvas.document.layers[1].adjustment_value).abs() < 0.001
    );
}

#[test]
fn test_deep_audit_transforms_crops_and_selection() {
    set_platform();
    let app = PhotoApp::new().expect("create PhotoApp");
    let state = Rc::new(create_audit_state());
    wire_transform_callback(&app, &state);
    wire_selection_callbacks(&app, &state);
    refresh_photo_with_state(&app, &state).expect("initial refresh");

    // Test layer transform
    app.invoke_layer_transform_changed(15.0, -10.0, 150.0, 120.0, 45.0);
    let transform = state.session.borrow().canvas.document.layers[0].transform;
    assert_eq!(transform.tx, 15.0);
    assert_eq!(transform.ty, -10.0);
    assert_ne!(transform, AffineTransform2D::identity());

    // Selection & canvas crop
    state.session.borrow_mut().set_selection(Some(Rect::new(2.0, 2.0, 6.0, 6.0))).expect("set selection");
    refresh_photo_with_state(&app, &state).expect("refresh selection");
    assert!(app.get_has_selection());

    app.invoke_crop_to_selection();
    assert_eq!(
        state.session.borrow().canvas.document.crop,
        Some(Rect::new(2.0, 2.0, 6.0, 6.0))
    );

    // Clear crop
    app.invoke_clear_crop();
    assert_eq!(state.session.borrow().canvas.document.crop, None);
}

#[test]
fn test_deep_audit_persistence_and_export_roundtrip() {
    let state = create_audit_state();
    let canvas = &state.session.borrow().canvas;

    // Save to package format
    let package_bytes = save_photo_canvas(canvas).expect("save canvas");
    assert!(!package_bytes.is_empty());

    // Load from package format
    let loaded_canvas = load_photo_canvas(&package_bytes).expect("load canvas");
    assert_eq!(loaded_canvas.document.name, canvas.document.name);
    assert_eq!(loaded_canvas.document.width, canvas.document.width);
    assert_eq!(loaded_canvas.document.height, canvas.document.height);
    assert_eq!(loaded_canvas.document.layers.len(), canvas.document.layers.len());

    // Composite and export
    let composite = canvas.composite().expect("composite image");
    let png_bytes = encode_png(&composite).expect("encode PNG");
    assert!(!png_bytes.is_empty());
    assert_eq!(&png_bytes[0..4], &[0x89, b'P', b'N', b'G']);

    let jpeg_bytes = encode_jpeg(&composite, 90).expect("encode JPEG");
    assert!(!jpeg_bytes.is_empty());
    assert_eq!(&jpeg_bytes[0..2], &[0xFF, 0xD8]);
}

#[test]
fn test_deep_audit_macos_global_menu_bar_command_projection() {
    set_platform();
    let app = PhotoApp::new().expect("create PhotoApp");
    configure_responsive_layout(&app, (1280, 800));
    let state = create_audit_state();
    let menu = NativeMenuBar::new();
    menu.install_menu_bar(&build_photo_menu_bar(&app)).expect("install menu bar");
    sync_menu_state(&menu, &app, &state);

    let installed = menu.installed_menu_bar().expect("get installed menu");
    let required_commands = [
        "file.new", "file.open", "file.save", "file.save_as",
        "file.import_image", "file.export_png", "file.export_jpeg",
        "edit.undo", "edit.redo",
        "layer.new", "layer.adjustment", "layer.delete", "layer.move_up", "layer.move_down",
        "view.inspector", "app.palette",
    ];

    for command in required_commands {
        assert!(
            installed.find_item(command).is_some(),
            "missing required command projection {command}"
        );
    }
}
