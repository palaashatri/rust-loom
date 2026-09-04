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
use loom_sheets_core::CellEditTransaction;
use loom_sheets_core::{
    evaluate, from_csv, sheet_from_json, sheet_to_json, to_csv, CellRange, CellRef, GridSelection,
    RangeEdit, Sheet, SheetDimensions, SheetViewport, Value, DEFAULT_COL_WIDTH, DEFAULT_ROW_HEIGHT,
};
use loom_test_support::capture::{set_platform, snapshot_component};
use slint::{ComponentHandle, ModelRc, PhysicalSize, SharedString, VecModel};

slint::include_modules!();

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
const INSPECTOR_WIDTH: f32 = 280.0;
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
    pub(crate) journey: Option<String>,
    pub(crate) size: (u32, u32),
    pub(crate) theme: String,
    pub(crate) rtl: bool,
    pub(crate) open: Option<String>,
    pub(crate) template_chooser: bool,
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
        journey: None,
        size: DEFAULT_SIZE,
        theme: "light".to_string(),
        rtl: false,
        open: None,
        template_chooser: false,
    };
    let mut it = raw_args.into_iter().map(Into::into);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--screenshot" => args.screenshot = Some(it.next().ok_or("--screenshot needs a path")?),
            "--smoke" => args.smoke = true,
            "--palette" => args.palette = true,
            "--chart" => args.chart = true,
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

