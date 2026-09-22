//! Desktop regression and UI integration tests for Loom Motion.

use super::*;
use loom_desktop::ScriptedFileDialogs;

fn test_state_with_dialogs(
    dialogs: Rc<dyn FileDialogService>,
    save_path: Option<PathBuf>,
) -> GuiState {
    GuiState {
        current: RefCell::new(empty_motion()),
        clock: RefCell::new(clock_for_document(&empty_motion())),
        history: RefCell::new(MotionHistory::default()),
        selected_keyframe: RefCell::new(None),
        transform_gesture_active: RefCell::new(false),
        transform_gesture_checkpointed: RefCell::new(false),
        save_path: RefCell::new(save_path),
        dialogs,
        composition_filter: FileFilter::new("Loom Motion composition", ["loommotion"])
            .expect("filter"),
        svg_filter: FileFilter::new("SVG image", ["svg"]).expect("filter"),
    }
}

fn test_state() -> GuiState {
    test_state_with_dialogs(
        Rc::new(ScriptedFileDialogs::default()),
        Some(PathBuf::from("projects/demo.loommotion")),
    )
}

#[test]
fn responsive_policy_transition_probes_are_exact() {
    set_platform();
    let app = MotionApp::new().expect("create MotionApp");
    let expected = [
        (1179, true, true, false, true),
        (1180, false, true, false, false),
        (1279, false, true, false, false),
        (1280, false, true, false, false),
        (1319, false, true, false, false),
        (1320, false, false, true, false),
    ];
    for (width, icon_only, overflow, labeled, inspector_action_labeled) in expected {
        assert_eq!(
            responsive_toolbar_state(&app, width),
            ResponsiveToolbarState {
                icon_only,
                overflow,
                labeled,
                inspector_action_labeled,
            }
        );
        configure_responsive_layout(&app, width);
        assert_eq!(app.get_icon_only_toolbar(), icon_only);
        assert_eq!(app.get_overflow_toolbar(), overflow);
        assert_eq!(app.get_labeled_toolbar(), labeled);
        assert_eq!(app.get_inspector_action_labeled(), inspector_action_labeled);
        assert_eq!(compact_layout_for_width(&app, width), icon_only);
    }
}

#[test]
fn new_motion_composition_is_blank() {
    let document = empty_motion();
    assert_eq!(document.name, "Untitled Composition");
    assert!(document.layers.is_empty());
}

#[test]
fn timecode_uses_the_clock_frame_and_frame_rate() {
    assert_eq!(format_timecode(0, 60.0), "00:00:00:00");
    assert_eq!(format_timecode(61, 60.0), "00:00:01:01");
    assert_eq!(format_timecode(3_723, 24.0), "00:02:35:03");
}

#[test]
fn transform_edit_is_quantized_to_current_frame_and_samples_immediately() {
    let mut document = empty_motion();
    document.frame_rate = 24.0;
    let mut layer = MotionLayer::new("layer", "Layer", "VectorShape");
    layer.add_keyframe("x", 0.0, 10.0);
    document.add_layer(layer);

    assert!(edit_transform_at_frame(&mut document, 13, "x", 42.0));
    assert!(edit_transform_at_frame(&mut document, 13, "x", 84.0));

    let keys = &document.layers[0].position_x_keys;
    assert_eq!(keys.len(), 2);
    assert!((keys[1].time_secs - 13.0 / 24.0).abs() < f32::EPSILON);
    assert_eq!(keys[1].value, 84.0);
    assert_eq!(document.frame(13).layers[0].x, 84.0);
}

#[test]
fn replacing_document_rebases_transport_clock() {
    let state = test_state();
    state.clock.borrow_mut().seek_frame(120);
    state.clock.borrow_mut().set_playing(true);

    let mut replacement = empty_motion();
    replacement.frame_rate = 24.0;
    replacement.duration_secs = 2.0;
    state.replace(replacement).expect("replace recovery state");

    let clock = state.clock.borrow();
    assert_eq!(clock.current_frame, 0);
    assert_eq!(clock.fps, 24.0);
    assert_eq!(clock.out_frame, 48);
    assert!(!clock.is_playing);
}

