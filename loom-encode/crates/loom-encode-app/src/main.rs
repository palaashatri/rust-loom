//! Loom Encode desktop batch transcoding application.

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use loom_desktop::{
    build_standard_menu_bar, FileDialogService, FileFilter, Menu, MenuBarService, MenuItem,
    MenuShortcut, NativeFileDialogs, NativeMenuBar, OpenFileRequest, SaveFileRequest,
    ScriptedFileDialogs,
};
use loom_encode_core::{
    discover_ffmpeg, execute_job_with_cancel, load_encode_queue, probe_duration, save_encode_queue,
    AudioSettings, DestinationCollisionPolicy, EncodeJob, EncodePreset, EncodeQueue,
    EncoderBackend, ExecutionPolicy, JobStatus, MetadataSettings, SubtitleMode, SubtitleSettings,
    VideoSettings,
};
use loom_test_support::capture::{set_platform, snapshot_component};
use loom_test_support::journey::{record_keyboard_palette_journey, PaletteProbe};
use slint::private_unstable_api::re_exports::DataTransfer;
use slint::{ComponentHandle, Model, ModelRc, PhysicalSize, SharedString, VecModel};

slint::include_modules!();

const DEFAULT_SIZE: (u32, u32) = (1280, 800);
const HISTORY_LIMIT: usize = 128;

loom_production::define_snapshot_recovery!(ENCODE_RECOVERY, "org.loom.encode", "loom.encode/1");

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

fn empty_queue() -> EncodeQueue {
    EncodeQueue::new("untitled-queue", "Untitled Batch Queue")
}

fn sample_queue() -> EncodeQueue {
    let mut queue = EncodeQueue::new("encode-sample", "Local Delivery Queue");
    queue.jobs.clear();
    queue.add_job(EncodeJob::new(
        "job-1",
        "sample-input.mov",
        "sample-output.mp4",
        EncodePreset::h264_1080p(),
    ));
    queue
}

fn initial_queue(args: &Args) -> Result<(EncodeQueue, Option<PathBuf>), String> {
    match args.open.as_deref() {
        Some(path) => {
            let p = PathBuf::from(path);
            let bytes = std::fs::read(&p)
                .map_err(|error| format!("failed to read encode queue '{path}': {error}"))?;
            let queue = load_encode_queue(&bytes)
                .map_err(|error| format!("failed to parse encode queue '{path}': {error}"))?;
            Ok((queue, Some(p)))
        }
        None => Ok((sample_queue(), None)),
    }
}

fn preset_label(preset: &EncodePreset) -> &'static str {
    if preset.video_codec.eq_ignore_ascii_case("prores") {
        "ProRes 422"
    } else if preset.video_codec.eq_ignore_ascii_case("av1") {
        "AV1 High"
    } else {
        "Web 1080p"
    }
}

fn av1_preset() -> EncodePreset {
    EncodePreset {
        name: "AV1 High Quality".into(),
        container: "mp4".into(),
        video_codec: "av1".into(),
        audio_codec: "aac".into(),
        bitrate_kbps: 5000,
    }
}

fn encode_filter() -> FileFilter {
    FileFilter {
        name: "Loom Encode Queue (*.loomencode)".into(),
        extensions: vec!["loomencode".into()],
    }
}

fn source_media_filter() -> FileFilter {
    FileFilter {
        name: "Media source (video or audio)".into(),
        extensions: vec![
            "mov".into(),
            "mp4".into(),
            "mkv".into(),
            "webm".into(),
            "avi".into(),
            "wav".into(),
            "flac".into(),
            "mp3".into(),
        ],
    }
}

fn subtitle_filter() -> FileFilter {
    FileFilter {
        name: "Subtitles (*.srt, *.vtt)".into(),
        extensions: vec!["srt".into(), "vtt".into()],
    }
}

fn source_file_request(current: Option<&Path>) -> OpenFileRequest {
    OpenFileRequest {
        title: "Choose Encode Source".into(),
        initial_directory: current
            .and_then(|p| p.parent())
            .filter(|p| !p.as_os_str().is_empty())
            .map(Path::to_path_buf),
        suggested_name: None,
        filters: vec![source_media_filter()],
    }
}

fn subtitle_file_request(current: Option<&Path>) -> OpenFileRequest {
    OpenFileRequest {
        title: "Choose Subtitle File".into(),
        initial_directory: current
            .and_then(|p| p.parent())
            .filter(|p| !p.as_os_str().is_empty())
            .map(Path::to_path_buf),
        suggested_name: None,
        filters: vec![subtitle_filter()],
    }
}

fn output_file_request(current: Option<&Path>, preset: Option<&EncodePreset>) -> SaveFileRequest {
    let extension = preset
        .map(|preset| preset.container.as_str())
        .unwrap_or("mp4");
    SaveFileRequest {
        title: "Choose Encode Destination".into(),
        initial_directory: current
            .and_then(|p| p.parent())
            .filter(|p| !p.as_os_str().is_empty())
            .map(Path::to_path_buf),
        suggested_name: current
            .and_then(Path::file_name)
            .map(|name| name.to_string_lossy().into_owned())
            .or_else(|| Some(format!("encoded.{extension}"))),
        filters: vec![FileFilter {
            name: format!("Output (*.{extension})"),
            extensions: vec![extension.into()],
        }],
    }
}

fn open_queue_request(save_path: Option<&Path>) -> OpenFileRequest {
    OpenFileRequest {
        title: "Open Encode Queue".into(),
        initial_directory: save_path
            .and_then(|p| p.parent())
            .filter(|p| !p.as_os_str().is_empty())
            .map(Path::to_path_buf),
        suggested_name: None,
        filters: vec![encode_filter()],
    }
}

fn save_queue_request(save_path: Option<&Path>) -> SaveFileRequest {
    SaveFileRequest {
        title: "Save Encode Queue".into(),
        initial_directory: save_path
            .and_then(|p| p.parent())
            .filter(|p| !p.as_os_str().is_empty())
            .map(Path::to_path_buf),
        suggested_name: save_path
            .and_then(Path::file_name)
            .map(|n| n.to_string_lossy().into_owned())
            .or_else(|| Some("Untitled.loomencode".into())),
        filters: vec![encode_filter()],
    }
}

fn job_progress(status: &JobStatus) -> f32 {
    match status {
        JobStatus::Encoding { progress } => progress * 100.0,
        JobStatus::Complete => 100.0,
        JobStatus::Queued
        | JobStatus::Retrying { .. }
        | JobStatus::Cancelled
        | JobStatus::Failed(_) => 0.0,
    }
}

fn job_status_label(status: &JobStatus) -> String {
    match status {
        JobStatus::Queued => "QUEUED".into(),
        JobStatus::Encoding { progress } => format!("RUNNING {:.0}%", progress * 100.0),
        JobStatus::Cancelled => "CANCELLED · Retry available".into(),
        JobStatus::Retrying { attempt } => format!("RETRYING · attempt {attempt}"),
        JobStatus::Complete => "COMPLETE".into(),
        JobStatus::Failed(reason) => format!("FAILED · {reason} · Retry available"),
    }
}

fn job_status_tone(status: &JobStatus) -> i32 {
    match status {
        JobStatus::Complete => 2,
        JobStatus::Failed(_) | JobStatus::Cancelled => 4,
        JobStatus::Encoding { .. } | JobStatus::Retrying { .. } => 3,
        JobStatus::Queued => 0,
    }
}

fn selected_video_summary(video: &VideoSettings) -> String {
    format!("Codec {} · {} kbps", video.codec, video.bitrate_kbps)
}

fn selected_audio_summary(audio: &AudioSettings) -> String {
    format!("Codec {} · {} kbps", audio.codec, audio.bitrate_kbps)
}

fn selected_subtitle_summary(subtitles: &SubtitleSettings) -> String {
    let mode = match subtitles.mode {
        SubtitleMode::None => "None",
        SubtitleMode::BurnIn => "Burn in",
        SubtitleMode::PassthroughCopy => "Passthrough",
        SubtitleMode::ConvertSrt => "Convert to embedded SRT",
    };
    match subtitles.path.as_deref() {
        Some(path) => format!("{mode} · {path}"),
        None => mode.into(),
    }
}

fn selected_metadata_summary(metadata: &MetadataSettings) -> &'static str {
    if metadata.copy {
        "Copy source metadata"
    } else {
        "Do not copy source metadata"
    }
}

