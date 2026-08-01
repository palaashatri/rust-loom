//! Loom Writer desktop application.
//!
//! GUI mode opens a real window (winit backend). Headless modes
//! (`--screenshot`, `--smoke`) render the same UI through the software
//! renderer and write a PNG, which is what the Docker visual-QA pipeline
//! and the offline test mode exercise.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use loom_test_support::capture::{set_platform, snapshot_component};
use loom_writer_core::{RichBlock, WriterDocument};
use slint::{ComponentHandle, PhysicalSize, SharedString};

slint::include_modules!();

const DEFAULT_SIZE: (u32, u32) = (1280, 800);
const SAVE_FILENAME: &str = "loom-writer-document.loomdoc";
const EXPORT_FILENAME: &str = "loom-writer-export.pdf";

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
        theme: "light".to_string(),
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
                    w.parse().map_err(|_| "bad --size width")?,
                    h.parse().map_err(|_| "bad --size height")?,
                );
            }
            "--theme" => {
                let t = it.next().ok_or("--theme needs a name")?;
                if !matches!(t.as_str(), "light" | "dark" | "high-contrast") {
                    return Err(format!("unknown theme: {t}"));
                }
                args.theme = t;
            }
            "--open" => {
                args.open = Some(it.next().ok_or("--open needs a path")?);
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(args)
}

/// A sample document used by `--smoke`, screenshots, and first launch.
fn sample_document() -> WriterDocument {
    let mut d = WriterDocument::new("quick-start", "Loom Writer — Quick Start");
    d.push(RichBlock::new(
        d.next_id(),
        "heading1",
        "Welcome to Loom Writer",
    ));
    d.push(RichBlock::new(
        d.next_id(),
        "paragraph",
        "Loom Writer is a calm, professional word processor. Everything is stored in an open, inspectable .loomdoc package on your computer — no account, no cloud, no telemetry.",
    ));
    d.push(RichBlock::new(
        d.next_id(),
        "paragraph",
        "Type styled text, insert images, track changes, and export to PDF or Markdown. All of it works offline.",
    ));
    d.push(RichBlock::new(d.next_id(), "heading2", "Getting started"));
    d.push(RichBlock::new(
        d.next_id(),
        "paragraph",
        "Use New to create a document, Open to load an existing .loomdoc file, and Export PDF to produce a deterministic PDF. Undo and redo are fully wired.",
    ));
    d
}

fn load_file(path: &str) -> Result<WriterDocument, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("read {path}: {e}"))?;
    loom_writer_core::load_document(&bytes).map_err(|e| format!("load {path}: {e}"))
}

fn save_file(path: &str, doc: &WriterDocument) -> Result<(), String> {
    let bytes = loom_writer_core::save_document(doc).map_err(|e| e.to_string())?;
    std::fs::write(path, bytes).map_err(|e| format!("write {path}: {e}"))
}

/// Mutable state shared between the UI callbacks.
struct GuiState {
    current: RefCell<WriterDocument>,
    save_path: RefCell<Option<String>>,
    undo_stack: RefCell<Vec<WriterDocument>>,
    redo_stack: RefCell<Vec<WriterDocument>>,
}

