//! Loom Studio DAW desktop application.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use loom_studio_core::{
    load_studio_project, save_studio_project, StudioProject, StudioTrack, TrackKind, WorkspaceMode,
};
use loom_test_support::capture::{set_platform, snapshot_component};
use slint::{ComponentHandle, ModelRc, PhysicalSize, SharedString, VecModel};

slint::include_modules!();

const DEFAULT_SIZE: (u32, u32) = (1280, 800);
const SAVE_FILENAME: &str = "song.loomstudio";

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

fn sample_studio() -> StudioProject {
    let mut proj = StudioProject::new("studio-sample", "Summer Acoustic Session");
    proj.bpm = 118.0;
    proj
}

fn initial_studio(args: &Args) -> Result<StudioProject, String> {
    match args.open.as_deref() {
        Some(path) => {
            let bytes = std::fs::read(path)
                .map_err(|e| format!("failed to read studio project '{path}': {e}"))?;
            load_studio_project(&bytes)
                .map_err(|e| format!("failed to load studio project '{path}': {e}"))
        }
        None => Ok(sample_studio()),
    }
}

fn apply_studio(app: &StudioApp, proj: &StudioProject) {
    app.set_song_title(proj.name.as_str().into());
    app.set_tempo_text(SharedString::from(format!("{:.0} BPM • 48kHz", proj.bpm)));
    app.set_bpm_val(proj.bpm);
    let (mode_str, mode_idx) = match proj.mode {
        WorkspaceMode::Quick => ("Quick Workspace", 0),
        WorkspaceMode::Pro => ("Pro Workspace", 1),
    };
    app.set_workspace_mode(mode_str.into());
    app.set_workspace_mode_index(mode_idx);

    let track_labels: Vec<SharedString> = proj
        .tracks
        .iter()
        .map(|track| {
            SharedString::from(format!(
                "{} ({:?}) • {:.1} dB",
                track.name, track.kind, track.volume_db
            ))
        })
        .collect();
    let track_mutes: Vec<bool> = proj.tracks.iter().map(|t| t.mute).collect();
    let track_solos: Vec<bool> = proj.tracks.iter().map(|t| t.solo).collect();
    let track_arms: Vec<bool> = proj.tracks.iter().map(|_| false).collect();
    let track_volumes: Vec<f32> = proj.tracks.iter().map(|t| t.volume_db).collect();
    let track_pans: Vec<f32> = proj.tracks.iter().map(|t| t.pan).collect();

    app.set_track_labels(ModelRc::new(VecModel::from(track_labels)));
    app.set_track_mutes(ModelRc::new(VecModel::from(track_mutes)));
    app.set_track_solos(ModelRc::new(VecModel::from(track_solos)));
    app.set_track_arms(ModelRc::new(VecModel::from(track_arms)));
    app.set_track_volumes(ModelRc::new(VecModel::from(track_volumes)));
    app.set_track_pans(ModelRc::new(VecModel::from(track_pans)));

    app.set_active_track_index(proj.active_track_index as i32);
    let selected = proj
        .tracks
        .get(proj.active_track_index)
        .map(|track| format!("{} ({:?})", track.name, track.kind))
        .unwrap_or_else(|| "No track selected".to_string());
    app.set_status_left(SharedString::from(format!(
        "{} tracks • Selected: {selected}",
        proj.tracks.len()
    )));
    app.set_status_right("Offline".into());
}

fn apply_theme(app: &StudioApp, theme: &str) {
    Theme::get(app).set_active_theme(SharedString::from(theme));
}

fn render_headless(args: &Args, out: &str) -> Result<(), String> {
    set_platform();
    let app = StudioApp::new().map_err(|e| e.to_string())?;
    apply_theme(&app, &args.theme);
    let proj = initial_studio(args)?;
    apply_studio(&app, &proj);
    let (w, h) = args.size;
    let img = snapshot_component(&app, w as f32, h as f32, 1.0).map_err(|e| e.to_string())?;
    loom_test_support::png::save_png(Path::new(out), &img).map_err(|e| e.to_string())?;
    Ok(())
}

struct GuiState {
    current: RefCell<StudioProject>,
}

