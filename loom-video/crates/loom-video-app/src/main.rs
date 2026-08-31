//! Loom Video desktop application with local FFmpeg media workflows.

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use std::{process::Command, thread};

use loom_desktop::{
    build_standard_menu_bar, FileDialogService, FileFilter, Menu, MenuBarService, MenuItem,
    MenuShortcut, NativeFileDialogs, NativeMenuBar, OpenFileRequest, SaveFileRequest,
    ScriptedFileDialogs,
};
use loom_test_support::capture::{set_platform, snapshot_component};
use loom_test_support::journey::PaletteProbe;
use loom_video_core::{
    build_timeline_export_plan, decode_preview_frame, discover_media_tools,
    execute_timeline_export_with_cancel, load_video_project, probe_media, save_video_project, Clip,
    ExportCancellation, MediaTools, TimelineMarker, VideoFrame, VideoProject, VideoSession,
};
use slint::{
    ComponentHandle, Image, Model, ModelRc, PhysicalSize, Rgba8Pixel, SharedPixelBuffer,
    SharedString, VecModel,
};

slint::include_modules!();

const DEFAULT_SIZE: (u32, u32) = (1280, 800);

loom_production::define_snapshot_recovery!(VIDEO_RECOVERY, "org.loom.video", "loom.video/1");

