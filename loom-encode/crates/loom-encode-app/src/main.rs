//! Loom Encode desktop batch media transcoding application.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use loom_encode_core::{
    load_encode_queue, save_encode_queue, EncodeJob, EncodePreset, EncodeQueue, JobStatus,
};
use loom_test_support::capture::{set_platform, snapshot_component};
use slint::{ComponentHandle, ModelRc, PhysicalSize, SharedString, VecModel};

slint::include_modules!();

const DEFAULT_SIZE: (u32, u32) = (1280, 800);
const SAVE_FILENAME: &str = "batch.loomencode";

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
        theme: "dark".to_string(),
        open: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--screenshot" => {
                args.screenshot = Some(it.next().ok_or("--screenshot needs a path")?);
            }
            "--smoke" => args.smoke = true,
            "--size" => {
                let v = it.next().ok_or("--size needs WxH")?;
                let (w, h) = v.split_once('x').ok_or("--size must be WxH")?;
                args.size = (
                    w.parse().map_err(|_| "bad width")?,
                    h.parse().map_err(|_| "bad height")?,
                );
            }
            "--theme" => {
                args.theme = it.next().ok_or("--theme needs a name")?;
            }
            "--open" => {
                args.open = Some(it.next().ok_or("--open needs a path")?);
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(args)
}

fn sample_encode() -> EncodeQueue {
    let mut q = EncodeQueue::new("encode-sample", "Web & Broadcast Presets");
    q.add_job(EncodeJob::new(
        "j2",
        "Documentary_Final.mov",
        "Documentary_Final_H264.mp4",
        EncodePreset::h264_1080p(),
    ));
    q
}

fn initial_encode(args: &Args) -> Result<EncodeQueue, String> {
    match args.open.as_deref() {
        Some(path) => {
            let bytes = std::fs::read(path)
                .map_err(|e| format!("failed to read encode queue '{path}': {e}"))?;
            load_encode_queue(&bytes)
                .map_err(|e| format!("failed to load encode queue '{path}': {e}"))
        }
        None => Ok(sample_encode()),
    }
}

fn apply_encode(app: &EncodeApp, q: &EncodeQueue) {
    app.set_queue_name(q.name.as_str().into());
    let job_labels: Vec<SharedString> = q
        .jobs
        .iter()
        .enumerate()
        .map(|(idx, job)| SharedString::from(format!("Job {}: {}", idx + 1, job.source_file)))
        .collect();
    let job_details: Vec<SharedString> = q
        .jobs
        .iter()
        .map(|job| {
            SharedString::from(format!(
                "Preset: {} ({} kbps) -> {}",
                job.preset.name, job.preset.bitrate_kbps, job.output_file
            ))
        })
        .collect();
    let job_statuses: Vec<SharedString> = q
        .jobs
        .iter()
        .map(|job| {
            let status = match &job.status {
                JobStatus::Queued => "QUEUED".to_string(),
                JobStatus::Encoding { progress } => format!("ENCODING ({:.0}%)", progress * 100.0),
                JobStatus::Complete => "COMPLETE".to_string(),
                JobStatus::Failed(reason) => format!("FAILED: {reason}"),
            };
            SharedString::from(status)
        })
        .collect();
    let job_progresses: Vec<f32> = q
        .jobs
        .iter()
        .map(|job| match &job.status {
            JobStatus::Queued => 0.0,
            JobStatus::Encoding { progress } => progress * 100.0,
            JobStatus::Complete => 100.0,
            JobStatus::Failed(_) => 0.0,
        })
        .collect();

    app.set_job_labels(ModelRc::new(VecModel::from(job_labels)));
    app.set_job_details(ModelRc::new(VecModel::from(job_details)));
    app.set_job_statuses(ModelRc::new(VecModel::from(job_statuses)));
    app.set_job_progresses(ModelRc::new(VecModel::from(job_progresses)));
    app.set_active_job_index(q.active_job_index as i32);

    let progress_for = |job: &loom_encode_core::EncodeJob| match &job.status {
        JobStatus::Queued => 0.0,
        JobStatus::Encoding { progress } => (*progress).clamp(0.0, 1.0) * 100.0,
        JobStatus::Complete => 100.0,
        JobStatus::Failed(_) => 0.0,
    };
    let batch_progress = if q.jobs.is_empty() {
        0.0
    } else {
        q.jobs.iter().map(progress_for).sum::<f32>() / q.jobs.len() as f32
    };
    let active_progress = q.jobs.get(q.active_job_index).map(progress_for).unwrap_or(0.0);
    app.set_batch_progress(batch_progress);
    app.set_active_job_progress(active_progress);

    let (selected_job_text, selected_job_details, preset_name) = q
        .jobs
        .get(q.active_job_index)
        .map(|job| {
            (
                format!("Selected: {}", job.source_file),
                format!("{} • {} kbps", job.preset.name, job.preset.bitrate_kbps),
                job.preset.name.clone(),
            )
        })
        .unwrap_or_else(|| ("No job selected".to_string(), String::new(), "Web 1080p".to_string()));

    app.set_selected_job_text(selected_job_text.into());
    app.set_selected_job_details(selected_job_details.into());
    app.set_selected_preset(match preset_name.as_str() {
        p if p.contains("ProRes") => "ProRes 422".into(),
        p if p.contains("AV1") => "AV1 High".into(),
        p if p.contains("MP4") => "MP4 H.264".into(),
        _ => "Web 1080p".into(),
    });

    app.set_status_left(SharedString::from(format!("{} jobs queued", q.jobs.len())));
    app.set_status_right("Offline".into());
}

