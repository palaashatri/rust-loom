//! Loom Sheets desktop application.
//!
//! GUI mode opens a real window (winit backend). Headless modes
//! (`--screenshot`, `--smoke`) render the same UI through the software
//! renderer and write a PNG, which is what the Docker visual-QA pipeline
//! and the offline test mode exercise.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use loom_desktop::{
    FileDialogService, FileFilter, NativeFileDialogs, OpenFileRequest, SaveFileRequest,
};
use loom_package::manifest::{json as pkg_json, Checksum, Manifest, ManifestEntry};
use loom_package::{MimeType, PackageArchive, PackageKind, SchemaVersion};
use loom_sheets_core::{
    evaluate, from_csv, sheet_from_json, sheet_to_json, to_csv, CellEditTransaction, CellRef,
    Sheet, Value,
};
use loom_test_support::capture::{set_platform, snapshot_component};
use loom_test_support::journey::{record_keyboard_palette_journey, PaletteProbe};
use slint::{ComponentHandle, Model, ModelRc, PhysicalSize, SharedString, VecModel};

slint::include_modules!();

const DEFAULT_SIZE: (u32, u32) = (1280, 800);
const GRID_COLS: usize = 8;
const GRID_ROWS: usize = 6;
const SAVE_FILENAME: &str = "loom-sheets-workbook.loomtable";
const EXPORT_FILENAME: &str = "loom-sheets-export.csv";

loom_production::define_snapshot_recovery!(SHEETS_RECOVERY, "org.loom.sheets", "loom.sheets/1");

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
            "--palette" => args.palette = true,
            "--journey" => {
                args.journey = Some(it.next().ok_or("--journey needs an output directory")?);
            }
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
            other if !other.starts_with('-') && args.open.is_none() => {
                args.open = Some(other.to_string());
            }

            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(args)
}

fn blank_sheet() -> Sheet {
    Sheet::new("Untitled")
}

/// A small budget workbook used by `--smoke`, screenshots, and first launch.
fn sample_sheet() -> Sheet {
    let mut sheet = Sheet::new("Budget");
    sheet.set_str("A1", "Item");
    sheet.set_str("A2", "Rent");
    sheet.set_str("A3", "Food");
    sheet.set_str("A4", "Transport");
    sheet.set_str("A5", "Total");
    sheet.set_str("A6", "Average");
    sheet.set_str("B1", "Amount");
    sheet.set_str("B2", "1200");
    sheet.set_str("B3", "450");
    sheet.set_str("B4", "150");
    sheet.set_str("B5", "=SUM(B2:B4)");
    sheet.set_str("B6", "=AVERAGE(B2:B4)");
    sheet.set_str("C1", "Note");
    sheet.set_str("C2", "monthly");
    sheet.set_str("C3", "weekly");
    sheet.set_str("C4", "monthly");
    sheet
}

fn load_sheet(path: &Path) -> Result<Sheet, String> {
    let bytes = std::fs::read(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    if path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("csv"))
    {
        let csv = std::str::from_utf8(&bytes).map_err(|e| format!("csv utf8: {e}"))?;
        return Ok(from_csv("imported", csv));
    }
    let arch = PackageArchive::from_bytes(&bytes).map_err(|e| format!("archive: {e}"))?;
    let manifest_bytes = arch
        .get("manifest.json")
        .ok_or_else(|| "missing manifest.json".to_string())?;
    let manifest_str =
        std::str::from_utf8(manifest_bytes).map_err(|_| "manifest not utf8".to_string())?;
    let manifest: Manifest =
        pkg_json::parse_manifest(manifest_str).map_err(|e| format!("manifest: {e}"))?;
    if manifest.kind != PackageKind::Sheets {
        return Err("not a Sheets workbook".to_string());
    }
    arch.validate_manifest(&manifest)
        .map_err(|e| format!("validation: {e}"))?;
    let content = arch
        .get("content/sheet.json")
        .ok_or_else(|| "missing sheet.json".to_string())?;
    let s = std::str::from_utf8(content).map_err(|_| "sheet not utf8".to_string())?;
    sheet_from_json(s).map_err(|e| format!("sheet: {e}"))
}

