//! Loom Motion desktop application.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use loom_motion_core::{load_motion, save_motion, CompositionDocument, MotionLayer};
use loom_test_support::capture::{set_platform, snapshot_component};
use slint::{ComponentHandle, ModelRc, PhysicalSize, SharedString, VecModel};

slint::include_modules!();

const DEFAULT_SIZE: (u32, u32) = (1280, 800);
const SAVE_FILENAME: &str = "comp.loommotion";

loom_production::define_snapshot_recovery!(MOTION_RECOVERY, "org.loom.motion", "loom.motion/1");

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

fn sample_motion() -> CompositionDocument {
    let mut doc = CompositionDocument::new("comp-sample", "Kinetic Typography Intro");
    doc.add_layer(MotionLayer::new("l-sub", "Subtitle Motion", "Text"));
    doc
}

fn initial_motion(args: &Args) -> Result<CompositionDocument, String> {
    match args.open.as_deref() {
        Some(path) => {
            let bytes = std::fs::read(path)
                .map_err(|e| format!("failed to read motion composition '{path}': {e}"))?;
            load_motion(&bytes)
                .map_err(|e| format!("failed to load motion composition '{path}': {e}"))
        }
        None => Ok(sample_motion()),
    }
}

fn apply_motion(app: &MotionApp, doc: &CompositionDocument) {
    app.set_comp_name(doc.name.as_str().into());
    app.set_timecode_text(SharedString::from(format!(
        "00:00:00:00 ({} fps • {:.0}s)",
        doc.frame_rate, doc.duration_secs
    )));
    app.set_timecode_display("00:00:00:00".into());
    let layer_labels: Vec<SharedString> = doc
        .layers
        .iter()
        .map(|layer| SharedString::from(format!("{} ({})", layer.name, layer.layer_type)))
        .collect();
    app.set_layer_labels(ModelRc::new(VecModel::from(layer_labels)));
    let selected = doc
        .layers
        .get(doc.active_layer_index)
        .map(|layer| layer.name.as_str())
        .unwrap_or("No layer selected");
    app.set_active_layer_index(doc.active_layer_index as i32);
    app.set_status_left(SharedString::from(format!(
        "{} motion layers • Selected: {selected}",
        doc.len()
    )));
    app.set_status_right("Offline".into());
    if let Ok(bytes) = save_motion(doc) {
        let _ = record_snapshot_recovery("motion state", bytes);
    }
}

fn apply_theme(app: &MotionApp, theme: &str) {
    Theme::get(app).set_active_theme(SharedString::from(theme));
}

fn render_headless(args: &Args, out: &str) -> Result<(), String> {
    set_platform();
    let app = MotionApp::new().map_err(|e| e.to_string())?;
    apply_theme(&app, &args.theme);
    let doc = initial_motion(args)?;
    apply_motion(&app, &doc);
    let (w, h) = args.size;
    let img = snapshot_component(&app, w as f32, h as f32, 1.0).map_err(|e| e.to_string())?;
    loom_test_support::png::save_png(Path::new(out), &img).map_err(|e| e.to_string())?;
    Ok(())
}

struct GuiState {
    current: RefCell<CompositionDocument>,
}

