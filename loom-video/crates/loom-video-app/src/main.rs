//! Loom Video desktop application with local FFmpeg media workflows.

use std::path::{Path, PathBuf};
use std::process::Child;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use loom_desktop::{
    build_standard_menu_bar, FileDialogService, FileFilter, Menu, MenuBarService, MenuItem,
    MenuShortcut, NativeFileDialogs, NativeMenuBar, OpenFileRequest, SaveFileRequest,
};
use loom_test_support::capture::{set_platform, snapshot_component};
use loom_test_support::journey::{record_keyboard_palette_journey, PaletteProbe};
use loom_video_core::{
    build_timeline_export_plan, decode_preview_frame, discover_media_tools,
    execute_timeline_export, load_video_project, probe_media, save_video_project,
    spawn_preview_player, Clip, MediaTools, TimelineMarker, VideoFrame, VideoProject, VideoSession,
};
use slint::{
    ComponentHandle, Image, Model, ModelRc, PhysicalSize, Rgba8Pixel, SharedPixelBuffer,
    SharedString, VecModel,
};

slint::include_modules!();

const DEFAULT_SIZE: (u32, u32) = (1280, 800);
const COMPACT_LAYOUT_MAX_WIDTH: u32 = 1200;

loom_production::define_snapshot_recovery!(VIDEO_RECOVERY, "org.loom.video", "loom.video/1");

