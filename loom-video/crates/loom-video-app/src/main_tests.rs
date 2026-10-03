use super::*;

#[cfg(test)]
mod tests {
    use super::*;
    use loom_desktop::{DesktopError, SaveFileRequest, ScriptedFileDialogs};

    #[derive(Default)]
    struct RecordingDialogs {
        save_results: Mutex<VecDeque<Option<PathBuf>>>,
        save_requests: Mutex<Vec<SaveFileRequest>>,
    }

    impl RecordingDialogs {
        fn with_save_results(results: impl IntoIterator<Item = Option<PathBuf>>) -> Self {
            Self {
                save_results: Mutex::new(results.into_iter().collect()),
                save_requests: Mutex::new(Vec::new()),
            }
        }
    }

    impl FileDialogService for RecordingDialogs {
        fn open_file(&self, _request: &OpenFileRequest) -> Result<Option<PathBuf>, DesktopError> {
            Ok(None)
        }

        fn save_file(&self, request: &SaveFileRequest) -> Result<Option<PathBuf>, DesktopError> {
            lock(&self.save_requests).push(request.clone());
            lock(&self.save_results)
                .pop_front()
                .ok_or(DesktopError::ScriptExhausted("save_file"))
        }
    }

    fn test_app_and_state(scripted: ScriptedFileDialogs) -> (VideoApp, Arc<AppState>) {
        test_app_and_state_with_dialogs(Arc::new(scripted))
    }

    // These tests exercise real FFmpeg decoding. Keep the rest of the app
    // suite useful on machines that do not have the optional media tools.
    fn test_media_tools() -> Option<MediaTools> {
        match discover_media_tools() {
            Ok(tools) => Some(tools),
            Err(error) => {
                eprintln!("skipping real-media fixture test: {error}");
                None
            }
        }
    }

    fn test_app_and_state_with_dialogs(
        dialogs: Arc<dyn FileDialogService>,
    ) -> (VideoApp, Arc<AppState>) {
        set_platform();
        let app = VideoApp::new().expect("create VideoApp");
        let state = Arc::new(AppState {
            session: Mutex::new(VideoSession::new(sample_project())),
            save_path: Mutex::new(None),
            dialogs,
            selected_clip: Mutex::new(0),
            preview: Mutex::new(Some(procedural_preview())),
            preview_synthetic: AtomicBool::new(true),
            tools: Mutex::new(None),
            exporting: AtomicBool::new(false),
            export_cancel: ExportCancellation::default(),
            preview_generation: PreviewGeneration::default(),
            preview_cancel: Mutex::new(None),
            preview_in_flight: AtomicBool::new(false),
            preview_cache: Mutex::new(PreviewCache::default()),
            preview_cache_hits: AtomicU64::new(0),
            waveform_cache_hits: AtomicU64::new(0),
            gesture: Mutex::new(None),
            playback_clock: Mutex::new(None),
        });
        wire_application(&app, state.clone());
        refresh(&app, &state);
        (app, state)
    }

    fn test_app_and_state_with_project(
        project: VideoProject,
        tools: MediaTools,
    ) -> (VideoApp, Arc<AppState>) {
        set_platform();
        let app = VideoApp::new().expect("create VideoApp");
        let state = Arc::new(AppState {
            session: Mutex::new(VideoSession::new(project)),
            save_path: Mutex::new(None),
            dialogs: Arc::new(ScriptedFileDialogs::default()),
            selected_clip: Mutex::new(0),
            preview: Mutex::new(Some(procedural_preview())),
            preview_synthetic: AtomicBool::new(true),
            tools: Mutex::new(Some(tools)),
            exporting: AtomicBool::new(false),
            export_cancel: ExportCancellation::default(),
            preview_generation: PreviewGeneration::default(),
            preview_cancel: Mutex::new(None),
            preview_in_flight: AtomicBool::new(false),
            preview_cache: Mutex::new(PreviewCache::default()),
            preview_cache_hits: AtomicU64::new(0),
            waveform_cache_hits: AtomicU64::new(0),
            gesture: Mutex::new(None),
            playback_clock: Mutex::new(None),
        });
        wire_application(&app, state.clone());
        refresh(&app, &state);
        (app, state)
    }

    #[test]
    fn compact_media_recovery_actions_are_not_clipped() {
        let (app, _) = test_app_and_state(ScriptedFileDialogs::default());
        configure_responsive_layout(&app, 1024);
        let image = snapshot_component(&app, 1024.0, 720.0, 1.0).unwrap();

        for x in [24, 360] {
            let mut longest = 0;
            let mut current = 0;
            for y in 0..image.height() {
                let pixel = image.get_pixel(x, y).0;
                let button_fill = pixel[0] > 180 && (50..=120).contains(&pixel[1]) && pixel[2] < 70;
                if button_fill {
                    current += 1;
                    longest = longest.max(current);
                } else {
                    current = 0;
                }
            }

            assert!(
                longest >= 28,
                "compact recovery button at x={x} is clipped to {longest}px"
            );
        }
    }

