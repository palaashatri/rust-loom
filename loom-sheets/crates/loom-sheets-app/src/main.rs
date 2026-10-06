//! Loom Sheets desktop application.
//!
//! GUI mode opens a real window (winit backend). Headless modes
//! (`--screenshot`, `--smoke`, `--example`) render the same UI through the software
//! renderer and write a PNG, which is what the Docker visual-QA pipeline
//! and the offline test mode exercise.
#![cfg_attr(
    all(windows, not(test), not(debug_assertions)),
    windows_subsystem = "windows"
)]

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

#[cfg(test)]
use loom_desktop::{build_standard_menu_bar, Menu, MenuItem};
use loom_desktop::{
    CommandAction, CommandStateProjection, DesktopError, FileDialogService, FileFilter,
    MenuBarService, NativeFileDialogs, NativeMenuBar, OpenFileRequest, SaveFileRequest,
};
use loom_sheets_core::persistence::WorkbookFile;
#[cfg(test)]
use loom_sheets_core::sheet_to_json;
use loom_sheets_core::style::CellStyle;
use loom_sheets_core::workbook::evaluate_workbook;
#[cfg(test)]
use loom_sheets_core::CellEditTransaction;
use loom_sheets_core::{
    evaluate, workbook_to_json, CellAlignment, CellRange, CellRef, GridSelection, NumberFormat,
    RangeEdit, Sheet, SheetDimensions, SheetViewport, Value, DEFAULT_COL_WIDTH, DEFAULT_ROW_HEIGHT,
};
use loom_test_support::capture::{set_platform, snapshot_component};
use slint::{
    ComponentHandle, Image, ModelRc, PhysicalSize, Rgba8Pixel, SharedPixelBuffer, SharedString,
    VecModel,
};

slint::include_modules!();

pub mod formatting;

mod analysis;
mod grid_gestures;
mod grid_navigation;
mod grid_pointer;
mod tab_run;
use analysis::{plan_chart, plan_chart_in_range};

mod assets;

mod workbook_io;
#[cfg(test)]
pub(crate) use workbook_io::load_workbook_with_report;
#[cfg(test)]
pub(crate) use workbook_io::workbook_package_bytes;
pub(crate) use workbook_io::{
    blank_sheet, load_sheet, load_workbook, restore_workbook_from_snapshot, save_sheet,
    save_workbook, starter_workbook, template_sheet, LoadedWorkbook,
};

mod xlsx_import;
use xlsx_import::{cancel_pending_xlsx_import, continue_pending_xlsx_import, PendingXlsxImport};
#[cfg(test)]
use xlsx_import::{prepare_startup_import, stage_xlsx_import};

mod object_actions;

mod object_layout;
use object_layout::editor_dimensions_with_preview;
#[cfg(test)]
use object_layout::editor_dimensions_with_width;

mod chart_actions;

mod local_menu;

mod cli;
#[cfg(test)]
use cli::parse_args_from;
use cli::{parse_args, Args};

mod headless;
use headless::render_headless;

mod cell_edit_recovery;
#[cfg(test)]
#[path = "cell_edit_recovery_tests.rs"]
mod cell_edit_recovery_tests;
mod close_operations;
mod evaluation_cache;
mod export_operations;
mod file_operation_completions;
mod legacy_migration;
mod open_operations;
mod recovery_pause;
mod recovery_policy;
mod save_operations;
mod worker_failure;
use open_operations::{
    begin_new_workbook, open_workbook_from_picker, request_replacement_after_dialog,
    start_file_timer_after_show, start_startup_open, OpenOperations, PendingReplacement,
    StartupOpenOptions,
};
mod workbook_worker;

mod palette;
use palette::*;

mod journey;
use journey::*;

mod cell_spill;
mod scroll_projection;
mod template_navigation;
mod window_chrome;

mod actions;
mod clipboard_actions;
mod system_clipboard;
use actions::*;

mod command_dispatch;
mod toolbar_commands;
use command_dispatch::*;

mod cell_actions;
pub(crate) use cell_actions::register_cell_edit_action;