fn main() -> Result<(), String> {
    let args = parse_args()?;
    if let Some(out) = &args.screenshot {
        return render_headless(&args, out);
    }
    if args.smoke {
        let out =
            std::env::temp_dir().join(format!("loom-studio-smoke-{}.png", std::process::id()));
        let out = out.to_string_lossy().into_owned();
        return render_headless(&args, &out);
    }

    let app = StudioApp::new().map_err(|e| e.to_string())?;
    apply_theme(&app, &args.theme);
    app.window()
        .set_size(PhysicalSize::new(args.size.0, args.size.1));

    let state = Rc::new(GuiState {
        current: RefCell::new(initial_studio(&args)?),
    });

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_new_song(move || {
            if let Some(app) = app_ref.upgrade() {
                *state.current.borrow_mut() = sample_studio();
                apply_studio(&app, &state.current.borrow());
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_open_song(move || {
            if let Some(app) = app_ref.upgrade() {
                match std::fs::read(SAVE_FILENAME)
                    .map_err(|e| format!("failed to read {SAVE_FILENAME}: {e}"))
                    .and_then(|bytes| load_studio_project(&bytes))
                {
                    Ok(project) => {
                        *state.current.borrow_mut() = project;
                        apply_studio(&app, &state.current.borrow());
                        app.set_status_left(SharedString::from(format!("Opened {SAVE_FILENAME}")));
                    }
                    Err(err) => {
                        app.set_status_left(SharedString::from(format!("Open failed: {err}")))
                    }
                }
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
                    apply_studio(&app, &current);
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_toggle_workspace(move || {
            if let Some(app) = app_ref.upgrade() {
                let mut current = state.current.borrow_mut();
                current.mode = match current.mode {
                    WorkspaceMode::Quick => WorkspaceMode::Pro,
                    WorkspaceMode::Pro => WorkspaceMode::Quick,
                };
                apply_studio(&app, &current);
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_set_workspace_mode(move |idx| {
            if let Some(app) = app_ref.upgrade() {
                let mut current = state.current.borrow_mut();
                current.mode = if idx == 1 {
                    WorkspaceMode::Pro
                } else {
                    WorkspaceMode::Quick
                };
                apply_studio(&app, &current);
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_add_track(move || {
            if let Some(app) = app_ref.upgrade() {
                let mut current = state.current.borrow_mut();
                let count = current.tracks.len() + 1;
                current.add_track(StudioTrack::new(
                    format!("track-{count}"),
                    format!("Track {count}"),
                    TrackKind::Audio,
                ));
                apply_studio(&app, &current);
            }
        });
    }

    {
        let app_ref = app.as_weak();
        app.on_play(move || {
            if let Some(app) = app_ref.upgrade() {
                let playing = app.get_is_playing();
                app.set_status_left(SharedString::from(if playing {
                    "DAW Transport: Playing audio engine"
                } else {
                    "DAW Transport: Paused"
                }));
            }
        });
    }

    {
        let app_ref = app.as_weak();
        app.on_stop(move || {
            if let Some(app) = app_ref.upgrade() {
                app.set_status_left(SharedString::from("DAW Transport: Stopped"));
            }
        });
    }

    {
        let app_ref = app.as_weak();
        app.on_record(move || {
            if let Some(app) = app_ref.upgrade() {
                let rec = app.get_is_recording();
                app.set_status_left(SharedString::from(if rec {
                    "DAW Transport: Recording active..."
                } else {
                    "DAW Transport: Recording disarmed"
                }));
            }
        });
    }

    {
        let app_ref = app.as_weak();
        app.on_toggle_loop(move || {
            if let Some(app) = app_ref.upgrade() {
                let looping = app.get_is_looping();
                app.set_status_left(SharedString::from(format!("DAW Loop Mode: {looping}")));
            }
        });
    }

    {
        let app_ref = app.as_weak();
        app.on_toggle_metronome(move || {
            if let Some(app) = app_ref.upgrade() {
                let metro = app.get_metronome_on();
                app.set_status_left(SharedString::from(format!("Metronome Click: {metro}")));
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_bpm_changed(move |val| {
            if let Some(app) = app_ref.upgrade() {
                let mut current = state.current.borrow_mut();
                current.bpm = val;
                app.set_status_left(SharedString::from(format!("Tempo updated: {val:.0} BPM")));
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_toggle_mute(move |idx| {
            if let Some(app) = app_ref.upgrade() {
                if idx < 0 {
                    return;
                }
                let mut current = state.current.borrow_mut();
                if let Some(track) = current.tracks.get_mut(idx as usize) {
                    track.mute = !track.mute;
                    apply_studio(&app, &current);
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_toggle_solo(move |idx| {
            if let Some(app) = app_ref.upgrade() {
                if idx < 0 {
                    return;
                }
                let mut current = state.current.borrow_mut();
                if let Some(track) = current.tracks.get_mut(idx as usize) {
                    track.solo = !track.solo;
                    apply_studio(&app, &current);
                }
            }
        });
    }

    {
        let app_ref = app.as_weak();
        app.on_toggle_rec_arm(move |idx| {
            if let Some(app) = app_ref.upgrade() {
                app.set_status_left(SharedString::from(format!(
                    "Toggled Rec Arm on track {idx}"
                )));
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_volume_changed(move |idx, vol| {
            if let Some(app) = app_ref.upgrade() {
                if idx < 0 {
                    return;
                }
                let mut current = state.current.borrow_mut();
                if let Some(track) = current.tracks.get_mut(idx as usize) {
                    track.volume_db = vol;
                    apply_studio(&app, &current);
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_pan_changed(move |idx, pan| {
            if let Some(app) = app_ref.upgrade() {
                if idx < 0 {
                    return;
                }
                let mut current = state.current.borrow_mut();
                if let Some(track) = current.tracks.get_mut(idx as usize) {
                    track.pan = pan;
                    apply_studio(&app, &current);
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_save_song(move || {
            if let Some(app) = app_ref.upgrade() {
                if let Ok(bytes) = save_studio_project(&state.current.borrow()) {
                    let _ = std::fs::write(SAVE_FILENAME, bytes);
                    app.set_status_left(SharedString::from(format!("Saved {SAVE_FILENAME}")));
                }
            }
        });
    }

    apply_studio(&app, &state.current.borrow());
    app.show().map_err(|e| e.to_string())?;
    slint::run_event_loop().map_err(|e| e.to_string())?;
    Ok(())
}