fn selected_destination_summary(policy: DestinationCollisionPolicy, output: &str) -> String {
    let policy = match policy {
        DestinationCollisionPolicy::Fail => "Fail on collision",
        DestinationCollisionPolicy::Rename => "Rename on collision",
        DestinationCollisionPolicy::Overwrite => "Overwrite atomically",
    };
    format!("{policy} · {output}")
}

fn subtitle_mode_index(mode: SubtitleMode) -> i32 {
    match mode {
        SubtitleMode::None => 0,
        SubtitleMode::BurnIn => 1,
        SubtitleMode::PassthroughCopy => 2,
        SubtitleMode::ConvertSrt => 3,
    }
}

fn collision_policy_index(policy: DestinationCollisionPolicy) -> i32 {
    match policy {
        DestinationCollisionPolicy::Fail => 0,
        DestinationCollisionPolicy::Rename => 1,
        DestinationCollisionPolicy::Overwrite => 2,
    }
}

fn percent_decode(s: &str) -> String {
    let mut out = Vec::with_capacity(s.len());
    let mut bytes = s.bytes();
    while let Some(b) = bytes.next() {
        if b == b'%' {
            let mut clone = bytes.clone();
            if let (Some(h1), Some(h2)) = (clone.next(), clone.next()) {
                if let (Some(n1), Some(n2)) = ((h1 as char).to_digit(16), (h2 as char).to_digit(16))
                {
                    out.push((n1 * 16 + n2) as u8);
                    bytes = clone;
                    continue;
                }
            }
        }
        out.push(b);
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Converts Slint's platform-neutral drop payload into a filesystem path.
/// Native file managers expose a `text/uri-list`-compatible plain-text view;
/// scripted or in-app drops may provide a plain path directly.
fn dropped_path(data: &DataTransfer) -> Option<String> {
    let text = data.plain_text().ok()?.to_string();
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            if let Some(path) = line.strip_prefix("file://") {
                let path = percent_decode(path);
                #[cfg(windows)]
                let path = if path.len() > 2 && path.starts_with('/') && path.as_bytes()[2] == b':'
                {
                    path[1..].to_string()
                } else if path.starts_with('/') {
                    path
                } else {
                    format!("/{path}")
                };
                #[cfg(not(windows))]
                let path = if path.starts_with('/') {
                    path
                } else {
                    format!("/{path}")
                };
                path
            } else {
                line.to_string()
            }
        })
        .next()
}

fn refresh(app: &EncodeApp, queue: &EncodeQueue, backend: Option<&EncoderBackend>, running: bool) {
    app.set_queue_name(queue.name.as_str().into());
    app.set_job_labels(ModelRc::new(VecModel::from(
        queue
            .jobs
            .iter()
            .enumerate()
            .map(|(index, job)| {
                SharedString::from(format!(
                    "{:02}  {}",
                    index + 1,
                    Path::new(&job.source_file)
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or(&job.source_file)
                ))
            })
            .collect::<Vec<_>>(),
    )));
    app.set_job_details(ModelRc::new(VecModel::from(
        queue
            .jobs
            .iter()
            .map(|job| {
                SharedString::from(format!(
                    "{} · {} kbps → {}",
                    job.preset.name, job.preset.bitrate_kbps, job.output_file
                ))
            })
            .collect::<Vec<_>>(),
    )));
    app.set_job_statuses(ModelRc::new(VecModel::from(
        queue
            .jobs
            .iter()
            .map(|job| SharedString::from(job_status_label(&job.status)))
            .collect::<Vec<_>>(),
    )));
    app.set_job_progresses(ModelRc::new(VecModel::from(
        queue
            .jobs
            .iter()
            .map(|job| job_progress(&job.status))
            .collect::<Vec<_>>(),
    )));
    app.set_job_tones(ModelRc::new(VecModel::from(
        queue
            .jobs
            .iter()
            .map(|job| job_status_tone(&job.status))
            .collect::<Vec<_>>(),
    )));
    app.set_job_running(ModelRc::new(VecModel::from(
        queue
            .jobs
            .iter()
            .map(|job| {
                matches!(
                    job.status,
                    JobStatus::Encoding { .. } | JobStatus::Retrying { .. }
                )
            })
            .collect::<Vec<_>>(),
    )));
    app.set_active_job_index(queue.active_job_index as i32);
    app.set_batch_progress(queue.progress() * 100.0);
    app.set_running(running);
    app.set_backend_available(backend.is_some());
    app.set_backend_version(
        backend
            .map(|item| SharedString::from(item.version.as_str()))
            .unwrap_or_else(|| "Install FFmpeg and ensure it is available on PATH".into()),
    );
    app.set_can_start(backend.is_some() && !running && queue.next_queued_index().is_some());
    app.set_can_retry(
        !running
            && queue
                .jobs
                .get(queue.active_job_index)
                .is_some_and(EncodeJob::can_retry),
    );
    if let Some(job) = queue.jobs.get(queue.active_job_index) {
        app.set_selected_job_text(job.source_file.as_str().into());
        app.set_selected_job_details(SharedString::from(format!(
            "{} · {} kbps · {}",
            job.preset.name,
            job.preset.bitrate_kbps,
            job.preset.container.to_uppercase()
        )));
        app.set_selected_preset(preset_label(&job.preset).into());
        app.set_selected_source(job.source_file.as_str().into());
        app.set_selected_output(job.output_file.as_str().into());
        app.set_active_job_progress(job_progress(&job.status));
        app.set_selected_video(selected_video_summary(&job.video).into());
        app.set_selected_audio(selected_audio_summary(&job.audio).into());
        app.set_selected_subtitles(selected_subtitle_summary(&job.subtitles).into());
        app.set_selected_audio_codec(job.audio.codec.as_str().into());
        app.set_selected_subtitle_mode(subtitle_mode_index(job.subtitles.mode));
        app.set_selected_metadata(selected_metadata_summary(&job.metadata).into());
        app.set_metadata_copy(job.metadata.copy);
        app.set_selected_destination(
            selected_destination_summary(job.destination.collision_policy, &job.output_file).into(),
        );
        app.set_collision_policy(collision_policy_index(job.destination.collision_policy));
        app.set_selected_status(job_status_label(&job.status).into());
    } else {
        app.set_selected_job_text("No job selected".into());
        app.set_selected_job_details("".into());
        app.set_selected_source("".into());
        app.set_selected_output("".into());
        app.set_active_job_progress(0.0);
        app.set_selected_video("".into());
        app.set_selected_audio("".into());
        app.set_selected_subtitles("".into());
        app.set_selected_audio_codec("aac".into());
        app.set_selected_subtitle_mode(0);
        app.set_selected_metadata("".into());
        app.set_metadata_copy(true);
        app.set_selected_destination("".into());
        app.set_collision_policy(0);
        app.set_selected_status("No job selected".into());
    }
    let report = queue.conformance_report();
    let passed = report.jobs.iter().filter(|job| job.passed).count();
    app.set_report_summary(
        if report.passed {
            format!("Conformance passed · {passed}/{} jobs", report.jobs.len())
        } else {
            format!(
                "Conformance pending · {passed}/{} jobs passed",
                report.jobs.len()
            )
        }
        .into(),
    );
    app.set_status_right(
        if backend.is_some() {
            "Local FFmpeg"
        } else {
            "Encoder unavailable"
        }
        .into(),
    );
    if let Ok(bytes) = save_encode_queue(queue) {
        let _ = record_snapshot_recovery("encode queue state", bytes);
    }
}

fn set_selected_source_path(app: &EncodeApp, state: &Arc<AppState>, path: impl Into<String>) {
    if state.running.load(Ordering::Relaxed) {
        app.set_status_left("Queue is running; stop it before changing source paths".into());
        return;
    }
    let path = path.into();
    if path.trim().is_empty() {
        app.set_status_left("Source path cannot be empty".into());
        return;
    }
    let mut queue = state
        .queue
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let before = queue.clone();
    let index = queue.active_job_index;
    if let Some(job) = queue.jobs.get_mut(index) {
        job.set_source_file(path);
    }
    if before.queue_digest() != queue.queue_digest() {
        checkpoint_queue(state, &before);
    }
    refresh(
        app,
        &queue,
        state.backend.as_ref(),
        state.running.load(Ordering::Relaxed),
    );
    update_history_controls(app, state);
    app.set_status_left("Source selected · queue ready".into());
}

