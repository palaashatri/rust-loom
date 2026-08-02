//! Loom Photo desktop application.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use loom_photo_core::{load_photo, save_photo, Layer, PhotoDocument};
use loom_test_support::capture::{set_platform, snapshot_component};
use slint::{ComponentHandle, ModelRc, PhysicalSize, SharedString, VecModel};

slint::include_modules!();

const DEFAULT_SIZE: (u32, u32) = (1280, 800);
const SAVE_FILENAME: &str = "image.loomphoto";

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

fn sample_photo() -> PhotoDocument {
    let mut doc = PhotoDocument::new("photo-sample", "Studio Composite", 3840, 2160);
    doc.add_layer(Layer::new_adjustment(
        "adj-1",
        "Curves Adjustment",
        "Curves",
        1.0,
    ));
    doc
}

fn initial_photo(args: &Args) -> Result<PhotoDocument, String> {
    match args.open.as_deref() {
        Some(path) => {
            let bytes = std::fs::read(path)
                .map_err(|e| format!("failed to read photo project '{path}': {e}"))?;
            load_photo(&bytes).map_err(|e| format!("failed to load photo project '{path}': {e}"))
        }
        None => Ok(sample_photo()),
    }
}

fn apply_photo(app: &PhotoApp, doc: &PhotoDocument) {
    app.set_project_name(doc.name.as_str().into());
    app.set_dimensions_text(SharedString::from(format!(
        "{} x {} • {} DPI",
        doc.width, doc.height, doc.dpi
    )));
    let layer_labels: Vec<SharedString> = doc
        .layers
        .iter()
        .map(|layer| SharedString::from(format!("{} ({:?})", layer.name, layer.kind)))
        .collect();
    let layer_visibilities: Vec<bool> = doc.layers.iter().map(|l| l.visible).collect();
    app.set_layer_labels(ModelRc::new(VecModel::from(layer_labels)));
    app.set_layer_visibilities(ModelRc::new(VecModel::from(layer_visibilities)));

    if let Some(active_layer) = doc.layers.get(doc.active_layer_index) {
        let mode_str = match active_layer.blend_mode {
            loom_photo_core::BlendMode::Normal => "Normal",
            loom_photo_core::BlendMode::Multiply => "Multiply",
            loom_photo_core::BlendMode::Screen => "Screen",
            loom_photo_core::BlendMode::Overlay => "Overlay",
        };
        app.set_active_blend_mode(mode_str.into());
        app.set_active_layer_opacity(active_layer.opacity * 100.0);
    }

    let selected = doc
        .layers
        .get(doc.active_layer_index)
        .map(|layer| layer.name.as_str())
        .unwrap_or("No layer selected");
    app.set_active_layer_index(doc.active_layer_index as i32);
    app.set_status_left(SharedString::from(format!(
        "{} layers • Selected: {selected}",
        doc.len()
    )));
    app.set_status_right("Offline".into());
}

fn apply_theme(app: &PhotoApp, theme: &str) {
    Theme::get(app).set_active_theme(SharedString::from(theme));
}

fn render_headless(args: &Args, out: &str) -> Result<(), String> {
    set_platform();
    let app = PhotoApp::new().map_err(|e| e.to_string())?;
    apply_theme(&app, &args.theme);
    let doc = initial_photo(args)?;
    apply_photo(&app, &doc);
    let (w, h) = args.size;
    let img = snapshot_component(&app, w as f32, h as f32, 1.0).map_err(|e| e.to_string())?;
    loom_test_support::png::save_png(Path::new(out), &img).map_err(|e| e.to_string())?;
    Ok(())
}

struct GuiState {
    current: RefCell<PhotoDocument>,
}