const DEFAULT_SIZE: (u32, u32) = (1280, 800);
pub(crate) const DEFAULT_VISIBLE_COLS: u32 = 8;
pub(crate) const DEFAULT_VISIBLE_ROWS: u32 = 15;
pub(crate) const GRID_ROW_HEIGHT: f32 = DEFAULT_ROW_HEIGHT;
pub(crate) const GRID_COL_WIDTH: f32 = DEFAULT_COL_WIDTH;
const GRID_ROW_HEADER_WIDTH: f32 = 36.0;
const GRID_COLUMN_HEADER_HEIGHT: f32 = 26.0;
const INSPECTOR_WIDTH: f32 = 320.0;
const TABLE_HORIZONTAL_MARGIN: f32 = 0.0;
const SHELL_VERTICAL_CHROME: f32 = 220.0;
const SAVE_FILENAME: &str = "loom-sheets-workbook.loomtable";
const EXPORT_FILENAME: &str = "loom-sheets-export.csv";

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
            let raw_val = cell_value(sheet, values, row, col);
            // General alignment follows the content, as in every mainstream
            // spreadsheet: numbers sit against the right edge, text on the left.
            let numeric = matches!(values.get(&cell), Some(Value::Number(_)))
                || raw_val.trim().parse::<f64>().is_ok();
            let align_code = match sheet.cell_alignment(cell) {
                CellAlignment::General if numeric => 2,
                CellAlignment::General | CellAlignment::Left => 0,
                CellAlignment::Center => 1,
                CellAlignment::Right => 2,
            };
            let display_val = style.format_value_fit(
                &raw_val,
                sheet.col_width(col),
                formatting::effective_font_size(style) as f32,
            );
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
#[cfg(test)]
fn editor_dimensions(
    sheet: &Sheet,
    selected: CellRef,
    fill: Option<(u32, u32)>,
) -> SheetDimensions {
    editor_dimensions_with_width(sheet, selected, fill, GRID_COL_WIDTH)
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

pub(crate) fn dimension_extent(
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

pub(crate) fn dimension_offset(
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
    // Materialize two extra columns and rows past the edge. The scroll offset
    // usually sits partway into the first cell, so an exact-fit window leaves
    // a blank strip on the far side until the next refresh.
    let visible_cols = dimension_visible_count(
        first_col,
        dimensions.cols,
        viewport_width + 2.0 * default_col_width,
        default_col_width,
        col_widths,
    )
    .min(dimensions.cols - first_col);
    let visible_rows = dimension_visible_count(
        first_row,
        dimensions.rows,
        viewport_height + 2.0 * GRID_ROW_HEIGHT,
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
    views: Vec<SheetObjectView>,
}

fn project_sheet_objects(
    sheet: &Sheet,
    geometry: &GridGeometry,
    zoom: f32,
    scroll_x: f32,
    scroll_y: f32,
    preview: Option<(usize, CellRef, u32, u32)>,
    image_for: &dyn Fn(usize, &loom_sheets_core::SheetObject) -> Image,
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
    let mut projected = ProjectedSheetObjects {
        views: Vec::with_capacity(sheet.objects.len()),
    };
    for (index, object) in sheet.objects.iter().enumerate() {
        let (anchor, width, height) = preview
            .filter(|(preview_index, _, _, _)| *preview_index == index)
            .map(|(_, anchor, width, height)| (anchor, width, height))
            .unwrap_or((object.anchor, object.width, object.height));
        let absolute_x = dimension_offset(anchor.col, geometry.default_col_width, &scaled_cols);
        let absolute_y = dimension_offset(anchor.row, GRID_ROW_HEIGHT * zoom, &scaled_rows);
        let details = format!(
                "Anchor {}; size {} by {} document pixels. Tab and Shift+Tab browse objects; M moves one cell; R resizes by 10 pixels; arrow keys preview; Enter commits one undo step; Escape cancels.",
                anchor.to_a1(),
                width,
                height,
            );
        let x = absolute_x + scroll_x;
        let y = absolute_y + scroll_y;
        let rendered_width = object_layout::rendered_object_extent(
            width,
            zoom,
            object_layout::MIN_RENDERED_OBJECT_WIDTH,
        );
        let rendered_height = object_layout::rendered_object_extent(
            height,
            zoom,
            object_layout::MIN_RENDERED_OBJECT_HEIGHT,
        );
        projected.views.push(SheetObjectView {
            kind: object.kind.as_str().into(),
            label: object.label.as_str().into(),
            details: details.into(),
            image: image_for(index, object),
            x,
            y,
            width: rendered_width,
            height: rendered_height,
            fill: object.fill.swatch_index(),
            visible: object_layout::intersects_object_viewport(
                x,
                y,
                rendered_width as f32,
                rendered_height as f32,
                geometry.visible_width,
                geometry.visible_height,
            ),
        });
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
    zoom: f32,
) -> GridGeometry {
    grid_geometry_fit(sheet, dimensions, viewport, GRID_COL_WIDTH, zoom)
}

/// Geometry for a known unscaled default column width. Scrolling reuses the
/// width from the last full projection instead of rescanning the sheet.
fn grid_geometry_fit(
    sheet: &Sheet,
    dimensions: SheetDimensions,
    viewport: SheetViewport,
    fit_col_width: f32,
    zoom: f32,
) -> GridGeometry {
    // Zoom scales rendered pixels uniformly; the persisted model keeps
    // unscaled pixels (see `viewport_from_app`).
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

fn viewport_from_app(
    app: &SheetsApp,
    sheet: &Sheet,
    preview: Option<(usize, CellRef, u32, u32)>,
) -> SheetViewport {
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
    let dimensions = editor_dimensions_with_preview(
        sheet,
        selected,
        window_fill(app, zoom),
        GRID_COL_WIDTH,
        zoom,
        preview,
    );
    let default_col_width = GRID_COL_WIDTH * zoom;
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
    object_actions::cancel_active_gesture(app, state);
    state.mark_content_dirty();
    sync_window_title(app, state);
    let worker_running = state.workbook_worker.borrow().is_some();
    let vals = if worker_running {
        values_for_projection(state)
    } else {
        recalculate_current(state)
    };
    let sheet = state.current.borrow();
    project_sheet_inner(app, &sheet, &vals, true);
    drop(sheet);
    if worker_running {
        app.set_status_left("Calculating…".into());
    }
    if let Err(error) = record_workbook_snapshot(state) {
        app.set_status_left(SharedString::from(format!(
            "Calculation unavailable; workbook update was not queued: {error}"
        )));
        app.set_status_right(SharedString::from(format!(
            "Workbook update unavailable: {error}"
        )));
    }
}

/// Re-project after changing workbook context, such as selecting another tab,
/// without marking its cells as edited. The selected tab is compared against
/// the saved active index by `is_dirty`, and the recovery checkpoint still
/// records the new workbook context.
pub(crate) fn apply_sheet_view_change(app: &SheetsApp, state: &GuiState) {
    if close_operations::reject_admission(app, state) {
        return;
    }
    sync_window_title(app, state);
    let active = *state.active_sheet_index.borrow();
    let worker_running = {
        let worker = state.workbook_worker.borrow();
        if let Some(worker) = worker.as_ref() {
            let revision = state.next_worker_revision();
            if let Err(error) = worker.submit_active(revision, active) {
                state.mark_worker_submission_failure(revision, error.clone());
                app.set_status_right(SharedString::from(format!(
                    "Workbook calculation unavailable: {error}"
                )));
                false
            } else {
                state.last_queued_worker_revision.set(revision);
                true
            }
        } else {
            false
        }
    };
    let vals = values_for_projection(state);
    let sheet = state.current.borrow();
    project_sheet_inner(app, &sheet, &vals, true);
    if let Some(message) = state.unaccepted_worker_revision_message() {
        app.set_status_left(SharedString::from(format!(
            "Calculation unavailable: {message}"
        )));
    } else if worker_running {
        app.set_status_left("Calculating…".into());
    }
}

/// Re-project the live tab with workbook-resolved values, without recording
/// a recovery snapshot (selection/scroll/view-only refreshes).
pub(crate) fn project_current(app: &SheetsApp, state: &GuiState) {
    sync_window_title(app, state);
    let sheet = state.current.borrow();
    let vals = values_for_projection(state);
    project_sheet_inner_with_preview(
        app,
        &sheet,
        &vals,
        true,
        object_actions::preview_geometry(state),
    );
}

/// Re-project without revealing the selection and without snapshotting.
pub(crate) fn project_current_without_reveal(app: &SheetsApp, state: &GuiState) {
    sync_window_title(app, state);
    let sheet = state.current.borrow();
    let vals = values_for_projection(state);
    project_sheet_inner_with_preview(
        app,
        &sheet,
        &vals,
        false,
        object_actions::preview_geometry(state),
    );
}

pub(crate) fn project_sheet(app: &SheetsApp, sheet: &Sheet) {
    let vals = evaluate(sheet);
    project_sheet_inner(app, sheet, &vals, true);
}

pub(crate) fn project_sheet_without_reveal(app: &SheetsApp, sheet: &Sheet) {
    let vals = evaluate(sheet);
    project_sheet_inner(app, sheet, &vals, false);
}

/// Send the full workbook to the worker after a non-cell mutation. The worker
/// evaluates it and records a recovery snapshot away from the UI thread.
pub(crate) fn record_workbook_snapshot(state: &GuiState) -> Result<(), String> {
    if close_operations::blocks_admission(state) {
        return Err("workbook edits are paused while the window is closing".into());
    }
    let worker = state.workbook_worker.borrow();
    let Some(worker) = worker.as_ref() else {
        return Ok(());
    };
    let (sheets, active) = workbook_sheets(state);
    let document_generation = state.open_operations.borrow().document_generation();
    let revision = state.next_worker_revision();
    if let Err(error) = worker.submit_replacement(revision, active, sheets) {
        state.mark_worker_submission_failure(revision, error.clone());
        return Err(error);
    }
    state.last_queued_worker_revision.set(revision);
    worker_failure::mark_full_resync_accepted(state, document_generation, revision);
    state.clear_worker_submission_failure();
    Ok(())
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
fn calculate_current_values(state: &GuiState) -> std::collections::HashMap<CellRef, Value> {
    let (siblings, active) = workbook_sheets(state);
    evaluate_workbook(&siblings)
        .into_iter()
        .nth(active)
        .unwrap_or_default()
}

/// Recalculate after an edit and save the result for later view-only updates.
fn recalculate_current(state: &GuiState) -> Rc<std::collections::HashMap<CellRef, Value>> {
    let active = *state.active_sheet_index.borrow();
    state
        .evaluation_cache
        .borrow_mut()
        .refresh(active, || calculate_current_values(state))
}

/// Return the saved results while the workbook has not changed. Cell
/// selection, scrolling, and window resizing all use this path.
pub(crate) fn evaluate_current(state: &GuiState) -> Rc<std::collections::HashMap<CellRef, Value>> {
    let active = *state.active_sheet_index.borrow();
    state
        .evaluation_cache
        .borrow_mut()
        .get_or_calculate(active, || calculate_current_values(state))
}

fn values_for_projection(state: &GuiState) -> Rc<std::collections::HashMap<CellRef, Value>> {
    if state.workbook_worker.borrow().is_some() {
        let active = *state.active_sheet_index.borrow();
        state.evaluation_cache.borrow_mut().cached_or_empty(active)
    } else {
        evaluate_current(state)
    }
}

fn project_sheet_inner(
    app: &SheetsApp,
    sheet: &Sheet,
    vals: &std::collections::HashMap<CellRef, Value>,
    reveal_selection: bool,
) {
    project_sheet_inner_with_preview(app, sheet, vals, reveal_selection, None);
}

fn project_sheet_inner_with_preview(
    app: &SheetsApp,
    sheet: &Sheet,
    vals: &std::collections::HashMap<CellRef, Value>,
    reveal_selection: bool,
    preview: Option<(usize, CellRef, u32, u32)>,
) {
    let selection = selection_from_app(app);
    let selected = selection.focus;
    let dimensions = sheet.dimensions();
    let zoom = zoom_factor(app);
    let editor_dimensions = editor_dimensions_with_preview(
        sheet,
        selected,
        window_fill(app, zoom),
        GRID_COL_WIDTH,
        zoom,
        preview,
    );
    // Set content extents before touching Flickable offsets.  The two-way
    // viewport binding clamps offsets against these extents, so updating them
    // first preserves a requested tail scroll on a newly loaded sparse sheet.
    app.set_workbook_rows(editor_dimensions.rows as i32);
    app.set_workbook_cols(editor_dimensions.cols as i32);
    let current_scroll_x = (-app.get_grid_scroll_x()).max(0.0);
    let current_scroll_y = (-app.get_grid_scroll_y()).max(0.0);
    let mut viewport = viewport_from_app(app, sheet, preview);
    let projected_before_reveal = viewport;
    if reveal_selection {
        grid_navigation::reveal_selected(app, sheet, &mut viewport, selected);
    }
    app.set_view_row_origin(viewport.first_row as i32);
    app.set_view_col_origin(viewport.first_col as i32);
    app.set_zoom_factor(zoom);
    app.set_zoom_level(SharedString::from(format!(
        "{}%",
        (zoom * 100.0).round() as i32
    )));
    let geometry = grid_geometry(sheet, editor_dimensions, viewport, zoom);
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
        &geometry,
        zoom,
        app.get_grid_scroll_x(),
        app.get_grid_scroll_y(),
        preview,
        &|_, object| load_sheet_object_image(object),
    );

    scroll_projection::apply_grid(app, grid, Some(objects.views), &geometry, zoom);
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
    app.set_status_summary(SharedString::from(format!(
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
    menu_service.sync_command_states(&projection)?;
    local_menu::sync(app, menu_service)
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
}

impl WorkbookUndoState {
    fn capture(state: &GuiState) -> Self {
        Self {
            sheets: state.sheets.borrow().clone(),
            active: *state.active_sheet_index.borrow(),
        }
    }

    fn restore(&self, state: &GuiState) {
        state.advance_object_context();
        *state.sheets.borrow_mut() = self.sheets.clone();
        let active = self.active.min(self.sheets.len().saturating_sub(1));
        *state.active_sheet_index.borrow_mut() = active;
        *state.current.borrow_mut() = self.sheets[active].clone();
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

/// Maximum owned history data retained by one undo or redo stack.
///
/// Entries are full document snapshots for workbook operations and sparse
/// deltas for cell operations. The byte cap is checked after every push so a
/// long editing session cannot retain an unbounded amount of user data even
/// when individual entries are large.
pub(crate) const MAX_HISTORY_BYTES: usize = 8 * 1024 * 1024;

fn sheet_history_bytes(sheet: &Sheet) -> usize {
    let cells = sheet
        .cells
        .values()
        .map(|cell| cell.raw.capacity())
        .sum::<usize>();
    let objects = sheet
        .objects
        .iter()
        .map(|object| {
            object.label.capacity()
                + object.path.capacity()
                + object.embedded.as_ref().map_or(0, Vec::capacity)
                + object.asset.as_ref().map_or(0, String::capacity)
        })
        .sum::<usize>();
    std::mem::size_of_val(sheet)
        + sheet.name.capacity()
        + cells
        + objects
        + sheet.col_widths.len() * std::mem::size_of::<(u32, f32)>()
        + sheet.row_heights.len() * std::mem::size_of::<(u32, f32)>()
        + sheet.alignments.len() * std::mem::size_of::<(CellRef, CellAlignment)>()
        + sheet.styles.len() * std::mem::size_of::<(CellRef, CellStyle)>()
}

fn workbook_state_bytes(state: &WorkbookUndoState) -> usize {
    std::mem::size_of_val(state) + state.sheets.iter().map(sheet_history_bytes).sum::<usize>()
}

fn transaction_bytes(tx: &SheetTransaction) -> usize {
    match tx {
        SheetTransaction::Range(edit) => edit.memory_bytes(),
        SheetTransaction::Batch(edits) => edits.iter().map(RangeEdit::memory_bytes).sum(),
        SheetTransaction::Alignment { before, .. } => {
            std::mem::size_of_val(tx)
                + before.capacity() * std::mem::size_of::<(CellRef, CellAlignment)>()
        }
        SheetTransaction::Style { before, after } => {
            std::mem::size_of_val(tx)
                + (before.capacity() + after.capacity())
                    * std::mem::size_of::<(CellRef, CellStyle)>()
        }
        SheetTransaction::Snapshot { before, after } => {
            std::mem::size_of_val(tx) + sheet_history_bytes(before) + sheet_history_bytes(after)
        }
        SheetTransaction::Workbook { before, after } => {
            std::mem::size_of_val(tx) + workbook_state_bytes(before) + workbook_state_bytes(after)
        }
    }
}

pub(crate) fn history_bytes(stack: &[SheetTransaction]) -> usize {
    stack.iter().map(transaction_bytes).sum()
}

/// Push a transaction, evicting the oldest entry past the bound.
pub(crate) fn push_history(stack: &mut Vec<SheetTransaction>, tx: SheetTransaction) {
    stack.push(tx);
    while stack.len() > MAX_HISTORY_ENTRIES || history_bytes(stack) > MAX_HISTORY_BYTES {
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
    state.advance_object_context();
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
    // Workbook snapshots contain document state only. Histories stay in the
    // live per-tab slots so a tab rename cannot copy all previous workbook
    // transactions into the next transaction.
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
    // The popped workbook transaction was held by the current tab. Preserve
    // that tab's post-pop stacks before moving to the snapshot's active tab.
    // The snapshot deliberately has no history fields, so this is the only
    // place where per-tab history is transferred during workbook undo/redo.
    let current_active = *state.active_sheet_index.borrow();
    let current_history = (
        state.undo_stack.borrow().clone(),
        state.redo_stack.borrow().clone(),
    );
    {
        let mut histories = state.sheet_histories.borrow_mut();
        if histories.len() <= current_active {
            histories.resize_with(current_active + 1, || (Vec::new(), Vec::new()));
        }
        histories[current_active] = current_history;
    }
    snapshot.restore(state);
    let active = *state.active_sheet_index.borrow();
    let (undo, redo) = state
        .sheet_histories
        .borrow()
        .get(active)
        .cloned()
        .unwrap_or_default();
    *state.undo_stack.borrow_mut() = undo;
    *state.redo_stack.borrow_mut() = redo;
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

/// Where a key move of `delta` takes the active cell, counting a page as the
/// rows that fit in the window.
fn moved_focus(app: &SheetsApp, sheet: &Sheet, from: CellRef, delta: (i32, i32)) -> CellRef {
    let page = window_fill(app, zoom_factor(app)).map_or(DEFAULT_VISIBLE_ROWS, |(_, rows)| rows);
    grid_navigation::destination(sheet, from, delta, page)
}

fn navigate_selection(app: &SheetsApp, sheet: &Sheet, row_delta: i32, col_delta: i32) {
    let selection = selection_from_app(app);
    let next = moved_focus(app, sheet, selection.focus, (row_delta, col_delta));
    update_selection(app, sheet, &evaluate(sheet), next);
}

fn extend_selection(app: &SheetsApp, sheet: &Sheet, row_delta: i32, col_delta: i32) {
    let selection = selection_from_app(app);
    let next = moved_focus(app, sheet, selection.focus, (row_delta, col_delta));
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
    let width = width as f32 / app.get_template_text_scale().max(1.0);
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
    app.set_overflow_toolbar(state.overflow);
    // Keep Format available at every width. The UI moves it into a drawer
    // when the window is compact instead of disabling the action.
    app.set_inspector_available(true);
}

/// Size the headless projection like the native canvas. Snapshot rendering
/// skips the native resize event, so compute the viewport before projecting.
pub(crate) fn apply_headless_viewport_size(app: &SheetsApp, width: u32, height: u32) {
    let policy = ResponsivePolicy::get(app);
    let effective_width = width as f32 / app.get_template_text_scale().max(1.0);
    let inspector_width =
        if app.get_show_inspector() && effective_width >= policy.get_priority_1_icon_only_below() {
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

pub(crate) struct GuiState {
    pub(crate) current: RefCell<Sheet>,
    pub(crate) sheets: RefCell<Vec<Sheet>>,
    pub(crate) active_sheet_index: RefCell<usize>,
    evaluation_cache: RefCell<evaluation_cache::EvaluationCache>,
    pub(crate) workbook_worker: RefCell<Option<workbook_worker::WorkbookWorker>>,
    pub(crate) worker_revision: Cell<u64>,
    /// Highest workbook revision whose worker submission was accepted.
    pub(crate) last_queued_worker_revision: Cell<u64>,
    /// Excel-style Tab/Enter run tracking.
    pub(crate) tab_run: tab_run::TabRun,
    /// A failed worker submission leaves the UI model ahead of canonical worker
    /// state. Only a later accepted full replacement can clear this marker.
    worker_submission_failure: RefCell<Option<(u64, String)>>,
    /// An accepted worker input can still fail to apply. Preserve that failure
    /// until an accepted full replacement at or beyond its revision completes.
    pub(crate) worker_input_failure: RefCell<Option<worker_failure::WorkerInputFailure>>,
    /// Latest full replacement accepted by the worker, scoped to its document.
    pub(crate) worker_full_resync_revision: Cell<Option<(u64, u64)>>,
    /// Highest worker result removed from the result mailbox, whether or not it
    /// was still current enough to project into the UI.
    pub(crate) applied_worker_result_revision: Cell<u64>,
    pub(crate) close_state: Cell<close_operations::CloseState>,
    pub(crate) deferred_close_recovery_error: RefCell<Option<(u64, String)>>,
    pub(crate) worker_saved_baseline_generation: Cell<Option<u64>>,
    pub(crate) worker_saved_baseline_revision: Cell<Option<u64>>,
    pub(crate) pending_cell_commit: Cell<Option<(u64, CellRef)>>,
    /// Revision of a Retry Recovery checkpoint whose outcome is still unreported.
    pub(crate) recovery_retry_revision: Cell<Option<u64>>,
    pub(crate) save_path: RefCell<Option<PathBuf>>,
    /// Workbook state from the last completed save/open/new operation.
    /// Comparing document content, rather than undo depth, means undoing back
    /// to the saved state clears the dirty flag.
    pub(crate) last_saved: RefCell<Option<(Vec<Sheet>, usize)>>,
    /// Set by document edits so title updates do not serialize the workbook.
    /// Undo and redo recompute exact equality against `last_saved`.
    dirty_content: Cell<bool>,
    pub(crate) pending_replacement: Cell<Option<PendingReplacement>>,
    pub(crate) pending_replacement_token: Cell<u64>,
    pub(crate) open_operations: RefCell<OpenOperations>,
    pub(crate) save_operations: RefCell<save_operations::SaveOperations>,
    pub(crate) export_operations: RefCell<export_operations::ExportOperations>,
    pub(crate) file_operation_completions:
        RefCell<file_operation_completions::FileOperationCompletions>,
    pending_xlsx_import: RefCell<Option<PendingXlsxImport>>,
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
    pub(crate) object_context_generation: Cell<u64>,
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
            evaluation_cache: RefCell::new(evaluation_cache::EvaluationCache::default()),
            workbook_worker: RefCell::new(None),
            worker_revision: Cell::new(0),
            last_queued_worker_revision: Cell::new(0),
            tab_run: tab_run::TabRun::default(),
            worker_submission_failure: RefCell::new(None),
            worker_input_failure: RefCell::new(None),
            worker_full_resync_revision: Cell::new(None),
            applied_worker_result_revision: Cell::new(0),
            close_state: Cell::new(close_operations::CloseState::Idle),
            deferred_close_recovery_error: RefCell::new(None),
            worker_saved_baseline_generation: Cell::new(None),
            worker_saved_baseline_revision: Cell::new(None),
            pending_cell_commit: Cell::new(None),
            recovery_retry_revision: Cell::new(None),
            save_path: RefCell::new(path),
            last_saved: RefCell::new(None),
            dirty_content: Cell::new(false),
            pending_replacement: Cell::new(None),
            pending_replacement_token: Cell::new(0),
            open_operations: RefCell::new(OpenOperations::default()),
            save_operations: RefCell::new(save_operations::SaveOperations::default()),
            export_operations: RefCell::new(export_operations::ExportOperations::default()),
            file_operation_completions: RefCell::new(
                file_operation_completions::FileOperationCompletions::default(),
            ),
            pending_xlsx_import: RefCell::new(None),
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
            object_context_generation: Cell::new(0),
        }
    }

    pub(crate) fn advance_object_context(&self) {
        self.object_context_generation.set(
            self.object_context_generation
                .get()
                .checked_add(1)
                .expect("Sheets object context generation exhausted"),
        );
    }

    /// Install a fully loaded workbook (all tabs + active index) into a fresh
    /// single-sheet state. Histories start empty: loading is not an undoable
    /// edit, matching open/new/template behavior.
    pub(crate) fn install_workbook(&self, sheets: Vec<Sheet>, active: usize) {
        self.advance_object_context();
        self.object_gesture.borrow_mut().take();
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

    pub(crate) fn mark_saved(&self) {
        *self.last_saved.borrow_mut() = Some(workbook_sheets(self));
        self.dirty_content.set(false);
    }

    /// Recovered contents exist only in the recovery store, so they get no saved
    /// baseline: every later dirty recheck keeps the workbook unsaved.
    pub(crate) fn set_startup_baseline(&self, recovered_unsaved: bool) {
        if recovered_unsaved {
            *self.last_saved.borrow_mut() = None;
            self.mark_content_dirty();
        } else {
            self.mark_saved();
        }
    }

    pub(crate) fn clear_dirty(&self) {
        self.dirty_content.set(false);
    }

    pub(crate) fn mark_content_dirty(&self) {
        self.dirty_content.set(true);
    }

    pub(crate) fn advance_pending_replacement_token(&self) -> u64 {
        let token = self
            .pending_replacement_token
            .get()
            .checked_add(1)
            .expect("Sheets pending replacement token exhausted");
        self.pending_replacement_token.set(token);
        token
    }

    pub(crate) fn next_worker_revision(&self) -> u64 {
        let revision = self
            .worker_revision
            .get()
            .checked_add(1)
            .expect("Sheets workbook revision exhausted");
        self.worker_revision.set(revision);
        revision
    }

    pub(crate) fn mark_worker_submission_failure(&self, revision: u64, error: String) {
        *self.worker_submission_failure.borrow_mut() = Some((revision, error));
    }

    pub(crate) fn clear_worker_submission_failure(&self) {
        self.worker_submission_failure.borrow_mut().take();
    }

    /// Recheck full content after undo/redo, where the edit marker alone would
    /// stay set even after returning exactly to the last saved workbook.
    pub(crate) fn recompute_dirty_from_saved(&self) {
        let Some(saved) = self.last_saved.borrow().clone() else {
            self.dirty_content.set(true);
            return;
        };
        let current = workbook_sheets(self);
        self.dirty_content
            .set(workbook_to_json(&current.0, current.1) != workbook_to_json(&saved.0, saved.1));
    }

    pub(crate) fn is_dirty(&self) -> bool {
        if worker_failure::has_current_failure(self) {
            return true;
        }
        let saved = self.last_saved.borrow();
        let Some((_, saved_active)) = saved.as_ref() else {
            return true;
        };
        self.dirty_content.get() || *self.active_sheet_index.borrow() != *saved_active
    }
}

fn initial_directory(path: Option<&Path>) -> Option<PathBuf> {
    path.and_then(Path::parent)
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(Path::to_path_buf)
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

pub(crate) fn wire_export_callbacks(app: &SheetsApp, state: &Rc<GuiState>) {
    {
        let state = Rc::clone(state);
        let app_ref = app.as_weak();
        app.on_export_csv(move || {
            if let Some(app) = app_ref.upgrade() {
                export_operations::export_with_picker(
                    &app,
                    &state,
                    export_operations::ExportFormat::Csv,
                );
            }
        });
    }
    {
        let state = Rc::clone(state);
        let app_ref = app.as_weak();
        app.on_export_xlsx(move || {
            if let Some(app) = app_ref.upgrade() {
                export_operations::export_with_picker(
                    &app,
                    &state,
                    export_operations::ExportFormat::Xlsx,
                );
            }
        });
    }
}

/// The one document identity used by the window title, the close prompt, and
/// the replacement prompt. A saved file is named by its filename; an unsaved
/// document is named by its sheet so a created template never contradicts the
/// tab and title the user is looking at.
pub(crate) fn workbook_identity_name(save_path: Option<&Path>, sheet_name: &str) -> String {
    save_path
        .and_then(Path::file_name)
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| match sheet_name {
            "" | "Sheet1" | "Sheet 1" => "Untitled".to_string(),
            name => name.to_string(),
        })
}

pub(crate) fn workbook_display_name(state: &GuiState) -> String {
    workbook_identity_name(
        state.save_path.borrow().as_deref(),
        state.current.borrow().name.as_str(),
    )
}

pub(crate) fn workbook_window_title(
    save_path: Option<&Path>,
    sheet_name: &str,
    dirty: bool,
) -> String {
    let title = workbook_identity_name(save_path, sheet_name);
    if dirty {
        format!("{title} *")
    } else {
        title
    }
}

pub(crate) fn sync_window_title(app: &SheetsApp, state: &GuiState) {
    let mut title = workbook_window_title(
        state.save_path.borrow().as_deref(),
        state.current.borrow().name.as_str(),
        state.is_dirty(),
    );
    if recovery_pause::paused_reason(state).is_some() {
        title.push_str(recovery_pause::TITLE_SUFFIX);
    }
    app.set_window_title(SharedString::from(title));
}

/// Apply a worker result only while it still represents the current workbook
/// revision and active tab. Old results may finish while a newer edit is
/// already queued, so they must never replace newer visible values.
pub(crate) fn apply_workbook_worker_result(
    app: &SheetsApp,
    state: &GuiState,
    result: workbook_worker::WorkbookResult,
) -> bool {
    state.applied_worker_result_revision.set(
        state
            .applied_worker_result_revision
            .get()
            .max(result.revision),
    );
    let active = *state.active_sheet_index.borrow();
    if result.revision != state.worker_revision.get() || result.active_sheet != active {
        return false;
    }
    let generation = state.open_operations.borrow().document_generation();
    let previous_input_failure_status = worker_failure::status_message(state);
    let mut input_failure_resolved = false;
    if let Some(error) = result.input_error.as_ref() {
        worker_failure::record_input_failure(state, generation, result.revision, error.clone());
    } else {
        input_failure_resolved =
            worker_failure::clear_after_successful_resync(state, generation, result.revision);
    }
    let pending_cell = state.pending_cell_commit.get();
    let cell_feedback = pending_cell.and_then(|(revision, cell)| {
        (revision == result.revision).then(|| match result.values.get(&cell) {
            Some(Value::Error(error)) => {
                format!("Formula error in {}: #{}", cell.to_a1(), error.code())
            }
            _ => format!("Cell {} updated", cell.to_a1()),
        })
    });
    let superseded_pending_cell =
        pending_cell.is_some_and(|(revision, _)| revision < result.revision);
    if pending_cell.is_some_and(|(revision, _)| revision <= result.revision) {
        state.pending_cell_commit.set(None);
    }
    let values = state
        .evaluation_cache
        .borrow_mut()
        .set_values(result.active_sheet, result.values);
    let sheet = state.current.borrow();
    let formula_draft = app.get_formula_edit_buffer();
    project_sheet_inner_with_preview(
        app,
        &sheet,
        &values,
        false,
        object_actions::preview_geometry(state),
    );
    app.set_formula_edit_buffer(formula_draft);
    drop(sheet);

    match result.recovery_error {
        // A latched failure is reported (and cleared) by recovery_pause::sync.
        Some(_) if recovery_pause::paused_reason(state).is_some() => {}
        Some(error) => app.set_status_right(SharedString::from(format!(
            "Recovery checkpoint unavailable: {error}"
        ))),
        None => {}
    }
    if let Some(error) = result.input_error.as_ref() {
        app.set_status_right(SharedString::from(format!(
            "Workbook update failed: {error}"
        )));
    }
    if state.worker_saved_baseline_generation.get() == Some(generation)
        && state
            .worker_saved_baseline_revision
            .get()
            .is_some_and(|revision| result.revision > revision)
    {
        state.dirty_content.set(result.dirty);
    }
    if worker_failure::has_current_failure(state) {
        state.dirty_content.set(true);
    }
    if let Some(feedback) = cell_feedback {
        app.set_formula_feedback(SharedString::from(feedback));
    } else if superseded_pending_cell && app.get_formula_feedback().as_str() == "Calculating…" {
        app.set_formula_feedback("".into());
    }
    if let Some(failure) = worker_failure::status_message(state) {
        app.set_status_left(SharedString::from(failure));
    } else if input_failure_resolved {
        let status = app.get_status_left().to_string();
        if let Some(previous_failure) =
            previous_input_failure_status.filter(|failure| status.contains(failure))
        {
            let remaining = status.replace(&previous_failure, "");
            let remaining = remaining.trim().trim_matches('·').trim();
            app.set_status_left(if remaining.is_empty() {
                "Ready".into()
            } else {
                SharedString::from(remaining)
            });
        }
    } else if app.get_status_left().as_str() == "Calculating…" {
        app.set_status_left("Ready".into());
    }
    true
}

fn start_workbook_worker_timer(
    app: &SheetsApp,
    state: &Rc<GuiState>,
    menu_service: &Arc<NativeMenuBar>,
) -> slint::Timer {
    let timer = slint::Timer::default();
    let app_ref = app.as_weak();
    let state = Rc::clone(state);
    let menu_service = Arc::clone(menu_service);
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(16),
        move || {
            if let Some(app) = app_ref.upgrade() {
                close_operations::process_worker_tick(&app, &state, &menu_service);
            }
        },
    );
    timer
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

pub(crate) fn save_current_sheet(
    app: &SheetsApp,
    state: &GuiState,
    force_picker: bool,
) -> Result<bool, String> {
    object_actions::cancel_active_gesture(app, state);
    if close_operations::reject_admission(app, state) {
        return Err("workbook file operations are paused while the window is closing".into());
    }
    if state.save_operations.borrow().is_active() {
        return Err("a Save operation is already in progress".into());
    }
    if let Some(message) = state.unaccepted_worker_revision_message() {
        return Err(format!("Cannot save because {message}"));
    }
    let draft = app.get_formula_edit_buffer();
    if draft != app.get_selection_formula() {
        app.invoke_commit_selected_cell(draft);
    }
    if let Some(message) = state.unaccepted_worker_revision_message() {
        return Err(format!("Cannot save because {message}"));
    }

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
    let document_generation = state.open_operations.borrow().document_generation();
    let target_revision = state.last_queued_worker_revision.get();
    let pending_replacement_token = (app.get_save_changes_open()
        && state.pending_replacement.get().is_some())
    .then(|| state.pending_replacement_token.get());
    let operation = state.save_operations.borrow_mut().begin_operation(
        document_generation,
        target_revision,
        pending_replacement_token,
    )?;
    let queue_result = match state.workbook_worker.borrow().as_ref() {
        Some(worker) => worker.queue_save(
            operation.operation_id,
            operation.document_generation,
            operation.target_revision,
            operation.pending_replacement_token,
            path,
        ),
        None => Err("workbook worker is unavailable".to_string()),
    };
    if let Err(error) = queue_result {
        state.save_operations.borrow_mut().clear(operation);
        return Err(error);
    }
    app.set_status_left("Saving…".into());
    Ok(true)
}

fn save_error_feedback(action: &str, error: &str) -> String {
    let normalized = error.to_ascii_lowercase();
    let message = if normalized.contains("read-only") || normalized.contains("read only") {
        "destination is read-only"
    } else if normalized.contains("permission denied") {
        "permission denied"
    } else {
        return format!("{action}: {error}");
    };
    format!("{action}: {message}")
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
                object_actions::cancel_active_gesture(&app, &state);
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
                    state.recompute_dirty_from_saved();
                    sync_window_title(&app, &state);
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
                object_actions::cancel_active_gesture(&app, &state);
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
                    state.recompute_dirty_from_saved();
                    sync_window_title(&app, &state);
                    push_history(&mut state.undo_stack.borrow_mut(), edit);
                }
            }
        });
    }
}

/// Picks the workbook a session starts with. A recovered workbook has no save
/// path: its contents exist only in the recovery store, so the caller must show
/// it as unsaved. The bool is true exactly when the workbook was recovered.
fn startup_workbook(recovered: Option<WorkbookFile>, example: bool) -> (WorkbookFile, bool) {
    match recovered {
        Some(file) => (file, true),
        None => (
            WorkbookFile {
                sheets: vec![if example {
                    starter_workbook()
                } else {
                    blank_sheet()
                }],
                active: 0,
            },
            false,
        ),
    }
}

/// Shift+arrow extends the selected range.
fn wire_selection_extension(app: &SheetsApp, state: &Rc<GuiState>) {
    let state = state.clone();
    let app_ref = app.as_weak();
    app.on_extend_selection(move |row_delta, col_delta| {
        if let Some(app) = app_ref.upgrade() {
            extend_selection(&app, &state.current.borrow(), row_delta, col_delta);
            project_current(&app, &state);
        }
    });
}

fn run_gui_with_dialogs(args: &Args, dialogs: Rc<dyn FileDialogService>) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let backend = i_slint_backend_winit::Backend::builder()
            .with_default_menu_bar(false)
            .build()
            .map_err(|error| error.to_string())?;
        slint::platform::set_platform(Box::new(backend)).map_err(|error| error.to_string())?;
    }

    let app = SheetsApp::new().map_err(|e| e.to_string())?;
    app.set_local_menu_visible(!cfg!(target_os = "macos"));
    window_chrome::install(&app);
    configure_direction(&app, args.rtl);
    apply_theme(&app, &args.theme);
    app.set_template_text_scale(args.text_scale);
    app.window()
        .set_size(PhysicalSize::new(args.size.0, args.size.1));
    apply_layout_breakpoints(&app, args.size.0);
    toolbar_commands::start_with_inspector_open(&app);
    toolbar_commands::wire(&app);

    let save_operations = save_operations::SaveOperations::default();
    let save_completion_sender = save_operations.sender();
    let export_operations = export_operations::ExportOperations::default();
    let export_completion_sender = export_operations.sender();
    let (worker, startup) = workbook_worker::WorkbookWorker::start(
        "org.loom.sheets",
        "loom.sheets/1",
        save_completion_sender,
        export_completion_sender,
    )?;
    let startup_recovery_error = startup.recovery_error.clone();
    let (fallback, recovered_unsaved) = startup_workbook(
        startup
            .restored_payload
            .as_deref()
            .and_then(restore_workbook_from_snapshot),
        args.example,
    );
    let startup_open = args.open.as_ref().map(PathBuf::from);
    let mut initial = fallback;
    if initial.sheets.is_empty() {
        initial.sheets.push(blank_sheet());
    }
    initial.active = initial.active.min(initial.sheets.len() - 1);
    if startup_open.is_none() {
        let initial_sheet = &mut initial.sheets[initial.active];
        if args.objects {
            object_actions::seed_demo_objects(initial_sheet);
        }
        if args.chart {
            let chart = if initial_sheet.name == "Example Budget" {
                plan_chart_in_range(initial_sheet, 0, 1, 1, 3).ok()
            } else {
                plan_chart(initial_sheet, 0, 1).ok()
            };
            if let Some(chart) = chart {
                initial_sheet.chart = Some(chart);
            }
        }
    }
    let workbook_filter = FileFilter::new("Loom Sheets workbook", ["loomtable"])
        .map_err(|error| error.to_string())?;
    let import_filter =
        FileFilter::new("Comma-separated values", ["csv"]).map_err(|error| error.to_string())?;
    let csv_filter = import_filter.clone();
    let xlsx_filter =
        FileFilter::new("Excel Spreadsheet", ["xlsx"]).map_err(|error| error.to_string())?;
    let state = Rc::new(GuiState::new(
        blank_sheet(),
        None,
        dialogs,
        workbook_filter,
        import_filter,
        csv_filter,
        xlsx_filter,
    ));
    *state.save_operations.borrow_mut() = save_operations;
    *state.export_operations.borrow_mut() = export_operations;
    let initial_revision = state.next_worker_revision();
    let initial_model = if startup_open.is_some() {
        worker.initialize_workbook_without_recovery(
            initial_revision,
            initial.active,
            initial.sheets,
        )?
    } else {
        worker.initialize_workbook(initial_revision, initial.active, initial.sheets)?
    };
    state.last_queued_worker_revision.set(initial_revision);
    let initial_generation = state.open_operations.borrow().document_generation();
    worker_failure::mark_full_resync_accepted(&state, initial_generation, initial_revision);
    state.install_workbook(initial_model.sheets, initial_model.active_sheet);
    state.set_startup_baseline(recovered_unsaved && startup_open.is_none());
    *state.workbook_worker.borrow_mut() = Some(worker);
    if args.objects && startup_open.is_none() {
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
                if !request_replacement_after_dialog(&app, &state, PendingReplacement::NewWorkbook)
                {
                    begin_new_workbook(&app, &state, &menu_service);
                }
            }
        });
    }
    register_cell_edit_action(&app, &state, &menu_service);
    recovery_pause::wire(&app, &state);
    {
        let app_ref = app.as_weak();
        let state = state.clone();
        app.on_cancel_selected_cell(move || {
            if let Some(app) = app_ref.upgrade() {
                state.tab_run.reset();
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
                if !request_replacement_after_dialog(&app, &state, PendingReplacement::OpenWorkbook)
                {
                    open_workbook_from_picker(&app, &state, &menu_service, None);
                }
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_xlsx_import_continue(move || {
            if let Some(app) = app_ref.upgrade() {
                continue_pending_xlsx_import(&app, &state, &menu_service);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_xlsx_import_cancel(move || {
            if let Some(app) = app_ref.upgrade() {
                cancel_pending_xlsx_import(&app, &state);
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_save_sheet(move || {
            if let Some(app) = app_ref.upgrade() {
                if let Err(error) = save_current_sheet(&app, &state, false) {
                    app.set_status_left(SharedString::from(save_error_feedback(
                        "Save failed",
                        &error,
                    )));
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
                    app.set_status_left(SharedString::from(save_error_feedback(
                        "Save As failed",
                        &error,
                    )));
                }
            }
        });
    }
    close_operations::wire_save_changes_callbacks(&app, &state, &menu_service);
    wire_export_callbacks(&app, &state);
    close_operations::wire_window_close_handler(&app, &state);
    register_history_actions(&app, &state, &menu_service);
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_cell_clicked(move |r, c| {
            if grid_pointer::take_swallowed_click() {
                return;
            }
            if let Some(app) = app_ref.upgrade() {
                select_cell(&app, &state.current.borrow(), r, c);
                project_current(&app, &state);
            }
        });
    }
    tab_run::register_navigation(&app, &state);
    wire_selection_extension(&app, &state);
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
                scroll_projection::project_scroll(&app, &state);
                scroll_projection::settle_after_scroll(&app_ref, &state);
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
                    let next = !app.get_show_inspector();
                    app.set_inspector_preference(next);
                    app.set_show_inspector(next);
                    sync_menu_state(&menu_service, &app, &state);
                }
            }
        });
    }
    // Template-chooser callbacks live in `register_sheet_actions` (actions.rs)
    // alongside every other sheet callback; see `create_template_workbook`.

    let menu_bar = local_menu::sheets_menu_bar();
    menu_service
        .install_menu_bar(&menu_bar)
        .map_err(|error| error.to_string())?;
    let app_ref = app.as_weak();
    menu_service
        .register_action_sink(Arc::new(move |action: CommandAction| {
            schedule_menu_action(&app_ref, action)
        }))
        .map_err(|error| error.to_string())?;

    local_menu::wire_action(&app, menu_service.clone());

    register_sheet_actions(&app, &state, &menu_service);
    object_actions::register_object_actions(&app, &state, &menu_service);
    if let Some(zoom) = args.zoom {
        app.set_zoom_factor(zoom);
    }
    if args.objects && startup_open.is_none() {
        app.set_selected_object(0);
    }
    sync_sheet_tabs(&app, &state);
    sync_menu_state_result(&menu_service, &app, &state).map_err(|error| error.to_string())?;
    wire_palette(&app);
    if args.chart && startup_open.is_none() && state.current.borrow().chart.is_some() {
        app.set_chart_visible(true);
        sync_chart_to_app(&app, &state.current.borrow());
    }
    if args.template_chooser && startup_open.is_none() {
        app.set_template_chooser_open(true);
    }
    project_current(&app, &state);
    app.set_status_left("Calculating…".into());
    if let Some(error) = startup_recovery_error {
        app.set_status_right(SharedString::from(format!(
            "Recovery journal unavailable: {error}"
        )));
    }
    state.recompute_dirty_from_saved();
    sync_window_title(&app, &state);
    app.show().map_err(|e| e.to_string())?;
    let _worker_completion_timer = start_workbook_worker_timer(&app, &state, &menu_service);
    let _open_completion_timer = start_file_timer_after_show(&app, &state, &menu_service);
    if let Some(path) = startup_open {
        start_startup_open(
            &app,
            &state,
            path,
            StartupOpenOptions::new(args.objects, args.chart, args.template_chooser),
        );
    }
    // A visible selection is not enough to receive keyboard input. Focus the
    // initially visible view after the native window is shown; winit may
    // replace the focus item during presentation, so doing this before `show`
    // is not durable.
    if app.get_xlsx_import_warning_open() {
        app.invoke_focus_xlsx_import_warning();
    } else if app.get_template_chooser_open() {
        app.invoke_focus_template_chooser();
    } else {
        app.invoke_focus_grid();
    }
    if args.objects && args.open.is_none() {
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

#[cfg(test)]
#[path = "perf_tests.rs"]
mod perf_tests;

#[cfg(test)]
#[path = "frame_bench_tests.rs"]
mod frame_bench_tests;