fn set_selected_output_path(app: &EncodeApp, state: &Arc<AppState>, path: impl Into<String>) {
    if state.running.load(Ordering::Relaxed) {
        app.set_status_left("Queue is running; stop it before changing destination paths".into());
        return;
    }
    let path = path.into();
    if path.trim().is_empty() {
        app.set_status_left("Destination path cannot be empty".into());
        return;
    }
    let mut queue = state
        .queue
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let before = queue.clone();
    let index = queue.active_job_index;
    if let Some(job) = queue.jobs.get_mut(index) {
        job.set_output_file(path);
    }
    if before.queue_digest() != queue.queue_digest() {
        checkpoint_queue(state, &before);
    }
    refresh(
        app,
        &queue,
        state.backend.as_ref(),
        state.running.load(Ordering::Relaxed),
    );
    update_history_controls(app, state);
    app.set_status_left("Destination selected · collision-safe output".into());
}

fn set_selected_subtitle_path(app: &EncodeApp, state: &Arc<AppState>, path: impl Into<String>) {
    if state.running.load(Ordering::Relaxed) {
        app.set_status_left("Queue is running; stop it before changing subtitles".into());
        return;
    }
    let path = path.into();
    if path.trim().is_empty() {
        app.set_status_left("Subtitle path cannot be empty".into());
        return;
    }
    let mut queue = state
        .queue
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let before = queue.clone();
    let index = queue.active_job_index;
    if let Some(job) = queue.jobs.get_mut(index) {
        job.subtitles.path = Some(path);
        job.subtitles.mode = loom_encode_core::SubtitleMode::PassthroughCopy;
        job.status = JobStatus::Queued;
    }
    if before.queue_digest() != queue.queue_digest() {
        checkpoint_queue(state, &before);
    }
    refresh(
        app,
        &queue,
        state.backend.as_ref(),
        state.running.load(Ordering::Relaxed),
    );
    update_history_controls(app, state);
    app.set_status_left("Subtitles selected · passthrough enabled".into());
}

fn apply_theme(app: &EncodeApp, theme: &str) {
    Theme::get(app).set_active_theme(theme.into());
}

fn configure_responsive_layout(app: &EncodeApp, size: (u32, u32)) {
    configure_responsive_width(app, size.0);
}

fn configure_responsive_width(app: &EncodeApp, width: u32) {
    let policy = ResponsivePolicy::get(app);
    app.set_compact_layout((width as f32) < policy.get_priority_1_icon_only_below());
}

fn configure_direction(app: &EncodeApp, rtl: bool) {
    app.set_rtl(rtl);
}

fn wire_responsive_layout(app: &EncodeApp) {
    let app_ref = app.as_weak();
    app.on_window_resized(move |width| {
        if let Some(app) = app_ref.upgrade() {
            configure_responsive_width(&app, width.max(0.0) as u32);
        }
    });
}

fn render_headless(args: &Args, output: &str) -> Result<(), String> {
    set_platform();
    let app = EncodeApp::new().map_err(|error| error.to_string())?;
    configure_direction(&app, args.rtl);
    apply_theme(&app, &args.theme);
    configure_responsive_layout(&app, args.size);
    let (queue, _) = initial_queue(args)?;
    let backend = discover_ffmpeg(&[]).ok();
    refresh(&app, &queue, backend.as_ref(), false);
    if args.palette {
        app.set_palette_query(SharedString::from("qu"));
        rebuild_palette(&app, "qu");
        // The filtered screenshot probe has one matching queue command.
        // Keep the preview selection within that list so the Flickable does
        // not scroll its only row into the clipped viewport.
        app.set_palette_selected(0);
        app.set_palette_open(true);
    }
    app.set_status_left("Queue ready · source paths are editable".into());
    let image = snapshot_component(&app, args.size.0 as f32, args.size.1 as f32, 1.0)
        .map_err(|error| error.to_string())?;
    loom_test_support::png::save_png(Path::new(output), &image).map_err(|error| error.to_string())
}

fn refresh_journey(app: &EncodeApp, state: &Arc<AppState>) {
    let queue = snapshot(state);
    let backend = state.backend.clone();
    refresh(
        app,
        &queue,
        backend.as_ref(),
        state.running.load(Ordering::Relaxed),
    );
    update_history_controls(app, state);
}

fn capture_encode_workflow_step(
    app: &EncodeApp,
    state: &Arc<AppState>,
    size: (u32, u32),
    out_dir: &Path,
    name: &str,
    detail: &str,
    steps: &mut Vec<String>,
) -> Result<(), String> {
    let image = snapshot_component(app, size.0 as f32, size.1 as f32, 1.0)
        .map_err(|error| format!("capture {name}: {error}"))?;
    let path = out_dir.join(format!("encode-workflow-{name}.png"));
    loom_test_support::png::save_png(&path, &image)
        .map_err(|error| format!("save {}: {error}", path.display()))?;
    let queue = snapshot(state);
    let statuses = queue
        .jobs
        .iter()
        .map(|job| format!("{}={}", job.id, job_status_label(&job.status)))
        .collect::<Vec<_>>()
        .join(", ");
    steps.push(format!(
        "{name}: {detail} | statuses=[{statuses}] | artifact={}",
        path.display()
    ));
    Ok(())
}

fn wait_for_job_status<F>(
    state: &AppState,
    index: usize,
    timeout: Duration,
    label: &str,
    predicate: F,
) -> Result<JobStatus, String>
where
    F: Fn(&JobStatus) -> bool,
{
    let deadline = Instant::now() + timeout;
    loop {
        let status = state
            .queue
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .jobs
            .get(index)
            .map(|job| job.status.clone());
        if let Some(status) = status {
            if predicate(&status) {
                return Ok(status);
            }
            if !state.running.load(Ordering::Acquire) {
                return Err(format!(
                    "{label} ended before the expected state (last status: {status:?})"
                ));
            }
        }
        if Instant::now() >= deadline {
            let status = state
                .queue
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .jobs
                .get(index)
                .map(|job| format!("{:?}", job.status))
                .unwrap_or_else(|| "missing job".into());
            return Err(format!(
                "timed out waiting for {label} (last status: {status})"
            ));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn wait_for_queue_idle(state: &AppState, timeout: Duration, label: &str) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    while state.running.load(Ordering::Acquire) {
        if Instant::now() >= deadline {
            return Err(format!("timed out waiting for {label}"));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    Ok(())
}

fn has_encode_temporary_output(out_dir: &Path) -> Result<bool, String> {
    Ok(std::fs::read_dir(out_dir)
        .map_err(|error| format!("read {}: {error}", out_dir.display()))?
        .filter_map(Result::ok)
        .any(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .contains(".loom-encode-")
        }))
}

#[cfg(unix)]
fn create_journey_encoder(out_dir: &Path) -> Result<EncoderBackend, String> {
    use std::os::unix::fs::PermissionsExt;

    let executable = out_dir.join("loom-encode-fixture.sh");
    let script = r##"#!/bin/sh
out=""
source=""
for arg in "$@"; do
    out="$arg"
    case "$arg" in
        *cancel.mov|*failure.mov|*success.mov) source="$arg" ;;
    esac
done
if [ -z "$out" ]; then
    exit 2
fi
case "$source" in
    *cancel.mov)
        printf 'partial' > "$out"
        i=0
        while [ "$i" -lt 400 ]; do
            echo "out_time_us=$((i * 10000))"
            sleep 0.01
            i=$((i + 1))
        done
        echo progress=end
        ;;
    *failure.mov)
        marker="${source}.loom-fail-once"
        if [ ! -e "$marker" ]; then
            : > "$marker"
            printf 'partial' > "$out"
            echo out_time_us=100000
            echo progress=end
            exit 7
        fi
        # Minimal deterministic ISO-BMFF header. The queue conformance probe
        # rejects the old plain-text fixture and requires this media marker.
        printf '\000\000\000\034ftypisom\000\000\002\000isomiso2mp41' > "$out"
        echo out_time_us=100000
        echo progress=end
        ;;
    *)
        printf '\000\000\000\034ftypisom\000\000\002\000isomiso2mp41' > "$out"
        echo out_time_us=100000
        echo progress=end
        ;;
esac
"##;
    std::fs::write(&executable, script)
        .map_err(|error| format!("write fixture encoder: {error}"))?;
    let mut permissions = std::fs::metadata(&executable)
        .map_err(|error| format!("stat fixture encoder: {error}"))?
        .permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&executable, permissions)
        .map_err(|error| format!("make fixture encoder executable: {error}"))?;
    let probe = out_dir.join("ffprobe");
    std::fs::write(&probe, "#!/bin/sh\necho 1.0\n")
        .map_err(|error| format!("write fixture probe: {error}"))?;
    let mut probe_permissions = std::fs::metadata(&probe)
        .map_err(|error| format!("stat fixture probe: {error}"))?
        .permissions();
    probe_permissions.set_mode(0o755);
    std::fs::set_permissions(&probe, probe_permissions)
        .map_err(|error| format!("make fixture probe executable: {error}"))?;
    Ok(EncoderBackend {
        executable,
        version: "Loom Encode deterministic fixture encoder".into(),
    })
}