#[test]
fn selected_layer_keyframes_are_absolute_and_bounded_to_timeline() {
    let mut document = empty_motion();
    document.duration_secs = 4.0;
    let mut layer = MotionLayer::new("layer", "Layer", "Text");
    layer.start_time = 1.0;
    layer.add_keyframe("x", -2.0, 10.0);
    layer.add_keyframe("y", 1.0, 20.0);
    layer.add_keyframe("opacity", 8.0, 30.0);
    document.add_layer(layer);

    assert_eq!(selected_layer_keyframes(&document), vec![2.0]);
}

#[test]
fn transform_edit_rejects_frames_outside_layer_interval() {
    let mut document = empty_motion();
    document.frame_rate = 10.0;
    let mut layer = MotionLayer::new("layer", "Layer", "VectorShape");
    layer.start_time = 2.0;
    layer.duration = 3.0;
    layer.add_keyframe("x", 0.0, 10.0);
    document.add_layer(layer);

    assert!(!edit_transform_at_frame(&mut document, 19, "x", 20.0));
    assert!(!edit_transform_at_frame(&mut document, 51, "x", 30.0));
    assert!(edit_transform_at_frame(&mut document, 20, "x", 40.0));
    assert!(edit_transform_at_frame(&mut document, 49, "x", 50.0));
    assert_eq!(selected_layer_keyframes(&document), vec![2.0, 4.9]);
}

#[test]
fn keyframe_time_move_preserves_channel_order_and_interval() {
    let mut document = empty_motion();
    let mut layer = MotionLayer::new("layer", "Layer", "VectorShape");
    layer.add_keyframe("x", 1.0, 10.0);
    layer.add_keyframe("x", 3.0, 30.0);
    document.add_layer(layer);

    assert!(move_keyframe_at_time(&mut document, "x", 1.0, 2.0));
    assert!(!move_keyframe_at_time(&mut document, "x", 2.0, -1.0));
    assert_eq!(document.layers[0].position_x_keys[0].time_secs, 2.0);
    assert_eq!(document.layers[0].position_x_keys[0].value, 10.0);
}

#[test]
fn rejected_keyframe_time_edit_does_not_create_history_entry() {
    let state = test_state();
    let mut document = empty_motion();
    let mut layer = MotionLayer::new("layer", "Layer", "VectorShape");
    layer.start_time = 2.0;
    layer.duration = 3.0;
    layer.add_keyframe("x", 0.0, 10.0);
    document.add_layer(layer);
    *state.current.borrow_mut() = document.clone();
    let selected = SelectedKeyframe {
        property: "x".into(),
        time_secs: 2.0,
    };

    assert!(apply_keyframe_time_edit(&state, &selected, "x", 0.0).is_err());
    assert!(state.history.borrow().undo.is_empty());
    assert_eq!(*state.current.borrow(), document);
}

#[test]
fn rejected_transform_edit_does_not_create_history_entry() {
    let state = test_state();
    let mut document = empty_motion();
    let mut layer = MotionLayer::new("layer", "Layer", "VectorShape");
    layer.start_time = 2.0;
    layer.duration = 3.0;
    layer.add_keyframe("x", 0.0, 10.0);
    document.add_layer(layer);
    *state.current.borrow_mut() = document.clone();

    assert!(apply_transform_edit(&state, 0, "x", 20.0).is_err());
    assert!(state.history.borrow().undo.is_empty());
    assert_eq!(*state.current.borrow(), document);
}

#[test]
fn layer_timing_projection_uses_safe_values_for_timeline_lanes() {
    let mut document = empty_motion();
    let mut valid = MotionLayer::new("valid", "Valid", "Text");
    valid.start_time = 1.5;
    valid.duration = 3.0;
    let mut invalid = MotionLayer::new("invalid", "Invalid", "Text");
    invalid.start_time = f32::NAN;
    invalid.duration = f32::NEG_INFINITY;
    document.add_layer(valid);
    document.add_layer(invalid);

    assert_eq!(layer_timing(&document), (vec![1.5, 0.0], vec![3.0, 0.0]));
}

