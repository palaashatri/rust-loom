use super::*;
use loom_desktop::ScriptedFileDialogs;

fn scripted_state() -> GuiState {
    new_gui_state(
        PhotoSession::new(blank_canvas().expect("blank canvas")),
        None,
        Rc::new(ScriptedFileDialogs::new(
            [
                Some(PathBuf::from("opened.loomphoto")),
                Some(PathBuf::from("source.png")),
            ],
            [
                Some(PathBuf::from("saved")),
                Some(PathBuf::from("exported")),
                Some(PathBuf::from("exported-jpeg")),
            ],
        )),
    )
    .expect("state")
}

#[test]
fn responsive_policy_transition_probes_are_exact() {
    set_platform();
    let app = PhotoApp::new().expect("create PhotoApp");
    let expected = [
        (1179, true, true, false),
        (1180, false, true, false),
        (1279, false, true, false),
        (1280, false, true, false),
        (1319, false, true, false),
        (1320, false, false, true),
    ];
    for (width, icon_only, overflow, labeled) in expected {
        assert_eq!(
            responsive_toolbar_state(&app, width),
            ResponsiveToolbarState {
                icon_only,
                overflow,
                labeled,
            }
        );
        configure_responsive_width(&app, width);
        assert_eq!(app.get_icon_only_toolbar(), icon_only);
        assert_eq!(app.get_overflow_toolbar(), overflow);
        assert_eq!(app.get_labeled_toolbar(), labeled);
    }
}

#[test]
fn dialog_requests_keep_projects_imports_and_exports_separate() {
    let state = scripted_state();
    let project = open_project_request(&state);
    let import = import_image_request(&state);
    let save = save_project_request(&state);
    let png = export_request(&state, ExportKind::Png);
    let jpeg = export_request(&state, ExportKind::Jpeg);

    assert_eq!(project.filters[0].extensions, vec!["loomphoto".to_string()]);
    assert_eq!(
        import.filters[0].extensions,
        vec!["png".to_string(), "jpg".to_string(), "jpeg".to_string()]
    );
    assert_eq!(save.suggested_name.as_deref(), Some(SAVE_FILENAME));
    assert_eq!(png.filters[0].extensions, vec!["png".to_string()]);
    assert_eq!(
        jpeg.filters[0].extensions,
        vec!["jpg".to_string(), "jpeg".to_string()]
    );
}

#[test]
fn scripted_dialog_backend_drives_all_photo_file_operations() {
    let state = scripted_state();
    assert_eq!(
        state
            .dialogs
            .open_file(&open_project_request(&state))
            .expect("project picker"),
        Some(PathBuf::from("opened.loomphoto"))
    );
    assert_eq!(
        state
            .dialogs
            .open_file(&import_image_request(&state))
            .expect("import picker"),
        Some(PathBuf::from("source.png"))
    );
    assert_eq!(
        state
            .dialogs
            .save_file(&save_project_request(&state))
            .expect("save picker"),
        Some(PathBuf::from("saved"))
    );
    assert_eq!(
        state
            .dialogs
            .save_file(&export_request(&state, ExportKind::Png))
            .expect("png picker"),
        Some(PathBuf::from("exported"))
    );
    assert_eq!(
        state
            .dialogs
            .save_file(&export_request(&state, ExportKind::Jpeg))
            .expect("jpeg picker"),
        Some(PathBuf::from("exported-jpeg"))
    );
}

#[test]
fn imported_rasters_never_become_project_save_paths() {
    assert!(is_native_project(Path::new("project.loomphoto")));
    assert!(!is_native_project(Path::new("source.png")));
    assert_eq!(
        ensure_extension(PathBuf::from("project"), "loomphoto"),
        PathBuf::from("project.loomphoto")
    );
    assert_eq!(
        ensure_extension(PathBuf::from("already.jpeg"), "jpg"),
        PathBuf::from("already.jpeg")
    );
}

