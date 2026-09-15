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
#[cfg(test)]
use loom_sheets_core::sheet_to_json;
use loom_sheets_core::style::CellStyle;
use loom_sheets_core::workbook::evaluate_workbook;
#[cfg(test)]
use loom_sheets_core::CellEditTransaction;
use loom_sheets_core::{
    evaluate, from_csv_sniffed, sheet_from_json, to_csv_with_formulas, workbook_from_json,
    workbook_to_json, CellAlignment, CellRange, CellRef, GridSelection, NumberFormat, RangeEdit,
    Sheet, SheetDimensions, SheetViewport, Value, DEFAULT_COL_WIDTH, DEFAULT_ROW_HEIGHT,
};
use loom_test_support::capture::{set_platform, snapshot_component};
use slint::{
    ComponentHandle, Image, ModelRc, PhysicalSize, Rgba8Pixel, SharedPixelBuffer, SharedString,
    VecModel,
};

slint::include_modules!();

pub mod formatting;

mod analysis;
use analysis::plan_chart;

mod assets;

mod object_actions;

mod palette;
use palette::*;

mod journey;
use journey::*;

mod actions;
use actions::*;

const DEFAULT_SIZE: (u32, u32) = (1280, 800);
const DEFAULT_VISIBLE_COLS: u32 = 8;
const DEFAULT_VISIBLE_ROWS: u32 = 15;
const GRID_ROW_HEIGHT: f32 = DEFAULT_ROW_HEIGHT;
const GRID_COL_WIDTH: f32 = DEFAULT_COL_WIDTH;
const GRID_ROW_HEADER_WIDTH: f32 = 36.0;
const GRID_COLUMN_HEADER_HEIGHT: f32 = 26.0;
const FIT_COLUMN_MAX_WIDTH: f32 = 160.0;
const INSPECTOR_WIDTH: f32 = 320.0;
const TABLE_HORIZONTAL_MARGIN: f32 = 48.0;
const SHELL_VERTICAL_CHROME: f32 = 252.0;
const SAVE_FILENAME: &str = "loom-sheets-workbook.loomtable";
const EXPORT_FILENAME: &str = "loom-sheets-export.csv";

loom_production::define_snapshot_recovery!(SHEETS_RECOVERY, "org.loom.sheets", "loom.sheets/1");

pub(crate) struct Args {
    pub(crate) screenshot: Option<String>,
    pub(crate) smoke: bool,
    pub(crate) palette: bool,
    pub(crate) chart: bool,
    pub(crate) objects: bool,
    pub(crate) journey: Option<String>,
    pub(crate) size: (u32, u32),
    pub(crate) theme: String,
    pub(crate) rtl: bool,
    pub(crate) open: Option<String>,
    pub(crate) template_chooser: bool,
    pub(crate) zoom: Option<f32>,
}

fn parse_args() -> Result<Args, String> {
    parse_args_from(std::env::args().skip(1))
}

