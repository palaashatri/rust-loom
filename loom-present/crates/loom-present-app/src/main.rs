//! Loom Present desktop presentation application.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use loom_present_core::{export_pdf, save_presentation, PresentationDocument};
use loom_test_support::capture::{set_platform, snapshot_component};
use slint::{ComponentHandle, PhysicalSize, SharedString};

slint::include_modules!();

const DEFAULT_SIZE: (u32, u32) = (1280, 800);
const SAVE_FILENAME: &str = "presentation.loomdeck";
const EXPORT_FILENAME: &str = "presentation.pdf";

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

fn sample_deck() -> PresentationDocument {
    let mut doc = PresentationDocument::new("deck-sample", "Loom Present Showcase");
    doc.add_slide("Product Vision", "headline");
    doc.add_slide("Key Pillars", "grid");
    doc
}

fn apply_deck(app: &PresentApp, doc: &PresentationDocument) {
    app.set_deck_title(doc.title.as_str().into());
    if let Some(slide) = doc.active_slide() {
        app.set_slide_title(slide.title.as_str().into());
    } else {
        app.set_slide_title("No Slide Selected".into());
    }
    app.set_slide_count_text(SharedString::from(format!("{} Slides", doc.len())));
    app.set_status_left(SharedString::from(format!("{} slides in deck", doc.len())));
    app.set_status_right("Offline".into());
}

fn apply_theme(app: &PresentApp, theme: &str) {
    Theme::get(app).set_active_theme(SharedString::from(theme));
}

fn render_headless(args: &Args, out: &str) -> Result<(), String> {
    set_platform();
    let app = PresentApp::new().map_err(|e| e.to_string())?;
    apply_theme(&app, &args.theme);
    let doc = sample_deck();
    apply_deck(&app, &doc);
    let (w, h) = args.size;
    let img = snapshot_component(&app, w as f32, h as f32, 1.0).map_err(|e| e.to_string())?;
    loom_test_support::png::save_png(Path::new(out), &img).map_err(|e| e.to_string())?;
    Ok(())
}

struct GuiState {
    current: RefCell<PresentationDocument>,
}

fn main() -> Result<(), String> {
    let args = parse_args()?;
    if let Some(out) = &args.screenshot {
        return render_headless(&args, out);
    }
    if args.smoke {
        let out =
            std::env::temp_dir().join(format!("loom-present-smoke-{}.png", std::process::id()));
        return render_headless(&args, out.to_str().unwrap());
    }

    let app = PresentApp::new().map_err(|e| e.to_string())?;
    apply_theme(&app, &args.theme);
    app.window()
        .set_size(PhysicalSize::new(args.size.0, args.size.1));

    let state = Rc::new(GuiState {
        current: RefCell::new(sample_deck()),
    });

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_new_deck(move || {
            if let Some(app) = app_ref.upgrade() {
                *state.current.borrow_mut() = sample_deck();
                apply_deck(&app, &state.current.borrow());
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_add_slide(move || {
            if let Some(app) = app_ref.upgrade() {
                let mut current = state.current.borrow_mut();
                let count = current.len() + 1;
                current.add_slide(format!("New Slide {count}"), "content");
                apply_deck(&app, &current);
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_save_deck(move || {
            if let Some(app) = app_ref.upgrade() {
                if let Ok(bytes) = save_presentation(&state.current.borrow()) {
                    let _ = std::fs::write(SAVE_FILENAME, bytes);
                    app.set_status_left(SharedString::from(format!("Saved {SAVE_FILENAME}")));
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_export_pdf(move || {
            if let Some(app) = app_ref.upgrade() {
                let bytes = export_pdf(&state.current.borrow());
                let _ = std::fs::write(EXPORT_FILENAME, bytes);
                app.set_status_left(SharedString::from(format!("Exported {EXPORT_FILENAME}")));
            }
        });
    }

    apply_deck(&app, &state.current.borrow());
    app.show().map_err(|e| e.to_string())?;
    slint::run_event_loop().map_err(|e| e.to_string())?;
    Ok(())
}