fn main() -> Result<(), String> {
    let args = parse_args()?;
    if let Some(out) = &args.screenshot {
        return render_headless(&args, out);
    }
    if args.smoke {
        let out =
            std::env::temp_dir().join(format!("loom-motion-smoke-{}.png", std::process::id()));
        let out = out.to_string_lossy().into_owned();
        return render_headless(&args, &out);
    }

    let app = MotionApp::new().map_err(|e| e.to_string())?;
    apply_theme(&app, &args.theme);
    app.window()
        .set_size(PhysicalSize::new(args.size.0, args.size.1));

    let recovered = initialize_snapshot_recovery()?;
    let initial = if args.open.is_some() {
        initial_motion(&args)?
    } else {
        recovered
            .as_deref()
            .and_then(|bytes| load_motion(bytes).ok())
            .unwrap_or(initial_motion(&args)?)
    };
    let state = Rc::new(GuiState {
        current: RefCell::new(initial),
    });

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_new_comp(move || {
            if let Some(app) = app_ref.upgrade() {
                *state.current.borrow_mut() = sample_motion();
                apply_motion(&app, &state.current.borrow());
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_open_comp(move || {
            if let Some(app) = app_ref.upgrade() {
                match std::fs::read(SAVE_FILENAME)
                    .map_err(|e| format!("failed to read {SAVE_FILENAME}: {e}"))
                    .and_then(|bytes| load_motion(&bytes))
                {
                    Ok(doc) => {
                        *state.current.borrow_mut() = doc;
                        apply_motion(&app, &state.current.borrow());
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
        app.on_add_layer(move || {
            if let Some(app) = app_ref.upgrade() {
                let mut current = state.current.borrow_mut();
                let count = current.len() + 1;
                current.add_layer(MotionLayer::new(
                    format!("layer-{count}"),
                    format!("Motion Layer {count}"),
                    "VectorShape",
                ));
                apply_motion(&app, &current);
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_select_layer(move |index| {
            if let Some(app) = app_ref.upgrade() {
                if index < 0 {
                    return;
                }
                let mut current = state.current.borrow_mut();
                if current.select_layer(index as usize) {
                    apply_motion(&app, &current);
                }
            }
        });
    }

    {
        let app_ref = app.as_weak();
        app.on_play_pause(move || {
            if let Some(app) = app_ref.upgrade() {
                let playing = app.get_is_playing();
                app.set_status_left(SharedString::from(if playing {
                    "Playing motion preview..."
                } else {
                    "Paused playback"
                }));
            }
        });
    }

    {
        let app_ref = app.as_weak();
        app.on_step_back(move || {
            if let Some(app) = app_ref.upgrade() {
                app.set_status_left(SharedString::from("Stepped back 1 frame"));
            }
        });
    }

    {
        let app_ref = app.as_weak();
        app.on_step_forward(move || {
            if let Some(app) = app_ref.upgrade() {
                app.set_status_left(SharedString::from("Stepped forward 1 frame"));
            }
        });
    }

    {
        let app_ref = app.as_weak();
        app.on_toggle_loop(move || {
            if let Some(app) = app_ref.upgrade() {
                let looping = app.get_is_looping();
                app.set_status_left(SharedString::from(format!("Loop mode: {looping}")));
            }
        });
    }

    {
        let app_ref = app.as_weak();
        app.on_transform_changed(move |prop, val| {
            if let Some(app) = app_ref.upgrade() {
                app.set_status_left(SharedString::from(format!("Transform {prop}: {val:.1}")));
            }
        });
    }

    {
        let app_ref = app.as_weak();
        app.on_toggle_curve_drawer(move || {
            if let Some(app) = app_ref.upgrade() {
                let open = app.get_curve_drawer_open();
                app.set_status_left(SharedString::from(format!("Keyframe drawer open: {open}")));
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_save_comp(move || {
            if let Some(app) = app_ref.upgrade() {
                match save_motion(&state.current.borrow()) {
                    Ok(bytes) => match std::fs::write(SAVE_FILENAME, &bytes)
                        .map_err(|error| error.to_string())
                        .and_then(|_| checkpoint_snapshot_recovery(bytes))
                    {
                        Ok(()) => app
                            .set_status_left(SharedString::from(format!("Saved {SAVE_FILENAME}"))),
                        Err(error) => app.set_status_left(SharedString::from(format!(
                            "Save/checkpoint failed: {error}"
                        ))),
                    },
                    Err(error) => {
                        app.set_status_left(SharedString::from(format!("Save failed: {error}")))
                    }
                }
            }
        });
    }

    apply_motion(&app, &state.current.borrow());
    app.show().map_err(|e| e.to_string())?;
    slint::run_event_loop().map_err(|e| e.to_string())?;
    Ok(())
}