struct Args {
    screenshot: Option<String>,
    smoke: bool,
    palette: bool,
    journey: Option<String>,
    size: (u32, u32),
    theme: String,
    open: Option<String>,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        screenshot: None,
        smoke: false,
        palette: false,
        journey: None,
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
            "--palette" => args.palette = true,
            "--journey" => {
                args.journey = Some(
                    iterator
                        .next()
                        .ok_or("--journey needs an output directory")?,
                );
            }
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
            other if !other.starts_with('-') && args.open.is_none() => {
                args.open = Some(other.to_string());
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(args)
}

fn empty_project() -> VideoProject {
    VideoProject::new("untitled-project", "Untitled Project")
}

fn compact_layout_for_width(width: u32) -> bool {
    width < COMPACT_LAYOUT_MAX_WIDTH
}

fn configure_responsive_layout(app: &VideoApp, width: u32) {
    app.set_compact_layout(compact_layout_for_width(width));
}

fn wire_responsive_layout(app: &VideoApp) {
    let app_ref = app.as_weak();
    app.on_window_resized(move |width| {
        if let Some(app) = app_ref.upgrade() {
            configure_responsive_layout(&app, width.max(0.0) as u32);
        }
    });
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

fn initial_session(args: &Args) -> Result<(VideoSession, Option<PathBuf>), String> {
    match args.open.as_deref() {
        Some(path) => {
            let p = PathBuf::from(path);
            let bytes = std::fs::read(&p)
                .map_err(|error| format!("failed to read video project '{path}': {error}"))?;
            let project = load_video_project(&bytes)?;
            Ok((VideoSession::new(project), Some(p)))
        }
        None => Ok((VideoSession::new(sample_project()), None)),
    }
}

struct AppState {
    session: Mutex<VideoSession>,
    save_path: Mutex<Option<PathBuf>>,
    dialogs: Arc<dyn FileDialogService>,
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

fn video_filter() -> FileFilter {
    FileFilter {
        name: "Loom Video Project (*.loomvideo)".into(),
        extensions: vec!["loomvideo".into()],
    }
}

fn media_filter() -> FileFilter {
    FileFilter {
        name: "Supported Media (*.mp4, *.mov, *.mkv, *.avi, *.wav, *.mp3)".into(),
        extensions: vec![
            "mp4".into(),
            "mov".into(),
            "mkv".into(),
            "avi".into(),
            "wav".into(),
            "mp3".into(),
        ],
    }
}

fn open_video_request(save_path: Option<&Path>) -> OpenFileRequest {
    OpenFileRequest {
        title: "Open Video Project".into(),
        initial_directory: save_path.and_then(Path::parent).map(Path::to_path_buf),
        suggested_name: None,
        filters: vec![video_filter()],
    }
}

fn save_video_request(save_path: Option<&Path>) -> SaveFileRequest {
    SaveFileRequest {
        title: "Save Video Project".into(),
        initial_directory: save_path.and_then(Path::parent).map(Path::to_path_buf),
        suggested_name: save_path
            .and_then(Path::file_name)
            .map(|n| n.to_string_lossy().into_owned())
            .or_else(|| Some("Untitled.loomvideo".into())),
        filters: vec![video_filter()],
    }
}

fn open_media_request(save_path: Option<&Path>) -> OpenFileRequest {
    OpenFileRequest {
        title: "Import Media".into(),
        initial_directory: save_path.and_then(Path::parent).map(Path::to_path_buf),
        suggested_name: None,
        filters: vec![media_filter()],
    }
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
    let path_label = lock(&state.save_path)
        .as_ref()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Untitled".into());
    app.set_status_left(
        format!(
            "{path_label} · {} tracks · {} clips · {} markers",
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
    if let Ok(bytes) = save_video_project(project) {
        let _ = record_snapshot_recovery("video state", bytes);
    }
}

fn apply_theme(app: &VideoApp, theme: &str) {
    Theme::get(app).set_active_theme(theme.into());
}

fn render_headless(args: &Args, output: &str) -> Result<(), String> {
    set_platform();
    let app = VideoApp::new().map_err(|error| error.to_string())?;
    configure_responsive_layout(&app, args.size.0);
    apply_theme(&app, &args.theme);
    let (initial_proj, initial_path) = initial_session(args)?;
    let state = AppState {
        session: Mutex::new(initial_proj),
        save_path: Mutex::new(initial_path),
        dialogs: Arc::new(NativeFileDialogs),
        selected_clip: Mutex::new(0),
        preview: Mutex::new(Some(procedural_preview())),
        tools: discover_media_tools().ok(),
        player: Mutex::new(None),
        exporting: AtomicBool::new(false),
    };
    refresh(&app, &state);
    if args.palette {
        app.set_palette_query(SharedString::from("pr"));
        rebuild_palette(&app, "pr");
        app.set_palette_selected(1);
        app.set_palette_open(true);
    }
    let image = snapshot_component(&app, args.size.0 as f32, args.size.1 as f32, 1.0)
        .map_err(|error| error.to_string())?;
    loom_test_support::png::save_png(Path::new(output), &image).map_err(|error| error.to_string())
}

/// Record the keyboard command-palette journey with per-step screenshots.
fn run_journey(args: &Args, out_dir: &str) -> Result<(), String> {
    set_platform();
    let app = VideoApp::new().map_err(|error| error.to_string())?;
    configure_responsive_layout(&app, args.size.0);
    apply_theme(&app, &args.theme);
    let (initial_proj, initial_path) = initial_session(args)?;
    let state = AppState {
        session: Mutex::new(initial_proj),
        save_path: Mutex::new(initial_path),
        dialogs: Arc::new(NativeFileDialogs),
        selected_clip: Mutex::new(0),
        preview: Mutex::new(Some(procedural_preview())),
        tools: discover_media_tools().ok(),
        player: Mutex::new(None),
        exporting: AtomicBool::new(false),
    };
    refresh(&app, &state);
    let menu_bar = build_standard_menu_bar(
        "Loom Video",
        vec![MenuItem::action_with_shortcut(
            "file.export_video",
            "Export Timeline...",
            MenuShortcut::primary("E"),
        )],
        vec![],
        vec![MenuItem::check(
            "view.inspector",
            "Inspector",
            true,
        )],
        vec![Menu::new(
            "Clip",
            vec![
                MenuItem::action_with_shortcut(
                    "clip.split",
                    "Split Clip at Playhead",
                    MenuShortcut::primary("B"),
                ),
                MenuItem::action("clip.delete", "Ripple Delete Selected Clip"),
            ],
        )],
    );
    let menu_service = NativeMenuBar::new();
    let _ = menu_service.install_menu_bar(&menu_bar);

    wire_palette(&app);
    rebuild_palette(&app, "");
    app.window()
        .set_size(PhysicalSize::new(args.size.0, args.size.1));
    let report = record_keyboard_palette_journey(&app, "video", Path::new(out_dir), "clip")
        .map_err(|error| format!("journey failed: {error}"))?;
    println!(
        "keyboard journey: {} ({})",
        if report.passed { "PASS" } else { "FAIL" },
        out_dir
    );
    if !report.passed {
        return Err("keyboard journey invariants failed".to_string());
    }
    Ok(())
}

impl PaletteProbe for VideoApp {
    fn palette_open(&self) -> bool {
        self.get_palette_open()
    }

    fn palette_commands(&self) -> usize {
        self.get_palette_commands().row_count()
    }

    fn palette_selected(&self) -> i32 {
        self.get_palette_selected()
    }

    fn palette_query(&self) -> String {
        self.get_palette_query().to_string()
    }

    fn open_palette(&self) {
        self.invoke_open_palette();
    }
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

fn wire_application(app: &VideoApp, state: Arc<AppState>) {
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_new_project(move || {
            if let Some(app) = app_ref.upgrade() {
                *lock(&state.session) = VideoSession::new(empty_project());
                *lock(&state.save_path) = None;
                *lock(&state.selected_clip) = 0;
                *lock(&state.preview) = Some(procedural_preview());
                refresh(&app, &state);
                app.set_status_left("New untitled project created".into());
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_open_project(move || {
            if let Some(app) = app_ref.upgrade() {
                let current_path = lock(&state.save_path).clone();
                let request = open_video_request(current_path.as_deref());
                match state.dialogs.open_file(&request) {
                    Ok(Some(path)) => match std::fs::read(&path)
                        .map_err(|error| error.to_string())
                        .and_then(|bytes| load_video_project(&bytes))
                    {
                        Ok(project) => {
                            *lock(&state.session) = VideoSession::new(project);
                            *lock(&state.save_path) = Some(path.clone());
                            refresh(&app, &state);
                            app.set_status_left(
                                format!("Opened {}", path.file_name().unwrap().to_string_lossy())
                                    .into(),
                            );
                        }
                        Err(error) => app.set_status_left(format!("Open failed: {error}").into()),
                    },
                    Ok(None) => {
                        app.set_status_left("Open cancelled".into());
                    }
                    Err(error) => {
                        app.set_status_left(format!("Open dialog failed: {error}").into());
                    }
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_save_project(move || {
            if let Some(app) = app_ref.upgrade() {
                let target_path = lock(&state.save_path).clone();
                let path_to_save = match target_path {
                    Some(p) => Some(p),
                    None => {
                        let req = save_video_request(None);
                        match state.dialogs.save_file(&req) {
                            Ok(Some(p)) => Some(p),
                            Ok(None) => {
                                app.set_status_left("Save cancelled".into());
                                return;
                            }
                            Err(error) => {
                                app.set_status_left(format!("Save dialog failed: {error}").into());
                                return;
                            }
                        }
                    }
                };

                if let Some(path) = path_to_save {
                    let result =
                        save_video_project(&lock(&state.session).project).and_then(|bytes| {
                            loom_storage::atomic_write(&path, &bytes)
                                .map_err(|error| error.to_string())
                                .and_then(|_| checkpoint_snapshot_recovery(bytes))
                        });
                    match result {
                        Ok(()) => {
                            *lock(&state.save_path) = Some(path.clone());
                            refresh(&app, &state);
                            app.set_status_left(
                                format!("Saved {}", path.file_name().unwrap().to_string_lossy())
                                    .into(),
                            );
                        }
                        Err(error) => {
                            app.set_status_left(format!("Save failed: {error}").into());
                        }
                    }
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_save_as_project(move || {
            if let Some(app) = app_ref.upgrade() {
                let current_path = lock(&state.save_path).clone();
                let req = save_video_request(current_path.as_deref());
                match state.dialogs.save_file(&req) {
                    Ok(Some(path)) => {
                        let result =
                            save_video_project(&lock(&state.session).project).and_then(|bytes| {
                                loom_storage::atomic_write(&path, &bytes)
                                    .map_err(|error| error.to_string())
                                    .and_then(|_| checkpoint_snapshot_recovery(bytes))
                            });
                        match result {
                            Ok(()) => {
                                *lock(&state.save_path) = Some(path.clone());
                                refresh(&app, &state);
                                app.set_status_left(
                                    format!(
                                        "Saved As {}",
                                        path.file_name().unwrap().to_string_lossy()
                                    )
                                    .into(),
                                );
                            }
                            Err(error) => {
                                app.set_status_left(format!("Save As failed: {error}").into());
                            }
                        }
                    }
                    Ok(None) => {
                        app.set_status_left("Save As cancelled".into());
                    }
                    Err(error) => {
                        app.set_status_left(format!("Save As dialog failed: {error}").into());
                    }
                }
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
                let chosen_path = if path.trim().is_empty() {
                    let current_dir = lock(&state.save_path).clone();
                    let req = open_media_request(current_dir.as_deref());
                    match state.dialogs.open_file(&req) {
                        Ok(Some(p)) => p,
                        Ok(None) => {
                            app.set_status_left("Import cancelled".into());
                            return;
                        }
                        Err(e) => {
                            app.set_status_left(format!("Import dialog failed: {e}").into());
                            return;
                        }
                    }
                } else {
                    PathBuf::from(path.trim())
                };

                match probe_media(tools, &chosen_path) {
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
                            chosen_path
                                .file_name()
                                .and_then(|name| name.to_str())
                                .unwrap_or("Imported Clip"),
                            probe.duration.max(0.001),
                        );
                        clip.source_path = chosen_path.to_string_lossy().into_owned();
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
                        app.set_status_left(
                            format!(
                                "Imported {}",
                                chosen_path.file_name().unwrap().to_string_lossy()
                            )
                            .into(),
                        );
                    }
                    Err(error) => app.set_status_left(format!("Probe failed: {error}").into()),
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
                    let mut session = lock(&state.session);
                    if (index as usize) < session.project.tracks.len() {
                        session.project.active_track_index = index as usize;
                    }
                    drop(session);
                    refresh(&app, &state);
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_toggle_track_mute(move |index| {
            if let Some(app) = app_ref.upgrade() {
                if index >= 0 {
                    let mut session = lock(&state.session);
                    session.checkpoint();
                    if let Some(track) = session.project.tracks.get_mut(index as usize) {
                        track.muted = !track.muted;
                    }
                    drop(session);
                    refresh(&app, &state);
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_toggle_track_solo(move |index| {
            if let Some(app) = app_ref.upgrade() {
                if index >= 0 {
                    let mut session = lock(&state.session);
                    session.checkpoint();
                    if let Some(track) = session.project.tracks.get_mut(index as usize) {
                        track.solo = !track.solo;
                    }
                    drop(session);
                    refresh(&app, &state);
                }
            }
        });
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
                            Ok((_left_id, _right_id)) => app
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
    if let Some(out_dir) = &args.journey {
        return run_journey(&args, out_dir);
    }
    let app = VideoApp::new().map_err(|error| error.to_string())?;
    configure_responsive_layout(&app, args.size.0);
    apply_theme(&app, &args.theme);
    app.window()
        .set_size(PhysicalSize::new(args.size.0, args.size.1));
    wire_responsive_layout(&app);
    let recovered = initialize_snapshot_recovery()?;
    let (initial_proj, initial_path) = if args.open.is_some() {
        initial_session(&args)?
    } else {
        match recovered
            .as_deref()
            .and_then(|bytes| load_video_project(bytes).ok())
        {
            Some(p) => (VideoSession::new(p), None),
            None => initial_session(&args)?,
        }
    };
    let state = Arc::new(AppState {
        session: Mutex::new(initial_proj),
        save_path: Mutex::new(initial_path),
        dialogs: Arc::new(NativeFileDialogs),
        selected_clip: Mutex::new(0),
        preview: Mutex::new(Some(procedural_preview())),
        tools: discover_media_tools().ok(),
        player: Mutex::new(None),
        exporting: AtomicBool::new(false),
    });

    wire_application(&app, state.clone());
    let menu_bar = build_standard_menu_bar(
        "Loom Video",
        vec![MenuItem::action_with_shortcut(
            "file.export_video",
            "Export Timeline...",
            MenuShortcut::primary("E"),
        )],
        vec![],
        vec![MenuItem::check(
            "view.inspector",
            "Inspector",
            true,
        )],
        vec![Menu::new(
            "Clip",
            vec![
                MenuItem::action_with_shortcut(
                    "clip.split",
                    "Split Clip at Playhead",
                    MenuShortcut::primary("B"),
                ),
                MenuItem::action("clip.delete", "Ripple Delete Selected Clip"),
            ],
        )],
    );
    let menu_service = NativeMenuBar::new();
    let _ = menu_service.install_menu_bar(&menu_bar);

    wire_palette(&app);
    refresh(&app, &state);
    app.show().map_err(|error| error.to_string())?;
    slint::run_event_loop().map_err(|error| error.to_string())
}

/// Commands exposed through the command palette.
#[derive(Debug, Clone)]
enum PaletteAction {
    NewProject,
    OpenProject,
    SaveProject,
    SaveAsProject,
    Undo,
    Redo,
    ImportMedia,
    SplitClip,
    RemoveClip,
    PlayPause,
    Stop,
    SelectClip(i32),
    Export,
}

struct PaletteCommand {
    action: PaletteAction,
    id: &'static str,
    label: &'static str,
    shortcut: &'static str,
}

const PALETTE_IMPORT_SOURCE: &str = "";

fn master_palette(app: &VideoApp) -> Vec<PaletteCommand> {
    vec![
        PaletteCommand {
            action: PaletteAction::NewProject,
            id: "video.new",
            label: "New Project",
            shortcut: "Ctrl+N",
        },
        PaletteCommand {
            action: PaletteAction::OpenProject,
            id: "video.open",
            label: "Open Project",
            shortcut: "Ctrl+O",
        },
        PaletteCommand {
            action: PaletteAction::SaveProject,
            id: "video.save",
            label: "Save Project",
            shortcut: "Ctrl+S",
        },
        PaletteCommand {
            action: PaletteAction::SaveAsProject,
            id: "video.save-as",
            label: "Save Project As",
            shortcut: "Ctrl+Shift+S",
        },
        PaletteCommand {
            action: PaletteAction::Undo,
            id: "video.undo",
            label: "Undo",
            shortcut: "Ctrl+Z",
        },
        PaletteCommand {
            action: PaletteAction::Redo,
            id: "video.redo",
            label: "Redo",
            shortcut: "Ctrl+Shift+Z",
        },
        PaletteCommand {
            action: PaletteAction::ImportMedia,
            id: "video.import",
            label: "Import Media",
            shortcut: "Ctrl+I",
        },
        PaletteCommand {
            action: PaletteAction::SplitClip,
            id: "video.split",
            label: "Split Clip",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::RemoveClip,
            id: "video.remove-clip",
            label: "Remove Clip",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::PlayPause,
            id: "video.play-pause",
            label: "Play / Pause",
            shortcut: "Space",
        },
        PaletteCommand {
            action: PaletteAction::Stop,
            id: "video.stop",
            label: "Stop Playback",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::SelectClip(0),
            id: "video.select-clip",
            label: "Select Clip 1",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::Export,
            id: "video.export",
            label: "Export Timeline",
            shortcut: "Ctrl+E",
        },
    ]
    .into_iter()
    .filter(|c| match c.action {
        PaletteAction::Undo => app.get_can_undo(),
        PaletteAction::Redo => app.get_can_redo(),
        _ => true,
    })
    .collect()
}

fn rebuild_palette(app: &VideoApp, query: &str) {
    let query_lower = query.trim().to_lowercase();
    let items: Vec<CommandPaletteItem> = master_palette(app)
        .into_iter()
        .filter(|c| {
            query_lower.is_empty()
                || c.label.to_lowercase().contains(&query_lower)
                || c.id.to_lowercase().contains(&query_lower)
        })
        .map(|c| CommandPaletteItem {
            id: c.id.into(),
            label: c.label.into(),
            shortcut: c.shortcut.into(),
            enabled: true,
        })
        .collect();
    app.set_palette_commands(Rc::new(VecModel::from(items)).into());
    let count = app.get_palette_commands().row_count() as i32;
    let selected = app.get_palette_selected();
    if selected >= count && count > 0 {
        app.set_palette_selected(count - 1);
    } else if count == 0 {
        app.set_palette_selected(0);
    }
}

fn wire_palette(app: &VideoApp) {
    {
        let app_ref = app.as_weak();
        app.on_palette_query_changed(move |query| {
            if let Some(app) = app_ref.upgrade() {
                rebuild_palette(&app, query.as_str());
                app.set_palette_selected(0);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        app.on_palette_move(move |delta| {
            if let Some(app) = app_ref.upgrade() {
                let count = app.get_palette_commands().row_count() as i32;
                if count == 0 {
                    return;
                }
                let next = (app.get_palette_selected() + delta).clamp(0, count - 1);
                app.set_palette_selected(next);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        app.on_palette_key_text(move |text| {
            if let Some(app) = app_ref.upgrade() {
                let mut query = app.get_palette_query().to_string();
                query.push_str(text.as_str());
                let query = SharedString::from(query.as_str());
                app.set_palette_query(query.clone());
                rebuild_palette(&app, query.as_str());
                app.set_palette_selected(0);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        app.on_palette_backspace(move || {
            if let Some(app) = app_ref.upgrade() {
                let mut query = app.get_palette_query().to_string();
                query.pop();
                let query = SharedString::from(query.as_str());
                app.set_palette_query(query.clone());
                rebuild_palette(&app, query.as_str());
                app.set_palette_selected(0);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        app.on_palette_close(move || {
            if let Some(app) = app_ref.upgrade() {
                app.set_palette_open(false);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        app.on_palette_invoked(move |index| {
            if let Some(app) = app_ref.upgrade() {
                let command = master_palette(&app)
                    .into_iter()
                    .filter(|c| match c.action {
                        PaletteAction::Undo => app.get_can_undo(),
                        PaletteAction::Redo => app.get_can_redo(),
                        _ => true,
                    })
                    .filter(|c| {
                        let q = app.get_palette_query().trim().to_lowercase();
                        q.is_empty()
                            || c.label.to_lowercase().contains(&q)
                            || c.id.to_lowercase().contains(&q)
                    })
                    .nth(index as usize);
                if let Some(command) = command {
                    app.set_palette_open(false);
                    match command.action {
                        PaletteAction::NewProject => app.invoke_new_project(),
                        PaletteAction::OpenProject => app.invoke_open_project(),
                        PaletteAction::SaveProject => app.invoke_save_project(),
                        PaletteAction::SaveAsProject => app.invoke_save_as_project(),
                        PaletteAction::Undo => app.invoke_undo(),
                        PaletteAction::Redo => app.invoke_redo(),
                        PaletteAction::ImportMedia => {
                            app.invoke_import_media(PALETTE_IMPORT_SOURCE.into())
                        }
                        PaletteAction::SplitClip => app.invoke_split_clip(),
                        PaletteAction::RemoveClip => app.invoke_remove_clip(),
                        PaletteAction::PlayPause => app.invoke_play_pause(),
                        PaletteAction::Stop => app.invoke_stop_playback(),
                        PaletteAction::SelectClip(index) => app.invoke_select_clip(index),
                        PaletteAction::Export => app.invoke_export_timeline(app.get_export_path()),
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loom_desktop::ScriptedFileDialogs;

    fn test_app_and_state(scripted: ScriptedFileDialogs) -> (VideoApp, Arc<AppState>) {
        set_platform();
        let app = VideoApp::new().expect("create VideoApp");
        let state = Arc::new(AppState {
            session: Mutex::new(VideoSession::new(sample_project())),
            save_path: Mutex::new(None),
            dialogs: Arc::new(scripted),
            selected_clip: Mutex::new(0),
            preview: Mutex::new(Some(procedural_preview())),
            tools: None,
            player: Mutex::new(None),
            exporting: AtomicBool::new(false),
        });
        wire_application(&app, state.clone());
        refresh(&app, &state);
        (app, state)
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
    fn compact_layout_boundary_keeps_reference_width_stable() {
        assert!(compact_layout_for_width(1024));
        assert!(compact_layout_for_width(1199));
        assert!(!compact_layout_for_width(1200));
        assert!(!compact_layout_for_width(1440));
    }
}
