use super::*;
use loom_desktop::ScriptedFileDialogs;

fn test_app_and_state(scripted: ScriptedFileDialogs) -> (StudioApp, Rc<GuiState>) {
    set_platform();
    let app = StudioApp::new().expect("create StudioApp");
    let (session, assets) = sample_session().expect("sample_session");
    let state = Rc::new(GuiState {
        session: RefCell::new(session),
        assets: RefCell::new(assets),
        save_path: RefCell::new(None),
        dialogs: Rc::new(scripted),
        audio: RefCell::new(None),
        midi_status: RefCell::new("Test MIDI harness".into()),
        metronome_enabled: Cell::new(true),
        audio_error: RefCell::new(Some("test harness".into())),
        gesture: RefCell::new(None),
        job: RefCell::new(JobUiState::default()),
        models: StableModels::new(),
    });
    wire_application(&app, state.clone());
    refresh(&app, &state);
    (app, state)
}

#[test]
fn toggle_metronome_updates_state_and_ui() {
    let scripted = ScriptedFileDialogs::default();
    let (app, state) = test_app_and_state(scripted);
    assert!(state.metronome_enabled.get());
    assert!(app.get_metronome_on());

    app.invoke_toggle_metronome();
    assert!(!state.metronome_enabled.get());
    assert!(!app.get_metronome_on());

    app.invoke_toggle_metronome();
    assert!(state.metronome_enabled.get());
    assert!(app.get_metronome_on());
}

#[test]
fn new_song_creates_untitled_clean_state() {
    let scripted = ScriptedFileDialogs::default();
    let (app, state) = test_app_and_state(scripted);
    *state.save_path.borrow_mut() = Some(PathBuf::from("/tmp/existing.loomstudio"));

    app.invoke_new_song();
    assert_eq!(*state.save_path.borrow(), None);
    assert_eq!(state.session.borrow().project.name, "Untitled Session");
    assert_eq!(app.get_song_title().as_str(), "Untitled Session");
}

