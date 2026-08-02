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
    let track_mutes: Vec<bool> = proj.tracks.iter().map(|t| t.muted).collect();
    let track_solos: Vec<bool> = proj.tracks.iter().map(|_| false).collect();
    app.set_track_labels(ModelRc::new(VecModel::from(track_labels)));
    app.set_track_mutes(ModelRc::new(VecModel::from(track_mutes)));
    app.set_track_solos(ModelRc::new(VecModel::from(track_solos)));
    app.set_active_track_index(proj.active_track_index as i32);
    let selected = proj
        .tracks
        .get(proj.active_track_index)
        .map(|track| {
            format!(
                "{} ({:?}, {} clips)",
                track.name,
                track.track_type,
                track.clips.len()
            )
        })
        .unwrap_or_else(|| "No track selected".to_string());
    app.set_status_left(SharedString::from(format!(
        "{} tracks, {} clips • Selected: {selected}",
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
                let active_track_index = current.active_track_index;
                if let Some(track) = current.tracks.get_mut(active_track_index) {
                    track.add_clip(Clip::new(
                        format!("c-{count}"),
                        format!("Clip_{count}.mp4"),
                        5.0,
                    ));
                }
                apply_video(&app, &current);
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_select_track(move |index| {
            if let Some(app) = app_ref.upgrade() {
                if index < 0 {
                    return;
                }
                let mut current = state.current.borrow_mut();
                if current.select_track(index as usize) {
                    apply_video(&app, &current);
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_toggle_track_mute(move |index| {
            if let Some(app) = app_ref.upgrade() {
                if index < 0 {
                    return;
                }
                let mut current = state.current.borrow_mut();
                if let Some(track) = current.tracks.get_mut(index as usize) {
                    track.muted = !track.muted;
                    apply_video(&app, &current);
                }
            }
        });
    }

    {
        let app_ref = app.as_weak();
        app.on_toggle_track_solo(move |index| {
            if let Some(app) = app_ref.upgrade() {
                app.set_status_left(SharedString::from(format!("Toggled Solo for Track {index}")));
            }
        });
    }

    {
        let app_ref = app.as_weak();
        app.on_select_nle_tool(move |tool| {
            if let Some(app) = app_ref.upgrade() {
                app.set_status_left(SharedString::from(format!("Selected NLE Tool: {tool}")));
            }
        });
    }

    {
        let app_ref = app.as_weak();
        app.on_toggle_snap(move || {
            if let Some(app) = app_ref.upgrade() {
                let snap = app.get_snap_enabled();
                app.set_status_left(SharedString::from(format!("Timeline Snapping: {snap}")));
            }
        });
    }

    {
        let app_ref = app.as_weak();
        app.on_play_pause(move || {
            if let Some(app) = app_ref.upgrade() {
                let playing = app.get_is_playing();
                app.set_status_left(SharedString::from(if playing {
                    "Playing video timeline..."
                } else {
                    "Paused timeline playback"
                }));
            }
        });
    }

    {
        let app_ref = app.as_weak();
        app.on_seek(move |pos| {
            if let Some(app) = app_ref.upgrade() {
                let sec = (pos / 100.0 * 180.0) as u32;
                let min = sec / 60;
                let s = sec % 60;
                let timecode = format!("00:{min:02}:{s:02}:00");
                app.set_timecode_display(timecode.into());
                app.set_status_left(SharedString::from(format!("Scrubbed playhead to {pos:.1}%")));
            }
        });
    }

    {
        let app_ref = app.as_weak();
        app.on_add_marker(move || {
            if let Some(app) = app_ref.upgrade() {
                app.set_status_left(SharedString::from("Added timeline marker at current playhead"));
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