    #[test]
    fn new_project_creates_untitled_clean_state() {
        let scripted = ScriptedFileDialogs::default();
        let (app, state) = test_app_and_state(scripted);
        *lock(&state.save_path) = Some(PathBuf::from("/tmp/existing.loomvideo"));

        app.invoke_new_project();
        assert_eq!(*lock(&state.save_path), None);
        assert_eq!(lock(&state.session).project.name, "Untitled Project");
        assert_eq!(app.get_project_name().as_str(), "Untitled Project");
    }

    #[test]
    fn offline_sample_clips_are_labelled_without_source_paths() {
        let mut offline = Clip::new("clip", "Opening Scene", 2.0);
        assert_eq!(
            clip_display_name(&offline),
            "Opening Scene · offline sample"
        );
        offline.source_path = "/tmp/source.mov".into();
        assert_eq!(clip_display_name(&offline), "Opening Scene");
    }

    #[test]
    fn open_project_with_dialog_loads_path_and_updates_state() {
        let dir = std::env::temp_dir().join(format!("loom-video-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("open_test.loomvideo");

        let mut proj = VideoProject::new("loaded-proj", "Loaded Video Project");
        proj.tracks[0].clips.clear();
        let bytes = save_video_project(&proj).unwrap();
        std::fs::write(&file, bytes).unwrap();

        let scripted = ScriptedFileDialogs::new(vec![Some(file.clone())], vec![]);

        let (app, state) = test_app_and_state(scripted);
        app.invoke_open_project();

        assert_eq!(*lock(&state.save_path), Some(file));
        assert_eq!(lock(&state.session).project.name, "Loaded Video Project");
        assert_eq!(app.get_project_name().as_str(), "Loaded Video Project");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cancelled_open_leaves_current_session_untouched() {
        let scripted = ScriptedFileDialogs::new(vec![None], vec![]); // User clicked Cancel in native open dialog

        let (app, state) = test_app_and_state(scripted);
        let original_name = lock(&state.session).project.name.clone();

        app.invoke_open_project();
        assert_eq!(lock(&state.session).project.name, original_name);
        assert_eq!(app.get_status_left().as_str(), "Open cancelled");
    }

    #[test]
    fn save_untitled_prompts_dialog_and_writes_file() {
        let dir = std::env::temp_dir().join(format!("loom-video-save-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("saved_project.loomvideo");

        let scripted = ScriptedFileDialogs::new(vec![], vec![Some(file.clone())]);

        let (app, state) = test_app_and_state(scripted);
        assert_eq!(*lock(&state.save_path), None);

        app.invoke_save_project();

        assert_eq!(*lock(&state.save_path), Some(file.clone()));
        assert!(file.is_file());
        let read_bytes = std::fs::read(&file).unwrap();
        let loaded = load_video_project(&read_bytes).unwrap();
        assert_eq!(loaded.name, "Documentary Assembly");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_as_prompts_dialog_and_updates_path() {
        let dir = std::env::temp_dir().join(format!("loom-video-saveas-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file_v1 = dir.join("v1.loomvideo");
        let file_v2 = dir.join("v2.loomvideo");

        let scripted = ScriptedFileDialogs::new(vec![], vec![Some(file_v2.clone())]);

        let (app, state) = test_app_and_state(scripted);
        *lock(&state.save_path) = Some(file_v1);

        app.invoke_save_as_project();

        assert_eq!(*lock(&state.save_path), Some(file_v2.clone()));
        assert!(file_v2.is_file());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn timeline_callbacks_select_trim_move_split_and_undo() {
        let (app, state) = test_app_and_state(ScriptedFileDialogs::default());

        app.invoke_select_clip(1);
        assert_eq!(*lock(&state.selected_clip), 1);
        assert_eq!(app.get_active_clip_index(), 1);
        assert_eq!(app.get_clip_in_points().row_count(), 2);

        let original = lock(&state.session).project.tracks[0].clips[1].clone();
        app.invoke_trim_selected(0.5, 0.0);
        let trimmed = lock(&state.session).project.tracks[0].clips[1].clone();
        assert!(trimmed.start_time > original.start_time);
        assert!(trimmed.duration < original.duration);

        app.invoke_move_clip(1, 0.75);
        let moved = lock(&state.session).project.tracks[0].clips[1].start_time;
        assert!((moved - (trimmed.start_time + 0.75)).abs() < 1e-6);
        app.invoke_undo();
        let restored = lock(&state.session).project.tracks[0].clips[1].start_time;
        assert!((restored - trimmed.start_time).abs() < 1e-6);

        app.set_playhead_seconds(8.0);
        app.invoke_split_clip();
        assert_eq!(lock(&state.session).project.tracks[0].clips.len(), 3);
        assert!(app.get_can_undo());
    }

    #[test]
    fn timeline_gesture_commits_once_and_cancel_restores_baseline() {
        let (app, state) = test_app_and_state(ScriptedFileDialogs::default());
        app.set_snap_enabled(false);
        let baseline = lock(&state.session).project.clone();

        app.invoke_begin_clip_gesture(1, "Move".into());
        app.invoke_move_clip(1, 0.25);
        app.invoke_move_clip(1, 0.25);
        assert!(!lock(&state.session).can_undo());
        assert_ne!(lock(&state.session).project, baseline);

        app.invoke_end_clip_gesture();
        assert!(lock(&state.session).can_undo());
        app.invoke_undo();
        assert_eq!(lock(&state.session).project, baseline);
        assert!(!lock(&state.session).can_undo());

        app.invoke_begin_clip_gesture(1, "Move".into());
        app.invoke_move_clip(1, 0.5);
        app.invoke_cancel_clip_gesture();
        assert_eq!(lock(&state.session).project, baseline);
        assert!(!lock(&state.session).can_undo());
    }

    #[test]
    fn moving_clip_across_neighbor_keeps_clip_selected_after_sort() {
        let (app, state) = test_app_and_state(ScriptedFileDialogs::default());

        app.invoke_select_clip(0);
        let moved_id = lock(&state.session).project.tracks[0].clips[0].id.clone();
        app.invoke_move_clip(0, 10.0);

        let session = lock(&state.session);
        let clips = &session.project.tracks[0].clips;
        let moved_index = clips
            .iter()
            .position(|clip| clip.id == moved_id)
            .expect("moved clip remains in timeline");
        assert_eq!(moved_index, 1);
        assert_eq!(*lock(&state.selected_clip), moved_index);
        assert_eq!(app.get_active_clip_index(), moved_index as i32);
    }

    #[test]
    fn selecting_offline_clip_restores_synthetic_preview() {
        let (app, state) = test_app_and_state(ScriptedFileDialogs::default());
        state.preview_synthetic.store(false, Ordering::Release);
        app.set_preview_synthetic(false);

        app.invoke_select_clip(0);

        assert!(state.preview_synthetic.load(Ordering::Acquire));
        assert!(app.get_preview_synthetic());
        assert!(app.get_has_preview());
    }

    #[test]
    fn zero_delta_trim_does_not_create_history() {
        let (app, state) = test_app_and_state(ScriptedFileDialogs::default());
        assert!(!lock(&state.session).can_undo());
        app.invoke_trim_selected(0.0, 0.0);
        assert!(!lock(&state.session).can_undo());
    }

    #[test]
    fn preview_cache_reports_thumbnail_progress_and_failure() {
        let dir =
            std::env::temp_dir().join(format!("loom-video-preview-cache-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("source.mp4");
        std::fs::write(&source, b"fixture").unwrap();
        let mut clip = Clip::new("clip", "Clip", 2.0);
        clip.source_path = source.to_string_lossy().into_owned();
        let identity = source_identity(&source);
        let mut cache = PreviewCache::default();
        assert_eq!(
            cache.status_for(&clip),
            "Thumbnail pending · waveform pending"
        );
        cache.mark_pending_at(&clip.id, &identity, 0.0);
        assert_eq!(
            cache.status_for(&clip),
            "Thumbnail pending · waveform pending"
        );
        cache.mark_thumbnail_ready_at(
            &clip.id,
            &identity,
            0.0,
            VideoFrame {
                width: 2,
                height: 2,
                pixels: vec![255; 16],
            },
        );
        assert_eq!(
            cache.status_for(&clip),
            "Thumbnail ready · waveform pending"
        );
        cache.mark_thumbnail_failed(&clip.id, &identity);
        assert_eq!(
            cache.status_for(&clip),
            "Thumbnail failed · waveform pending · retry on demand"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_export_path_uses_mp4_save_dialog() {
        let dir =
            std::env::temp_dir().join(format!("loom-video-export-dialog-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let output = dir.join("chosen.mp4");
        let dialogs = Arc::new(RecordingDialogs::with_save_results([Some(output)]));
        let (app, _state) = test_app_and_state_with_dialogs(dialogs.clone());

        app.invoke_export_timeline("".into());

        let requests = lock(&dialogs.save_requests);
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].title, "Export Video Timeline");
        assert_eq!(
            requests[0].suggested_name.as_deref(),
            Some("Untitled-export.mp4")
        );
        assert_eq!(requests[0].filters[0].extensions, ["mp4"]);
        assert_eq!(
            app.get_status_left().as_str(),
            "FFmpeg/FFprobe are unavailable"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_reopen_preserves_trimmed_clip_state() {
        let dir = std::env::temp_dir().join(format!("loom-video-reopen-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("edited.loomvideo");
        let scripted = ScriptedFileDialogs::new([Some(file.clone())], []);
        let (app, state) = test_app_and_state(scripted);
        *lock(&state.save_path) = Some(file.clone());

        app.invoke_select_clip(0);
        app.invoke_trim_selected(0.5, 0.0);
        let expected_in = lock(&state.session).project.tracks[0].clips[0].in_point;
        app.invoke_save_project();
        assert!(file.is_file());
        app.invoke_open_project();
        let reopened_in = lock(&state.session).project.tracks[0].clips[0].in_point;
        assert!((reopened_in - expected_in).abs() < 1e-6);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cancel_export_callback_signals_active_worker() {
        let (app, state) = test_app_and_state(ScriptedFileDialogs::default());
        state.exporting.store(true, Ordering::Release);
        app.invoke_cancel_export();
        assert!(state.export_cancel.is_cancelled());
        state.exporting.store(false, Ordering::Release);
    }

    #[test]
    fn compact_layout_boundary_keeps_reference_width_stable() {
        // The breakpoint is owned by the shared Slint policy; the pure helper
        // keeps the boundary test independent from AppKit's main-thread window
        // requirement on macOS.
        assert!(compact_layout_for_breakpoint(1024, 1180.0));
        assert!(compact_layout_for_breakpoint(1179, 1180.0));
        assert!(!compact_layout_for_breakpoint(1180, 1180.0));
        assert!(!compact_layout_for_breakpoint(1440, 1180.0));
    }

    #[test]
    fn responsive_policy_transition_probes_are_exact() {
        set_platform();
        let app = VideoApp::new().expect("create VideoApp");
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
            assert_eq!(compact_layout_for_width(&app, width), icon_only);
        }
    }

    #[test]
    fn compact_layout_disables_inspector_without_mutating_preference() {
        set_platform();
        let app = VideoApp::new().expect("create VideoApp");

        assert!(app.get_show_inspector());
        assert!(app.get_inspector_available());
        configure_responsive_layout(&app, 1179);
        assert!(!app.get_inspector_available());
        assert!(app.get_show_inspector());
        assert!(app.get_icon_only_toolbar());

        configure_responsive_layout(&app, 1180);
        assert!(app.get_inspector_available());
        assert!(app.get_show_inspector());
        assert!(!app.get_icon_only_toolbar());
    }

    #[test]
    fn compact_inspector_menu_action_does_not_mutate_preference() {
        set_platform();
        let app = VideoApp::new().expect("create VideoApp");
        configure_responsive_layout(&app, 1179);
        let menu = install_video_menu(
            &app,
            build_standard_menu_bar(
                "Loom Video",
                vec![],
                vec![],
                vec![MenuItem::check("view.inspector", "Inspector", true)],
                vec![],
            ),
        );

        sync_menu_state(&menu, &app);
        assert!(matches!(
            menu.installed_menu_bar()
                .and_then(|bar| bar.find_item("view.inspector").cloned()),
            Some(MenuItem::Check {
                enabled: false,
                checked: false,
                ..
            })
        ));
        assert!(menu.dispatch_action("view.inspector").is_err());
        assert!(app.get_show_inspector());

        configure_responsive_layout(&app, 1180);
        sync_menu_state(&menu, &app);
        assert!(matches!(
            menu.installed_menu_bar()
                .and_then(|bar| bar.find_item("view.inspector").cloned()),
            Some(MenuItem::Check {
                enabled: true,
                checked: true,
                ..
            })
        ));
        menu.dispatch_action("view.inspector")
            .expect("dispatch available inspector menu action");
        assert!(!app.get_show_inspector());
        assert!(matches!(
            menu.installed_menu_bar()
                .and_then(|bar| bar.find_item("view.inspector").cloned()),
            Some(MenuItem::Check {
                enabled: true,
                checked: false,
                ..
            })
        ));
    }

    #[test]
    fn inspector_palette_action_respects_responsive_availability() {
        set_platform();
        let app = VideoApp::new().expect("create VideoApp");
        wire_palette(&app);

        configure_responsive_layout(&app, 1180);
        assert!(app.get_inspector_available());
        app.set_palette_query("inspector".into());
        rebuild_palette(&app, "inspector");
        assert_eq!(app.get_palette_commands().row_count(), 1);
        assert_eq!(
            app.get_palette_commands()
                .row_data(0)
                .expect("visible inspector command")
                .id,
            "video.inspector"
        );
        let before = app.get_show_inspector();
        app.invoke_palette_invoked(0);
        assert_ne!(app.get_show_inspector(), before);

        configure_responsive_layout(&app, 1179);
        assert!(!app.get_inspector_available());
        app.set_show_inspector(true);
        app.set_palette_query("inspector".into());
        rebuild_palette(&app, "inspector");
        assert_eq!(app.get_palette_commands().row_count(), 0);
        app.invoke_palette_invoked(0);
        assert!(app.get_show_inspector());
    }

    #[test]
    fn inspector_toggle_paths_keep_native_menu_check_in_sync() {
        set_platform();
        let app = VideoApp::new().expect("create VideoApp");
        configure_responsive_layout(&app, 1180);
        let menu = install_video_menu(
            &app,
            build_standard_menu_bar(
                "Loom Video",
                vec![],
                vec![],
                vec![MenuItem::check("view.inspector", "Inspector", true)],
                vec![],
            ),
        );
        sync_menu_state(&menu, &app);
        wire_palette(&app);

        app.set_palette_query("inspector".into());
        rebuild_palette(&app, "inspector");
        app.invoke_palette_invoked(0);
        slint::platform::update_timers_and_animations();
        assert!(!app.get_show_inspector());
        assert!(matches!(
            menu.installed_menu_bar()
                .and_then(|bar| bar.find_item("view.inspector").cloned()),
            Some(MenuItem::Check {
                enabled: true,
                checked: false,
                ..
            })
        ));

        app.set_show_inspector(true);
        slint::platform::update_timers_and_animations();
        assert!(matches!(
            menu.installed_menu_bar()
                .and_then(|bar| bar.find_item("view.inspector").cloned()),
            Some(MenuItem::Check {
                enabled: true,
                checked: true,
                ..
            })
        ));
    }

    #[test]
    fn responsive_resize_rebuilds_palette_availability() {
        set_platform();
        let app = VideoApp::new().expect("create VideoApp");
        configure_responsive_layout(&app, 1180);
        let menu = install_video_menu(
            &app,
            build_standard_menu_bar(
                "Loom Video",
                vec![],
                vec![],
                vec![MenuItem::check("view.inspector", "Inspector", true)],
                vec![],
            ),
        );
        wire_responsive_layout(&app, menu);
        wire_palette(&app);

        app.set_palette_query("inspector".into());
        rebuild_palette(&app, "inspector");
        assert_eq!(app.get_palette_commands().row_count(), 1);

        app.invoke_window_resized(1179.0);
        assert_eq!(app.get_palette_commands().row_count(), 0);

        app.invoke_window_resized(1180.0);
        assert_eq!(app.get_palette_commands().row_count(), 1);
    }

    #[test]
    fn monotonic_fallback_clock_exposes_seek_position() {
        let mut clock = PlaybackClock::start(2.0);
        assert_eq!(clock.source(), ClockSource::MonotonicFallback);
        assert!(clock.position() >= 2.0);
        clock.seek(4.5);
        assert!((clock.position() - 4.5).abs() < 0.05);
    }

    #[test]
    fn cross_clip_seek_keeps_source_change_status_through_refresh() {
        let (app, state) = test_app_and_state(ScriptedFileDialogs::default());
        *lock(&state.playback_clock) = Some(PlaybackClock::start_for_clip_with_status(
            0.0,
            Some("clip-1".into()),
            "Audio seek unavailable · monotonic fallback clock",
        ));

        app.invoke_seek(6.0);

        {
            let clock = lock(&state.playback_clock);
            let clock = clock.as_ref().expect("seek should retain a playback clock");
            assert_eq!(clock.source(), ClockSource::MonotonicFallback);
            assert_eq!(clock.active_clip_id(), None);
            assert_eq!(
                clock.audio_status(),
                "Audio source changed · monotonic fallback clock"
            );
        }
        assert_eq!(
            app.get_audio_output_status().as_str(),
            "Audio source changed · monotonic fallback clock"
        );

        refresh(&app, &state);
        assert_eq!(
            app.get_audio_output_status().as_str(),
            "Audio source changed · monotonic fallback clock"
        );
        app.invoke_stop_playback();
    }

    #[test]
    fn preview_generation_rejects_stale_results() {
        let generation = PreviewGeneration::default();
        let first = generation.next();
        let second = generation.next();
        assert!(generation.is_current(second));
        assert!(!generation.is_current(first));
    }

    #[test]
    fn audio_master_clock_tick_consumes_decoded_samples_and_stalls_without_them() {
        let Some(tools) = test_media_tools() else {
            return;
        };
        let dir =
            std::env::temp_dir().join(format!("loom-video-audio-clock-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = create_workflow_media(&tools, &dir).expect("create audio/video fixture");
        let probe = probe_media(&tools, &source).expect("probe audio/video fixture");
        assert!(probe.has_audio);
        let sample_rate = probe
            .audio_sample_rate
            .expect("audio fixture must expose a sample rate");

        let stalled_consumer = spawn_decoded_audio_consumer(&tools, &source, 999.0, sample_rate)
            .expect("spawn bounded local audio consumer");
        let mut stalled_clock = PlaybackClock::start_audio(0.0, stalled_consumer);
        std::thread::sleep(Duration::from_millis(100));
        let stalled_position = stalled_clock.position();
        assert_eq!(stalled_clock.tick(), stalled_position);

        let consumer = spawn_decoded_audio_consumer(&tools, &source, 0.0, sample_rate)
            .expect("spawn bounded local audio consumer");
        let mut clock = PlaybackClock::start_audio(0.0, consumer);
        assert_eq!(clock.source(), ClockSource::AudioMaster);
        let first_frame = decode_preview_frame(&tools, &source, clock.position(), 320, 180)
            .expect("decode first preview frame");
        let initial_position = clock.position();
        let deadline = Instant::now() + Duration::from_secs(4);
        let mut advanced_position = initial_position;
        while Instant::now() < deadline {
            advanced_position = clock.tick();
            if advanced_position > initial_position {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            advanced_position > initial_position,
            "decoded sample consumer never advanced the audio clock"
        );
        let second_frame = decode_preview_frame(&tools, &source, clock.position(), 320, 180)
            .expect("decode advanced preview frame");
        assert_ne!(first_frame.pixels, second_frame.pixels);
        clock.seek_with_source(1.0, 0.5);
        assert_eq!(clock.source(), ClockSource::AudioMaster);
        assert!((clock.position() - 1.0).abs() < 1e-6);
        assert!(
            (clock
                .audio
                .as_ref()
                .expect("audio clock should retain its consumer after seek")
                .consumer
                .start_time()
                - 0.5)
                .abs()
                < 1e-6
        );
        let seek_position = clock.position();
        let seek_deadline = Instant::now() + Duration::from_secs(4);
        let mut seek_advanced = seek_position;
        while Instant::now() < seek_deadline {
            seek_advanced = clock.tick();
            if seek_advanced > seek_position {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            seek_advanced > seek_position,
            "audio clock did not resume after source-aware seek"
        );
        drop(clock);
        drop(stalled_clock);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn terminal_audio_eof_switches_playing_controller_to_monotonic_fallback() {
        let Some(tools) = test_media_tools() else {
            return;
        };
        let dir = std::env::temp_dir().join(format!(
            "loom-video-audio-eof-controller-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let source = create_workflow_media(&tools, &dir).expect("create audio/video fixture");
        let sample_rate = probe_media(&tools, &source)
            .expect("probe audio/video fixture")
            .audio_sample_rate
            .expect("audio fixture must expose a sample rate");

        let mut project = sample_project();
        project.tracks[0].clips[0].source_path = source.to_string_lossy().into_owned();
        project.tracks[0].clips[0].duration = 6.0;
        project.tracks[0].clips[0].out_point = 6.0;
        let (app, state) = test_app_and_state_with_project(project, tools);
        let consumer = spawn_decoded_audio_consumer(
            state.media_tools().as_ref().unwrap(),
            &source,
            999.0,
            sample_rate,
        )
        .expect("spawn bounded local audio consumer");
        *lock(&state.playback_clock) = Some(PlaybackClock::start_audio_for_clip(
            0.0,
            consumer,
            Some("clip-1".into()),
            1.0,
        ));
        app.set_is_playing(true);

        let deadline = Instant::now() + Duration::from_secs(4);
        while Instant::now() < deadline
            && lock(&state.playback_clock)
                .as_ref()
                .is_some_and(|clock| clock.source() == ClockSource::AudioMaster)
        {
            assert!(advance_playback_tick(&app, &state));
            thread::sleep(Duration::from_millis(10));
        }

        assert!(app.get_is_playing());
        assert_eq!(
            lock(&state.playback_clock)
                .as_ref()
                .map(PlaybackClock::source),
            Some(ClockSource::MonotonicFallback)
        );
        assert_eq!(
            app.get_playback_clock_source().as_str(),
            "Monotonic fallback"
        );
        assert!(app
            .get_audio_output_status()
            .to_string()
            .contains("Audio stream ended"));
        app.invoke_stop_playback();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn same_clip_fallback_seek_rebuilds_audio_clock_and_preserves_unavailable_fallback() {
        let Some(tools) = test_media_tools() else {
            return;
        };
        let dir = std::env::temp_dir().join(format!(
            "loom-video-audio-fallback-seek-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let source = create_workflow_media(&tools, &dir).expect("create audio/video fixture");
        let sample_rate = probe_media(&tools, &source)
            .expect("probe audio/video fixture")
            .audio_sample_rate
            .expect("audio fixture must expose a sample rate");

        let mut project = sample_project();
        project.tracks[0].clips[0].source_path = source.to_string_lossy().into_owned();
        let (app, state) = test_app_and_state_with_project(project.clone(), tools.clone());
        let consumer = spawn_decoded_audio_consumer(
            state.media_tools().as_ref().unwrap(),
            &source,
            999.0,
            sample_rate,
        )
        .expect("spawn bounded local audio consumer");
        *lock(&state.playback_clock) = Some(PlaybackClock::start_audio_for_clip(
            0.0,
            consumer,
            Some("clip-1".into()),
            1.0,
        ));
        app.set_is_playing(true);

        let deadline = Instant::now() + Duration::from_secs(4);
        while Instant::now() < deadline
            && lock(&state.playback_clock)
                .as_ref()
                .is_some_and(|clock| clock.source() == ClockSource::AudioMaster)
        {
            assert!(advance_playback_tick(&app, &state));
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            lock(&state.playback_clock)
                .as_ref()
                .map(PlaybackClock::source),
            Some(ClockSource::MonotonicFallback)
        );
        assert_eq!(
            lock(&state.playback_clock)
                .as_ref()
                .and_then(|clock| clock.active_clip_id()),
            Some("clip-1")
        );

        app.invoke_seek(0.75);

        {
            let clock = lock(&state.playback_clock);
            let clock = clock
                .as_ref()
                .expect("same-clip seek should retain a playback clock");
            assert_eq!(clock.source(), ClockSource::AudioMaster);
            assert_eq!(clock.audio_clip_id(), Some("clip-1"));
            assert_eq!(clock.active_clip_id(), Some("clip-1"));
            assert!((clock.position() - 0.75).abs() < 1e-6);
            assert!(
                (clock
                    .audio
                    .as_ref()
                    .expect("same-clip fallback seek should reattach audio")
                    .consumer
                    .start_time()
                    - 0.75)
                    .abs()
                    < 1e-6
            );
        }
        assert_eq!(app.get_playback_clock_source().as_str(), "Audio master");

        let failed_tools = MediaTools {
            ffmpeg: dir.join("missing-ffmpeg"),
            ..tools
        };
        let (fallback_app, fallback_state) = test_app_and_state_with_project(project, failed_tools);
        *lock(&fallback_state.playback_clock) = Some(PlaybackClock::start_for_clip_with_status(
            0.0,
            Some("clip-1".into()),
            "Audio stream ended · monotonic fallback clock",
        ));

        fallback_app.invoke_seek(0.75);

        {
            let clock = lock(&fallback_state.playback_clock);
            let clock = clock
                .as_ref()
                .expect("failed consumer seek should retain a playback clock");
            assert_eq!(clock.source(), ClockSource::MonotonicFallback);
            assert_eq!(clock.active_clip_id(), Some("clip-1"));
            assert_eq!(
                clock.audio_status(),
                "Audio stream detected · decoded consumer unavailable; monotonic fallback clock"
            );
        }
        assert_eq!(
            fallback_app.get_audio_output_status().as_str(),
            "Audio stream detected · decoded consumer unavailable; monotonic fallback clock"
        );

        fallback_app.invoke_stop_playback();
        app.invoke_stop_playback();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn audio_master_boundary_rebuilds_consumer_for_next_clip() {
        let Some(tools) = test_media_tools() else {
            return;
        };
        let dir = std::env::temp_dir().join(format!(
            "loom-video-audio-boundary-controller-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let source = create_workflow_media(&tools, &dir).expect("create audio/video fixture");

        let mut project = sample_project();
        let source_path = source.to_string_lossy().into_owned();
        let first = &mut project.tracks[0].clips[0];
        first.source_path = source_path.clone();
        first.duration = 0.1;
        first.in_point = 0.0;
        first.out_point = 0.1;
        let second = &mut project.tracks[0].clips[1];
        second.source_path = source_path;
        second.start_time = 0.1;
        second.duration = 0.2;
        second.in_point = 1.25;
        second.out_point = 1.65;
        second.playback_rate = 2.0;
        let (app, state) = test_app_and_state_with_project(project, tools);
        let setup = build_playback_clock(&state, 0.0);
        assert_eq!(setup.clock.source(), ClockSource::AudioMaster);
        *lock(&state.playback_clock) = Some(setup.clock);
        app.set_is_playing(true);

        let deadline = Instant::now() + Duration::from_secs(8);
        while Instant::now() < deadline
            && lock(&state.playback_clock)
                .as_ref()
                .and_then(PlaybackClock::audio_clip_id)
                != Some("clip-2")
        {
            assert!(advance_playback_tick(&app, &state));
            thread::sleep(Duration::from_millis(10));
        }

        {
            let clock = lock(&state.playback_clock);
            let clock = clock
                .as_ref()
                .expect("boundary should retain playback clock");
            assert_eq!(clock.source(), ClockSource::AudioMaster);
            assert_eq!(clock.audio_clip_id(), Some("clip-2"));
            let position = clock.position();
            let expected_source_time = 1.25 + (position - 0.1) * 2.0;
            let actual_source_time = clock
                .audio
                .as_ref()
                .expect("next clip should have decoded audio")
                .consumer
                .start_time();
            assert!((actual_source_time - expected_source_time).abs() < 0.08);
        }
        app.invoke_stop_playback();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn waveform_cache_survives_thumbnail_seek_and_invalidates_source() {
        let dir =
            std::env::temp_dir().join(format!("loom-video-waveform-cache-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let source_a = dir.join("source-a.mp4");
        let source_b = dir.join("source-b.mp4");
        std::fs::write(&source_a, b"a").unwrap();
        std::fs::write(&source_b, b"b").unwrap();
        let identity_a = source_identity(&source_a);
        let identity_b = source_identity(&source_b);
        let mut cache = PreviewCache::default();
        let peaks = vec![(-0.5, 0.5), (-0.25, 0.25)];

        cache.mark_pending_at("clip", &identity_a, 0.0);
        cache.mark_waveform_ready("clip", &identity_a, peaks.clone());
        assert_eq!(
            cache.cached_waveform("clip", &identity_a),
            Some(peaks.clone())
        );

        cache.mark_pending_at("clip", &identity_a, 0.75);
        assert_eq!(cache.cached_waveform("clip", &identity_a), Some(peaks));

        cache.mark_pending_at("clip", &identity_b, 0.0);
        assert!(cache.cached_waveform("clip", &identity_b).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn preview_request_generates_waveform_and_serves_cached_frame() {
        set_platform();
        let Some(tools) = test_media_tools() else {
            return;
        };
        let dir = std::env::temp_dir().join(format!(
            "loom-video-preview-cache-hit-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let source = create_workflow_media(&tools, &dir).expect("create preview fixture");
        let app = VideoApp::new().expect("create VideoApp");
        let mut project = sample_project();
        project.tracks[0].clips[0].source_path = source.to_string_lossy().into_owned();
        project.tracks[0].clips[0].duration = 4.0;
        project.tracks[0].clips[0].out_point = 4.0;
        let state = Arc::new(AppState {
            session: Mutex::new(VideoSession::new(project)),
            save_path: Mutex::new(None),
            dialogs: Arc::new(ScriptedFileDialogs::default()),
            selected_clip: Mutex::new(0),
            preview: Mutex::new(Some(procedural_preview())),
            preview_synthetic: AtomicBool::new(true),
            tools: Mutex::new(Some(tools)),
            exporting: AtomicBool::new(false),
            export_cancel: ExportCancellation::default(),
            preview_generation: PreviewGeneration::default(),
            preview_cancel: Mutex::new(None),
            preview_in_flight: AtomicBool::new(false),
            preview_cache: Mutex::new(PreviewCache::default()),
            preview_cache_hits: AtomicU64::new(0),
            waveform_cache_hits: AtomicU64::new(0),
            gesture: Mutex::new(None),
            playback_clock: Mutex::new(None),
        });
        let clip = lock(&state.session).project.tracks[0].clips[0].clone();

        request_preview(state.clone(), app.as_weak(), 0.0);
        let deadline = Instant::now() + Duration::from_secs(12);
        loop {
            let ready = lock(&state.preview_cache)
                .entries
                .iter()
                .find(|entry| entry.clip_id == clip.id)
                .map(|entry| {
                    entry.thumbnail == CacheState::Ready
                        && entry.waveform == CacheState::Ready
                        && entry
                            .waveform_peaks
                            .as_ref()
                            .is_some_and(|peaks| !peaks.is_empty())
                })
                .unwrap_or(false);
            if ready || Instant::now() >= deadline {
                assert!(ready, "preview cache did not become ready");
                break;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        let cached = lock(&state.preview_cache)
            .cached_frame(&clip.id, &source_identity(&source), 0.0)
            .expect("generated preview frame should be cached");
        assert!(!cached.pixels.is_empty());

        let identity = source_identity(&source);
        let first_peaks = lock(&state.preview_cache)
            .waveform_for(&clip.id, &identity)
            .map(|peaks| peaks.to_vec())
            .expect("generated waveform peaks should be available");
        assert!(!first_peaks.is_empty());

        request_preview_internal(state.clone(), app.as_weak(), 0.75, true);
        let waveform_hit_deadline = Instant::now() + Duration::from_secs(12);
        loop {
            let ready = lock(&state.preview_cache)
                .entries
                .iter()
                .find(|entry| entry.clip_id == clip.id && entry.source_identity == identity)
                .map(|entry| {
                    entry.thumbnail == CacheState::Ready
                        && (entry.frame_time - 0.75).abs() <= 1e-3
                        && entry.waveform == CacheState::Ready
                })
                .unwrap_or(false);
            if ready || Instant::now() >= waveform_hit_deadline {
                assert!(ready, "seeked preview did not become ready");
                break;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        assert_eq!(
            lock(&state.preview_cache)
                .waveform_for(&clip.id, &identity)
                .expect("waveform should survive thumbnail seek"),
            first_peaks.as_slice()
        );
        assert_eq!(state.waveform_cache_hits.load(Ordering::Acquire), 1);

        // A second request at the same time serves both projections directly
        // from cache; the status text is the user-visible waveform projection.
        request_preview_internal(state.clone(), app.as_weak(), 0.75, true);
        assert_eq!(state.preview_cache_hits.load(Ordering::Acquire), 1);
        assert_eq!(state.waveform_cache_hits.load(Ordering::Acquire), 2);
        assert!(!state.preview_in_flight.load(Ordering::Acquire));
        let served = lock(&state.preview)
            .as_ref()
            .cloned()
            .expect("cached seek frame should be served");
        assert_ne!(served.pixels, cached.pixels);
        assert!(app
            .get_status_left()
            .to_string()
            .contains(&format!("waveform {} bins", first_peaks.len())));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