fn parse_args_from<I, S>(raw_args: I) -> Result<Args, String>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut args = Args {
        screenshot: None,
        smoke: false,
        palette: false,
        chart: false,
        objects: false,
        journey: None,
        size: DEFAULT_SIZE,
        theme: "light".to_string(),
        rtl: false,
        open: None,
        template_chooser: false,
        zoom: None,
    };
    let mut it = raw_args.into_iter().map(Into::into);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--screenshot" => args.screenshot = Some(it.next().ok_or("--screenshot needs a path")?),
            "--smoke" => args.smoke = true,
            "--palette" => args.palette = true,
            "--chart" => args.chart = true,
            "--objects" => args.objects = true,
            "--journey" => {
                args.journey = Some(it.next().ok_or("--journey needs an output directory")?)
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
            "--rtl" => args.rtl = true,
            "--template-chooser" => args.template_chooser = true,
            "--zoom" => {
                let v: f32 = it
                    .next()
                    .ok_or("--zoom needs a factor")?
                    .parse()
                    .map_err(|_| "bad --zoom factor")?;
                if !(0.5..=3.0).contains(&v) {
                    return Err("--zoom must be between 0.5 and 3.0".to_string());
                }
                args.zoom = Some(v);
            }
            "--open" => args.open = Some(it.next().ok_or("--open needs a path")?),
            other if !other.starts_with('-') && args.open.is_none() => {
                args.open = Some(other.to_string())
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(args)
}

fn blank_sheet() -> Sheet {
    Sheet::new("Untitled")
}

/// A small, editable budget workbook used by `--smoke`, screenshots, and first launch.
pub(crate) fn starter_workbook() -> Sheet {
    let mut sheet = Sheet::new("Budget");
    for (c, v) in [
        ("A1", "Item"),
        ("A2", "Rent"),
        ("A3", "Food"),
        ("A4", "Transport"),
        ("A5", "Total"),
        ("A6", "Average"),
        ("B1", "Amount"),
        ("B2", "1200"),
        ("B3", "450"),
        ("B4", "150"),
        ("B5", "=SUM(B2:B4)"),
        ("B6", "=AVERAGE(B2:B4)"),
        ("C1", "Note"),
        ("C2", "monthly"),
        ("C3", "weekly"),
        ("C4", "monthly"),
    ] {
        sheet.set_str(c, v);
    }
    sheet
}

/// Build the workbook for a template-chooser card index. Every advertised
/// card (0-10) produces its named sheet with live formulas; unknown indices
/// fall back to a blank sheet.
pub(crate) fn template_sheet(idx: i32) -> Sheet {
    match idx {
        1 => seeded_sheet(
            "Monthly Budget",
            &[
                ("A1", "Category"),
                ("B1", "Projected"),
                ("C1", "Actual"),
                ("A2", "Housing"),
                ("B2", "1200"),
                ("C2", "1200"),
                ("A3", "Food"),
                ("B3", "400"),
                ("C3", "450"),
                ("A4", "Total"),
                ("B4", "=SUM(B2:B3)"),
                ("C4", "=SUM(C2:C3)"),
            ],
        ),
        2 => seeded_sheet(
            "Invoice",
            &[
                ("A1", "Description"),
                ("B1", "Hours"),
                ("C1", "Rate"),
                ("D1", "Amount"),
                ("A2", "Design Work"),
                ("B2", "20"),
                ("C2", "85"),
                ("D2", "=B2*C2"),
            ],
        ),
        3 => seeded_sheet(
            "Checklist",
            &[
                ("A1", "Task"),
                ("B1", "Done"),
                ("A2", "Review budget"),
                ("B2", "x"),
                ("A3", "Pay rent"),
                ("B3", "x"),
                ("A4", "Book flights"),
                ("B4", ""),
                ("A5", "Call bank"),
                ("B5", ""),
                ("A7", "Done"),
                ("B7", "=COUNTIF(B2:B5, \"x\")"),
                ("A8", "Total"),
                ("B8", "=COUNTA(A2:A5)"),
            ],
        ),
        4 => seeded_sheet(
            "Table and Chart",
            &[
                ("A1", "Quarter"),
                ("B1", "Revenue"),
                ("C1", "Expenses"),
                ("A2", "Q1"),
                ("B2", "15000"),
                ("C2", "9200"),
                ("A3", "Q2"),
                ("B3", "18500"),
                ("C3", "11000"),
                ("A4", "Total"),
                ("B4", "=SUM(B2:B3)"),
                ("C4", "=SUM(C2:C3)"),
            ],
        ),
        5 => seeded_sheet(
            "Expense Summary",
            &[
                ("A1", "Category"),
                ("B1", "Amount"),
                ("A2", "Food"),
                ("B2", "320"),
                ("A3", "Travel"),
                ("B3", "150"),
                ("A4", "Food"),
                ("B4", "85"),
                ("A5", "Utilities"),
                ("B5", "120"),
                ("A7", "Total"),
                ("B7", "=SUM(B2:B5)"),
                ("A8", "Food total"),
                ("B8", "=SUMIF(A2:A5, \"Food\", B2:B5)"),
            ],
        ),
        6 => seeded_sheet(
            "Sales Chart",
            &[
                ("A1", "Month"),
                ("B1", "Sales"),
                ("A2", "Jan"),
                ("B2", "4200"),
                ("A3", "Feb"),
                ("B3", "5100"),
                ("A4", "Mar"),
                ("B4", "4800"),
                ("A5", "Apr"),
                ("B5", "6300"),
                ("A6", "Total"),
                ("B6", "=SUM(B2:B5)"),
            ],
        ),
        7 => seeded_sheet(
            "Budget",
            &[
                ("A1", "Category"),
                ("B1", "Budget"),
                ("C1", "Spent"),
                ("A2", "Housing"),
                ("B2", "1500"),
                ("C2", "1500"),
                ("A3", "Food"),
                ("B3", "600"),
                ("C3", "520"),
                ("A4", "Total"),
                ("B4", "=SUM(B2:B3)"),
                ("C4", "=SUM(C2:C3)"),
            ],
        ),
        8 => seeded_sheet(
            "Monthly Goal",
            &[
                ("A1", "Goal"),
                ("B1", "Target"),
                ("C1", "Saved"),
                ("D1", "Progress"),
                ("A2", "Emergency fund"),
                ("B2", "5000"),
                ("C2", "3250"),
                ("D2", "=C2/B2"),
                ("A3", "Vacation"),
                ("B3", "2000"),
                ("C3", "2000"),
                ("D3", "=C3/B3"),
                ("A4", "Totals"),
                ("B4", "=SUM(B2:B3)"),
                ("C4", "=SUM(C2:C3)"),
            ],
        ),
        9 => seeded_sheet(
            "Portfolio",
            &[
                ("A1", "Holding"),
                ("B1", "Shares"),
                ("C1", "Price"),
                ("D1", "Value"),
                ("A2", "LOOM"),
                ("B2", "100"),
                ("C2", "42.5"),
                ("D2", "=B2*C2"),
                ("A3", "ACME"),
                ("B3", "50"),
                ("C3", "18"),
                ("D3", "=B3*C3"),
                ("A4", "Total"),
                ("D4", "=SUM(D2:D3)"),
            ],
        ),
        10 => seeded_sheet(
            "Net Worth",
            &[
                ("A1", "Item"),
                ("B1", "Amount"),
                ("A2", "Savings"),
                ("B2", "12000"),
                ("A3", "Investments"),
                ("B3", "8500"),
                ("A4", "Total assets"),
                ("B4", "=SUM(B2:B3)"),
                ("A5", "Credit card"),
                ("B5", "1200"),
                ("A6", "Net worth"),
                ("B6", "=B4-B5"),
            ],
        ),
        _ => blank_sheet(),
    }
}

/// Fill a named sheet from (cell, raw) pairs.
fn seeded_sheet(name: &str, cells: &[(&str, &str)]) -> Sheet {
    let mut sheet = Sheet::new(name);
    for (cell, raw) in cells {
        sheet.set_str(cell, raw);
    }
    sheet
}

/// Restore a crash-recovery payload: current workbook shape first, then the
/// legacy single-sheet shape written by older builds.
fn restore_workbook_from_snapshot(
    json: &str,
) -> Option<loom_sheets_core::persistence::WorkbookFile> {
    use loom_sheets_core::persistence::WorkbookFile;
    workbook_from_json(json).ok().or_else(|| {
        sheet_from_json(json).ok().map(|sheet| WorkbookFile {
            sheets: vec![sheet],
            active: 0,
        })
    })
}

pub(crate) fn load_sheet(path: &Path) -> Result<Sheet, String> {
    let workbook = load_workbook(path)?;
    Ok(workbook
        .sheets
        .get(workbook.active)
        .cloned()
        .unwrap_or_else(|| Sheet::new("Untitled")))
}

/// Load a workbook: all tabs plus the active tab index. Legacy single-sheet
/// packages (`content/sheet.json`) load as a one-tab workbook.
pub(crate) fn load_workbook(
    path: &Path,
) -> Result<loom_sheets_core::persistence::WorkbookFile, String> {
    use loom_sheets_core::persistence::WorkbookFile;
    let bytes = std::fs::read(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    if path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("csv"))
    {
        let csv = std::str::from_utf8(&bytes).map_err(|e| format!("csv utf8: {e}"))?;
        return Ok(WorkbookFile {
            sheets: vec![from_csv_sniffed("imported", csv)],
            active: 0,
        });
    }
    if path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("xlsx"))
    {
        let sheets = loom_sheets_core::extract_xlsx_sheets(&bytes)?;
        if sheets.is_empty() {
            return Err("xlsx workbook has no worksheets".to_string());
        }
        return Ok(WorkbookFile { sheets, active: 0 });
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
    if let Some(content) = arch.get("content/workbook.json") {
        let s = std::str::from_utf8(content).map_err(|_| "workbook not utf8".to_string())?;
        let mut workbook = workbook_from_json(s).map_err(|e| format!("workbook: {e}"))?;
        assets::attach_workbook_assets(&mut workbook, &arch)?;
        return Ok(workbook);
    }
    let content = arch
        .get("content/sheet.json")
        .ok_or_else(|| "missing sheet.json".to_string())?;
    let s = std::str::from_utf8(content).map_err(|_| "sheet not utf8".to_string())?;
    let mut sheet = sheet_from_json(s).map_err(|e| format!("sheet: {e}"))?;
    let mut workbook = loom_sheets_core::persistence::WorkbookFile {
        sheets: vec![sheet.clone()],
        active: 0,
    };
    assets::attach_workbook_assets(&mut workbook, &arch)?;
    sheet = workbook.sheets.remove(0);
    Ok(WorkbookFile {
        sheets: vec![sheet],
        active: 0,
    })
}

pub(crate) fn save_sheet(path: &Path, sheet: &Sheet) -> Result<(), String> {
    save_workbook(path, std::slice::from_ref(sheet), 0)
}

/// Save every tab plus the active tab index as `content/workbook.json`.
pub(crate) fn save_workbook(path: &Path, sheets: &[Sheet], active: usize) -> Result<(), String> {
    if sheets.is_empty() {
        return Err("cannot save an empty workbook".to_string());
    }
    let (persisted_sheets, embedded_assets) = assets::prepare_workbook(sheets)?;
    let mut arch = PackageArchive::new();
    let json = workbook_to_json(&persisted_sheets, active);
    arch.add("content/workbook.json", json.clone().into_bytes())
        .map_err(|e| e.to_string())?;
    let title = persisted_sheets
        .get(active.min(sheets.len() - 1))
        .map(|sheet| sheet.name.clone())
        .unwrap_or_else(|| "Untitled".to_string());
    let mut entries = vec![ManifestEntry {
        path: "content/workbook.json".into(),
        mime: MimeType::parse("application/vnd.loom.sheet-content")
            .map_err(|e| format!("invalid built-in sheets MIME type: {e}"))?,
        size: json.len() as u64,
        sha256: Checksum::from_bytes(loom_package::zip::sha256(json.as_bytes())),
    }];
    for asset in embedded_assets {
        let size = asset.bytes.len() as u64;
        let sha256 = Checksum::from_bytes(loom_package::zip::sha256(&asset.bytes));
        arch.add(&asset.path, asset.bytes)
            .map_err(|e| e.to_string())?;
        entries.push(ManifestEntry {
            path: asset.path,
            mime: MimeType::parse(asset.mime)
                .map_err(|e| format!("invalid image MIME type: {e}"))?,
            size,
            sha256,
        });
    }
    let manifest = Manifest {
        schema: SchemaVersion::CURRENT,
        kind: PackageKind::Sheets,
        id: "sheets-doc".to_string(),
        title,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        entries,
    };
    let manifest_str = pkg_json::write(&manifest);
    arch.add("manifest.json", manifest_str.into_bytes())
        .map_err(|e| e.to_string())?;
    let bytes = arch.to_bytes().map_err(|e| e.to_string())?;
    loom_storage::atomic_write(path, &bytes)
        .map_err(|error| format!("atomic write {}: {error}", path.display()))
}

pub(crate) fn cell_value(
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
    cell_alignments: Vec<i32>,
    cell_bolds: Vec<bool>,
    cell_italics: Vec<bool>,
    cell_underlines: Vec<bool>,
    cell_borders: Vec<bool>,
    cell_fills: Vec<i32>,
    cell_font_sizes: Vec<i32>,
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
    let cap = (viewport.visible_rows * viewport.visible_cols) as usize;
    let mut cells = Vec::with_capacity(cap);
    let mut cell_alignments = Vec::with_capacity(cap);
    let mut cell_bolds = Vec::with_capacity(cap);
    let mut cell_italics = Vec::with_capacity(cap);
    let mut cell_underlines = Vec::with_capacity(cap);
    let mut cell_borders = Vec::with_capacity(cap);
    let mut cell_fills = Vec::with_capacity(cap);
    let mut cell_font_sizes = Vec::with_capacity(cap);
    for local_row in 0..viewport.visible_rows {
        for local_col in 0..viewport.visible_cols {
            let Some(row) = viewport.row_at(local_row) else {
                continue;
            };
            let Some(col) = viewport.column_at(local_col) else {
                continue;
            };
            let cell = CellRef { row, col };
            let style = sheet.cell_style(cell);
            let align_code = match sheet.cell_alignment(cell) {
                CellAlignment::General | CellAlignment::Left => 0,
                CellAlignment::Center => 1,
                CellAlignment::Right => 2,
            };
            let raw_val = cell_value(sheet, values, row, col);
            let display_val = style.format_value(&raw_val);
            cells.push(display_val);
            cell_alignments.push(align_code);
            cell_bolds.push(style.bold);
            cell_italics.push(style.italic);
            cell_underlines.push(style.underline);
            cell_borders.push(style.border);
            cell_fills.push(style.fill.swatch_index());
            cell_font_sizes.push(formatting::effective_font_size(style) as i32);
        }
    }

    ProjectedSheetGrid {
        rows,
        cols,
        column_headers,
        row_headers,
        cells,
        cell_alignments,
        cell_bolds,
        cell_italics,
        cell_underlines,
        cell_borders,
        cell_fills,
        cell_font_sizes,
    }
}

#[cfg(test)]
fn project_sheet_grid(sheet: &Sheet, viewport: SheetViewport) -> ProjectedSheetGrid {
    let values = evaluate(sheet);
    project_sheet_grid_with_values(sheet, &values, viewport)
}

/// Addressable grid size: used cells and selection always fit, and the grid
/// fills the viewport plus a scroll-ahead margin so tall/wide windows show a
/// live worksheet (not a fixed 15x8 card on dead canvas) and the void beyond
/// content stays navigable. `fill` is `None` in unit tests without a window.
fn editor_dimensions(
    sheet: &Sheet,
    selected: CellRef,
    fill: Option<(u32, u32)>,
) -> SheetDimensions {
    const SCROLL_AHEAD_COLS: u32 = 12;
    const SCROLL_AHEAD_ROWS: u32 = 30;
    let dimensions = sheet.dimensions();
    // Unit tests without a window keep the legacy 15x8 minimum exactly.
    let (fill_cols, fill_rows) = fill.unwrap_or((DEFAULT_VISIBLE_COLS, DEFAULT_VISIBLE_ROWS));
    let (ahead_cols, ahead_rows) = if fill.is_some() {
        (SCROLL_AHEAD_COLS, SCROLL_AHEAD_ROWS)
    } else {
        (0, 0)
    };
    SheetDimensions::new(
        dimensions
            .rows
            .max(selected.row.saturating_add(1))
            .max(DEFAULT_VISIBLE_ROWS)
            .max(fill_rows + ahead_rows),
        dimensions
            .cols
            .max(selected.col.saturating_add(1))
            .max(DEFAULT_VISIBLE_COLS)
            .max(fill_cols + ahead_cols),
    )
}

/// Return the default width used by the projected grid. Small workbooks (at
/// most 8 used columns, no custom widths) stretch to the available table
/// width up to a comfortable cap; everything else keeps the persisted 80px
/// default so horizontal scrolling stays useful. The fit keys off *used*
/// columns (not the fill-expanded addressable grid) so navigating or
/// auto-filling the void never fattens cells.
fn grid_default_col_width(sheet: &Sheet, viewport_width: f32) -> f32 {
    if !sheet.col_widths.is_empty() || sheet.dimensions().cols > DEFAULT_VISIBLE_COLS {
        return GRID_COL_WIDTH;
    }
    let available = (viewport_width - GRID_ROW_HEADER_WIDTH).max(0.0);
    let fitted = available / DEFAULT_VISIBLE_COLS as f32;
    fitted.clamp(GRID_COL_WIDTH, FIT_COLUMN_MAX_WIDTH)
}

fn valid_dimension(value: f32, fallback: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        fallback
    }
}

