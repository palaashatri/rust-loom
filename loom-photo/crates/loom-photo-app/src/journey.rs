use super::*;

fn capture_photo_journey_step(
    app: &PhotoApp,
    state: &GuiState,
    args: &Args,
    out_dir: &Path,
    index: usize,
    name: &str,
) -> Result<String, String> {
    let image = snapshot_component(app, args.size.0 as f32, args.size.1 as f32, 1.0)
        .map_err(|error| error.to_string())?;
    let file_name = format!("photo-vertical-{index:02}-{name}.png");
    let path = out_dir.join(&file_name);
    loom_test_support::png::save_png(&path, &image).map_err(|error| error.to_string())?;
    let session = state.session.borrow();
    let active = session.canvas.document.active_layer();
    Ok(format!(
        "{index:02} {name} status={:?} tab={} scroll={} active={} transform={:?} selection={} crop={} undo={} pixels={}",
        app.get_status_left().as_str(),
        app.get_inspector_tab(),
        app.get_inspector_scroll_y(),
        active.map(|layer| layer.id.as_str()).unwrap_or("none"),
        active.map(|layer| layer.transform),
        format_rect(session.canvas.document.selection),
        format_rect(session.canvas.document.crop),
        session.can_undo(),
        session.canvas.pixel_payload_count(),
    ))
}

fn payload_digests(canvas: &PhotoCanvas) -> Vec<Option<u64>> {
    canvas
        .document
        .layers
        .iter()
        .map(|layer| canvas.layer_image(&layer.id).map(RgbaImage::pixel_digest))
        .collect()
}