#[test]
fn pan_tool_is_a_viewport_mode_separate_from_brush() {
    assert_eq!(photo_tool_state("Pan"), (0, true));
    assert_eq!(photo_tool_state("brush"), (1, false));
    assert_eq!(photo_tool_state("WAND"), (2, false));
    assert_eq!(photo_tool_state("unknown"), (0, false));
}

#[test]
fn imported_raster_canvas_preserves_payload_identity_through_reopen() {
    let path =
        std::env::temp_dir().join(format!("loom-photo-import-test-{}.png", std::process::id()));
    let source = sample_raster_payload().expect("sample payload");
    std::fs::write(&path, source).expect("write sample payload");
    let canvas = load_raster_canvas(&path).expect("decode imported raster");
    let layer = canvas.document.active_layer().expect("imported layer");
    assert!(layer.id.starts_with("layer-imported-"));
    assert_eq!(
        layer.source_digest,
        canvas.layer_image(&layer.id).map(RgbaImage::pixel_digest)
    );
    assert_eq!(canvas.pixel_payload_count(), 1);

    let bytes = save_photo_canvas(&canvas).expect("save imported canvas");
    let reopened = load_photo_canvas(&bytes).expect("reopen imported canvas");
    let reopened_layer = reopened.document.active_layer().expect("reopened layer");
    assert_eq!(reopened_layer.id, layer.id);
    assert_eq!(reopened_layer.source_digest, layer.source_digest);
    assert_eq!(reopened.pixel_payload_count(), 1);
    assert_eq!(
        reopened
            .layer_image(&reopened_layer.id)
            .map(RgbaImage::pixel_digest),
        canvas.layer_image(&layer.id).map(RgbaImage::pixel_digest)
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn transformed_layer_crop_callback_maps_document_selection_to_source_pixels() {
    set_platform();
    let app = PhotoApp::new().expect("create PhotoApp");
    let document = PhotoDocument::new("crop-callback", "Crop Callback", 4, 1);
    let mut canvas = PhotoCanvas::new(document).expect("create canvas");
    let mut source = RgbaImage::transparent(4, 1).expect("source image");
    source.set_pixel(0, 0, [255, 0, 0, 255]);
    source.set_pixel(1, 0, [0, 255, 0, 255]);
    source.set_pixel(2, 0, [0, 0, 255, 255]);
    source.set_pixel(3, 0, [255, 255, 255, 255]);
    canvas
        .set_layer_image("layer-bg", source)
        .expect("attach source image");
    let state = Rc::new(
        new_gui_state(
            PhotoSession::new(canvas),
            None,
            Rc::new(ScriptedFileDialogs::new(
                std::iter::empty::<Option<PathBuf>>(),
                std::iter::empty::<Option<PathBuf>>(),
            )),
        )
        .expect("create state"),
    );
    wire_transform_callback(&app, &state);
    wire_selection_callbacks(&app, &state);
    refresh_photo_with_state(&app, &state).expect("initial refresh");

    app.invoke_layer_transform_changed(1.0, 0.0, 100.0, 100.0, 0.0);
    state
        .session
        .borrow_mut()
        .set_selection(Some(Rect::new(1.0, 0.0, 1.0, 1.0)))
        .expect("set document selection");
    refresh_photo_with_state(&app, &state).expect("refresh selection");

    app.invoke_crop_active_layer_to_selection();
    assert_eq!(
        state.session.borrow().canvas.document.layers[0].crop,
        Some(Rect::new(0.0, 0.0, 1.0, 1.0))
    );
    let composite = state
        .session
        .borrow()
        .canvas
        .composite()
        .expect("composite cropped layer");
    assert_eq!(composite.pixel(0, 0), Some([0, 0, 0, 0]));
    assert_eq!(composite.pixel(1, 0), Some([255, 0, 0, 255]));
    assert_eq!(composite.pixel(2, 0), Some([0, 0, 0, 0]));
}

#[test]
fn layer_bounds_selection_is_disabled_without_a_pixel_payload() {
    set_platform();
    let app = PhotoApp::new().expect("create PhotoApp");
    let document = PhotoDocument::new("no-payload", "No Payload", 4, 1);
    let state = Rc::new(
        new_gui_state(
            PhotoSession::new(PhotoCanvas::new(document).expect("create canvas")),
            None,
            Rc::new(ScriptedFileDialogs::new(
                std::iter::empty::<Option<PathBuf>>(),
                std::iter::empty::<Option<PathBuf>>(),
            )),
        )
        .expect("create state"),
    );
    wire_selection_callbacks(&app, &state);
    refresh_photo_with_state(&app, &state).expect("initial refresh");

    assert!(!app.get_has_selected_layer_bounds());
    assert_eq!(state.session.borrow().canvas.document.selection, None);
    app.invoke_select_layer_bounds();
    assert!(!app.get_has_selected_layer_bounds());
    assert_eq!(state.session.borrow().canvas.document.selection, None);
    assert!(app
        .get_status_left()
        .as_str()
        .contains("Selection failed: selected layer has no visible bounds"));
    assert!(!state.session.borrow().can_undo());
}

#[test]
fn inspector_scroll_state_accepts_lower_content_positions() {
    set_platform();
    let app = PhotoApp::new().expect("create PhotoApp");
    configure_responsive_layout(&app, (1280, 800));
    let state = Rc::new(scripted_state());
    refresh_photo_with_state(&app, &state).expect("initial refresh");

    app.set_inspector_scroll_y(-460.0);
    assert_eq!(app.get_inspector_scroll_y(), -460.0);
    app.set_inspector_scroll_y(0.0);
    assert_eq!(app.get_inspector_scroll_y(), 0.0);
}

#[test]
fn inspector_tab_change_clamps_scroll_for_shorter_content() {
    set_platform();
    let app = PhotoApp::new().expect("create PhotoApp");
    configure_responsive_layout(&app, (1280, 800));
    let state = Rc::new(scripted_state());
    refresh_photo_with_state(&app, &state).expect("initial refresh");

    app.set_inspector_tab(0);
    app.set_inspector_scroll_y(-460.0);
    assert_eq!(app.get_inspector_scroll_y(), -460.0);

    app.set_inspector_tab(1);
    snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render Layers tab");
    assert_eq!(app.get_inspector_scroll_y(), 0.0);
    app.set_inspector_tab(2);
    snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render Export tab");
    assert_eq!(app.get_inspector_scroll_y(), 0.0);
}

#[test]
fn photo_edit_callbacks_mutate_selection_transform_crop_and_adjustment() {
    set_platform();
    let app = PhotoApp::new().expect("create PhotoApp");
    let state = Rc::new(scripted_state());
    wire_transform_callback(&app, &state);
    wire_selection_callbacks(&app, &state);
    wire_add_adjustment_callback(&app, &state);
    wire_adjustment_callbacks(&app, &state);
    refresh_photo_with_state(&app, &state).expect("initial refresh");

    app.invoke_layer_transform_changed(12.0, 8.0, 125.0, 100.0, 4.0);
    let transformed = state.session.borrow().canvas.document.layers[0].transform;
    assert_eq!(transformed.tx, 12.0);
    assert_eq!(transformed.ty, 8.0);
    assert!(state.session.borrow().can_undo());

    app.invoke_select_layer_bounds();
    assert!(state.session.borrow().canvas.document.selection.is_some());
    app.invoke_crop_to_selection();
    assert!(state.session.borrow().canvas.document.crop.is_some());
    app.invoke_add_adjustment();
    assert_eq!(
        state
            .session
            .borrow()
            .canvas
            .document
            .active_layer()
            .and_then(|layer| layer.adjustment_type.as_deref()),
        Some("brightness")
    );
    app.invoke_brightness_changed(25.0);
    assert!(
        (adjustment_value(&state.session.borrow().canvas.document, "brightness") - 0.25).abs()
            < 0.001
    );

    assert!(state.session.borrow_mut().undo());
    assert!(
        adjustment_value(&state.session.borrow().canvas.document, "brightness").abs() < 0.001
    );
    let bytes = save_photo_canvas(&state.session.borrow().canvas).expect("save edits");
    let reopened = load_photo_canvas(&bytes).expect("reopen edits");
    assert_eq!(
        reopened.document.layers[0].transform,
        state.session.borrow().canvas.document.layers[0].transform
    );
    assert_eq!(
        reopened.document.selection,
        state.session.borrow().canvas.document.selection
    );
    assert_eq!(
        reopened.document.crop,
        state.session.borrow().canvas.document.crop
    );
}

#[test]
fn adjustment_callbacks_are_scoped_to_the_active_adjustment_layer() {
    set_platform();
    let app = PhotoApp::new().expect("create PhotoApp");
    let state = Rc::new(scripted_state());
    {
        let mut session = state.session.borrow_mut();
        session.add_adjustment("brightness", "Brightness", "brightness", 0.0);
        session.add_adjustment("contrast", "Contrast", "contrast", 0.0);
        assert!(session.canvas.document.select_layer(1));
    }
    wire_adjustment_callbacks(&app, &state);
    refresh_photo_with_state(&app, &state).expect("initial refresh");
    assert!(app.get_brightness_enabled());
    assert!(!app.get_contrast_enabled());
    app.invoke_brightness_changed(40.0);
    assert!(
        (state.session.borrow().canvas.document.layers[1].adjustment_value - 0.4).abs() < 0.001
    );

    state.session.borrow_mut().canvas.document.select_layer(2);
    refresh_photo_with_state(&app, &state).expect("contrast refresh");
    assert!(!app.get_brightness_enabled());
    assert!(app.get_contrast_enabled());
    app.invoke_brightness_changed(90.0);
    assert!(
        (state.session.borrow().canvas.document.layers[1].adjustment_value - 0.4).abs() < 0.001
    );
    app.invoke_contrast_changed(-30.0);
    assert!(
        (state.session.borrow().canvas.document.layers[2].adjustment_value + 0.3).abs() < 0.001
    );
}

#[test]
fn photo_menu_uses_canonical_commands_and_disables_unhandled_items() {
    set_platform();
    let app = PhotoApp::new().expect("create PhotoApp");
    let menu = build_photo_menu_bar(&app);

    for id in [
        "file.new",
        "file.open",
        "file.save",
        "file.save_as",
        "file.import_image",
        "file.export_png",
        "file.export_jpeg",
        "edit.undo",
        "edit.redo",
        "layer.new",
        "layer.adjustment",
        "layer.delete",
        "layer.move_up",
        "layer.move_down",
        "view.inspector",
        "app.palette",
    ] {
        assert!(
            menu.find_item(id).is_some(),
            "missing Photo menu command {id}"
        );
    }
    for id in [
        "edit.cut",
        "edit.copy",
        "edit.paste",
        "edit.select_all",
        "view.zoom_in",
        "view.zoom_out",
        "view.zoom_actual",
        "layer.duplicate",
    ] {
        if let Some(item) = menu.find_item(id) {
            assert!(
                !item.is_enabled(),
                "unhandled Photo command {id} must be disabled"
            );
        }
    }
}

#[test]
fn photo_menu_projection_tracks_history_layer_boundaries_and_inspector() {
    set_platform();
    let app = PhotoApp::new().expect("create PhotoApp");
    configure_responsive_layout(&app, (1280, 800));
    let state = scripted_state();
    let menu = NativeMenuBar::new();
    menu.install_menu_bar(&build_photo_menu_bar(&app))
        .expect("install menu");

    sync_menu_state(&menu, &app, &state);
    let installed = menu.installed_menu_bar().expect("installed menu");
    assert!(matches!(
        installed.find_item("edit.undo"),
        Some(MenuItem::Action { enabled: false, .. })
    ));
    assert!(matches!(
        installed.find_item("edit.redo"),
        Some(MenuItem::Action { enabled: false, .. })
    ));
    assert!(matches!(
        installed.find_item("layer.delete"),
        Some(MenuItem::Action { enabled: false, .. })
    ));
    assert!(matches!(
        installed.find_item("layer.move_up"),
        Some(MenuItem::Action { enabled: false, .. })
    ));
    assert!(matches!(
        installed.find_item("layer.move_down"),
        Some(MenuItem::Action { enabled: false, .. })
    ));
    assert!(matches!(
        installed.find_item("view.inspector"),
        Some(MenuItem::Check {
            checked,
            enabled: true,
            ..
        }) if *checked == app.get_show_inspector()
    ));

    state
        .session
        .borrow_mut()
        .add_pixel_layer("layer-2", "Pixel Layer 2");
    sync_menu_state(&menu, &app, &state);
    let installed = menu.installed_menu_bar().expect("installed menu");
    assert!(matches!(
        installed.find_item("layer.delete"),
        Some(MenuItem::Action { enabled: true, .. })
    ));
    assert!(matches!(
        installed.find_item("layer.move_up"),
        Some(MenuItem::Action { enabled: false, .. })
    ));
    assert!(matches!(
        installed.find_item("layer.move_down"),
        Some(MenuItem::Action { enabled: true, .. })
    ));

    state.session.borrow_mut().canvas.document.select_layer(0);
    sync_menu_state(&menu, &app, &state);
    let installed = menu.installed_menu_bar().expect("installed menu");
    assert!(matches!(
        installed.find_item("layer.move_up"),
        Some(MenuItem::Action { enabled: true, .. })
    ));
    assert!(matches!(
        installed.find_item("layer.move_down"),
        Some(MenuItem::Action { enabled: false, .. })
    ));

    app.set_show_inspector(false);
    sync_menu_state(&menu, &app, &state);
    assert!(matches!(
        menu.installed_menu_bar()
            .and_then(|bar| bar.find_item("view.inspector").cloned()),
        Some(MenuItem::Check {
            checked: false,
            enabled: true,
            ..
        })
    ));
}

#[test]
fn photo_menu_sink_preserves_source_mutates_once_and_guards_boundaries() {
    set_platform();
    let app = PhotoApp::new().expect("create PhotoApp");
    let state = Rc::new(scripted_state());
    let menu = Rc::new(NativeMenuBar::new());
    menu.install_menu_bar(&build_photo_menu_bar(&app))
        .expect("install menu");

    wire_add_layer_callback(&app, &state);
    wire_move_layer_callback(&app, &state);
    let app_ref = app.as_weak();
    let observed_source = std::sync::Arc::new(std::sync::Mutex::new(None));
    let observed_source_ref = observed_source.clone();
    menu.register_action_sink(std::sync::Arc::new(
        move |action: loom_desktop::CommandAction| {
            *observed_source_ref.lock().expect("source lock") = Some(action.source);
            let app = app_ref.upgrade().ok_or_else(|| {
                loom_desktop::DesktopError::InvalidRequest("Photo app was dropped".into())
            })?;
            if dispatch_command(&app, &action.id) {
                Ok(())
            } else {
                Err(loom_desktop::DesktopError::InvalidRequest(format!(
                    "unsupported Photo menu command {}",
                    action.id
                )))
            }
        },
    ))
    .expect("register sink");

    sync_menu_state(&menu, &app, &state);
    let before = state.session.borrow().canvas.document.layers.len();
    menu.dispatch_action_from("layer.new", loom_desktop::CommandSource::Menu)
        .expect("enabled layer.new");
    assert_eq!(
        state.session.borrow().canvas.document.layers.len(),
        before + 1
    );
    assert_eq!(
        *observed_source.lock().expect("source lock"),
        Some(loom_desktop::CommandSource::Menu)
    );

    state.session.borrow_mut().canvas.document.select_layer(0);
    sync_menu_state(&menu, &app, &state);
    let order_before = state
        .session
        .borrow()
        .canvas
        .document
        .layers
        .iter()
        .map(|layer| layer.id.clone())
        .collect::<Vec<_>>();
    assert_eq!(order_before, vec!["layer-bg", "layer-2"]);
    menu.dispatch_action_from("layer.move_up", loom_desktop::CommandSource::Menu)
        .expect("enabled layer.move_up");
    let session = state.session.borrow();
    assert_eq!(session.canvas.document.active_layer_index, 1);
    assert_eq!(
        session
            .canvas
            .document
            .layers
            .iter()
            .map(|layer| layer.id.as_str())
            .collect::<Vec<_>>(),
        vec!["layer-2", "layer-bg"]
    );
    drop(session);

    *state.session.borrow_mut() = PhotoSession::new(blank_canvas().expect("blank canvas"));
    sync_menu_state(&menu, &app, &state);
    let before = state.session.borrow().canvas.document.layers.len();
    assert!(menu
        .dispatch_action_from("layer.delete", loom_desktop::CommandSource::Menu)
        .is_err());
    assert_eq!(
        state.session.borrow().canvas.document.layers.len(),
        before,
        "one-layer delete stays unchanged"
    );
    assert!(menu
        .dispatch_action_from("layer.move_up", loom_desktop::CommandSource::Menu)
        .is_err());
    assert!(menu
        .dispatch_action_from("layer.move_down", loom_desktop::CommandSource::Menu)
        .is_err());
    assert_eq!(
        state.session.borrow().canvas.document.active_layer_index,
        0,
        "one-layer move boundaries stay unchanged"
    );
}

#[test]
fn photo_toolbar_inspector_toggle_reprojects_checked_state() {
    set_platform();
    let app = PhotoApp::new().expect("create PhotoApp");
    configure_responsive_layout(&app, (1280, 800));
    let menu = Rc::new(NativeMenuBar::new());
    menu.install_menu_bar(&build_photo_menu_bar(&app))
        .expect("install menu");
    let mut gui_state = scripted_state();
    gui_state.menu_service = Some(menu.clone());
    let state = Rc::new(gui_state);
    wire_inspector_callback(&app, &state);
    sync_menu_state(&menu, &app, &state);
    assert!(app.get_show_inspector());

    app.invoke_toggle_inspector();

    assert!(!app.get_show_inspector());
    assert!(matches!(
        menu.installed_menu_bar()
            .and_then(|bar| bar.find_item("view.inspector").cloned()),
        Some(MenuItem::Check {
            checked: false,
            enabled: true,
            ..
        })
    ));
}

#[test]
fn photo_palette_move_commands_follow_layer_boundaries() {
    set_platform();
    let app = PhotoApp::new().expect("create PhotoApp");
    let state = scripted_state();

    let has_command = |id: &str| master_palette(&app).iter().any(|command| command.id == id);

    refresh_photo(&app, &state.session.borrow()).expect("refresh first layer");
    assert!(!app.get_can_move_up());
    assert!(!app.get_can_move_down());
    assert!(!has_command("photo.layer.move-up"));
    assert!(!has_command("photo.layer.move-down"));

    state
        .session
        .borrow_mut()
        .add_pixel_layer("layer-2", "Pixel Layer 2");
    refresh_photo(&app, &state.session.borrow()).expect("refresh last layer");
    assert!(!app.get_can_move_up());
    assert!(app.get_can_move_down());
    assert!(!has_command("photo.layer.move-up"));
    assert!(has_command("photo.layer.move-down"));

    state
        .session
        .borrow_mut()
        .add_pixel_layer("layer-3", "Pixel Layer 3");
    state.session.borrow_mut().canvas.document.select_layer(1);
    refresh_photo(&app, &state.session.borrow()).expect("refresh middle layer");
    assert!(app.get_can_move_up());
    assert!(app.get_can_move_down());
    assert!(has_command("photo.layer.move-up"));
    assert!(has_command("photo.layer.move-down"));

    state.session.borrow_mut().canvas.document.select_layer(0);
    refresh_photo(&app, &state.session.borrow()).expect("refresh first layer");
    assert!(app.get_can_move_up());
    assert!(!app.get_can_move_down());
    assert!(has_command("photo.layer.move-up"));
    assert!(!has_command("photo.layer.move-down"));
}

#[test]
fn photo_palette_invocation_uses_visible_action_after_boundary_change() {
    set_platform();
    let app = PhotoApp::new().expect("create PhotoApp");
    let state = Rc::new(scripted_state());
    wire_move_layer_callback(&app, &state);
    wire_palette(&app);

    state
        .session
        .borrow_mut()
        .add_pixel_layer("layer-2", "Pixel Layer 2");
    refresh_photo(&app, &state.session.borrow()).expect("refresh last layer");
    app.set_palette_query("move layer".into());
    rebuild_palette(&app, "move layer");
    assert_eq!(app.get_palette_commands().row_count(), 1);
    assert_eq!(
        app.get_palette_commands()
            .row_data(0)
            .expect("visible move command")
            .id,
        "photo.layer.move-down"
    );

    state.session.borrow_mut().canvas.document.select_layer(0);
    refresh_photo(&app, &state.session.borrow()).expect("refresh first layer");
    assert!(!app.get_can_move_down());
    app.invoke_palette_invoked(0);
    assert_eq!(
        state.session.borrow().canvas.document.active_layer_index,
        0,
        "stale visible move-down must not dispatch move-up"
    );
}

#[test]
fn photo_overflow_palette_exposes_zoom_and_inspector() {
    set_platform();
    let app = PhotoApp::new().expect("create PhotoApp");

    configure_responsive_width(&app, 1180);
    rebuild_palette(&app, "");
    let ids = (0..app.get_palette_commands().row_count())
        .filter_map(|index| app.get_palette_commands().row_data(index))
        .map(|item| item.id.to_string())
        .collect::<Vec<_>>();
    assert!(ids.iter().any(|id| id == "photo.zoom"));
    assert!(ids.iter().any(|id| id == "photo.inspector"));

    configure_responsive_width(&app, 1179);
    rebuild_palette(&app, "inspector");
    assert_eq!(app.get_palette_commands().row_count(), 0);
}

#[test]
fn photo_overflow_palette_invocation_mutates_zoom_and_inspector_state() {
    set_platform();
    let app = PhotoApp::new().expect("create PhotoApp");
    configure_responsive_width(&app, 1180);
    let state = Rc::new(scripted_state());
    wire_inspector_callback(&app, &state);
    wire_palette(&app);

    app.set_zoom_value(100.0);
    app.set_palette_query("zoom".into());
    rebuild_palette(&app, "zoom");
    assert_eq!(app.get_palette_commands().row_count(), 1);
    assert_eq!(
        app.get_palette_commands()
            .row_data(0)
            .expect("zoom command")
            .id,
        "photo.zoom"
    );
    app.invoke_palette_invoked(0);
    assert_eq!(app.get_zoom_value(), 125.0);

    app.set_palette_query("inspector".into());
    rebuild_palette(&app, "inspector");
    assert_eq!(app.get_palette_commands().row_count(), 1);
    let before = app.get_show_inspector();
    app.invoke_palette_invoked(0);
    assert_ne!(app.get_show_inspector(), before);
}