fn dimension_size(
    index: u32,
    default_size: f32,
    custom: &std::collections::BTreeMap<u32, f32>,
) -> f32 {
    valid_dimension(
        custom.get(&index).copied().unwrap_or(default_size),
        default_size,
    )
}

fn dimension_extent(
    count: u32,
    default_size: f32,
    custom: &std::collections::BTreeMap<u32, f32>,
) -> f32 {
    let mut extent = count as f32 * default_size;
    for (&index, &size) in custom {
        if index < count && size.is_finite() && size > 0.0 {
            extent += size - default_size;
        }
    }
    extent.max(default_size)
}

fn dimension_offset(
    first: u32,
    default_size: f32,
    custom: &std::collections::BTreeMap<u32, f32>,
) -> f32 {
    let mut offset = first as f32 * default_size;
    for &size in custom.range(..first).map(|(_, size)| size) {
        if size.is_finite() && size > 0.0 {
            offset += size - default_size;
        }
    }
    offset.max(0.0)
}

/// Maps a cumulative pixel offset to the worksheet index containing it.
/// Custom dimensions are sparse, so default-sized runs are skipped in one
/// step while the handful of persisted overrides are visited explicitly.
fn dimension_index_at_offset(
    offset: f32,
    count: u32,
    default_size: f32,
    custom: &std::collections::BTreeMap<u32, f32>,
) -> u32 {
    let count = count.max(1);
    let default_size = valid_dimension(default_size, 1.0);
    let offset = if offset.is_finite() {
        offset.max(0.0)
    } else {
        0.0
    };
    let mut index = 0_u32;
    let mut consumed = 0.0_f32;

    for (&custom_index, &custom_size) in custom {
        if custom_index >= count
            || custom_index < index
            || !custom_size.is_finite()
            || custom_size <= 0.0
        {
            continue;
        }
        if custom_index > index {
            let span = (custom_index - index) as f32 * default_size;
            if offset < consumed + span {
                return (index + ((offset - consumed) / default_size).floor().max(0.0) as u32)
                    .min(count - 1);
            }
            consumed += span;
            index = custom_index;
        }
        if offset < consumed + custom_size {
            return index;
        }
        consumed += custom_size;
        index = custom_index.saturating_add(1);
    }

    if index < count {
        return (index + ((offset - consumed) / default_size).floor().max(0.0) as u32)
            .min(count - 1);
    }
    count - 1
}

fn dimension_visible_count(
    first: u32,
    count: u32,
    viewport_size: f32,
    default_size: f32,
    custom: &std::collections::BTreeMap<u32, f32>,
) -> u32 {
    let count = count.max(1);
    let first = first.min(count - 1);
    let viewport_size = if viewport_size.is_finite() && viewport_size > 0.0 {
        viewport_size
    } else {
        default_size
    };
    let mut consumed = 0.0_f32;
    let mut index = first;
    while index < count && (consumed < viewport_size || index == first) {
        consumed += dimension_size(index, default_size, custom);
        index += 1;
    }
    index.saturating_sub(first).max(1)
}

fn viewport_from_dimensions(
    (scroll_x, scroll_y): (f32, f32),
    (viewport_width, viewport_height): (f32, f32),
    dimensions: SheetDimensions,
    default_col_width: f32,
    row_heights: &std::collections::BTreeMap<u32, f32>,
    col_widths: &std::collections::BTreeMap<u32, f32>,
) -> SheetViewport {
    let dimensions = SheetDimensions::new(dimensions.rows, dimensions.cols);
    let default_col_width = valid_dimension(default_col_width, GRID_COL_WIDTH);
    let viewport_width = if viewport_width.is_finite() && viewport_width > 0.0 {
        viewport_width
    } else {
        default_col_width
    };
    let viewport_height = if viewport_height.is_finite() && viewport_height > 0.0 {
        viewport_height
    } else {
        GRID_ROW_HEIGHT
    };
    let content_width = dimension_extent(dimensions.cols, default_col_width, col_widths);
    let content_height = dimension_extent(dimensions.rows, GRID_ROW_HEIGHT, row_heights);
    let max_scroll_x = (content_width - viewport_width).max(0.0);
    let max_scroll_y = (content_height - viewport_height).max(0.0);
    let scroll_x = if scroll_x.is_finite() {
        scroll_x.clamp(0.0, max_scroll_x)
    } else {
        0.0
    };
    let scroll_y = if scroll_y.is_finite() {
        scroll_y.clamp(0.0, max_scroll_y)
    } else {
        0.0
    };
    let first_col =
        dimension_index_at_offset(scroll_x, dimensions.cols, default_col_width, col_widths);
    let first_row =
        dimension_index_at_offset(scroll_y, dimensions.rows, GRID_ROW_HEIGHT, row_heights);
    let visible_cols = dimension_visible_count(
        first_col,
        dimensions.cols,
        viewport_width,
        default_col_width,
        col_widths,
    )
    .min(dimensions.cols - first_col);
    let visible_rows = dimension_visible_count(
        first_row,
        dimensions.rows,
        viewport_height,
        GRID_ROW_HEIGHT,
        row_heights,
    )
    .min(dimensions.rows - first_row);
    SheetViewport {
        first_row,
        first_col,
        visible_rows: visible_rows.max(1),
        visible_cols: visible_cols.max(1),
    }
}

/// Concrete dimensions/offsets consumed by the Slint projection. Persisted
/// row and column sizes are retained for materialized cells, viewport indexing,
/// and the full scroll extents.
struct GridGeometry {
    default_col_width: f32,
    column_widths: Vec<f32>,
    row_heights: Vec<f32>,
    col_offset: f32,
    row_offset: f32,
    visible_width: f32,
    visible_height: f32,
    content_width: f32,
    content_height: f32,
}

/// Materialized positions for the bounded worksheet object layer. Objects
/// retain worksheet coordinates in the core model while the UI receives only
/// the small list of objects and their current viewport-relative geometry.
struct ProjectedSheetObjects {
    kinds: Vec<SharedString>,
    labels: Vec<SharedString>,
    images: Vec<Image>,
    positions_x: Vec<f32>,
    positions_y: Vec<f32>,
    widths: Vec<i32>,
    heights: Vec<i32>,
    fills: Vec<i32>,
    visible: Vec<bool>,
    selected: Vec<bool>,
}

fn project_sheet_objects(
    sheet: &Sheet,
    viewport: SheetViewport,
    geometry: &GridGeometry,
    zoom: f32,
    scroll_x: f32,
    scroll_y: f32,
    selected_object: i32,
) -> ProjectedSheetObjects {
    let scaled_cols: std::collections::BTreeMap<u32, f32> = sheet
        .col_widths
        .iter()
        .map(|(&col, &width)| (col, width * zoom))
        .collect();
    let scaled_rows: std::collections::BTreeMap<u32, f32> = sheet
        .row_heights
        .iter()
        .map(|(&row, &height)| (row, height * zoom))
        .collect();
    let first_row = viewport.first_row;
    let first_col = viewport.first_col;
    let last_row = first_row.saturating_add(viewport.visible_rows);
    let last_col = first_col.saturating_add(viewport.visible_cols);
    let mut projected = ProjectedSheetObjects {
        kinds: Vec::with_capacity(sheet.objects.len()),
        labels: Vec::with_capacity(sheet.objects.len()),
        images: Vec::with_capacity(sheet.objects.len()),
        positions_x: Vec::with_capacity(sheet.objects.len()),
        positions_y: Vec::with_capacity(sheet.objects.len()),
        widths: Vec::with_capacity(sheet.objects.len()),
        heights: Vec::with_capacity(sheet.objects.len()),
        fills: Vec::with_capacity(sheet.objects.len()),
        visible: Vec::with_capacity(sheet.objects.len()),
        selected: Vec::with_capacity(sheet.objects.len()),
    };
    for object in &sheet.objects {
        let absolute_x =
            dimension_offset(object.anchor.col, geometry.default_col_width, &scaled_cols);
        let absolute_y = dimension_offset(object.anchor.row, GRID_ROW_HEIGHT * zoom, &scaled_rows);
        projected.kinds.push(object.kind.as_str().into());
        projected.labels.push(object.label.as_str().into());
        let image = load_sheet_object_image(object);
        projected.images.push(image);
        projected.positions_x.push(24.0 + absolute_x + scroll_x);
        projected.positions_y.push(52.0 + absolute_y + scroll_y);
        projected
            .widths
            .push((object.width as f32 * zoom).round().max(1.0) as i32);
        projected
            .heights
            .push((object.height as f32 * zoom).round().max(1.0) as i32);
        projected.fills.push(object.fill.swatch_index());
        projected.visible.push(
            object.anchor.row >= first_row
                && object.anchor.row < last_row
                && object.anchor.col >= first_col
                && object.anchor.col < last_col,
        );
        projected
            .selected
            .push(selected_object == projected.kinds.len() as i32 - 1);
    }
    projected
}

