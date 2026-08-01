//! Loom Encode desktop batch media transcoding application.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use loom_encode_core::{
    save_encode_queue, EncodeJob, EncodePreset, EncodeQueue,
};
use loom_test_support::capture::{set_platform, snapshot_component};
use slint::{ComponentHandle, PhysicalSize, SharedString};

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

fn apply_encode(app: &EncodeApp, q: &EncodeQueue) {
    app.set_queue_name(q.name.as_str().into());
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
    let q = sample_encode();
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
        return render_headless(&args, out.to_str().unwrap());
    }

    let app = EncodeApp::new().map_err(|e| e.to_string())?;
    apply_theme(&app, &args.theme);
    app.window()
        .set_size(PhysicalSize::new(args.size.0, args.size.1));

    let state = Rc::new(GuiState {
        current: RefCell::new(sample_encode()),
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
