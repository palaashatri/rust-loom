//! Loom Sheets desktop application.
//!
//! GUI mode opens a real window (winit backend). Headless modes
//! (`--screenshot`, `--smoke`) render the same UI through the software
//! renderer and write a PNG, which is what the Docker visual-QA pipeline
//! and the offline test mode exercise.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use loom_desktop::{
    build_standard_menu_bar, CommandAction, CommandStateProjection, DesktopError,
    FileDialogService, FileFilter, Menu, MenuBarService, MenuItem, MenuShortcut, NativeFileDialogs,
    NativeMenuBar, OpenFileRequest, SaveFileRequest,
};
use loom_package::manifest::{json as pkg_json, Checksum, Manifest, ManifestEntry};
use loom_package::{MimeType, PackageArchive, PackageKind, SchemaVersion};
use loom_sheets_core::{
    evaluate, from_csv, sheet_from_json, sheet_to_json, to_csv, CellEditTransaction, CellRange,
    CellRef, GridSelection, RangeEdit, Sheet, SheetDimensions, SheetViewport, Value,
};
use loom_test_support::capture::{set_platform, snapshot_component};
use loom_test_support::journey::{record_keyboard_palette_journey, PaletteProbe};
use slint::{ComponentHandle, Model, ModelRc, PhysicalSize, SharedString, VecModel};

slint::include_modules!();

const DEFAULT_SIZE: (u32, u32) = (1280, 800);
const DEFAULT_VISIBLE_COLS: u32 = 8;
const DEFAULT_VISIBLE_ROWS: u32 = 15;
const GRID_ROW_HEIGHT: f32 = 28.0;
const GRID_COL_WIDTH: f32 = 90.0;
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
    template_chooser: bool,
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
        template_chooser: false,
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
            "--template-chooser" => args.template_chooser = true,
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
    loom_storage::atomic_write(path, &bytes)
        .map_err(|error| format!("atomic write {}: {error}", path.display()))
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

/// The small, renderable slice of a sparse worksheet shown by the Slint grid.
/// Cell coordinates in this structure are local to the viewport; the
/// `SheetViewport` owns their corresponding worksheet offsets.
struct ProjectedSheetGrid {
    rows: Vec<i32>,
    cols: Vec<i32>,
    column_headers: Vec<String>,
    row_headers: Vec<String>,
    cells: Vec<String>,
}

fn project_sheet_grid_with_values(
    sheet: &Sheet,
    values: &std::collections::HashMap<CellRef, Value>,
    viewport: SheetViewport,
) -> ProjectedSheetGrid {
    let rows: Vec<i32> = (0..viewport.visible_rows).map(|row| row as i32).collect();
    let cols: Vec<i32> = (0..viewport.visible_cols).map(|col| col as i32).collect();
    let column_headers = (0..viewport.visible_cols)
        .filter_map(|index| viewport.column_at(index))
        .map(|column| {
            CellRef {
                row: 0,
                col: column,
            }
            .to_a1()
            .trim_end_matches('1')
            .to_string()
        })
        .collect();
    let row_headers = (0..viewport.visible_rows)
        .filter_map(|index| viewport.row_at(index))
        .map(|row| (row + 1).to_string())
        .collect();
    let mut cells = Vec::with_capacity((viewport.visible_rows * viewport.visible_cols) as usize);
    for local_row in 0..viewport.visible_rows {
        for local_col in 0..viewport.visible_cols {
            let Some(row) = viewport.row_at(local_row) else {
                continue;
            };
            let Some(col) = viewport.column_at(local_col) else {
                continue;
            };
            cells.push(cell_value(sheet, values, row, col));
        }
    }

    ProjectedSheetGrid {
        rows,
        cols,
        column_headers,
        row_headers,
        cells,
    }
}

#[cfg(test)]
fn project_sheet_grid(sheet: &Sheet, viewport: SheetViewport) -> ProjectedSheetGrid {
    let values = evaluate(sheet);
    project_sheet_grid_with_values(sheet, &values, viewport)
}

fn viewport_from_app(app: &SheetsApp, sheet: &Sheet) -> SheetViewport {
    let selected = selection_from_app(app).focus;
    let mut dimensions = sheet.dimensions();
    let viewport_width = if app.get_grid_viewport_width() > 1.0 {
        app.get_grid_viewport_width()
    } else {
        GRID_COL_WIDTH * DEFAULT_VISIBLE_COLS as f32
    };
    let viewport_height = if app.get_grid_viewport_height() > 1.0 {
        app.get_grid_viewport_height()
    } else {
        GRID_ROW_HEIGHT * DEFAULT_VISIBLE_ROWS as f32
    };
    // Keep an empty/new sheet navigable beyond A1 while retaining sparse
    // workbook dimensions for populated sheets.
    dimensions = SheetDimensions::new(
        dimensions
            .rows
            .max(selected.row.saturating_add(1))
            .max(DEFAULT_VISIBLE_ROWS),
        dimensions
            .cols
            .max(selected.col.saturating_add(1))
            .max(DEFAULT_VISIBLE_COLS),
    );
    let scroll_x = (-app.get_grid_scroll_x()).max(0.0);
    let scroll_y = (-app.get_grid_scroll_y()).max(0.0);
    SheetViewport::from_scroll(
        scroll_x,
        scroll_y,
        viewport_width,
        viewport_height,
        GRID_ROW_HEIGHT,
        GRID_COL_WIDTH,
        dimensions,
    )
}

fn apply_sheet(app: &SheetsApp, sheet: &Sheet) {
    apply_sheet_inner(app, sheet, true);
}

fn apply_sheet_without_reveal(app: &SheetsApp, sheet: &Sheet) {
    apply_sheet_inner(app, sheet, false);
}