fn main() -> Result<(), String> {
    let args = parse_args()?;
    if let Some(out) = &args.screenshot {
        return render_headless(&args, out);
    }
    if args.smoke {
        let out = std::env::temp_dir().join(format!("loom-photo-smoke-{}.png", std::process::id()));
        let out = out.to_string_lossy().into_owned();
        return render_headless(&args, &out);
    }

    let app = PhotoApp::new().map_err(|e| e.to_string())?;
    apply_theme(&app, &args.theme);
    app.window()
        .set_size(PhysicalSize::new(args.size.0, args.size.1));

    let state = Rc::new(GuiState {
        current: RefCell::new(initial_photo(&args)?),
    });

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_new_project(move || {
            if let Some(app) = app_ref.upgrade() {
                *state.current.borrow_mut() = sample_photo();
                apply_photo(&app, &state.current.borrow());
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_open_project(move || {
            if let Some(app) = app_ref.upgrade() {
                match std::fs::read(SAVE_FILENAME)
                    .map_err(|e| format!("failed to read {SAVE_FILENAME}: {e}"))
                    .and_then(|bytes| load_photo(&bytes))
                {
                    Ok(doc) => {
                        *state.current.borrow_mut() = doc;
                        apply_photo(&app, &state.current.borrow());
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
                current.add_layer(Layer::new_pixel(
                    format!("layer-{count}"),
                    format!("Pixel Layer {count}"),
                ));
                apply_photo(&app, &current);
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_add_adjustment(move || {
            if let Some(app) = app_ref.upgrade() {
                let mut current = state.current.borrow_mut();
                let count = current.layers.len() + 1;
                current.add_layer(Layer::new_adjustment(
                    format!("adjustment-{count}"),
                    format!("Brightness {count}"),
                    "brightness",
                    0.0,
                ));
                apply_photo(&app, &current);
                app.set_status_left(SharedString::from(
                    "Added editable brightness adjustment metadata",
                ));
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
                    apply_photo(&app, &current);
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_toggle_layer_visibility(move |index| {
            if let Some(app) = app_ref.upgrade() {
                if index < 0 {
                    return;
                }
                let mut current = state.current.borrow_mut();
                if let Some(layer) = current.layers.get_mut(index as usize) {
                    layer.visible = !layer.visible;
                    apply_photo(&app, &current);
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_blend_mode_changed(move |val| {
            if let Some(app) = app_ref.upgrade() {
                let mut current = state.current.borrow_mut();
                let idx = current.active_layer_index;
                if let Some(layer) = current.layers.get_mut(idx) {
                    layer.blend_mode = match val.as_str() {
                        "Multiply" => loom_photo_core::BlendMode::Multiply,
                        "Screen" => loom_photo_core::BlendMode::Screen,
                        "Overlay" => loom_photo_core::BlendMode::Overlay,
                        _ => loom_photo_core::BlendMode::Normal,
                    };
                    apply_photo(&app, &current);
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_opacity_changed(move |val| {
            if let Some(app) = app_ref.upgrade() {
                let mut current = state.current.borrow_mut();
                let idx = current.active_layer_index;
                if let Some(layer) = current.layers.get_mut(idx) {
                    layer.opacity = (val / 100.0).clamp(0.0, 1.0);
                    apply_photo(&app, &current);
                }
            }
        });
    }

    {
        let app_ref = app.as_weak();
        app.on_select_tool(move |tool| {
            if let Some(app) = app_ref.upgrade() {
                app.set_status_left(SharedString::from(format!("Selected Tool: {tool}")));
            }
        });
    }

    {
        let app_ref = app.as_weak();
        app.on_brightness_changed(move |v| {
            if let Some(app) = app_ref.upgrade() {
                app.set_status_left(SharedString::from(format!("Brightness: {v:.0}")));
            }
        });
    }

    {
        let app_ref = app.as_weak();
        app.on_contrast_changed(move |v| {
            if let Some(app) = app_ref.upgrade() {
                app.set_status_left(SharedString::from(format!("Contrast: {v:.0}")));
            }
        });
    }

    {
        let app_ref = app.as_weak();
        app.on_saturation_changed(move |v| {
            if let Some(app) = app_ref.upgrade() {
                app.set_status_left(SharedString::from(format!("Saturation: {v:.0}")));
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_save_project(move || {
            if let Some(app) = app_ref.upgrade() {
                if let Ok(bytes) = save_photo(&state.current.borrow()) {
                    let _ = std::fs::write(SAVE_FILENAME, bytes);
                    app.set_status_left(SharedString::from(format!("Saved {SAVE_FILENAME}")));
                }
            }
        });
    }

    apply_photo(&app, &state.current.borrow());
    app.show().map_err(|e| e.to_string())?;
    slint::run_event_loop().map_err(|e| e.to_string())?;
    Ok(())
}
