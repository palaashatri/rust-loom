use super::*;

#[cfg(test)]
mod tests {
    use super::*;
    use loom_desktop::ScriptedFileDialogs;

    fn test_app_and_state(scripted: ScriptedFileDialogs) -> (EncodeApp, Arc<AppState>) {
        set_platform();
        let app = EncodeApp::new().expect("create EncodeApp");
        let queue = sample_queue();
        let state = Arc::new(AppState {
            queue: Mutex::new(queue.clone()),
            history: Mutex::new(QueueHistory::default()),
            save_path: Mutex::new(None),
            dialogs: Arc::new(scripted),
            backend: Mutex::new(None),
            cancel: AtomicBool::new(false),
            running: AtomicBool::new(false),
        });
        wire_application(&app, state.clone());
        refresh(&app, &queue, None, false);
        update_history_controls(&app, &state);
        (app, state)
    }

    #[test]
    fn invalid_backend_selection_preserves_queue_and_reports_error() {
        let (app, state) = test_app_and_state(ScriptedFileDialogs::new(
            vec![Some(PathBuf::from("/nonexistent/loom-ffmpeg"))],
            vec![],
        ));
        let before = snapshot(&state).queue_digest();
        app.invoke_choose_backend();
        assert_eq!(snapshot(&state).queue_digest(), before);
        assert!(state.backend().is_none());
        assert!(app.get_setup_error().contains("Cannot run"));
    }

    #[test]
    fn unavailable_backend_guidance_matches_recovery_actions() {
        let (app, _) = test_app_and_state(ScriptedFileDialogs::default());

        assert_eq!(
            app.get_backend_version().as_str(),
            "Choose an FFmpeg executable or use Check again"
        );
    }

    #[test]
    fn new_queue_creates_untitled_clean_state() {
        let scripted = ScriptedFileDialogs::default();
        let (app, state) = test_app_and_state(scripted);
        *state.save_path.lock().unwrap() = Some(PathBuf::from("/tmp/existing.loomencode"));

        app.invoke_new_queue();
        assert_eq!(*state.save_path.lock().unwrap(), None);
        assert_eq!(state.queue.lock().unwrap().name, "Untitled Batch Queue");
        assert_eq!(app.get_queue_name().as_str(), "Untitled Batch Queue");
    }