fn load_sheet_object_image(object: &loom_sheets_core::SheetObject) -> Image {
    if object.kind != loom_sheets_core::SheetObjectKind::Image {
        return Image::default();
    }
    if let Some(bytes) = object.embedded.as_deref() {
        let is_svg = object
            .path
            .rsplit_once('.')
            .is_some_and(|(_, extension)| extension.eq_ignore_ascii_case("svg"));
        if is_svg {
            if let Ok(image) = Image::load_from_svg_data(bytes) {
                return image;
            }
        } else if let Ok(decoded) = image::load_from_memory(bytes) {
            let rgba = decoded.to_rgba8();
            return Image::from_rgba8(SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
                rgba.as_raw(),
                rgba.width(),
                rgba.height(),
            ));
        }
    }
    Image::load_from_path(Path::new(&object.path)).unwrap_or_default()
}

fn grid_geometry(
    sheet: &Sheet,
    dimensions: SheetDimensions,
    viewport: SheetViewport,
    viewport_width: f32,
    zoom: f32,
) -> GridGeometry {
    // Zoom scales rendered pixels uniformly; the persisted model keeps
    // unscaled pixels (see `viewport_from_app`).
    let fit_col_width = grid_default_col_width(sheet, viewport_width);
    let default_col_width = fit_col_width * zoom;
    let default_row_height = GRID_ROW_HEIGHT * zoom;
    let scaled_cols: std::collections::BTreeMap<u32, f32> = sheet
        .col_widths
        .iter()
        .map(|(&col, &width)| (col, width * zoom))
        .collect();
    let scaled_rows: std::collections::BTreeMap<u32, f32> = sheet
        .row_heights
        .iter()
        .map(|(&row, &height)| (row, height * zoom))
        .collect();
    let column_widths: Vec<f32> = (0..viewport.visible_cols)
        .filter_map(|index| viewport.column_at(index))
        .map(|col| dimension_size(col, default_col_width, &scaled_cols))
        .collect();
    let row_heights: Vec<f32> = (0..viewport.visible_rows)
        .filter_map(|index| viewport.row_at(index))
        .map(|row| dimension_size(row, default_row_height, &scaled_rows))
        .collect();
    let visible_width = column_widths.iter().sum();
    let visible_height = row_heights.iter().sum();
    let content_width = if sheet.col_widths.is_empty() {
        dimensions.cols as f32 * default_col_width
    } else {
        dimension_extent(dimensions.cols, default_col_width, &scaled_cols)
    };
    let content_height = dimension_extent(dimensions.rows, default_row_height, &scaled_rows);
    GridGeometry {
        default_col_width,
        column_widths,
        row_heights,
        col_offset: dimension_offset(viewport.first_col, default_col_width, &scaled_cols),
        row_offset: dimension_offset(viewport.first_row, default_row_height, &scaled_rows),
        visible_width,
        visible_height,
        content_width,
        content_height,
    }
}

/// Visible grid capacity in cells for filling the addressable dimensions.
/// `None` while the window has no measured viewport yet (unit tests), which
/// keeps the legacy minimum and avoids projecting a phantom window.
fn window_fill(app: &SheetsApp, zoom: f32) -> Option<(u32, u32)> {
    let width = app.get_grid_viewport_width();
    let height = app.get_grid_viewport_height();
    if width > 1.0 && height > 1.0 {
        Some((
            ((width - GRID_ROW_HEADER_WIDTH).max(0.0) / (GRID_COL_WIDTH * zoom)) as u32,
            ((height - GRID_COLUMN_HEADER_HEIGHT).max(0.0) / (GRID_ROW_HEIGHT * zoom)) as u32,
        ))
    } else {
        None
    }
}

fn viewport_from_app(app: &SheetsApp, sheet: &Sheet) -> SheetViewport {
    let selected = selection_from_app(app).focus;
    let zoom = zoom_factor(app);
    let viewport_width = if app.get_grid_viewport_width() > 1.0 {
        app.get_grid_viewport_width()
    } else {
        GRID_COL_WIDTH * DEFAULT_VISIBLE_COLS as f32 + GRID_ROW_HEADER_WIDTH
    };
    let viewport_height = if app.get_grid_viewport_height() > 1.0 {
        app.get_grid_viewport_height()
    } else {
        GRID_ROW_HEIGHT * DEFAULT_VISIBLE_ROWS as f32 + GRID_COLUMN_HEADER_HEIGHT
    };
    let dimensions = editor_dimensions(sheet, selected, window_fill(app, zoom));
    let default_col_width = grid_default_col_width(sheet, viewport_width) * zoom;
    let default_row_height = GRID_ROW_HEIGHT * zoom;
    // Zoom scales rendered geometry uniformly; the persisted model keeps
    // unscaled pixels so Save/Set-140px round-trips are zoom-independent.
    let scaled_cols: std::collections::BTreeMap<u32, f32> = sheet
        .col_widths
        .iter()
        .map(|(&col, &width)| (col, width * zoom))
        .collect();
    let scaled_rows: std::collections::BTreeMap<u32, f32> = sheet
        .row_heights
        .iter()
        .map(|(&row, &height)| (row, height * zoom))
        .collect();
    let scroll_x = (-app.get_grid_scroll_x()).max(0.0);
    let scroll_y = (-app.get_grid_scroll_y()).max(0.0);
    viewport_from_dimensions(
        (scroll_x, scroll_y),
        (
            (viewport_width - GRID_ROW_HEADER_WIDTH).max(default_col_width),
            (viewport_height - GRID_COLUMN_HEADER_HEIGHT).max(default_row_height),
        ),
        dimensions,
        default_col_width,
        &scaled_rows,
        &scaled_cols,
    )
}

/// Rendered zoom scale from the window (1.0 = 100%). Clamped so corrupt or
/// extreme values cannot collapse or explode grid geometry.
pub(crate) fn zoom_factor(app: &SheetsApp) -> f32 {
    let factor = app.get_zoom_factor();
    if factor.is_finite() {
        factor.clamp(0.5, 3.0)
    } else {
        1.0
    }
}

pub(crate) fn apply_sheet(app: &SheetsApp, state: &GuiState) {
    let sheet = state.current.borrow().clone();
    let vals = evaluate_current(state);
    project_sheet_inner(app, &sheet, &vals, true);
    record_workbook_snapshot(state);
}

/// Re-project the live tab with workbook-resolved values, without recording
/// a recovery snapshot (selection/scroll/view-only refreshes).
pub(crate) fn project_current(app: &SheetsApp, state: &GuiState) {
    let sheet = state.current.borrow().clone();
    let vals = evaluate_current(state);
    project_sheet_inner(app, &sheet, &vals, true);
}

/// Re-project without revealing the selection and without snapshotting.
pub(crate) fn project_current_without_reveal(app: &SheetsApp, state: &GuiState) {
    let sheet = state.current.borrow().clone();
    let vals = evaluate_current(state);
    project_sheet_inner(app, &sheet, &vals, false);
}

pub(crate) fn project_sheet(app: &SheetsApp, sheet: &Sheet) {
    let vals = evaluate(sheet);
    project_sheet_inner(app, sheet, &vals, true);
}

pub(crate) fn project_sheet_without_reveal(app: &SheetsApp, sheet: &Sheet) {
    let vals = evaluate(sheet);
    project_sheet_inner(app, sheet, &vals, false);
}

/// Snapshot the full workbook (all tabs, active sheet first-class) so crash
/// recovery restores tabs, not just the visible sheet.
pub(crate) fn record_workbook_snapshot(state: &GuiState) {
    let (siblings, active) = workbook_sheets(state);
    let _ = record_snapshot_recovery(
        "sheets state",
        workbook_to_json(&siblings, active).into_bytes(),
    );
}

/// All tabs with the live current sheet synced into its slot, plus the
/// clamped active index. Backs workbook evaluation, saving, and snapshots.
pub(crate) fn workbook_sheets(state: &GuiState) -> (Vec<Sheet>, usize) {
    let mut siblings = state.sheets.borrow().clone();
    let active = (*state.active_sheet_index.borrow()).min(siblings.len().saturating_sub(1));
    if siblings.is_empty() {
        siblings.push(state.current.borrow().clone());
    } else {
        siblings[active] = state.current.borrow().clone();
    }
    (siblings, active)
}

/// Evaluate the active tab with cross-sheet references resolved against all
/// tabs. Single-sheet callers keep using `evaluate`.
pub(crate) fn evaluate_current(state: &GuiState) -> std::collections::HashMap<CellRef, Value> {
    let (siblings, active) = workbook_sheets(state);
    evaluate_workbook(&siblings)
        .into_iter()
        .nth(active)
        .unwrap_or_default()
}