#[test]
fn dialog_requests_use_current_directory_and_expected_extensions() {
    let state = test_state();
    let open = open_request(&state);
    let save = save_request(&state);
    let export = export_request(&state);

    assert_eq!(open.initial_directory, Some(PathBuf::from("projects")));
    assert_eq!(open.filters[0].extensions, vec!["loommotion".to_string()]);
    assert_eq!(save.suggested_name.as_deref(), Some("demo.loommotion"));
    assert_eq!(export.suggested_name.as_deref(), Some(EXPORT_FILENAME));
    assert_eq!(export.filters[0].extensions, vec!["svg".to_string()]);
}

#[test]
fn motion_path_round_trip_preserves_composition() {
    let path = std::env::temp_dir().join(format!(
        "loom-motion-roundtrip-{}.loommotion",
        std::process::id()
    ));
    let mut document = empty_motion();
    document.width = 3840;
    document.height = 2160;
    document.frame_rate = 23.976;
    document.duration_secs = 37.5;
    let mut layer = MotionLayer::new("layer", "Layer", "VectorShape");
    layer.start_time = 1.25;
    layer.duration = 12.0;
    layer.add_keyframe("x", 0.0, 120.0);
    layer.add_keyframe("x", 2.0, 840.0);
    layer.add_keyframe("rotation", 1.0, 33.0);
    document.add_layer(layer);
    document.active_layer_index = 0;

    let bytes = save_motion(&document).expect("serialize");
    std::fs::write(&path, bytes).expect("write");
    let loaded = load_motion_path(&path).expect("load");
    let loaded_again = load_motion_path(&path).expect("repeated load");
    let _ = std::fs::remove_file(&path);

    assert_eq!(loaded, document);
    assert_eq!(loaded_again, loaded);
}

#[test]
fn save_then_save_as_changes_path_and_preserves_each_version() {
    let first = std::env::temp_dir().join(format!(
        "loom-motion-save-{}-first.loommotion",
        std::process::id()
    ));
    let second = std::env::temp_dir().join(format!(
        "loom-motion-save-{}-second.loommotion",
        std::process::id()
    ));
    let dialogs: Rc<dyn FileDialogService> = Rc::new(ScriptedFileDialogs::new(
        std::iter::empty::<Option<PathBuf>>(),
        [Some(first.clone()), Some(second.clone())],
    ));
    let state = test_state_with_dialogs(dialogs, None);
    state
        .current
        .borrow_mut()
        .add_layer(MotionLayer::new("layer-a", "Layer A", "VectorShape"));
    let first_document = state.current.borrow().clone();

    let first_result = persist_current_motion(&state, false)
        .expect("first save")
        .expect("first save cancelled");
    assert_eq!(first_result.0, first);
    assert_eq!(*state.save_path.borrow(), Some(first.clone()));

    state
        .current
        .borrow_mut()
        .add_layer(MotionLayer::new("layer-b", "Layer B", "Text"));
    let second_document = state.current.borrow().clone();
    let second_result = persist_current_motion(&state, true)
        .expect("Save As")
        .expect("Save As cancelled");
    assert_eq!(second_result.0, second);
    assert_eq!(*state.save_path.borrow(), Some(second.clone()));
    assert_eq!(
        load_motion_path(&first).expect("load first"),
        first_document
    );
    assert_eq!(
        load_motion_path(&second).expect("load second"),
        second_document
    );

    let _ = std::fs::remove_file(first);
    let _ = std::fs::remove_file(second);
}

#[test]
fn cancelled_save_and_open_leave_state_untouched() {
    let dialogs: Rc<dyn FileDialogService> = Rc::new(ScriptedFileDialogs::new([None], [None]));
    let state = test_state_with_dialogs(dialogs, None);
    state
        .current
        .borrow_mut()
        .add_layer(MotionLayer::new("layer", "Layer", "Text"));
    state.checkpoint("before-cancel");
    let before_document = state.current.borrow().clone();
    let before_path = state.save_path.borrow().clone();
    let before_history = {
        let history = state.history.borrow();
        (history.undo.len(), history.redo.len())
    };

    assert!(persist_current_motion(&state, false)
        .expect("cancelled save")
        .is_none());
    assert!(selected_motion_to_open(&state)
        .expect("cancelled open")
        .is_none());
    assert_eq!(*state.current.borrow(), before_document);
    assert_eq!(*state.save_path.borrow(), before_path);
    let after_history = state.history.borrow();
    assert_eq!(
        (after_history.undo.len(), after_history.redo.len()),
        before_history
    );
}

