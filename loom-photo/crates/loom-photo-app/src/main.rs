//! Loom Photo desktop application.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use loom_photo_core::{save_photo, Layer, PhotoDocument};
use loom_test_support::capture::{set_platform, snapshot_component};
use slint::{ComponentHandle, PhysicalSize, SharedString};

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

fn apply_photo(app: &PhotoApp, doc: &PhotoDocument) {
    app.set_project_name(doc.name.as_str().into());
    app.set_dimensions_text(SharedString::from(format!(
        "{} x {} • {} DPI",
        doc.width, doc.height, doc.dpi
    )));
    app.set_status_left(SharedString::from(format!("{} layers", doc.len())));
    app.set_status_right("Offline".into());
}

fn apply_theme(app: &PhotoApp, theme: &str) {
    Theme::get(app).set_active_theme(SharedString::from(theme));
}

fn render_headless(args: &Args, out: &str) -> Result<(), String> {
    set_platform();
    let app = PhotoApp::new().map_err(|e| e.to_string())?;
    apply_theme(&app, &args.theme);
    let doc = sample_photo();
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
        return render_headless(&args, out.to_str().unwrap());
    }

    let app = PhotoApp::new().map_err(|e| e.to_string())?;
    apply_theme(&app, &args.theme);
    app.window()
        .set_size(PhysicalSize::new(args.size.0, args.size.1));

    let state = Rc::new(GuiState {
        current: RefCell::new(sample_photo()),
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