fn project_sheet_inner(
    app: &SheetsApp,
    sheet: &Sheet,
    vals: &std::collections::HashMap<CellRef, Value>,
    reveal_selection: bool,
) {
    let selection = selection_from_app(app);
    let selected = selection.focus;
    let dimensions = sheet.dimensions();
    let zoom = zoom_factor(app);
    let editor_dimensions = editor_dimensions(sheet, selected, window_fill(app, zoom));
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
    app.set_zoom_factor(zoom);
    app.set_zoom_level(SharedString::from(format!(
        "{}%",
        (zoom * 100.0).round() as i32
    )));
    let geometry = grid_geometry(
        sheet,
        editor_dimensions,
        viewport,
        app.get_grid_viewport_width(),
        zoom,
    );
    // Flickable coordinates are negative because its content is translated
    // opposite to the positive worksheet scroll offset. Preserve fractional
    // wheel/touchpad offsets while snapping only when selection auto-reveal
    // moved the projected window. Direct headless/property updates can be
    // outside Flickable's legal range, so clamp the retained offset to the
    // same extents as the materialized content.
    if viewport.first_col != projected_before_reveal.first_col {
        app.set_grid_scroll_x(-geometry.col_offset);
    } else {
        let viewport_width = if app.get_grid_viewport_width() > 1.0 {
            app.get_grid_viewport_width()
        } else {
            GRID_COL_WIDTH * DEFAULT_VISIBLE_COLS as f32 + GRID_ROW_HEADER_WIDTH
        };
        let max_scroll_x =
            (geometry.content_width + GRID_ROW_HEADER_WIDTH - viewport_width).max(0.0);
        app.set_grid_scroll_x(-current_scroll_x.min(max_scroll_x));
    }
    if viewport.first_row != projected_before_reveal.first_row {
        app.set_grid_scroll_y(-geometry.row_offset);
    } else {
        let viewport_height = if app.get_grid_viewport_height() > 1.0 {
            app.get_grid_viewport_height()
        } else {
            GRID_ROW_HEIGHT * DEFAULT_VISIBLE_ROWS as f32 + GRID_COLUMN_HEADER_HEIGHT
        };
        let max_scroll_y =
            (geometry.content_height + GRID_COLUMN_HEADER_HEIGHT - viewport_height).max(0.0);
        app.set_grid_scroll_y(-current_scroll_y.min(max_scroll_y));
    }
    let grid = project_sheet_grid_with_values(sheet, vals, viewport);
    let objects = project_sheet_objects(
        sheet,
        viewport,
        &geometry,
        zoom,
        app.get_grid_scroll_x(),
        app.get_grid_scroll_y(),
        app.get_selected_object(),
    );

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
    app.set_cell_alignments(ModelRc::new(VecModel::from(grid.cell_alignments)));
    app.set_cell_bolds(ModelRc::new(VecModel::from(grid.cell_bolds)));
    app.set_cell_italics(ModelRc::new(VecModel::from(grid.cell_italics)));
    app.set_cell_underlines(ModelRc::new(VecModel::from(grid.cell_underlines)));
    app.set_cell_borders(ModelRc::new(VecModel::from(grid.cell_borders)));
    app.set_cell_fills(ModelRc::new(VecModel::from(grid.cell_fills)));
    app.set_cell_font_sizes(ModelRc::new(VecModel::from(grid.cell_font_sizes)));
    app.set_object_kinds(ModelRc::new(VecModel::from(objects.kinds)));
    app.set_object_labels(ModelRc::new(VecModel::from(objects.labels)));
    app.set_object_images(ModelRc::new(VecModel::from(objects.images)));
    app.set_object_positions_x(ModelRc::new(VecModel::from(objects.positions_x)));
    app.set_object_positions_y(ModelRc::new(VecModel::from(objects.positions_y)));
    app.set_object_widths(ModelRc::new(VecModel::from(objects.widths)));
    app.set_object_heights(ModelRc::new(VecModel::from(objects.heights)));
    app.set_object_fills(ModelRc::new(VecModel::from(objects.fills)));
    app.set_visible_objects(ModelRc::new(VecModel::from(objects.visible)));
    app.set_selected_objects(ModelRc::new(VecModel::from(objects.selected)));
    app.set_grid_col_width(geometry.default_col_width);
    app.set_grid_row_height(GRID_ROW_HEIGHT * zoom);
    app.set_grid_col_offset(geometry.col_offset);
    app.set_grid_row_offset(geometry.row_offset);
    app.set_grid_visible_width(geometry.visible_width);
    app.set_grid_visible_height(geometry.visible_height);
    app.set_grid_content_width(geometry.content_width);
    app.set_grid_content_height(geometry.content_height);
    app.set_grid_column_widths(ModelRc::new(VecModel::from(geometry.column_widths)));
    app.set_grid_row_heights(ModelRc::new(VecModel::from(geometry.row_heights)));
    if app.get_selected_object() >= sheet.objects.len() as i32 {
        app.set_selected_object(-1);
    }
    update_selection_range(app, sheet, vals, selection);
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
    if app.get_selection_count() <= 1 {
        app.set_status_right("Offline".into());
    }
    actions::sync_chart_to_app(app, sheet);
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

pub(crate) fn sync_menu_state(menu_service: &NativeMenuBar, app: &SheetsApp, state: &GuiState) {
    if let Err(error) = sync_menu_state_result(menu_service, app, state) {
        app.set_status_right(SharedString::from(format!("Menu update failed: {error}")));
    }
}

pub(crate) fn selection_from_app(app: &SheetsApp) -> GridSelection {
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

pub(crate) fn update_selection(
    app: &SheetsApp,
    sheet: &Sheet,
    vals: &std::collections::HashMap<CellRef, Value>,
    selected: CellRef,
) {
    update_selection_range(app, sheet, vals, GridSelection::new(selected, selected));
}

pub(crate) fn update_selection_range(
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
    let align_code = match sheet.cell_alignment(selected) {
        CellAlignment::General | CellAlignment::Left => 0,
        CellAlignment::Center => 1,
        CellAlignment::Right => 2,
    };
    app.set_cell_alignment(align_code);
    let style = sheet.cell_style(selected);
    app.set_cell_bold(style.bold);
    app.set_cell_italic(style.italic);
    app.set_cell_underline(style.underline);
    let fmt_code = match style.number_format {
        NumberFormat::General
        | NumberFormat::PlainText
        | NumberFormat::Scientific
        | NumberFormat::DateIso => 0,
        NumberFormat::Currency => 1,
        NumberFormat::Percentage => 2,
        NumberFormat::Number => 3,
    };
    app.set_cell_format(fmt_code);
    let decimals = style.decimal_places.unwrap_or(2);
    app.set_cell_decimals(decimals as i32);
    app.set_cell_decimals_label(SharedString::from(decimals.to_string()));
    app.set_cell_border(style.border);
    app.set_cell_fill(style.fill.swatch_index());
    let font_size = formatting::effective_font_size(style);
    app.set_cell_font_size(font_size as i32);
    app.set_cell_font_label(SharedString::from(font_size.to_string()));

    let raw_display = cell_value(sheet, vals, selected.row, selected.col);
    let display = style.format_value(&raw_display);
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
    app.set_selection_value(SharedString::from(display));
    let cell_count = range.cells().len();
    if cell_count > 1 {
        let mut sum = 0.0;
        let mut count = 0;
        for c in range.cells() {
            let val = cell_value(sheet, vals, c.row, c.col);
            let clean = val.trim().trim_start_matches('$').trim_end_matches('%');
            if let Ok(n) = clean.parse::<f64>() {
                sum += n;
                count += 1;
            }
        }
        if count > 0 {
            let avg = sum / count as f64;
            app.set_status_right(SharedString::from(format!(
                "SUM: {sum:.2}  AVG: {avg:.2}  COUNT: {count}"
            )));
        }
    }
}

/// A typed undo/redo transaction in Loom Sheets supporting single cell edits,
/// range fills, range clearing, and cell text alignment changes.
#[derive(Debug, Clone)]
pub(crate) enum SheetTransaction {
    Range(RangeEdit),
    Batch(Vec<RangeEdit>),
    Alignment {
        range: CellRange,
        before: Vec<(CellRef, CellAlignment)>,
        after: CellAlignment,
    },
    Style {
        before: Vec<(CellRef, CellStyle)>,
        after: Vec<(CellRef, CellStyle)>,
    },
    Snapshot {
        before: Box<Sheet>,
        after: Box<Sheet>,
    },
    /// Workbook-level tab operation (add/delete/rename sheet). Boxed to keep
    /// the enum size bounded; applied only via `restore_workbook_state`, never
    /// through the single-sheet `apply`/`revert` below.
    Workbook {
        before: Box<WorkbookUndoState>,
        after: Box<WorkbookUndoState>,
    },
}

/// Full tab-strip state captured for workbook-level undo/redo.
#[derive(Debug, Clone)]
pub(crate) struct WorkbookUndoState {
    sheets: Vec<Sheet>,
    active: usize,
    histories: Vec<(Vec<SheetTransaction>, Vec<SheetTransaction>)>,
}

impl WorkbookUndoState {
    fn capture(state: &GuiState) -> Self {
        Self {
            sheets: state.sheets.borrow().clone(),
            active: *state.active_sheet_index.borrow(),
            histories: state.sheet_histories.borrow().clone(),
        }
    }

    fn restore(&self, state: &GuiState) {
        *state.sheets.borrow_mut() = self.sheets.clone();
        *state.active_sheet_index.borrow_mut() = self.active;
        let active = self.active.min(self.sheets.len().saturating_sub(1));
        *state.current.borrow_mut() = self.sheets[active].clone();
        *state.sheet_histories.borrow_mut() = self.histories.clone();
        let (undo, redo) = self.histories.get(active).cloned().unwrap_or_default();
        *state.undo_stack.borrow_mut() = undo;
        *state.redo_stack.borrow_mut() = redo;
    }
}

impl SheetTransaction {
    pub(crate) fn apply(&self, sheet: &mut Sheet) {
        match self {
            SheetTransaction::Range(edit) => edit.apply(sheet),
            SheetTransaction::Batch(edits) => {
                for edit in edits {
                    edit.apply(sheet);
                }
            }
            SheetTransaction::Alignment { range, after, .. } => {
                sheet.set_range_alignment(range.start, range.end, *after);
            }
            SheetTransaction::Style { after, .. } => {
                for (cell, style) in after {
                    sheet.set_cell_style(*cell, *style);
                }
            }
            SheetTransaction::Snapshot { after, .. } => {
                *sheet = (**after).clone();
            }
            // Workbook transactions span the whole tab strip and are applied
            // only via `restore_workbook_state` at the undo/redo sites below.
            SheetTransaction::Workbook { .. } => {}
        }
    }

    pub(crate) fn revert(&self, sheet: &mut Sheet) {
        match self {
            SheetTransaction::Range(edit) => edit.revert(sheet),
            SheetTransaction::Batch(edits) => {
                for edit in edits.iter().rev() {
                    edit.revert(sheet);
                }
            }
            SheetTransaction::Alignment { before, .. } => {
                for (cell, align) in before {
                    sheet.set_cell_alignment(*cell, *align);
                }
            }
            SheetTransaction::Style { before, .. } => {
                for (cell, style) in before {
                    sheet.set_cell_style(*cell, *style);
                }
            }
            SheetTransaction::Snapshot { before, .. } => {
                *sheet = (**before).clone();
            }
            // See `apply`: workbook transactions restore via `restore_workbook_state`.
            SheetTransaction::Workbook { .. } => {}
        }
    }
}

pub(crate) fn commit_transaction(
    sheet: &mut Sheet,
    undo_stack: &mut Vec<SheetTransaction>,
    redo_stack: &mut Vec<SheetTransaction>,
    tx: SheetTransaction,
) -> bool {
    tx.apply(sheet);
    push_history(undo_stack, tx);
    redo_stack.clear();
    true
}

/// Maximum undo/redo entries per stack. Beyond this the oldest transaction
/// drops, so long editing sessions cannot grow memory without bound.
pub(crate) const MAX_HISTORY_ENTRIES: usize = 200;

/// Push a transaction, evicting the oldest entry past the bound.
pub(crate) fn push_history(stack: &mut Vec<SheetTransaction>, tx: SheetTransaction) {
    stack.push(tx);
    if stack.len() > MAX_HISTORY_ENTRIES {
        stack.remove(0);
    }
}

/// Apply one committed formula-bar edit and record one undo transaction.
pub(crate) fn commit_formula_edit(
    sheet: &mut Sheet,
    undo_stack: &mut Vec<SheetTransaction>,
    redo_stack: &mut Vec<SheetTransaction>,
    selected: CellRef,
    draft: &str,
) -> bool {
    let edit = RangeEdit::replace(sheet, selected, Some(draft.to_owned()));
    if edit.is_noop() {
        return false;
    }
    commit_transaction(sheet, undo_stack, redo_stack, SheetTransaction::Range(edit))
}

/// Apply a sparse range edit as one undoable transaction.
pub(crate) fn commit_range_edit(
    sheet: &mut Sheet,
    undo_stack: &mut Vec<SheetTransaction>,
    redo_stack: &mut Vec<SheetTransaction>,
    edit: RangeEdit,
) -> bool {
    if edit.is_empty() || edit.is_noop() {
        return false;
    }
    commit_transaction(sheet, undo_stack, redo_stack, SheetTransaction::Range(edit))
}

/// Record a workbook-level tab operation (add/delete/rename sheet) as one
/// undoable transaction. The live current sheet is synced into its tab slot
/// first so `before` captures unsaved cell edits. Per-tab undo/redo stacks
/// follow the same stash-and-restore discipline as sheet switching, and
/// `drop_history` removes the deleted tab's stashed stacks for deletions.
pub(crate) fn commit_workbook_transaction(
    state: &GuiState,
    after_sheets: Vec<Sheet>,
    after_active: usize,
    drop_history: Option<usize>,
) {
    sync_current_to_tabs(state);
    if after_sheets.is_empty() {
        return;
    }
    let old_active = *state.active_sheet_index.borrow();

    // Stash the outgoing tab's live stacks, mirroring sheet switching.
    let live_undo = std::mem::take(&mut *state.undo_stack.borrow_mut());
    let live_redo = std::mem::take(&mut *state.redo_stack.borrow_mut());
    {
        let mut histories = state.sheet_histories.borrow_mut();
        if histories.len() <= old_active {
            histories.resize_with(old_active + 1, || (Vec::new(), Vec::new()));
        }
        histories[old_active] = (live_undo, live_redo);
    }
    // Workbook snapshots keep per-tab histories as data, rather than taking
    // a recursive copy of the live workbook undo/redo stacks. This keeps tab
    // operations bounded even after long editing sessions.
    let before = WorkbookUndoState::capture(state);

    let after_active = after_active.min(after_sheets.len().saturating_sub(1));
    *state.sheets.borrow_mut() = after_sheets;
    *state.active_sheet_index.borrow_mut() = after_active;
    *state.current.borrow_mut() = state.sheets.borrow()[after_active].clone();
    {
        let mut histories = state.sheet_histories.borrow_mut();
        let tabs = state.sheets.borrow().len();
        if let Some(dropped) = drop_history {
            if dropped < histories.len() {
                histories.remove(dropped);
            }
        }
        if histories.len() < tabs {
            histories.resize_with(tabs, || (Vec::new(), Vec::new()));
        }
        if !histories.is_empty() {
            let landed = after_active.min(histories.len() - 1);
            let (landed_undo, landed_redo) = histories[landed].clone();
            *state.undo_stack.borrow_mut() = landed_undo;
            *state.redo_stack.borrow_mut() = landed_redo;
        }
    }

    let after = WorkbookUndoState::capture(state);
    push_history(
        &mut state.undo_stack.borrow_mut(),
        SheetTransaction::Workbook {
            before: Box::new(before),
            after: Box::new(after),
        },
    );
    state.redo_stack.borrow_mut().clear();
}

/// Restore a workbook snapshot at the undo/redo sites and refresh the full
/// window (tabs, grid, history controls) around it.
pub(crate) fn restore_workbook_state(
    app: &SheetsApp,
    state: &GuiState,
    snapshot: &WorkbookUndoState,
    menu_service: &Arc<loom_desktop::NativeMenuBar>,
) {
    snapshot.restore(state);
    apply_sheet(app, state);
    sync_sheet_tabs(app, state);
    sync_menu_state(menu_service, app, state);
    sync_history_controls(app, state);
}

/// Clear selected cells or range with full undo/redo history.
pub(crate) fn clear_selection(
    sheet: &mut Sheet,
    undo_stack: &mut Vec<SheetTransaction>,
    redo_stack: &mut Vec<SheetTransaction>,
    range: CellRange,
) -> bool {
    let mut edits = Vec::new();
    for cell in range.cells() {
        if sheet.raw(cell).is_some() {
            edits.push(RangeEdit::replace(sheet, cell, None));
        }
    }
    if edits.is_empty() {
        return false;
    }
    let tx = if edits.len() == 1 {
        SheetTransaction::Range(edits.into_iter().next().unwrap())
    } else {
        SheetTransaction::Batch(edits)
    };
    commit_transaction(sheet, undo_stack, redo_stack, tx)
}

/// Apply text alignment across selected cells with full undo/redo history.
pub(crate) fn set_selection_alignment(
    sheet: &mut Sheet,
    undo_stack: &mut Vec<SheetTransaction>,
    redo_stack: &mut Vec<SheetTransaction>,
    range: CellRange,
    alignment: CellAlignment,
) -> bool {
    let mut before = Vec::new();
    let mut any_change = false;
    for cell in range.cells() {
        let prev = sheet.cell_alignment(cell);
        if prev != alignment {
            any_change = true;
        }
        before.push((cell, prev));
    }
    if !any_change {
        return false;
    }
    let tx = SheetTransaction::Alignment {
        range,
        before,
        after: alignment,
    };
    commit_transaction(sheet, undo_stack, redo_stack, tx)
}

/// The Sheets fill handle repeats a selected block into the next block below
/// it.  A single-cell selection has no fill source and is therefore disabled
/// in the UI rather than pretending to perform an operation.
pub(crate) fn fill_selection_down(
    sheet: &mut Sheet,
    undo_stack: &mut Vec<SheetTransaction>,
    redo_stack: &mut Vec<SheetTransaction>,
    selection: GridSelection,
) -> bool {
    let source = selection.range();
    let Some(target) = fill_target_range(source) else {
        return false;
    };
    let edit = RangeEdit::fill(sheet, source, target);
    commit_range_edit(sheet, undo_stack, redo_stack, edit)
}

pub(crate) fn fill_target_range(source: CellRange) -> Option<CellRange> {
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

pub(crate) fn select_cell(app: &SheetsApp, sheet: &Sheet, r: i32, c: i32) {
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

fn inspector_tab_index(index: i32) -> i32 {
    index.clamp(0, 1)
}

#[cfg(test)]
fn inspector_context_matches(index: i32, query: &str) -> bool {
    let (table, cell) = inspector_section_visibility(query);
    match inspector_tab_index(index) {
        0 => table,
        _ => cell,
    }
}

pub(crate) fn apply_theme(app: &SheetsApp, theme: &str) {
    Theme::get(app).set_active_theme(SharedString::from(theme));
}

pub(crate) fn configure_direction(app: &SheetsApp, rtl: bool) {
    app.set_rtl(rtl);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ResponsiveToolbarState {
    icon_only: bool,
    overflow: bool,
    labeled: bool,
}

fn layout_breakpoints(app: &SheetsApp, width: u32) -> ResponsiveToolbarState {
    let policy = ResponsivePolicy::get(app);
    let width = width as f32;
    ResponsiveToolbarState {
        icon_only: width < policy.get_priority_1_icon_only_below(),
        overflow: width < policy.get_priority_2_overflow_below(),
        labeled: width >= policy.get_priority_2_overflow_below(),
    }
}

pub(crate) fn apply_layout_breakpoints(app: &SheetsApp, width: u32) {
    let state = layout_breakpoints(app, width);
    app.set_icon_only_toolbar(state.icon_only);
    app.set_labeled_toolbar(state.labeled);
    app.set_show_quick_formulas(state.labeled);
    app.set_wide_toolbar(state.labeled);
    app.set_labeled_export(state.labeled);
    if !state.overflow && app.get_toolbar_overflow_open() {
        app.invoke_close_toolbar_overflow();
    }
    app.set_overflow_toolbar(state.overflow);
    if !state.overflow {
        app.set_toolbar_overflow_open(false);
    }
    let inspector_available = !state.icon_only;
    let was_inspector_available = app.get_inspector_available();
    app.set_inspector_available(inspector_available);
    if !inspector_available {
        app.set_show_inspector(false);
    } else if !was_inspector_available {
        // Re-entering a reference/wide window restores the contextual panel
        // after compact mode hid it to preserve editing width. A user toggle
        // made while already wide remains authoritative.
        app.set_show_inspector(true);
    }
}

/// Size the headless grid viewport to the same canvas geometry used by the
/// native layout. `snapshot_component` resizes and renders immediately,
/// without running a native resize event through the Slint loop, so relying
/// only on `grid-viewport-changed` would leave the projection at its
/// construction-time fallback size.
pub(crate) fn apply_headless_viewport_size(app: &SheetsApp, width: u32, height: u32) {
    let policy = ResponsivePolicy::get(app);
    let inspector_width = if (width as f32) >= policy.get_priority_1_icon_only_below() {
        INSPECTOR_WIDTH
    } else {
        0.0
    };
    app.set_grid_viewport_width(
        (width as f32 - inspector_width - TABLE_HORIZONTAL_MARGIN).max(GRID_COL_WIDTH),
    );
    app.set_grid_viewport_height(
        (height as f32 - SHELL_VERTICAL_CHROME)
            .max(GRID_ROW_HEIGHT * DEFAULT_VISIBLE_ROWS as f32 + GRID_COLUMN_HEADER_HEIGHT),
    );
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
    configure_direction(&app, args.rtl);
    apply_theme(&app, &args.theme);
    let (w, h) = args.size;
    app.window().set_size(PhysicalSize::new(w, h));
    apply_layout_breakpoints(&app, w);
    apply_headless_viewport_size(&app, w, h);
    let mut sheet = match &args.open {
        Some(p) => load_sheet(Path::new(p))?,
        None => starter_workbook(),
    };
    if args.objects {
        object_actions::seed_demo_objects(&mut sheet);
        app.set_selected_object(0);
    }
    if let Some(zoom) = args.zoom {
        app.set_zoom_factor(zoom);
    }
    project_sheet(&app, &sheet);
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
    if args.chart {
        if let Ok(chart) = plan_chart(&sheet, 0, 1) {
            sheet.chart = Some(chart);
            app.set_chart_visible(true);
            sync_chart_to_app(&app, &sheet);
        }
    }
    let img = snapshot_component(&app, w as f32, h as f32, 1.0).map_err(|e| e.to_string())?;
    loom_test_support::png::save_png(Path::new(out), &img).map_err(|e| e.to_string())?;
    Ok(())
}

pub(crate) struct GuiState {
    pub(crate) current: RefCell<Sheet>,
    pub(crate) sheets: RefCell<Vec<Sheet>>,
    pub(crate) active_sheet_index: RefCell<usize>,
    pub(crate) save_path: RefCell<Option<PathBuf>>,
    pub(crate) undo_stack: RefCell<Vec<SheetTransaction>>,
    pub(crate) redo_stack: RefCell<Vec<SheetTransaction>>,
    pub(crate) sheet_histories: RefCell<Vec<(Vec<SheetTransaction>, Vec<SheetTransaction>)>>,
    pub(crate) dialogs: Rc<dyn FileDialogService>,
    pub(crate) workbook_filter: FileFilter,
    pub(crate) import_filter: FileFilter,
    pub(crate) csv_filter: FileFilter,
    pub(crate) xlsx_filter: FileFilter,
    pub(crate) clipboard: RefCell<Option<Vec<Vec<String>>>>,
    pub(crate) object_gesture: RefCell<Option<object_actions::ObjectGesture>>,
}

impl GuiState {
    pub(crate) fn new(
        sheet: Sheet,
        path: Option<PathBuf>,
        dialogs: Rc<dyn FileDialogService>,
        workbook_filter: FileFilter,
        import_filter: FileFilter,
        csv_filter: FileFilter,
        xlsx_filter: FileFilter,
    ) -> Self {
        Self {
            current: RefCell::new(sheet.clone()),
            sheets: RefCell::new(vec![sheet]),
            active_sheet_index: RefCell::new(0),
            save_path: RefCell::new(path),
            undo_stack: RefCell::new(Vec::new()),
            redo_stack: RefCell::new(Vec::new()),
            sheet_histories: RefCell::new(vec![(Vec::new(), Vec::new())]),
            dialogs,
            workbook_filter,
            import_filter,
            csv_filter,
            xlsx_filter,
            clipboard: RefCell::new(None),
            object_gesture: RefCell::new(None),
        }
    }

    /// Install a fully loaded workbook (all tabs + active index) into a fresh
    /// single-sheet state. Histories start empty: loading is not an undoable
    /// edit, matching open/new/template behavior.
    pub(crate) fn install_workbook(&self, sheets: Vec<Sheet>, active: usize) {
        let mut sheets = sheets;
        if sheets.is_empty() {
            sheets.push(blank_sheet());
        }
        let active = active.min(sheets.len() - 1);
        *self.current.borrow_mut() = sheets[active].clone();
        *self.sheets.borrow_mut() = sheets;
        *self.active_sheet_index.borrow_mut() = active;
        self.undo_stack.borrow_mut().clear();
        self.redo_stack.borrow_mut().clear();
        let tabs = self.sheets.borrow().len();
        *self.sheet_histories.borrow_mut() = vec![(Vec::new(), Vec::new()); tabs];
    }
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
        filters: vec![
            state.workbook_filter.clone(),
            state.import_filter.clone(),
            state.xlsx_filter.clone(),
        ],
    }
}

pub(crate) fn image_open_request(state: &GuiState) -> Result<OpenFileRequest, String> {
    let filter = FileFilter::new("Images", ["png", "jpg", "jpeg", "webp", "gif", "svg"])
        .map_err(|error| error.to_string())?;
    Ok(OpenFileRequest {
        title: "Insert Image".into(),
        initial_directory: initial_directory(state.save_path.borrow().as_deref()),
        suggested_name: None,
        filters: vec![filter],
    })
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

fn export_xlsx_request(state: &GuiState) -> SaveFileRequest {
    SaveFileRequest {
        title: "Export Excel Spreadsheet".into(),
        initial_directory: initial_directory(state.save_path.borrow().as_deref()),
        suggested_name: Some(format!("{}.xlsx", state.current.borrow().name)),
        filters: vec![state.xlsx_filter.clone()],
    }
}

fn is_native_workbook(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("loomtable"))
}

fn replace_opened_workbook(
    app: &SheetsApp,
    state: &GuiState,
    path: PathBuf,
    sheets: Vec<Sheet>,
    active: usize,
) {
    let active = active.min(sheets.len().saturating_sub(1));
    *state.current.borrow_mut() = sheets[active].clone();
    *state.sheets.borrow_mut() = sheets;
    *state.active_sheet_index.borrow_mut() = active;
    *state.save_path.borrow_mut() = is_native_workbook(&path).then_some(path);
    state.undo_stack.borrow_mut().clear();
    state.redo_stack.borrow_mut().clear();
    *state.sheet_histories.borrow_mut() =
        vec![(Vec::new(), Vec::new()); state.sheets.borrow().len()];
    apply_sheet(app, state);
    sync_sheet_tabs(app, state);
}

/// Copy the live current sheet back into its tab slot so multi-tab
/// serialization always sees fresh edits.
fn sync_current_to_tabs(state: &GuiState) {
    let current = state.current.borrow().clone();
    let active = *state.active_sheet_index.borrow();
    let mut sheets = state.sheets.borrow_mut();
    if active >= sheets.len() {
        sheets.resize_with(active + 1, || Sheet::new("Untitled"));
    }
    sheets[active] = current;
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
    sync_current_to_tabs(state);
    let active = *state.active_sheet_index.borrow();
    save_workbook(&path, &state.sheets.borrow(), active)?;
    *state.save_path.borrow_mut() = Some(path.clone());
    let checkpoint = workbook_to_json(&state.sheets.borrow(), active).into_bytes();
    match checkpoint_snapshot_recovery(checkpoint) {
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

/// Register the live undo/redo callbacks shared by the native bootstrap and
/// callback-level tests. Keeping this wiring separate from the window setup
/// ensures every UI action can exercise the same history path headlessly.
pub(crate) fn register_history_actions(
    app: &SheetsApp,
    state: &Rc<GuiState>,
    menu_service: &Arc<NativeMenuBar>,
) {
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_undo(move || {
            if let Some(app) = app_ref.upgrade() {
                let popped = state.undo_stack.borrow_mut().pop();
                if let Some(edit) = popped {
                    if let SheetTransaction::Workbook { before, .. } = &edit {
                        restore_workbook_state(&app, &state, before, &menu_service);
                    } else {
                        {
                            let mut sheet = state.current.borrow_mut();
                            edit.revert(&mut sheet);
                        }
                        apply_sheet(&app, &state);
                        sync_menu_state(&menu_service, &app, &state);
                    }
                    push_history(&mut state.redo_stack.borrow_mut(), edit);
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
                let popped = state.redo_stack.borrow_mut().pop();
                if let Some(edit) = popped {
                    if let SheetTransaction::Workbook { after, .. } = &edit {
                        restore_workbook_state(&app, &state, after, &menu_service);
                    } else {
                        {
                            let mut sheet = state.current.borrow_mut();
                            edit.apply(&mut sheet);
                        }
                        apply_sheet(&app, &state);
                        sync_menu_state(&menu_service, &app, &state);
                    }
                    push_history(&mut state.undo_stack.borrow_mut(), edit);
                }
            }
        });
    }
}

fn run_gui_with_dialogs(args: &Args, dialogs: Rc<dyn FileDialogService>) -> Result<(), String> {
    let app = SheetsApp::new().map_err(|e| e.to_string())?;
    configure_direction(&app, args.rtl);
    apply_theme(&app, &args.theme);
    app.window()
        .set_size(PhysicalSize::new(args.size.0, args.size.1));
    apply_layout_breakpoints(&app, args.size.0);

    let recovered = initialize_snapshot_recovery()?;
    let initial = match &args.open {
        Some(path) => load_workbook(Path::new(path))?,
        None => recovered
            .as_deref()
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
            .and_then(restore_workbook_from_snapshot)
            .unwrap_or_else(|| loom_sheets_core::persistence::WorkbookFile {
                sheets: vec![starter_workbook()],
                active: 0,
            }),
    };
    let workbook_filter = FileFilter::new("Loom Sheets workbook", ["loomtable"])
        .map_err(|error| error.to_string())?;
    let import_filter =
        FileFilter::new("Comma-separated values", ["csv"]).map_err(|error| error.to_string())?;
    let csv_filter = import_filter.clone();
    let xlsx_filter =
        FileFilter::new("Excel Spreadsheet", ["xlsx"]).map_err(|error| error.to_string())?;
    let initial_path = args.open.as_ref().map(PathBuf::from);
    let state = Rc::new(GuiState::new(
        starter_workbook(),
        initial_path.filter(|path| is_native_workbook(path)),
        dialogs,
        workbook_filter,
        import_filter,
        csv_filter,
        xlsx_filter,
    ));
    state.install_workbook(initial.sheets, initial.active);
    if args.objects {
        let active = *state.active_sheet_index.borrow();
        let mut sheet = state.current.borrow_mut();
        object_actions::seed_demo_objects(&mut sheet);
        state.sheets.borrow_mut()[active] = sheet.clone();
        app.set_selected_object(0);
    }
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
                let sheet = blank_sheet();
                *state.current.borrow_mut() = sheet.clone();
                *state.sheets.borrow_mut() = vec![sheet];
                *state.active_sheet_index.borrow_mut() = 0;
                *state.save_path.borrow_mut() = None;
                state.undo_stack.borrow_mut().clear();
                state.redo_stack.borrow_mut().clear();
                *state.sheet_histories.borrow_mut() = vec![(Vec::new(), Vec::new())];
                apply_sheet(&app, &state);
                sync_sheet_tabs(&app, &state);
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
                        apply_sheet(&app, &state);
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
        let app_ref = app.as_weak();
        app.on_begin_edit(move |initial_text| {
            if let Some(app) = app_ref.upgrade() {
                app.set_formula_edit_buffer(initial_text);
                app.invoke_focus_formula_bar();
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
                    let range_str = app.get_selection_range();
                    let formula_text = if range_str.contains(':') {
                        format!("={func}({range_str})")
                    } else {
                        let col = cell
                            .to_a1()
                            .trim_end_matches(|c: char| c.is_ascii_digit())
                            .to_string();
                        format!("={func}({col}1:{col}5)")
                    };
                    let committed = {
                        let mut current = state.current.borrow_mut();
                        let mut undo = state.undo_stack.borrow_mut();
                        let mut redo = state.redo_stack.borrow_mut();
                        commit_formula_edit(&mut current, &mut undo, &mut redo, cell, &formula_text)
                    };
                    if committed {
                        apply_sheet(&app, &state);
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
                    Ok(Some(path)) => match load_workbook(&path) {
                        Ok(workbook) => {
                            let imported = !is_native_workbook(&path);
                            let tabs = workbook.sheets.len();
                            replace_opened_workbook(
                                &app,
                                &state,
                                path.clone(),
                                workbook.sheets,
                                workbook.active,
                            );
                            sync_menu_state(&menu_service, &app, &state);
                            app.set_status_left(SharedString::from(if imported {
                                format!(
                                    "Imported {}; use Save As for a Loom workbook",
                                    path.display()
                                )
                            } else {
                                format!(
                                    "Opened {} ({} {})",
                                    path.display(),
                                    tabs,
                                    if tabs == 1 { "sheet" } else { "sheets" }
                                )
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
                        let sheet = state.current.borrow().clone();
                        let csv = to_csv_with_formulas(&sheet);
                        let name = sheet.name.clone();
                        match loom_storage::atomic_write(&path, csv.as_bytes()) {
                            Ok(()) => app.set_status_left(SharedString::from(format!(
                                "Exported {name} to {} (formulas preserved)",
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
        app.on_export_xlsx(move || {
            if let Some(app) = app_ref.upgrade() {
                match state.dialogs.save_file(&export_xlsx_request(&state)) {
                    Ok(Some(path)) => {
                        let (siblings, _) = workbook_sheets(&state);
                        let tabs = siblings.len();
                        match loom_sheets_core::export_xlsx_sheets(&siblings) {
                            Ok(bytes) => match loom_storage::atomic_write(&path, &bytes) {
                                Ok(()) => app.set_status_left(SharedString::from(format!(
                                    "Exported {} {} to {} (formulas kept)",
                                    tabs,
                                    if tabs == 1 { "sheet" } else { "sheets" },
                                    path.display()
                                ))),
                                Err(error) => app.set_status_left(SharedString::from(format!(
                                    "Export failed: {error}"
                                ))),
                            },
                            Err(error) => app.set_status_left(SharedString::from(format!(
                                "Excel generation failed: {error}"
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
    register_history_actions(&app, &state, &menu_service);
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_cell_clicked(move |r, c| {
            if let Some(app) = app_ref.upgrade() {
                select_cell(&app, &state.current.borrow(), r, c);
                project_current(&app, &state);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_navigate_selection(move |row_delta, col_delta| {
            if let Some(app) = app_ref.upgrade() {
                navigate_selection(&app, &state.current.borrow(), row_delta, col_delta);
                project_current(&app, &state);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_extend_selection(move |row_delta, col_delta| {
            if let Some(app) = app_ref.upgrade() {
                extend_selection(&app, &state.current.borrow(), row_delta, col_delta);
                project_current(&app, &state);
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
                    apply_sheet(&app, &state);
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
                project_current_without_reveal(&app, &state);
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
                    project_current(&app, &state);
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
                // Search is also a context switch when only one inspector
                // section matches. This keeps a query such as "formula"
                // useful even when the Table tab was previously selected.
                if table && !cell {
                    app.set_inspector_tab(0);
                } else if cell && !table {
                    app.set_inspector_tab(1);
                }
            }
        });
    }

    {
        let app_ref = app.as_weak();
        app.on_inspector_context_changed(move |index| {
            if let Some(app) = app_ref.upgrade() {
                // TabStrip owns the visual selection; clamp the mirrored
                // state so keyboard/programmatic activation cannot address a
                // context that has no inspector section.
                app.set_inspector_tab(inspector_tab_index(index));
                let (table, cell) =
                    inspector_section_visibility(app.get_inspector_search().as_str());
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
    // Template-chooser callbacks live in `register_sheet_actions` (actions.rs)
    // alongside every other sheet callback; see `create_template_workbook`.

    let mut menu_bar = build_standard_menu_bar(
        "Loom Sheets",
        vec![
            MenuItem::action("file.new_template", "New from Template..."),
            MenuItem::action_with_shortcut(
                "file.export_csv",
                "Export to CSV...",
                MenuShortcut::primary("E"),
            ),
            MenuItem::action("file.export_xlsx", "Export to Excel (.xlsx)..."),
        ],
        vec![],
        vec![MenuItem::check("view.inspector", "Format Inspector", false)],
        vec![Menu::new(
            "Table",
            vec![
                MenuItem::action("table.add_row", "Add Row"),
                MenuItem::action("table.delete_row", "Delete Row"),
                MenuItem::action("table.add_col", "Add Column"),
                MenuItem::action("table.delete_col", "Delete Column"),
                MenuItem::action("table.sort_asc", "Sort Ascending"),
                MenuItem::action("table.sort_desc", "Sort Descending"),
                MenuItem::action("table.freeze_header", "Freeze Header Row"),
                MenuItem::action("table.unfreeze_panes", "Unfreeze Panes"),
                MenuItem::action("table.pivot_sum", "Pivot Summary (Sum)"),
                MenuItem::action("sheets.insert_shape", "Insert Shape"),
                MenuItem::action("sheets.insert_image", "Insert Image"),
                MenuItem::action("sheets.delete_sheet", "Delete Sheet"),
            ],
        )],
    );
    // Only commands with a registered Sheets/controller sink are enabled.
    // Application/window/help entries remain disabled until a real native
    // host bridge is installed for them.
    menu_bar.disable_items_except([
        "file.new",
        "file.new_template",
        "file.open",
        "file.save",
        "file.save_as",
        "file.export_csv",
        "file.export_xlsx",
        "edit.undo",
        "edit.redo",
        "edit.cut",
        "edit.copy",
        "edit.paste",
        "edit.select_all",
        "app.palette",
        "view.inspector",
        "view.zoom_in",
        "view.zoom_out",
        "view.zoom_actual",
        "table.add_row",
        "table.delete_row",
        "table.add_col",
        "table.delete_col",
        "table.sort_asc",
        "table.sort_desc",
        "table.freeze_header",
        "table.unfreeze_panes",
        "table.pivot_sum",
        "sheets.insert_shape",
        "sheets.insert_image",
        "sheets.delete_sheet",
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

    register_sheet_actions(&app, &state, &menu_service);
    object_actions::register_object_actions(&app, &state, &menu_service);
    if let Some(zoom) = args.zoom {
        app.set_zoom_factor(zoom);
    }
    apply_sheet(&app, &state);
    if args.objects {
        app.set_selected_object(0);
    }
    sync_sheet_tabs(&app, &state);
    sync_menu_state_result(&menu_service, &app, &state).map_err(|error| error.to_string())?;
    wire_palette(&app);
    if args.chart {
        let chart = {
            let sheet = state.current.borrow();
            plan_chart(&sheet, 0, 1).ok()
        };
        if let Some(chart) = chart {
            state.current.borrow_mut().chart = Some(chart);
            app.set_chart_visible(true);
            sync_chart_to_app(&app, &state.current.borrow());
        }
    }
    if args.template_chooser {
        app.set_template_chooser_open(true);
    }
    app.show().map_err(|e| e.to_string())?;
    // A visible selection is not enough to receive keyboard input. Focus the
    // grid after the native window is shown; winit may replace the focus item
    // during presentation, so doing this before `show` is not durable.
    app.invoke_focus_grid();
    if args.objects {
        // Keep the scalar selection in sync with the projected selection list.
        app.set_selected_object(0);
        let app_ref = app.as_weak();
        slint::invoke_from_event_loop(move || {
            if let Some(app) = app_ref.upgrade() {
                app.set_selected_object(0);
            }
        })
        .map_err(|error| error.to_string())?;
    }
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

#[cfg(test)]
#[path = "main_tests.rs"]
mod tests;