struct Args {
    screenshot: Option<String>,
    smoke: bool,
    palette: bool,
    journey: Option<String>,
    size: (u32, u32),
    theme: String,
    rtl: bool,
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
        rtl: false,
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
            "--rtl" => args.rtl = true,
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

fn compact_layout_for_width(app: &VideoApp, width: u32) -> bool {
    let policy = ResponsivePolicy::get(app);
    compact_layout_for_breakpoint(width, policy.get_priority_1_icon_only_below())
}

fn compact_layout_for_breakpoint(width: u32, breakpoint: f32) -> bool {
    (width as f32) < breakpoint
}

fn configure_responsive_layout(app: &VideoApp, width: u32) {
    app.set_compact_layout(compact_layout_for_width(app, width));
}

fn configure_direction(app: &VideoApp, rtl: bool) {
    app.set_rtl(rtl);
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
    preview_synthetic: AtomicBool,
    tools: Option<MediaTools>,
    exporting: AtomicBool,
    export_cancel: ExportCancellation,
    preview_generation: PreviewGeneration,
    playback_clock: Mutex<Option<PlaybackClock>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClockSource {
    AudioMaster,
    MonotonicFallback,
}

#[derive(Debug)]
struct PlaybackClock {
    source: ClockSource,
    anchor: Instant,
    anchor_position: f64,
}

impl PlaybackClock {
    fn start(source: ClockSource, position: f64) -> Self {
        Self {
            source,
            anchor: Instant::now(),
            anchor_position: position.max(0.0),
        }
    }

    fn source(&self) -> ClockSource {
        self.source
    }

    fn position(&self) -> f64 {
        self.anchor_position + self.anchor.elapsed().as_secs_f64()
    }

    fn seek(&mut self, position: f64) {
        self.anchor = Instant::now();
        self.anchor_position = position.max(0.0);
    }
}

#[derive(Debug)]
struct PreviewGeneration(AtomicU64);

impl Default for PreviewGeneration {
    fn default() -> Self {
        Self(AtomicU64::new(0))
    }
}

impl PreviewGeneration {
    fn next(&self) -> u64 {
        self.0.fetch_add(1, Ordering::AcqRel) + 1
    }

    fn is_current(&self, generation: u64) -> bool {
        self.0.load(Ordering::Acquire) == generation
    }
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

fn clip_display_name(clip: &Clip) -> String {
    if clip.source_path.trim().is_empty() {
        format!("{} · offline sample", clip.name)
    } else {
        clip.name.clone()
    }
}

fn clip_cache_status(clip: &Clip) -> String {
    let source = clip.source_path.trim();
    if source.is_empty() {
        "Offline sample · synthetic preview".to_string()
    } else if Path::new(source).is_file() {
        "Preview on demand · waveform pending".to_string()
    } else {
        "Source missing · relink required".to_string()
    }
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
            .map(|clip| SharedString::from(clip_display_name(clip)))
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
            .map(|clip| clip.effective_timeline_duration() as f32)
            .collect::<Vec<_>>(),
    )));
    app.set_clip_in_points(ModelRc::new(VecModel::from(
        clips
            .iter()
            .map(|clip| clip.in_point as f32)
            .collect::<Vec<_>>(),
    )));
    app.set_clip_out_points(ModelRc::new(VecModel::from(
        clips
            .iter()
            .map(|clip| clip.out_point as f32)
            .collect::<Vec<_>>(),
    )));
    app.set_clip_cache_status(ModelRc::new(VecModel::from(
        clips
            .iter()
            .map(|clip| SharedString::from(clip_cache_status(clip)))
            .collect::<Vec<_>>(),
    )));
    let selected = (*lock(&state.selected_clip)).min(clips.len().saturating_sub(1));
    *lock(&state.selected_clip) = selected;
    app.set_active_clip_index(selected as i32);
    let duration = timeline_duration(project);
    app.set_timeline_duration(duration as f32);
    let zoom = f64::from(app.get_timeline_zoom()).clamp(1.0, 4.0);
    app.set_timeline_zoom(zoom as f32);
    let visible_duration = (duration / zoom).min(duration);
    let max_scroll = (duration - visible_duration).max(0.0);
    let scroll = f64::from(app.get_timeline_scroll()).clamp(0.0, max_scroll);
    app.set_timeline_scroll(scroll as f32);
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
    let preview_synthetic = state.preview_synthetic.load(Ordering::Relaxed);
    app.set_preview_synthetic(preview_synthetic);
    app.set_status_right(
        if state.tools.is_some() {
            if preview_synthetic {
                "Local FFmpeg · synthetic preview"
            } else {
                "Local FFmpeg media"
            }
        } else {
            "Media backend unavailable"
        }
        .into(),
    );
    match lock(&state.playback_clock)
        .as_ref()
        .map(PlaybackClock::source)
    {
        Some(ClockSource::AudioMaster) => {
            app.set_playback_clock_source("Audio master".into());
            app.set_audio_output_status(
                "Audio stream detected · output device unavailable; clock is audio-master".into(),
            );
        }
        Some(ClockSource::MonotonicFallback) => {
            app.set_playback_clock_source("Monotonic fallback".into());
            app.set_audio_output_status("No audio stream · monotonic fallback clock".into());
        }
        None => {
            app.set_playback_clock_source("No clock".into());
            app.set_audio_output_status("Audio output unavailable".into());
        }
    }
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
    configure_direction(&app, args.rtl);
    configure_responsive_layout(&app, args.size.0);
    apply_theme(&app, &args.theme);
    let (initial_proj, initial_path) = initial_session(args)?;
    let state = AppState {
        session: Mutex::new(initial_proj),
        save_path: Mutex::new(initial_path),
        dialogs: Arc::new(NativeFileDialogs),
        selected_clip: Mutex::new(0),
        preview: Mutex::new(Some(procedural_preview())),
        preview_synthetic: AtomicBool::new(true),
        tools: discover_media_tools().ok(),
        exporting: AtomicBool::new(false),
        export_cancel: ExportCancellation::default(),
        preview_generation: PreviewGeneration::default(),
        playback_clock: Mutex::new(None),
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

fn capture_workflow_step(
    app: &VideoApp,
    out_dir: &Path,
    name: &str,
    detail: &str,
    steps: &mut Vec<String>,
) -> Result<(), String> {
    let image = snapshot_component(app, 1280.0, 800.0, 1.0)
        .map_err(|error| format!("capture {name}: {error}"))?;
    let path = out_dir.join(format!("video-workflow-{name}.png"));
    loom_test_support::png::save_png(&path, &image)
        .map_err(|error| format!("save {}: {error}", path.display()))?;
    steps.push(format!("{name}: {detail} | artifact={}", path.display()));
    Ok(())
}

fn create_workflow_media(tools: &MediaTools, out_dir: &Path) -> Result<PathBuf, String> {
    let path = out_dir.join("video-workflow-source.mp4");
    let output = Command::new(&tools.ffmpeg)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=320x180:rate=30",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000",
            "-t",
            "4",
            "-c:v",
            "mpeg4",
            "-c:a",
            "aac",
            "-pix_fmt",
            "yuv420p",
            "-shortest",
        ])
        .arg(&path)
        .output()
        .map_err(|error| format!("start journey media generator: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "journey media generator failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(path)
}

fn wait_for_export(state: &AppState, timeout: Duration) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    while state.exporting.load(Ordering::Acquire) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(25));
    }
    if state.exporting.load(Ordering::Acquire) {
        Err("timed out waiting for timeline export worker".into())
    } else {
        Ok(())
    }
}