#[cfg(not(unix))]
fn create_journey_encoder(_out_dir: &Path) -> Result<EncoderBackend, String> {
    Err("Encode lifecycle journey requires the Unix fixture encoder; native Windows journey remains unverified".into())
}

/// Execute the controller-backed queue lifecycle and retain a screenshot/report
/// for each durable state transition, then append the canonical palette probe.
fn run_journey(args: &Args, out_dir: &str) -> Result<(), String> {
    set_platform();
    let out_dir = Path::new(out_dir);
    std::fs::create_dir_all(out_dir)
        .map_err(|error| format!("create journey output {}: {error}", out_dir.display()))?;
    let source_cancel = out_dir.join("cancel.mov");
    let source_failure = out_dir.join("failure.mov");
    let source_success = out_dir.join("success.mov");
    let subtitle = out_dir.join("captions.srt");
    let project_path = out_dir.join("encode-workflow.loomencode");
    let cancel_output = out_dir.join("cancelled.mp4");
    let failure_output = out_dir.join("failure.mp4");
    for (path, bytes) in [
        (&source_cancel, b"cancel source".as_slice()),
        (&source_failure, b"failure source".as_slice()),
        (&source_success, b"success source".as_slice()),
        (
            &subtitle,
            b"1\n00:00:00,000 --> 00:00:01,000\nLoom\n".as_slice(),
        ),
    ] {
        std::fs::write(path, bytes)
            .map_err(|error| format!("write journey fixture {}: {error}", path.display()))?;
    }
    for path in [&project_path, &cancel_output, &failure_output] {
        let _ = std::fs::remove_file(path);
    }
    let backend = create_journey_encoder(out_dir)?;
    let dialogs = ScriptedFileDialogs::new(
        [
            Some(source_cancel.clone()),
            Some(subtitle.clone()),
            Some(project_path.clone()),
        ],
        [Some(cancel_output.clone()), Some(project_path.clone())],
    );
    let app = EncodeApp::new().map_err(|error| error.to_string())?;
    configure_direction(&app, args.rtl);
    apply_theme(&app, &args.theme);
    configure_responsive_layout(&app, args.size);
    app.window()
        .set_size(PhysicalSize::new(args.size.0, args.size.1));
    let state = Arc::new(AppState {
        queue: Mutex::new(sample_queue()),
        history: Mutex::new(QueueHistory::default()),
        save_path: Mutex::new(None),
        dialogs: Arc::new(dialogs),
        backend: Some(backend.clone()),
        cancel: AtomicBool::new(false),
        running: AtomicBool::new(false),
    });
    wire_application(&app, state.clone());
    wire_palette(&app);
    rebuild_palette(&app, "");
    refresh_journey(&app, &state);

    let menu_bar = build_standard_menu_bar(
        "Loom Encode",
        vec![],
        vec![],
        vec![MenuItem::check("view.inspector", "Inspector", true)],
        vec![Menu::new(
            "Queue",
            vec![
                MenuItem::action_with_shortcut(
                    "queue.add_job",
                    "Add Transcode Job",
                    MenuShortcut::primary_shift("N"),
                ),
                MenuItem::action("queue.remove_job", "Remove Selected Job"),
                MenuItem::action_with_shortcut(
                    "queue.start",
                    "Start Queue",
                    MenuShortcut::primary("R"),
                ),
                MenuItem::action("queue.cancel", "Cancel Queue"),
            ],
        )],
    );
    let menu_service = NativeMenuBar::new();
    let _ = menu_service.install_menu_bar(&menu_bar);

    let mut steps = Vec::new();
    capture_encode_workflow_step(
        &app,
        &state,
        args.size,
        out_dir,
        "00-initial",
        "fresh queue with one queued job",
        &mut steps,
    )?;

    app.invoke_choose_source();
    app.invoke_source_dropped(DataTransfer::from(SharedString::from(
        source_cancel.to_string_lossy().as_ref(),
    )));
    app.invoke_choose_output();
    app.invoke_choose_subtitles();
    app.invoke_audio_codec_changed("opus".into());
    app.invoke_subtitle_mode_changed(3);
    app.invoke_metadata_copy_changed(false);
    app.invoke_collision_policy_changed(2);
    {
        let queue = snapshot(&state);
        let job = queue.jobs.first().ok_or("journey queue lost initial job")?;
        if job.source_file != source_cancel.to_string_lossy()
            || job.output_file != cancel_output.to_string_lossy()
            || job.subtitles.path.as_deref() != Some(subtitle.to_string_lossy().as_ref())
            || job.audio.codec != "opus"
            || job.subtitles.mode != SubtitleMode::ConvertSrt
            || job.metadata.copy
            || job.destination.collision_policy != DestinationCollisionPolicy::Overwrite
        {
            return Err("typed chooser/drop configuration did not reach the selected job".into());
        }
    }
    capture_encode_workflow_step(
        &app,
        &state,
        args.size,
        out_dir,
        "01-configured",
        "source/output chooser, source drop, subtitles, audio, metadata, and collision policy updated typed fields",
        &mut steps,
    )?;

    app.invoke_add_job();
    app.invoke_source_changed(source_failure.to_string_lossy().into_owned().into());
    app.invoke_output_changed(failure_output.to_string_lossy().into_owned().into());
    {
        let queue = snapshot(&state);
        if queue.jobs.len() != 2 || queue.active_job_index != 1 {
            return Err("add job did not select the new queue row".into());
        }
    }
    capture_encode_workflow_step(
        &app,
        &state,
        args.size,
        out_dir,
        "02-added",
        "added a second job and configured its failure-once fixture source/output",
        &mut steps,
    )?;

    app.invoke_move_job(1, 0);
    if snapshot(&state).jobs.first().map(|job| job.id.as_str()) != Some("job-2") {
        return Err("queue reorder did not move the selected job".into());
    }
    capture_encode_workflow_step(
        &app,
        &state,
        args.size,
        out_dir,
        "03-reordered",
        "moved the selected job as a stable queue row",
        &mut steps,
    )?;
    app.invoke_undo();
    {
        let queue = snapshot(&state);
        if queue.jobs.first().map(|job| job.id.as_str()) != Some("job-1")
            || queue.active_job_index != 1
        {
            return Err("undo did not restore queue order and selection".into());
        }
    }
    capture_encode_workflow_step(
        &app,
        &state,
        args.size,
        out_dir,
        "04-undo",
        "undo restored order, selection, and the configured typed fields",
        &mut steps,
    )?;

    app.invoke_select_job(0);
    app.invoke_save_as_queue();
    if !project_path.is_file() {
        return Err("save-as callback did not write the queue package".into());
    }
    app.invoke_open_queue();
    {
        let queue = snapshot(&state);
        let job = queue.jobs.first().ok_or("reopened queue lost first job")?;
        if queue.jobs.len() != 2
            || job.source_file != source_cancel.to_string_lossy()
            || job.audio.codec != "opus"
            || job.subtitles.mode != SubtitleMode::ConvertSrt
        {
            return Err("save/reopen did not preserve typed queue configuration".into());
        }
    }
    refresh_journey(&app, &state);
    capture_encode_workflow_step(
        &app,
        &state,
        args.size,
        out_dir,
        "05-save-reopen",
        "saved and reopened the durable queue package through scripted native dialog services",
        &mut steps,
    )?;

    std::fs::write(&cancel_output, b"prior destination")
        .map_err(|error| format!("seed cancellation destination: {error}"))?;
    app.invoke_select_job(0);
    app.invoke_start_queue();
    wait_for_job_status(
        &state,
        0,
        Duration::from_secs(3),
        "cancel fixture to enter encoding state",
        |status| matches!(status, JobStatus::Encoding { .. }),
    )?;
    app.invoke_cancel_queue();
    wait_for_queue_idle(&state, Duration::from_secs(12), "cancelled queue")?;
    {
        let queue = snapshot(&state);
        if !matches!(queue.jobs[0].status, JobStatus::Cancelled)
            || std::fs::read(&cancel_output).ok().as_deref() != Some(b"prior destination")
            || has_encode_temporary_output(out_dir)?
        {
            return Err(
                "cancelled encode did not preserve destination and clean temporary output".into(),
            );
        }
    }
    app.set_status_left("Queue cancelled · destination preserved · retry available".into());
    refresh_journey(&app, &state);
    capture_encode_workflow_step(
        &app,
        &state,
        args.size,
        out_dir,
        "06-cancelled",
        "cancelled the active encode while preserving its existing destination and removing the temporary output",
        &mut steps,
    )?;

    app.invoke_select_job(0);
    app.invoke_retry_job();
    if !matches!(snapshot(&state).jobs[0].status, JobStatus::Retrying { .. }) {
        return Err("cancelled job did not enter retrying state".into());
    }
    app.set_status_left("Retrying cancelled job · source can be corrected".into());
    refresh_journey(&app, &state);
    capture_encode_workflow_step(
        &app,
        &state,
        args.size,
        out_dir,
        "07-retry-cancelled",
        "explicit retry moved the cancelled job into a durable retrying state",
        &mut steps,
    )?;
    app.invoke_source_changed(source_success.to_string_lossy().into_owned().into());
    app.invoke_start_queue();
    wait_for_queue_idle(
        &state,
        Duration::from_secs(12),
        "first completion/failure pass",
    )?;
    {
        let queue = snapshot(&state);
        if !matches!(queue.jobs[0].status, JobStatus::Complete)
            || !matches!(queue.jobs[1].status, JobStatus::Failed(_))
            || !cancel_output.is_file()
            || has_encode_temporary_output(out_dir)?
        {
            return Err(
                "first retry pass did not complete the corrected job and fail the fixture job"
                    .into(),
            );
        }
    }
    app.set_status_left("Job 2 failed · retry available · temporary output removed".into());
    refresh_journey(&app, &state);
    capture_encode_workflow_step(
        &app,
        &state,
        args.size,
        out_dir,
        "08-failure",
        "worker reported a process failure and left no partial destination",
        &mut steps,
    )?;

    app.invoke_select_job(1);
    app.invoke_retry_job();
    if !matches!(snapshot(&state).jobs[1].status, JobStatus::Retrying { .. }) {
        return Err("failed job did not enter retrying state".into());
    }
    app.set_status_left("Retrying failed job · attempt 1".into());
    refresh_journey(&app, &state);
    capture_encode_workflow_step(
        &app,
        &state,
        args.size,
        out_dir,
        "09-retry-failure",
        "explicit retry re-queued the failed job for another worker attempt",
        &mut steps,
    )?;
    app.invoke_start_queue();
    wait_for_queue_idle(&state, Duration::from_secs(12), "successful retry pass")?;
    let queue = snapshot(&state);
    let conformance = queue.conformance_report();
    if !conformance.passed
        || !queue
            .jobs
            .iter()
            .all(|job| matches!(job.status, JobStatus::Complete))
        || !failure_output.is_file()
        || has_encode_temporary_output(out_dir)?
    {
        return Err("successful retry did not produce a passing conformance report".into());
    }
    app.set_status_left("Queue complete · conformance passed".into());
    refresh_journey(&app, &state);
    capture_encode_workflow_step(
        &app,
        &state,
        args.size,
        out_dir,
        "10-complete",
        "successful retry produced complete outputs and a passing durable conformance report",
        &mut steps,
    )?;

    let palette = record_keyboard_palette_journey(&app, "encode", out_dir, "queue")
        .map_err(|error| format!("palette journey failed: {error}"))?;
    if !palette.passed {
        return Err("keyboard palette journey invariants failed".into());
    }
    let report_path = out_dir.join("encode-workflow.txt");
    let mut report = format!(
        "Encode controller lifecycle: PASS\nqueue={}\nproject={}\nfixture_encoder={}\npalette_journey_passed={}\n\n",
        queue.id,
        project_path.display(),
        backend.executable.display(),
        palette.passed
    );
    report.push_str(&steps.join("\n"));
    report.push_str("\n\nFinal conformance report:\n");
    report.push_str(&format!(
        "queue_id={} passed={} jobs={}\n",
        conformance.queue_id,
        conformance.passed,
        conformance.jobs.len()
    ));
    for job in &conformance.jobs {
        report.push_str(&format!(
            "job_id={} status={} passed={} output={} checks={}\n",
            job.job_id,
            job.status,
            job.passed,
            job.output_file,
            job.checks.join(", ")
        ));
    }
    report.push_str(
        "\nEvidence limits: the lifecycle uses a deterministic local fixture encoder and software-rendered Slint captures; native AppKit menu/file-panel delivery, realtime hardware codecs, and media semantic conformance are not claimed.\n",
    );
    std::fs::write(&report_path, report)
        .map_err(|error| format!("write journey report {}: {error}", report_path.display()))?;
    println!("encode workflow journey: PASS ({})", report_path.display());
    Ok(())
}