fn apply_sheet_inner(app: &SheetsApp, sheet: &Sheet, reveal_selection: bool) {
    let vals = evaluate(sheet);
    let selection = selection_from_app(app);
    let selected = selection.focus;
    let dimensions = sheet.dimensions();
    let editor_dimensions = SheetDimensions::new(
        dimensions
            .rows
            .max(selected.row.saturating_add(1))
            .max(DEFAULT_VISIBLE_ROWS),
        dimensions
            .cols
            .max(selected.col.saturating_add(1))
            .max(DEFAULT_VISIBLE_COLS),
    );
    // Set content extents before touching Flickable offsets.  The two-way
    // viewport binding clamps offsets against these extents, so updating them
    // first preserves a requested tail scroll on a newly loaded sparse sheet.
    app.set_workbook_rows(editor_dimensions.rows as i32);
    app.set_workbook_cols(editor_dimensions.cols as i32);
    let current_scroll_x = (-app.get_grid_scroll_x()).max(0.0);
    let current_scroll_y = (-app.get_grid_scroll_y()).max(0.0);
    let mut viewport = viewport_from_app(app, sheet);
    let projected_before_reveal = viewport;
    if reveal_selection {
        viewport.reveal(selected);
    }
    app.set_view_row_origin(viewport.first_row as i32);
    app.set_view_col_origin(viewport.first_col as i32);
    // Flickable coordinates are negative because its content is translated
    // opposite to the positive worksheet scroll offset. Preserve fractional
    // wheel/touchpad offsets while snapping only when selection auto-reveal
    // moved the projected window.
    if viewport.first_col != projected_before_reveal.first_col {
        app.set_grid_scroll_x(-(viewport.first_col as f32 * GRID_COL_WIDTH));
    } else {
        app.set_grid_scroll_x(-current_scroll_x);
    }
    if viewport.first_row != projected_before_reveal.first_row {
        app.set_grid_scroll_y(-(viewport.first_row as f32 * GRID_ROW_HEIGHT));
    } else {
        app.set_grid_scroll_y(-current_scroll_y);
    }
    let grid = project_sheet_grid_with_values(sheet, &vals, viewport);

    app.set_cols(ModelRc::new(VecModel::from(grid.cols)));
    app.set_rows(ModelRc::new(VecModel::from(grid.rows)));
    app.set_column_headers(ModelRc::new(VecModel::from(
        grid.column_headers
            .into_iter()
            .map(SharedString::from)
            .collect::<Vec<_>>(),
    )));
    app.set_row_headers(ModelRc::new(VecModel::from(
        grid.row_headers
            .into_iter()
            .map(SharedString::from)
            .collect::<Vec<_>>(),
    )));
    app.set_cells(ModelRc::new(VecModel::from(
        grid.cells
            .into_iter()
            .map(SharedString::from)
            .collect::<Vec<_>>(),
    )));
    update_selection_range(app, sheet, &vals, selection);
    app.set_table_rows_label(SharedString::from(dimensions.rows.to_string()));
    app.set_table_cols_label(SharedString::from(dimensions.cols.to_string()));
    app.set_selected_row_height(SharedString::from(format!(
        "{:.0} px",
        sheet.row_height(selected.row)
    )));
    app.set_selected_col_width(SharedString::from(format!(
        "{:.0} px",
        sheet.col_width(selected.col)
    )));
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

fn sync_history_controls(app: &SheetsApp, state: &GuiState) {
    app.set_can_undo(!state.undo_stack.borrow().is_empty());
    app.set_can_redo(!state.redo_stack.borrow().is_empty());
}

/// Build the live state consumed by the standard desktop menu.  Undo/redo
/// enablement is derived from the same stacks that back the visible editor;
/// inspector check state is derived from the actual window properties.
fn menu_projection(
    menu_service: &NativeMenuBar,
    app: &SheetsApp,
) -> Result<CommandStateProjection, DesktopError> {
    let menu_bar = menu_service
        .installed_menu_bar()
        .ok_or_else(|| DesktopError::InvalidRequest("Sheets menu bar is not installed".into()))?;
    let mut projection = menu_bar.command_state_projection();

    let mut undo = projection
        .get("edit.undo")
        .cloned()
        .ok_or_else(|| DesktopError::InvalidRequest("Sheets menu is missing edit.undo".into()))?;
    undo.enabled = app.get_can_undo();
    projection.insert(undo);

    let mut redo = projection
        .get("edit.redo")
        .cloned()
        .ok_or_else(|| DesktopError::InvalidRequest("Sheets menu is missing edit.redo".into()))?;
    redo.enabled = app.get_can_redo();
    projection.insert(redo);

    let mut inspector = projection.get("view.inspector").cloned().ok_or_else(|| {
        DesktopError::InvalidRequest("Sheets menu is missing view.inspector".into())
    })?;
    inspector.enabled = app.get_inspector_available();
    inspector.checked = Some(app.get_show_inspector());
    projection.insert(inspector);

    Ok(projection)
}

/// Push the current command projection to the installed native menu after a
/// state transition. The adapter performs strict installed/ID validation;
/// application mutations remain authoritative even if an OS menu backend is
/// unavailable.
fn sync_menu_state_result(
    menu_service: &NativeMenuBar,
    app: &SheetsApp,
    state: &GuiState,
) -> Result<(), DesktopError> {
    sync_history_controls(app, state);
    rebuild_palette(app, app.get_palette_query().as_str());
    let projection = menu_projection(menu_service, app)?;
    menu_service.sync_command_states(&projection)
}

fn sync_menu_state(menu_service: &NativeMenuBar, app: &SheetsApp, state: &GuiState) {
    if let Err(error) = sync_menu_state_result(menu_service, app, state) {
        app.set_status_right(SharedString::from(format!("Menu update failed: {error}")));
    }
}

fn selection_from_app(app: &SheetsApp) -> GridSelection {
    let focus = CellRef::parse(app.get_selected_cell().as_str()).unwrap_or(CellRef {
        row: app.get_selected_row().max(0) as u32,
        col: app.get_selected_col().max(0) as u32,
    });
    let anchor = CellRef {
        row: app.get_selection_anchor_row().max(0) as u32,
        col: app.get_selection_anchor_col().max(0) as u32,
    };
    GridSelection::new(anchor, focus)
}

fn update_selection(
    app: &SheetsApp,
    sheet: &Sheet,
    vals: &std::collections::HashMap<CellRef, Value>,
    selected: CellRef,
) {
    update_selection_range(app, sheet, vals, GridSelection::new(selected, selected));
}

fn update_selection_range(
    app: &SheetsApp,
    sheet: &Sheet,
    vals: &std::collections::HashMap<CellRef, Value>,
    selection: GridSelection,
) {
    let selected = selection.focus;
    let range = selection.range();
    let formula = sheet
        .raw(selected)
        .map(SharedString::from)
        .unwrap_or_default();
    app.set_selection_formula(formula);
    app.invoke_reset_formula_edit_buffer();
    app.set_selected_cell(selected.to_a1().into());
    app.set_selected_row(selected.row as i32);
    app.set_selected_col(selected.col as i32);
    app.set_selection_anchor_row(selection.anchor.row as i32);
    app.set_selection_anchor_col(selection.anchor.col as i32);
    app.set_selection_start_row(range.start.row as i32);
    app.set_selection_start_col(range.start.col as i32);
    app.set_selection_end_row(range.end.row as i32);
    app.set_selection_end_col(range.end.col as i32);
    app.set_selection_range(range.to_a1().into());
    app.set_selection_count((range.cells().len() as i32).max(1));
    let display = cell_value(sheet, vals, selected.row, selected.col);
    let formula_text = sheet.raw(selected).unwrap_or("");
    let value_text = if display.is_empty() {
        "Empty".to_string()
    } else {
        display.clone()
    };
    let formula_suffix = if formula_text.is_empty() {
        String::new()
    } else {
        format!("; formula: {formula_text}")
    };
    app.set_selection_announcement(SharedString::from(format!(
        "{} selected; value: {}{}",
        selection.label(),
        value_text,
        formula_suffix
    )));
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

/// Apply a sparse range edit as one undoable transaction.  The serialized
/// snapshot is deliberately kept at the controller boundary for now so all
/// existing save/reopen and recovery paths observe the same document state.
fn commit_range_edit(
    sheet: &mut Sheet,
    undo_stack: &mut Vec<String>,
    redo_stack: &mut Vec<String>,
    edit: RangeEdit,
) -> bool {
    if edit.is_empty() || edit.is_noop() {
        return false;
    }
    undo_stack.push(sheet_to_json(sheet));
    redo_stack.clear();
    edit.apply(sheet);
    true
}

/// The Sheets fill handle repeats a selected block into the next block below
/// it.  A single-cell selection has no fill source and is therefore disabled
/// in the UI rather than pretending to perform an operation.
fn fill_selection_down(
    sheet: &mut Sheet,
    undo_stack: &mut Vec<String>,
    redo_stack: &mut Vec<String>,
    selection: GridSelection,
) -> bool {
    let source = selection.range();
    let Some(target) = fill_target_range(source) else {
        return false;
    };
    let edit = RangeEdit::fill(sheet, source, target);
    commit_range_edit(sheet, undo_stack, redo_stack, edit)
}

fn fill_target_range(source: CellRange) -> Option<CellRange> {
    if source.start == source.end {
        return None;
    }
    let height = source.end.row - source.start.row + 1;
    let target_end_row = source.end.row.checked_add(height)?;
    Some(CellRange::new(
        CellRef {
            row: source.end.row.saturating_add(1),
            col: source.start.col,
        },
        CellRef {
            row: target_end_row,
            col: source.end.col,
        },
    ))
}

fn select_cell(app: &SheetsApp, sheet: &Sheet, r: i32, c: i32) {
    if r < 0 || c < 0 {
        return;
    }
    let (r, c) = (r as u32, c as u32);
    let refr = CellRef { row: r, col: c };
    let vals = evaluate(sheet);
    update_selection(app, sheet, &vals, refr);
}

fn offset_coordinate(value: u32, delta: i32) -> u32 {
    if delta < 0 {
        value.saturating_sub(delta.unsigned_abs())
    } else {
        value.saturating_add(delta as u32)
    }
}

fn navigate_selection(app: &SheetsApp, sheet: &Sheet, row_delta: i32, col_delta: i32) {
    let selection = selection_from_app(app);
    let selected = selection.focus;
    let next = CellRef {
        row: offset_coordinate(selected.row, row_delta),
        col: offset_coordinate(selected.col, col_delta),
    };
    update_selection(app, sheet, &evaluate(sheet), next);
}

fn extend_selection(app: &SheetsApp, sheet: &Sheet, row_delta: i32, col_delta: i32) {
    let selection = selection_from_app(app);
    let focus = selection.focus;
    let next = CellRef {
        row: offset_coordinate(focus.row, row_delta),
        col: offset_coordinate(focus.col, col_delta),
    };
    update_selection_range(app, sheet, &evaluate(sheet), selection.extend(next));
}

fn inspector_section_visibility(query: &str) -> (bool, bool) {
    let query = query.trim().to_ascii_lowercase();
    let table = query.is_empty() || "table name rows columns worksheet".contains(query.as_str());
    let cell = query.is_empty()
        || "cell selection value formula row height column width".contains(query.as_str());
    (table, cell)
}

fn apply_theme(app: &SheetsApp, theme: &str) {
    Theme::get(app).set_active_theme(SharedString::from(theme));
}

fn layout_breakpoints(width: u32) -> (bool, bool) {
    (width >= 1320, width >= 1180)
}

fn apply_layout_breakpoints(app: &SheetsApp, width: u32) {
    let (show_quick_formulas, labeled_export) = layout_breakpoints(width);
    app.set_show_quick_formulas(show_quick_formulas);
    app.set_wide_toolbar(show_quick_formulas);
    app.set_labeled_export(labeled_export);
    let overflow_toolbar = width < 1180;
    if !overflow_toolbar && app.get_toolbar_overflow_open() {
        app.invoke_close_toolbar_overflow();
    }
    app.set_overflow_toolbar(overflow_toolbar);
    if !overflow_toolbar {
        app.set_toolbar_overflow_open(false);
    }
    let inspector_available = width >= 1180;
    app.set_inspector_available(inspector_available);
    if !inspector_available {
        app.set_show_inspector(false);
    }
}

#[allow(dead_code)] // exercised by headless breakpoint/focus regression tests
fn wire_responsive_layout(app: &SheetsApp) {
    let app_ref = app.as_weak();
    app.on_window_resized(move |width| {
        if let Some(app) = app_ref.upgrade() {
            apply_layout_breakpoints(&app, width.max(0.0) as u32);
        }
    });
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
        // The filtered screenshot probe has one matching export command.
        // Keep the preview selection within that list so the Flickable does
        // not scroll its only row into the clipped viewport.
        app.set_palette_selected(0);
        app.set_palette_open(true);
    }
    if args.template_chooser {
        app.set_template_chooser_open(true);
    }
    let (w, h) = args.size;
    apply_layout_breakpoints(&app, w);
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
    apply_layout_breakpoints(&app, args.size.0);

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
    // One menu adapter owns the application sink for its entire lifetime so
    // accepted native actions and toolbar/palette callbacks share a route.
    let menu_service = Arc::new(NativeMenuBar::new());

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_window_resized(move |width| {
            if let Some(app) = app_ref.upgrade() {
                apply_layout_breakpoints(&app, width.max(0.0) as u32);
                sync_menu_state(&menu_service, &app, &state);
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_new_sheet(move || {
            if let Some(app) = app_ref.upgrade() {
                *state.current.borrow_mut() = blank_sheet();
                *state.save_path.borrow_mut() = None;
                state.undo_stack.borrow_mut().clear();
                state.redo_stack.borrow_mut().clear();
                apply_sheet(&app, &state.current.borrow());
                sync_menu_state(&menu_service, &app, &state);
                app.set_status_left("Created unsaved workbook".into());
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
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
                        sync_menu_state(&menu_service, &app, &state);
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
        let menu_service = menu_service.clone();
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
                        sync_menu_state(&menu_service, &app, &state);
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
        let menu_service = menu_service.clone();
        app.on_open_sheet(move || {
            if let Some(app) = app_ref.upgrade() {
                match state.dialogs.open_file(&open_request(&state)) {
                    Ok(Some(path)) => match load_sheet(&path) {
                        Ok(sheet) => {
                            let imported = !is_native_workbook(&path);
                            replace_opened_sheet(&app, &state, path.clone(), sheet);
                            sync_menu_state(&menu_service, &app, &state);
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
                        match loom_storage::atomic_write(&path, csv.as_bytes()) {
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
        let menu_service = menu_service.clone();
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
                        sync_menu_state(&menu_service, &app, &state);
                    }
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
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
                        sync_menu_state(&menu_service, &app, &state);
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
                apply_sheet(&app, &state.current.borrow());
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_navigate_selection(move |row_delta, col_delta| {
            if let Some(app) = app_ref.upgrade() {
                navigate_selection(&app, &state.current.borrow(), row_delta, col_delta);
                apply_sheet(&app, &state.current.borrow());
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_extend_selection(move |row_delta, col_delta| {
            if let Some(app) = app_ref.upgrade() {
                extend_selection(&app, &state.current.borrow(), row_delta, col_delta);
                apply_sheet(&app, &state.current.borrow());
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_fill_selection(move || {
            if let Some(app) = app_ref.upgrade() {
                let changed = {
                    let selection = selection_from_app(&app);
                    let mut current = state.current.borrow_mut();
                    let mut undo = state.undo_stack.borrow_mut();
                    let mut redo = state.redo_stack.borrow_mut();
                    fill_selection_down(&mut current, &mut undo, &mut redo, selection)
                };
                if changed {
                    let source = selection_from_app(&app).range();
                    if let Some(target) = fill_target_range(source) {
                        let expanded = CellRange::new(source.start, target.end);
                        update_selection_range(
                            &app,
                            &state.current.borrow(),
                            &evaluate(&state.current.borrow()),
                            GridSelection::new(source.start, expanded.end),
                        );
                    }
                    apply_sheet(&app, &state.current.borrow());
                    sync_menu_state(&menu_service, &app, &state);
                    app.set_formula_feedback(SharedString::from(format!(
                        "Filled {} down",
                        app.get_selection_range()
                    )));
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_grid_scrolled(move || {
            if let Some(app) = app_ref.upgrade() {
                apply_sheet_without_reveal(&app, &state.current.borrow());
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_grid_viewport_changed(move |width, height| {
            if let Some(app) = app_ref.upgrade() {
                // Ignore the transient zero-size pass during component
                // construction; subsequent layout changes carry real bounds.
                if width > 1.0 && height > 1.0 {
                    app.set_grid_viewport_width(width);
                    app.set_grid_viewport_height(height);
                    apply_sheet(&app, &state.current.borrow());
                }
            }
        });
    }
    {
        let app_ref = app.as_weak();
        app.on_inspector_search_edited(move |query| {
            if let Some(app) = app_ref.upgrade() {
                let (table, cell) = inspector_section_visibility(query.as_str());
                app.set_inspector_show_table(table);
                app.set_inspector_show_cell(cell);
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_toggle_inspector(move || {
            if let Some(app) = app_ref.upgrade() {
                if app.get_inspector_available() {
                    app.set_show_inspector(!app.get_show_inspector());
                    sync_menu_state(&menu_service, &app, &state);
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_create_template(move |idx| {
            if let Some(app) = app_ref.upgrade() {
                let sheet = match idx {
                    1 => {
                        let mut s = Sheet::new("Monthly Budget");
                        s.set_str("A1", "Category");
                        s.set_str("B1", "Projected");
                        s.set_str("C1", "Actual");
                        s.set_str("A2", "Housing");
                        s.set_str("B2", "1200");
                        s.set_str("C2", "1200");
                        s.set_str("A3", "Food");
                        s.set_str("B3", "400");
                        s.set_str("C3", "450");
                        s.set_str("A4", "Total");
                        s.set_str("B4", "=SUM(B2:B3)");
                        s.set_str("C4", "=SUM(C2:C3)");
                        s
                    }
                    2 => {
                        let mut s = Sheet::new("Invoice");
                        s.set_str("A1", "Description");
                        s.set_str("B1", "Hours");
                        s.set_str("C1", "Rate");
                        s.set_str("D1", "Amount");
                        s.set_str("A2", "Design Work");
                        s.set_str("B2", "20");
                        s.set_str("C2", "85");
                        s.set_str("D2", "=B2*C2");
                        s
                    }
                    _ => blank_sheet(),
                };
                *state.current.borrow_mut() = sheet;
                *state.save_path.borrow_mut() = None;
                state.undo_stack.borrow_mut().clear();
                state.redo_stack.borrow_mut().clear();
                apply_sheet(&app, &state.current.borrow());
                sync_menu_state(&menu_service, &app, &state);
                app.set_template_chooser_open(false);
                app.set_status_left("Created template workbook".into());
            }
        });
    }
    {
        let app_ref = app.as_weak();
        app.on_cancel_template(move || {
            if let Some(app) = app_ref.upgrade() {
                app.set_template_chooser_open(false);
            }
        });
    }

    let mut menu_bar = build_standard_menu_bar(
        "Loom Sheets",
        vec![MenuItem::action_with_shortcut(
            "file.export_csv",
            "Export to CSV...",
            MenuShortcut::primary("E"),
        )],
        vec![],
        vec![MenuItem::check("view.inspector", "Format Inspector", false)],
        vec![Menu::new(
            "Table",
            vec![
                MenuItem::action("table.add_row", "Add Row"),
                MenuItem::action("table.add_col", "Add Column"),
            ],
        )],
    );
    // Only commands with a registered Sheets/controller sink are enabled.
    // Application/window/help entries remain disabled until a real native
    // host bridge is installed for them. Table actions and unsupported
    // standard commands therefore cannot appear executable.
    menu_bar.disable_items_except([
        "file.new",
        "file.open",
        "file.save",
        "file.save_as",
        "file.export_csv",
        "edit.undo",
        "edit.redo",
        "app.palette",
        "view.inspector",
    ]);
    menu_service
        .install_menu_bar(&menu_bar)
        .map_err(|error| error.to_string())?;
    let app_ref = app.as_weak();
    menu_service
        .register_action_sink(Arc::new(move |action: CommandAction| {
            schedule_menu_action(&app_ref, action)
        }))
        .map_err(|error| error.to_string())?;

    apply_sheet(&app, &state.current.borrow());
    sync_menu_state_result(&menu_service, &app, &state).map_err(|error| error.to_string())?;
    wire_palette(&app);
    app.show().map_err(|e| e.to_string())?;
    // A visible selection is not enough to receive keyboard input. Focus the
    // grid after the native window is shown; winit may replace the focus item
    // during presentation, so doing this before `show` is not durable.
    app.invoke_focus_grid();
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
    apply_layout_breakpoints(&app, args.size.0);
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
    run_sparse_edit_journey(args, Path::new(out_dir))?;
    Ok(())
}

/// Exercise the sparse-workbook path with the same controller helpers used by
/// the desktop app.  The journey intentionally keeps only a handful of cells
/// in a 1,000-row worksheet, then records each durable transition so visual
/// and persistence evidence can be inspected alongside the keyboard journey.
fn run_sparse_edit_journey(args: &Args, out_dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(out_dir).map_err(|error| format!("journey output: {error}"))?;
    let app = SheetsApp::new().map_err(|error| error.to_string())?;
    apply_theme(&app, &args.theme);
    app.window()
        .set_size(PhysicalSize::new(args.size.0, args.size.1));
    apply_layout_breakpoints(&app, args.size.0);
    app.set_show_inspector(true);

    let mut sheet = Sheet::new("Sparse 1000");
    sheet.set_str("A1", "10");
    sheet.set_str("A2", "20");
    sheet.set_str("A995", "10");
    sheet.set_str("A996", "20");
    sheet.set_str("A1000", "tail");
    let mut undo = Vec::new();
    let mut redo = Vec::new();

    apply_sheet(&app, &sheet);
    capture_journey_frame(&app, out_dir, "01-start", args.size)?;

    // Scroll to the sparse tail.  The negative Flickable viewport coordinate
    // is the same value that touchpad/mouse wheel gestures update.
    app.set_grid_scroll_y(-26_600.0);
    apply_sheet_without_reveal(&app, &sheet);
    capture_journey_frame(&app, out_dir, "02-scroll", args.size)?;

    let tail_selection = GridSelection::new(
        CellRef::parse("A995").expect("valid source coordinate"),
        CellRef::parse("A996").expect("valid source coordinate"),
    );
    update_selection_range(&app, &sheet, &evaluate(&sheet), tail_selection);
    apply_sheet(&app, &sheet);
    capture_journey_frame(&app, out_dir, "03-range", args.size)?;

    // Enter a formula in the active cell's adjacent column, then restore the
    // range as the fill source.  Each operation contributes exactly one
    // snapshot to the existing undo stack.
    let formula_cell = CellRef::parse("B995").expect("valid formula coordinate");
    assert!(commit_formula_edit(
        &mut sheet,
        &mut undo,
        &mut redo,
        formula_cell,
        "=A995+1",
    ));
    update_selection(&app, &sheet, &evaluate(&sheet), formula_cell);
    apply_sheet(&app, &sheet);
    capture_journey_frame(&app, out_dir, "04-formula", args.size)?;

    update_selection_range(&app, &sheet, &evaluate(&sheet), tail_selection);
    assert!(fill_selection_down(
        &mut sheet,
        &mut undo,
        &mut redo,
        tail_selection
    ));
    let filled_range = CellRange::new(
        tail_selection.range().start,
        fill_target_range(tail_selection.range())
            .expect("multi-cell range has a fill target")
            .end,
    );
    update_selection_range(
        &app,
        &sheet,
        &evaluate(&sheet),
        GridSelection::new(filled_range.start, filled_range.end),
    );
    apply_sheet(&app, &sheet);
    capture_journey_frame(&app, out_dir, "05-fill", args.size)?;
    if sheet.raw(CellRef::parse("A997").unwrap()) != Some("10")
        || sheet.raw(CellRef::parse("A998").unwrap()) != Some("20")
    {
        return Err("sparse fill journey wrote unexpected values".to_string());
    }

    let before_undo = sheet_to_json(&sheet);
    let Some(previous) = undo.pop() else {
        return Err("sparse fill journey did not record undo".to_string());
    };
    redo.push(before_undo);
    sheet = sheet_from_json(&previous).map_err(|error| format!("journey undo: {error}"))?;
    apply_sheet(&app, &sheet);
    capture_journey_frame(&app, out_dir, "06-undo", args.size)?;
    if sheet.raw(CellRef::parse("A997").unwrap()).is_some()
        || sheet.raw(CellRef::parse("A998").unwrap()).is_some()
    {
        return Err("sparse fill journey undo left filled cells".to_string());
    }

    let save_path = out_dir.join("sparse-1000.loomtable");
    save_sheet(&save_path, &sheet)?;
    let reopened = load_sheet(&save_path)?;
    if reopened.dimensions().rows != 1_000
        || reopened.raw(CellRef::parse("B995").unwrap()) != Some("=A995+1")
    {
        return Err("sparse save/reopen changed workbook semantics".to_string());
    }
    apply_sheet(&app, &reopened);
    capture_journey_frame(&app, out_dir, "07-save-reopen", args.size)?;

    let evidence = format!(
        "rows={} cells={} selection={} formula={} undo={} save={}\n",
        reopened.dimensions().rows,
        reopened.cells.len(),
        tail_selection.label(),
        reopened.raw(formula_cell).unwrap_or(""),
        sheet.raw(CellRef::parse("A997").unwrap()).is_none(),
        save_path.display(),
    );
    std::fs::write(out_dir.join("sparse-journey.txt"), evidence)
        .map_err(|error| format!("journey evidence: {error}"))?;
    println!("sparse workbook journey: PASS ({})", out_dir.display());
    Ok(())
}

fn capture_journey_frame(
    app: &SheetsApp,
    out_dir: &Path,
    name: &str,
    size: (u32, u32),
) -> Result<(), String> {
    let image = snapshot_component(app, size.0 as f32, size.1 as f32, 1.0)
        .map_err(|error| format!("capture {name}: {error}"))?;
    let path = out_dir.join(format!("{name}.png"));
    loom_test_support::png::save_png(&path, &image).map_err(|error| error.to_string())
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

/// Dispatch canonical command IDs through the same Slint callbacks used by
/// Sheets toolbar and palette controls.  The prefixed aliases keep the
/// existing palette command IDs stable while native menus use shared desktop
/// IDs.
fn dispatch_command(app: &SheetsApp, id: &str) -> bool {
    match id {
        "file.new" | "sheets.new" => app.invoke_new_sheet(),
        "file.open" | "sheets.open" => app.invoke_open_sheet(),
        "file.save" | "sheets.save" => app.invoke_save_sheet(),
        "file.save_as" | "sheets.save-as" => app.invoke_save_as_sheet(),
        "file.export_csv" | "sheets.export-csv" => app.invoke_export_csv(),
        "edit.undo" | "sheets.undo" => app.invoke_undo(),
        "edit.redo" | "sheets.redo" => app.invoke_redo(),
        "app.palette" => app.invoke_open_palette(),
        "view.inspector" => app.invoke_toggle_inspector(),
        _ => return false,
    }
    true
}

/// Queue an accepted native-menu action on Slint's event-loop thread. The
/// menu adapter may receive events from AppKit/DBus worker threads, so it must
/// not upgrade or mutate a component directly on that caller thread.
fn schedule_menu_action(
    app_ref: &slint::Weak<SheetsApp>,
    action: CommandAction,
) -> Result<(), DesktopError> {
    let CommandAction { id, .. } = action;
    let error_id = id.clone();
    app_ref
        .upgrade_in_event_loop(move |app| {
            if !dispatch_command(&app, &id) {
                app.set_status_right(SharedString::from(format!(
                    "Unsupported menu command: {id}"
                )));
            }
        })
        .map_err(|error| {
            DesktopError::InvalidRequest(format!(
                "failed to schedule Sheets menu command {error_id}: {error}"
            ))
        })
}

/// Route a palette action through the canonical command dispatcher.
fn dispatch_palette_action(app: &SheetsApp, action: PaletteAction) -> bool {
    match action {
        PaletteAction::NewSheet => dispatch_command(app, "sheets.new"),
        PaletteAction::OpenSheet => dispatch_command(app, "sheets.open"),
        PaletteAction::SaveSheet => dispatch_command(app, "sheets.save"),
        PaletteAction::SaveAsSheet => dispatch_command(app, "sheets.save-as"),
        PaletteAction::ExportCsv => dispatch_command(app, "sheets.export-csv"),
        PaletteAction::Undo if app.get_can_undo() => dispatch_command(app, "sheets.undo"),
        PaletteAction::Redo if app.get_can_redo() => dispatch_command(app, "sheets.redo"),
        PaletteAction::Undo | PaletteAction::Redo => false,
    }
}

/// Resolve a rendered palette row back to the canonical action. Invocation
/// must use the row model that Slint displayed rather than rebuilding a
/// potentially different, state-filtered list: history can change between
/// rendering and Enter/click delivery, and rebuilding would shift indices.
fn palette_action_for_id(id: &str) -> Option<PaletteAction> {
    match id {
        "sheets.new" => Some(PaletteAction::NewSheet),
        "sheets.open" => Some(PaletteAction::OpenSheet),
        "sheets.save" => Some(PaletteAction::SaveSheet),
        "sheets.save-as" => Some(PaletteAction::SaveAsSheet),
        "sheets.export-csv" => Some(PaletteAction::ExportCsv),
        "sheets.undo" => Some(PaletteAction::Undo),
        "sheets.redo" => Some(PaletteAction::Redo),
        _ => None,
    }
}

struct PaletteCommand {
    action: PaletteAction,
    id: &'static str,
    label: &'static str,
    shortcut: &'static str,
}

fn master_palette(app: &SheetsApp) -> Vec<PaletteCommand> {
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
    .into_iter()
    .filter(|c| match c.action {
        PaletteAction::Undo => app.get_can_undo(),
        PaletteAction::Redo => app.get_can_redo(),
        _ => true,
    })
    .collect()
}

fn rebuild_palette(app: &SheetsApp, query: &str) {
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
                let Some(item) = app.get_palette_commands().row_data(index as usize) else {
                    return;
                };
                if !item.enabled {
                    return;
                }
                let Some(action) = palette_action_for_id(item.id.as_str()) else {
                    return;
                };
                if dispatch_palette_action(&app, action) {
                    app.set_palette_open(false);
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
    fn sheet_grid_projection_uses_viewport_offsets_for_headers_and_cells() {
        let mut sheet = Sheet::new("test");
        sheet.set_str("C11", "Bottom right");
        sheet.set_str("D12", "Tail");
        let viewport = loom_sheets_core::SheetViewport {
            first_row: 10,
            first_col: 2,
            visible_rows: 2,
            visible_cols: 2,
        };

        let projection = project_sheet_grid(&sheet, viewport);

        assert_eq!(projection.column_headers, ["C", "D"]);
        assert_eq!(projection.row_headers, ["11", "12"]);
        assert_eq!(projection.cells, ["Bottom right", "", "", "Tail"]);
    }

    #[test]
    fn sparse_viewport_projection_tracks_scroll_and_dimensions() {
        set_platform();
        let app = SheetsApp::new().expect("create SheetsApp");
        app.set_grid_viewport_width(360.0);
        app.set_grid_viewport_height(280.0);
        app.set_grid_scroll_x(-180.0);
        app.set_grid_scroll_y(-672.0);

        let mut sheet = Sheet::new("sparse");
        sheet.set_str("AZ1000", "tail");
        let viewport = viewport_from_app(&app, &sheet);

        assert_eq!(sheet.dimensions(), SheetDimensions::new(1_000, 52));
        assert_eq!(viewport.first_row, 24);
        assert_eq!(viewport.first_col, 2);
        assert_eq!(viewport.visible_rows, 10);
        assert_eq!(viewport.visible_cols, 4);
        assert!(viewport.contains(CellRef::parse("C25").unwrap()));
        assert!(!viewport.contains(CellRef::parse("B25").unwrap()));
    }

    #[test]
    fn reverse_shift_extension_keeps_anchor_and_normalizes_range() {
        set_platform();
        let app = SheetsApp::new().expect("create SheetsApp");
        let sheet = sample_sheet();
        update_selection(
            &app,
            &sheet,
            &evaluate(&sheet),
            CellRef::parse("C3").unwrap(),
        );

        extend_selection(&app, &sheet, -1, -1);

        assert_eq!(app.get_selected_cell().as_str(), "B2");
        assert_eq!(app.get_selection_anchor_row(), 2);
        assert_eq!(app.get_selection_anchor_col(), 2);
        assert_eq!(app.get_selection_range().as_str(), "B2:C3");
        assert_eq!(app.get_selection_count(), 4);

        // Moving back through the anchor contracts the range without
        // changing which cell was the original anchor.
        extend_selection(&app, &sheet, 1, 1);
        assert_eq!(app.get_selected_cell().as_str(), "C3");
        assert_eq!(app.get_selection_anchor_row(), 2);
        assert_eq!(app.get_selection_anchor_col(), 2);
        assert_eq!(app.get_selection_range().as_str(), "C3");
        assert_eq!(app.get_selection_count(), 1);
    }

    #[test]
    fn fill_down_records_one_undo_snapshot_and_restores_sparse_cells() {
        let mut sheet = Sheet::new("fill");
        sheet.set_str("A1", "10");
        sheet.set_str("A2", "20");
        let mut undo = Vec::new();
        let mut redo = Vec::new();
        let selection =
            GridSelection::new(CellRef::parse("A1").unwrap(), CellRef::parse("A2").unwrap());

        assert!(fill_selection_down(
            &mut sheet, &mut undo, &mut redo, selection
        ));
        assert_eq!(undo.len(), 1);
        assert_eq!(sheet.raw(CellRef::parse("A3").unwrap()), Some("10"));
        assert_eq!(sheet.raw(CellRef::parse("A4").unwrap()), Some("20"));

        let previous = undo.pop().expect("fill undo snapshot");
        sheet = sheet_from_json(&previous).expect("restore fill snapshot");
        assert_eq!(sheet.raw(CellRef::parse("A3").unwrap()), None);
        assert_eq!(sheet.raw(CellRef::parse("A4").unwrap()), None);
        assert!(!fill_selection_down(
            &mut sheet,
            &mut undo,
            &mut redo,
            GridSelection::new(CellRef::parse("A1").unwrap(), CellRef::parse("A1").unwrap()),
        ));
    }

    #[test]
    fn selection_announcement_and_inspector_values_follow_live_cell_state() {
        set_platform();
        let app = SheetsApp::new().expect("create SheetsApp");
        let mut sheet = Sheet::new("live");
        sheet.set_str("A1", "2");
        sheet.set_str("B1", "=A1+1");
        let values = evaluate(&sheet);

        update_selection_range(
            &app,
            &sheet,
            &values,
            GridSelection::new(CellRef::parse("B1").unwrap(), CellRef::parse("B1").unwrap()),
        );
        assert_eq!(app.get_selection_value().as_str(), "3");
        assert_eq!(
            app.get_selection_announcement().as_str(),
            "B1 selected; value: 3; formula: =A1+1"
        );
        assert_eq!(app.get_selection_formula().as_str(), "=A1+1");

        apply_sheet(&app, &sheet);
        assert_eq!(app.get_sheet_name().as_str(), "live");
        assert_eq!(app.get_table_rows_label().as_str(), "1");
        assert_eq!(app.get_table_cols_label().as_str(), "2");
        assert_eq!(app.get_selection_value().as_str(), "3");
    }

    #[test]
    fn inspector_search_filters_table_and_cell_sections() {
        assert_eq!(inspector_section_visibility(""), (true, true));
        assert_eq!(inspector_section_visibility(" rows "), (true, false));
        assert_eq!(inspector_section_visibility("formula"), (false, true));
        assert_eq!(
            inspector_section_visibility("does-not-exist"),
            (false, false)
        );
    }

    #[test]
    fn focused_grid_routes_arrow_keys_to_selection_navigation() {
        set_platform();
        let app = SheetsApp::new().expect("create SheetsApp");
        let calls = Rc::new(std::cell::Cell::new((0, 0)));
        let calls_ref = calls.clone();
        app.on_navigate_selection(move |row_delta, col_delta| {
            calls_ref.set((row_delta, col_delta));
        });

        app.invoke_focus_grid();
        app.window()
            .dispatch_event(slint::platform::WindowEvent::KeyPressed {
                text: slint::platform::Key::DownArrow.into(),
            });
        assert_eq!(calls.get(), (1, 0));

        app.window()
            .dispatch_event(slint::platform::WindowEvent::KeyPressed {
                text: slint::platform::Key::RightArrow.into(),
            });
        assert_eq!(calls.get(), (0, 1));
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

    #[test]
    fn layout_breakpoints_match_supported_width_boundaries() {
        assert_eq!(layout_breakpoints(1024), (false, false));
        assert_eq!(layout_breakpoints(1179), (false, false));
        assert_eq!(layout_breakpoints(1180), (false, true));
        assert_eq!(layout_breakpoints(1199), (false, true));
        assert_eq!(layout_breakpoints(1319), (false, true));
        assert_eq!(layout_breakpoints(1320), (true, true));
        assert_eq!(layout_breakpoints(1440), (true, true));
    }

    #[test]
    fn sheets_inspector_is_closed_by_default() {
        set_platform();
        let app = SheetsApp::new().expect("create SheetsApp");
        assert!(!app.get_show_inspector());
    }

    #[test]
    fn native_menu_and_palette_share_sheets_callback_dispatch() {
        set_platform();
        let app = SheetsApp::new().expect("create SheetsApp");
        let calls = Rc::new(std::cell::Cell::new(0));
        let calls_ref = calls.clone();
        app.on_save_sheet(move || calls_ref.set(calls_ref.get() + 1));

        assert!(dispatch_command(&app, "file.save"));
        assert!(dispatch_palette_action(&app, PaletteAction::SaveSheet));

        let menu = NativeMenuBar::new();
        let bar = build_standard_menu_bar("Loom Sheets", vec![], vec![], vec![], vec![]);
        menu.install_menu_bar(&bar).expect("install menu");
        let app_ref = app.as_weak();
        menu.register_action_sink(Arc::new(move |action: CommandAction| {
            schedule_menu_action(&app_ref, action)
        }))
        .expect("register menu sink");
        let error = menu
            .dispatch_action("file.save")
            .expect_err("capture platform has no event loop provider");
        assert!(error
            .to_string()
            .contains("failed to schedule Sheets menu command"));

        assert_eq!(calls.get(), 2);
    }

    #[test]
    fn sheets_palette_undo_redo_follow_history_and_disabled_guard() {
        set_platform();
        let app = SheetsApp::new().expect("create SheetsApp");
        app.set_can_undo(false);
        app.set_can_redo(false);
        rebuild_palette(&app, "undo");
        assert_eq!(app.get_palette_commands().row_count(), 0);
        rebuild_palette(&app, "redo");
        assert_eq!(app.get_palette_commands().row_count(), 0);
        assert!(!dispatch_palette_action(&app, PaletteAction::Undo));
        assert!(!dispatch_palette_action(&app, PaletteAction::Redo));

        let undo_calls = Rc::new(std::cell::Cell::new(0));
        let undo_calls_ref = undo_calls.clone();
        app.on_undo(move || undo_calls_ref.set(undo_calls_ref.get() + 1));
        app.set_can_undo(true);
        rebuild_palette(&app, "undo");
        assert!(
            app.get_palette_commands()
                .row_data(0)
                .expect("Undo command")
                .enabled
        );
        assert!(dispatch_palette_action(&app, PaletteAction::Undo));
        assert_eq!(undo_calls.get(), 1);
    }

    #[test]
    fn sheets_palette_invocation_uses_rendered_row_when_history_changes() {
        set_platform();
        let app = SheetsApp::new().expect("create SheetsApp");
        let redo_calls = Rc::new(std::cell::Cell::new(0));
        let redo_calls_ref = redo_calls.clone();
        app.on_redo(move || redo_calls_ref.set(redo_calls_ref.get() + 1));

        // Render both history commands, then invalidate Undo before the user
        // presses Enter. The visible Redo row is still at index 6; resolving
        // against a freshly filtered master list would incorrectly look at
        // index 6 (out of range) or shift to the wrong command.
        app.set_can_undo(true);
        app.set_can_redo(true);
        wire_palette(&app);
        rebuild_palette(&app, "");
        assert_eq!(app.get_palette_commands().row_count(), 7);
        assert_eq!(
            app.get_palette_commands()
                .row_data(6)
                .expect("rendered Redo row")
                .id
                .as_str(),
            "sheets.redo"
        );

        app.set_can_undo(false);
        app.invoke_palette_invoked(6);

        assert_eq!(redo_calls.get(), 1);
        assert!(!app.get_palette_open());
    }

    #[test]
    fn sheets_menu_disables_unhandled_controller_commands() {
        set_platform();
        let mut menu = build_standard_menu_bar(
            "Loom Sheets",
            vec![MenuItem::action("file.export_csv", "Export to CSV...")],
            vec![],
            vec![MenuItem::check("view.inspector", "Format Inspector", false)],
            vec![Menu::new(
                "Table",
                [
                    MenuItem::action("table.add_row", "Add Row"),
                    MenuItem::action("table.add_col", "Add Column"),
                ],
            )],
        );
        menu.disable_items_except([
            "file.new",
            "file.open",
            "file.save",
            "file.save_as",
            "file.export_csv",
            "edit.undo",
            "edit.redo",
            "app.palette",
            "view.inspector",
        ]);
        for id in [
            "edit.cut",
            "edit.copy",
            "edit.paste",
            "edit.select_all",
            "view.zoom_in",
            "view.zoom_out",
            "view.zoom_actual",
            "table.add_row",
            "table.add_col",
        ] {
            assert!(
                !menu.find_item(id).expect("menu command").is_enabled(),
                "unhandled Sheets command {id} must be disabled"
            );
        }
    }

    #[test]
    fn sheets_inspector_menu_check_tracks_live_window_state() {
        set_platform();
        let app = SheetsApp::new().expect("create SheetsApp");
        let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], []));
        let state = GuiState {
            current: RefCell::new(sample_sheet()),
            save_path: RefCell::new(None),
            undo_stack: RefCell::new(Vec::new()),
            redo_stack: RefCell::new(Vec::new()),
            dialogs,
            workbook_filter: FileFilter::new("Workbook", ["loomtable"]).expect("filter"),
            import_filter: FileFilter::new("CSV", ["csv"]).expect("filter"),
            csv_filter: FileFilter::new("CSV", ["csv"]).expect("filter"),
        };
        let menu = NativeMenuBar::new();
        let bar = build_standard_menu_bar(
            "Loom Sheets",
            vec![],
            vec![],
            vec![MenuItem::check("view.inspector", "Format Inspector", false)],
            vec![],
        );
        menu.install_menu_bar(&bar).expect("install menu");

        app.set_inspector_available(true);
        app.set_show_inspector(false);
        sync_menu_state(&menu, &app, &state);
        assert!(matches!(
            menu.installed_menu_bar()
                .and_then(|bar| bar.find_item("view.inspector").cloned()),
            Some(MenuItem::Check {
                checked: false,
                enabled: true,
                ..
            })
        ));

        app.set_show_inspector(true);
        sync_menu_state(&menu, &app, &state);
        assert!(matches!(
            menu.installed_menu_bar()
                .and_then(|bar| bar.find_item("view.inspector").cloned()),
            Some(MenuItem::Check {
                checked: true,
                enabled: true,
                ..
            })
        ));
    }

    #[test]
    fn expanding_past_overflow_breakpoint_closes_menu() {
        set_platform();
        let app = SheetsApp::new().expect("create SheetsApp");
        apply_layout_breakpoints(&app, 1024);
        assert!(app.get_overflow_toolbar());
        app.set_toolbar_overflow_open(true);

        apply_layout_breakpoints(&app, 1180);

        assert!(!app.get_overflow_toolbar());
        assert!(!app.get_toolbar_overflow_open());
    }

    #[test]
    fn widening_window_preserves_palette_focus() {
        set_platform();
        let app = SheetsApp::new().expect("create SheetsApp");
        apply_layout_breakpoints(&app, 1024);
        wire_responsive_layout(&app);
        let _ = snapshot_component(&app, 1024.0, 800.0, 1.0).expect("render compact window");

        app.invoke_open_palette();
        let _ = snapshot_component(&app, 1024.0, 800.0, 1.0).expect("render open palette");
        let focused_before =
            slint::private_unstable_api::re_exports::WindowInner::from_pub(app.window())
                .focus_item
                .borrow()
                .upgrade()
                .expect("palette should own focus");

        app.window().set_size(PhysicalSize::new(1280, 800));
        let _ = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render widened window");
        let focused_after =
            slint::private_unstable_api::re_exports::WindowInner::from_pub(app.window())
                .focus_item
                .borrow()
                .upgrade()
                .expect("palette focus should remain present");

        assert_eq!(focused_after, focused_before);
        assert!(app.get_palette_open());
    }
}
