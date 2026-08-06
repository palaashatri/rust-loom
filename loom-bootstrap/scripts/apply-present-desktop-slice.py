#!/usr/bin/env python3
"""Apply the Loom Present native desktop-service vertical slice."""

from pathlib import Path


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one match, found {count}")
    return text.replace(old, new, 1)


cargo = Path("loom-present/crates/loom-present-app/Cargo.toml")
text = cargo.read_text()
text = replace_once(
    text,
    'loom-present-core = { path = "../loom-present-core" }\n',
    'loom-present-core = { path = "../loom-present-core" }\nloom-desktop = { path = "../../../loom-core/crates/loom-desktop" }\n',
    "Present desktop dependency",
)
cargo.write_text(text)

ui = Path("loom-present/crates/loom-present-app/ui/app.slint")
text = ui.read_text()
text = replace_once(
    text,
    "    callback save-deck;\n    callback add-slide;",
    "    callback save-deck;\n    callback save-as-deck;\n    callback add-slide;",
    "Save As callback",
)
text = replace_once(
    text,
    '            ToolButton { icon: "save"; text: "Save"; clicked => { root.save-deck(); } }\n',
    '            ToolButton { icon: "save"; text: "Save"; clicked => { root.save-deck(); } }\n            ToolButton { icon: "save"; text: "Save As"; clicked => { root.save-as-deck(); } }\n',
    "Save As toolbar",
)
text = replace_once(
    text,
    '''            if ((event.modifiers.control || event.modifiers.meta) && (event.text == Key.K || event.text == "k")) {
                root.open-palette();
                return accept;
            }
            return reject;''',
    '''            if (event.modifiers.control || event.modifiers.meta) {
                if (event.text == Key.K || event.text == "k") {
                    root.open-palette();
                    return accept;
                }
                if (event.text == "n" || event.text == "N") {
                    root.new-deck();
                    return accept;
                }
                if (event.text == "o" || event.text == "O") {
                    root.open-deck();
                    return accept;
                }
                if (event.text == "s" || event.text == "S") {
                    if (event.modifiers.shift) { root.save-as-deck(); } else { root.save-deck(); }
                    return accept;
                }
                if (event.text == "e" || event.text == "E") {
                    root.export-pdf();
                    return accept;
                }
            }
            return reject;''',
    "Present file shortcuts",
)
ui.write_text(text)