impl PaletteProbe for EncodeApp {
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

#[derive(Default)]
struct QueueHistory {
    undo: Vec<EncodeQueue>,
    redo: Vec<EncodeQueue>,
}

impl QueueHistory {
    fn checkpoint(&mut self, queue: &EncodeQueue) {
        self.undo.push(queue.clone());
        if self.undo.len() > HISTORY_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    fn undo(&mut self, queue: &mut EncodeQueue) -> bool {
        let Some(previous) = self.undo.pop() else {
            return false;
        };
        self.redo.push(queue.clone());
        *queue = previous;
        true
    }

    fn redo(&mut self, queue: &mut EncodeQueue) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo.push(queue.clone());
        *queue = next;
        true
    }
}

struct AppState {
    queue: Mutex<EncodeQueue>,
    history: Mutex<QueueHistory>,
    save_path: Mutex<Option<PathBuf>>,
    dialogs: Arc<dyn FileDialogService + Send + Sync>,
    backend: Option<EncoderBackend>,
    cancel: AtomicBool,
    running: AtomicBool,
}

fn snapshot(state: &AppState) -> EncodeQueue {
    state
        .queue
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

fn update_history_controls(app: &EncodeApp, state: &AppState) {
    let history = state
        .history
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let editable = !state.running.load(Ordering::Relaxed);
    app.set_can_undo(editable && !history.undo.is_empty());
    app.set_can_redo(editable && !history.redo.is_empty());
}

fn checkpoint_queue(state: &AppState, queue: &EncodeQueue) {
    state
        .history
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .checkpoint(queue);
}

fn post_refresh(weak: &slint::Weak<EncodeApp>, state: &Arc<AppState>, message: String) {
    let queue = snapshot(state);
    let backend = state.backend.clone();
    let running = state.running.load(Ordering::Relaxed);
    let state = state.clone();
    let _ = weak.upgrade_in_event_loop(move |app| {
        refresh(&app, &queue, backend.as_ref(), running);
        update_history_controls(&app, &state);
        app.set_status_left(message.into());
    });
}

fn wire_application(app: &EncodeApp, state: Arc<AppState>) {
    macro_rules! queue_callback {
        ($method:ident, $operation:expr) => {{
            let state = state.clone();
            let app_ref = app.as_weak();
            app.$method(move || {
                if let Some(app) = app_ref.upgrade() {
                    if state.running.load(Ordering::Acquire) {
                        app.set_status_left(
                            "Queue is running; stop it before changing jobs".into(),
                        );
                        return;
                    }
                    let mut queue = state
                        .queue
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    let before = queue.clone();
                    ($operation)(&mut queue);
                    let changed = before.queue_digest() != queue.queue_digest();
                    if changed {
                        checkpoint_queue(&state, &before);
                    }
                    refresh(
                        &app,
                        &queue,
                        state.backend.as_ref(),
                        state.running.load(Ordering::Relaxed),
                    );
                    update_history_controls(&app, &state);
                }
            });
        }};
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_new_queue(move || {
            if let Some(app) = app_ref.upgrade() {
                if state.running.load(Ordering::Acquire) {
                    app.set_status_left(
                        "Queue is running; stop it before creating a new queue".into(),
                    );
                    return;
                }
                *state
                    .queue
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()) = empty_queue();
                *state
                    .history
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()) = QueueHistory::default();
                *state
                    .save_path
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
                let queue = snapshot(&state);
                refresh(&app, &queue, state.backend.as_ref(), false);
                update_history_controls(&app, &state);
                app.set_status_left("Created a new untitled batch queue".into());
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_open_queue(move || {
            if let Some(app) = app_ref.upgrade() {
                if state.running.load(Ordering::Acquire) {
                    app.set_status_left(
                        "Queue is running; stop it before opening another queue".into(),
                    );
                    return;
                }
                let current_path = state
                    .save_path
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .clone();
                let request = open_queue_request(current_path.as_deref());
                match state.dialogs.open_file(&request) {
                    Ok(Some(path)) => {
                        match std::fs::read(&path)
                            .map_err(|error| error.to_string())
                            .and_then(|bytes| load_encode_queue(&bytes))
                        {
                            Ok(mut queue) => {
                                queue.recover_interrupted();
                                *state
                                    .queue
                                    .lock()
                                    .unwrap_or_else(|poisoned| poisoned.into_inner()) = queue;
                                *state
                                    .history
                                    .lock()
                                    .unwrap_or_else(|poisoned| poisoned.into_inner()) =
                                    QueueHistory::default();
                                *state
                                    .save_path
                                    .lock()
                                    .unwrap_or_else(|poisoned| poisoned.into_inner()) =
                                    Some(path.clone());
                                let queue = snapshot(&state);
                                refresh(&app, &queue, state.backend.as_ref(), false);
                                update_history_controls(&app, &state);
                                app.set_status_left(
                                    format!(
                                        "Opened {}",
                                        path.file_name().unwrap().to_string_lossy()
                                    )
                                    .into(),
                                );
                            }
                            Err(error) => {
                                app.set_status_left(format!("Open failed: {error}").into())
                            }
                        }
                    }
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
        app.on_save_queue(move || {
            if let Some(app) = app_ref.upgrade() {
                let current_path = state
                    .save_path
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .clone();
                let path_to_save = match current_path {
                    Some(p) => Some(p),
                    None => {
                        let req = save_queue_request(None);
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
                    let queue = snapshot(&state);
                    let result = save_encode_queue(&queue).and_then(|bytes| {
                        loom_storage::atomic_write(&path, &bytes)
                            .map_err(|error| error.to_string())
                            .and_then(|_| checkpoint_snapshot_recovery(bytes))
                    });
                    match result {
                        Ok(()) => {
                            *state
                                .save_path
                                .lock()
                                .unwrap_or_else(|poisoned| poisoned.into_inner()) =
                                Some(path.clone());
                            refresh(
                                &app,
                                &queue,
                                state.backend.as_ref(),
                                state.running.load(Ordering::Relaxed),
                            );
                            update_history_controls(&app, &state);
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
        app.on_save_as_queue(move || {
            if let Some(app) = app_ref.upgrade() {
                let current_path = state
                    .save_path
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .clone();
                let req = save_queue_request(current_path.as_deref());
                match state.dialogs.save_file(&req) {
                    Ok(Some(path)) => {
                        let queue = snapshot(&state);
                        let result = save_encode_queue(&queue).and_then(|bytes| {
                            loom_storage::atomic_write(&path, &bytes)
                                .map_err(|error| error.to_string())
                                .and_then(|_| checkpoint_snapshot_recovery(bytes))
                        });
                        match result {
                            Ok(()) => {
                                *state
                                    .save_path
                                    .lock()
                                    .unwrap_or_else(|poisoned| poisoned.into_inner()) =
                                    Some(path.clone());
                                refresh(
                                    &app,
                                    &queue,
                                    state.backend.as_ref(),
                                    state.running.load(Ordering::Relaxed),
                                );
                                update_history_controls(&app, &state);
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
    queue_callback!(on_add_job, |queue: &mut EncodeQueue| {
        let count = queue.jobs.len() + 1;
        queue.add_job(EncodeJob::new(
            format!("job-{count}"),
            format!("source-{count}.mov"),
            format!("output-{count}.mp4"),
            EncodePreset::h264_1080p(),
        ));
        let _ = queue.select_job(queue.jobs.len() - 1);
    });
    queue_callback!(on_remove_job, |queue: &mut EncodeQueue| {
        if !queue.jobs.is_empty() {
            let index = queue.active_job_index.min(queue.jobs.len() - 1);
            let _ = queue.remove_job(index);
        }
    });
    queue_callback!(on_retry_job, |queue: &mut EncodeQueue| {
        let _ = queue.retry_job(queue.active_job_index);
    });

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_move_job(move |from, to| {
            if let Some(app) = app_ref.upgrade() {
                if state.running.load(Ordering::Acquire) {
                    app.set_status_left("Queue is running; stop it before reordering jobs".into());
                    return;
                }
                if from < 0 || to < 0 {
                    return;
                }
                let mut queue = state
                    .queue
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                let before = queue.clone();
                let changed = queue.move_job(from as usize, to as usize);
                if changed {
                    checkpoint_queue(&state, &before);
                    refresh(
                        &app,
                        &queue,
                        state.backend.as_ref(),
                        state.running.load(Ordering::Relaxed),
                    );
                    update_history_controls(&app, &state);
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_select_job(move |index| {
            if let Some(app) = app_ref.upgrade() {
                if index >= 0 {
                    let mut queue = state
                        .queue
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    queue.select_job(index as usize);
                    refresh(
                        &app,
                        &queue,
                        state.backend.as_ref(),
                        state.running.load(Ordering::Relaxed),
                    );
                }
            }
        });
    }
    for field in ["source", "output", "preset"] {
        let state = state.clone();
        let app_ref = app.as_weak();
        match field {
            "source" => app.on_source_changed(move |value| {
                if let Some(app) = app_ref.upgrade() {
                    if state.running.load(Ordering::Acquire) {
                        app.set_status_left(
                            "Queue is running; stop it before changing source paths".into(),
                        );
                        return;
                    }
                    let mut queue = state
                        .queue
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    let index = queue.active_job_index;
                    let before = queue.clone();
                    if let Some(job) = queue.jobs.get_mut(index) {
                        job.set_source_file(value.as_str());
                    }
                    if before.queue_digest() != queue.queue_digest() {
                        checkpoint_queue(&state, &before);
                    }
                    refresh(
                        &app,
                        &queue,
                        state.backend.as_ref(),
                        state.running.load(Ordering::Relaxed),
                    );
                    update_history_controls(&app, &state);
                }
            }),
            "output" => app.on_output_changed(move |value| {
                if let Some(app) = app_ref.upgrade() {
                    if state.running.load(Ordering::Acquire) {
                        app.set_status_left(
                            "Queue is running; stop it before changing destination paths".into(),
                        );
                        return;
                    }
                    let mut queue = state
                        .queue
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    let index = queue.active_job_index;
                    let before = queue.clone();
                    if let Some(job) = queue.jobs.get_mut(index) {
                        job.set_output_file(value.as_str());
                    }
                    if before.queue_digest() != queue.queue_digest() {
                        checkpoint_queue(&state, &before);
                    }
                    refresh(
                        &app,
                        &queue,
                        state.backend.as_ref(),
                        state.running.load(Ordering::Relaxed),
                    );
                    update_history_controls(&app, &state);
                }
            }),
            _ => app.on_preset_changed(move |value| {
                if let Some(app) = app_ref.upgrade() {
                    if state.running.load(Ordering::Acquire) {
                        app.set_status_left(
                            "Queue is running; stop it before changing presets".into(),
                        );
                        return;
                    }
                    let mut queue = state
                        .queue
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    let index = queue.active_job_index;
                    let before = queue.clone();
                    if let Some(job) = queue.jobs.get_mut(index) {
                        let preset = match value.as_str() {
                            "ProRes 422" => EncodePreset::prores_master(),
                            "AV1 High" => av1_preset(),
                            _ => EncodePreset::h264_1080p(),
                        };
                        job.set_preset(preset);
                        let output = PathBuf::from(&job.output_file);
                        let stem = output
                            .file_stem()
                            .and_then(|stem| stem.to_str())
                            .unwrap_or("output");
                        job.output_file = output
                            .with_file_name(format!("{stem}.{}", job.preset.container))
                            .to_string_lossy()
                            .into_owned();
                    }
                    if before.queue_digest() != queue.queue_digest() {
                        checkpoint_queue(&state, &before);
                    }
                    refresh(
                        &app,
                        &queue,
                        state.backend.as_ref(),
                        state.running.load(Ordering::Relaxed),
                    );
                    update_history_controls(&app, &state);
                }
            }),
        }
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_choose_source(move || {
            if let Some(app) = app_ref.upgrade() {
                let current = snapshot(&state)
                    .jobs
                    .get(snapshot(&state).active_job_index)
                    .map(|job| PathBuf::from(&job.source_file));
                match state
                    .dialogs
                    .open_file(&source_file_request(current.as_deref()))
                {
                    Ok(Some(path)) => {
                        set_selected_source_path(&app, &state, path.to_string_lossy())
                    }
                    Ok(None) => app.set_status_left("Source selection cancelled".into()),
                    Err(error) => {
                        app.set_status_left(format!("Source dialog failed: {error}").into())
                    }
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_choose_output(move || {
            if let Some(app) = app_ref.upgrade() {
                let queue = snapshot(&state);
                let current_job = queue.jobs.get(queue.active_job_index);
                let current = current_job.map(|job| PathBuf::from(&job.output_file));
                match state.dialogs.save_file(&output_file_request(
                    current.as_deref(),
                    current_job.map(|job| &job.preset),
                )) {
                    Ok(Some(path)) => {
                        set_selected_output_path(&app, &state, path.to_string_lossy())
                    }
                    Ok(None) => app.set_status_left("Destination selection cancelled".into()),
                    Err(error) => {
                        app.set_status_left(format!("Destination dialog failed: {error}").into())
                    }
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_choose_subtitles(move || {
            if let Some(app) = app_ref.upgrade() {
                let queue = snapshot(&state);
                let current = queue
                    .jobs
                    .get(queue.active_job_index)
                    .and_then(|job| job.subtitles.path.as_deref())
                    .map(PathBuf::from);
                match state
                    .dialogs
                    .open_file(&subtitle_file_request(current.as_deref()))
                {
                    Ok(Some(path)) => {
                        set_selected_subtitle_path(&app, &state, path.to_string_lossy())
                    }
                    Ok(None) => app.set_status_left("Subtitle selection cancelled".into()),
                    Err(error) => {
                        app.set_status_left(format!("Subtitle dialog failed: {error}").into())
                    }
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_source_dropped(move |data| {
            if let Some(app) = app_ref.upgrade() {
                match dropped_path(&data) {
                    Some(path) => set_selected_source_path(&app, &state, path),
                    None => {
                        app.set_status_left("Source drop rejected · expected a file path".into())
                    }
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_output_dropped(move |data| {
            if let Some(app) = app_ref.upgrade() {
                match dropped_path(&data) {
                    Some(path) => set_selected_output_path(&app, &state, path),
                    None => app
                        .set_status_left("Destination drop rejected · expected a file path".into()),
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_audio_codec_changed(move |codec| {
            if let Some(app) = app_ref.upgrade() {
                if state.running.load(Ordering::Relaxed) {
                    app.set_status_left(
                        "Queue is running; stop it before changing audio codec".into(),
                    );
                    return;
                }
                let mut queue = state
                    .queue
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                let before = queue.clone();
                let index = queue.active_job_index;
                let result = queue
                    .jobs
                    .get_mut(index)
                    .ok_or_else(|| "no job selected".to_string())
                    .and_then(|job| job.set_audio_codec(codec.to_string()));
                if let Err(error) = result {
                    app.set_status_left(format!("Audio codec rejected: {error}").into());
                    return;
                }
                if before.queue_digest() != queue.queue_digest() {
                    checkpoint_queue(&state, &before);
                }
                refresh(&app, &queue, state.backend.as_ref(), false);
                update_history_controls(&app, &state);
                app.set_status_left(format!("Audio codec set to {codec}").into());
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_subtitle_mode_changed(move |mode| {
            if let Some(app) = app_ref.upgrade() {
                if state.running.load(Ordering::Relaxed) {
                    app.set_status_left(
                        "Queue is running; stop it before changing subtitle mode".into(),
                    );
                    return;
                }
                let Some(mode) = (match mode {
                    0 => Some(SubtitleMode::None),
                    1 => Some(SubtitleMode::BurnIn),
                    2 => Some(SubtitleMode::PassthroughCopy),
                    3 => Some(SubtitleMode::ConvertSrt),
                    _ => None,
                }) else {
                    app.set_status_left("Subtitle mode rejected".into());
                    return;
                };
                let mut queue = state
                    .queue
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                let before = queue.clone();
                let index = queue.active_job_index;
                if let Some(job) = queue.jobs.get_mut(index) {
                    job.set_subtitle_mode(mode);
                }
                if before.queue_digest() != queue.queue_digest() {
                    checkpoint_queue(&state, &before);
                }
                refresh(&app, &queue, state.backend.as_ref(), false);
                update_history_controls(&app, &state);
                app.set_status_left("Subtitle mode updated · queue ready".into());
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_metadata_copy_changed(move |copy| {
            if let Some(app) = app_ref.upgrade() {
                if state.running.load(Ordering::Relaxed) {
                    app.set_status_left(
                        "Queue is running; stop it before changing metadata".into(),
                    );
                    return;
                }
                let mut queue = state
                    .queue
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                let before = queue.clone();
                let index = queue.active_job_index;
                if let Some(job) = queue.jobs.get_mut(index) {
                    job.set_metadata_copy(copy);
                }
                if before.queue_digest() != queue.queue_digest() {
                    checkpoint_queue(&state, &before);
                }
                refresh(&app, &queue, state.backend.as_ref(), false);
                update_history_controls(&app, &state);
                app.set_status_left(
                    if copy {
                        "Source metadata will be copied"
                    } else {
                        "Source metadata will be dropped"
                    }
                    .into(),
                );
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_collision_policy_changed(move |policy| {
            if let Some(app) = app_ref.upgrade() {
                if state.running.load(Ordering::Relaxed) {
                    app.set_status_left(
                        "Queue is running; stop it before changing collision policy".into(),
                    );
                    return;
                }
                let Some(policy) = (match policy {
                    0 => Some(DestinationCollisionPolicy::Fail),
                    1 => Some(DestinationCollisionPolicy::Rename),
                    2 => Some(DestinationCollisionPolicy::Overwrite),
                    _ => None,
                }) else {
                    app.set_status_left("Collision policy rejected".into());
                    return;
                };
                let mut queue = state
                    .queue
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                let before = queue.clone();
                let index = queue.active_job_index;
                if let Some(job) = queue.jobs.get_mut(index) {
                    job.set_collision_policy(policy);
                }
                if before.queue_digest() != queue.queue_digest() {
                    checkpoint_queue(&state, &before);
                }
                refresh(&app, &queue, state.backend.as_ref(), false);
                update_history_controls(&app, &state);
                app.set_status_left("Destination collision policy updated".into());
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_undo(move || {
            if let Some(app) = app_ref.upgrade() {
                if state.running.load(Ordering::Relaxed) {
                    return;
                }
                let changed = {
                    let mut queue = state
                        .queue
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    state
                        .history
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .undo(&mut queue)
                };
                if changed {
                    let queue = snapshot(&state);
                    refresh(&app, &queue, state.backend.as_ref(), false);
                    update_history_controls(&app, &state);
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_redo(move || {
            if let Some(app) = app_ref.upgrade() {
                if state.running.load(Ordering::Relaxed) {
                    return;
                }
                let changed = {
                    let mut queue = state
                        .queue
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    state
                        .history
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .redo(&mut queue)
                };
                if changed {
                    let queue = snapshot(&state);
                    refresh(&app, &queue, state.backend.as_ref(), false);
                    update_history_controls(&app, &state);
                }
            }
        });
    }

    {
        let state = state.clone();
        let weak = app.as_weak();
        app.on_start_queue(move || {
            if state.backend.is_none() || state.running.swap(true, Ordering::SeqCst) {
                return;
            }
            {
                let mut queue = state
                    .queue
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                let before = queue.clone();
                if let Err(error) = queue.apply_destination_policies() {
                    state.running.store(false, Ordering::SeqCst);
                    let _ = weak.upgrade_in_event_loop(move |app| {
                        app.set_status_left(format!("Queue blocked: {error}").into());
                    });
                    return;
                }
                if before.queue_digest() != queue.queue_digest() {
                    checkpoint_queue(&state, &before);
                }
            }
            state.cancel.store(false, Ordering::SeqCst);
            let state = state.clone();
            let weak = weak.clone();
            std::thread::spawn(move || {
                let backend = state.backend.clone().expect("checked before worker start");
                post_refresh(
                    &weak,
                    &state,
                    format!("Started local queue with {}", backend.version),
                );
                loop {
                    if state.cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    let next = {
                        state
                            .queue
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner())
                            .next_queued_index()
                    };
                    let Some(index) = next else {
                        break;
                    };
                    let mut job = {
                        state
                            .queue
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner())
                            .jobs[index]
                            .clone()
                    };
                    let plan = match job.plan(
                        &backend,
                        ExecutionPolicy {
                            overwrite: matches!(
                                job.destination.collision_policy,
                                DestinationCollisionPolicy::Overwrite
                            ),
                            create_parent_directories: true,
                        },
                    ) {
                        Ok(plan) => plan,
                        Err(error) => {
                            job.status = JobStatus::Failed(error.to_string());
                            state
                                .queue
                                .lock()
                                .unwrap_or_else(|poisoned| poisoned.into_inner())
                                .jobs[index] = job;
                            post_refresh(
                                &weak,
                                &state,
                                format!("Job {} failed validation: {error}", index + 1),
                            );
                            continue;
                        }
                    };
                    let duration = probe_duration(&backend, &plan.input).ok().flatten();
                    let progress_state = state.clone();
                    let progress_weak = weak.clone();
                    let result = execute_job_with_cancel(
                        &mut job,
                        &plan,
                        duration,
                        &state.cancel,
                        move |progress| {
                            let snapshot = {
                                let mut queue = progress_state
                                    .queue
                                    .lock()
                                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                                if let Some(active) = queue.jobs.get_mut(index) {
                                    active.status = JobStatus::Encoding { progress };
                                }
                                queue.clone()
                            };
                            let backend = progress_state.backend.clone();
                            let _ = progress_weak.upgrade_in_event_loop(move |app| {
                                refresh(&app, &snapshot, backend.as_ref(), true);
                                app.set_status_left(
                                    format!(
                                        "Encoding job {} · {:.0}%",
                                        index + 1,
                                        progress * 100.0
                                    )
                                    .into(),
                                );
                            });
                        },
                    );
                    state
                        .queue
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .jobs[index] = job;
                    post_refresh(
                        &weak,
                        &state,
                        match result {
                            Ok(()) => format!("Completed job {}", index + 1),
                            Err(error) => format!("Job {} stopped: {error}", index + 1),
                        },
                    );
                    if state.cancel.load(Ordering::Relaxed) {
                        break;
                    }
                }
                state.running.store(false, Ordering::SeqCst);
                post_refresh(
                    &weak,
                    &state,
                    if state.cancel.load(Ordering::Relaxed) {
                        "Queue cancelled".into()
                    } else {
                        "Queue finished".into()
                    },
                );
            });
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_cancel_queue(move || {
            state.cancel.store(true, Ordering::SeqCst);
            if let Some(app) = app_ref.upgrade() {
                app.set_status_left("Cancelling active encode...".into());
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
            std::env::temp_dir().join(format!("loom-encode-smoke-{}.png", std::process::id()));
        return render_headless(&args, &output.to_string_lossy());
    }
    if let Some(out_dir) = &args.journey {
        return run_journey(&args, out_dir);
    }

    let app = EncodeApp::new().map_err(|error| error.to_string())?;
    configure_direction(&app, args.rtl);
    apply_theme(&app, &args.theme);
    app.window()
        .set_size(PhysicalSize::new(args.size.0, args.size.1));
    configure_responsive_layout(&app, args.size);
    wire_responsive_layout(&app);
    let backend = discover_ffmpeg(&[]).ok();
    let recovered = initialize_snapshot_recovery()?;
    let (mut initial, save_path) = if args.open.is_some() {
        initial_queue(&args)?
    } else {
        match recovered
            .as_deref()
            .and_then(|bytes| load_encode_queue(bytes).ok())
        {
            Some(queue) => (queue, None),
            None => initial_queue(&args)?,
        }
    };
    initial.recover_interrupted();
    let state = Arc::new(AppState {
        queue: Mutex::new(initial),
        history: Mutex::new(QueueHistory::default()),
        save_path: Mutex::new(save_path),
        dialogs: Arc::new(NativeFileDialogs),
        backend,
        cancel: AtomicBool::new(false),
        running: AtomicBool::new(false),
    });

    wire_application(&app, state.clone());
    let menu_bar = build_standard_menu_bar(
        "Loom Encode",
        vec![],
        vec![],
        vec![MenuItem::check("view.inspector", "Inspector", true)],
        vec![Menu::new(
            "Queue",
            vec![
                MenuItem::action_with_shortcut(
                    "queue.add_job",
                    "Add Transcode Job",
                    MenuShortcut::primary_shift("N"),
                ),
                MenuItem::action("queue.remove_job", "Remove Selected Job"),
                MenuItem::action_with_shortcut(
                    "queue.start",
                    "Start Queue",
                    MenuShortcut::primary("R"),
                ),
                MenuItem::action("queue.cancel", "Cancel Queue"),
            ],
        )],
    );
    let menu_service = NativeMenuBar::new();
    let _ = menu_service.install_menu_bar(&menu_bar);

    wire_palette(&app);
    let queue = snapshot(&state);
    refresh(&app, &queue, state.backend.as_ref(), false);
    update_history_controls(&app, &state);
    app.set_status_left("Queue ready · double click a source to edit".into());
    app.show().map_err(|error| error.to_string())?;
    slint::run_event_loop().map_err(|error| error.to_string())
}

/// Commands exposed through the command palette.
#[derive(Debug, Clone)]
enum PaletteAction {
    NewQueue,
    OpenQueue,
    SaveQueue,
    SaveAsQueue,
    Undo,
    Redo,
    AddJob,
    RemoveJob,
    StartQueue,
    CancelQueue,
    RetryJob,
}

struct PaletteCommand {
    action: PaletteAction,
    id: &'static str,
    label: &'static str,
    shortcut: &'static str,
}

fn master_palette(app: &EncodeApp) -> Vec<PaletteCommand> {
    vec![
        PaletteCommand {
            action: PaletteAction::NewQueue,
            id: "encode.new",
            label: "New Queue",
            shortcut: "Ctrl+N",
        },
        PaletteCommand {
            action: PaletteAction::OpenQueue,
            id: "encode.open",
            label: "Open Queue",
            shortcut: "Ctrl+O",
        },
        PaletteCommand {
            action: PaletteAction::SaveQueue,
            id: "encode.save",
            label: "Save Queue",
            shortcut: "Ctrl+S",
        },
        PaletteCommand {
            action: PaletteAction::SaveAsQueue,
            id: "encode.save-as",
            label: "Save Queue As",
            shortcut: "Ctrl+Shift+S",
        },
        PaletteCommand {
            action: PaletteAction::Undo,
            id: "encode.undo",
            label: "Undo",
            shortcut: "Ctrl+Z",
        },
        PaletteCommand {
            action: PaletteAction::Redo,
            id: "encode.redo",
            label: "Redo",
            shortcut: "Ctrl+Shift+Z",
        },
        PaletteCommand {
            action: PaletteAction::AddJob,
            id: "encode.add-job",
            label: "Add Job",
            shortcut: "Ctrl+J",
        },
        PaletteCommand {
            action: PaletteAction::RemoveJob,
            id: "encode.remove-job",
            label: "Remove Job",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::StartQueue,
            id: "encode.start",
            label: "Start Queue",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::CancelQueue,
            id: "encode.cancel",
            label: "Cancel Queue",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::RetryJob,
            id: "encode.retry",
            label: "Retry Job",
            shortcut: "",
        },
    ]
    .into_iter()
    .filter(|c| match c.action {
        PaletteAction::Undo => app.get_can_undo(),
        PaletteAction::Redo => app.get_can_redo(),
        PaletteAction::RetryJob => app.get_can_retry(),
        _ => true,
    })
    .collect()
}

fn rebuild_palette(app: &EncodeApp, query: &str) {
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

fn wire_palette(app: &EncodeApp) {
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
                        PaletteAction::RetryJob => app.get_can_retry(),
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
                        PaletteAction::NewQueue => app.invoke_new_queue(),
                        PaletteAction::OpenQueue => app.invoke_open_queue(),
                        PaletteAction::SaveQueue => app.invoke_save_queue(),
                        PaletteAction::SaveAsQueue => app.invoke_save_as_queue(),
                        PaletteAction::Undo => app.invoke_undo(),
                        PaletteAction::Redo => app.invoke_redo(),
                        PaletteAction::AddJob => app.invoke_add_job(),
                        PaletteAction::RemoveJob => app.invoke_remove_job(),
                        PaletteAction::StartQueue => app.invoke_start_queue(),
                        PaletteAction::CancelQueue => app.invoke_cancel_queue(),
                        PaletteAction::RetryJob => app.invoke_retry_job(),
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

    fn test_app_and_state(scripted: ScriptedFileDialogs) -> (EncodeApp, Arc<AppState>) {
        set_platform();
        let app = EncodeApp::new().expect("create EncodeApp");
        let queue = sample_queue();
        let state = Arc::new(AppState {
            queue: Mutex::new(queue.clone()),
            history: Mutex::new(QueueHistory::default()),
            save_path: Mutex::new(None),
            dialogs: Arc::new(scripted),
            backend: None,
            cancel: AtomicBool::new(false),
            running: AtomicBool::new(false),
        });
        wire_application(&app, state.clone());
        refresh(&app, &queue, None, false);
        update_history_controls(&app, &state);
        (app, state)
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