fn apply_document(app: &WriterApp, doc: &WriterDocument) {
    app.set_doc_title(doc.title.as_str().into());
    let content = doc
        .blocks
        .iter()
        .map(|b| b.text.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");
    app.set_doc_content(SharedString::from(content));
    app.set_doc_empty(doc.blocks.is_empty());
    app.set_status_left(SharedString::from(format!("{} blocks", doc.len())));
    app.set_status_right("Offline".into());
}

fn apply_theme(app: &WriterApp, theme: &str) {
    Theme::get(app).set_active_theme(SharedString::from(theme));
}

fn render_headless(args: &Args, out: &str) -> Result<(), String> {
    set_platform();
    let app = WriterApp::new().map_err(|e| e.to_string())?;
    apply_theme(&app, &args.theme);
    let doc = match &args.open {
        Some(p) => load_file(p)?,
        None => sample_document(),
    };
    apply_document(&app, &doc);
    let (w, h) = args.size;
    let img = snapshot_component(&app, w as f32, h as f32, 1.0).map_err(|e| e.to_string())?;
    loom_test_support::png::save_png(Path::new(out), &img).map_err(|e| e.to_string())?;
    Ok(())
}

fn run_gui(args: &Args) -> Result<(), String> {
    let app = WriterApp::new().map_err(|e| e.to_string())?;
    apply_theme(&app, &args.theme);
    app.window()
        .set_size(PhysicalSize::new(args.size.0, args.size.1));

    let state = Rc::new(GuiState {
        current: RefCell::new(match &args.open {
            Some(p) => load_file(p)?,
            None => sample_document(),
        }),
        save_path: RefCell::new(args.open.clone()),
        undo_stack: RefCell::new(Vec::new()),
        redo_stack: RefCell::new(Vec::new()),
    });

    fn apply_with_history(app: &WriterApp, state: &GuiState, next: WriterDocument) {
        state
            .undo_stack
            .borrow_mut()
            .push(state.current.borrow().clone());
        state.redo_stack.borrow_mut().clear();
        *state.current.borrow_mut() = next;
        apply_document(app, &state.current.borrow());
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_new_doc(move || {
            if let Some(app) = app_ref.upgrade() {
                apply_with_history(&app, &state, sample_document());
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_open_doc(move || {
            if let Some(app) = app_ref.upgrade() {
                let p = state
                    .save_path
                    .borrow()
                    .clone()
                    .unwrap_or_else(|| SAVE_FILENAME.to_string());
                match load_file(&p) {
                    Ok(doc) => apply_with_history(&app, &state, doc),
                    Err(e) => {
                        app.set_status_left(SharedString::from(format!("open failed: {e}")));
                    }
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_save_doc(move || {
            if let Some(app) = app_ref.upgrade() {
                let p = state
                    .save_path
                    .borrow()
                    .clone()
                    .unwrap_or_else(|| SAVE_FILENAME.to_string());
                match save_file(&p, &state.current.borrow()) {
                    Ok(()) => app.set_status_left(SharedString::from(format!("saved {p}"))),
                    Err(e) => {
                        app.set_status_left(SharedString::from(format!("save failed: {e}")));
                    }
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_export_pdf(move || {
            if let Some(app) = app_ref.upgrade() {
                let bytes = loom_writer_core::export_pdf(&state.current.borrow());
                match std::fs::write(EXPORT_FILENAME, bytes) {
                    Ok(()) => app
                        .set_status_left(SharedString::from(format!("exported {EXPORT_FILENAME}"))),
                    Err(e) => {
                        app.set_status_left(SharedString::from(format!("export failed: {e}")));
                    }
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_undo(move || {
            if let Some(app) = app_ref.upgrade() {
                if let Some(prev) = state.undo_stack.borrow_mut().pop() {
                    state
                        .redo_stack
                        .borrow_mut()
                        .push(state.current.borrow().clone());
                    *state.current.borrow_mut() = prev;
                    apply_document(&app, &state.current.borrow());
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_redo(move || {
            if let Some(app) = app_ref.upgrade() {
                if let Some(next) = state.redo_stack.borrow_mut().pop() {
                    state
                        .undo_stack
                        .borrow_mut()
                        .push(state.current.borrow().clone());
                    *state.current.borrow_mut() = next;
                    apply_document(&app, &state.current.borrow());
                }
            }
        });
    }

    apply_document(&app, &state.current.borrow());
    app.show().map_err(|e| e.to_string())?;
    slint::run_event_loop().map_err(|e| e.to_string())?;
    Ok(())
}

fn main() -> Result<(), String> {
    let args = parse_args()?;
    if let Some(out) = &args.screenshot {
        return render_headless(&args, out);
    }
    if args.smoke {
        let out =
            std::env::temp_dir().join(format!("loom-writer-smoke-{}.png", std::process::id()));
        return render_headless(&args, out.to_str().unwrap());
    }
    run_gui(&args)
}