main = Path("loom-present/crates/loom-present-app/src/main.rs")
text = main.read_text()
text = replace_once(
    text,
    "use std::path::Path;",
    "use std::path::{Path, PathBuf};",
    "PathBuf import",
)
text = replace_once(
    text,
    "use loom_present_core::{",
    "use loom_desktop::{\n    FileDialogService, FileFilter, NativeFileDialogs, OpenFileRequest, SaveFileRequest,\n};\nuse loom_present_core::{",
    "desktop imports",
)
text = replace_once(
    text,
    '''fn initial_session(args: &Args) -> Result<PresentationSession, String> {
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
''',
    '''fn empty_session() -> PresentationSession {
    let mut document = PresentationDocument::new("untitled-deck", "Untitled Presentation");
    if let Some(slide) = document.active_slide_mut() {
        slide.title = "Untitled Slide".into();
        for element in &mut slide.elements {
            element.content.clear();
        }
    }
    PresentationSession::new(document)
}

fn load_session(path: &Path) -> Result<PresentationSession, String> {
    let bytes = std::fs::read(path)
        .map_err(|error| format!("failed to read presentation '{}': {error}", path.display()))?;
    load_presentation_session(&bytes)
        .map_err(|error| format!("failed to load presentation '{}': {error}", path.display()))
}

fn initial_session(args: &Args) -> Result<PresentationSession, String> {
    match args.open.as_deref() {
        Some(path) => load_session(Path::new(path)),
        None => Ok(sample_session()),
    }
}

struct GuiState {
    session: RefCell<PresentationSession>,
    selected_element: Cell<usize>,
    save_path: RefCell<Option<PathBuf>>,
    dialogs: Rc<dyn FileDialogService>,
    deck_filter: FileFilter,
    pdf_filter: FileFilter,
}
''',
    "blank session, path loader, and desktop state",
)
text = replace_once(
    text,
    '''fn set_status(app: &PresentApp, value: impl Into<SharedString>) {
    app.set_status_left(value.into());
}
''',
    '''fn set_status(app: &PresentApp, value: impl Into<SharedString>) {
    app.set_status_left(value.into());
}

fn initial_directory(path: Option<&Path>) -> Option<PathBuf> {
    path.and_then(Path::parent)
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(Path::to_path_buf)
}

fn open_request(state: &GuiState) -> OpenFileRequest {
    OpenFileRequest {
        title: "Open Loom Present Deck".into(),
        initial_directory: initial_directory(state.save_path.borrow().as_deref()),
        suggested_name: None,
        filters: vec![state.deck_filter.clone()],
    }
}

fn save_request(state: &GuiState) -> SaveFileRequest {
    let path = state.save_path.borrow();
    let suggested_name = path
        .as_deref()
        .and_then(Path::file_name)
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| SAVE_FILENAME.to_string());
    SaveFileRequest {
        title: "Save Loom Present Deck".into(),
        initial_directory: initial_directory(path.as_deref()),
        suggested_name: Some(suggested_name),
        filters: vec![state.deck_filter.clone()],
    }
}

fn export_request(state: &GuiState) -> SaveFileRequest {
    SaveFileRequest {
        title: "Export Loom Present PDF".into(),
        initial_directory: initial_directory(state.save_path.borrow().as_deref()),
        suggested_name: Some(EXPORT_FILENAME.to_string()),
        filters: vec![state.pdf_filter.clone()],
    }
}

fn replace_opened_deck(
    app: &PresentApp,
    state: &GuiState,
    path: PathBuf,
    session: PresentationSession,
) {
    *state.session.borrow_mut() = session;
    *state.save_path.borrow_mut() = Some(path);
    state.selected_element.set(0);
    refresh(app, state);
}

fn save_current_deck(
    app: &PresentApp,
    state: &GuiState,
    force_picker: bool,
) -> Result<bool, String> {
    let current_path = (!force_picker)
        .then(|| state.save_path.borrow().clone())
        .flatten();
    let path = match current_path {
        Some(path) => Some(path),
        None => state
            .dialogs
            .save_file(&save_request(state))
            .map_err(|error| error.to_string())?,
    };
    let Some(path) = path else {
        set_status(app, "Save cancelled");
        return Ok(false);
    };

    let bytes = save_presentation_session(&state.session.borrow())?;
    std::fs::write(&path, &bytes)
        .map_err(|error| format!("failed to write '{}': {error}", path.display()))?;
    *state.save_path.borrow_mut() = Some(path.clone());
    match checkpoint_snapshot_recovery(bytes) {
        Ok(()) => set_status(app, format!("Saved {}", path.display())),
        Err(error) => set_status(
            app,
            format!(
                "Saved {}, but recovery checkpoint failed: {error}",
                path.display()
            ),
        ),
    }
    Ok(true)
}
''',
    "Present dialog helpers",
)
text = replace_once(
    text,
    '''fn main() -> Result<(), String> {
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

    let app = PresentApp::new().map_err(|error| error.to_string())?;''',
    '''fn main() -> Result<(), String> {
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
    run_gui_with_dialogs(&args, Rc::new(NativeFileDialogs))
}

fn run_gui_with_dialogs(
    args: &Args,
    dialogs: Rc<dyn FileDialogService>,
) -> Result<(), String> {
    let app = PresentApp::new().map_err(|error| error.to_string())?;''',
    "injectable Present GUI entry point",
)
text = replace_once(
    text,
    '''    let recovered = initialize_snapshot_recovery()?;
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
    });''',
    '''    let recovered = initialize_snapshot_recovery()?;
    let initial_path = args.open.as_ref().map(PathBuf::from);
    let initial = if let Some(path) = initial_path.as_deref() {
        load_session(path)?
    } else {
        recovered
            .as_deref()
            .and_then(|bytes| load_presentation_session(bytes).ok())
            .unwrap_or_else(sample_session)
    };
    let deck_filter =
        FileFilter::new("Loom Present deck", ["loomdeck"]).map_err(|error| error.to_string())?;
    let pdf_filter =
        FileFilter::new("PDF document", ["pdf"]).map_err(|error| error.to_string())?;
    let state = Rc::new(GuiState {
        session: RefCell::new(initial),
        selected_element: Cell::new(0),
        save_path: RefCell::new(initial_path),
        dialogs,
        deck_filter,
        pdf_filter,
    });''',
    "Present desktop state initialization",
)
text = replace_once(
    text,
    '''        app.on_new_deck(move || {
            if let Some(app) = app_ref.upgrade() {
                *state.session.borrow_mut() = sample_session();
                state.selected_element.set(0);
                refresh(&app, &state);
            }
        });''',
    '''        app.on_new_deck(move || {
            if let Some(app) = app_ref.upgrade() {
                *state.session.borrow_mut() = empty_session();
                *state.save_path.borrow_mut() = None;
                state.selected_element.set(0);
                refresh(&app, &state);
                set_status(&app, "Created unsaved presentation");
            }
        });''',
    "blank New deck behavior",
)
text = replace_once(
    text,
    '''        app.on_open_deck(move || {
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
        });''',
    '''        app.on_open_deck(move || {
            if let Some(app) = app_ref.upgrade() {
                match state.dialogs.open_file(&open_request(&state)) {
                    Ok(Some(path)) => match load_session(&path) {
                        Ok(session) => {
                            replace_opened_deck(&app, &state, path.clone(), session);
                            set_status(&app, format!("Opened {}", path.display()));
                        }
                        Err(error) => set_status(&app, format!("Open failed: {error}")),
                    },
                    Ok(None) => set_status(&app, "Open cancelled"),
                    Err(error) => set_status(&app, format!("Open dialog failed: {error}")),
                }
            }
        });''',
    "native Open deck behavior",
)
text = replace_once(
    text,
    '''        app.on_save_deck(move || {
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
        });''',
    '''        app.on_save_deck(move || {
            if let Some(app) = app_ref.upgrade() {
                if let Err(error) = save_current_deck(&app, &state, false) {
                    set_status(&app, format!("Save failed: {error}"));
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_save_as_deck(move || {
            if let Some(app) = app_ref.upgrade() {
                if let Err(error) = save_current_deck(&app, &state, true) {
                    set_status(&app, format!("Save As failed: {error}"));
                }
            }
        });''',
    "native Save and Save As deck behavior",
)
text = replace_once(
    text,
    '''        app.on_export_pdf(move || {
            if let Some(app) = app_ref.upgrade() {
                match std::fs::write(
                    EXPORT_FILENAME,
                    export_pdf(&state.session.borrow().document),
                ) {
                    Ok(()) => set_status(&app, format!("Exported {EXPORT_FILENAME}")),
                    Err(error) => set_status(&app, format!("Export failed: {error}")),
                }
            }
        });''',
    '''        app.on_export_pdf(move || {
            if let Some(app) = app_ref.upgrade() {
                match state.dialogs.save_file(&export_request(&state)) {
                    Ok(Some(path)) => match std::fs::write(
                        &path,
                        export_pdf(&state.session.borrow().document),
                    ) {
                        Ok(()) => set_status(&app, format!("Exported {}", path.display())),
                        Err(error) => set_status(&app, format!("Export failed: {error}")),
                    },
                    Ok(None) => set_status(&app, "Export cancelled"),
                    Err(error) => set_status(&app, format!("Export dialog failed: {error}")),
                }
            }
        });''',
    "native PDF export behavior",
)
text = replace_once(
    text,
    "    SaveDeck,\n    AddSlide,",
    "    SaveDeck,\n    SaveAsDeck,\n    AddSlide,",
    "Save As palette action",
)
text = replace_once(
    text,
    '''        PaletteCommand {
            action: PaletteAction::SaveDeck,
            id: "present.save",
            label: "Save Deck",
            shortcut: "Ctrl+S",
        },
        PaletteCommand {
            action: PaletteAction::AddSlide,''',
    '''        PaletteCommand {
            action: PaletteAction::SaveDeck,
            id: "present.save",
            label: "Save Deck",
            shortcut: "Ctrl+S",
        },
        PaletteCommand {
            action: PaletteAction::SaveAsDeck,
            id: "present.save-as",
            label: "Save Deck As",
            shortcut: "Ctrl+Shift+S",
        },
        PaletteCommand {
            action: PaletteAction::AddSlide,''',
    "Save As palette command",
)
text = replace_once(
    text,
    "                        PaletteAction::SaveDeck => app.invoke_save_deck(),\n                        PaletteAction::AddSlide =>",
    "                        PaletteAction::SaveDeck => app.invoke_save_deck(),\n                        PaletteAction::SaveAsDeck => app.invoke_save_as_deck(),\n                        PaletteAction::AddSlide =>",
    "Save As palette dispatch",
)
text += '''

#[cfg(test)]
mod desktop_tests {
    use super::*;
    use loom_desktop::ScriptedFileDialogs;

    fn test_state() -> GuiState {
        GuiState {
            session: RefCell::new(empty_session()),
            selected_element: Cell::new(0),
            save_path: RefCell::new(Some(PathBuf::from("projects/demo.loomdeck"))),
            dialogs: Rc::new(ScriptedFileDialogs::default()),
            deck_filter: FileFilter::new("Loom Present deck", ["loomdeck"]).expect("filter"),
            pdf_filter: FileFilter::new("PDF document", ["pdf"]).expect("filter"),
        }
    }

    #[test]
    fn new_presentation_is_blank_and_single_slide() {
        let session = empty_session();
        assert_eq!(session.document.len(), 1);
        assert_eq!(session.document.title, "Untitled Presentation");
        assert!(session.document.slides[0]
            .elements
            .iter()
            .all(|element| element.content.is_empty()));
    }

    #[test]
    fn dialog_requests_use_current_directory_and_expected_extensions() {
        let state = test_state();
        let open = open_request(&state);
        let save = save_request(&state);
        let export = export_request(&state);

        assert_eq!(open.initial_directory, Some(PathBuf::from("projects")));
        assert_eq!(open.filters[0].extensions, ["loomdeck"]);
        assert_eq!(save.suggested_name.as_deref(), Some("demo.loomdeck"));
        assert_eq!(export.suggested_name.as_deref(), Some(EXPORT_FILENAME));
        assert_eq!(export.filters[0].extensions, ["pdf"]);
    }

    #[test]
    fn presentation_path_round_trip_preserves_document() {
        let path = std::env::temp_dir().join(format!(
            "loom-present-roundtrip-{}.loomdeck",
            std::process::id()
        ));
        let session = empty_session();
        let bytes = save_presentation_session(&session).expect("serialize");
        std::fs::write(&path, bytes).expect("write");
        let loaded = load_session(&path).expect("load");
        let _ = std::fs::remove_file(&path);

        assert_eq!(loaded.document.title, session.document.title);
        assert_eq!(loaded.document.len(), session.document.len());
    }
}
'''
main.write_text(text)