#[test]
fn open_replaces_document_and_resets_history_boundary() {
    let path = std::env::temp_dir().join(format!(
        "loom-motion-open-{}.loommotion",
        std::process::id()
    ));
    let mut opened = empty_motion();
    opened.name = "Opened Composition".into();
    opened.add_layer(MotionLayer::new("opened-layer", "Opened Layer", "Text"));
    std::fs::write(
        &path,
        save_motion(&opened).expect("serialize opened document"),
    )
    .expect("write opened document");

    let dialogs: Rc<dyn FileDialogService> = Rc::new(ScriptedFileDialogs::new(
        [Some(path.clone())],
        std::iter::empty::<Option<PathBuf>>(),
    ));
    let state = test_state_with_dialogs(dialogs, None);
    state.checkpoint("old-document");
    state
        .current
        .borrow_mut()
        .add_layer(MotionLayer::new("stale", "Stale", "VectorShape"));

    let (opened_path, document) = selected_motion_to_open(&state)
        .expect("open selection")
        .expect("open cancelled");
    state.replace(document).expect("replace recovery state");
    *state.save_path.borrow_mut() = Some(opened_path.clone());

    assert_eq!(*state.current.borrow(), opened);
    assert_eq!(*state.save_path.borrow(), Some(path.clone()));
    let history = state.history.borrow();
    assert!(history.undo.is_empty());
    assert!(history.redo.is_empty());
    let _ = std::fs::remove_file(path);
}

