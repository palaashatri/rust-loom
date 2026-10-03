use super::*;

fn capture_motion_journey_step(
    app: &MotionApp,
    state: &GuiState,
    args: &Args,
    out_dir: &Path,
    index: usize,
    name: &str,
) -> Result<serde_json::Value, String> {
    let image = snapshot_component(app, args.size.0 as f32, args.size.1 as f32, 1.0)
        .map_err(|error| format!("capture {name}: {error}"))?;
    let screenshot = format!("motion-journey-{index:02}-{name}.png");
    loom_test_support::png::save_png(&out_dir.join(&screenshot), &image)
        .map_err(|error| format!("save {screenshot}: {error}"))?;
    let clock = state.clock.borrow();
    let selected = state.selected_keyframe.borrow().clone();
    Ok(serde_json::json!({
        "name": name,
        "frame": clock.current_frame,
        "time_secs": clock.current_time_seconds(),
        "is_playing": clock.is_playing,
        "selected_keyframe": selected.map(|keyframe| serde_json::json!({
            "property": keyframe.property,
            "time_secs": keyframe.time_secs,
        })),
        "layers": state.current.borrow().len(),
        "screenshot": screenshot,
    }))
}

/// Record a deterministic controller-backed in-app journey.  The scripted
/// desktop service supplies save/open/cancel/failure responses while every
/// screenshot is rendered from the real MotionApp component.
pub(super) fn run_journey(args: &Args, out_dir: &str) -> Result<(), String> {
    set_platform();
    let out_dir = Path::new(out_dir);
    std::fs::create_dir_all(out_dir).map_err(|error| format!("create journey output: {error}"))?;
    let app = MotionApp::new().map_err(|e| e.to_string())?;
    configure_direction(&app, args.rtl);
    configure_responsive_layout(&app, args.size.0);
    apply_theme(&app, &args.theme);
    let save_path = std::env::temp_dir().join(format!(
        "loom-motion-journey-{}.loommotion",
        std::process::id()
    ));
    let export_path =
        std::env::temp_dir().join(format!("loom-motion-journey-{}.svg", std::process::id()));
    let dialogs: Rc<dyn FileDialogService> = Rc::new(loom_desktop::ScriptedFileDialogs::new(
        [Some(save_path.clone()), None],
        [Some(save_path.clone()), Some(std::env::temp_dir())],
    ));
    let mut document = initial_motion(args)?;
    if let Some(layer) = document.layers.first_mut() {
        layer.add_keyframe("x", 1.0, 1_040.0);
    }
    let initial_clock = clock_for_document(&document);
    let state = GuiState {
        current: RefCell::new(document),
        clock: RefCell::new(initial_clock),
        history: RefCell::new(MotionHistory::default()),
        selected_keyframe: RefCell::new(None),
        transform_gesture_active: RefCell::new(false),
        transform_gesture_checkpointed: RefCell::new(false),
        save_path: RefCell::new(None),
        dialogs,
        composition_filter: FileFilter::new("Loom Motion composition", ["loommotion"])
            .map_err(|error| error.to_string())?,
        svg_filter: FileFilter::new("SVG image", ["svg"]).map_err(|error| error.to_string())?,
    };
    // Keep transport and sampled state aligned with the edited document.
    *state.clock.borrow_mut() = clock_for_document(&state.current.borrow());
    apply_motion(&app, &state.current.borrow())?;
    app.window()
        .set_size(PhysicalSize::new(args.size.0, args.size.1));
    let mut steps = Vec::new();
    steps.push(capture_motion_journey_step(
        &app, &state, args, out_dir, 0, "initial",
    )?);

    let selected = selected_layer_keyframe_markers(&state.current.borrow())
        .into_iter()
        .find(|(_, property)| property == "x")
        .ok_or("journey fixture has no x keyframe")?;
    *state.selected_keyframe.borrow_mut() = Some(SelectedKeyframe {
        property: selected.1,
        time_secs: selected.0,
    });
    state.clock.borrow_mut().seek_seconds(selected.0 as f64);
    refresh_motion(&app, &state);
    steps.push(capture_motion_journey_step(
        &app,
        &state,
        args,
        out_dir,
        1,
        "select-keyframe",
    )?);

    state.checkpoint("journey-keyframe-time");
    let moved_time = selected.0 + 0.5;
    if !move_keyframe_at_time(&mut state.current.borrow_mut(), "x", selected.0, moved_time) {
        return Err("journey keyframe timing edit was rejected".into());
    }
    *state.selected_keyframe.borrow_mut() = Some(SelectedKeyframe {
        property: "x".into(),
        time_secs: moved_time,
    });
    state.clock.borrow_mut().seek_seconds(moved_time as f64);
    state.record_recovery()?;
    refresh_motion(&app, &state);
    steps.push(capture_motion_journey_step(
        &app,
        &state,
        args,
        out_dir,
        2,
        "edit-keyframe",
    )?);

    state.clock.borrow_mut().set_playing(true);
    state.clock.borrow_mut().advance_seconds(0.5);
    state.clock.borrow_mut().set_playing(false);
    state.clock.borrow_mut().seek_seconds(2.0);
    refresh_motion(&app, &state);
    steps.push(capture_motion_journey_step(
        &app,
        &state,
        args,
        out_dir,
        3,
        "play-seek",
    )?);

    // Exercise the same validated mutation path used by the production
    // `on_transform_changed` callback. Calling the generated callback here
    // would only dispatch to listeners registered on the app component; the
    // journey owns a standalone GuiState, so invoke the controller operation
    // directly and assert that it changed the sampled document.
    let (transform_frame, transform_time, active_layer, previous_transform_y) = {
        let clock = state.clock.borrow();
        let current = state.current.borrow();
        let active_layer = current.active_layer_index;
        let transform_time = clock.current_time_seconds() as f32;
        let previous_transform_y = current
            .layers
            .get(active_layer)
            .map(|layer| layer.sample(transform_time).y)
            .ok_or("journey transform fixture has no active layer")?;
        (
            clock.current_frame,
            transform_time,
            active_layer,
            previous_transform_y,
        )
    };
    if !previous_transform_y.is_finite() || (previous_transform_y - 250.0).abs() <= f32::EPSILON {
        return Err("journey transform fixture has no distinct pre-edit y value".into());
    }
    apply_transform_edit(&state, transform_frame, "y", 250.0)?;
    let edited_transform_y = state
        .current
        .borrow()
        .layers
        .get(active_layer)
        .map(|layer| layer.sample(transform_time).y)
        .ok_or("journey transform edit removed the active layer")?;
    if !edited_transform_y.is_finite() || (edited_transform_y - 250.0).abs() > 0.001 {
        return Err(format!(
            "journey transform edit did not mutate y (sampled {edited_transform_y:.3})"
        ));
    }
    state.record_recovery()?;
    refresh_motion(&app, &state);
    steps.push(capture_motion_journey_step(
        &app,
        &state,
        args,
        out_dir,
        4,
        "edit-transform",
    )?);

    if !state
        .history
        .borrow_mut()
        .undo(&mut state.current.borrow_mut())
    {
        return Err("journey undo did not revert the transform edit".into());
    }
    let undone_transform_y = state
        .current
        .borrow()
        .layers
        .get(active_layer)
        .map(|layer| layer.sample(transform_time).y)
        .ok_or("journey transform undo removed the active layer")?;
    if !undone_transform_y.is_finite() || (undone_transform_y - previous_transform_y).abs() > 0.001
    {
        return Err(format!(
            "journey transform undo did not restore y (sampled {undone_transform_y:.3}, expected {previous_transform_y:.3})"
        ));
    }
    state.record_recovery()?;
    refresh_motion(&app, &state);
    steps.push(capture_motion_journey_step(
        &app,
        &state,
        args,
        out_dir,
        5,
        "undo-transform",
    )?);

    if !state
        .history
        .borrow_mut()
        .undo(&mut state.current.borrow_mut())
    {
        return Err("journey undo did not revert the timing edit".into());
    }
    *state.selected_keyframe.borrow_mut() = None;
    state.record_recovery()?;
    refresh_motion(&app, &state);
    steps.push(capture_motion_journey_step(
        &app, &state, args, out_dir, 6, "undo",
    )?);
    if !state
        .history
        .borrow_mut()
        .redo(&mut state.current.borrow_mut())
    {
        return Err("journey redo did not restore the timing edit".into());
    }
    *state.selected_keyframe.borrow_mut() = Some(SelectedKeyframe {
        property: "x".into(),
        time_secs: moved_time,
    });
    state.record_recovery()?;
    refresh_motion(&app, &state);
    steps.push(capture_motion_journey_step(
        &app, &state, args, out_dir, 7, "redo",
    )?);

    if !state
        .history
        .borrow_mut()
        .redo(&mut state.current.borrow_mut())
    {
        return Err("journey redo did not restore the transform edit".into());
    }
    let redone_transform_y = state
        .current
        .borrow()
        .layers
        .get(active_layer)
        .map(|layer| layer.sample(transform_time).y)
        .ok_or("journey transform redo removed the active layer")?;
    if !redone_transform_y.is_finite() || (redone_transform_y - 250.0).abs() > 0.001 {
        return Err(format!(
            "journey transform redo did not restore y (sampled {redone_transform_y:.3})"
        ));
    }
    state.record_recovery()?;
    refresh_motion(&app, &state);
    steps.push(capture_motion_journey_step(
        &app,
        &state,
        args,
        out_dir,
        8,
        "redo-transform",
    )?);

    let saved =
        persist_current_motion(&state, false)?.ok_or("journey save unexpectedly cancelled")?;
    let saved_document = state.current.borrow().clone();
    steps.push(capture_motion_journey_step(
        &app, &state, args, out_dir, 9, "save",
    )?);

    let (opened_path, reopened) =
        selected_motion_to_open(&state)?.ok_or("journey reopen unexpectedly cancelled")?;
    if reopened != saved_document || opened_path != saved.0 {
        return Err("journey save/reopen did not preserve the document".into());
    }
    state.replace(reopened)?;
    *state.save_path.borrow_mut() = Some(opened_path);
    refresh_motion(&app, &state);
    steps.push(capture_motion_journey_step(
        &app, &state, args, out_dir, 10, "reopen",
    )?);

    write_svg_frame(
        &state.current.borrow(),
        &export_path,
        state.clock.borrow().current_time_seconds() as f32,
    )?;
    let exported = std::fs::read_to_string(&export_path)
        .map_err(|error| format!("read journey export: {error}"))?;
    if !exported.contains("<svg") {
        return Err("journey export did not produce SVG".into());
    }
    steps.push(capture_motion_journey_step(
        &app, &state, args, out_dir, 11, "export",
    )?);

    let cancelled = selected_motion_to_open(&state)?.is_none();
    if !cancelled {
        return Err("journey cancellation response was not observed".into());
    }
    steps.push(capture_motion_journey_step(
        &app, &state, args, out_dir, 12, "cancel",
    )?);

    let failure = persist_current_motion(&state, true).expect_err("journey failure was accepted");
    if !failure.contains("failed to atomic write") {
        return Err(format!("journey failure was not actionable: {failure}"));
    }
    steps.push(capture_motion_journey_step(
        &app, &state, args, out_dir, 13, "failure",
    )?);

    let report = serde_json::json!({
        "app": "motion",
        "journey": "keyframe-selection-edit-play-seek-transform-undo-redo-save-reopen-export-cancel-failure",
        "passed": true,
        "notes": ["Controller-backed deterministic journey with scripted desktop dialogs; screenshots are rendered from the real MotionApp component."],
        "steps": steps,
    });
    std::fs::write(
        out_dir.join("motion.json"),
        serde_json::to_string_pretty(&report).map_err(|error| error.to_string())?,
    )
    .map_err(|error| format!("write journey report: {error}"))?;
    let _ = std::fs::remove_file(save_path);
    let _ = std::fs::remove_file(export_path);
    println!("motion journey: PASS ({})", out_dir.display());
    Ok(())
}
