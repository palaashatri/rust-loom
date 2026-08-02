//! Loom Present desktop presentation application.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use loom_present_core::{export_pdf, load_presentation, save_presentation, PresentationDocument};
use loom_test_support::capture::{set_platform, snapshot_component};
use slint::{ComponentHandle, ModelRc, PhysicalSize, SharedString, VecModel};

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

fn initial_deck(args: &Args) -> Result<PresentationDocument, String> {
    match args.open.as_deref() {
        Some(path) => {
            let bytes = std::fs::read(path)
                .map_err(|e| format!("failed to read presentation '{path}': {e}"))?;
            load_presentation(&bytes)
                .map_err(|e| format!("failed to load presentation '{path}': {e}"))
        }
        None => Ok(sample_deck()),
    }
}

fn apply_deck(app: &PresentApp, doc: &PresentationDocument) {
    app.set_deck_title(doc.title.as_str().into());
    if let Some(slide) = doc.active_slide() {
        app.set_slide_title(slide.title.as_str().into());
        app.set_slide_notes(SharedString::from(&slide.speaker_notes));
    } else {
        app.set_slide_title("No Slide Selected".into());
        app.set_slide_notes("".into());
    }
    let slide_titles: Vec<SharedString> = doc
        .slides
        .iter()
        .map(|slide| SharedString::from(slide.title.as_str()))
        .collect();
    app.set_slide_titles(ModelRc::new(VecModel::from(slide_titles)));
    app.set_active_slide_index(doc.active_index as i32);
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
    let doc = initial_deck(args)?;
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
        let out = out.to_string_lossy().into_owned();
        return render_headless(&args, &out);
    }

    let app = PresentApp::new().map_err(|e| e.to_string())?;
    apply_theme(&app, &args.theme);
    app.window()
        .set_size(PhysicalSize::new(args.size.0, args.size.1));

    let state = Rc::new(GuiState {
        current: RefCell::new(initial_deck(&args)?),
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
        app.on_open_deck(move || {
            if let Some(app) = app_ref.upgrade() {
                match std::fs::read(SAVE_FILENAME)
                    .map_err(|e| format!("failed to read {SAVE_FILENAME}: {e}"))
                    .and_then(|bytes| load_presentation(&bytes))
                {
                    Ok(doc) => {
                        *state.current.borrow_mut() = doc;
                        apply_deck(&app, &state.current.borrow());
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
        app.on_select_slide(move |index| {
            if let Some(app) = app_ref.upgrade() {
                let mut current = state.current.borrow_mut();
                if current.select_slide(index as usize) {
                    apply_deck(&app, &current);
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_notes_edited(move |notes| {
            if let Some(_app) = app_ref.upgrade() {
                let mut current = state.current.borrow_mut();
                if let Some(slide) = current.active_slide_mut() {
                    slide.speaker_notes = notes.as_str().to_string();
                }
            }
        });
    }

    {
        let app_ref = app.as_weak();
        app.on_toggle_preview_mode(move || {
            if let Some(app) = app_ref.upgrade() {
                let current = app.get_is_preview_mode();
                app.set_is_preview_mode(!current);
                app.set_status_left(SharedString::from(if !current {
                    "Entered Presenter Playback Preview Mode"
                } else {
                    "Exited Presenter Playback Preview Mode"
                }));
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_apply_template(move |tmpl_idx| {
            if let Some(app) = app_ref.upgrade() {
                let mut current = state.current.borrow_mut();
                let layout = match tmpl_idx {
                    0 => "cover",
                    2 => "two-column",
                    3 => "image-text",
                    _ => "content",
                };
                if let Some(slide) = current.active_slide_mut() {
                    slide.layout = layout.to_string();
                }
                apply_deck(&app, &current);
                app.set_status_left(SharedString::from(format!("Applied template: {layout}")));
            }
        });
    }

    {
        let app_ref = app.as_weak();
        app.on_update_property(move |prop_name, val| {
            if let Some(app) = app_ref.upgrade() {
                app.set_status_left(SharedString::from(format!("Updated {prop_name} to {val:.1}")));
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_prev_slide(move || {
            if let Some(app) = app_ref.upgrade() {
                let mut current = state.current.borrow_mut();
                if current.active_index > 0 {
                    let prev = current.active_index - 1;
                    current.select_slide(prev);
                    apply_deck(&app, &current);
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_next_slide(move || {
            if let Some(app) = app_ref.upgrade() {
                let mut current = state.current.borrow_mut();
                if current.active_index + 1 < current.len() {
                    let next = current.active_index + 1;
                    current.select_slide(next);
                    apply_deck(&app, &current);
                }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slide_notes_and_templates() {
        let mut deck = sample_deck();
        assert_eq!(deck.len(), 3);
        if let Some(slide) = deck.active_slide_mut() {
            slide.speaker_notes = "Key notes for presenter".to_string();
            slide.layout = "two-column".to_string();
        }

        let active = deck.active_slide().unwrap();
        assert_eq!(active.speaker_notes, "Key notes for presenter");
        assert_eq!(active.layout, "two-column");
    }
}