fn wait_for_decoded_preview(
    app: &VideoApp,
    state: &AppState,
    timeout: Duration,
) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    while state.preview_synthetic.load(Ordering::Acquire) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(25));
    }
    if state.preview_synthetic.load(Ordering::Acquire) {
        return Err(format!(
            "timed out waiting for decoded in-window preview (status={})",
            app.get_status_left()
        ));
    }
    if let Some(frame) = lock(&state.preview).as_ref() {
        app.set_preview_image(frame_image(frame));
        app.set_has_preview(true);
        app.set_preview_synthetic(false);
        app.set_status_right("Local FFmpeg media".into());
    }
    Ok(())
}

/// Execute the controller-backed import/edit/save/play/export workflow with
/// per-step screenshots and assertions tied to `AppState` and persisted data.
fn run_journey(args: &Args, out_dir: &str) -> Result<(), String> {
    set_platform();
    let out_dir = Path::new(out_dir);
    std::fs::create_dir_all(out_dir).map_err(|error| format!("create journey output: {error}"))?;
    let tools = discover_media_tools()
        .map_err(|error| format!("Video journey requires local FFmpeg/FFprobe: {error}"))?;
    let source_path = create_workflow_media(&tools, out_dir)?;
    let probe = probe_media(&tools, &source_path)?;
    if !probe.has_audio || probe.width == 0 || probe.height == 0 || probe.duration <= 0.0 {
        return Err(format!(
            "journey source probe did not produce a video+audio stream: {probe:?}"
        ));
    }

    let project_path = out_dir.join("video-workflow.loomvideo");
    let export_path = out_dir.join("video-workflow-export.mp4");
    let cancel_path = out_dir.join("video-workflow-cancelled.mp4");
    let _ = std::fs::remove_file(&project_path);
    let _ = std::fs::remove_file(&export_path);
    let _ = std::fs::remove_file(&cancel_path);

    let app = VideoApp::new().map_err(|error| error.to_string())?;
    configure_direction(&app, args.rtl);
    configure_responsive_layout(&app, args.size.0);
    apply_theme(&app, &args.theme);
    app.window()
        .set_size(PhysicalSize::new(args.size.0, args.size.1));

    let (initial_proj, initial_path) = initial_session(args)?;
    let dialogs = ScriptedFileDialogs::new([Some(project_path.clone())], []);
    let state = Arc::new(AppState {
        session: Mutex::new(initial_proj),
        save_path: Mutex::new(initial_path),
        dialogs: Arc::new(dialogs),
        selected_clip: Mutex::new(0),
        preview: Mutex::new(Some(procedural_preview())),
        preview_synthetic: AtomicBool::new(true),
        tools: Some(tools.clone()),
        exporting: AtomicBool::new(false),
        export_cancel: ExportCancellation::default(),
        preview_generation: PreviewGeneration::default(),
        playback_clock: Mutex::new(None),
    });
    wire_application(&app, state.clone());
    wire_palette(&app);
    rebuild_palette(&app, "");
    let menu_bar = build_standard_menu_bar(
        "Loom Video",
        vec![MenuItem::action_with_shortcut(
            "file.export_video",
            "Export Timeline...",
            MenuShortcut::primary("E"),
        )],
        vec![],
        vec![MenuItem::check("view.inspector", "Inspector", true)],
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
    refresh(&app, &state);

    let mut steps = Vec::new();
    capture_workflow_step(
        &app,
        out_dir,
        "00-initial",
        "sample project loaded",
        &mut steps,
    )?;

    app.invoke_import_media(source_path.to_string_lossy().into_owned().into());
    let imported_index = *lock(&state.selected_clip);
    let imported = {
        let session = lock(&state.session);
        timeline_clips(&session.project)
            .get(imported_index)
            .map(|clip| (*clip).clone())
            .ok_or("import callback did not add a clip")?
    };
    if imported.source_path != source_path.to_string_lossy()
        || imported.duration <= 0.0
        || !app.get_can_undo()
    {
        return Err("import did not update the controller session and undo state".into());
    }
    app.invoke_select_clip(imported_index as i32);
    wait_for_decoded_preview(&app, &state, Duration::from_secs(8))?;
    capture_workflow_step(
        &app,
        out_dir,
        "01-import",
        "imported probed video+audio and decoded an in-window preview",
        &mut steps,
    )?;

    let original_start = imported.start_time;
    let original_duration = imported.duration;
    app.invoke_trim_selected(0.25, 0.0);
    let trimmed = {
        let session = lock(&state.session);
        timeline_clips(&session.project)
            .get(imported_index)
            .map(|clip| (*clip).clone())
            .ok_or("trim removed the selected clip")?
    };
    if trimmed.start_time <= original_start
        || trimmed.duration >= original_duration
        || !app.get_can_undo()
    {
        return Err("trim callback did not produce a reversible timeline edit".into());
    }
    let trimmed_start = trimmed.start_time;
    app.invoke_move_clip(imported_index as i32, 0.5);
    let moved_start = timeline_clips(&lock(&state.session).project)
        .get(imported_index)
        .map(|clip| clip.start_time)
        .ok_or("move removed the selected clip")?;
    if (moved_start - (trimmed_start + 0.5)).abs() > 1e-6 {
        return Err(format!(
            "move callback landed at {moved_start}, expected {}",
            trimmed_start + 0.5
        ));
    }
    app.invoke_undo();
    let undone_start = timeline_clips(&lock(&state.session).project)
        .get(imported_index)
        .map(|clip| clip.start_time)
        .ok_or("undo removed the selected clip")?;
    if (undone_start - trimmed_start).abs() > 1e-6 {
        return Err("undo did not restore the pre-move timeline position".into());
    }
    capture_workflow_step(
        &app,
        out_dir,
        "02-edit-undo",
        "trimmed, moved, and undid the move through controller callbacks",
        &mut steps,
    )?;

    *lock(&state.save_path) = Some(project_path.clone());
    app.invoke_save_project();
    if !project_path.is_file() {
        return Err("save callback did not write the journey project".into());
    }
    let persisted = load_video_project(
        &std::fs::read(&project_path)
            .map_err(|error| format!("read saved journey project: {error}"))?,
    )?;
    if persisted.tracks[0].clips.len() != timeline_clips(&lock(&state.session).project).len() {
        return Err("saved project clip count differs from the live controller".into());
    }
    app.invoke_open_project();
    let reopened = lock(&state.session).project.clone();
    if reopened.tracks[0].clips.len() != persisted.tracks[0].clips.len()
        || (reopened.tracks[0].clips[imported_index].in_point
            - persisted.tracks[0].clips[imported_index].in_point)
            .abs()
            > 1e-6
    {
        return Err("open callback did not restore the saved trim state".into());
    }
    capture_workflow_step(
        &app,
        out_dir,
        "03-save-reopen",
        "saved to and reopened the package through the dialog-backed controller",
        &mut steps,
    )?;

    let playback_start = reopened.tracks[0].clips[imported_index].start_time + 0.5;
    app.invoke_seek(playback_start as f32);
    wait_for_decoded_preview(&app, &state, Duration::from_secs(8))?;
    app.invoke_play_pause();
    if !app.get_is_playing()
        || app.get_playback_clock_source().as_str() != "Audio master"
        || lock(&state.playback_clock)
            .as_ref()
            .map(PlaybackClock::source)
            != Some(ClockSource::AudioMaster)
    {
        return Err("playback did not select the audio-master clock".into());
    }
    app.invoke_seek((playback_start + 0.25) as f32);
    let seek_position = lock(&state.playback_clock)
        .as_ref()
        .map(PlaybackClock::position)
        .ok_or("seek cleared the active playback clock")?;
    if (seek_position - playback_start - 0.25).abs() > 0.08 {
        return Err(format!("seek position drifted to {seek_position:.3}"));
    }
    app.invoke_play_pause();
    capture_workflow_step(
        &app,
        out_dir,
        "04-play-seek",
        "played the decoded preview with audio-master timing and sought",
        &mut steps,
    )?;

    app.set_export_path(export_path.to_string_lossy().into_owned().into());
    app.invoke_export_timeline(export_path.to_string_lossy().into_owned().into());
    wait_for_export(&state, Duration::from_secs(20))?;
    if !export_path.is_file() {
        return Err("completed export did not produce an output file".into());
    }
    capture_workflow_step(
        &app,
        out_dir,
        "05-export",
        "exported the edited timeline through the local FFmpeg worker",
        &mut steps,
    )?;

    app.invoke_export_timeline(cancel_path.to_string_lossy().into_owned().into());
    app.invoke_cancel_export();
    wait_for_export(&state, Duration::from_secs(20))?;
    if !state.export_cancel.is_cancelled() {
        return Err("cancel callback did not signal the export worker".into());
    }
    let _ = std::fs::remove_file(&cancel_path);
    capture_workflow_step(
        &app,
        out_dir,
        "06-export-cancel",
        "cancelled the second export and removed any partial output",
        &mut steps,
    )?;

    let report_path = out_dir.join("video-workflow.txt");
    let mut report = format!(
        "Video controller workflow: PASS\nsource={}\nproject={}\nexport={}\n\n",
        source_path.display(),
        project_path.display(),
        export_path.display()
    );
    report.push_str(&steps.join("\n"));
    report.push('\n');
    report.push_str(
        "Evidence limits: preview and export use local FFmpeg; no realtime audio device output is claimed.\n",
    );
    std::fs::write(&report_path, report)
        .map_err(|error| format!("write journey report {}: {error}", report_path.display()))?;
    println!("video workflow journey: PASS ({})", report_path.display());
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
    let generation = state.preview_generation.next();
    let source = {
        let session = lock(&state.session);
        timeline_clips(&session.project)
            .into_iter()
            .find(|clip| timeline_time >= clip.start_time && timeline_time < clip.end_time())
            .map(|clip| {
                (
                    PathBuf::from(&clip.source_path),
                    clip.in_point + (timeline_time - clip.start_time) * clip.playback_rate,
                )
            })
    };
    let Some((path, source_time)) = source.filter(|(path, _)| path.is_file()) else {
        show_synthetic_preview(&state, &weak);
        return;
    };
    let Some(tools) = state.tools.clone() else {
        show_synthetic_preview(&state, &weak);
        return;
    };
    let callback_state = state.clone();
    std::thread::spawn(
        move || match decode_preview_frame(&tools, &path, source_time, 960, 540) {
            Ok(frame) => {
                if !state.preview_generation.is_current(generation) {
                    return;
                }
                *lock(&state.preview) = Some(frame.clone());
                state.preview_synthetic.store(false, Ordering::Relaxed);
                let _ = weak.upgrade_in_event_loop(move |app| {
                    if !callback_state.preview_generation.is_current(generation) {
                        return;
                    }
                    app.set_preview_image(frame_image(&frame));
                    app.set_has_preview(true);
                    app.set_preview_synthetic(false);
                    app.set_status_left(format!("Decoded preview at {timeline_time:.2}s").into());
                });
            }
            Err(error) => {
                if !state.preview_generation.is_current(generation) {
                    return;
                }
                let _ = weak.upgrade_in_event_loop(move |app| {
                    if callback_state.preview_generation.is_current(generation) {
                        app.set_status_left(format!("Preview decode failed: {error}").into());
                    }
                });
            }
        },
    );
}

fn show_synthetic_preview(state: &AppState, weak: &slint::Weak<VideoApp>) {
    let frame = procedural_preview();
    *lock(&state.preview) = Some(frame.clone());
    state.preview_synthetic.store(true, Ordering::Release);
    if let Some(app) = weak.upgrade() {
        app.set_preview_image(frame_image(&frame));
        app.set_has_preview(true);
        app.set_preview_synthetic(true);
        app.set_status_right(
            if state.tools.is_some() {
                "Local FFmpeg · synthetic preview"
            } else {
                "Media backend unavailable"
            }
            .into(),
        );
    }
}

fn wire_application(app: &VideoApp, state: Arc<AppState>) {
    let timer = Rc::new(slint::Timer::default());
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let timer = timer.clone();
        app.on_new_project(move || {
            if let Some(app) = app_ref.upgrade() {
                timer.stop();
                state.preview_generation.next();
                *lock(&state.playback_clock) = None;
                app.set_is_playing(false);
                *lock(&state.session) = VideoSession::new(empty_project());
                *lock(&state.save_path) = None;
                *lock(&state.selected_clip) = 0;
                *lock(&state.preview) = Some(procedural_preview());
                state.preview_synthetic.store(true, Ordering::Relaxed);
                refresh(&app, &state);
                app.set_status_left("New untitled project created".into());
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let timer = timer.clone();
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
                            timer.stop();
                            state.preview_generation.next();
                            *lock(&state.playback_clock) = None;
                            app.set_is_playing(false);
                            *lock(&state.session) = VideoSession::new(project);
                            *lock(&state.save_path) = Some(path.clone());
                            state.preview_synthetic.store(true, Ordering::Relaxed);
                            refresh(&app, &state);
                            let preview_time = {
                                let session = lock(&state.session);
                                timeline_clips(&session.project)
                                    .get(*lock(&state.selected_clip))
                                    .map(|clip| clip.start_time)
                            };
                            if let Some(preview_time) = preview_time {
                                request_preview(state.clone(), app.as_weak(), preview_time);
                            }
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
                        let Some(track_index) = session.project.tracks.iter().position(|track| {
                            matches!(track.track_type, loom_video_core::TrackType::Video)
                        }) else {
                            app.set_status_left("Import failed: project has no video track".into());
                            return;
                        };
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
                        let result: Result<(), loom_video_core::TimelineError> = session
                            .apply_edit(|project| {
                                project.width = probe.width.max(1);
                                project.height = probe.height.max(1);
                                if probe.frame_rate > 0.0 {
                                    project.frame_rate = probe.frame_rate;
                                }
                                project.tracks[track_index].insert_clip(clip)
                            });
                        match result {
                            Ok(()) => {
                                *lock(&state.selected_clip) = session.project.tracks[track_index]
                                    .clips
                                    .len()
                                    .saturating_sub(1);
                                state.preview_generation.next();
                                state.preview_synthetic.store(true, Ordering::Relaxed);
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
                            Err(error) => {
                                app.set_status_left(format!("Import failed: {error}").into());
                            }
                        }
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
                    let result: Result<(), loom_video_core::TimelineError> =
                        session.apply_edit(|project| {
                            let track = project
                                .tracks
                                .get_mut(index as usize)
                                .ok_or(loom_video_core::TimelineError::InvalidTrack)?;
                            track.muted = !track.muted;
                            Ok(())
                        });
                    drop(session);
                    if result.is_ok() {
                        refresh(&app, &state);
                    } else {
                        app.set_status_left("Mute failed: invalid track".into());
                    }
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
                    let result: Result<(), loom_video_core::TimelineError> =
                        session.apply_edit(|project| {
                            let track = project
                                .tracks
                                .get_mut(index as usize)
                                .ok_or(loom_video_core::TimelineError::InvalidTrack)?;
                            track.solo = !track.solo;
                            Ok(())
                        });
                    drop(session);
                    if result.is_ok() {
                        refresh(&app, &state);
                    } else {
                        app.set_status_left("Solo failed: invalid track".into());
                    }
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
        app.on_move_clip(move |index, delta| {
            if let Some(app) = app_ref.upgrade() {
                if index < 0 || !delta.is_finite() || delta.abs() < f32::EPSILON {
                    return;
                }
                let selected = index as usize;
                let mut session = lock(&state.session);
                let track_index = session.project.tracks.iter().position(|track| {
                    matches!(track.track_type, loom_video_core::TrackType::Video)
                });
                let Some(track_index) = track_index else {
                    app.set_status_left("Move failed: project has no video track".into());
                    return;
                };
                let Some(clip_id) = session.project.tracks[track_index]
                    .clips
                    .get(selected)
                    .map(|clip| clip.id.clone())
                else {
                    app.set_status_left("Move failed: selected clip is unavailable".into());
                    return;
                };
                let result = session.apply_edit(|project| {
                    let clip = project.tracks[track_index]
                        .clips
                        .iter()
                        .find(|clip| clip.id == clip_id)
                        .ok_or(loom_video_core::TimelineError::ClipNotFound)?;
                    let target = clip.start_time + f64::from(delta);
                    project.move_clip(track_index, track_index, &clip_id, target, false)
                });
                if let Err(error) = result {
                    app.set_status_left(format!("Move failed: {error}").into());
                } else {
                    let selected_after = session.project.tracks[track_index]
                        .clips
                        .iter()
                        .position(|clip| clip.id == clip_id)
                        .unwrap_or(selected);
                    *lock(&state.selected_clip) = selected_after;
                    drop(session);
                    refresh(&app, &state);
                    app.set_status_left("Moved selected clip".into());
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
                        let result = session.apply_edit(|project| {
                            project.tracks[track_index]
                                .remove_clip(&id, true)
                                .map(|_| ())
                        });
                        if result.is_ok() {
                            *lock(&state.selected_clip) = selected.saturating_sub(1);
                        } else {
                            app.set_status_left(
                                "Remove failed: selected clip is unavailable".into(),
                            );
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
                        let result = session.apply_edit(|project| {
                            project.split_clip(track_index, &id, playhead).map(|_| ())
                        });
                        match result {
                            Ok(()) => {
                                app.set_status_left(
                                    format!("Split clip at {:.2}s", playhead).into(),
                                );
                            }
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
                if !in_delta.is_finite()
                    || !out_delta.is_finite()
                    || (in_delta.abs() < f32::EPSILON && out_delta.abs() < f32::EPSILON)
                {
                    return;
                }
                let selected = *lock(&state.selected_clip);
                let mut session = lock(&state.session);
                let track_index = session.project.tracks.iter().position(|track| {
                    matches!(track.track_type, loom_video_core::TrackType::Video)
                });
                if let Some(track_index) = track_index {
                    if let Some(clip_id) = session.project.tracks[track_index]
                        .clips
                        .get(selected)
                        .map(|clip| clip.id.clone())
                    {
                        let result: Result<(), loom_video_core::TimelineError> = session
                            .apply_edit(|project| {
                                let trim_result = {
                                    let clip = project.tracks[track_index]
                                        .clips
                                        .iter_mut()
                                        .find(|clip| clip.id == clip_id)
                                        .ok_or(loom_video_core::TimelineError::ClipNotFound)?;
                                    if in_delta != 0.0 {
                                        clip.trim_in((clip.in_point + in_delta as f64).max(0.0))
                                    } else if out_delta != 0.0 {
                                        clip.trim_out(
                                            (clip.out_point + out_delta as f64)
                                                .max(clip.in_point + 0.01),
                                        )
                                    } else {
                                        Ok(())
                                    }
                                };
                                trim_result?;
                                project.tracks[track_index].sort_clips();
                                Ok(())
                            });
                        if let Err(error) = result {
                            app.set_status_left(format!("Trim failed: {error}").into());
                        } else {
                            let selected_after = session.project.tracks[track_index]
                                .clips
                                .iter()
                                .position(|clip| clip.id == clip_id)
                                .unwrap_or(selected);
                            *lock(&state.selected_clip) = selected_after;
                        }
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
        app.on_trim_clip_in(move |index, delta| {
            if let Some(app) = app_ref.upgrade() {
                if index < 0 || !delta.is_finite() || delta.abs() < f32::EPSILON {
                    return;
                }
                *lock(&state.selected_clip) = index as usize;
                app.set_active_clip_index(index);
                app.invoke_trim_selected(delta, 0.0);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_trim_clip_out(move |index, delta| {
            if let Some(app) = app_ref.upgrade() {
                if index < 0 || !delta.is_finite() || delta.abs() < f32::EPSILON {
                    return;
                }
                *lock(&state.selected_clip) = index as usize;
                app.set_active_clip_index(index);
                app.invoke_trim_selected(0.0, delta);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_seek(move |seconds| {
            if let Some(app) = app_ref.upgrade() {
                let duration = timeline_duration(&lock(&state.session).project);
                let seconds = f64::from(seconds).clamp(0.0, duration);
                app.set_playhead_seconds(seconds as f32);
                app.set_timecode_display(
                    timecode(seconds, lock(&state.session).project.frame_rate).into(),
                );
                if let Some(clock) = lock(&state.playback_clock).as_mut() {
                    clock.seek(seconds);
                }
                request_preview(state.clone(), app.as_weak(), seconds);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let timer = timer.clone();
        app.on_play_pause(move || {
            if let Some(app) = app_ref.upgrade() {
                if app.get_is_playing() {
                    timer.stop();
                    let position = lock(&state.playback_clock)
                        .as_ref()
                        .map(PlaybackClock::position)
                        .unwrap_or_else(|| f64::from(app.get_playhead_seconds()));
                    *lock(&state.playback_clock) = None;
                    app.set_playhead_seconds(position as f32);
                    app.set_is_playing(false);
                    app.set_status_left("Playback paused".into());
                    return;
                }
                let playhead = f64::from(app.get_playhead_seconds());
                let (source_path, has_audio) = {
                    let session = lock(&state.session);
                    let source_path = timeline_clips(&session.project)
                        .into_iter()
                        .find(|clip| playhead >= clip.start_time && playhead < clip.end_time())
                        .map(|clip| clip.source_path.clone());
                    let has_audio = source_path
                        .as_deref()
                        .zip(state.tools.as_ref())
                        .and_then(|(path, tools)| {
                            let path = Path::new(path);
                            path.is_file().then(|| probe_media(tools, path).ok())
                        })
                        .flatten()
                        .map(|probe| probe.has_audio)
                        .unwrap_or(false);
                    (source_path, has_audio)
                };
                let source = if has_audio {
                    ClockSource::AudioMaster
                } else {
                    ClockSource::MonotonicFallback
                };
                *lock(&state.playback_clock) = Some(PlaybackClock::start(source, playhead));
                app.set_playback_clock_source(
                    if has_audio {
                        "Audio master"
                    } else {
                        "Monotonic fallback"
                    }
                    .into(),
                );
                app.set_audio_output_status(
                    if has_audio {
                        "Audio stream detected · output device unavailable; clock is audio-master"
                    } else {
                        "No audio stream · monotonic fallback clock"
                    }
                    .into(),
                );
                app.set_is_playing(true);
                app.set_status_left(
                    if source_path
                        .as_deref()
                        .is_some_and(|path| Path::new(path).is_file())
                    {
                        "Playing decoded preview in-window"
                    } else {
                        "Playing in-window preview · offline sample"
                    }
                    .into(),
                );
                request_preview(state.clone(), app.as_weak(), playhead);
                let weak = app.as_weak();
                let timer_state = state.clone();
                timer.start(
                    slint::TimerMode::Repeated,
                    std::time::Duration::from_millis(33),
                    move || {
                        if let Some(app) = weak.upgrade() {
                            let position = lock(&timer_state.playback_clock)
                                .as_ref()
                                .map(PlaybackClock::position);
                            let Some(position) = position else {
                                return;
                            };
                            let duration = timeline_duration(&lock(&timer_state.session).project);
                            if position >= duration {
                                app.set_playhead_seconds(duration as f32);
                                app.set_timecode_display(
                                    timecode(
                                        duration,
                                        lock(&timer_state.session).project.frame_rate,
                                    )
                                    .into(),
                                );
                                app.invoke_stop_playback();
                            } else {
                                app.set_playhead_seconds(position as f32);
                                app.set_timecode_display(
                                    timecode(
                                        position,
                                        lock(&timer_state.session).project.frame_rate,
                                    )
                                    .into(),
                                );
                            }
                        }
                    },
                );
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let timer = timer.clone();
        app.on_stop_playback(move || {
            if let Some(app) = app_ref.upgrade() {
                timer.stop();
                *lock(&state.playback_clock) = None;
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
                let count = session.project.markers.len() + 1;
                let result = session.apply_edit(|project| {
                    project.add_marker(TimelineMarker {
                        id: format!("marker-{count}"),
                        time,
                        label: format!("Marker {count}"),
                        color: "#d97745".into(),
                    })
                });
                drop(session);
                if result.is_ok() {
                    refresh(&app, &state);
                } else {
                    app.set_status_left("Marker failed: invalid playhead".into());
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_cancel_export(move || {
            if state.exporting.load(Ordering::Acquire) {
                state.export_cancel.cancel();
                if let Some(app) = app_ref.upgrade() {
                    app.set_status_left("Cancelling export…".into());
                }
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
            state.export_cancel.reset();
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
            let cancel = state.export_cancel.clone();
            std::thread::spawn(move || {
                let plan = build_timeline_export_plan(&project, &tools, &output);
                let result = match plan {
                    Ok(plan) => execute_timeline_export_with_cancel(
                        &plan,
                        {
                            let progress_weak = worker_weak.clone();
                            move |progress| {
                                let _ = progress_weak.upgrade_in_event_loop(move |app| {
                                    app.set_export_progress(progress * 100.0);
                                    app.set_status_left(
                                        format!("Rendering timeline · {:.0}%", progress * 100.0)
                                            .into(),
                                    );
                                });
                            }
                        },
                        &cancel,
                    ),
                    Err(error) => Err(error),
                };
                worker_state.exporting.store(false, Ordering::SeqCst);
                let _ = worker_weak.upgrade_in_event_loop(move |app| {
                    app.set_exporting(false);
                    app.set_status_left(
                        match result {
                            Ok(()) => format!("Exported {}", output.display()),
                            Err(error) if error.contains("cancel") => {
                                "Export cancelled; no completed file was produced".into()
                            }
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
    configure_direction(&app, args.rtl);
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
        preview_synthetic: AtomicBool::new(true),
        tools: discover_media_tools().ok(),
        exporting: AtomicBool::new(false),
        export_cancel: ExportCancellation::default(),
        preview_generation: PreviewGeneration::default(),
        playback_clock: Mutex::new(None),
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
        vec![MenuItem::check("view.inspector", "Inspector", true)],
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
            preview_synthetic: AtomicBool::new(true),
            tools: None,
            exporting: AtomicBool::new(false),
            export_cancel: ExportCancellation::default(),
            preview_generation: PreviewGeneration::default(),
            playback_clock: Mutex::new(None),
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
    fn playback_clock_exposes_audio_master_and_seek_position() {
        let mut clock = PlaybackClock::start(ClockSource::AudioMaster, 2.0);
        assert_eq!(clock.source(), ClockSource::AudioMaster);
        assert!(clock.position() >= 2.0);
        clock.seek(4.5);
        assert!((clock.position() - 4.5).abs() < 0.05);
    }

    #[test]
    fn preview_generation_rejects_stale_results() {
        let generation = PreviewGeneration::default();
        let first = generation.next();
        let second = generation.next();
        assert!(generation.is_current(second));
        assert!(!generation.is_current(first));
    }
}