fn apply_theme(app: &EncodeApp, theme: &str) {
    Theme::get(app).set_active_theme(SharedString::from(theme));
}

fn render_headless(args: &Args, out: &str) -> Result<(), String> {
    set_platform();
    let app = EncodeApp::new().map_err(|e| e.to_string())?;
    apply_theme(&app, &args.theme);
    let q = initial_encode(args)?;
    apply_encode(&app, &q);
    let (w, h) = args.size;
    let img = snapshot_component(&app, w as f32, h as f32, 1.0).map_err(|e| e.to_string())?;
    loom_test_support::png::save_png(Path::new(out), &img).map_err(|e| e.to_string())?;
    Ok(())
}

struct GuiState {
    current: RefCell<EncodeQueue>,
}

fn main() -> Result<(), String> {
    let args = parse_args()?;
    if let Some(out) = &args.screenshot {
        return render_headless(&args, out);
    }
    if args.smoke {
        let out =
            std::env::temp_dir().join(format!("loom-encode-smoke-{}.png", std::process::id()));
        let out = out.to_string_lossy().into_owned();
        return render_headless(&args, &out);
    }

    let app = EncodeApp::new().map_err(|e| e.to_string())?;
    apply_theme(&app, &args.theme);
    app.window()
        .set_size(PhysicalSize::new(args.size.0, args.size.1));

    let state = Rc::new(GuiState {
        current: RefCell::new(initial_encode(&args)?),
    });

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_new_queue(move || {
            if let Some(app) = app_ref.upgrade() {
                *state.current.borrow_mut() = sample_encode();
                apply_encode(&app, &state.current.borrow());
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_open_queue(move || {
            if let Some(app) = app_ref.upgrade() {
                match std::fs::read(SAVE_FILENAME)
                    .map_err(|e| format!("failed to read {SAVE_FILENAME}: {e}"))
                    .and_then(|bytes| load_encode_queue(&bytes))
                {
                    Ok(queue) => {
                        *state.current.borrow_mut() = queue;
                        apply_encode(&app, &state.current.borrow());
                        app.set_status_left(SharedString::from(format!("Opened {SAVE_FILENAME}")));
                    }
                    Err(err) => app.set_status_left(SharedString::from(format!("Open failed: {err}"))),
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_select_job(move |index| {
            if let Some(app) = app_ref.upgrade() {
                if index < 0 {
                    return;
                }
                let mut current = state.current.borrow_mut();
                if current.select_job(index as usize) {
                    apply_encode(&app, &current);
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_add_job(move || {
            if let Some(app) = app_ref.upgrade() {
                let mut current = state.current.borrow_mut();
                let count = current.jobs.len() + 1;
                current.add_job(EncodeJob::new(
                    format!("job-{count}"),
                    format!("Source_Clip_{count}.mov"),
                    format!("Encoded_Clip_{count}.mp4"),
                    EncodePreset::h264_1080p(),
                ));
                apply_encode(&app, &current);
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_preset_changed(move |preset_str| {
            if let Some(app) = app_ref.upgrade() {
                let mut current = state.current.borrow_mut();
                let idx = current.active_job_index;
                if let Some(job) = current.jobs.get_mut(idx) {
                    job.preset = match preset_str.as_str() {
                        "ProRes 422" => EncodePreset::prores_master(),
                        "AV1 High" => EncodePreset {
                            name: "AV1 High Profile".to_string(),
                            container: "mp4".to_string(),
                            video_codec: "av1".to_string(),
                            audio_codec: "opus".to_string(),
                            bitrate_kbps: 6000,
                        },
                        "MP4 H.264" => EncodePreset::h264_1080p(),
                        _ => EncodePreset::h264_1080p(),
                    };
                    apply_encode(&app, &current);
                }
            }
        });
    }

    {
        let app_ref = app.as_weak();
        app.on_output_dir_changed(move |path| {
            if let Some(app) = app_ref.upgrade() {
                app.set_status_left(SharedString::from(format!("Output Path: {path}")));
            }
        });
    }

    {
        let app_ref = app.as_weak();
        app.on_browse_output_dir(move || {
            if let Some(app) = app_ref.upgrade() {
                app.set_output_directory("./exports/encode/".into());
                app.set_status_left(SharedString::from("Selected destination directory ./exports/encode/"));
            }
        });
    }


    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_save_queue(move || {
            if let Some(app) = app_ref.upgrade() {
                if let Ok(bytes) = save_encode_queue(&state.current.borrow()) {
                    let _ = std::fs::write(SAVE_FILENAME, bytes);
                    app.set_status_left(SharedString::from(format!("Saved {SAVE_FILENAME}")));
                }
            }
        });
    }

    apply_encode(&app, &state.current.borrow());
    app.show().map_err(|e| e.to_string())?;
    slint::run_event_loop().map_err(|e| e.to_string())?;
    Ok(())
}

