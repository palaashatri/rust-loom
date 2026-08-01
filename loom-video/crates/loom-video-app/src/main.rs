//! Loom Video desktop application.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use loom_test_support::capture::{set_platform, snapshot_component};
use loom_video_core::{load_video_project, save_video_project, Clip, VideoProject};
use slint::{ComponentHandle, ModelRc, PhysicalSize, SharedString, VecModel};

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

fn sample_video() -> VideoProject {
    let mut proj = VideoProject::new("v-sample", "Documentary Short Edit");
    proj.tracks[0].add_clip(Clip::new("c1", "Scene_01.mp4", 6.0));
    proj.tracks[0].add_clip(Clip::new("c2", "Scene_02.mp4", 10.5));
    proj
}

fn initial_video(args: &Args) -> Result<VideoProject, String> {
    match args.open.as_deref() {
        Some(path) => {
            let bytes = std::fs::read(path)
                .map_err(|e| format!("failed to read video project '{path}': {e}"))?;
            load_video_project(&bytes)
                .map_err(|e| format!("failed to load video project '{path}': {e}"))
        }
        None => Ok(sample_video()),
    }
}

fn apply_video(app: &VideoApp, proj: &VideoProject) {
    app.set_project_name(proj.name.as_str().into());
    let track_labels: Vec<SharedString> = proj
        .tracks
        .iter()
        .map(|track| {
            SharedString::from(format!(
                "{} ({:?}, {} clips)",
                track.name,
                track.track_type,
                track.clips.len()
            ))
        })
        .collect();
    app.set_track_labels(ModelRc::new(VecModel::from(track_labels)));
    app.set_status_left(SharedString::from(format!(
        "{} tracks, {} clips",
        proj.tracks.len(),
        proj.total_clips()
    )));
    app.set_status_right("Offline".into());
}

fn apply_theme(app: &VideoApp, theme: &str) {
    Theme::get(app).set_active_theme(SharedString::from(theme));
}

fn render_headless(args: &Args, out: &str) -> Result<(), String> {
    set_platform();
    let app = VideoApp::new().map_err(|e| e.to_string())?;
    apply_theme(&app, &args.theme);
    let proj = initial_video(args)?;
    apply_video(&app, &proj);
    let (w, h) = args.size;
    let img = snapshot_component(&app, w as f32, h as f32, 1.0).map_err(|e| e.to_string())?;
    loom_test_support::png::save_png(Path::new(out), &img).map_err(|e| e.to_string())?;
    Ok(())
}

struct GuiState {
    current: RefCell<VideoProject>,
}

fn main() -> Result<(), String> {
    let args = parse_args()?;
    if let Some(out) = &args.screenshot {
        return render_headless(&args, out);
    }
    if args.smoke {
        let out = std::env::temp_dir().join(format!("loom-video-smoke-{}.png", std::process::id()));
        return render_headless(&args, out.to_str().unwrap());
    }

    let app = VideoApp::new().map_err(|e| e.to_string())?;
    apply_theme(&app, &args.theme);
    app.window()
        .set_size(PhysicalSize::new(args.size.0, args.size.1));

    let state = Rc::new(GuiState {
        current: RefCell::new(initial_video(&args)?),
    });

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_new_project(move || {
            if let Some(app) = app_ref.upgrade() {
                *state.current.borrow_mut() = sample_video();
                apply_video(&app, &state.current.borrow());
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_add_clip(move || {
            if let Some(app) = app_ref.upgrade() {
                let mut current = state.current.borrow_mut();
                let count = current.total_clips() + 1;
                current.tracks[0].add_clip(Clip::new(
                    format!("c-{count}"),
                    format!("Clip_{count}.mp4"),
                    5.0,
                ));
                apply_video(&app, &current);
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_save_project(move || {
            if let Some(app) = app_ref.upgrade() {
                if let Ok(bytes) = save_video_project(&state.current.borrow()) {
                    let _ = std::fs::write(SAVE_FILENAME, bytes);
                    app.set_status_left(SharedString::from(format!("Saved {SAVE_FILENAME}")));
                }
            }
        });
    }

    apply_video(&app, &state.current.borrow());
    app.show().map_err(|e| e.to_string())?;
    slint::run_event_loop().map_err(|e| e.to_string())?;
    Ok(())
}