pub(crate) fn load_sheet(path: &Path) -> Result<Sheet, String> {
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

pub(crate) fn save_sheet(path: &Path, sheet: &Sheet) -> Result<(), String> {
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

fn editor_dimensions(sheet: &Sheet, selected: CellRef) -> SheetDimensions {
    let dimensions = sheet.dimensions();
    // Keep an empty/new sheet navigable beyond A1 while retaining sparse
    // workbook dimensions for populated sheets.
    SheetDimensions::new(
        dimensions
            .rows
            .max(selected.row.saturating_add(1))
            .max(DEFAULT_VISIBLE_ROWS),
        dimensions
            .cols
            .max(selected.col.saturating_add(1))
            .max(DEFAULT_VISIBLE_COLS),
    )
}

/// Return the default width used by the projected grid. Small workbooks use
/// the available table width (up to a comfortable cap); larger/sparse sheets
/// retain the persisted 80px default so horizontal scrolling remains useful.
fn grid_default_col_width(sheet: &Sheet, dimensions: SheetDimensions, viewport_width: f32) -> f32 {
    if !sheet.col_widths.is_empty() || dimensions.cols > DEFAULT_VISIBLE_COLS {
        return GRID_COL_WIDTH;
    }
    let available = (viewport_width - GRID_ROW_HEADER_WIDTH).max(0.0);
    let fitted = available / dimensions.cols.max(1) as f32;
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

fn column_width(sheet: &Sheet, col: u32, default_width: f32) -> f32 {
    dimension_size(col, default_width, &sheet.col_widths)
}

fn row_height(sheet: &Sheet, row: u32) -> f32 {
    dimension_size(row, GRID_ROW_HEIGHT, &sheet.row_heights)
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

fn grid_geometry(
    sheet: &Sheet,
    dimensions: SheetDimensions,
    viewport: SheetViewport,
    viewport_width: f32,
) -> GridGeometry {
    let default_col_width = grid_default_col_width(sheet, dimensions, viewport_width);
    let column_widths: Vec<f32> = (0..viewport.visible_cols)
        .filter_map(|index| viewport.column_at(index))
        .map(|col| column_width(sheet, col, default_col_width))
        .collect();
    let row_heights: Vec<f32> = (0..viewport.visible_rows)
        .filter_map(|index| viewport.row_at(index))
        .map(|row| row_height(sheet, row))
        .collect();
    let visible_width = column_widths.iter().sum();
    let visible_height = row_heights.iter().sum();
    let content_width = if sheet.col_widths.is_empty() {
        dimensions.cols as f32 * default_col_width
    } else {
        dimension_extent(dimensions.cols, default_col_width, &sheet.col_widths)
    };
    let content_height = dimension_extent(dimensions.rows, GRID_ROW_HEIGHT, &sheet.row_heights);
    GridGeometry {
        default_col_width,
        column_widths,
        row_heights,
        col_offset: dimension_offset(viewport.first_col, default_col_width, &sheet.col_widths),
        row_offset: dimension_offset(viewport.first_row, GRID_ROW_HEIGHT, &sheet.row_heights),
        visible_width,
        visible_height,
        content_width,
        content_height,
    }
}

fn viewport_from_app(app: &SheetsApp, sheet: &Sheet) -> SheetViewport {
    let selected = selection_from_app(app).focus;
    let dimensions = editor_dimensions(sheet, selected);
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
    let default_col_width = grid_default_col_width(sheet, dimensions, viewport_width);
    let scroll_x = (-app.get_grid_scroll_x()).max(0.0);
    let scroll_y = (-app.get_grid_scroll_y()).max(0.0);
    viewport_from_dimensions(
        (scroll_x, scroll_y),
        (
            (viewport_width - GRID_ROW_HEADER_WIDTH).max(default_col_width),
            (viewport_height - GRID_COLUMN_HEADER_HEIGHT).max(GRID_ROW_HEIGHT),
        ),
        dimensions,
        default_col_width,
        &sheet.row_heights,
        &sheet.col_widths,
    )
}

pub(crate) fn apply_sheet(app: &SheetsApp, sheet: &Sheet) {
    apply_sheet_inner(app, sheet, true);
}

pub(crate) fn apply_sheet_without_reveal(app: &SheetsApp, sheet: &Sheet) {
    apply_sheet_inner(app, sheet, false);
}

fn apply_sheet_inner(app: &SheetsApp, sheet: &Sheet, reveal_selection: bool) {
    let vals = evaluate(sheet);
    let selection = selection_from_app(app);
    let selected = selection.focus;
    let dimensions = sheet.dimensions();
    let editor_dimensions = editor_dimensions(sheet, selected);
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
    let geometry = grid_geometry(
        sheet,
        editor_dimensions,
        viewport,
        app.get_grid_viewport_width(),
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
    app.set_grid_col_width(geometry.default_col_width);
    app.set_grid_row_height(GRID_ROW_HEIGHT);
    app.set_grid_col_offset(geometry.col_offset);
    app.set_grid_row_offset(geometry.row_offset);
    app.set_grid_visible_width(geometry.visible_width);
    app.set_grid_visible_height(geometry.visible_height);
    app.set_grid_content_width(geometry.content_width);
    app.set_grid_content_height(geometry.content_height);
    app.set_grid_column_widths(ModelRc::new(VecModel::from(geometry.column_widths)));
    app.set_grid_row_heights(ModelRc::new(VecModel::from(geometry.row_heights)));
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
    if app.get_selection_count() <= 1 {
        app.set_status_right("Offline".into());
    }
    actions::sync_chart_to_app(app, sheet);
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

/// Apply one committed formula-bar edit and record one undo transaction.
pub(crate) fn commit_formula_edit(
    sheet: &mut Sheet,
    undo_stack: &mut Vec<RangeEdit>,
    redo_stack: &mut Vec<RangeEdit>,
    selected: CellRef,
    draft: &str,
) -> bool {
    let edit = RangeEdit::replace(sheet, selected, Some(draft.to_owned()));
    if edit.is_noop() {
        return false;
    }
    edit.apply(sheet);
    undo_stack.push(edit);
    redo_stack.clear();
    true
}

/// Apply a sparse range edit as one undoable transaction.
fn commit_range_edit(
    sheet: &mut Sheet,
    undo_stack: &mut Vec<RangeEdit>,
    redo_stack: &mut Vec<RangeEdit>,
    edit: RangeEdit,
) -> bool {
    if edit.is_empty() || edit.is_noop() {
        return false;
    }
    edit.apply(sheet);
    undo_stack.push(edit);
    redo_stack.clear();
    true
}

/// The Sheets fill handle repeats a selected block into the next block below
/// it.  A single-cell selection has no fill source and is therefore disabled
/// in the UI rather than pretending to perform an operation.
pub(crate) fn fill_selection_down(
    sheet: &mut Sheet,
    undo_stack: &mut Vec<RangeEdit>,
    redo_stack: &mut Vec<RangeEdit>,
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
    let sheet = match &args.open {
        Some(p) => load_sheet(Path::new(p))?,
        None => starter_workbook(),
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
    if args.chart {
        app.set_chart_visible(true);
        sync_chart_to_app(&app, &sheet);
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
    pub(crate) undo_stack: RefCell<Vec<RangeEdit>>,
    pub(crate) redo_stack: RefCell<Vec<RangeEdit>>,
    pub(crate) dialogs: Rc<dyn FileDialogService>,
    pub(crate) workbook_filter: FileFilter,
    pub(crate) import_filter: FileFilter,
    pub(crate) csv_filter: FileFilter,
}

impl GuiState {
    pub(crate) fn new(
        sheet: Sheet,
        path: Option<PathBuf>,
        dialogs: Rc<dyn FileDialogService>,
        workbook_filter: FileFilter,
        import_filter: FileFilter,
        csv_filter: FileFilter,
    ) -> Self {
        Self {
            current: RefCell::new(sheet.clone()),
            sheets: RefCell::new(vec![sheet]),
            active_sheet_index: RefCell::new(0),
            save_path: RefCell::new(path),
            undo_stack: RefCell::new(Vec::new()),
            redo_stack: RefCell::new(Vec::new()),
            dialogs,
            workbook_filter,
            import_filter,
            csv_filter,
        }
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
    *state.current.borrow_mut() = sheet.clone();
    *state.sheets.borrow_mut() = vec![sheet];
    *state.active_sheet_index.borrow_mut() = 0;
    *state.save_path.borrow_mut() = is_native_workbook(&path).then_some(path);
    state.undo_stack.borrow_mut().clear();
    state.redo_stack.borrow_mut().clear();
    apply_sheet(app, &state.current.borrow());
    sync_sheet_tabs(app, state);
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
    configure_direction(&app, args.rtl);
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
            .unwrap_or_else(starter_workbook),
    };
    let workbook_filter = FileFilter::new("Loom Sheets workbook", ["loomtable"])
        .map_err(|error| error.to_string())?;
    let import_filter =
        FileFilter::new("Comma-separated values", ["csv"]).map_err(|error| error.to_string())?;
    let csv_filter = import_filter.clone();
    let initial_path = args.open.as_ref().map(PathBuf::from);
    let state = Rc::new(GuiState::new(
        initial_sheet,
        initial_path.filter(|path| is_native_workbook(path)),
        dialogs,
        workbook_filter,
        import_filter,
        csv_filter,
    ));
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
                apply_sheet(&app, &state.current.borrow());
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
                if let Some(edit) = state.undo_stack.borrow_mut().pop() {
                    let mut sheet = state.current.borrow_mut();
                    edit.revert(&mut sheet);
                    state.redo_stack.borrow_mut().push(edit);
                    apply_sheet(&app, &sheet);
                    sync_menu_state(&menu_service, &app, &state);
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
                if let Some(edit) = state.redo_stack.borrow_mut().pop() {
                    let mut sheet = state.current.borrow_mut();
                    edit.apply(&mut sheet);
                    state.undo_stack.borrow_mut().push(edit);
                    apply_sheet(&app, &sheet);
                    sync_menu_state(&menu_service, &app, &state);
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
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_create_template(move |idx| {
            if let Some(app) = app_ref.upgrade() {
                let sheet = match idx {
                    1 => {
                        let mut s = Sheet::new("Monthly Budget");
                        for (c, v) in [
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
                        ] {
                            s.set_str(c, v);
                        }
                        s
                    }
                    2 => {
                        let mut s = Sheet::new("Invoice");
                        for (c, v) in [
                            ("A1", "Description"),
                            ("B1", "Hours"),
                            ("C1", "Rate"),
                            ("D1", "Amount"),
                            ("A2", "Design Work"),
                            ("B2", "20"),
                            ("C2", "85"),
                            ("D2", "=B2*C2"),
                        ] {
                            s.set_str(c, v);
                        }
                        s
                    }
                    _ => blank_sheet(),
                };
                *state.current.borrow_mut() = sheet.clone();
                *state.sheets.borrow_mut() = vec![sheet];
                *state.active_sheet_index.borrow_mut() = 0;
                *state.save_path.borrow_mut() = None;
                state.undo_stack.borrow_mut().clear();
                state.redo_stack.borrow_mut().clear();
                apply_sheet(&app, &state.current.borrow());
                sync_sheet_tabs(&app, &state);
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

    register_sheet_actions(&app, &state, &menu_service);
    apply_sheet(&app, &state.current.borrow());
    sync_sheet_tabs(&app, &state);
    sync_menu_state_result(&menu_service, &app, &state).map_err(|error| error.to_string())?;
    wire_palette(&app);
    if args.chart {
        app.set_chart_visible(true);
        sync_chart_to_app(&app, &state.current.borrow());
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use slint::Model;

    #[test]
    fn rtl_argument_is_parsed_and_applied_to_the_root() {
        let args = parse_args_from(["--rtl"] as [&str; 1]).expect("parse --rtl");
        assert!(args.rtl);

        set_platform();
        let app = SheetsApp::new().expect("create SheetsApp");
        configure_direction(&app, args.rtl);
        assert!(app.get_rtl());
    }

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
        let state = GuiState::new(
            starter_workbook(),
            Some(PathBuf::from("/tmp/current.loomtable")),
            dialogs,
            FileFilter::new("Workbook", ["loomtable"]).expect("filter"),
            FileFilter::new("CSV", ["csv"]).expect("filter"),
            FileFilter::new("CSV", ["csv"]).expect("filter"),
        );
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
        let mut redo = vec![RangeEdit::replace(
            &sheet,
            selected,
            Some("redo".to_string()),
        )];

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
        assert_eq!(viewport.first_row, 28);
        assert_eq!(viewport.first_col, 2);
        assert_eq!(viewport.visible_rows, 11);
        assert_eq!(viewport.visible_cols, 5);
        assert!(viewport.contains(CellRef::parse("C29").unwrap()));
        assert!(!viewport.contains(CellRef::parse("B29").unwrap()));
    }

    #[test]
    fn custom_dimensions_drive_viewport_projection_and_offsets() {
        set_platform();
        let app = SheetsApp::new().expect("create SheetsApp");
        app.set_grid_viewport_width(400.0);
        app.set_grid_viewport_height(200.0);
        app.set_grid_scroll_x(-170.0);
        app.set_grid_scroll_y(-50.0);

        let mut sheet = Sheet::new("custom viewport");
        sheet.set_col_width(0, 160.0);
        sheet.set_row_height(0, 48.0);
        sheet.set_str("B2", "target");

        apply_sheet_without_reveal(&app, &sheet);

        assert_eq!(app.get_column_headers().row_data(0).as_deref(), Some("B"));
        assert_eq!(app.get_row_headers().row_data(0).as_deref(), Some("2"));
        assert_eq!(app.get_cells().row_data(0).as_deref(), Some("target"));
        assert_eq!(app.get_grid_col_offset(), 160.0);
        assert_eq!(app.get_grid_row_offset(), 48.0);
    }

    #[test]
    fn sparse_tail_scroll_materializes_tail_headers_and_values() {
        set_platform();
        let app = SheetsApp::new().expect("create SheetsApp");
        app.window().set_size(PhysicalSize::new(1280, 800));
        apply_layout_breakpoints(&app, 1280);
        apply_headless_viewport_size(&app, 1280, 800);
        app.set_grid_scroll_y(-26_600.0);

        let mut sheet = Sheet::new("Sparse 1000");
        sheet.set_str("A995", "10");
        sheet.set_str("A996", "20");
        sheet.set_str("A1000", "tail");
        apply_sheet_without_reveal(&app, &sheet);

        let row_headers = app.get_row_headers();
        assert_eq!(row_headers.row_data(0).as_deref(), Some("979"));
        assert_eq!(row_headers.row_data(21).as_deref(), Some("1000"));
        let cells = app.get_cells();
        assert_eq!(cells.row_data(16 * 8).as_deref(), Some("10"));
        assert_eq!(cells.row_data(17 * 8).as_deref(), Some("20"));
        assert_eq!(cells.row_data(21 * 8).as_deref(), Some("tail"));
        assert!((app.get_grid_scroll_y() + 23_478.0).abs() < 0.1);
    }

    #[test]
    fn reverse_shift_extension_keeps_anchor_and_normalizes_range() {
        set_platform();
        let app = SheetsApp::new().expect("create SheetsApp");
        let sheet = starter_workbook();
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
    fn fill_down_records_one_undo_operation_and_restores_sparse_cells() {
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

        let previous = undo.pop().expect("fill undo operation");
        previous.revert(&mut sheet);
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
    fn inspector_context_switch_only_exposes_the_selected_context() {
        assert_eq!(inspector_tab_index(-1), 0);
        assert_eq!(inspector_tab_index(0), 0);
        assert_eq!(inspector_tab_index(1), 1);
        assert_eq!(inspector_tab_index(4), 1);

        assert!(inspector_context_matches(0, "rows"));
        assert!(!inspector_context_matches(1, "rows"));
        assert!(!inspector_context_matches(0, "formula"));
        assert!(inspector_context_matches(1, "formula"));
        assert!(!inspector_context_matches(1, "unknown"));
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
    fn focused_grid_starts_formula_edits_and_tab_navigates_selection() {
        set_platform();
        let app = SheetsApp::new().expect("create SheetsApp");
        app.set_selection_formula("=A1+1".into());
        let app_ref = app.as_weak();
        app.on_begin_edit(move |initial_text| {
            if let Some(app) = app_ref.upgrade() {
                app.set_formula_edit_buffer(initial_text);
                app.invoke_focus_formula_bar();
            }
        });
        let cancels = Rc::new(std::cell::Cell::new(0));
        let cancels_ref = cancels.clone();
        app.on_cancel_selected_cell(move || cancels_ref.set(cancels_ref.get() + 1));
        app.invoke_focus_grid();
        app.window()
            .dispatch_event(slint::platform::WindowEvent::KeyPressed {
                text: slint::platform::Key::Return.into(),
            });
        assert_eq!(app.get_formula_edit_buffer().as_str(), "=A1+1");

        app.window()
            .dispatch_event(slint::platform::WindowEvent::KeyPressed {
                text: slint::platform::Key::Escape.into(),
            });
        assert_eq!(cancels.get(), 1);

        app.invoke_focus_grid();
        app.window()
            .dispatch_event(slint::platform::WindowEvent::KeyPressed { text: "x".into() });
        assert_eq!(app.get_formula_edit_buffer().as_str(), "x");

        let moves = Rc::new(std::cell::Cell::new((0, 0)));
        let moves_ref = moves.clone();
        app.on_navigate_selection(move |row_delta, col_delta| {
            moves_ref.set((row_delta, col_delta));
        });
        app.invoke_focus_grid();
        app.window()
            .dispatch_event(slint::platform::WindowEvent::KeyPressed {
                text: slint::platform::Key::Tab.into(),
            });
        assert_eq!(moves.get(), (0, 1));
    }

    #[test]
    fn focused_grid_rejects_non_printable_edit_keys() {
        set_platform();
        let app = SheetsApp::new().expect("create SheetsApp");
        let begins = Rc::new(std::cell::Cell::new(0));
        let begins_ref = begins.clone();
        app.on_begin_edit(move |_| begins_ref.set(begins_ref.get() + 1));
        let moves = Rc::new(std::cell::Cell::new((0, 0)));
        let moves_ref = moves.clone();
        app.on_navigate_selection(move |row_delta, col_delta| {
            moves_ref.set((row_delta, col_delta));
        });

        for text in [
            slint::platform::Key::Backspace.into(),
            slint::platform::Key::Delete.into(),
            slint::platform::Key::F1.into(),
            slint::platform::Key::Home.into(),
            slint::platform::Key::PageUp.into(),
        ] {
            app.invoke_focus_grid();
            app.window()
                .dispatch_event(slint::platform::WindowEvent::KeyPressed { text });
        }
        assert_eq!(begins.get(), 0);
        assert_eq!(moves.get(), (0, 0));

        app.invoke_focus_grid();
        app.window()
            .dispatch_event(slint::platform::WindowEvent::KeyPressed {
                text: slint::platform::Key::Backtab.into(),
            });
        assert_eq!(begins.get(), 0);
        assert_eq!(moves.get(), (0, -1));
    }

    #[test]
    fn typed_history_undo_redo_restores_exact_raw_values() {
        let mut sheet = Sheet::new("history");
        let cell = CellRef::parse("A1").unwrap();
        sheet.set_raw(cell, "old");
        let mut undo = Vec::new();
        let mut redo = Vec::new();

        assert!(commit_formula_edit(
            &mut sheet, &mut undo, &mut redo, cell, ""
        ));
        assert_eq!(sheet.raw(cell), Some(""));
        let edit = undo.pop().expect("typed edit");
        edit.revert(&mut sheet);
        assert_eq!(sheet.raw(cell), Some("old"));
        redo.push(edit);
        let edit = redo.pop().expect("redo edit");
        edit.apply(&mut sheet);
        assert_eq!(sheet.raw(cell), Some(""));

        let absent = CellRef::parse("B1").unwrap();
        assert!(commit_formula_edit(
            &mut sheet, &mut undo, &mut redo, absent, ""
        ));
        assert_eq!(sheet.raw(absent), Some(""));
        undo.pop().expect("absent edit").revert(&mut sheet);
        assert_eq!(sheet.raw(absent), None);
    }

    #[test]
    fn quick_formula_insert_evaluation() {
        let mut sheet = Sheet::new("test");
        for (c, v) in [
            ("A1", "10"),
            ("A2", "20"),
            ("A3", "30"),
            ("A4", "40"),
            ("A5", "50"),
        ] {
            sheet.set_str(c, v);
        }

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
        set_platform();
        let app = SheetsApp::new().expect("create SheetsApp");
        let policy = ResponsivePolicy::get(&app);
        assert_eq!(policy.get_priority_1_icon_only_below(), 1180.0);
        assert_eq!(policy.get_priority_2_overflow_below(), 1320.0);
        let expected = [
            (1179, true, true, false),
            (1180, false, true, false),
            (1279, false, true, false),
            (1280, false, true, false),
            (1319, false, true, false),
            (1320, false, false, true),
        ];
        for (width, icon_only, overflow, labeled) in expected {
            assert_eq!(
                layout_breakpoints(&app, width),
                ResponsiveToolbarState {
                    icon_only,
                    overflow,
                    labeled,
                }
            );
            apply_layout_breakpoints(&app, width);
            assert_eq!(app.get_icon_only_toolbar(), icon_only);
            assert_eq!(app.get_overflow_toolbar(), overflow);
            assert_eq!(app.get_labeled_toolbar(), labeled);
        }
    }

    #[test]
    fn sheets_inspector_is_open_by_default_for_reference_windows() {
        set_platform();
        let app = SheetsApp::new().expect("create SheetsApp");
        assert!(app.get_show_inspector());
        apply_layout_breakpoints(&app, 1024);
        assert!(app.get_overflow_toolbar());
        assert!(!app.get_inspector_available());
        assert!(!app.get_show_inspector());
        apply_layout_breakpoints(&app, 1180);
        assert!(app.get_overflow_toolbar());
        assert!(app.get_inspector_available());
        assert!(app.get_show_inspector());
        apply_layout_breakpoints(&app, 1280);
        assert!(app.get_overflow_toolbar());
        assert!(app.get_inspector_available() && app.get_show_inspector());
        app.set_show_inspector(false);
        apply_layout_breakpoints(&app, 1280);
        assert!(!app.get_show_inspector());
        apply_layout_breakpoints(&app, 1320);
        assert!(!app.get_overflow_toolbar());
    }

    #[test]
    fn grid_geometry_uses_core_defaults_and_fits_small_workbooks() {
        assert_eq!(GRID_COL_WIDTH, DEFAULT_COL_WIDTH);
        assert_eq!(GRID_ROW_HEIGHT, DEFAULT_ROW_HEIGHT);

        let mut small = Sheet::new("small");
        small.set_str("C3", "value");
        let dimensions = editor_dimensions(&small, CellRef::parse("A1").unwrap());
        assert_eq!(dimensions, SheetDimensions::new(15, 8));

        let fitted = grid_default_col_width(&small, dimensions, 1_000.0);
        assert_eq!(fitted, 120.5);
        let viewport = SheetViewport::new(4, 8);
        let geometry = grid_geometry(&small, dimensions, viewport, 1_000.0);
        assert_eq!(geometry.column_widths.len(), 8);
        assert!(geometry.column_widths.iter().all(|width| *width == fitted));
        assert_eq!(geometry.content_width, 8.0 * fitted);

        let mut sparse = Sheet::new("sparse");
        sparse.set_str("AZ1000", "tail");
        let sparse_dimensions = editor_dimensions(&sparse, CellRef::parse("A1").unwrap());
        assert_eq!(sparse_dimensions, SheetDimensions::new(1_000, 52));
        assert_eq!(
            grid_default_col_width(&sparse, sparse_dimensions, 1_000.0),
            GRID_COL_WIDTH
        );
    }

    #[test]
    fn grid_geometry_retains_persisted_row_and_column_dimensions() {
        let mut sheet = Sheet::new("custom");
        sheet.set_str("B3", "value");
        sheet.set_col_width(1, 140.0);
        sheet.set_row_height(2, 40.0);
        let dimensions = editor_dimensions(&sheet, CellRef::parse("A1").unwrap());
        let viewport = SheetViewport::new(4, 3);
        let geometry = grid_geometry(&sheet, dimensions, viewport, 640.0);
        assert_eq!(geometry.column_widths, vec![80.0, 140.0, 80.0]);
        assert_eq!(geometry.row_heights, vec![24.0, 24.0, 40.0, 24.0]);
        assert_eq!(geometry.content_width, 8.0 * 80.0 + 60.0);
        assert_eq!(geometry.content_height, 15.0 * 24.0 + 16.0);

        let json = sheet_to_json(&sheet);
        let reopened = sheet_from_json(&json).expect("dimension metadata round-trips");
        assert_eq!(reopened.col_width(1), 140.0);
        assert_eq!(reopened.row_height(2), 40.0);
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
        let state = GuiState::new(
            starter_workbook(),
            None,
            dialogs,
            FileFilter::new("Workbook", ["loomtable"]).expect("filter"),
            FileFilter::new("CSV", ["csv"]).expect("filter"),
            FileFilter::new("CSV", ["csv"]).expect("filter"),
        );
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

        apply_layout_breakpoints(&app, 1320);

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
