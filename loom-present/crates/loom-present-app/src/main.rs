//! Loom Present desktop presentation application.

use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::Rc;

use loom_present_core::{
    export_pdf, load_presentation_session, save_presentation_session, ElementType,
    PresentationDocument, PresentationSession, SlideElement, TransitionKind,
};
use loom_test_support::capture::{set_platform, snapshot_component};
use loom_test_support::journey::{record_keyboard_palette_journey, PaletteProbe};
use slint::{ComponentHandle, Model, ModelRc, PhysicalSize, SharedString, VecModel};

slint::include_modules!();

const DEFAULT_SIZE: (u32, u32) = (1280, 800);
const SAVE_FILENAME: &str = "presentation.loomdeck";
const EXPORT_FILENAME: &str = "presentation.pdf";

loom_production::define_snapshot_recovery!(PRESENT_RECOVERY, "org.loom.present", "loom.present/1");

struct Args {
    screenshot: Option<String>,
    smoke: bool,
    palette: bool,
    journey: Option<String>,
    size: (u32, u32),
    theme: String,
    open: Option<String>,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        screenshot: None,
        smoke: false,
        palette: false,
        journey: None,
        size: DEFAULT_SIZE,
        theme: "dark".into(),
        open: None,
    };
    let mut iterator = std::env::args().skip(1);
    while let Some(argument) = iterator.next() {
        match argument.as_str() {
            "--screenshot" => {
                args.screenshot = Some(iterator.next().ok_or("--screenshot needs a path")?)
            }
            "--smoke" => args.smoke = true,
            "--palette" => args.palette = true,
            "--journey" => {
                args.journey = Some(
                    iterator
                        .next()
                        .ok_or("--journey needs an output directory")?,
                );
            }
            "--size" => {
                let value = iterator.next().ok_or("--size needs WxH")?;
                let (width, height) = value.split_once('x').ok_or("--size must be WxH")?;
                args.size = (
                    width.parse().map_err(|_| "bad width")?,
                    height.parse().map_err(|_| "bad height")?,
                );
            }
            "--theme" => args.theme = iterator.next().ok_or("--theme needs a name")?,
            "--open" => args.open = Some(iterator.next().ok_or("--open needs a path")?),
            other if !other.starts_with('-') && args.open.is_none() => {
                args.open = Some(other.to_string());
            }

            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(args)
}

fn text_element(
    id: &str,
    kind: ElementType,
    content: &str,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
) -> SlideElement {
    SlideElement {
        id: id.into(),
        element_type: kind,
        content: content.into(),
        x,
        y,
        width,
        height,
    }
}

fn sample_session() -> PresentationSession {
    let mut document = PresentationDocument::new("deck-sample", "Loom for Local Creators");
    if let Some(slide) = document.active_slide_mut() {
        slide.title = "Create without compromise".into();
        slide.elements.clear();
        slide.add_element(text_element(
            "cover-title",
            ElementType::Title,
            "Create without compromise",
            90.0,
            90.0,
            820.0,
            110.0,
        ));
        slide.add_element(text_element(
            "cover-body",
            ElementType::BodyText,
            "A private, native creative studio designed for Linux.",
            92.0,
            230.0,
            700.0,
            120.0,
        ));
    }
    document.add_slide("The creative system", "content");
    if let Some(slide) = document.active_slide_mut() {
        slide.add_element(text_element(
            "system-title",
            ElementType::Title,
            "The creative system",
            80.0,
            70.0,
            820.0,
            90.0,
        ));
        slide.add_element(text_element("system-body", ElementType::BodyText, "Writer, Sheets, Present, Photo, Motion, Video, Studio and Encode share one local-first foundation.", 82.0, 190.0, 760.0, 170.0));
    }
    document.add_slide("Built around ownership", "two-column");
    if let Some(slide) = document.active_slide_mut() {
        slide.add_element(text_element(
            "ownership-title",
            ElementType::Title,
            "Built around ownership",
            80.0,
            70.0,
            820.0,
            90.0,
        ));
        slide.add_element(text_element("ownership-body", ElementType::BodyText, "No required account. No hidden upload. Open formats, local models, and files that remain yours.", 82.0, 190.0, 760.0, 170.0));
    }
    document.active_index = 0;
    let mut session = PresentationSession::new(document);
    let first = session.document.slides[0].id.clone();
    session.set_transition(&first, TransitionKind::Dissolve);
    session
}

fn initial_session(args: &Args) -> Result<PresentationSession, String> {
    match args.open.as_deref() {
        Some(path) => std::fs::read(path)
            .map_err(|error| format!("failed to read presentation '{path}': {error}"))
            .and_then(|bytes| load_presentation_session(&bytes)),
        None => Ok(sample_session()),
    }
}

struct GuiState {
    session: RefCell<PresentationSession>,
    selected_element: Cell<usize>,
}

fn active_body(session: &PresentationSession) -> String {
    session
        .document
        .active_slide()
        .and_then(|slide| {
            slide
                .elements
                .iter()
                .find(|element| element.element_type == ElementType::BodyText)
        })
        .map(|element| element.content.clone())
        .unwrap_or_else(|| "Add supporting content from the toolbar.".into())
}

fn refresh(app: &PresentApp, state: &GuiState) {
    let session = state.session.borrow();
    let document = &session.document;
    app.set_deck_title(document.title.as_str().into());
    app.set_can_undo(session.can_undo());
    app.set_can_redo(session.can_redo());
    app.set_slide_count_text(SharedString::from(format!("{} slides", document.len())));
    app.set_slide_titles(ModelRc::new(VecModel::from(
        document
            .slides
            .iter()
            .map(|slide| SharedString::from(slide.title.as_str()))
            .collect::<Vec<_>>(),
    )));
    app.set_active_slide_index(document.active_index as i32);
    if let Some(slide) = document.active_slide() {
        app.set_slide_title(slide.title.as_str().into());
        app.set_slide_body(active_body(&session).into());
        app.set_slide_notes(slide.speaker_notes.as_str().into());
        let labels = slide
            .elements
            .iter()
            .map(|element| {
                SharedString::from(format!("{:?} · {}", element.element_type, element.content))
            })
            .collect::<Vec<_>>();
        app.set_element_labels(ModelRc::new(VecModel::from(labels)));
        let selected = state
            .selected_element
            .get()
            .min(slide.elements.len().saturating_sub(1));
        state.selected_element.set(selected);
        app.set_active_element_index(selected as i32);
        if let Some(element) = slide.elements.get(selected) {
            app.set_active_element_content(element.content.as_str().into());
            app.set_element_x(element.x);
            app.set_element_y(element.y);
            app.set_element_width(element.width);
            app.set_element_height(element.height);
        } else {
            app.set_active_element_content("".into());
        }
        app.set_transition_index(match session.transition_for(&slide.id) {
            TransitionKind::None => 0,
            TransitionKind::Dissolve => 1,
            TransitionKind::Push => 2,
            TransitionKind::Morph => 3,
        });
    }
    let issue_count = session.validate().len();
    app.set_status_left(SharedString::from(format!(
        "{} slides · {} validation issues · undo {}",
        document.len(),
        issue_count,
        if session.can_undo() {
            "available"
        } else {
            "clean"
        }
    )));
    app.set_status_right("Local deck engine".into());
    if let Ok(bytes) = save_presentation_session(&session) {
        let _ = record_snapshot_recovery("presentation state", bytes);
    }
}

fn apply_theme(app: &PresentApp, theme: &str) {
    Theme::get(app).set_active_theme(SharedString::from(theme));
}

fn render_headless(args: &Args, output: &str) -> Result<(), String> {
    set_platform();
    let app = PresentApp::new().map_err(|error| error.to_string())?;
    apply_theme(&app, &args.theme);
    let state = GuiState {
        session: RefCell::new(initial_session(args)?),
        selected_element: Cell::new(0),
    };
    refresh(&app, &state);
    if args.palette {
        app.set_palette_query(SharedString::from("ex"));
        rebuild_palette(&app, "ex");
        app.set_palette_selected(1);
        app.set_palette_open(true);
    }
    let image = snapshot_component(&app, args.size.0 as f32, args.size.1 as f32, 1.0)
        .map_err(|error| error.to_string())?;
    loom_test_support::png::save_png(Path::new(output), &image).map_err(|error| error.to_string())
}

fn set_status(app: &PresentApp, value: impl Into<SharedString>) {
    app.set_status_left(value.into());
}

/// Record the keyboard command-palette journey with per-step screenshots.
fn run_journey(args: &Args, out_dir: &str) -> Result<(), String> {
    set_platform();
    let app = PresentApp::new().map_err(|error| error.to_string())?;
    apply_theme(&app, &args.theme);
    let state = GuiState {
        session: RefCell::new(initial_session(args)?),
        selected_element: Cell::new(0),
    };
    refresh(&app, &state);
    wire_palette(&app);
    rebuild_palette(&app, "");
    app.window()
        .set_size(PhysicalSize::new(args.size.0, args.size.1));
    let report = record_keyboard_palette_journey(&app, "present", Path::new(out_dir), "template")
        .map_err(|error| format!("journey failed: {error}"))?;
    println!(
        "keyboard journey: {} ({})",
        if report.passed { "PASS" } else { "FAIL" },
        out_dir
    );
    if !report.passed {
        return Err("keyboard journey invariants failed".to_string());
    }
    Ok(())
}

impl PaletteProbe for PresentApp {
    fn palette_open(&self) -> bool {
        self.get_palette_open()
    }

    fn palette_commands(&self) -> usize {
        self.get_palette_commands().row_count()
    }

    fn palette_selected(&self) -> i32 {
        self.get_palette_selected()
    }

    fn palette_query(&self) -> String {
        self.get_palette_query().to_string()
    }

    fn open_palette(&self) {
        self.invoke_open_palette();
    }
}

fn main() -> Result<(), String> {
    let args = parse_args()?;
    if let Some(output) = &args.screenshot {
        return render_headless(&args, output);
    }
    if args.smoke {
        let output =
            std::env::temp_dir().join(format!("loom-present-smoke-{}.png", std::process::id()));
        return render_headless(&args, &output.to_string_lossy());
    }
    if let Some(out_dir) = &args.journey {
        return run_journey(&args, out_dir);
    }

    let app = PresentApp::new().map_err(|error| error.to_string())?;
    apply_theme(&app, &args.theme);
    app.window()
        .set_size(PhysicalSize::new(args.size.0, args.size.1));
    let recovered = initialize_snapshot_recovery()?;
    let initial = if args.open.is_some() {
        initial_session(&args)?
    } else {
        recovered
            .as_deref()
            .and_then(|bytes| load_presentation_session(bytes).ok())
            .unwrap_or(initial_session(&args)?)
    };
    let state = Rc::new(GuiState {
        session: RefCell::new(initial),
        selected_element: Cell::new(0),
    });

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_new_deck(move || {
            if let Some(app) = app_ref.upgrade() {
                *state.session.borrow_mut() = sample_session();
                state.selected_element.set(0);
                refresh(&app, &state);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_open_deck(move || {
            if let Some(app) = app_ref.upgrade() {
                match std::fs::read(SAVE_FILENAME)
                    .map_err(|error| error.to_string())
                    .and_then(|bytes| load_presentation_session(&bytes))
                {
                    Ok(session) => {
                        *state.session.borrow_mut() = session;
                        state.selected_element.set(0);
                        refresh(&app, &state);
                        set_status(&app, format!("Opened {SAVE_FILENAME}"));
                    }
                    Err(error) => set_status(&app, format!("Open failed: {error}")),
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_save_deck(move || {
            if let Some(app) = app_ref.upgrade() {
                match save_presentation_session(&state.session.borrow()) {
                    Ok(bytes) => match std::fs::write(SAVE_FILENAME, &bytes)
                        .map_err(|error| error.to_string())
                        .and_then(|_| checkpoint_snapshot_recovery(bytes))
                    {
                        Ok(()) => set_status(&app, format!("Saved {SAVE_FILENAME}")),
                        Err(error) => set_status(&app, format!("Save/checkpoint failed: {error}")),
                    },
                    Err(error) => set_status(&app, format!("Save failed: {error}")),
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_add_slide(move || {
            if let Some(app) = app_ref.upgrade() {
                let mut session = state.session.borrow_mut();
                session.checkpoint();
                let count = session.document.len() + 1;
                session
                    .document
                    .add_slide(format!("New Slide {count}"), "content");
                if let Some(slide) = session.document.active_slide_mut() {
                    slide.add_element(text_element(
                        &format!("title-{count}"),
                        ElementType::Title,
                        &slide.title,
                        80.0,
                        70.0,
                        820.0,
                        90.0,
                    ));
                    slide.add_element(text_element(
                        &format!("body-{count}"),
                        ElementType::BodyText,
                        "Add your story here.",
                        82.0,
                        190.0,
                        760.0,
                        170.0,
                    ));
                }
                state.selected_element.set(0);
                drop(session);
                refresh(&app, &state);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_duplicate_slide(move || {
            if let Some(app) = app_ref.upgrade() {
                let index = state.session.borrow().document.active_index;
                state.session.borrow_mut().duplicate_slide(index);
                state.selected_element.set(0);
                refresh(&app, &state);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_delete_slide(move || {
            if let Some(app) = app_ref.upgrade() {
                let index = state.session.borrow().document.active_index;
                state.session.borrow_mut().remove_slide(index);
                state.selected_element.set(0);
                refresh(&app, &state);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_undo(move || {
            if let Some(app) = app_ref.upgrade() {
                state.session.borrow_mut().undo();
                state.selected_element.set(0);
                refresh(&app, &state);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_redo(move || {
            if let Some(app) = app_ref.upgrade() {
                state.session.borrow_mut().redo();
                state.selected_element.set(0);
                refresh(&app, &state);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_select_slide(move |index| {
            if let Some(app) = app_ref.upgrade() {
                if index >= 0 {
                    state
                        .session
                        .borrow_mut()
                        .document
                        .select_slide(index as usize);
                    state.selected_element.set(0);
                    refresh(&app, &state);
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_select_element(move |index| {
            if let Some(app) = app_ref.upgrade() {
                if index >= 0 {
                    state.selected_element.set(index as usize);
                    refresh(&app, &state);
                }
            }
        });
    }
    for shape in [false, true] {
        let state = state.clone();
        let app_ref = app.as_weak();
        if shape {
            app.on_add_shape(move || {
                if let Some(app) = app_ref.upgrade() {
                    let mut session = state.session.borrow_mut();
                    let count = session
                        .document
                        .active_slide()
                        .map(|slide| slide.elements.len() + 1)
                        .unwrap_or(1);
                    session.add_element(text_element(
                        &format!("shape-{count}"),
                        ElementType::ShapeRectangle,
                        "Shape",
                        120.0,
                        260.0,
                        300.0,
                        140.0,
                    ));
                    state.selected_element.set(count - 1);
                    drop(session);
                    refresh(&app, &state);
                }
            });
        } else {
            app.on_add_text(move || {
                if let Some(app) = app_ref.upgrade() {
                    let mut session = state.session.borrow_mut();
                    let count = session
                        .document
                        .active_slide()
                        .map(|slide| slide.elements.len() + 1)
                        .unwrap_or(1);
                    session.add_element(text_element(
                        &format!("text-{count}"),
                        ElementType::BodyText,
                        "New text",
                        120.0,
                        220.0,
                        520.0,
                        100.0,
                    ));
                    state.selected_element.set(count - 1);
                    drop(session);
                    refresh(&app, &state);
                }
            });
        }
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_update_element_content(move |content| {
            if let Some(app) = app_ref.upgrade() {
                let mut session = state.session.borrow_mut();
                let selected = state.selected_element.get();
                if session
                    .document
                    .active_slide()
                    .and_then(|slide| slide.elements.get(selected))
                    .is_some()
                {
                    session.checkpoint();
                    let is_title = if let Some(element) = session
                        .document
                        .active_slide_mut()
                        .and_then(|slide| slide.elements.get_mut(selected))
                    {
                        element.content = content.as_str().to_string();
                        element.element_type == ElementType::Title
                    } else {
                        false
                    };
                    if is_title {
                        if let Some(slide) = session.document.active_slide_mut() {
                            slide.title = content.as_str().to_string();
                        }
                    }
                }
                drop(session);
                refresh(&app, &state);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_transform_element(move |property, value| {
            if let Some(app) = app_ref.upgrade() {
                let (id, mut x, mut y, mut width, mut height) = {
                    let session = state.session.borrow();
                    let selected = state.selected_element.get();
                    let Some(element) = session
                        .document
                        .active_slide()
                        .and_then(|slide| slide.elements.get(selected))
                    else {
                        return;
                    };
                    (
                        element.id.clone(),
                        element.x,
                        element.y,
                        element.width,
                        element.height,
                    )
                };
                match property.as_str() {
                    "x" => x = value,
                    "y" => y = value,
                    "width" => width = value,
                    "height" => height = value,
                    _ => {}
                }
                state
                    .session
                    .borrow_mut()
                    .transform_element(&id, x, y, width, height);
                refresh(&app, &state);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_notes_edited(move |notes| {
            if let Some(_app) = app_ref.upgrade() {
                let mut session = state.session.borrow_mut();
                session.checkpoint();
                if let Some(slide) = session.document.active_slide_mut() {
                    slide.speaker_notes = notes.as_str().to_string();
                }
            }
        });
    }
    {
        let app_ref = app.as_weak();
        app.on_toggle_preview_mode(move || {
            if let Some(app) = app_ref.upgrade() {
                app.set_is_preview_mode(!app.get_is_preview_mode());
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_apply_template(move |index| {
            if let Some(app) = app_ref.upgrade() {
                let layout = match index {
                    0 => "cover",
                    2 => "two-column",
                    3 => "image-text",
                    _ => "content",
                };
                let mut session = state.session.borrow_mut();
                session.checkpoint();
                if let Some(slide) = session.document.active_slide_mut() {
                    slide.layout = layout.into();
                }
                drop(session);
                refresh(&app, &state);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_set_transition(move |index| {
            if let Some(app) = app_ref.upgrade() {
                let mut session = state.session.borrow_mut();
                if let Some(slide) = session.document.active_slide() {
                    let id = slide.id.clone();
                    session.set_transition(
                        &id,
                        match index {
                            1 => TransitionKind::Dissolve,
                            2 => TransitionKind::Push,
                            3 => TransitionKind::Morph,
                            _ => TransitionKind::None,
                        },
                    );
                }
                drop(session);
                refresh(&app, &state);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_prev_slide(move || {
            if let Some(app) = app_ref.upgrade() {
                let index = state.session.borrow().document.active_index;
                if index > 0 {
                    state.session.borrow_mut().document.select_slide(index - 1);
                    state.selected_element.set(0);
                    refresh(&app, &state);
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_next_slide(move || {
            if let Some(app) = app_ref.upgrade() {
                let (index, len) = {
                    let session = state.session.borrow();
                    (session.document.active_index, session.document.len())
                };
                if index + 1 < len {
                    state.session.borrow_mut().document.select_slide(index + 1);
                    state.selected_element.set(0);
                    refresh(&app, &state);
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_export_pdf(move || {
            if let Some(app) = app_ref.upgrade() {
                match std::fs::write(
                    EXPORT_FILENAME,
                    export_pdf(&state.session.borrow().document),
                ) {
                    Ok(()) => set_status(&app, format!("Exported {EXPORT_FILENAME}")),
                    Err(error) => set_status(&app, format!("Export failed: {error}")),
                }
            }
        });
    }

    refresh(&app, &state);
    wire_palette(&app);
    app.show().map_err(|error| error.to_string())?;
    slint::run_event_loop().map_err(|error| error.to_string())
}

/// Commands exposed through the command palette. Dispatch reuses the same
/// application callbacks as the toolbar and menus.
#[derive(Debug, Clone)]
enum PaletteAction {
    NewDeck,
    OpenDeck,
    SaveDeck,
    AddSlide,
    DuplicateSlide,
    DeleteSlide,
    Undo,
    Redo,
    AddText,
    ExportPdf,
    TogglePreview,
    PrevSlide,
    NextSlide,
    ApplyTemplate(i32),
    SetTransition(i32),
}

struct PaletteCommand {
    action: PaletteAction,
    id: &'static str,
    label: &'static str,
    shortcut: &'static str,
}

fn master_palette(app: &PresentApp) -> Vec<PaletteCommand> {
    vec![
        PaletteCommand {
            action: PaletteAction::NewDeck,
            id: "present.new",
            label: "New Deck",
            shortcut: "Ctrl+N",
        },
        PaletteCommand {
            action: PaletteAction::OpenDeck,
            id: "present.open",
            label: "Open Deck",
            shortcut: "Ctrl+O",
        },
        PaletteCommand {
            action: PaletteAction::SaveDeck,
            id: "present.save",
            label: "Save Deck",
            shortcut: "Ctrl+S",
        },
        PaletteCommand {
            action: PaletteAction::AddSlide,
            id: "present.add-slide",
            label: "Add Slide",
            shortcut: "Ctrl+Shift+A",
        },
        PaletteCommand {
            action: PaletteAction::DuplicateSlide,
            id: "present.duplicate-slide",
            label: "Duplicate Slide",
            shortcut: "Ctrl+D",
        },
        PaletteCommand {
            action: PaletteAction::DeleteSlide,
            id: "present.delete-slide",
            label: "Delete Slide",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::Undo,
            id: "present.undo",
            label: "Undo",
            shortcut: "Ctrl+Z",
        },
        PaletteCommand {
            action: PaletteAction::Redo,
            id: "present.redo",
            label: "Redo",
            shortcut: "Ctrl+Shift+Z",
        },
        PaletteCommand {
            action: PaletteAction::AddText,
            id: "present.add-text",
            label: "Add Text",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::TogglePreview,
            id: "present.preview",
            label: "Toggle Preview Mode",
            shortcut: "F5",
        },
        PaletteCommand {
            action: PaletteAction::PrevSlide,
            id: "present.prev",
            label: "Previous Slide",
            shortcut: "PageUp",
        },
        PaletteCommand {
            action: PaletteAction::NextSlide,
            id: "present.next",
            label: "Next Slide",
            shortcut: "PageDown",
        },
        PaletteCommand {
            action: PaletteAction::ExportPdf,
            id: "present.export-pdf",
            label: "Export PDF",
            shortcut: "Ctrl+E",
        },
        PaletteCommand {
            action: PaletteAction::ApplyTemplate(0),
            id: "present.template.title",
            label: "Template: Title",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::ApplyTemplate(1),
            id: "present.template.content",
            label: "Template: Content",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::ApplyTemplate(2),
            id: "present.template.2col",
            label: "Template: 2 Column",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::ApplyTemplate(3),
            id: "present.template.image",
            label: "Template: Image",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::SetTransition(1),
            id: "present.transition.dissolve",
            label: "Transition: Dissolve",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::SetTransition(2),
            id: "present.transition.push",
            label: "Transition: Push",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::SetTransition(3),
            id: "present.transition.morph",
            label: "Transition: Morph",
            shortcut: "",
        },
    ]
    .into_iter()
    .filter(|c| match c.action {
        PaletteAction::Undo => app.get_can_undo(),
        PaletteAction::Redo => app.get_can_redo(),
        _ => true,
    })
    .collect()
}

fn rebuild_palette(app: &PresentApp, query: &str) {
    let query_lower = query.trim().to_lowercase();
    let items: Vec<CommandPaletteItem> = master_palette(app)
        .into_iter()
        .filter(|c| {
            query_lower.is_empty()
                || c.label.to_lowercase().contains(&query_lower)
                || c.id.to_lowercase().contains(&query_lower)
        })
        .map(|c| CommandPaletteItem {
            id: c.id.into(),
            label: c.label.into(),
            shortcut: c.shortcut.into(),
            enabled: true,
        })
        .collect();
    app.set_palette_commands(Rc::new(VecModel::from(items)).into());
    let count = app.get_palette_commands().row_count() as i32;
    let selected = app.get_palette_selected();
    if selected >= count && count > 0 {
        app.set_palette_selected(count - 1);
    } else if count == 0 {
        app.set_palette_selected(0);
    }
}

fn wire_palette(app: &PresentApp) {
    {
        let app_ref = app.as_weak();
        app.on_palette_query_changed(move |query| {
            if let Some(app) = app_ref.upgrade() {
                rebuild_palette(&app, query.as_str());
                app.set_palette_selected(0);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        app.on_palette_move(move |delta| {
            if let Some(app) = app_ref.upgrade() {
                let count = app.get_palette_commands().row_count() as i32;
                if count == 0 {
                    return;
                }
                let next = (app.get_palette_selected() + delta).clamp(0, count - 1);
                app.set_palette_selected(next);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        app.on_palette_key_text(move |text| {
            if let Some(app) = app_ref.upgrade() {
                let mut query = app.get_palette_query().to_string();
                query.push_str(text.as_str());
                let query = SharedString::from(query.as_str());
                app.set_palette_query(query.clone());
                rebuild_palette(&app, query.as_str());
                app.set_palette_selected(0);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        app.on_palette_backspace(move || {
            if let Some(app) = app_ref.upgrade() {
                let mut query = app.get_palette_query().to_string();
                query.pop();
                let query = SharedString::from(query.as_str());
                app.set_palette_query(query.clone());
                rebuild_palette(&app, query.as_str());
                app.set_palette_selected(0);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        app.on_palette_close(move || {
            if let Some(app) = app_ref.upgrade() {
                app.set_palette_open(false);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        app.on_palette_invoked(move |index| {
            if let Some(app) = app_ref.upgrade() {
                let query = app.get_palette_query().trim().to_lowercase();
                let command = master_palette(&app)
                    .into_iter()
                    .filter(|c| {
                        query.is_empty()
                            || c.label.to_lowercase().contains(&query)
                            || c.id.to_lowercase().contains(&query)
                    })
                    .nth(index as usize);
                if let Some(command) = command {
                    app.set_palette_open(false);
                    match command.action {
                        PaletteAction::NewDeck => app.invoke_new_deck(),
                        PaletteAction::OpenDeck => app.invoke_open_deck(),
                        PaletteAction::SaveDeck => app.invoke_save_deck(),
                        PaletteAction::AddSlide => app.invoke_add_slide(),
                        PaletteAction::DuplicateSlide => app.invoke_duplicate_slide(),
                        PaletteAction::DeleteSlide => app.invoke_delete_slide(),
                        PaletteAction::Undo => app.invoke_undo(),
                        PaletteAction::Redo => app.invoke_redo(),
                        PaletteAction::AddText => app.invoke_add_text(),
                        PaletteAction::TogglePreview => app.invoke_toggle_preview_mode(),
                        PaletteAction::PrevSlide => app.invoke_prev_slide(),
                        PaletteAction::NextSlide => app.invoke_next_slide(),
                        PaletteAction::ExportPdf => app.invoke_export_pdf(),
                        PaletteAction::ApplyTemplate(index) => app.invoke_apply_template(index),
                        PaletteAction::SetTransition(index) => app.invoke_set_transition(index),
                    }
                }
            }
        });
    }
}