#[test]
fn missing_and_malformed_projects_return_actionable_errors() {
    let missing = std::env::temp_dir().join(format!(
        "loom-motion-missing-{}.loommotion",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&missing);
    let missing_error = load_motion_path(&missing).expect_err("missing project loaded");
    assert!(missing_error.contains("failed to read motion composition"));

    let malformed = std::env::temp_dir().join(format!(
        "loom-motion-malformed-{}.loommotion",
        std::process::id()
    ));
    std::fs::write(&malformed, b"not a Loom Motion package").expect("write malformed");
    let malformed_error = load_motion_path(&malformed).expect_err("malformed project loaded");
    assert!(!malformed_error.trim().is_empty());
    let _ = std::fs::remove_file(malformed);
}

#[test]
fn read_only_destination_reports_failure_without_changing_active_path() {
    let path = std::env::temp_dir().join(format!(
        "loom-motion-read-only-{}.loommotion",
        std::process::id()
    ));
    std::fs::write(&path, b"existing").expect("create destination");
    let original_permissions = std::fs::metadata(&path)
        .expect("destination metadata")
        .permissions();
    let mut read_only = original_permissions.clone();
    read_only.set_readonly(true);
    std::fs::set_permissions(&path, read_only).expect("set read-only");

    let state =
        test_state_with_dialogs(Rc::new(ScriptedFileDialogs::default()), Some(path.clone()));
    let result = persist_current_motion(&state, false);

    std::fs::set_permissions(&path, original_permissions).expect("restore permissions");
    let _ = std::fs::remove_file(&path);
    assert!(
        result.is_err(),
        "read-only destination unexpectedly accepted"
    );
    assert_eq!(*state.save_path.borrow(), Some(path));
}

#[cfg(all(unix, not(target_os = "macos")))]
#[test]
fn non_utf8_path_round_trip_is_supported() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let mut name = format!("loom-motion-non-utf8-{}-", std::process::id()).into_bytes();
    name.push(0xff);
    name.extend_from_slice(b".loommotion");
    let path = std::env::temp_dir().join(OsString::from_vec(name));
    let state =
        test_state_with_dialogs(Rc::new(ScriptedFileDialogs::default()), Some(path.clone()));
    state
        .current
        .borrow_mut()
        .add_layer(MotionLayer::new("layer", "Layer", "Text"));
    let expected = state.current.borrow().clone();

    persist_current_motion(&state, false)
        .expect("save non-UTF-8 path")
        .expect("save unexpectedly cancelled");
    assert_eq!(
        load_motion_path(&path).expect("load non-UTF-8 path"),
        expected
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn svg_frame_export_contains_sampled_layers() {
    let svg = export_svg_frame(&sample_motion(), 0.0);
    assert!(svg.starts_with("<?xml"));
    assert!(svg.contains("<svg"));
    assert!(svg.contains("Subtitle Motion"));
    assert!(svg.ends_with("</svg>\n"));
}

#[test]
fn svg_frame_export_uses_document_dimensions_and_sampled_transforms() {
    let mut document = empty_motion();
    document.width = 640;
    document.height = 360;
    let mut shape = MotionLayer::new("shape", "Badge & One", "VectorShape");
    shape.add_keyframe("x", 0.0, 320.0);
    shape.add_keyframe("y", 0.0, 180.0);
    shape.add_keyframe("scale", 0.0, 0.75);
    shape.add_keyframe("rotation", 0.0, 12.5);
    document.add_layer(shape);

    let svg = export_svg_frame(&document, 0.0);
    assert!(svg.contains("width=\"640\" height=\"360\" viewBox=\"0 0 640 360\""));
    assert!(svg.contains("translate(320.000 180.000) rotate(12.500) scale(0.75000)"));
    assert!(svg.contains("fill=\"#b86f4b\""));
}

#[test]
fn svg_frame_export_renders_document_colors_and_text_in_independent_renderer() {
    let mut document = empty_motion();
    document.width = 640;
    document.height = 360;

    let mut shape = MotionLayer::new("shape", "Badge", "VectorShape");
    shape.add_keyframe("x", 0.0, 320.0);
    shape.add_keyframe("y", 0.0, 180.0);
    shape.add_keyframe("scale", 0.0, 0.75);
    shape.add_keyframe("rotation", 0.0, 12.5);
    document.add_layer(shape);

    let mut title = MotionLayer::new("title", "Exact SVG Title", "Text");
    title.add_keyframe("x", 0.0, 480.0);
    title.add_keyframe("y", 0.0, 80.0);
    document.add_layer(title);

    let svg = export_svg_frame(&document, 0.0);
    assert!(svg.contains(">Exact SVG Title</text>"));
    let options = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_data(svg.as_bytes(), &options)
        .expect("exported frame is valid SVG");
    let mut pixmap = resvg::tiny_skia::Pixmap::new(640, 360).expect("create frame image");
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::default(),
        &mut pixmap.as_mut(),
    );

    let pixel = |x: u32, y: u32| -> [u8; 4] {
        let offset = ((y * pixmap.width() + x) * 4) as usize;
        pixmap.data()[offset..offset + 4]
            .try_into()
            .expect("pixel has four channels")
    };
    assert_eq!(tree.size().width(), 640.0);
    assert_eq!(tree.size().height(), 360.0);
    assert_eq!(pixel(0, 0), [16, 18, 23, 255]);
    assert_eq!(pixel(320, 180), [184, 111, 75, 255]);

    let stage_image = stage_frame_image(&project_motion_frame(&document, 0.0))
        .expect("build stage preview from the same SVG frame");
    let stage_pixels = stage_image.to_rgba8().expect("rasterize stage preview");
    assert_eq!(stage_pixels.size().width, 640);
    assert_eq!(stage_pixels.size().height, 360);
    let stage_pixel = |x: u32, y: u32| {
        let offset = (y * stage_pixels.size().width + x) as usize;
        let pixel = stage_pixels.as_slice()[offset];
        [pixel.r, pixel.g, pixel.b, pixel.a]
    };
    assert_eq!(stage_pixel(0, 0), [16, 18, 23, 255]);
    assert_eq!(stage_pixel(320, 180), [184, 111, 75, 255]);
}

#[test]
fn exported_frame_preview_restores_exactly_after_transform_undo() {
    let mut document = empty_motion();
    document.width = 640;
    document.height = 360;
    let mut shape = MotionLayer::new("shape", "Undo Badge", "VectorShape");
    shape.add_keyframe("x", 0.0, 200.0);
    shape.add_keyframe("y", 0.0, 180.0);
    document.add_layer(shape);

    let before_svg = export_svg_frame(&document, 0.0);
    let before_pixels = stage_frame_image(&project_motion_frame(&document, 0.0))
        .expect("build initial preview")
        .to_rgba8()
        .expect("rasterize initial preview")
        .as_slice()
        .to_vec();

    let mut history = MotionHistory::default();
    history.checkpoint(&document, "move-layer");
    assert!(edit_transform_at_frame(&mut document, 0, "x", 440.0));
    let edited_svg = export_svg_frame(&document, 0.0);
    let edited_pixels = stage_frame_image(&project_motion_frame(&document, 0.0))
        .expect("build edited preview")
        .to_rgba8()
        .expect("rasterize edited preview")
        .as_slice()
        .to_vec();
    assert_ne!(edited_svg, before_svg);
    assert_ne!(edited_pixels, before_pixels);

    assert!(history.undo(&mut document));
    assert_eq!(export_svg_frame(&document, 0.0), before_svg);
    let restored_pixels = stage_frame_image(&project_motion_frame(&document, 0.0))
        .expect("build restored preview")
        .to_rgba8()
        .expect("rasterize restored preview")
        .as_slice()
        .to_vec();
    assert_eq!(restored_pixels, before_pixels);
}

#[test]
fn svg_frame_export_uses_non_zero_clock_time() {
    let mut document = empty_motion();
    let mut layer = MotionLayer::new("layer", "Animated", "VectorShape");
    layer.add_keyframe("x", 0.0, 0.0);
    layer.add_keyframe("x", 1.0, 100.0);
    document.add_layer(layer);

    let at_start = export_svg_frame(&document, 0.0);
    let at_playhead = export_svg_frame(&document, 1.0);
    assert!(at_start.contains("translate(0.000 0.000)"));
    assert!(at_playhead.contains("translate(100.000 0.000)"));
    assert_ne!(at_start, at_playhead);
}

#[test]
fn svg_frame_export_omits_layers_outside_their_active_interval() {
    let mut document = empty_motion();
    let mut hidden = MotionLayer::new("hidden", "Hidden Layer", "Text");
    hidden.start_time = 2.0;
    hidden.duration = 1.0;
    hidden.add_keyframe("x", 0.0, 960.0);
    document.add_layer(hidden);

    let before = export_svg_frame(&document, 0.0);
    let during = export_svg_frame(&document, 2.5);
    assert!(!before.contains("Hidden Layer"));
    assert!(during.contains("Hidden Layer"));
}

#[test]
fn motion_history_undoes_and_redoes_edits() {
    let mut current = sample_motion();
    let original = current.clone();
    let mut history = MotionHistory::default();
    history.checkpoint(&current, "add-layer");
    current.add_layer(MotionLayer::new("extra", "Extra", "VectorShape"));
    assert!(history.undo(&mut current));
    assert_eq!(current.layers.len(), original.layers.len());
    assert!(history.redo(&mut current));
    assert_eq!(current.layers.len(), original.layers.len() + 1);
}

#[test]
fn motion_overflow_palette_exposes_and_invokes_zoom() {
    set_platform();
    let app = MotionApp::new().expect("create MotionApp");
    configure_responsive_layout(&app, 1180);
    wire_palette(&app);

    app.set_palette_query("zoom".into());
    rebuild_palette(&app, "zoom");
    assert_eq!(app.get_palette_commands().row_count(), 1);
    assert_eq!(
        app.get_palette_commands()
            .row_data(0)
            .expect("zoom command")
            .id,
        "motion.zoom"
    );

    app.set_zoom_value(100.0);
    app.invoke_palette_invoked(0);
    assert_eq!(app.get_zoom_value(), 125.0);

    app.set_zoom_value(150.0);
    app.set_palette_query("zoom".into());
    rebuild_palette(&app, "zoom");
    app.invoke_palette_invoked(0);
    assert_eq!(app.get_zoom_value(), 100.0);
}
