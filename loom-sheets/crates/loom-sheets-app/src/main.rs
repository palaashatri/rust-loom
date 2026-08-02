//! Loom Sheets desktop application.
//!
//! GUI mode opens a real window (winit backend). Headless modes
//! (`--screenshot`, `--smoke`) render the same UI through the software
//! renderer and write a PNG, which is what the Docker visual-QA pipeline
//! and the offline test mode exercise.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use loom_package::manifest::{json as pkg_json, Checksum, Manifest, ManifestEntry};
use loom_package::{MimeType, PackageArchive, PackageKind, SchemaVersion};
use loom_sheets_core::{
    evaluate, from_csv, sheet_from_json, sheet_to_json, to_csv, CellEditTransaction, CellRef,
    Sheet, Value,
};
use loom_test_support::capture::{set_platform, snapshot_component};
use slint::{ComponentHandle, Model, ModelRc, PhysicalSize, SharedString, VecModel};

slint::include_modules!();

const DEFAULT_SIZE: (u32, u32) = (1280, 800);
const GRID_COLS: usize = 8;
const GRID_ROWS: usize = 6;
const SAVE_FILENAME: &str = "loom-sheets-workbook.loomtable";
const EXPORT_FILENAME: &str = "loom-sheets-export.csv";

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

fn load_sheet(path: &str) -> Result<Sheet, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("read {path}: {e}"))?;
    if path.to_lowercase().ends_with(".csv") {
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

fn save_sheet(path: &str, sheet: &Sheet) -> Result<(), String> {
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
    std::fs::write(path, bytes).map_err(|e| format!("write {path}: {e}"))
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
        Some(p) => load_sheet(p)?,
        None => sample_sheet(),
    };
    apply_sheet(&app, &sheet);
    let (w, h) = args.size;
    let img = snapshot_component(&app, w as f32, h as f32, 1.0).map_err(|e| e.to_string())?;
    loom_test_support::png::save_png(Path::new(out), &img).map_err(|e| e.to_string())?;
    Ok(())
}

struct GuiState {
    current: RefCell<Sheet>,
    save_path: RefCell<Option<String>>,
    undo_stack: RefCell<Vec<String>>,
    redo_stack: RefCell<Vec<String>>,
}

fn run_gui(args: &Args) -> Result<(), String> {
    let app = SheetsApp::new().map_err(|e| e.to_string())?;
    apply_theme(&app, &args.theme);
    app.window()
        .set_size(PhysicalSize::new(args.size.0, args.size.1));

    let state = Rc::new(GuiState {
        current: RefCell::new(match &args.open {
            Some(p) => load_sheet(p)?,
            None => sample_sheet(),
        }),
        save_path: RefCell::new(args.open.clone()),
        undo_stack: RefCell::new(Vec::new()),
        redo_stack: RefCell::new(Vec::new()),
    });

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_new_sheet(move || {
            if let Some(app) = app_ref.upgrade() {
                state
                    .undo_stack
                    .borrow_mut()
                    .push(sheet_to_json(&state.current.borrow()));
                state.redo_stack.borrow_mut().clear();
                *state.current.borrow_mut() = sample_sheet();
                apply_sheet(&app, &state.current.borrow());
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
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_open_sheet(move || {
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
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_save_sheet(move || {
            if let Some(app) = app_ref.upgrade() {
                let p = state
                    .save_path
                    .borrow()
                    .clone()
                    .unwrap_or_else(|| SAVE_FILENAME.to_string());
                match save_sheet(&p, &state.current.borrow()) {
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
        app.on_export_csv(move || {
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
    run_gui(&args)
}

#[cfg(test)]
mod tests {
    use super::*;

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