    #[test]
    fn open_queue_with_dialog_loads_path_and_updates_state() {
        let dir = std::env::temp_dir().join(format!("loom-encode-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("open_test.loomencode");

        let mut q = EncodeQueue::new("loaded-queue", "Loaded Batch Queue");
        q.jobs.clear();
        let bytes = save_encode_queue(&q).unwrap();
        std::fs::write(&file, bytes).unwrap();

        let scripted = ScriptedFileDialogs::new(vec![Some(file.clone())], vec![]);

        let (app, state) = test_app_and_state(scripted);
        app.invoke_open_queue();

        assert_eq!(*state.save_path.lock().unwrap(), Some(file));
        assert_eq!(state.queue.lock().unwrap().name, "Loaded Batch Queue");
        assert_eq!(app.get_queue_name().as_str(), "Loaded Batch Queue");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cancelled_open_leaves_current_queue_untouched() {
        let scripted = ScriptedFileDialogs::new(vec![None], vec![]);

        let (app, state) = test_app_and_state(scripted);
        let original_name = state.queue.lock().unwrap().name.clone();

        app.invoke_open_queue();
        assert_eq!(state.queue.lock().unwrap().name, original_name);
        assert_eq!(app.get_status_left().as_str(), "Open cancelled");
    }

    #[test]
    fn save_untitled_prompts_dialog_and_writes_file() {
        let dir = std::env::temp_dir().join(format!("loom-encode-save-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("saved_queue.loomencode");

        let scripted = ScriptedFileDialogs::new(vec![], vec![Some(file.clone())]);

        let (app, state) = test_app_and_state(scripted);
        assert_eq!(*state.save_path.lock().unwrap(), None);

        app.invoke_save_queue();

        assert_eq!(*state.save_path.lock().unwrap(), Some(file.clone()));
        assert!(file.is_file());
        let read_bytes = std::fs::read(&file).unwrap();
        let loaded = load_encode_queue(&read_bytes).unwrap();
        assert_eq!(loaded.name, "Local Delivery Queue");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_as_prompts_dialog_and_updates_path() {
        let dir = std::env::temp_dir().join(format!("loom-encode-saveas-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file_v1 = dir.join("v1.loomencode");
        let file_v2 = dir.join("v2.loomencode");

        let scripted = ScriptedFileDialogs::new(vec![], vec![Some(file_v2.clone())]);

        let (app, state) = test_app_and_state(scripted);
        *state.save_path.lock().unwrap() = Some(file_v1);

        app.invoke_save_as_queue();

        assert_eq!(*state.save_path.lock().unwrap(), Some(file_v2.clone()));
        assert!(file_v2.is_file());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn queue_configuration_callbacks_update_typed_fields_and_history() {
        let dir = std::env::temp_dir().join(format!("loom-encode-config-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("camera.mov");
        let subtitle = dir.join("captions.srt");
        let output = dir.join("camera.mp4");
        let scripted = ScriptedFileDialogs::new(
            [Some(source.clone()), Some(subtitle.clone())],
            [Some(output.clone())],
        );
        let (app, state) = test_app_and_state(scripted);

        app.invoke_choose_source();
        app.invoke_choose_output();
        app.invoke_choose_subtitles();
        app.invoke_audio_codec_changed("opus".into());
        app.invoke_subtitle_mode_changed(3);
        app.invoke_metadata_copy_changed(false);
        app.invoke_collision_policy_changed(1);

        let queue = state.queue.lock().unwrap();
        let job = &queue.jobs[queue.active_job_index];
        assert_eq!(job.source_file, source.to_string_lossy());
        assert_eq!(job.output_file, output.to_string_lossy());
        assert_eq!(
            job.subtitles.path.as_deref(),
            Some(subtitle.to_string_lossy().as_ref())
        );
        assert_eq!(job.audio.codec, "opus");
        assert_eq!(job.subtitles.mode, SubtitleMode::ConvertSrt);
        assert!(!job.metadata.copy);
        assert_eq!(
            job.destination.collision_policy,
            DestinationCollisionPolicy::Rename
        );
        assert!(app.get_can_undo());
        drop(queue);

        // A plain-text DataTransfer is the same payload used by the native
        // DropArea bridge, so this exercises the real drop callback path.
        app.invoke_source_dropped(DataTransfer::from(SharedString::from("/tmp/dropped.mov")));
        assert_eq!(
            state.queue.lock().unwrap().jobs[0].source_file,
            "/tmp/dropped.mov"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn add_move_remove_callbacks_keep_selection_and_undo_coherent() {
        let scripted = ScriptedFileDialogs::default();
        let (app, state) = test_app_and_state(scripted);
        app.invoke_add_job();
        {
            let queue = state.queue.lock().unwrap();
            assert_eq!(queue.active_job_index, 1);
            assert_eq!(queue.selected_job_indices, vec![1]);
        }
        app.invoke_move_job(1, 0);
        {
            let queue = state.queue.lock().unwrap();
            assert_eq!(queue.active_job_index, 0);
            assert_eq!(queue.selected_job_indices, vec![0]);
            assert_eq!(queue.jobs[0].id, "job-2");
        }
        assert!(app.get_can_undo());
        app.invoke_remove_job();
        let queue = state.queue.lock().unwrap();
        assert_eq!(queue.jobs.len(), 1);
        assert_eq!(queue.selected_job_indices, vec![0]);
    }

    #[test]
    fn responsive_layout_switches_at_the_compact_breakpoint() {
        set_platform();
        let app = EncodeApp::new().expect("create EncodeApp");

        configure_responsive_layout(&app, (1179, 800));
        assert!(app.get_compact_layout());

        configure_responsive_layout(&app, (1180, 800));
        assert!(!app.get_compact_layout());
    }

    #[test]
    fn responsive_policy_transition_probes_are_exact() {
        set_platform();
        let app = EncodeApp::new().expect("create EncodeApp");
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
    fn dialog_requests_omit_empty_parent_for_bare_filenames() {
        let source = source_file_request(Some(Path::new("source.mov")));
        let subtitle = subtitle_file_request(Some(Path::new("captions.srt")));
        let output = output_file_request(Some(Path::new("output.mp4")), None);
        let queue_open = open_queue_request(Some(Path::new("queue.loomencode")));
        let queue_save = save_queue_request(Some(Path::new("queue.loomencode")));

        assert!(source.initial_directory.is_none());
        assert!(subtitle.initial_directory.is_none());
        assert!(output.initial_directory.is_none());
        assert!(queue_open.initial_directory.is_none());
        assert!(queue_save.initial_directory.is_none());
    }

    #[test]
    fn dropped_file_uri_preserves_root_and_decodes_escapes() {
        let transfer = DataTransfer::from(SharedString::from("file:///tmp/encode%20source.mov"));
        assert_eq!(
            dropped_path(&transfer),
            Some("/tmp/encode source.mov".into())
        );

        let transfer = DataTransfer::from(SharedString::from("file:////tmp/encode.mov"));
        assert_eq!(dropped_path(&transfer), Some("//tmp/encode.mov".into()));
    }

    #[test]
    fn drop_targets_expose_default_accessible_chooser_actions() {
        let inspector = include_str!("../ui/encode_inspector.slint");
        assert!(inspector.contains(
            "accessible-action-default => { if (!root.running) { root.choose-source(); } }"
        ));
        assert!(inspector.contains(
            "accessible-action-default => { if (!root.running) { root.choose-output(); } }"
        ));
    }

    #[test]
    fn queue_reorder_controls_have_a_non_clipping_hit_target() {
        let queue = include_str!("../ui/encode_queue.slint");
        assert!(queue.contains("min-width: 48px;"));
        assert!(queue.contains("preferred-width: 48px;"));
        assert!(queue.contains("ToolbarIconButton"));
        assert!(!queue.contains("ToolbarButton"));
        assert!(!queue.contains("min-width: 28px;"));
        assert_eq!(queue.matches("horizontal-stretch: 1;").count(), 3);
    }
}
