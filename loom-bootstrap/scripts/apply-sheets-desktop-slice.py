#!/usr/bin/env python3
"""Apply native desktop file workflows and remove Sheets placebo commands."""

from pathlib import Path


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one match, found {count}")
    return text.replace(old, new, 1)


def update_cargo() -> None:
    path = Path("loom-sheets/crates/loom-sheets-app/Cargo.toml")
    text = path.read_text()
    text = replace_once(
        text,
        'loom-sheets-core = { path = "../loom-sheets-core" }\n',
        'loom-sheets-core = { path = "../loom-sheets-core" }\n'
        'loom-desktop = { path = "../../../loom-core/crates/loom-desktop" }\n',
        "Sheets desktop dependency",
    )
    path.write_text(text)


def update_ui() -> None:
    path = Path("loom-sheets/crates/loom-sheets-app/ui/app.slint")
    text = path.read_text()
    text = replace_once(
        text,
        '''    // Retained for compatibility with the current Rust adapter. Only the
    // persisted single-sheet workbook is surfaced until multi-sheet storage
    // is implemented in the engine.
    in-out property <[string]> sheets: ["Sheet 1"];
    in-out property <int> active-sheet-index: 0;
    in-out property <int> cell-format-index: 0;
    in-out property <string> formula-feedback: "Ready";
''',
        '''    // Only the persisted single-sheet workbook is surfaced until
    // multi-sheet storage and cell-format persistence exist in the engine.
    in-out property <string> formula-feedback: "Ready";
''',
        "remove non-persisted UI model properties",
    )
    text = replace_once(
        text,
        "    callback save-sheet;\n    callback export-csv;",
        "    callback save-sheet;\n    callback save-as-sheet;\n    callback export-csv;",
        "save-as callback",
    )
    text = replace_once(
        text,
        '''    callback quick-formula(string);
    callback select-sheet(int);
    callback add-new-sheet;
    callback set-cell-format(int);
''',
        '''    callback quick-formula(string);
''',
        "remove placebo callbacks",
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
                    root.new-sheet();
                    return accept;
                }
                if (event.text == "o" || event.text == "O") {
                    root.open-sheet();
                    return accept;
                }
                if (event.text == "s" || event.text == "S") {
                    if (event.modifiers.shift) { root.save-as-sheet(); } else { root.save-sheet(); }
                    return accept;
                }
                if (event.text == "e" || event.text == "E") {
                    root.export-csv();
                    return accept;
                }
            }
            return reject;''',
        "window shortcuts",
    )
    text = replace_once(
        text,
        '            ToolButton { icon: "save"; text: "Save"; clicked => { root.save-sheet(); } }\n',
        '            ToolButton { icon: "save"; text: "Save"; clicked => { root.save-sheet(); } }\n'
        '            ToolButton { icon: "save"; text: "Save As"; clicked => { root.save-as-sheet(); } }\n',
        "save-as toolbar",
    )
    path.write_text(text)


def update_main() -> None:
    path = Path("loom-sheets/crates/loom-sheets-app/src/main.rs")
    text = path.read_text()
    text = replace_once(
        text,
        "use std::path::Path;",
        "use std::path::{Path, PathBuf};",
        "path import",
    )
    text = replace_once(
        text,
        "use loom_package::manifest::{json as pkg_json, Checksum, Manifest, ManifestEntry};",
        "use loom_desktop::{\n"
        "    FileDialogService, FileFilter, NativeFileDialogs, OpenFileRequest, SaveFileRequest,\n"
        "};\n"
        "use loom_package::manifest::{json as pkg_json, Checksum, Manifest, ManifestEntry};",
        "desktop imports",
    )
    text = replace_once(
        text,
        '''fn load_sheet(path: &str) -> Result<Sheet, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("read {path}: {e}"))?;
    if path.to_lowercase().ends_with(".csv") {''',
        '''fn load_sheet(path: &Path) -> Result<Sheet, String> {
    let bytes = std::fs::read(path)
        .map_err(|error| format!("read {}: {error}", path.display()))?;
    if path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("csv"))
    {''',
        "path-aware load",
    )
    text = replace_once(
        text,
        '''fn save_sheet(path: &str, sheet: &Sheet) -> Result<(), String> {''',
        '''fn save_sheet(path: &Path, sheet: &Sheet) -> Result<(), String> {''',
        "path-aware save signature",
    )
    text = replace_once(
        text,
        '''    std::fs::write(path, bytes).map_err(|e| format!("write {path}: {e}"))
}''',
        '''    std::fs::write(path, bytes)
        .map_err(|error| format!("write {}: {error}", path.display()))
}''',
        "path-aware save error",
    )
    text = text.replace("Some(p) => load_sheet(p)?", "Some(p) => load_sheet(Path::new(p))?")
    text = text.replace("Some(path) => load_sheet(path)?", "Some(path) => load_sheet(Path::new(path))?")
    text = replace_once(
        text,
        '''struct GuiState {
    current: RefCell<Sheet>,
    save_path: RefCell<Option<String>>,
    undo_stack: RefCell<Vec<String>>,
    redo_stack: RefCell<Vec<String>>,
}

fn run_gui(args: &Args) -> Result<(), String> {''',
        '''struct GuiState {
    current: RefCell<Sheet>,
    save_path: RefCell<Option<PathBuf>>,
    undo_stack: RefCell<Vec<String>>,
    redo_stack: RefCell<Vec<String>>,
    dialogs: Rc<dyn FileDialogService>,
    workbook_filter: FileFilter,
    import_filter: FileFilter,
    csv_filter: FileFilter,
}

fn initial_directory(path: Option<&Path>) -> Option<PathBuf> {
    path.and_then(Path::parent)
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(Path::to_path_buf)
}

fn open_request(state: &GuiState) -> OpenFileRequest {
    OpenFileRequest {
        title: "Open or Import Loom Sheets Workbook".into(),
        initial_directory: initial_directory(state.save_path.borrow().as_deref()),
        suggested_name: None,
        filters: vec![state.workbook_filter.clone(), state.import_filter.clone()],
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
        title: "Save Loom Sheets Workbook".into(),
        initial_directory: initial_directory(path.as_deref()),
        suggested_name: Some(suggested_name),
        filters: vec![state.workbook_filter.clone()],
    }
}

fn export_request(state: &GuiState) -> SaveFileRequest {
    SaveFileRequest {
        title: "Export Loom Sheets CSV".into(),
        initial_directory: initial_directory(state.save_path.borrow().as_deref()),
        suggested_name: Some(EXPORT_FILENAME.to_string()),
        filters: vec![state.csv_filter.clone()],
    }
}

fn is_native_workbook(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("loomtable"))
}

fn replace_opened_sheet(app: &SheetsApp, state: &GuiState, path: PathBuf, sheet: Sheet) {
    *state.current.borrow_mut() = sheet;
    *state.save_path.borrow_mut() = is_native_workbook(&path).then_some(path);
    state.undo_stack.borrow_mut().clear();
    state.redo_stack.borrow_mut().clear();
    apply_sheet(app, &state.current.borrow());
}

fn save_current_sheet(app: &SheetsApp, state: &GuiState, force_picker: bool) -> Result<bool, String> {
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
        app.set_status_left("Save cancelled".into());
        return Ok(false);
    };
    save_sheet(&path, &state.current.borrow())?;
    *state.save_path.borrow_mut() = Some(path.clone());
    checkpoint_snapshot_recovery(sheet_to_json(&state.current.borrow()).into_bytes())
        .map_err(|error| format!("saved {}, but recovery checkpoint failed: {error}", path.display()))?;
    app.set_status_left(SharedString::from(format!("Saved {}", path.display())));
    Ok(true)
}

fn run_gui(args: &Args) -> Result<(), String> {
    run_gui_with_dialogs(args, Rc::new(NativeFileDialogs))
}

fn run_gui_with_dialogs(args: &Args, dialogs: Rc<dyn FileDialogService>) -> Result<(), String> {''',
        "desktop state and helpers",
    )
    text = replace_once(
        text,
        '''    let state = Rc::new(GuiState {
        current: RefCell::new(initial_sheet),
        save_path: RefCell::new(args.open.clone()),
        undo_stack: RefCell::new(Vec::new()),
        redo_stack: RefCell::new(Vec::new()),
    });''',
        '''    let workbook_filter =
        FileFilter::new("Loom Sheets workbook", ["loomtable"]).map_err(|error| error.to_string())?;
    let import_filter =
        FileFilter::new("Comma-separated values", ["csv"]).map_err(|error| error.to_string())?;
    let csv_filter = import_filter.clone();
    let initial_path = args.open.as_ref().map(PathBuf::from);
    let state = Rc::new(GuiState {
        current: RefCell::new(initial_sheet),
        save_path: RefCell::new(initial_path.filter(|path| is_native_workbook(path))),
        undo_stack: RefCell::new(Vec::new()),
        redo_stack: RefCell::new(Vec::new()),
        dialogs,
        workbook_filter,
        import_filter,
        csv_filter,
    });''',
        "initialize desktop state",
    )
    text = replace_once(
        text,
        '''        app.on_new_sheet(move || {
            if let Some(app) = app_ref.upgrade() {
                state
                    .undo_stack
                    .borrow_mut()
                    .push(sheet_to_json(&state.current.borrow()));
                state.redo_stack.borrow_mut().clear();
                *state.current.borrow_mut() = sample_sheet();
                apply_sheet(&app, &state.current.borrow());
            }
        });''',
        '''        app.on_new_sheet(move || {
            if let Some(app) = app_ref.upgrade() {
                *state.current.borrow_mut() = sample_sheet();
                *state.save_path.borrow_mut() = None;
                state.undo_stack.borrow_mut().clear();
                state.redo_stack.borrow_mut().clear();
                apply_sheet(&app, &state.current.borrow());
                app.set_status_left("Created unsaved workbook".into());
            }
        });''',
        "new workbook semantics",
    )
    text = replace_once(
        text,
        '''    {
        let app_ref = app.as_weak();
        app.on_select_sheet(move |idx| {
            if let Some(app) = app_ref.upgrade() {
                app.set_active_sheet_index(idx);
                app.set_formula_feedback(SharedString::from(format!("Selected Sheet {}", idx + 1)));
            }
        });
    }
    {
        let app_ref = app.as_weak();
        app.on_add_new_sheet(move || {
            if let Some(app) = app_ref.upgrade() {
                let current_sheets = app.get_sheets();
                let mut sheets_vec: Vec<SharedString> = (0..current_sheets.row_count())
                    .filter_map(|i| current_sheets.row_data(i))
                    .collect();
                let next_idx = sheets_vec.len() + 1;
                sheets_vec.push(SharedString::from(format!("Sheet {next_idx}")));
                let new_active = (sheets_vec.len() - 1) as i32;
                app.set_sheets(ModelRc::new(VecModel::from(sheets_vec)));
                app.set_active_sheet_index(new_active);
                app.set_formula_feedback(SharedString::from(format!("Added Sheet {next_idx}")));
            }
        });
    }
    {
        let app_ref = app.as_weak();
        app.on_set_cell_format(move |fmt_idx| {
            if let Some(app) = app_ref.upgrade() {
                let name = match fmt_idx {
                    1 => "Number",
                    2 => "Currency",
                    3 => "Percentage",
                    _ => "General",
                };
                app.set_formula_feedback(SharedString::from(format!("Format: {name}")));
            }
        });
    }
''',
        "",
        "remove non-persistent command callbacks",
    )
    text = replace_once(
        text,
        '''        app.on_open_sheet(move || {
            if let Some(app) = app_ref.upgrade() {
                let p = state
                    .save_path
                    .borrow()
                    .clone()
                    .unwrap_or_else(|| SAVE_FILENAME.to_string());
                match load_sheet(&p) {
                    Ok(sheet) => {
                        state
                            .undo_stack
                            .borrow_mut()
                            .push(sheet_to_json(&state.current.borrow()));
                        state.redo_stack.borrow_mut().clear();
                        *state.current.borrow_mut() = sheet;
                        apply_sheet(&app, &state.current.borrow());
                    }
                    Err(e) => {
                        app.set_status_left(SharedString::from(format!("open failed: {e}")));
                    }
                }
            }
        });''',
        '''        app.on_open_sheet(move || {
            if let Some(app) = app_ref.upgrade() {
                match state.dialogs.open_file(&open_request(&state)) {
                    Ok(Some(path)) => match load_sheet(&path) {
                        Ok(sheet) => {
                            let imported = !is_native_workbook(&path);
                            replace_opened_sheet(&app, &state, path.clone(), sheet);
                            app.set_status_left(SharedString::from(if imported {
                                format!("Imported {}; use Save As for a Loom workbook", path.display())
                            } else {
                                format!("Opened {}", path.display())
                            }));
                        }
                        Err(error) => app.set_status_left(SharedString::from(format!(
                            "Open failed: {error}"
                        ))),
                    },
                    Ok(None) => app.set_status_left("Open cancelled".into()),
                    Err(error) => app.set_status_left(SharedString::from(format!(
                        "Open dialog failed: {error}"
                    ))),
                }
            }
        });''',
        "native open callback",
    )
    text = replace_once(
        text,
        '''        app.on_save_sheet(move || {
            if let Some(app) = app_ref.upgrade() {
                let p = state
                    .save_path
                    .borrow()
                    .clone()
                    .unwrap_or_else(|| SAVE_FILENAME.to_string());
                match save_sheet(&p, &state.current.borrow()) {
                    Ok(()) => {
                        let checkpoint = checkpoint_snapshot_recovery(
                            sheet_to_json(&state.current.borrow()).into_bytes(),
                        );
                        match checkpoint {
                            Ok(()) => app.set_status_left(SharedString::from(format!("saved {p}"))),
                            Err(error) => app.set_status_left(SharedString::from(format!(
                                "saved {p}, but recovery checkpoint failed: {error}"
                            ))),
                        }
                    }
                    Err(e) => {
                        app.set_status_left(SharedString::from(format!("save failed: {e}")));
                    }
                }
            }
        });
    }
    {''',
        '''        app.on_save_sheet(move || {
            if let Some(app) = app_ref.upgrade() {
                if let Err(error) = save_current_sheet(&app, &state, false) {
                    app.set_status_left(SharedString::from(format!("Save failed: {error}")));
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_save_as_sheet(move || {
            if let Some(app) = app_ref.upgrade() {
                if let Err(error) = save_current_sheet(&app, &state, true) {
                    app.set_status_left(SharedString::from(format!("Save As failed: {error}")));
                }
            }
        });
    }
    {''',
        "native save callbacks",
    )
    text = replace_once(
        text,
        '''        app.on_export_csv(move || {
            if let Some(app) = app_ref.upgrade() {
                let csv = to_csv(&state.current.borrow());
                match std::fs::write(EXPORT_FILENAME, csv) {
                    Ok(()) => app
                        .set_status_left(SharedString::from(format!("exported {EXPORT_FILENAME}"))),
                    Err(e) => {
                        app.set_status_left(SharedString::from(format!("export failed: {e}")));
                    }
                }
            }
        });''',
        '''        app.on_export_csv(move || {
            if let Some(app) = app_ref.upgrade() {
                match state.dialogs.save_file(&export_request(&state)) {
                    Ok(Some(path)) => {
                        let csv = to_csv(&state.current.borrow());
                        match std::fs::write(&path, csv) {
                            Ok(()) => app.set_status_left(SharedString::from(format!(
                                "Exported {}",
                                path.display()
                            ))),
                            Err(error) => app.set_status_left(SharedString::from(format!(
                                "Export failed: {error}"
                            ))),
                        }
                    }
                    Ok(None) => app.set_status_left("Export cancelled".into()),
                    Err(error) => app.set_status_left(SharedString::from(format!(
                        "Export dialog failed: {error}"
                    ))),
                }
            }
        });''',
        "native export callback",
    )
    text = replace_once(
        text,
        '''enum PaletteAction {
    NewSheet,
    OpenSheet,
    SaveSheet,
    ExportCsv,
    Undo,
    Redo,
    AddSheet,
    GoToSheet(i32),
    CellFormat(i32),
}''',
        '''enum PaletteAction {
    NewSheet,
    OpenSheet,
    SaveSheet,
    SaveAsSheet,
    ExportCsv,
    Undo,
    Redo,
}''',
        "palette enum",
    )
    text = replace_once(
        text,
        '''        PaletteCommand {
            action: PaletteAction::SaveSheet,
            id: "sheets.save",
            label: "Save Sheet",
            shortcut: "Ctrl+S",
        },
        PaletteCommand {
            action: PaletteAction::ExportCsv,''',
        '''        PaletteCommand {
            action: PaletteAction::SaveSheet,
            id: "sheets.save",
            label: "Save Sheet",
            shortcut: "Ctrl+S",
        },
        PaletteCommand {
            action: PaletteAction::SaveAsSheet,
            id: "sheets.save-as",
            label: "Save Sheet As",
            shortcut: "Ctrl+Shift+S",
        },
        PaletteCommand {
            action: PaletteAction::ExportCsv,''',
        "save-as palette entry",
    )
    text = replace_once(
        text,
        '''        PaletteCommand {
            action: PaletteAction::AddSheet,
            id: "sheets.add-sheet",
            label: "Add New Sheet",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::GoToSheet(0),
            id: "sheets.goto-1",
            label: "Go To Sheet 1",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::GoToSheet(1),
            id: "sheets.goto-2",
            label: "Go To Sheet 2",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::GoToSheet(2),
            id: "sheets.goto-3",
            label: "Go To Sheet 3",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::CellFormat(0),
            id: "sheets.format.general",
            label: "Format: General",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::CellFormat(1),
            id: "sheets.format.number",
            label: "Format: Number",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::CellFormat(2),
            id: "sheets.format.currency",
            label: "Format: Currency",
            shortcut: "",
        },
        PaletteCommand {
            action: PaletteAction::CellFormat(3),
            id: "sheets.format.percentage",
            label: "Format: Percentage",
            shortcut: "",
        },
''',
        "",
        "remove placebo palette entries",
    )
    text = replace_once(
        text,
        '''                        PaletteAction::SaveSheet => app.invoke_save_sheet(),
                        PaletteAction::ExportCsv => app.invoke_export_csv(),
                        PaletteAction::Undo => app.invoke_undo(),
                        PaletteAction::Redo => app.invoke_redo(),
                        PaletteAction::AddSheet => app.invoke_add_new_sheet(),
                        PaletteAction::GoToSheet(index) => app.invoke_select_sheet(index),
                        PaletteAction::CellFormat(index) => app.invoke_set_cell_format(index),''',
        '''                        PaletteAction::SaveSheet => app.invoke_save_sheet(),
                        PaletteAction::SaveAsSheet => app.invoke_save_as_sheet(),
                        PaletteAction::ExportCsv => app.invoke_export_csv(),
                        PaletteAction::Undo => app.invoke_undo(),
                        PaletteAction::Redo => app.invoke_redo(),''',
        "palette dispatch",
    )
    text = replace_once(
        text,
        '''    #[test]
    fn formula_bar_draft_is_not_applied_before_commit() {''',
        '''    #[test]
    fn scripted_dialog_request_uses_current_workbook_directory() {
        let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new(
            [Some(PathBuf::from("/tmp/import.csv"))],
            [Some(PathBuf::from("/tmp/workbook.loomtable"))],
        ));
        let state = GuiState {
            current: RefCell::new(sample_sheet()),
            save_path: RefCell::new(Some(PathBuf::from("/tmp/current.loomtable"))),
            undo_stack: RefCell::new(Vec::new()),
            redo_stack: RefCell::new(Vec::new()),
            dialogs,
            workbook_filter: FileFilter::new("Workbook", ["loomtable"]).expect("filter"),
            import_filter: FileFilter::new("CSV", ["csv"]).expect("filter"),
            csv_filter: FileFilter::new("CSV", ["csv"]).expect("filter"),
        };
        let request = open_request(&state);
        assert_eq!(request.initial_directory, Some(PathBuf::from("/tmp")));
        assert_eq!(
            state.dialogs.open_file(&request).expect("open"),
            Some(PathBuf::from("/tmp/import.csv"))
        );
    }

    #[test]
    fn csv_import_does_not_become_native_save_target() {
        assert!(!is_native_workbook(Path::new("budget.csv")));
        assert!(is_native_workbook(Path::new("budget.loomtable")));
    }

    #[test]
    fn formula_bar_draft_is_not_applied_before_commit() {''',
        "dialog tests",
    )
    path.write_text(text)


def main() -> None:
    update_cargo()
    update_ui()
    update_main()


if __name__ == "__main__":
    main()
