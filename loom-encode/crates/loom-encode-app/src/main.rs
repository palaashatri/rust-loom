//! Loom Encode desktop batch transcoding application.

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use loom_desktop::{
    FileDialogService, FileFilter, NativeFileDialogs, OpenFileRequest, SaveFileRequest,
};
use loom_encode_core::{
    discover_ffmpeg, execute_job_with_cancel, load_encode_queue, probe_duration, save_encode_queue,
    EncodeJob, EncodePreset, EncodeQueue, EncoderBackend, ExecutionPolicy, JobStatus,
};
use loom_test_support::capture::{set_platform, snapshot_component};
use loom_test_support::journey::{record_keyboard_palette_journey, PaletteProbe};
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

fn open_queue_request(save_path: Option<&Path>) -> OpenFileRequest {
    OpenFileRequest {
        title: "Open Encode Queue".into(),
        initial_directory: save_path.and_then(Path::parent).map(Path::to_path_buf),
        suggested_name: None,
        filters: vec![encode_filter()],
    }
}

fn save_queue_request(save_path: Option<&Path>) -> SaveFileRequest {
    SaveFileRequest {
        title: "Save Encode Queue".into(),
        initial_directory: save_path.and_then(Path::parent).map(Path::to_path_buf),
        suggested_name: save_path
            .and_then(Path::file_name)
            .map(|n| n.to_string_lossy().into_owned())
            .or_else(|| Some("Untitled.loomencode".into())),
        filters: vec![encode_filter()],
    }
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
            .map(|job| {
                SharedString::from(match &job.status {
                    JobStatus::Queued => "QUEUED".into(),
                    JobStatus::Encoding { progress } => {
                        format!("ENCODING {:.0}%", progress * 100.0)
                    }
                    JobStatus::Complete => "COMPLETE".into(),
                    JobStatus::Failed(reason) => format!("FAILED · {reason}"),
                })
            })
            .collect::<Vec<_>>(),
    )));
    app.set_job_progresses(ModelRc::new(VecModel::from(
        queue
            .jobs
            .iter()
            .map(|job| match job.status {
                JobStatus::Queued | JobStatus::Failed(_) => 0.0,
                JobStatus::Encoding { progress } => progress * 100.0,
                JobStatus::Complete => 100.0,
            })
            .collect::<Vec<_>>(),
    )));
    app.set_job_tones(ModelRc::new(VecModel::from(
        queue
            .jobs
            .iter()
            .map(|job| match job.status {
                JobStatus::Complete => 1,
                JobStatus::Failed(_) => 2,
                JobStatus::Encoding { .. } => 3,
                JobStatus::Queued => 0,
            })
            .collect::<Vec<_>>(),
    )));
    app.set_job_running(ModelRc::new(VecModel::from(
        queue
            .jobs
            .iter()
            .map(|job| matches!(job.status, JobStatus::Encoding { .. }))
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
        app.set_active_job_progress(match job.status {
            JobStatus::Queued | JobStatus::Failed(_) => 0.0,
            JobStatus::Encoding { progress } => progress * 100.0,
            JobStatus::Complete => 100.0,
        });
    } else {
        app.set_selected_job_text("No job selected".into());
        app.set_selected_job_details("".into());
        app.set_selected_source("".into());
        app.set_selected_output("".into());
        app.set_active_job_progress(0.0);
    }
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

fn apply_theme(app: &EncodeApp, theme: &str) {
    Theme::get(app).set_active_theme(theme.into());
}

fn render_headless(args: &Args, output: &str) -> Result<(), String> {
    set_platform();
    let app = EncodeApp::new().map_err(|error| error.to_string())?;
    apply_theme(&app, &args.theme);
    let (queue, _) = initial_queue(args)?;
    let backend = discover_ffmpeg(&[]).ok();
    refresh(&app, &queue, backend.as_ref(), false);
    if args.palette {
        app.set_palette_query(SharedString::from("qu"));
        rebuild_palette(&app, "qu");
        app.set_palette_selected(1);
        app.set_palette_open(true);
    }
    app.set_status_left("Queue ready · source paths are editable".into());
    let image = snapshot_component(&app, args.size.0 as f32, args.size.1 as f32, 1.0)
        .map_err(|error| error.to_string())?;
    loom_test_support::png::save_png(Path::new(output), &image).map_err(|error| error.to_string())
}

/// Record the keyboard command-palette journey with per-step screenshots.
fn run_journey(args: &Args, out_dir: &str) -> Result<(), String> {
    set_platform();
    let app = EncodeApp::new().map_err(|error| error.to_string())?;
    apply_theme(&app, &args.theme);
    let (queue, _) = initial_queue(args)?;
    let backend = discover_ffmpeg(&[]).ok();
    refresh(&app, &queue, backend.as_ref(), false);
    wire_palette(&app);
    rebuild_palette(&app, "");
    app.window()
        .set_size(PhysicalSize::new(args.size.0, args.size.1));
    let report = record_keyboard_palette_journey(&app, "encode", Path::new(out_dir), "queue")
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
                    let mut queue = state
                        .queue
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    checkpoint_queue(&state, &queue);
                    ($operation)(&mut queue);
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
        queue.active_job_index = queue.jobs.len() - 1;
    });
    queue_callback!(on_remove_job, |queue: &mut EncodeQueue| {
        if !queue.jobs.is_empty() {
            queue
                .jobs
                .remove(queue.active_job_index.min(queue.jobs.len() - 1));
            queue.active_job_index = queue
                .active_job_index
                .min(queue.jobs.len().saturating_sub(1));
        }
    });
    queue_callback!(on_retry_job, |queue: &mut EncodeQueue| {
        if let Some(job) = queue.jobs.get_mut(queue.active_job_index) {
            job.status = JobStatus::Queued;
        }
    });

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
                    let mut queue = state
                        .queue
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    let index = queue.active_job_index;
                    checkpoint_queue(&state, &queue);
                    if let Some(job) = queue.jobs.get_mut(index) {
                        job.source_file = value.as_str().to_string();
                        job.status = JobStatus::Queued;
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
                    let mut queue = state
                        .queue
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    let index = queue.active_job_index;
                    checkpoint_queue(&state, &queue);
                    if let Some(job) = queue.jobs.get_mut(index) {
                        job.output_file = value.as_str().to_string();
                        job.status = JobStatus::Queued;
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
                    let mut queue = state
                        .queue
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    let index = queue.active_job_index;
                    checkpoint_queue(&state, &queue);
                    if let Some(job) = queue.jobs.get_mut(index) {
                        job.preset = match value.as_str() {
                            "ProRes 422" => EncodePreset::prores_master(),
                            "AV1 High" => av1_preset(),
                            _ => EncodePreset::h264_1080p(),
                        };
                        let output = PathBuf::from(&job.output_file);
                        let stem = output
                            .file_stem()
                            .and_then(|stem| stem.to_str())
                            .unwrap_or("output");
                        job.output_file = output
                            .with_file_name(format!("{stem}.{}", job.preset.container))
                            .to_string_lossy()
                            .into_owned();
                        job.status = JobStatus::Queued;
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
                            overwrite: true,
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
    apply_theme(&app, &args.theme);
    app.window()
        .set_size(PhysicalSize::new(args.size.0, args.size.1));
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
}