#[test]
fn open_song_with_dialog_loads_path_and_updates_state() {
    let dir = std::env::temp_dir().join(format!("loom-studio-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("open_test.loomstudio");

    let mut proj = StudioProject::new("loaded-proj", "Loaded Studio Project");
    proj.tracks.clear();
    let assets = AudioAssetStore::default();
    let bytes = save_studio_bundle(&proj, &assets).unwrap();
    std::fs::write(&file, bytes).unwrap();

    let scripted = ScriptedFileDialogs::new(vec![Some(file.clone())], vec![]);

    let (app, state) = test_app_and_state(scripted);
    // Opening a replacement while transport is active must stop the old
    // cursor/recording state before the new project is projected.
    app.set_playhead_seconds(7.25);
    app.set_is_playing(true);
    app.set_is_recording(true);
    app.invoke_open_song();

    assert_eq!(*state.save_path.borrow(), Some(file));
    assert_eq!(state.session.borrow().project.name, "Loaded Studio Project");
    assert_eq!(app.get_song_title().as_str(), "Loaded Studio Project");
    assert_eq!(app.get_playhead_seconds(), 0.0);
    assert!(!app.get_is_playing());
    assert!(!app.get_is_recording());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn cancelled_open_leaves_current_session_untouched() {
    let scripted = ScriptedFileDialogs::new(vec![None], vec![]);

    let (app, state) = test_app_and_state(scripted);
    let original_name = state.session.borrow().project.name.clone();

    app.invoke_open_song();
    assert_eq!(state.session.borrow().project.name, original_name);
    assert_eq!(app.get_status_left().as_str(), "Open cancelled");
}

#[test]
fn save_untitled_prompts_dialog_and_writes_file() {
    let dir = std::env::temp_dir().join(format!("loom-studio-save-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("saved_project.loomstudio");

    let scripted = ScriptedFileDialogs::new(vec![], vec![Some(file.clone())]);

    let (app, state) = test_app_and_state(scripted);
    assert_eq!(*state.save_path.borrow(), None);

    app.invoke_save_song();

    assert_eq!(*state.save_path.borrow(), Some(file.clone()));
    assert!(file.is_file());
    let read_bytes = std::fs::read(&file).unwrap();
    let (loaded_proj, _) = load_studio_bundle(&read_bytes).unwrap();
    assert_eq!(loaded_proj.name, "Loom Studio Session");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn save_as_prompts_dialog_and_updates_path() {
    let dir = std::env::temp_dir().join(format!("loom-studio-saveas-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file_v1 = dir.join("v1.loomstudio");
    let file_v2 = dir.join("v2.loomstudio");

    let scripted = ScriptedFileDialogs::new(vec![], vec![Some(file_v2.clone())]);

    let (app, state) = test_app_and_state(scripted);
    *state.save_path.borrow_mut() = Some(file_v1);

    app.invoke_save_as_song();

    assert_eq!(*state.save_path.borrow(), Some(file_v2.clone()));
    assert!(file_v2.is_file());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn compact_layout_boundary_keeps_reference_width_stable() {
    set_platform();
    let app = StudioApp::new().expect("create StudioApp");
    assert!(compact_layout_for_width(&app, 1024));
    assert!(compact_layout_for_width(&app, 1179));
    assert!(!compact_layout_for_width(&app, 1180));
    assert!(!compact_layout_for_width(&app, 1440));
}

#[test]
fn responsive_policy_transition_probes_are_exact() {
    set_platform();
    let app = StudioApp::new().expect("create StudioApp");
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
        configure_responsive_layout(&app, width);
        assert_eq!(app.get_icon_only_toolbar(), icon_only);
        assert_eq!(app.get_overflow_toolbar(), overflow);
        assert_eq!(app.get_labeled_toolbar(), labeled);
    }
}

#[test]
fn export_job_can_be_cancelled_and_preserves_destination() {
    let dir = std::env::temp_dir().join(format!("loom-studio-app-bounce-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let destination = dir.join("existing.wav");
    std::fs::write(&destination, b"existing").unwrap();
    let (app, state) = test_app_and_state(ScriptedFileDialogs::default());
    app.invoke_export_mix(destination.to_string_lossy().into_owned().into());
    assert!(matches!(
        state.job.borrow().state,
        StudioJobState::Running { .. }
    ));
    app.invoke_cancel_job();
    assert!(state
        .job
        .borrow()
        .cancellation
        .as_ref()
        .map(StudioCancellation::is_cancelled)
        .unwrap_or(false));
    wait_for_job(&app, &state, Duration::from_secs(5)).unwrap();
    assert!(matches!(
        state.job.borrow().state,
        StudioJobState::Cancelled { .. }
    ));
    assert_eq!(std::fs::read(&destination).unwrap(), b"existing");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn arrangement_selection_gesture_coalesces_and_cancel_rolls_back() {
    let (app, state) = test_app_and_state(ScriptedFileDialogs::default());
    app.invoke_select_region(0, "region-vocal".into());
    assert_eq!(
        state.session.borrow().selection.region_id.as_deref(),
        Some("region-vocal")
    );
    assert_eq!(app.get_selected_region_id().as_str(), "region-vocal");
    let original_start = state.session.borrow().project.tracks[0].regions[0].start_sample;
    app.invoke_begin_region_gesture(0, "region-vocal".into(), "move".into());
    app.invoke_move_region(0, "region-vocal".into(), 0.25);
    app.invoke_move_region(0, "region-vocal".into(), 0.25);
    app.invoke_end_region_gesture();
    assert_eq!(
        state.session.borrow().project.tracks[0].regions[0].start_sample,
        original_start + 24_000
    );
    assert!(state.session.borrow().can_undo());
    app.invoke_undo();
    assert_eq!(
        state.session.borrow().project.tracks[0].regions[0].start_sample,
        original_start
    );

    app.invoke_begin_region_gesture(0, "region-vocal".into(), "move".into());
    app.invoke_move_region(0, "region-vocal".into(), 0.5);
    app.invoke_cancel_region_gesture();
    assert_eq!(
        state.session.borrow().project.tracks[0].regions[0].start_sample,
        original_start
    );
    assert_eq!(app.get_status_left().as_str(), "Region edit cancelled");
}

#[test]
fn order_changing_gesture_keeps_stable_region_identity() {
    let (app, state) = test_app_and_state(ScriptedFileDialogs::default());
    {
        let mut session = state.session.borrow_mut();
        let track = &mut session.project.tracks[0];
        track.regions.clear();
        track.add_region(AudioRegion {
            id: "region-a".into(),
            name: "A.wav".into(),
            start_sample: 0,
            length_samples: 4_800,
        });
        track.add_region(AudioRegion {
            id: "region-b".into(),
            name: "B.wav".into(),
            start_sample: 24_000,
            length_samples: 4_800,
        });
    }
    refresh(&app, &state);

    // The first update moves A past B, so the indexed repeater must
    // re-order.  The second update still targets A by its captured id.
    app.invoke_select_region(0, "region-a".into());
    app.invoke_begin_region_gesture(0, "region-a".into(), "move".into());
    app.invoke_move_region(0, "region-a".into(), 1.0);
    app.invoke_move_region(0, "region-a".into(), 0.5);
    app.invoke_end_region_gesture();

    let session = state.session.borrow();
    let track = &session.project.tracks[0];
    assert_eq!(track.regions[0].id, "region-b");
    assert_eq!(track.regions[0].start_sample, 24_000);
    assert_eq!(track.regions[1].id, "region-a");
    assert_eq!(track.regions[1].start_sample, 72_000);
    assert_eq!(session.selection.region_id.as_deref(), Some("region-a"));
    drop(session);

    app.invoke_undo();
    let session = state.session.borrow();
    assert_eq!(session.project.tracks[0].regions[0].id, "region-a");
    assert_eq!(session.project.tracks[0].regions[0].start_sample, 0);
    assert_eq!(session.project.tracks[0].regions[1].id, "region-b");
    assert_eq!(session.project.tracks[0].regions[1].start_sample, 24_000);
}

#[test]
fn arrangement_keyboard_dispatch_reaches_region_commands() {
    let (app, state) = test_app_and_state(ScriptedFileDialogs::default());
    let original_start = state.session.borrow().project.tracks[0].regions[0].start_sample;
    app.invoke_select_region(0, "region-vocal".into());
    app.invoke_focus_arrangement();
    app.window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed {
            text: slint::platform::Key::RightArrow.into(),
        });
    assert_eq!(
        state.session.borrow().project.tracks[0].regions[0].start_sample,
        original_start + 4_800
    );
    assert!(state.session.borrow().can_undo());

    // Split and delete are also routed through the same callbacks as the
    // toolbar/accessibility actions, rather than being UI-only labels.
    app.invoke_focus_arrangement();
    app.window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed { text: "s".into() });
    assert_eq!(state.session.borrow().project.tracks[0].regions.len(), 2);
    assert_eq!(
        state.session.borrow().selection.region_id.as_deref(),
        Some("region-vocal-b")
    );
    app.invoke_focus_arrangement();
    app.window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed {
            text: slint::platform::Key::Delete.into(),
        });
    assert_eq!(state.session.borrow().project.tracks[0].regions.len(), 1);
    assert_eq!(
        state.session.borrow().selection,
        loom_studio_core::TimelineSelection::track(0)
    );
}

#[test]
fn physical_arrangement_pointer_selects_and_focuses_region_commands() {
    let (app, state) = test_app_and_state(ScriptedFileDialogs::default());
    // Materialize the deterministic scene before dispatching a native-style
    // pointer event; this is the same software window used by screenshots.
    snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render arrangement");
    let position = slint::LogicalPosition { x: 640.0, y: 210.0 };
    app.window()
        .dispatch_event(slint::platform::WindowEvent::PointerMoved { position });
    app.window()
        .dispatch_event(slint::platform::WindowEvent::PointerPressed {
            position,
            button: slint::platform::PointerEventButton::Left,
        });
    app.window()
        .dispatch_event(slint::platform::WindowEvent::PointerReleased {
            position,
            button: slint::platform::PointerEventButton::Left,
        });
    assert_eq!(
        state.session.borrow().selection.region_id.as_deref(),
        Some("region-vocal")
    );
    assert_eq!(app.get_selected_region_id().as_str(), "region-vocal");

    let original_start = state.session.borrow().project.tracks[0].regions[0].start_sample;
    app.invoke_focus_arrangement();
    app.window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed {
            text: slint::platform::Key::RightArrow.into(),
        });
    assert_eq!(
        state.session.borrow().project.tracks[0].regions[0].start_sample,
        original_start + 4_800
    );

    app.invoke_focus_arrangement();
    app.window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed {
            text: slint::platform::Key::Delete.into(),
        });
    assert!(state.session.borrow().project.tracks[0].regions.is_empty());
    assert_eq!(
        state.session.borrow().selection,
        loom_studio_core::TimelineSelection::track(0)
    );
}

#[test]
fn secondary_pointer_opens_context_actions_and_routes_typed_move() {
    let (app, state) = test_app_and_state(ScriptedFileDialogs::default());
    snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render arrangement");
    let position = slint::LogicalPosition { x: 640.0, y: 210.0 };
    app.window()
        .dispatch_event(slint::platform::WindowEvent::PointerMoved { position });
    app.window()
        .dispatch_event(slint::platform::WindowEvent::PointerPressed {
            position,
            button: slint::platform::PointerEventButton::Right,
        });
    assert!(app.get_context_menu_open());
    assert_eq!(
        state.session.borrow().selection.region_id.as_deref(),
        Some("region-vocal")
    );

    let original_start = state.session.borrow().project.tracks[0].regions[0].start_sample;
    app.invoke_move_region(0, "region-vocal".into(), 0.1);
    assert_eq!(
        state.session.borrow().project.tracks[0].regions[0].start_sample,
        original_start + 4_800
    );
    assert!(state.session.borrow().can_undo());
    app.set_context_menu_open(false);
}

#[test]
fn context_menu_dismisses_when_selection_changes() {
    let (app, state) = test_app_and_state(ScriptedFileDialogs::default());
    snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render arrangement");
    let position = slint::LogicalPosition { x: 640.0, y: 210.0 };
    app.window()
        .dispatch_event(slint::platform::WindowEvent::PointerMoved { position });
    app.window()
        .dispatch_event(slint::platform::WindowEvent::PointerPressed {
            position,
            button: slint::platform::PointerEventButton::Right,
        });
    assert!(app.get_context_menu_open());
    assert_eq!(
        state.session.borrow().selection.region_id.as_deref(),
        Some("region-vocal")
    );

    // Selecting another region while the menu is still visible must close
    // it before any stale A-targeted action can be invoked.  Invoke the
    // old callback in the same turn to cover the controller-side guard,
    // before queued Slint property handlers have a chance to run.
    let original_start = state.session.borrow().project.tracks[0].regions[0].start_sample;
    app.invoke_select_region(1, "region-guitar".into());
    app.invoke_move_region(0, "region-vocal".into(), 0.1);
    assert_eq!(
        state.session.borrow().project.tracks[0].regions[0].start_sample,
        original_start
    );
    slint::platform::update_timers_and_animations();
    assert_eq!(
        state.session.borrow().selection.region_id.as_deref(),
        Some("region-guitar")
    );
    assert!(!app.get_context_menu_open());
}

#[test]
fn refresh_updates_bound_region_model_without_replacing_it() {
    let (app, state) = test_app_and_state(ScriptedFileDialogs::default());
    let regions_before = app.get_regions();
    let labels_before = app.get_track_labels();
    {
        let mut session = state.session.borrow_mut();
        session.project.tracks[0].regions[0].start_sample = 9_600;
    }
    refresh(&app, &state);
    assert_eq!(regions_before, app.get_regions());
    assert_eq!(labels_before, app.get_track_labels());
    let region = app.get_regions().row_data(0).expect("region row");
    assert_eq!(region.id.as_str(), "region-vocal");
    assert_eq!(region.start_seconds, 0.2);
}

#[test]
fn new_and_open_record_replacement_recovery_boundaries() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "loom-studio-recovery-{}-{unique}",
        std::process::id()
    ));
    let recovery_dir = root.join("recovery");
    let project_path = root.join("loaded.loomstudio");
    std::fs::create_dir_all(&root).expect("create recovery fixture");
    let mut loaded = StudioProject::new("loaded-project", "Loaded Replacement");
    loaded.tracks.clear();
    let loaded_bytes = save_studio_bundle(&loaded, &AudioAssetStore::default()).unwrap();
    std::fs::write(&project_path, loaded_bytes).expect("write loaded project");

    let recovery = loom_production::snapshot::SnapshotRecovery::open_at(&recovery_dir)
        .expect("open test recovery journal");
    STUDIO_RECOVERY.with(|slot| *slot.borrow_mut() = Some(recovery));
    let scripted = ScriptedFileDialogs::new(vec![Some(project_path.clone())], vec![]);
    let (app, state) = test_app_and_state(scripted);

    app.invoke_new_song();
    let mut journal = loom_production::snapshot::SnapshotRecovery::open_at(&recovery_dir)
        .expect("read New recovery snapshot");
    let new_payload = journal
        .take_restored_payload()
        .expect("New snapshot payload");
    let (new_project, _) = load_studio_bundle(&new_payload).expect("decode New snapshot");
    assert_eq!(new_project.name, "Untitled Session");

    app.invoke_open_song();
    let mut journal = loom_production::snapshot::SnapshotRecovery::open_at(&recovery_dir)
        .expect("read Open recovery snapshot");
    let open_payload = journal
        .take_restored_payload()
        .expect("Open snapshot payload");
    let (open_project, _) = load_studio_bundle(&open_payload).expect("decode Open snapshot");
    assert_eq!(open_project.name, "Loaded Replacement");
    assert_eq!(state.session.borrow().project.name, "Loaded Replacement");

    STUDIO_RECOVERY.with(|slot| *slot.borrow_mut() = None);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn mixer_and_record_arm_callbacks_update_projection_and_reject_invalid_edits() {
    let (app, state) = test_app_and_state(ScriptedFileDialogs::default());
    app.invoke_toggle_rec_arm(1);
    assert!(state.session.borrow().project.tracks[1].record_arm);
    assert!(app.get_track_arms().row_data(1).unwrap());
    assert_eq!(app.get_active_track_index(), 1);
    app.invoke_volume_changed(0, -9.0);
    app.invoke_pan_changed(0, 0.4);
    assert_eq!(state.session.borrow().project.tracks[0].volume_db, -9.0);
    assert_eq!(state.session.borrow().project.tracks[0].pan, 0.4);
    assert_eq!(app.get_active_track_index(), 0);
    let history_len_before = state.session.borrow().can_undo();
    app.invoke_pan_changed(0, 4.0);
    assert_eq!(state.session.borrow().project.tracks[0].pan, 0.4);
    assert!(history_len_before);
    app.invoke_toggle_mute(-1);
    assert!(app.get_status_left().contains("track index is invalid"));
    app.invoke_toggle_rec_arm(-1);
    assert!(app.get_status_left().contains("track index is invalid"));
}

#[test]
fn import_job_adds_a_selected_region_and_device_failure_is_visible() {
    let dir = std::env::temp_dir().join(format!("loom-studio-app-import-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let source = dir.join("input.wav");
    let bytes = AudioBuffer::sine(48_000, 1, 440.0, 0.05, 0.2)
        .unwrap()
        .to_wav_pcm16()
        .unwrap();
    std::fs::write(&source, bytes).unwrap();
    let (app, state) = test_app_and_state(ScriptedFileDialogs::default());
    let before = state.session.borrow().project.total_regions();
    app.invoke_import_audio(source.to_string_lossy().into_owned().into());
    wait_for_job(&app, &state, Duration::from_secs(5)).unwrap();
    assert_eq!(state.session.borrow().project.total_regions(), before + 1);
    assert!(state.session.borrow().selection.is_region());
    assert!(state.assets.borrow().get("input.wav").is_some());
    app.invoke_undo();
    assert_eq!(state.session.borrow().project.total_regions(), before);
    assert!(state.assets.borrow().get("input.wav").is_none());
    app.invoke_redo();
    assert_eq!(state.session.borrow().project.total_regions(), before + 1);
    assert!(state.assets.borrow().get("input.wav").is_some());
    app.invoke_play_pause();
    assert!(app.get_status_left().contains("No audio output device"));
    let _ = std::fs::remove_dir_all(&dir);
}