/// Record the controller-backed Photo editing journey with per-step screenshots. The existing
/// keyboard palette recorder remains a separate regression at the end of this journey.
pub(super) fn run_journey(args: &Args, out_dir: &str) -> Result<(), String> {
    set_platform();
    let out_dir = Path::new(out_dir);
    std::fs::create_dir_all(out_dir)
        .map_err(|error| format!("create journey directory: {error}"))?;

    let source_path = out_dir.join("photo-import-source.png");
    let invalid_path = out_dir.join("photo-import-invalid.png");
    std::fs::write(&source_path, sample_raster_payload()?)
        .map_err(|error| format!("write journey source: {error}"))?;
    std::fs::write(&invalid_path, b"not a raster image")
        .map_err(|error| format!("write invalid journey source: {error}"))?;
    let save_path = out_dir.join("photo-vertical.loomphoto");
    let export_path = out_dir.join("photo-vertical.png");
    let failing_export_path = out_dir.join("photo-export-failure.png");
    if failing_export_path.exists() {
        std::fs::remove_dir_all(&failing_export_path)
            .map_err(|error| format!("remove stale failure target: {error}"))?;
    }
    std::fs::create_dir(&failing_export_path)
        .map_err(|error| format!("create failure target: {error}"))?;

    let app = PhotoApp::new().map_err(|error| error.to_string())?;
    configure_direction(&app, args.rtl);
    apply_theme(&app, &args.theme);
    configure_responsive_layout(&app, args.size);
    let dialogs = Rc::new(ScriptedFileDialogs::new(
        [Some(source_path.clone()), Some(invalid_path.clone()), None],
        [
            Some(save_path.clone()),
            Some(export_path.clone()),
            Some(failing_export_path.clone()),
        ],
    ));
    let state = Rc::new(new_gui_state(
        PhotoSession::new(initial_canvas(args)?),
        None,
        dialogs,
    )?);
    wire_transform_callback(&app, &state);
    wire_selection_callbacks(&app, &state);
    wire_add_adjustment_callback(&app, &state);
    wire_adjustment_callbacks(&app, &state);
    wire_tool_callback(&app);
    wire_import_callback(&app, &state);
    wire_export_callbacks(&app, &state);
    refresh_photo_with_state(&app, &state)?;
    app.window()
        .set_size(PhysicalSize::new(args.size.0, args.size.1));
    let mut steps = Vec::new();
    steps.push(capture_photo_journey_step(
        &app, &state, args, out_dir, 0, "initial",
    )?);

    app.invoke_import_image();
    let imported_id = state
        .session
        .borrow()
        .canvas
        .document
        .active_layer()
        .map(|layer| layer.id.clone())
        .ok_or("journey import has no layer")?;
    if !imported_id.starts_with("layer-imported-")
        || state.session.borrow().canvas.pixel_payload_count() != 1
    {
        return Err("journey import did not preserve a real identified payload".into());
    }
    refresh_photo_with_state(&app, &state)?;
    steps.push(capture_photo_journey_step(
        &app, &state, args, out_dir, 1, "imported",
    )?);

    state.session.borrow_mut().canvas.document.select_layer(0);
    set_status(&app, "Selected imported layer");
    refresh_photo_with_state(&app, &state)?;
    steps.push(capture_photo_journey_step(
        &app, &state, args, out_dir, 2, "selected",
    )?);

    app.invoke_layer_transform_changed(36.0, 20.0, 118.0, 96.0, 8.0);
    {
        let session = state.session.borrow();
        let transformed = &session.canvas.document.layers[0].transform;
        if transformed.tx.abs() < f32::EPSILON || transformed.ty.abs() < f32::EPSILON {
            return Err("journey transform did not mutate the imported layer".into());
        }
    }
    set_status(&app, "Transformed selected layer");
    steps.push(capture_photo_journey_step(
        &app,
        &state,
        args,
        out_dir,
        3,
        "transformed",
    )?);
    app.invoke_select_tool("pan".into());
    app.set_zoom_value(125.0);
    app.set_viewport_pan_x(36.0);
    app.set_viewport_pan_y(-18.0);
    if !app.get_pan_mode()
        || (app.get_zoom_value() - 125.0).abs() > 0.001
        || (app.get_viewport_pan_x() - 36.0).abs() > 0.001
        || (app.get_viewport_pan_y() + 18.0).abs() > 0.001
    {
        return Err("journey pan/zoom did not persist non-default viewport state".into());
    }
    set_status(&app, "Panned viewport at 125% zoom");
    steps.push(capture_photo_journey_step(
        &app, &state, args, out_dir, 4, "pan-zoom",
    )?);

    app.invoke_select_layer_bounds();
    if !app.get_has_selected_layer_bounds() {
        return Err("journey layer bounds selection did not expose bounds".into());
    }
    set_status(&app, "Selected transformed layer bounds");
    steps.push(capture_photo_journey_step(
        &app,
        &state,
        args,
        out_dir,
        5,
        "selection",
    )?);
    app.invoke_crop_to_selection();
    if state.session.borrow().canvas.document.crop.is_none() {
        return Err("journey canvas crop callback did not mutate document state".into());
    }
    set_status(&app, "Cropped preview to selection");
    steps.push(capture_photo_journey_step(
        &app, &state, args, out_dir, 6, "cropped",
    )?);

    app.invoke_crop_active_layer_to_selection();
    {
        let session = state.session.borrow();
        if session.canvas.document.layers[0].crop.is_none() || !app.get_has_active_layer_crop() {
            return Err("journey active-layer crop callback did not mutate document state".into());
        }
    }
    set_status(&app, "Cropped active layer to selection");
    steps.push(capture_photo_journey_step(
        &app,
        &state,
        args,
        out_dir,
        7,
        "layer-cropped",
    )?);

    app.invoke_add_adjustment();
    {
        let session = state.session.borrow();
        let Some(active) = session.canvas.document.active_layer() else {
            return Err("journey adjustment layer was not created".into());
        };
        if active.kind != loom_photo_core::LayerKind::Adjustment
            || active.adjustment_type.as_deref() != Some("brightness")
        {
            return Err("journey adjustment did not create an active brightness layer".into());
        }
    }
    set_status(&app, "Added active brightness adjustment");
    app.set_inspector_scroll_y(-460.0);
    if app.get_inspector_scroll_y() >= 0.0 {
        return Err("journey inspector did not accept a lower-content scroll position".into());
    }
    steps.push(capture_photo_journey_step(
        &app,
        &state,
        args,
        out_dir,
        8,
        "adjustment-layer",
    )?);

    app.invoke_brightness_changed(35.0);
    if (adjustment_value(&state.session.borrow().canvas.document, "brightness") - 0.35).abs()
        > 0.001
    {
        return Err("journey adjustment did not mutate active layer state".into());
    }
    set_status(&app, "Adjusted brightness");

    // A long Adjust panel can legitimately scroll below the fold, but that
    // offset must not carry into the shorter Layers or Export panels. Render
    // after each transition so the deferred Slint change handler is evaluated,
    // then assert the app-owned scroll state before recording the evidence.
    app.set_inspector_tab(1);
    snapshot_component(&app, args.size.0 as f32, args.size.1 as f32, 1.0)
        .map_err(|error| format!("render Layers tab: {error}"))?;
    if app.get_inspector_scroll_y() != 0.0 {
        return Err("journey Layers tab retained the Adjust scroll offset".into());
    }
    app.set_inspector_tab(2);
    snapshot_component(&app, args.size.0 as f32, args.size.1 as f32, 1.0)
        .map_err(|error| format!("render Export tab: {error}"))?;
    if app.get_inspector_scroll_y() != 0.0 {
        return Err("journey Export tab retained the Adjust scroll offset".into());
    }
    steps.push(capture_photo_journey_step(
        &app,
        &state,
        args,
        out_dir,
        9,
        "adjusted-tab-reset",
    )?);

    if !state.session.borrow_mut().undo() {
        return Err("journey adjustment undo was unavailable".into());
    }
    if adjustment_value(&state.session.borrow().canvas.document, "brightness").abs() > 0.001 {
        return Err("journey undo did not restore brightness".into());
    }
    set_status(&app, "Undid brightness adjustment");
    app.set_inspector_tab(0);
    snapshot_component(&app, args.size.0 as f32, args.size.1 as f32, 1.0)
        .map_err(|error| format!("render Adjust tab after undo: {error}"))?;
    refresh_photo_with_state(&app, &state)?;
    steps.push(capture_photo_journey_step(
        &app,
        &state,
        args,
        out_dir,
        10,
        "undo-adjustment",
    )?);
    let (pre_save_metadata, pre_save_payloads, pre_save_composite_digest) = {
        let session = state.session.borrow();
        let composite = session.canvas.composite()?;
        (
            session.canvas.document.metadata_digest(),
            payload_digests(&session.canvas),
            composite.pixel_digest(),
        )
    };
    save_current_project(&app, &state, false)?;
    steps.push(capture_photo_journey_step(
        &app, &state, args, out_dir, 11, "saved",
    )?);
    let saved_bytes =
        std::fs::read(&save_path).map_err(|error| format!("read saved project: {error}"))?;
    let saved_canvas = load_photo_canvas(&saved_bytes)?;
    let reopened_metadata = saved_canvas.document.metadata_digest();
    let reopened_payloads = payload_digests(&saved_canvas);
    let reopened_composite_digest = saved_canvas.composite()?.pixel_digest();
    if reopened_metadata != pre_save_metadata
        || reopened_payloads != pre_save_payloads
        || reopened_composite_digest != pre_save_composite_digest
    {
        return Err("journey save/reopen changed metadata, payload, or composite state".into());
    }
    *state.session.borrow_mut() = PhotoSession::new(saved_canvas);
    *state.save_path.borrow_mut() = Some(save_path.clone());
    refresh_photo_with_state(&app, &state)?;
    steps.push(capture_photo_journey_step(
        &app, &state, args, out_dir, 12, "reopened",
    )?);

    app.invoke_export_png();
    let exported =
        std::fs::read(&export_path).map_err(|error| format!("read exported PNG: {error}"))?;
    let decoded_export = decode_raster(&exported)?;
    let expected_export = state.session.borrow().canvas.composite()?;
    if decoded_export != expected_export {
        return Err("journey PNG export pixels did not match the compositor".into());
    }
    steps.push(capture_photo_journey_step(
        &app, &state, args, out_dir, 13, "exported",
    )?);

    app.invoke_import_image();
    if !app.get_status_left().as_str().contains("Import failed") {
        return Err("journey invalid import did not produce actionable feedback".into());
    }
    steps.push(capture_photo_journey_step(
        &app,
        &state,
        args,
        out_dir,
        14,
        "import-failure",
    )?);

    app.invoke_export_png();
    if !app.get_status_left().as_str().contains("PNG export failed") {
        return Err("journey invalid export did not produce actionable feedback".into());
    }
    steps.push(capture_photo_journey_step(
        &app,
        &state,
        args,
        out_dir,
        15,
        "export-failure",
    )?);

    app.invoke_import_image();
    if !app.get_status_left().as_str().contains("Import cancelled") {
        return Err("journey import cancellation response was not observed".into());
    }
    steps.push(capture_photo_journey_step(
        &app,
        &state,
        args,
        out_dir,
        16,
        "import-cancel",
    )?);

    wire_palette(&app);
    rebuild_palette(&app, "");
    let report = record_keyboard_palette_journey(&app, "photo", out_dir, "layer")
        .map_err(|error| format!("journey failed: {error}"))?;
    println!(
        "keyboard journey: {} ({})",
        if report.passed { "PASS" } else { "FAIL" },
        out_dir.display()
    );
    if !report.passed {
        return Err("keyboard journey invariants failed".to_string());
    }
    let log = format!(
        "Photo vertical journey: PASS\njourney=import-select-transform-selection-crop-adjust-tab-reset-undo-save-reopen-export-failures\n{}\n",
        steps.join("\n")
    );
    std::fs::write(out_dir.join("photo-vertical.log"), log)
        .map_err(|error| format!("write journey log: {error}"))?;
    println!("photo journey: PASS ({})", out_dir.display());
    Ok(())
}