fn save_sheet(path: &Path, sheet: &Sheet) -> Result<(), String> {
    let mut arch = PackageArchive::new();
    let json = sheet_to_json(sheet);
    arch.add("content/sheet.json", json.clone().into_bytes())
        .map_err(|e| e.to_string())?;
    let manifest = Manifest {
        schema: SchemaVersion::CURRENT,
        kind: PackageKind::Sheets,
        id: "sheets-doc".to_string(),
        title: sheet.name.clone(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        entries: vec![ManifestEntry {
            path: "content/sheet.json".into(),
            mime: MimeType::parse("application/vnd.loom.sheet-content")
                .map_err(|e| format!("invalid built-in sheets MIME type: {e}"))?,
            size: json.len() as u64,
            sha256: Checksum::from_bytes(loom_package::zip::sha256(json.as_bytes())),
        }],
    };
    let manifest_str = pkg_json::write(&manifest);
    arch.add("manifest.json", manifest_str.into_bytes())
        .map_err(|e| e.to_string())?;
    let bytes = arch.to_bytes().map_err(|e| e.to_string())?;
    std::fs::write(path, bytes).map_err(|error| format!("write {}: {error}", path.display()))
}

fn cell_value(
    sheet: &Sheet,
    vals: &std::collections::HashMap<CellRef, Value>,
    r: u32,
    c: u32,
) -> String {
    match vals.get(&CellRef { row: r, col: c }) {
        Some(v) if *v != Value::Empty => v.display(),
        _ => sheet
            .raw(CellRef { row: r, col: c })
            .map(|s| s.to_string())
            .unwrap_or_default(),
    }
}

fn apply_sheet(app: &SheetsApp, sheet: &Sheet) {
    let vals = evaluate(sheet);
    let selected =
        CellRef::parse(app.get_selected_cell().as_str()).unwrap_or(CellRef { row: 0, col: 0 });
    let cols: Vec<i32> = (0..GRID_COLS as i32).collect();
    let rows: Vec<i32> = (0..GRID_ROWS as i32).collect();
    let headers: Vec<SharedString> = (0..GRID_COLS)
        .map(|c| {
            SharedString::from(
                CellRef {
                    row: 0,
                    col: c as u32,
                }
                .to_a1()
                .trim_end_matches('1'),
            )
        })
        .collect();
    let row_headers: Vec<SharedString> = (1..=GRID_ROWS as i32)
        .map(|r| SharedString::from(r.to_string()))
        .collect();
    let mut cells: Vec<SharedString> = Vec::new();
    for r in 0..GRID_ROWS as u32 {
        for c in 0..GRID_COLS as u32 {
            cells.push(SharedString::from(cell_value(sheet, &vals, r, c)));
        }
    }

    app.set_cols(ModelRc::new(VecModel::from(cols)));
    app.set_rows(ModelRc::new(VecModel::from(rows)));
    app.set_column_headers(ModelRc::new(VecModel::from(headers)));
    app.set_row_headers(ModelRc::new(VecModel::from(row_headers)));
    app.set_cells(ModelRc::new(VecModel::from(cells)));
    update_selection(app, sheet, &vals, selected);
    app.set_sheet_name(sheet.name.as_str().into());
    let formulas = sheet
        .cells
        .values()
        .filter(|c| c.raw.trim_start().starts_with('='))
        .count();
    app.set_status_left(SharedString::from(format!(
        "{} cells · {} formulas",
        sheet.cells.len(),
        formulas
    )));
    app.set_status_right("Offline".into());
    let _ = record_snapshot_recovery("sheets state", sheet_to_json(sheet).into_bytes());
}

fn update_selection(
    app: &SheetsApp,
    sheet: &Sheet,
    vals: &std::collections::HashMap<CellRef, Value>,
    selected: CellRef,
) {
    let formula = sheet
        .raw(selected)
        .map(SharedString::from)
        .unwrap_or_default();
    app.set_selection_formula(formula);
    app.invoke_reset_formula_edit_buffer();
    app.set_selected_cell(selected.to_a1().into());
    app.set_selected_row(selected.row as i32);
    app.set_selected_col(selected.col as i32);
    app.set_selection_value(SharedString::from(cell_value(
        sheet,
        vals,
        selected.row,
        selected.col,
    )));
}

/// Apply one committed formula-bar edit and record one undo transaction.
fn commit_formula_edit(
    sheet: &mut Sheet,
    undo_stack: &mut Vec<String>,
    redo_stack: &mut Vec<String>,
    selected: CellRef,
    draft: &str,
) -> bool {
    let mut transaction = CellEditTransaction::begin(sheet.raw(selected));
    transaction.update(draft.to_owned());
    let Some(edit) = transaction.commit() else {
        return false;
    };
    undo_stack.push(sheet_to_json(sheet));
    redo_stack.clear();
    sheet.set_raw(selected, edit.after().to_owned());
    true
}

fn select_cell(app: &SheetsApp, sheet: &Sheet, r: i32, c: i32) {
    if r < 0 || c < 0 || r >= GRID_ROWS as i32 || c >= GRID_COLS as i32 {
        return;
    }
    let (r, c) = (r as u32, c as u32);
    let refr = CellRef { row: r, col: c };
    let vals = evaluate(sheet);
    update_selection(app, sheet, &vals, refr);
}

fn apply_theme(app: &SheetsApp, theme: &str) {
    Theme::get(app).set_active_theme(SharedString::from(theme));
}

fn render_headless(args: &Args, out: &str) -> Result<(), String> {
    set_platform();
    let app = SheetsApp::new().map_err(|e| e.to_string())?;
    apply_theme(&app, &args.theme);
    let sheet = match &args.open {
        Some(p) => load_sheet(Path::new(p))?,
        None => sample_sheet(),
    };
    apply_sheet(&app, &sheet);
    if args.palette {
        app.set_palette_query(SharedString::from("ex"));
        rebuild_palette(&app, "ex");
        app.set_palette_selected(1);
        app.set_palette_open(true);
    }
    let (w, h) = args.size;
    let img = snapshot_component(&app, w as f32, h as f32, 1.0).map_err(|e| e.to_string())?;
    loom_test_support::png::save_png(Path::new(out), &img).map_err(|e| e.to_string())?;
    Ok(())
}

struct GuiState {
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

fn save_current_sheet(
    app: &SheetsApp,
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
        app.set_status_left("Save cancelled".into());
        return Ok(false);
    };
    save_sheet(&path, &state.current.borrow())?;
    *state.save_path.borrow_mut() = Some(path.clone());
    match checkpoint_snapshot_recovery(sheet_to_json(&state.current.borrow()).into_bytes()) {
        Ok(()) => app.set_status_left(SharedString::from(format!("Saved {}", path.display()))),
        Err(error) => app.set_status_left(SharedString::from(format!(
            "Saved {}, but recovery checkpoint failed: {error}",
            path.display()
        ))),
    }
    Ok(true)
}

fn run_gui(args: &Args) -> Result<(), String> {
    run_gui_with_dialogs(args, Rc::new(NativeFileDialogs))
}

fn run_gui_with_dialogs(args: &Args, dialogs: Rc<dyn FileDialogService>) -> Result<(), String> {
    let app = SheetsApp::new().map_err(|e| e.to_string())?;
    apply_theme(&app, &args.theme);
    app.window()
        .set_size(PhysicalSize::new(args.size.0, args.size.1));

    let recovered = initialize_snapshot_recovery()?;
    let initial_sheet = match &args.open {
        Some(path) => load_sheet(Path::new(path))?,
        None => recovered
            .as_deref()
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
            .and_then(|json| sheet_from_json(json).ok())
            .unwrap_or_else(sample_sheet),
    };
    let workbook_filter = FileFilter::new("Loom Sheets workbook", ["loomtable"])
        .map_err(|error| error.to_string())?;
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
    });

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_new_sheet(move || {
            if let Some(app) = app_ref.upgrade() {
                *state.current.borrow_mut() = blank_sheet();
                *state.save_path.borrow_mut() = None;
                state.undo_stack.borrow_mut().clear();
                state.redo_stack.borrow_mut().clear();
                apply_sheet(&app, &state.current.borrow());
                app.set_status_left("Created unsaved workbook".into());
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_commit_selected_cell(move |draft| {
            if let Some(app) = app_ref.upgrade() {
                if let Some(cell) = CellRef::parse(app.get_selected_cell().as_str()) {
                    let committed = {
                        let mut current = state.current.borrow_mut();
                        let mut undo = state.undo_stack.borrow_mut();
                        let mut redo = state.redo_stack.borrow_mut();
                        commit_formula_edit(
                            &mut current,
                            &mut undo,
                            &mut redo,
                            cell,
                            draft.as_str(),
                        )
                    };
                    if committed {
                        apply_sheet(&app, &state.current.borrow());
                        app.set_formula_feedback(SharedString::from(format!(
                            "Cell {} updated",
                            cell.to_a1()
                        )));
                    }
                }
            }
        });
    }
    {
        let app_ref = app.as_weak();
        app.on_cancel_selected_cell(move || {
            if let Some(app) = app_ref.upgrade() {
                app.invoke_reset_formula_edit_buffer();
                app.set_formula_feedback("Edit cancelled".into());
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_quick_formula(move |func| {
            if let Some(app) = app_ref.upgrade() {
                if let Some(cell) = CellRef::parse(app.get_selected_cell().as_str()) {
                    let formula_text = format!("={func}(A1:A5)");
                    let committed = {
                        let mut current = state.current.borrow_mut();
                        let mut undo = state.undo_stack.borrow_mut();
                        let mut redo = state.redo_stack.borrow_mut();
                        commit_formula_edit(&mut current, &mut undo, &mut redo, cell, &formula_text)
                    };
                    if committed {
                        apply_sheet(&app, &state.current.borrow());
                        app.set_formula_feedback(SharedString::from(format!(
                            "Inserted {func} formula"
                        )));
                    }
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_open_sheet(move || {
            if let Some(app) = app_ref.upgrade() {
                match state.dialogs.open_file(&open_request(&state)) {
                    Ok(Some(path)) => match load_sheet(&path) {
                        Ok(sheet) => {
                            let imported = !is_native_workbook(&path);
                            replace_opened_sheet(&app, &state, path.clone(), sheet);
                            app.set_status_left(SharedString::from(if imported {
                                format!(
                                    "Imported {}; use Save As for a Loom workbook",
                                    path.display()
                                )
                            } else {
                                format!("Opened {}", path.display())
                            }));
                        }
                        Err(error) => {
                            app.set_status_left(SharedString::from(format!("Open failed: {error}")))
                        }
                    },
                    Ok(None) => app.set_status_left("Open cancelled".into()),
                    Err(error) => app.set_status_left(SharedString::from(format!(
                        "Open dialog failed: {error}"
                    ))),
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_save_sheet(move || {
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
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_export_csv(move || {
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
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_undo(move || {
            if let Some(app) = app_ref.upgrade() {
                if let Some(prev) = state.undo_stack.borrow_mut().pop() {
                    if let Ok(sheet) = sheet_from_json(&prev) {
                        state
                            .redo_stack
                            .borrow_mut()
                            .push(sheet_to_json(&state.current.borrow()));
                        *state.current.borrow_mut() = sheet;
                        apply_sheet(&app, &state.current.borrow());
                    }
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
                    if let Ok(sheet) = sheet_from_json(&next) {
                        state
                            .undo_stack
                            .borrow_mut()
                            .push(sheet_to_json(&state.current.borrow()));
                        *state.current.borrow_mut() = sheet;
                        apply_sheet(&app, &state.current.borrow());
                    }
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_cell_clicked(move |r, c| {
            if let Some(app) = app_ref.upgrade() {
                select_cell(&app, &state.current.borrow(), r, c);
            }
        });
    }

    apply_sheet(&app, &state.current.borrow());
    wire_palette(&app);
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
            std::env::temp_dir().join(format!("loom-sheets-smoke-{}.png", std::process::id()));
        let out = out.to_string_lossy().into_owned();
        return render_headless(&args, &out);
    }
    if let Some(out_dir) = &args.journey {
        return run_journey(&args, out_dir);
    }
    run_gui(&args)
}

/// Record the keyboard command-palette journey with per-step screenshots.
fn run_journey(args: &Args, out_dir: &str) -> Result<(), String> {
    set_platform();
    let app = SheetsApp::new().map_err(|e| e.to_string())?;
    apply_theme(&app, &args.theme);
    let sheet = match &args.open {
        Some(p) => load_sheet(Path::new(p))?,
        None => sample_sheet(),
    };
    apply_sheet(&app, &sheet);
    wire_palette(&app);
    rebuild_palette(&app, "");
    app.window()
        .set_size(PhysicalSize::new(args.size.0, args.size.1));
    let report = record_keyboard_palette_journey(&app, "sheets", Path::new(out_dir), "save")
        .map_err(|e| format!("journey failed: {e}"))?;
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

impl PaletteProbe for SheetsApp {
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

/// Commands exposed through the command palette. Invocation dispatches
/// through the same application callbacks as the toolbar.
#[derive(Debug, Clone)]
enum PaletteAction {
    NewSheet,
    OpenSheet,
    SaveSheet,
    SaveAsSheet,
    ExportCsv,
    Undo,
    Redo,
}

struct PaletteCommand {
    action: PaletteAction,
    id: &'static str,
    label: &'static str,
    shortcut: &'static str,
}

fn master_palette() -> Vec<PaletteCommand> {
    vec![
        PaletteCommand {
            action: PaletteAction::NewSheet,
            id: "sheets.new",
            label: "New Sheet",
            shortcut: "Ctrl+N",
        },
        PaletteCommand {
            action: PaletteAction::OpenSheet,
            id: "sheets.open",
            label: "Open Sheet",
            shortcut: "Ctrl+O",
        },
        PaletteCommand {
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
            action: PaletteAction::ExportCsv,
            id: "sheets.export-csv",
            label: "Export CSV",
            shortcut: "Ctrl+E",
        },
        PaletteCommand {
            action: PaletteAction::Undo,
            id: "sheets.undo",
            label: "Undo",
            shortcut: "Ctrl+Z",
        },
        PaletteCommand {
            action: PaletteAction::Redo,
            id: "sheets.redo",
            label: "Redo",
            shortcut: "Ctrl+Shift+Z",
        },
    ]
}

fn rebuild_palette(app: &SheetsApp, query: &str) {
    let query_lower = query.trim().to_lowercase();
    let items: Vec<CommandPaletteItem> = master_palette()
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

fn wire_palette(app: &SheetsApp) {
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
                let command = master_palette()
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
                        PaletteAction::NewSheet => app.invoke_new_sheet(),
                        PaletteAction::OpenSheet => app.invoke_open_sheet(),
                        PaletteAction::SaveSheet => app.invoke_save_sheet(),
                        PaletteAction::SaveAsSheet => app.invoke_save_as_sheet(),
                        PaletteAction::ExportCsv => app.invoke_export_csv(),
                        PaletteAction::Undo => app.invoke_undo(),
                        PaletteAction::Redo => app.invoke_redo(),
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_workbook_is_blank_and_named_untitled() {
        let sheet = blank_sheet();
        assert!(sheet.cells.is_empty());
        assert_eq!(sheet.name, "Untitled");
    }

    #[test]
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
    fn formula_bar_draft_is_not_applied_before_commit() {
        let mut sheet = Sheet::new("test");
        let selected = CellRef::parse("B1").unwrap();
        sheet.set_str("A1", "2");
        sheet.set_str("B1", "3");

        let mut edit = CellEditTransaction::begin(sheet.raw(selected));
        edit.update("=A1+1");

        assert_eq!(selected.to_a1(), "B1");
        assert_eq!(edit.commit().unwrap().after(), "=A1+1");
        assert_eq!(sheet.raw(selected), Some("3"));
    }

    #[test]
    fn formula_bar_commit_preserves_formula_raw_and_selected_cell() {
        let mut sheet = Sheet::new("test");
        let selected = CellRef::parse("B1").unwrap();
        sheet.set_str("A1", "2");
        sheet.set_str("B1", "3");
        let mut undo = Vec::new();
        let mut redo = Vec::new();

        assert!(commit_formula_edit(
            &mut sheet, &mut undo, &mut redo, selected, "=A1+1",
        ));

        assert_eq!(selected.to_a1(), "B1");
        assert_eq!(sheet.raw(selected), Some("=A1+1"));
        assert_eq!(evaluate(&sheet).get(&selected), Some(&Value::Number(3.0)));
    }

    #[test]
    fn formula_bar_commit_preserves_literal_and_empty_raw_text() {
        let mut sheet = Sheet::new("test");
        let literal = CellRef::parse("A1").unwrap();
        let empty = CellRef::parse("B1").unwrap();
        sheet.set_raw(literal, "old");
        sheet.set_raw(empty, "old");
        let mut undo = Vec::new();
        let mut redo = Vec::new();

        assert!(commit_formula_edit(
            &mut sheet,
            &mut undo,
            &mut redo,
            literal,
            "  literal text  ",
        ));
        assert!(commit_formula_edit(
            &mut sheet, &mut undo, &mut redo, empty, "",
        ));

        assert_eq!(sheet.raw(literal), Some("  literal text  "));
        assert_eq!(sheet.raw(empty), Some(""));
        assert_eq!(evaluate(&sheet).get(&empty), Some(&Value::Empty));
    }

    #[test]
    fn formula_bar_commit_records_one_transaction_and_noop_records_none() {
        let mut sheet = Sheet::new("test");
        let selected = CellRef::parse("A1").unwrap();
        sheet.set_str("A1", "old");
        let mut undo = Vec::new();
        let mut redo = vec!["redo".to_string()];

        assert!(commit_formula_edit(
            &mut sheet, &mut undo, &mut redo, selected, "new",
        ));
        assert_eq!(undo.len(), 1);
        assert!(redo.is_empty());

        assert!(!commit_formula_edit(
            &mut sheet, &mut undo, &mut redo, selected, "new",
        ));
        assert_eq!(undo.len(), 1);
    }

    #[test]
    fn quick_formula_insert_evaluation() {
        let mut sheet = Sheet::new("test");
        sheet.set_str("A1", "10");
        sheet.set_str("A2", "20");
        sheet.set_str("A3", "30");
        sheet.set_str("A4", "40");
        sheet.set_str("A5", "50");

        let target = CellRef::parse("B1").unwrap();
        let mut undo = Vec::new();
        let mut redo = Vec::new();

        assert!(commit_formula_edit(
            &mut sheet,
            &mut undo,
            &mut redo,
            target,
            "=SUM(A1:A5)",
        ));

        let vals = evaluate(&sheet);
        assert_eq!(vals.get(&target), Some(&Value::Number(150.0)));
    }
}
