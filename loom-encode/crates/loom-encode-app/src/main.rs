//! Loom Encode desktop batch transcoding application.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use loom_encode_core::{
    discover_ffmpeg, execute_job_with_cancel, load_encode_queue, probe_duration, save_encode_queue,
    EncodeJob, EncodePreset, EncodeQueue, EncoderBackend, ExecutionPolicy, JobStatus,
};
use loom_test_support::capture::{set_platform, snapshot_component};
use slint::{ComponentHandle, ModelRc, PhysicalSize, SharedString, VecModel};

slint::include_modules!();

const DEFAULT_SIZE: (u32, u32) = (1280, 800);
const SAVE_FILENAME: &str = "batch.loomencode";

loom_production::define_snapshot_recovery!(ENCODE_RECOVERY, "org.loom.encode", "loom.encode/1");

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
            other if !other.starts_with('-') && args.open.is_none() => {
                args.open = Some(other.to_string());
            }

            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(args)
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

fn initial_queue(args: &Args) -> Result<EncodeQueue, String> {
    match args.open.as_deref() {
        Some(path) => std::fs::read(path)
            .map_err(|error| format!("failed to read encode queue '{path}': {error}"))
            .and_then(|bytes| load_encode_queue(&bytes)),
        None => Ok(sample_queue()),
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
    let queue = initial_queue(args)?;
    let backend = discover_ffmpeg(&[]).ok();
    refresh(&app, &queue, backend.as_ref(), false);
    app.set_status_left("Queue ready · source paths are editable".into());
    let image = snapshot_component(&app, args.size.0 as f32, args.size.1 as f32, 1.0)
        .map_err(|error| error.to_string())?;
    loom_test_support::png::save_png(Path::new(output), &image).map_err(|error| error.to_string())
}

struct AppState {
    queue: Mutex<EncodeQueue>,
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

fn post_refresh(weak: &slint::Weak<EncodeApp>, state: &Arc<AppState>, message: String) {
    let queue = snapshot(state);
    let backend = state.backend.clone();
    let running = state.running.load(Ordering::Relaxed);
    let _ = weak.upgrade_in_event_loop(move |app| {
        refresh(&app, &queue, backend.as_ref(), running);
        app.set_status_left(message.into());
    });
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

    let app = EncodeApp::new().map_err(|error| error.to_string())?;
    apply_theme(&app, &args.theme);
    app.window()
        .set_size(PhysicalSize::new(args.size.0, args.size.1));
    let backend = discover_ffmpeg(&[]).ok();
    let recovered = initialize_snapshot_recovery()?;
    let mut initial = if args.open.is_some() {
        initial_queue(&args)?
    } else {
        recovered
            .as_deref()
            .and_then(|bytes| load_encode_queue(bytes).ok())
            .unwrap_or(initial_queue(&args)?)
    };
    initial.recover_interrupted();
    let state = Arc::new(AppState {
        queue: Mutex::new(initial),
        backend,
        cancel: AtomicBool::new(false),
        running: AtomicBool::new(false),
    });

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
                    ($operation)(&mut queue);
                    refresh(
                        &app,
                        &queue,
                        state.backend.as_ref(),
                        state.running.load(Ordering::Relaxed),
                    );
                }
            });
        }};
    }

    queue_callback!(on_new_queue, |queue: &mut EncodeQueue| {
        *queue = sample_queue();
    });
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_open_queue(move || {
            if let Some(app) = app_ref.upgrade() {
                match std::fs::read(SAVE_FILENAME)
                    .map_err(|error| error.to_string())
                    .and_then(|bytes| load_encode_queue(&bytes))
                {
                    Ok(mut queue) => {
                        queue.recover_interrupted();
                        *state
                            .queue
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner()) = queue;
                        let queue = snapshot(&state);
                        refresh(&app, &queue, state.backend.as_ref(), false);
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
        app.on_save_queue(move || {
            if let Some(app) = app_ref.upgrade() {
                let result = save_encode_queue(&snapshot(&state)).and_then(|bytes| {
                    std::fs::write(SAVE_FILENAME, &bytes)
                        .map_err(|error| error.to_string())
                        .and_then(|_| checkpoint_snapshot_recovery(bytes))
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
                }
            }),
            "output" => app.on_output_changed(move |value| {
                if let Some(app) = app_ref.upgrade() {
                    let mut queue = state
                        .queue
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    let index = queue.active_job_index;
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
                }
            }),
            _ => app.on_preset_changed(move |value| {
                if let Some(app) = app_ref.upgrade() {
                    let mut queue = state
                        .queue
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    let index = queue.active_job_index;
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
                }
            }),
        }
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
            if let Some(app) = app_ref.upgrade() {
                state.cancel.store(true, Ordering::SeqCst);
                app.set_status_left("Cancelling active encoder process…".into());
            }
        });
    }

    let queue = snapshot(&state);
    refresh(&app, &queue, state.backend.as_ref(), false);
    app.set_status_left("Edit a source/output path, then start the local queue".into());
    app.show().map_err(|error| error.to_string())?;
    slint::run_event_loop().map_err(|error| error.to_string())
}
