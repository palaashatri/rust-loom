use super::*;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn video_timeline_digest_stability() {
        let mut project = VideoProject::new("proj-digest", "Digest Edit");
        let mut intro = Clip::new("c1", "Intro.mov", 6.0);
        intro.source_path = "intro.mov".into();
        intro.set_playback_rate(2.0).unwrap();
        let mut broll = Clip::new("c2", "B-Roll.mov", 4.0);
        broll.in_point = 1.0;
        broll.out_point = 5.0;
        broll.sync_duration().unwrap();
        project.tracks[0].insert_clip(intro).unwrap();
        project.tracks[0].insert_clip(broll).unwrap();

        // Stable across repeated calls.
        let baseline = project.timeline_digest();
        assert_eq!(baseline, project.timeline_digest());

        // Trimming a clip changes the digest.
        let mut trimmed = project.clone();
        trimmed.tracks[0].clips[0].trim_out(4.0).unwrap();
        assert_ne!(trimmed.timeline_digest(), baseline);

        // Splitting a clip changes the digest.
        let mut split = project.clone();
        split.split_clip(0, "c2", 2.0).unwrap();
        assert_ne!(split.timeline_digest(), baseline);

        // Adding a marker changes the digest: markers are user-authored annotations
        // (added through VideoProject::add_marker), a choice documented on
        // timeline_digest itself.
        let mut marked = project.clone();
        marked
            .add_marker(TimelineMarker {
                id: "m1".into(),
                time: 1.0,
                label: "Cut Here".into(),
                color: "#ef4444".into(),
            })
            .unwrap();
        assert_ne!(marked.timeline_digest(), baseline);

        // An empty project digests differently from a populated one.
        let empty = VideoProject::new("proj-empty", "Empty");
        assert_ne!(empty.timeline_digest(), baseline);
    }

    #[test]
    fn stderr_prefix_reader_drains_beyond_retained_limit() {
        let limit = WAVEFORM_STDERR_LIMIT_BYTES as usize;
        let mut input = vec![b'x'; limit + WAVEFORM_READ_BUFFER_BYTES + 17];
        input.extend_from_slice(b"tail");
        let mut reader = Cursor::new(input.clone());

        let retained = drain_reader_with_prefix(&mut reader, limit).unwrap();

        assert_eq!(reader.position() as usize, input.len());
        assert_eq!(retained.len(), limit);
        assert!(retained.bytes().all(|byte| byte == b'x'));
    }

    #[test]
    fn test_video_creation() {
        let proj = VideoProject::new("v-1", "Documentary Edit");
        assert_eq!(proj.frame_rate, 30.0);
        assert_eq!(proj.tracks.len(), 2);
    }

    #[test]
    fn test_add_clips() {
        let mut proj = VideoProject::new("v-1", "Documentary Edit");
        proj.tracks[0].add_clip(Clip::new("clip-1", "B-Roll Shot 1.mp4", 5.0));
        assert_eq!(proj.total_clips(), 1);
    }

    #[test]
    fn test_select_track_rejects_invalid_index() {
        let mut proj = VideoProject::new("v-1", "Documentary Edit");
        assert!(proj.select_track(1));
        assert!(!proj.select_track(2));
        assert_eq!(proj.active_track_index, 1);
    }

    #[test]
    fn test_save_load_roundtrip() {
        let mut proj = VideoProject::new("v-test", "Short Film");
        proj.tracks[0].add_clip(Clip::new("c1", "Scene1.mp4", 12.0));
        let bytes = save_video_project(&proj).expect("save failed");
        let arch = PackageArchive::from_bytes(&bytes).expect("archive parse failed");
        let manifest_bytes = arch.get("manifest.json").expect("manifest missing");
        let manifest_str = std::str::from_utf8(manifest_bytes).expect("manifest not utf8");
        let manifest = pkg_json::parse_manifest(manifest_str).expect("manifest parse failed");
        assert_eq!(manifest.kind, PackageKind::Video);
        arch.validate_manifest(&manifest)
            .expect("manifest validation failed");
        let loaded = load_video_project(&bytes).expect("load failed");
        assert_eq!(loaded.name, "Short Film");
        assert_eq!(loaded.total_clips(), 1);
    }

    #[test]
    fn split_trim_move_and_ripple_edits_are_consistent() {
        let mut project = VideoProject::new("video-edit", "Edit Test");
        let mut clip = Clip::new("clip", "Source", 10.0);
        clip.source_path = "source.mov".into();
        project.tracks[0].insert_clip(clip).unwrap();
        let (left, right) = project.split_clip(0, "clip", 4.0).unwrap();
        assert_eq!(project.tracks[0].clips.len(), 2);
        assert_eq!(project.tracks[0].clips[0].duration, 4.0);
        project.move_clip(0, 0, &right, 8.0, false).unwrap();
        assert_eq!(project.duration(), 14.0);
        project.tracks[0].remove_clip(&left, true).unwrap();
        assert_eq!(project.tracks[0].clips[0].start_time, 4.0);
    }

    #[test]
    fn playback_rate_render_plan_and_edl_are_real() {
        let mut project = VideoProject::new("video-plan", "Plan");
        let mut clip = Clip::new("c1", "Shot", 8.0);
        clip.source_path = "shot.mov".into();
        clip.set_playback_rate(2.0).unwrap();
        project.tracks[0].insert_clip(clip).unwrap();
        assert_eq!(project.duration(), 4.0);
        let plan = project.render_plan();
        assert_eq!(plan[0].playback_rate, 2.0);
        assert!(project.to_edl().contains("FROM CLIP NAME: c1"));
    }

    #[test]
    fn captions_markers_and_overlap_detection_are_sorted() {
        let mut project = VideoProject::new("video-meta", "Metadata");
        project
            .add_marker(TimelineMarker {
                id: "m2".into(),
                time: 2.0,
                label: "Second".into(),
                color: "#fff".into(),
            })
            .unwrap();
        project
            .add_marker(TimelineMarker {
                id: "m1".into(),
                time: 1.0,
                label: "First".into(),
                color: "#fff".into(),
            })
            .unwrap();
        project
            .add_caption(CaptionCue {
                id: "cap".into(),
                start: 0.0,
                end: 1.5,
                text: "Hello".into(),
                language: "en".into(),
            })
            .unwrap();
        assert_eq!(project.markers[0].id, "m1");
        assert_eq!(project.duration(), 1.5);
        let mut first = Clip::new("a", "A", 2.0);
        first.start_time = 0.0;
        let mut second = Clip::new("b", "B", 2.0);
        second.start_time = 1.0;
        project.tracks[0].insert_clip(first).unwrap();
        project.tracks[0].insert_clip(second).unwrap();
        assert_eq!(project.tracks[0].overlaps(), vec![("a".into(), "b".into())]);
    }

    #[test]
    fn session_history_restores_timeline_mutations() {
        let mut session = VideoSession::new(VideoProject::new("video", "Video"));
        session.checkpoint();
        session.project.tracks[0].add_clip(Clip::new("clip", "Clip", 2.0));
        assert!(session.can_undo());
        assert!(session.undo());
        assert_eq!(session.project.total_clips(), 0);
        assert!(session.redo());
        assert_eq!(session.project.total_clips(), 1);
    }

    #[test]
    fn export_plan_maps_real_sources_and_concat_filter() {
        let directory =
            std::env::temp_dir().join(format!("loom-video-plan-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let source = directory.join("source.mov");
        std::fs::write(&source, b"fixture").unwrap();
        let mut project = VideoProject::new("video", "Video");
        let mut clip = Clip::new("clip", "Clip", 2.0);
        clip.source_path = source.to_string_lossy().into_owned();
        project.tracks[0].add_clip(clip);
        let tools = MediaTools {
            ffmpeg: "ffmpeg".into(),
            ffprobe: "ffprobe".into(),
            ffplay: "ffplay".into(),
            version: "test".into(),
        };
        let plan = build_timeline_export_plan(&project, &tools, directory.join("out.mp4")).unwrap();
        assert!(plan
            .arguments
            .iter()
            .any(|argument| argument.contains("concat=n=1")));
        assert!(plan.arguments.iter().any(|argument| argument == "libx264"));
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn slip_and_slide_clip_operations() {
        let mut track = Track::new("v1", "Video", TrackType::Video);
        let mut clip = Clip::new("c1", "Clip 1", 5.0);
        clip.start_time = 2.0;
        clip.in_point = 1.0;
        clip.out_point = 6.0;
        track.insert_clip(clip).unwrap();

        // Slip by +1.0 sec
        track.slip_clip("c1", 1.0).unwrap();
        let c = &track.clips[0];
        assert_eq!(c.start_time, 2.0);
        assert_eq!(c.in_point, 2.0);
        assert_eq!(c.out_point, 7.0);

        // Slide by +3.0 sec
        track.slide_clip("c1", 3.0).unwrap();
        let c = &track.clips[0];
        assert_eq!(c.start_time, 5.0);
        assert_eq!(c.in_point, 2.0);
        assert_eq!(c.out_point, 7.0);
    }

    #[test]
    fn clip_delete_and_ripple_delete_operations() {
        let mut project = VideoProject::new("v-test", "Video Test");
        let mut c1 = Clip::new("c1", "Clip 1", 3.0);
        c1.start_time = 0.0;
        let mut c2 = Clip::new("c2", "Clip 2", 4.0);
        c2.start_time = 3.0;
        let mut c3 = Clip::new("c3", "Clip 3", 2.0);
        c3.start_time = 7.0;

        project.tracks[0].clips.clear();
        project.tracks[0].add_clip(c1);
        project.tracks[0].add_clip(c2);
        project.tracks[0].add_clip(c3);
        assert_eq!(project.tracks[0].clips.len(), 3);

        // Ripple delete middle clip (c2, duration 4.0)
        let removed = project.ripple_delete_clip(0, "c2").unwrap();
        assert_eq!(removed.id, "c2");
        assert_eq!(project.tracks[0].clips.len(), 2);
        // c3 start_time should ripple from 7.0 to 3.0
        assert_eq!(project.tracks[0].clips[1].id, "c3");
        assert_eq!(project.tracks[0].clips[1].start_time, 3.0);

        // Delete first clip without rippling
        let removed1 = project.delete_clip(0, "c1").unwrap();
        assert_eq!(removed1.id, "c1");
        assert_eq!(project.tracks[0].clips.len(), 1);

        // Marker removal
        project
            .add_marker(TimelineMarker {
                id: "m1".into(),
                time: 2.5,
                label: "Cut".into(),
                color: "#ff0000".into(),
            })
            .unwrap();
        assert_eq!(project.markers.len(), 1);
        assert!(project.remove_marker("m1"));
        assert_eq!(project.markers.len(), 0);
    }

    #[test]
    fn timecode_formatting_and_smpte_conversions() {
        let tc = Timecode::from_seconds(3665.5, 30.0);
        assert_eq!(tc.hours, 1);
        assert_eq!(tc.minutes, 1);
        assert_eq!(tc.seconds, 5);
        assert_eq!(tc.frames, 15);
        assert_eq!(tc.format_smpte(), "01:01:05:15");

        let secs = tc.to_seconds(30.0);
        assert!((secs - 3665.5).abs() < 1e-4);
    }

    #[test]
    fn clip_speed_and_effective_duration_scaling() {
        let mut clip = Clip::new("c1", "Shot 1", 10.0);
        assert_eq!(clip.duration, 10.0);
        assert_eq!(clip.effective_timeline_duration(), 10.0);

        // 2x Fast Forward
        clip.set_speed(2.0);
        assert_eq!(clip.playback_rate, 2.0);
        assert_eq!(clip.duration, 5.0);

        // 0.5x Slow Motion
        clip.set_speed(0.5);
        assert_eq!(clip.playback_rate, 0.5);
        assert_eq!(clip.duration, 20.0);
    }

    #[test]
    fn timeline_zoom_and_clip_count() {
        let mut project = VideoProject::new("proj-zoom", "Zoom Project");
        assert_eq!(project.total_clips(), 0);

        project.tracks[0].add_clip(Clip::new("c1", "Shot 1", 5.0));
        assert_eq!(project.total_clips(), 1);

        // 100 pixels per second
        assert_eq!(VideoProject::seconds_to_pixels(5.5, 100.0), 550.0);
        assert_eq!(VideoProject::pixels_to_seconds(550.0, 100.0), 5.5);
    }

    #[test]
    fn timeline_track_close_gaps() {
        let mut track = Track::new("v1", "Video 1", TrackType::Video);
        let mut c1 = Clip::new("c1", "Intro", 3.0);
        c1.start_time = 2.0; // gap of 2s before c1
        let mut c2 = Clip::new("c2", "Main", 5.0);
        c2.start_time = 8.0; // gap of 3s between c1 and c2

        track.add_clip(c1);
        track.add_clip(c2);

        let moved = track.close_gaps().unwrap();
        assert_eq!(moved, 2);
        assert_eq!(track.clips[0].start_time, 0.0);
        assert_eq!(track.clips[1].start_time, 3.0);
    }

    #[test]
    fn waveform_peak_decimation() {
        let samples = vec![0.1, -0.8, 0.9, -0.2, 0.5, -0.5, 0.3, -0.1];
        let peaks = compute_waveform_peaks(&samples, 2);
        assert_eq!(peaks.len(), 2);

        // First half: min is -0.8, max is 0.9
        assert_eq!(peaks[0].0, -0.8);
        assert_eq!(peaks[0].1, 0.9);

        // Second half: min is -0.5, max is 0.5
        assert_eq!(peaks[1].0, -0.5);
        assert_eq!(peaks[1].1, 0.5);
    }

    #[test]
    fn timeline_split_clip_operation() {
        let mut track = Track::new("t1", "Video 1", TrackType::Video);
        let mut clip = Clip::new("c1", "Media", 10.0);
        clip.start_time = 0.0;
        track.add_clip(clip);

        // Split clip at 4.0s
        let new_id = track.split_clip("c1", 4.0).unwrap();
        assert_eq!(track.clips.len(), 2);

        // First clip: 0.0 -> 4.0, duration 4.0
        assert_eq!(track.clips[0].id, "c1");
        assert_eq!(track.clips[0].start_time, 0.0);
        assert_eq!(track.clips[0].duration, 4.0);

        // Second clip: 4.0 -> 10.0, duration 6.0
        assert_eq!(track.clips[1].id, new_id);
        assert_eq!(track.clips[1].start_time, 4.0);
        assert_eq!(track.clips[1].duration, 6.0);
        assert_eq!(track.clips[1].in_point, 4.0);
    }

    #[test]
    fn timeline_split_clip_respects_playback_rate() {
        for (rate, split_time, expected_left_duration, expected_right_duration) in
            [(2.0, 2.0, 2.0, 3.0), (0.5, 4.0, 4.0, 16.0)]
        {
            let mut track = Track::new("t1", "Video 1", TrackType::Video);
            let mut clip = Clip::new("c1", "Media", 10.0);
            clip.set_playback_rate(rate).unwrap();
            track.add_clip(clip);

            let new_id = track.split_clip("c1", split_time).unwrap();
            let left = track.clips.iter().find(|clip| clip.id == "c1").unwrap();
            let right = track.clips.iter().find(|clip| clip.id == new_id).unwrap();

            assert!((left.start_time - 0.0).abs() < 1e-9);
            assert!((left.duration - expected_left_duration).abs() < 1e-9);
            assert!((left.in_point - 0.0).abs() < 1e-9);
            assert!((left.out_point - (split_time * rate)).abs() < 1e-9);
            assert!((right.start_time - split_time).abs() < 1e-9);
            assert!((right.duration - expected_right_duration).abs() < 1e-9);
            assert!((right.in_point - (split_time * rate)).abs() < 1e-9);
            assert!((right.out_point - 10.0).abs() < 1e-9);
            assert!((track.duration() - 10.0 / rate).abs() < 1e-9);
        }
    }

    #[test]
    fn timeline_edit_point_snapping() {
        let mut track = Track::new("t1", "Video 1", TrackType::Video);
        let mut clip = Clip::new("c1", "Media", 5.0);
        clip.start_time = 2.0; // spans 2.0 -> 7.0
        track.add_clip(clip);

        let marker = TimelineMarker {
            id: "m1".into(),
            time: 12.0,
            label: "Intro".into(),
            color: "blue".into(),
        };

        // Snapping near start boundary (2.0s)
        let snapped_start = snap_timeline_to_edit_points(
            2.05,
            std::slice::from_ref(&track),
            std::slice::from_ref(&marker),
            0.1,
        );
        assert_eq!(snapped_start, 2.0);

        // Snapping near end boundary (7.0s)
        let snapped_end = snap_timeline_to_edit_points(
            6.98,
            std::slice::from_ref(&track),
            std::slice::from_ref(&marker),
            0.1,
        );
        assert_eq!(snapped_end, 7.0);

        // Snapping near marker (12.0s)
        let snapped_marker = snap_timeline_to_edit_points(
            12.04,
            std::slice::from_ref(&track),
            std::slice::from_ref(&marker),
            0.1,
        );
        assert_eq!(snapped_marker, 12.0);
    }

    #[test]
    fn roll_and_slip_editing() {
        let mut left = Clip::new("c1", "MediaA", 5.0);
        left.start_time = 0.0;
        let mut right = Clip::new("c2", "MediaB", 5.0);
        right.start_time = 5.0;

        // Roll cut by +1.0s
        roll_edit(&mut left, &mut right, 1.0).unwrap();
        assert_eq!(left.duration, 6.0);
        assert_eq!(right.start_time, 6.0);
        assert_eq!(right.in_point, 1.0);
        assert_eq!(right.duration, 4.0);

        // Slip edit right clip by +2.0s within 10.0s media
        slip_edit(&mut right, 2.0, 10.0).unwrap();
        assert_eq!(right.in_point, 3.0);
        assert_eq!(right.duration, 4.0); // duration unchanged
        assert_eq!(right.start_time, 6.0); // start_time unchanged
    }

    #[test]
    fn video_transition_overlap_calculation() {
        let (center_start, center_end) = calculate_transition_overlap(10.0, 2.0, "CenterOnCut");
        assert_eq!(center_start, 9.0);
        assert_eq!(center_end, 11.0);

        let (start_on_cut, start_end) = calculate_transition_overlap(10.0, 2.0, "StartOnCut");
        assert_eq!(start_on_cut, 10.0);
        assert_eq!(start_end, 12.0);

        let (end_on_cut, end_end) = calculate_transition_overlap(10.0, 2.0, "EndOnCut");
        assert_eq!(end_on_cut, 8.0);
        assert_eq!(end_end, 10.0);
    }

    #[test]
    fn timeline_markers_and_color_presets() {
        let mut proj = VideoProject::new("vp1", "Test Video");
        assert_eq!(MarkerColor::Blue.as_hex(), "#3b82f6");
        assert_eq!(MarkerColor::Red.as_hex(), "#ef4444");

        let m1 = TimelineMarker {
            id: "m1".into(),
            time: 5.0,
            label: "Cut 1".into(),
            color: MarkerColor::Red.as_hex().into(),
        };
        let m2 = TimelineMarker {
            id: "m2".into(),
            time: 15.0,
            label: "Cut 2".into(),
            color: MarkerColor::Green.as_hex().into(),
        };

        proj.add_marker(m1).unwrap();
        proj.add_marker(m2).unwrap();

        // Sorted by time
        assert_eq!(proj.markers[0].id, "m1");
        assert_eq!(proj.markers[1].id, "m2");

        let in_range = proj.find_markers_in_range(0.0, 10.0);
        assert_eq!(in_range.len(), 1);
        assert_eq!(in_range[0].id, "m1");

        assert!(proj.remove_marker("m1"));
        assert_eq!(proj.markers.len(), 1);
    }

    #[test]
    fn audio_envelope_volume_automation() {
        let mut env = AudioEnvelope::new();
        assert_eq!(env.evaluate_volume_db_at(2.5), 0.0);

        // Fade in from -24dB at 0.0s to 0dB at 2.0s
        env.add_key(0.0, -24.0);
        env.add_key(2.0, 0.0);

        assert_eq!(env.evaluate_volume_db_at(-1.0), -24.0);
        assert_eq!(env.evaluate_volume_db_at(0.0), -24.0);
        assert_eq!(env.evaluate_volume_db_at(1.0), -12.0); // halfway
        assert_eq!(env.evaluate_volume_db_at(2.0), 0.0);
        assert_eq!(env.evaluate_volume_db_at(5.0), 0.0);
    }

    #[test]
    fn ken_burns_pan_and_zoom_interpolation() {
        let effect = KenBurnsEffect {
            start_rect: (0.0, 0.0, 1.0, 1.0),
            end_rect: (0.2, 0.2, 0.6, 0.6),
        };

        let start = effect.interpolate_crop_rect(0.0);
        assert_eq!(start, (0.0, 0.0, 1.0, 1.0));

        let end = effect.interpolate_crop_rect(1.0);
        assert_eq!(end, (0.2, 0.2, 0.6, 0.6));

        let mid = effect.interpolate_crop_rect(0.5);
        // At t=0.5, smoothstep(0.5) = 0.5
        assert_eq!(mid, (0.1, 0.1, 0.8, 0.8));
    }

    #[test]
    fn track_audio_mixer_pan_and_volume() {
        let mut track_audio = TrackAudioConfig::default();
        let (left, right) = track_audio.stereo_linear_gains();
        // At center pan, gains should be equal and nonzero (~0.707)
        assert!((left - right).abs() < 1e-4);
        assert!(left > 0.7);

        // Muted track should produce silence
        track_audio.is_muted = true;
        assert_eq!(track_audio.stereo_linear_gains(), (0.0, 0.0));

        // Panned hard left
        track_audio.is_muted = false;
        track_audio.pan = -1.0;
        let (left_pan, right_pan) = track_audio.stereo_linear_gains();
        assert!(left_pan > 0.99);
        assert!(right_pan < 1e-4);
    }

    #[test]
    fn master_audio_limiter_brickwall_processing() {
        let limiter = MasterAudioLimiter {
            ceiling_db: -1.0, // ceiling ~ 0.891
            threshold_db: -1.0,
            release_ms: 20.0,
            enabled: true,
        };

        let mut left = vec![1.5f32, 2.0f32, 0.5f32];
        let mut right = vec![1.2f32, 1.8f32, 0.4f32];

        limiter.process_stereo_samples(&mut left, &mut right, 48000);

        let ceiling_lin = 10.0f32.powf(-1.0 / 20.0);
        // All limited samples must not exceed ceiling
        for s in left {
            assert!(s <= ceiling_lin + 1e-5);
            assert!(s >= -ceiling_lin - 1e-5);
        }
        for s in right {
            assert!(s <= ceiling_lin + 1e-5);
            assert!(s >= -ceiling_lin - 1e-5);
        }
    }

    #[test]
    fn clip_color_tag_labeling() {
        let mut clip = Clip::new("c1", "Intro B-Roll", 5.0);
        assert_eq!(clip.color_tag, ClipColorTag::Orange);
        assert_eq!(clip.color_tag.hex_color(), "#f97316");

        clip.set_color_tag(ClipColorTag::Teal);
        assert_eq!(clip.color_tag, ClipColorTag::Teal);
        assert_eq!(clip.color_tag.hex_color(), "#14b8a6");
    }

    #[test]
    fn align_clips_to_beat_grid_bpm() {
        // At 120 BPM, 1 beat = 0.5s. Beats occur at 0.0, 0.5, 1.0, 1.5, 2.0...
        let mut clips = vec![
            Clip {
                start_time: 0.53, // within 0.05s of 0.5s beat -> snaps to 0.5
                ..Clip::new("c1", "Clip 1", 2.0)
            },
            Clip {
                start_time: 1.25, // 0.25s away from 1.0 and 1.5 -> does NOT snap with 0.1s threshold
                ..Clip::new("c2", "Clip 2", 2.0)
            },
            Clip {
                start_time: 1.98, // within 0.05s of 2.0s beat -> snaps to 2.0
                ..Clip::new("c3", "Clip 3", 2.0)
            },
        ];

        let aligned = align_clips_to_beat_grid(&mut clips, 120.0, 0.0, 0.05);
        assert_eq!(aligned, 2);
        assert_eq!(clips[0].start_time, 0.5);
        assert_eq!(clips[1].start_time, 1.25);
        assert_eq!(clips[2].start_time, 2.0);
    }

    #[test]
    fn multicam_active_angle_switching() {
        let angles = vec![
            MulticamAngle {
                clip_id: "wide".into(),
                label: "Wide".into(),
            },
            MulticamAngle {
                clip_id: "closeup".into(),
                label: "Close-Up".into(),
            },
            MulticamAngle {
                clip_id: "overhead".into(),
                label: "Overhead".into(),
            },
        ];
        // Deliberately unsorted; the function must not mutate its input.
        let cuts = [
            MulticamCut {
                timeline_time: 5.5,
                angle_index: 2,
            },
            MulticamCut {
                timeline_time: 2.0,
                angle_index: 1,
            },
        ];

        assert_eq!(active_angle_at(&angles, &cuts, 0.0), Some(0));
        assert_eq!(active_angle_at(&angles, &cuts, 1.999), Some(0));
        assert_eq!(active_angle_at(&angles, &cuts, 2.0), Some(1));
        assert_eq!(active_angle_at(&angles, &cuts, 5.499), Some(1));
        assert_eq!(active_angle_at(&angles, &cuts, 5.5), Some(2));
        assert_eq!(active_angle_at(&angles, &cuts, 120.0), Some(2));
        assert_eq!(cuts.len(), 2);
        assert_eq!(cuts[0].timeline_time, 5.5);

        assert_eq!(active_angle_at(&[], &cuts, 3.0), None);

        let out_of_range = [MulticamCut {
            timeline_time: 1.0,
            angle_index: 7,
        }];
        assert_eq!(active_angle_at(&angles, &out_of_range, 3.0), Some(0));
    }

    #[test]
    fn ducking_envelope_generation() {
        let config = DuckingConfig {
            reduction_db: 12.0,
            attack_seconds: 0.5,
            release_seconds: 0.8,
            padding_seconds: 1.0,
        };

        // Single dialogue region [10, 15]: padded to [9, 16], attack ramp from 8.5,
        // full reduction across the padded region, release ramp ending at 16.8.
        let keys = generate_ducking_envelope(&config, &[(10.0, 15.0)]).unwrap();
        let expected = [(8.5, 0.0), (9.0, -12.0), (16.0, -12.0), (16.8, 0.0)];
        assert_eq!(keys.len(), expected.len());
        for ((time, gain), (want_time, want_gain)) in keys.iter().zip(expected.iter()) {
            assert!(
                (time - want_time).abs() < 1e-9,
                "time {time} != {want_time}"
            );
            assert!(
                (gain - want_gain).abs() < 1e-9,
                "gain {gain} != {want_gain}"
            );
        }

        // Invalid parameters are rejected.
        for bad in [
            DuckingConfig {
                reduction_db: 0.0,
                ..config.clone()
            },
            DuckingConfig {
                reduction_db: -3.0,
                ..config.clone()
            },
            DuckingConfig {
                attack_seconds: -0.5,
                ..config.clone()
            },
            DuckingConfig {
                release_seconds: -1.0,
                ..config.clone()
            },
            DuckingConfig {
                padding_seconds: -0.25,
                ..config.clone()
            },
        ] {
            assert!(generate_ducking_envelope(&bad, &[(10.0, 15.0)]).is_err());
        }

        // Inverted region is rejected.
        assert!(generate_ducking_envelope(&config, &[(15.0, 10.0)]).is_err());

        // Two unsorted regions produce sorted ascending output with no merging.
        let keys = generate_ducking_envelope(&config, &[(20.0, 25.0), (5.0, 8.0)]).unwrap();
        assert_eq!(keys.len(), 8);
        for pair in keys.windows(2) {
            assert!(pair[0].0 < pair[1].0, "keys not sorted: {keys:?}");
        }
        assert!((keys[0].0 - 3.5).abs() < 1e-9);
        assert!((keys[7].0 - 26.8).abs() < 1e-9);
    }

    #[test]
    fn proxy_registry_lifecycle() {
        let mut registry = ProxyRegistry::new();
        registry
            .set_link(ProxyLink {
                source_clip_id: "clip-a".into(),
                proxy_path: "/media/proxies/a.mov".into(),
                scale: 0.5,
                codec: "prores_proxy".into(),
            })
            .unwrap();
        registry
            .set_link(ProxyLink {
                source_clip_id: "clip-b".into(),
                proxy_path: "/media/proxies/b.mov".into(),
                scale: 0.25,
                codec: "prores_proxy".into(),
            })
            .unwrap();
        assert_eq!(registry.links.len(), 2);

        // Replacing the link for an existing source clip updates in place.
        registry
            .set_link(ProxyLink {
                source_clip_id: "clip-a".into(),
                proxy_path: "/media/proxies/a_v2.mov".into(),
                scale: 0.5,
                codec: "prores_proxy".into(),
            })
            .unwrap();
        assert_eq!(registry.links.len(), 2);
        assert_eq!(
            registry.proxy_for("clip-a"),
            Some("/media/proxies/a_v2.mov")
        );
        assert_eq!(registry.proxy_for("clip-b"), Some("/media/proxies/b.mov"));
        assert_eq!(registry.proxy_for("clip-missing"), None);

        // Removal succeeds once, then reports false.
        assert!(registry.remove_link("clip-b"));
        assert!(!registry.remove_link("clip-b"));
        assert_eq!(registry.links.len(), 1);

        // Only clip-a's proxy is reported stale when its path fails the predicate.
        let stale = registry.stale_links(|path| path != "/media/proxies/a_v2.mov");
        assert_eq!(stale, vec!["clip-a".to_string()]);

        // Validation errors.
        for bad_scale in [0.0_f32, 1.5] {
            let result = registry.set_link(ProxyLink {
                source_clip_id: "clip-c".into(),
                proxy_path: "/media/proxies/c.mov".into(),
                scale: bad_scale,
                codec: "prores_proxy".into(),
            });
            assert!(result.is_err(), "scale {bad_scale} must be rejected");
        }
        assert!(registry
            .set_link(ProxyLink {
                source_clip_id: "clip-c".into(),
                proxy_path: String::new(),
                scale: 0.5,
                codec: "prores_proxy".into(),
            })
            .is_err());
        assert_eq!(registry.links.len(), 1);
    }

    #[test]
    fn shuttle_transport_ladder() {
        // Stopped state has zero rate
        let mut s = ShuttleState::stopped();
        assert!(s.is_stopped());
        assert_eq!(s.rate(), 0.0);

        // Forward ladder: 0.5x, 1x, 2x, 4x, 8x then saturates
        s.press_shuttle_forward();
        assert_eq!(s.rate(), 0.5);
        s.press_shuttle_forward();
        assert_eq!(s.rate(), 1.0);
        s.press_shuttle_forward();
        assert_eq!(s.rate(), 2.0);
        s.press_shuttle_forward();
        assert_eq!(s.rate(), 4.0);
        s.press_shuttle_forward();
        assert_eq!(s.rate(), 8.0);
        let top = s.clone();
        s.press_shuttle_forward();
        assert_eq!(s.rate(), 8.0, "ladder saturates at maximum speed");
        assert_eq!(s.step, SHUTTLE_SPEED_LADDER.len());

        // Stop resets to neutral paused state
        s.press_stop();
        assert!(s.is_stopped());
        assert_eq!(s.rate(), 0.0);

        // Reverse ladder mirrors with negative rates
        let mut r = ShuttleState::default();
        assert!(r.is_stopped());
        r.press_shuttle_reverse();
        assert_eq!(r.rate(), -0.5);
        r.press_shuttle_reverse();
        assert_eq!(r.rate(), -1.0);
        r.press_shuttle_reverse();
        assert_eq!(r.direction, ShuttleDirection::Reverse);
        assert!((r.rate() - (-2.0)).abs() < 1e-9);

        // Direction switch from reverse keeps the step magnitude
        r.press_shuttle_forward();
        assert_eq!(r.direction, ShuttleDirection::Forward);
        assert!((r.rate() - 4.0).abs() < 1e-9);

        // Determinism: identical sequences produce identical states
        let mut a = ShuttleState::stopped();
        let mut b = ShuttleState::stopped();
        for _ in 0..3 {
            a.press_shuttle_forward();
            b.press_shuttle_forward();
        }
        assert_eq!(a, b);
        assert_eq!(top.direction, ShuttleDirection::Forward);
    }

    #[test]
    fn normalize_gain_analysis() {
        // A clip peaking at 0.25 needs +12.04 dB to hit 1.0
        let clip = [0.0, 0.1, -0.25, 0.05];
        let gain = suggest_normalize_gain(&clip, 1.0).unwrap();
        assert!((gain - (20.0 * 4.0f64.log10())).abs() < 1e-9);

        // Halving the target halves the linear ratio (+6.02 dB from 0.25)
        let half = suggest_normalize_gain(&clip, 0.5).unwrap();
        assert!((half - (20.0 * 2.0f64.log10())).abs() < 1e-9);

        // Already-peaked clip needs no gain
        assert!((suggest_normalize_gain(&[1.0, -0.5], 1.0).unwrap()).abs() < 1e-9);
        // Over-peaked input clamps and attenuates: 0.5 ratio is -6.02 dB
        let attenuation = suggest_normalize_gain(&[2.0], 0.5).unwrap();
        assert!((attenuation - (-6.020_599_913_279_624)).abs() < 1e-6);

        // Silence and empty inputs need zero gain
        assert_eq!(suggest_normalize_gain(&[], 1.0).unwrap(), 0.0);
        assert_eq!(suggest_normalize_gain(&[0.0; 10], 0.8).unwrap(), 0.0);

        // Invalid target peaks are rejected
        assert!(suggest_normalize_gain(&[0.5], -0.1).is_err());
        assert!(suggest_normalize_gain(&[0.5], 1.5).is_err());
        assert!(suggest_normalize_gain(&[0.5], f32::NAN).is_err());

        // Gain clamping bounds boost but allows attenuation
        assert_eq!(clamp_gain_db(30.0, 12.0), 12.0);
        assert_eq!(clamp_gain_db(-90.0, 12.0), -60.0);
        assert_eq!(clamp_gain_db(3.0, 12.0), 3.0);
    }

    #[test]
    fn srt_round_trip_parsing() {
        let sample = "1\r\n00:00:01,000 --> 00:00:02,500\r\nHello there.\r\n\r\n\
                      2\r\n00:00:03.250 --> 00:00:05,750\r\nSecond cue,\r\nkeeps interior newlines.\r\n";
        let cues = parse_srt(sample).expect("sample should parse");
        assert_eq!(cues.len(), 2);
        assert_eq!(
            cues[0],
            SubtitleCue {
                index: 1,
                start_seconds: 1.0,
                end_seconds: 2.5,
                text: "Hello there.".into(),
            }
        );
        assert_eq!(cues[1].start_seconds, 3.25);
        assert_eq!(cues[1].end_seconds, 5.75);
        assert_eq!(cues[1].text, "Second cue,\nkeeps interior newlines.");

        let written = write_srt(&cues);
        assert!(written.contains("00:00:01,000 --> 00:00:02,500"));
        assert!(written.contains("00:00:03,250 --> 00:00:05,750"));
        let reparsed = parse_srt(&written).expect("written SRT should re-parse");
        assert_eq!(reparsed, cues);

        // Malformed timestamp block names the offending block.
        let malformed = parse_srt("1\r\n00:00:01,000 --> 00:00:02,500\r\nFine.\r\n\r\n2\r\n00:00:zz --> 00:00:02,500\r\nBad.\r\n");
        assert!(malformed.is_err());
        assert!(malformed.unwrap_err().contains("block 2"));

        // End not after start is rejected.
        assert!(parse_srt("1\r\n00:00:02,000 --> 00:00:01,000\r\nBackwards.\r\n").is_err());
        assert!(parse_srt("1\r\n00:00:01,000 --> 00:00:01,000\r\nZero length.\r\n").is_err());
    }

    #[test]
    fn caption_lane_entries_and_lookup() {
        // Input intentionally out of chronological order; cue 2 is multi-line.
        let cues = vec![
            SubtitleCue {
                index: 2,
                start_seconds: 4.0,
                end_seconds: 6.0,
                text: "Second\tcue,\n keeps   interior \r newlines.".into(),
            },
            SubtitleCue {
                index: 1,
                start_seconds: 1.0,
                end_seconds: 2.5,
                text: "Hello there.".into(),
            },
            SubtitleCue {
                index: 3,
                start_seconds: 7.0,
                end_seconds: 8.0,
                text: " Final. ".into(),
            },
        ];
        let entries = captions_from_cues(&cues).expect("valid cues convert");
        assert_eq!(
            entries,
            vec![
                CaptionEntry {
                    start_seconds: 1.0,
                    end_seconds: 2.5,
                    text: "Hello there.".into(),
                },
                CaptionEntry {
                    start_seconds: 4.0,
                    end_seconds: 6.0,
                    text: "Second cue, keeps interior newlines.".into(),
                },
                CaptionEntry {
                    start_seconds: 7.0,
                    end_seconds: 8.0,
                    text: "Final.".into(),
                },
            ]
        );

        // Hits inside every caption, including the inclusive start boundary.
        assert_eq!(active_caption_at(&entries, 1.5), Some(&entries[0]));
        assert_eq!(active_caption_at(&entries, 4.0), Some(&entries[1]));
        assert_eq!(active_caption_at(&entries, 7.25), Some(&entries[2]));

        // Misses before all, exclusive ends, gaps, and after all.
        assert_eq!(active_caption_at(&entries, 0.5), None);
        assert_eq!(active_caption_at(&entries, 2.5), None);
        assert_eq!(active_caption_at(&entries, 3.0), None);
        assert_eq!(active_caption_at(&entries, 6.5), None);
        assert_eq!(active_caption_at(&entries, 9.0), None);

        // Empty lane has no active caption at any time.
        assert_eq!(active_caption_at(&[], 1.0), None);

        // Overlapping cues are rejected with an error naming the conflict.
        let overlapping = vec![
            SubtitleCue {
                index: 1,
                start_seconds: 1.0,
                end_seconds: 3.0,
                text: "A".into(),
            },
            SubtitleCue {
                index: 2,
                start_seconds: 2.5,
                end_seconds: 4.0,
                text: "B".into(),
            },
        ];
        let err = captions_from_cues(&overlapping).unwrap_err();
        assert!(
            err.contains("overlap") && err.contains("cue 2") && err.contains("cue 1"),
            "error must name the conflict: {err}"
        );

        // Zero and negative durations are rejected.
        let zero = vec![SubtitleCue {
            index: 1,
            start_seconds: 1.0,
            end_seconds: 1.0,
            text: "Zero".into(),
        }];
        let err = captions_from_cues(&zero).unwrap_err();
        assert!(err.contains("end must be after start"), "unexpected: {err}");
        let backwards = vec![SubtitleCue {
            index: 1,
            start_seconds: 2.0,
            end_seconds: 1.0,
            text: "Backwards".into(),
        }];
        assert!(captions_from_cues(&backwards).is_err());
    }

    #[test]
    fn motion_template_binding_validation() {
        let valid = MotionTemplateBinding {
            template_id: "loom.motion.lower-third".into(),
            schema_version: 2,
            parameters: vec![
                ("title".into(), "Interview".into()),
                ("accent_color".into(), "#3b82f6".into()),
            ],
            start_seconds: 4.5,
            duration_seconds: 6.0,
        };
        valid.validate().expect("valid binding must validate");

        // Parameter lookup hits and misses.
        assert_eq!(valid.parameter("title"), Some("Interview"));
        assert_eq!(valid.parameter("accent_color"), Some("#3b82f6"));
        assert_eq!(valid.parameter("missing"), None);
        assert_eq!(valid.parameter(""), None);

        // Schema migration review is required only on version mismatch.
        assert!(!valid.needs_migration(2));
        assert!(valid.needs_migration(3));
        assert!(valid.needs_migration(1));

        // Empty template id names its rule.
        let no_id = MotionTemplateBinding {
            template_id: String::new(),
            ..valid.clone()
        };
        let err = no_id.validate().unwrap_err();
        assert!(err.contains("template_id"), "unexpected error: {err}");

        // Zero and negative durations are rejected.
        for bad_duration in [0.0_f64, -1.5] {
            let bad = MotionTemplateBinding {
                duration_seconds: bad_duration,
                ..valid.clone()
            };
            let err = bad.validate().unwrap_err();
            assert!(
                err.contains("duration_seconds"),
                "duration {bad_duration} not rejected: {err}"
            );
        }

        // Negative start is rejected.
        let negative_start = MotionTemplateBinding {
            start_seconds: -0.25,
            ..valid.clone()
        };
        let err = negative_start.validate().unwrap_err();
        assert!(
            err.contains("start_seconds") && !err.contains("duration_seconds"),
            "unexpected error: {err}"
        );

        // Duplicate parameter names are rejected.
        let duplicated = MotionTemplateBinding {
            parameters: vec![
                ("title".into(), "First".into()),
                ("title".into(), "Second".into()),
            ],
            ..valid.clone()
        };
        let err = duplicated.validate().unwrap_err();
        assert!(err.contains("duplicate"), "unexpected error: {err}");
    }

    #[test]
    fn edl_export_record_structure() {
        // Three clips at 25 fps. Timecode arithmetic: 1 s = 25 frames.
        // Clip A: source offset 10 s, timeline 0..5 s
        //   src in 00:00:10:00, src out 00:00:15:00, rec in 00:00:00:00, rec out 00:00:05:00
        // Clip B ("interview b-roll!"): source offset 60 s, timeline 5..8 s
        //   reel truncates to 8 chars uppercased with '_' -> "INTERVIE"
        //   src in 00:01:00:00, src out 00:01:03:00, rec in 00:00:05:00, rec out 00:00:08:00
        let clips = vec![
            ("Clip A".to_string(), 10.0, 0.0, 5.0),
            ("interview b-roll!".to_string(), 60.0, 5.0, 3.0),
        ];

        let records = build_edl_records(&clips, 25.0).unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].event_number, 1);
        assert_eq!(records[0].reel, "CLIP_A");
        assert_eq!(records[0].source_in, "00:00:10:00");
        assert_eq!(records[0].source_out, "00:00:15:00");
        assert_eq!(records[0].record_in, "00:00:00:00");
        assert_eq!(records[0].record_out, "00:00:05:00");

        assert_eq!(records[1].event_number, 2);
        assert_eq!(records[1].reel, "INTERVIE");
        assert_eq!(records[1].source_in, "00:01:00:00");
        assert_eq!(records[1].source_out, "00:01:03:00");
        assert_eq!(records[1].record_in, "00:00:05:00");
        assert_eq!(records[1].record_out, "00:00:08:00");

        // Serialization carries event lines and clip-name comments in order
        let text = write_edl(&records);
        assert!(text.contains("001  CLIP_A"), "event line missing: {text}");
        assert!(text.contains("* FROM CLIP NAME: interview b-roll!\r\n"));
        assert!(text.ends_with("\r\n"));

        // Empty input serializes to an empty string
        assert_eq!(write_edl(&[]), "");

        // Invalid frame rates are rejected
        assert!(build_edl_records(&clips, 0.0).is_err());
        assert!(build_edl_records(&clips, -25.0).is_err());
        assert!(build_edl_records(&clips, f64::NAN).is_err());

        // Zero-length clips still produce valid ordered ranges
        let zero = build_edl_records(&[("z".to_string(), 4.0, 2.0, 0.0)], 25.0).unwrap();
        assert_eq!(zero[0].source_in, "00:00:04:00");
        assert_eq!(zero[0].source_out, "00:00:04:00");
    }

    #[test]
    fn apply_edit_checkpoints_only_after_successful_validation() {
        let mut session = VideoSession::new(VideoProject::new("video", "Edit"));
        let error = session
            .apply_edit(|project| project.split_clip(0, "missing", 1.0).map(|_| ()))
            .unwrap_err();
        assert!(matches!(error, TimelineError::ClipNotFound));
        assert!(!session.can_undo());

        let mut clip = Clip::new("clip", "Clip", 4.0);
        clip.source_path = "clip.mp4".into();
        session.project.tracks[0].insert_clip(clip).unwrap();
        session
            .apply_edit(|project| project.tracks[0].clips[0].trim_out(3.0))
            .unwrap();
        assert!(session.can_undo());
        assert_eq!(session.project.tracks[0].clips[0].duration, 3.0);
        assert!(session.undo());
        assert_eq!(session.project.tracks[0].clips[0].duration, 4.0);
    }

    #[test]
    fn gesture_edits_commit_once_or_restore_without_history() {
        let mut session = VideoSession::new(VideoProject::new("video", "Gesture"));
        session.project.tracks[0].add_clip(Clip::new("clip", "Clip", 4.0));
        let baseline = session.project.clone();

        session
            .apply_edit_without_history(|project| project.move_clip(0, 0, "clip", 1.0, false))
            .unwrap();
        session
            .apply_edit_without_history(|project| project.move_clip(0, 0, "clip", 2.0, false))
            .unwrap();
        assert!(
            !session.can_undo(),
            "gesture updates must not create history"
        );
        assert!(session.commit_gesture(baseline.clone()));
        assert!(session.can_undo());
        assert!(session.undo());
        assert_eq!(session.project, baseline);
        assert!(
            !session.can_undo(),
            "one gesture should produce one undo entry"
        );

        let cancel_baseline = session.project.clone();
        session
            .apply_edit_without_history(|project| project.move_clip(0, 0, "clip", 3.0, false))
            .unwrap();
        session.rollback_gesture(cancel_baseline.clone());
        assert_eq!(session.project, cancel_baseline);
        assert!(
            !session.can_undo(),
            "cancelled gestures must not create history"
        );
    }

    #[test]
    fn timeline_coordinate_conversion_is_zoom_stable() {
        assert_eq!(VideoProject::seconds_to_pixels(2.5, 100.0), 250.0);
        assert_eq!(VideoProject::pixels_to_seconds(250.0, 100.0), 2.5);
        assert_eq!(VideoProject::seconds_to_pixels(-1.0, 100.0), 0.0);
        assert_eq!(VideoProject::pixels_to_seconds(100.0, 0.0), 100.0);
    }

    #[test]
    fn export_cancellation_token_is_cooperative() {
        let token = ExportCancellation::default();
        assert!(!token.is_cancelled());
        token.cancel();
        assert!(token.is_cancelled());
        assert!(execute_timeline_export_with_cancel(
            &TimelineExportPlan {
                executable: "definitely-not-a-real-ffmpeg".into(),
                arguments: Vec::new(),
                output: "out.mp4".into(),
                duration: 1.0,
            },
            |_| {},
            &token,
        )
        .is_err());
    }

    #[cfg(unix)]
    #[test]
    fn cancelled_export_removes_partial_temp_and_preserves_destination() {
        use std::os::unix::fs::PermissionsExt;

        let dir = std::env::temp_dir().join(format!(
            "loom-video-export-cancel-{}-{}",
            std::process::id(),
            NEXT_EXPORT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("create export test directory");
        let script = dir.join("fake-ffmpeg.sh");
        std::fs::write(
            &script,
            "#!/bin/sh\nmarker=\"$1\"\noutput=\"$2\"\nprintf started > \"$marker\"\nprintf partial > \"$output\"\nwhile :; do sleep 0.01; done\n",
        )
        .expect("write fake ffmpeg");
        let mut permissions = std::fs::metadata(&script)
            .expect("stat fake ffmpeg")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&script, permissions).expect("make fake ffmpeg executable");

        let destination = dir.join("render.mp4");
        std::fs::write(&destination, b"existing destination").expect("seed destination");
        let marker = dir.join("started");
        let plan = TimelineExportPlan {
            executable: script,
            arguments: vec![marker.to_string_lossy().into_owned()],
            output: destination.clone(),
            duration: 1.0,
        };
        let cancel = ExportCancellation::default();
        let worker_cancel = cancel.clone();
        let worker = std::thread::spawn(move || {
            execute_timeline_export_with_cancel(&plan, |_| {}, &worker_cancel)
        });

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while !marker.is_file() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(marker.is_file(), "fake worker did not start");
        cancel.cancel();
        assert!(worker.join().expect("join export worker").is_err());
        assert_eq!(
            std::fs::read(&destination).expect("read existing destination"),
            b"existing destination"
        );
        let temporary_left = std::fs::read_dir(&dir)
            .expect("read export directory")
            .filter_map(Result::ok)
            .any(|entry| entry.file_name().to_string_lossy().contains(".loom-video-"));
        assert!(!temporary_left, "temporary export output was not removed");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn failed_export_removes_partial_temp_and_preserves_destination() {
        use std::os::unix::fs::PermissionsExt;

        let dir = std::env::temp_dir().join(format!(
            "loom-video-export-failure-{}-{}",
            std::process::id(),
            NEXT_EXPORT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("create export test directory");
        let script = dir.join("fake-ffmpeg.sh");
        std::fs::write(
            &script,
            "#!/bin/sh\nmarker=\"$1\"\noutput=\"$2\"\nprintf started > \"$marker\"\nprintf partial > \"$output\"\nprintf failed >&2\nexit 17\n",
        )
        .expect("write fake ffmpeg");
        let mut permissions = std::fs::metadata(&script)
            .expect("stat fake ffmpeg")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&script, permissions).expect("make fake ffmpeg executable");

        let destination = dir.join("render.mp4");
        std::fs::write(&destination, b"existing destination").expect("seed destination");
        let marker = dir.join("started");
        let plan = TimelineExportPlan {
            executable: script,
            arguments: vec![marker.to_string_lossy().into_owned()],
            output: destination.clone(),
            duration: 1.0,
        };
        let result =
            execute_timeline_export_with_cancel(&plan, |_| {}, &ExportCancellation::default());
        assert!(result.is_err(), "fake FFmpeg must fail");
        assert!(marker.is_file(), "fake worker did not start");
        assert_eq!(
            std::fs::read(&destination).expect("read existing destination"),
            b"existing destination"
        );
        let temporary_left = std::fs::read_dir(&dir)
            .expect("read export directory")
            .filter_map(Result::ok)
            .any(|entry| entry.file_name().to_string_lossy().contains(".loom-video-"));
        assert!(!temporary_left, "temporary export output was not removed");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
