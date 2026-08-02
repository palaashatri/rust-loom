//! Loom Video desktop application with local FFmpeg media workflows.

use std::path::{Path, PathBuf};
use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use loom_test_support::capture::{set_platform, snapshot_component};
use loom_video_core::{
    build_timeline_export_plan, decode_preview_frame, discover_media_tools,
    execute_timeline_export, load_video_project, probe_media, save_video_project,
    spawn_preview_player, Clip, MediaTools, TimelineMarker, VideoFrame, VideoProject, VideoSession,
};
use slint::{
    ComponentHandle, Image, ModelRc, PhysicalSize, Rgba8Pixel, SharedPixelBuffer, SharedString,
    VecModel,
};

slint::include_modules!();

const DEFAULT_SIZE: (u32, u32) = (1280, 800);
const SAVE_FILENAME: &str = "project.loomvideo";

struct Args {
    screenshot: Option<String>,
    smoke: bool,
    size: (u32, u32),
    theme: String,
    open: Option<String>,
}
fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        screenshot: None,
        smoke: false,
        size: DEFAULT_SIZE,
        theme: "dark".into(),
        open: None,
    };
    let mut iterator = std::env::args().skip(1);
    while let Some(argument) = iterator.next() {
        match argument.as_str() {
            "--screenshot" => {
                args.screenshot = Some(iterator.next().ok_or("--screenshot needs a path")?)
            }
            "--smoke" => args.smoke = true,
            "--size" => {
                let value = iterator.next().ok_or("--size needs WxH")?;
                let (width, height) = value.split_once('x').ok_or("--size must be WxH")?;
                args.size = (
                    width.parse().map_err(|_| "bad width")?,
                    height.parse().map_err(|_| "bad height")?,
                );
            }
            "--theme" => args.theme = iterator.next().ok_or("--theme needs a name")?,
            "--open" => args.open = Some(iterator.next().ok_or("--open needs a path")?),
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(args)
}

fn sample_project() -> VideoProject {
    let mut project = VideoProject::new("video-sample", "Documentary Assembly");
    let mut first = Clip::new("clip-1", "Opening Scene", 6.0);
    first.start_time = 0.0;
    let mut second = Clip::new("clip-2", "Interview Select", 10.5);
    second.start_time = 6.0;
    project.tracks[0].add_clip(first);
    project.tracks[0].add_clip(second);
    project
}

fn initial_session(args: &Args) -> Result<VideoSession, String> {
    match args.open.as_deref() {
        Some(path) => std::fs::read(path)
            .map_err(|error| format!("failed to read video project '{path}': {error}"))
            .and_then(|bytes| load_video_project(&bytes))
            .map(VideoSession::new),
        None => Ok(VideoSession::new(sample_project())),
    }
}

struct AppState {
    session: Mutex<VideoSession>,
    selected_clip: Mutex<usize>,
    preview: Mutex<Option<VideoFrame>>,
    tools: Option<MediaTools>,
    player: Mutex<Option<Child>>,
    exporting: AtomicBool,
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn timeline_clips(project: &VideoProject) -> Vec<&Clip> {
    project
        .tracks
        .iter()
        .find(|track| matches!(track.track_type, loom_video_core::TrackType::Video))
        .map(|track| track.clips.iter().collect())
        .unwrap_or_default()
}

fn timeline_duration(project: &VideoProject) -> f64 {
    timeline_clips(project)
        .iter()
        .map(|clip| clip.end_time())
        .fold(0.0, f64::max)
        .max(0.01)
}

fn procedural_preview() -> VideoFrame {
    let (width, height) = (640, 360);
    let mut pixels = vec![0u8; width * height * 4];
    for y in 0..height {
        for x in 0..width {
            let index = (y * width + x) * 4;
            let nx = x as f32 / width as f32;
            let ny = y as f32 / height as f32;
            pixels[index] = (18.0 + nx * 95.0) as u8;
            pixels[index + 1] = (28.0 + ny * 48.0) as u8;
            pixels[index + 2] = (46.0 + (1.0 - nx) * 68.0) as u8;
            pixels[index + 3] = 255;
        }
    }
    VideoFrame {
        width: width as u32,
        height: height as u32,
        pixels,
    }
}

fn frame_image(frame: &VideoFrame) -> Image {
    Image::from_rgba8(SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
        &frame.pixels,
        frame.width,
        frame.height,
    ))
}

fn timecode(seconds: f64, frame_rate: f64) -> String {
    let seconds = seconds.max(0.0);
    let hours = (seconds / 3600.0).floor() as u64;
    let minutes = ((seconds % 3600.0) / 60.0).floor() as u64;
    let whole = (seconds % 60.0).floor() as u64;
    let frames = ((seconds.fract() * frame_rate.max(1.0)).round() as u64)
        .min(frame_rate.max(1.0) as u64 - 1);
    format!("{hours:02}:{minutes:02}:{whole:02}:{frames:02}")
}

fn refresh(app: &VideoApp, state: &AppState) {
    let session = lock(&state.session);
    let project = &session.project;
    app.set_project_name(project.name.as_str().into());
    app.set_project_format(
        format!(
            "{} × {} · {:.3} fps",
            project.width, project.height, project.frame_rate
        )
        .into(),
    );
    app.set_track_labels(ModelRc::new(VecModel::from(
        project
            .tracks
            .iter()
            .map(|track| SharedString::from(format!("{} · {:?}", track.name, track.track_type)))
            .collect::<Vec<_>>(),
    )));
    app.set_track_mutes(ModelRc::new(VecModel::from(
        project
            .tracks
            .iter()
            .map(|track| track.muted)
            .collect::<Vec<_>>(),
    )));
    app.set_track_solos(ModelRc::new(VecModel::from(
        project
            .tracks
            .iter()
            .map(|track| track.solo)
            .collect::<Vec<_>>(),
    )));
    app.set_active_track_index(project.active_track_index as i32);
    let clips = timeline_clips(project);
    app.set_clip_labels(ModelRc::new(VecModel::from(
        clips
            .iter()
            .map(|clip| SharedString::from(clip.name.as_str()))
            .collect::<Vec<_>>(),
    )));
    app.set_clip_starts(ModelRc::new(VecModel::from(
        clips
            .iter()
            .map(|clip| clip.start_time as f32)
            .collect::<Vec<_>>(),
    )));
    app.set_clip_durations(ModelRc::new(VecModel::from(
        clips
            .iter()
            .map(|clip| clip.duration as f32)
            .collect::<Vec<_>>(),
    )));
    let selected = (*lock(&state.selected_clip)).min(clips.len().saturating_sub(1));
    *lock(&state.selected_clip) = selected;
    app.set_active_clip_index(selected as i32);
    app.set_timeline_duration(timeline_duration(project) as f32);
    app.set_can_undo(session.can_undo());
    app.set_can_redo(session.can_redo());
    let media = project
        .tracks
        .iter()
        .flat_map(|track| &track.clips)
        .filter(|clip| !clip.source_path.is_empty())
        .map(|clip| {
            SharedString::from(
                Path::new(&clip.source_path)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or(&clip.name),
            )
        })
        .collect::<Vec<_>>();
    app.set_media_bin_items(ModelRc::new(VecModel::from(media)));
    app.set_backend_available(state.tools.is_some());
    app.set_backend_version(
        state
            .tools
            .as_ref()
            .map(|tools| SharedString::from(tools.version.as_str()))
            .unwrap_or_else(|| "Install FFmpeg, FFprobe and FFplay on PATH".into()),
    );
    app.set_status_right(
        if state.tools.is_some() {
            "Local FFmpeg media"
        } else {
            "Media backend unavailable"
        }
        .into(),
    );
    app.set_status_left(
        format!(
            "{} tracks · {} clips · {} markers",
            project.tracks.len(),
            project.total_clips(),
            project.markers.len()
        )
        .into(),
    );
    if let Some(frame) = lock(&state.preview).as_ref() {
        app.set_preview_image(frame_image(frame));
        app.set_has_preview(true);
    }
}

fn apply_theme(app: &VideoApp, theme: &str) {
    Theme::get(app).set_active_theme(theme.into());
}
fn render_headless(args: &Args, output: &str) -> Result<(), String> {
    set_platform();
    let app = VideoApp::new().map_err(|error| error.to_string())?;
    apply_theme(&app, &args.theme);
    let state = AppState {
        session: Mutex::new(initial_session(args)?),
        selected_clip: Mutex::new(0),
        preview: Mutex::new(Some(procedural_preview())),
        tools: discover_media_tools().ok(),
        player: Mutex::new(None),
        exporting: AtomicBool::new(false),
    };
    refresh(&app, &state);
    let image = snapshot_component(&app, args.size.0 as f32, args.size.1 as f32, 1.0)
        .map_err(|error| error.to_string())?;
    loom_test_support::png::save_png(Path::new(output), &image).map_err(|error| error.to_string())
}

fn request_preview(state: Arc<AppState>, weak: slint::Weak<VideoApp>, timeline_time: f64) {
    let Some(tools) = state.tools.clone() else {
        return;
    };
    let source = {
        let session = lock(&state.session);
        timeline_clips(&session.project)
            .into_iter()
            .find(|clip| timeline_time >= clip.start_time && timeline_time <= clip.end_time())
            .map(|clip| {
                (
                    PathBuf::from(&clip.source_path),
                    clip.in_point + (timeline_time - clip.start_time) * clip.playback_rate,
                )
            })
    };
    let Some((path, source_time)) = source.filter(|(path, _)| path.is_file()) else {
        return;
    };
    std::thread::spawn(
        move || match decode_preview_frame(&tools, &path, source_time, 960, 540) {
            Ok(frame) => {
                *lock(&state.preview) = Some(frame.clone());
                let _ = weak.upgrade_in_event_loop(move |app| {
                    app.set_preview_image(frame_image(&frame));
                    app.set_has_preview(true);
                    app.set_status_left(format!("Decoded preview at {timeline_time:.2}s").into());
                });
            }
            Err(error) => {
                let _ = weak.upgrade_in_event_loop(move |app| {
                    app.set_status_left(format!("Preview decode failed: {error}").into())
                });
            }
        },
    );
}

fn main() -> Result<(), String> {
    let args = parse_args()?;
    if let Some(output) = &args.screenshot {
        return render_headless(&args, output);
    }
    if args.smoke {
        let output =
            std::env::temp_dir().join(format!("loom-video-smoke-{}.png", std::process::id()));
        return render_headless(&args, &output.to_string_lossy());
    }
    let app = VideoApp::new().map_err(|error| error.to_string())?;
    apply_theme(&app, &args.theme);
    app.window()
        .set_size(PhysicalSize::new(args.size.0, args.size.1));
    let state = Arc::new(AppState {
        session: Mutex::new(initial_session(&args)?),
        selected_clip: Mutex::new(0),
        preview: Mutex::new(Some(procedural_preview())),
        tools: discover_media_tools().ok(),
        player: Mutex::new(None),
        exporting: AtomicBool::new(false),
    });

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_new_project(move || {
            if let Some(app) = app_ref.upgrade() {
                *lock(&state.session) = VideoSession::new(sample_project());
                *lock(&state.selected_clip) = 0;
                *lock(&state.preview) = Some(procedural_preview());
                refresh(&app, &state);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_open_project(move || {
            if let Some(app) = app_ref.upgrade() {
                match std::fs::read(SAVE_FILENAME)
                    .map_err(|error| error.to_string())
                    .and_then(|bytes| load_video_project(&bytes))
                {
                    Ok(project) => {
                        *lock(&state.session) = VideoSession::new(project);
                        refresh(&app, &state);
                        app.set_status_left(format!("Opened {SAVE_FILENAME}").into());
                    }
                    Err(error) => app.set_status_left(format!("Open failed: {error}").into()),
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_save_project(move || {
            if let Some(app) = app_ref.upgrade() {
                let result = save_video_project(&lock(&state.session).project).and_then(|bytes| {
                    std::fs::write(SAVE_FILENAME, bytes).map_err(|error| error.to_string())
                });
                app.set_status_left(
                    match result {
                        Ok(()) => format!("Saved {SAVE_FILENAME}"),
                        Err(error) => format!("Save failed: {error}"),
                    }
                    .into(),
                );
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_undo(move || {
            if let Some(app) = app_ref.upgrade() {
                lock(&state.session).undo();
                refresh(&app, &state);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_redo(move || {
            if let Some(app) = app_ref.upgrade() {
                lock(&state.session).redo();
                refresh(&app, &state);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_import_media(move |path| {
            if let Some(app) = app_ref.upgrade() {
                let Some(tools) = state.tools.as_ref() else {
                    app.set_status_left("FFmpeg tools are unavailable".into());
                    return;
                };
                let path = PathBuf::from(path.trim());
                match probe_media(tools, &path) {
                    Ok(probe) => {
                        let mut session = lock(&state.session);
                        session.checkpoint();
                        let track_index = session
                            .project
                            .tracks
                            .iter()
                            .position(|track| {
                                matches!(track.track_type, loom_video_core::TrackType::Video)
                            })
                            .unwrap_or(0);
                        let start = session.project.tracks[track_index].duration();
                        let count = session.project.total_clips() + 1;
                        let mut clip = Clip::new(
                            format!("clip-{count}"),
                            path.file_name()
                                .and_then(|name| name.to_str())
                                .unwrap_or("Imported Clip"),
                            probe.duration.max(0.001),
                        );
                        clip.source_path = path.to_string_lossy().into_owned();
                        clip.start_time = start;
                        session.project.width = probe.width.max(1);
                        session.project.height = probe.height.max(1);
                        if probe.frame_rate > 0.0 {
                            session.project.frame_rate = probe.frame_rate;
                        }
                        session.project.tracks[track_index].insert_clip(clip).ok();
                        *lock(&state.selected_clip) = session.project.tracks[track_index]
                            .clips
                            .len()
                            .saturating_sub(1);
                        drop(session);
                        refresh(&app, &state);
                        request_preview(state.clone(), app.as_weak(), start);
                        app.set_status_left(
                            format!("Imported {} · {:.2}s", path.display(), probe.duration).into(),
                        );
                    }
                    Err(error) => app.set_status_left(format!("Import failed: {error}").into()),
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_select_track(move |index| {
            if let Some(app) = app_ref.upgrade() {
                if index >= 0 {
                    lock(&state.session).project.select_track(index as usize);
                    refresh(&app, &state);
                }
            }
        });
    }
    for solo in [false, true] {
        let state = state.clone();
        let app_ref = app.as_weak();
        if solo {
            app.on_toggle_track_solo(move |index| {
                if let Some(app) = app_ref.upgrade() {
                    if index >= 0 {
                        let mut session = lock(&state.session);
                        if (index as usize) < session.project.tracks.len() {
                            session.checkpoint();
                            session.project.tracks[index as usize].solo =
                                !session.project.tracks[index as usize].solo;
                        }
                        drop(session);
                        refresh(&app, &state);
                    }
                }
            });
        } else {
            app.on_toggle_track_mute(move |index| {
                if let Some(app) = app_ref.upgrade() {
                    if index >= 0 {
                        let mut session = lock(&state.session);
                        if (index as usize) < session.project.tracks.len() {
                            session.checkpoint();
                            session.project.tracks[index as usize].muted =
                                !session.project.tracks[index as usize].muted;
                        }
                        drop(session);
                        refresh(&app, &state);
                    }
                }
            });
        }
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_select_clip(move |index| {
            if let Some(app) = app_ref.upgrade() {
                if index >= 0 {
                    *lock(&state.selected_clip) = index as usize;
                    let time = {
                        let session = lock(&state.session);
                        timeline_clips(&session.project)
                            .get(index as usize)
                            .map(|clip| clip.start_time)
                            .unwrap_or(0.0)
                    };
                    app.set_playhead_seconds(time as f32);
                    app.set_timecode_display(
                        timecode(time, lock(&state.session).project.frame_rate).into(),
                    );
                    refresh(&app, &state);
                    request_preview(state.clone(), app.as_weak(), time);
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_remove_clip(move || {
            if let Some(app) = app_ref.upgrade() {
                let selected = *lock(&state.selected_clip);
                let mut session = lock(&state.session);
                let track_index = session.project.tracks.iter().position(|track| {
                    matches!(track.track_type, loom_video_core::TrackType::Video)
                });
                if let Some(track_index) = track_index {
                    if let Some(id) = session.project.tracks[track_index]
                        .clips
                        .get(selected)
                        .map(|clip| clip.id.clone())
                    {
                        session.checkpoint();
                        let _ = session.project.tracks[track_index].remove_clip(&id, true);
                        *lock(&state.selected_clip) = selected.saturating_sub(1);
                    }
                }
                drop(session);
                refresh(&app, &state);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_split_clip(move || {
            if let Some(app) = app_ref.upgrade() {
                let selected = *lock(&state.selected_clip);
                let playhead = app.get_playhead_seconds() as f64;
                let mut session = lock(&state.session);
                let track_index = session.project.tracks.iter().position(|track| {
                    matches!(track.track_type, loom_video_core::TrackType::Video)
                });
                if let Some(track_index) = track_index {
                    if let Some(id) = session.project.tracks[track_index]
                        .clips
                        .get(selected)
                        .map(|clip| clip.id.clone())
                    {
                        session.checkpoint();
                        match session.project.split_clip(track_index, &id, playhead) {
                            Ok(()) => app
                                .set_status_left(format!("Split clip at {:.2}s", playhead).into()),
                            Err(error) => {
                                app.set_status_left(format!("Split failed: {error}").into())
                            }
                        }
                    }
                }
                drop(session);
                refresh(&app, &state);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_trim_selected(move |in_delta, out_delta| {
            if let Some(app) = app_ref.upgrade() {
                let selected = *lock(&state.selected_clip);
                let mut session = lock(&state.session);
                let track_index = session.project.tracks.iter().position(|track| {
                    matches!(track.track_type, loom_video_core::TrackType::Video)
                });
                if let Some(track_index) = track_index {
                    if selected < session.project.tracks[track_index].clips.len() {
                        session.checkpoint();
                        let clip = &mut session.project.tracks[track_index].clips[selected];
                        let result = if in_delta != 0.0 {
                            clip.trim_in((clip.in_point + in_delta as f64).max(0.0))
                        } else {
                            clip.trim_out(
                                (clip.out_point + out_delta as f64).max(clip.in_point + 0.01),
                            )
                        };
                        if let Err(error) = result {
                            app.set_status_left(format!("Trim failed: {error}").into());
                        }
                        session.project.tracks[track_index].sort_clips();
                    }
                }
                drop(session);
                refresh(&app, &state);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        app.on_select_nle_tool(move |tool| {
            if let Some(app) = app_ref.upgrade() {
                app.set_status_left(format!("{} tool active", tool.as_str()).into());
            }
        });
    }
    {
        let app_ref = app.as_weak();
        app.on_toggle_snap(move || {
            if let Some(app) = app_ref.upgrade() {
                app.set_status_left(
                    format!(
                        "Timeline snapping {}",
                        if app.get_snap_enabled() { "on" } else { "off" }
                    )
                    .into(),
                );
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_seek(move |seconds| {
            if let Some(app) = app_ref.upgrade() {
                let seconds = seconds as f64;
                app.set_timecode_display(
                    timecode(seconds, lock(&state.session).project.frame_rate).into(),
                );
                request_preview(state.clone(), app.as_weak(), seconds);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_play_pause(move || {
            if let Some(app) = app_ref.upgrade() {
                if app.get_is_playing() {
                    if let Some(mut child) = lock(&state.player).take() {
                        let _ = child.kill();
                    }
                    app.set_is_playing(false);
                    return;
                }
                let selected = *lock(&state.selected_clip);
                let source = {
                    let session = lock(&state.session);
                    timeline_clips(&session.project)
                        .get(selected)
                        .map(|clip| (clip.source_path.clone(), clip.in_point, clip.source_span()))
                };
                let Some((path, start, duration)) =
                    source.filter(|(path, _, _)| Path::new(path).is_file())
                else {
                    app.set_status_left("Selected clip has no readable local source".into());
                    return;
                };
                let Some(tools) = state.tools.as_ref() else {
                    return;
                };
                match spawn_preview_player(tools, Path::new(&path), start, Some(duration)) {
                    Ok(child) => {
                        *lock(&state.player) = Some(child);
                        app.set_is_playing(true);
                        app.set_status_left("Playing selected source range in local FFplay".into());
                    }
                    Err(error) => app.set_status_left(format!("Playback failed: {error}").into()),
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_stop_playback(move || {
            if let Some(app) = app_ref.upgrade() {
                if let Some(mut child) = lock(&state.player).take() {
                    let _ = child.kill();
                }
                app.set_is_playing(false);
                app.set_status_left("Playback stopped".into());
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_add_marker(move || {
            if let Some(app) = app_ref.upgrade() {
                let time = app.get_playhead_seconds() as f64;
                let mut session = lock(&state.session);
                session.checkpoint();
                let count = session.project.markers.len() + 1;
                let _ = session.project.add_marker(TimelineMarker {
                    id: format!("marker-{count}"),
                    time,
                    label: format!("Marker {count}"),
                    color: "#d97745".into(),
                });
                drop(session);
                refresh(&app, &state);
            }
        });
    }
    {
        let state = state.clone();
        let weak = app.as_weak();
        app.on_export_timeline(move |path| {
            if state.exporting.swap(true, Ordering::SeqCst) {
                return;
            }
            let Some(tools) = state.tools.clone() else {
                state.exporting.store(false, Ordering::SeqCst);
                if let Some(app) = weak.upgrade() {
                    app.set_status_left("FFmpeg/FFprobe are unavailable".into());
                }
                return;
            };
            let mut output = PathBuf::from(path.trim());
            if output.as_os_str().is_empty() {
                output = "loom-video-export.mp4".into();
            }
            if output.extension().is_none() {
                output.set_extension("mp4");
            }
            let project = lock(&state.session).project.clone();
            let worker_state = state.clone();
            let worker_weak = weak.clone();
            std::thread::spawn(move || {
                let plan = build_timeline_export_plan(&project, &tools, &output);
                let result = match plan {
                    Ok(plan) => execute_timeline_export(&plan, {
                        let progress_weak = worker_weak.clone();
                        move |progress| {
                            let _ = progress_weak.upgrade_in_event_loop(move |app| {
                                app.set_export_progress(progress * 100.0);
                                app.set_status_left(
                                    format!("Rendering timeline · {:.0}%", progress * 100.0).into(),
                                );
                            });
                        }
                    }),
                    Err(error) => Err(error),
                };
                worker_state.exporting.store(false, Ordering::SeqCst);
                let _ = worker_weak.upgrade_in_event_loop(move |app| {
                    app.set_exporting(false);
                    app.set_status_left(
                        match result {
                            Ok(()) => format!("Exported {}", output.display()),
                            Err(error) => format!("Export failed: {error}"),
                        }
                        .into(),
                    );
                });
            });
            if let Some(app) = weak.upgrade() {
                app.set_exporting(true);
                app.set_export_progress(0.0);
            }
        });
    }

    refresh(&app, &state);
    app.show().map_err(|error| error.to_string())?;
    slint::run_event_loop().map_err(|error| error.to_string())
}
