//! Sparse grid viewport projection, dimension-offset traversal, and
//! renderable slice construction for `SheetsApp`.

use std::collections::{BTreeMap, HashMap};

use loom_sheets_core::{CellAlignment, CellRef, Sheet, SheetDimensions, SheetViewport, Value};

use crate::{cell_value, formatting, GRID_COL_WIDTH, GRID_ROW_HEIGHT};

/// The small, renderable slice of a sparse worksheet shown by the Slint grid.
/// `rows` and `cols` store the materialized worksheet indices for each slot,
/// while cell arrays use row-major slot order (`r * cols.len() + c`);
/// consumers must use `rows` and `cols` to recover worksheet coordinates
/// rather than calculating them from viewport offsets.
pub(crate) struct ProjectedSheetGrid {
    pub(crate) rows: Vec<i32>,
    pub(crate) cols: Vec<i32>,
    pub(crate) column_headers: Vec<String>,
    pub(crate) row_headers: Vec<String>,
    pub(crate) cells: Vec<String>,
    pub(crate) cell_labels: Vec<String>,
    pub(crate) cell_alignments: Vec<i32>,
    pub(crate) cell_bolds: Vec<bool>,
    pub(crate) cell_italics: Vec<bool>,
    pub(crate) cell_underlines: Vec<bool>,
    pub(crate) cell_borders: Vec<bool>,
    pub(crate) cell_fills: Vec<i32>,
    pub(crate) cell_font_sizes: Vec<i32>,
    /// `0xRRGGBB` per cell, `-1` for the paper's ink.
    pub(crate) cell_text_colors: Vec<i32>,
    pub(crate) cell_wraps: Vec<bool>,
}

/// What a screen reader announces for a worksheet cell: its address, then its
/// value and, for a formula, the formula. An empty cell is announced as empty.
pub(crate) fn accessible_cell_label(cell: CellRef, raw: Option<&str>, value: &str) -> String {
    let address = cell.to_a1();
    match raw.filter(|raw| !raw.is_empty()) {
        None => format!("{address}, empty"),
        Some(raw) if raw.starts_with('=') => format!("{address}, value {value}, formula {raw}"),
        Some(_) => format!("{address}, value {value}"),
    }
}

/// The non-hidden worksheet rows inside `viewport`'s row span.
/// Returns an empty vector when every row in the span is hidden so hidden rows
/// never enter the materialized row, cell, label, or height models.
pub(crate) fn materialized_rows(sheet: &Sheet, viewport: SheetViewport) -> Vec<u32> {
    let start = viewport.first_row;
    let end = start.saturating_add(viewport.visible_rows.max(1));
    let mut rows = Vec::new();
    let mut cursor = start;
    for &hidden in sheet.hidden_rows.range(start..end) {
        if hidden > cursor {
            rows.extend(cursor..hidden);
        }
        cursor = hidden.saturating_add(1);
    }
    if cursor < end {
        rows.extend(cursor..end);
    }
    rows
}

/// The worksheet columns inside `viewport`'s column span.
pub(crate) fn materialized_cols(viewport: SheetViewport) -> Vec<u32> {
    let start = viewport.first_col;
    let end = start.saturating_add(viewport.visible_cols.max(1));
    (start..end).collect()
}

pub(crate) fn project_sheet_grid_with_values(
    sheet: &Sheet,
    values: &HashMap<CellRef, Value>,
    viewport: SheetViewport,
) -> ProjectedSheetGrid {
    let mat_rows = materialized_rows(sheet, viewport);
    let mat_cols = materialized_cols(viewport);
    let rows: Vec<i32> = mat_rows.iter().map(|&row| row as i32).collect();
    let cols: Vec<i32> = mat_cols.iter().map(|&col| col as i32).collect();
    let column_headers = mat_cols
        .iter()
        .map(|&column| {
            CellRef {
                row: 0,
                col: column,
            }
            .to_a1()
            .trim_end_matches('1')
            .to_string()
        })
        .collect();
    let row_headers = mat_rows.iter().map(|&row| (row + 1).to_string()).collect();
    let cap = mat_rows.len() * mat_cols.len();
    let mut cells = Vec::with_capacity(cap);
    let mut cell_labels = Vec::with_capacity(cap);
    let mut cell_alignments = Vec::with_capacity(cap);
    let mut cell_bolds = Vec::with_capacity(cap);
    let mut cell_italics = Vec::with_capacity(cap);
    let mut cell_underlines = Vec::with_capacity(cap);
    let mut cell_borders = Vec::with_capacity(cap);
    let mut cell_fills = Vec::with_capacity(cap);
    let mut cell_font_sizes = Vec::with_capacity(cap);
    let mut cell_text_colors = Vec::with_capacity(cap);
    let mut cell_wraps = Vec::with_capacity(cap);
    for &row in &mat_rows {
        for &col in &mat_cols {
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
            let formatted_val = style.format_value(&raw_val);
            let display_val = style.format_value_fit(
                &raw_val,
                sheet.col_width(col),
                formatting::effective_font_size(style) as f32,
            );
            let effective = sheet.effective_style(cell, &formatted_val);
            cell_labels.push(accessible_cell_label(cell, sheet.raw(cell), &raw_val));
            cells.push(display_val);
            cell_alignments.push(align_code);
            cell_bolds.push(effective.bold);
            cell_italics.push(effective.italic);
            cell_underlines.push(effective.underline);
            cell_borders.push(effective.borders.any());
            cell_fills.push(effective.fill.swatch_index());
            cell_font_sizes.push(formatting::effective_font_size(effective) as i32);
            cell_text_colors.push(
                effective
                    .text_color
                    .map_or(-1, |rgb| (rgb & 0x00FF_FFFF) as i32),
            );
            cell_wraps.push(effective.wrap);
        }
    }

    ProjectedSheetGrid {
        rows,
        cols,
        column_headers,
        row_headers,
        cells,
        cell_labels,
        cell_alignments,
        cell_bolds,
        cell_italics,
        cell_underlines,
        cell_borders,
        cell_fills,
        cell_font_sizes,
        cell_text_colors,
        cell_wraps,
    }
}

#[cfg(test)]
pub(crate) fn project_sheet_grid(sheet: &Sheet, viewport: SheetViewport) -> ProjectedSheetGrid {
    let values = loom_sheets_core::evaluate(sheet);
    project_sheet_grid_with_values(sheet, &values, viewport)
}

pub(crate) fn valid_dimension(value: f32, fallback: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        fallback
    }
}

pub(crate) fn dimension_size(index: u32, default_size: f32, custom: &BTreeMap<u32, f32>) -> f32 {
    match custom.get(&index).copied() {
        Some(size) if size.is_finite() && size >= 0.0 => size,
        _ => default_size,
    }
}

pub(crate) fn scaled_row_heights(sheet: &Sheet, zoom: f32) -> BTreeMap<u32, f32> {
    let mut heights: BTreeMap<u32, f32> = sheet
        .row_heights
        .iter()
        .filter(|(_, &height)| height.is_finite() && height > 0.0)
        .map(|(&row, &height)| (row, height * zoom))
        .collect();
    for &row in &sheet.hidden_rows {
        heights.insert(row, 0.0);
    }
    heights
}

pub(crate) fn dimension_extent(count: u32, default_size: f32, custom: &BTreeMap<u32, f32>) -> f32 {
    let mut extent = count as f32 * default_size;
    for (&index, &size) in custom {
        if index < count && size.is_finite() && size >= 0.0 {
            extent += size - default_size;
        }
    }
    extent.max(default_size)
}

pub(crate) fn dimension_offset(first: u32, default_size: f32, custom: &BTreeMap<u32, f32>) -> f32 {
    let mut offset = first as f32 * default_size;
    for &size in custom.range(..first).map(|(_, size)| size) {
        if size.is_finite() && size >= 0.0 {
            offset += size - default_size;
        }
    }
    offset.max(0.0)
}

/// Maps a cumulative pixel offset to the worksheet index containing it.
/// Custom dimensions are sparse, so default-sized runs are skipped in one
/// step while the handful of persisted overrides are visited explicitly.
pub(crate) fn dimension_index_at_offset(
    offset: f32,
    count: u32,
    default_size: f32,
    custom: &BTreeMap<u32, f32>,
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
            || custom_size < 0.0
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

pub(crate) fn dimension_visible_count(
    first: u32,
    count: u32,
    viewport_size: f32,
    default_size: f32,
    custom: &BTreeMap<u32, f32>,
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
    for (&custom_index, &custom_size) in custom.range(first..count) {
        if !custom_size.is_finite() || custom_size < 0.0 {
            continue;
        }
        while index < custom_index && (consumed < viewport_size || index == first) {
            consumed += default_size;
            index += 1;
        }
        if consumed >= viewport_size && index > first {
            return index.saturating_sub(first).max(1);
        }
        consumed += custom_size;
        index += 1;
        if consumed >= viewport_size && index > first {
            return index.saturating_sub(first).max(1);
        }
    }
    while index < count && (consumed < viewport_size || index == first) {
        consumed += default_size;
        index += 1;
    }
    index.saturating_sub(first).max(1)
}

pub(crate) fn viewport_from_dimensions(
    (scroll_x, scroll_y): (f32, f32),
    (viewport_width, viewport_height): (f32, f32),
    dimensions: SheetDimensions,
    default_col_width: f32,
    default_row_height: f32,
    row_heights: &BTreeMap<u32, f32>,
    col_widths: &BTreeMap<u32, f32>,
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
        default_row_height
    };
    let content_width = dimension_extent(dimensions.cols, default_col_width, col_widths);
    let content_height = dimension_extent(dimensions.rows, default_row_height, row_heights);
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
        dimension_index_at_offset(scroll_y, dimensions.rows, default_row_height, row_heights);
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
        viewport_height + 2.0 * default_row_height,
        default_row_height,
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
pub(crate) struct GridGeometry {
    pub(crate) default_col_width: f32,
    pub(crate) column_widths: Vec<f32>,
    pub(crate) row_heights: Vec<f32>,
    pub(crate) col_offset: f32,
    pub(crate) row_offset: f32,
    pub(crate) visible_width: f32,
    pub(crate) visible_height: f32,
    pub(crate) content_width: f32,
    pub(crate) content_height: f32,
}

pub(crate) fn grid_geometry(
    sheet: &Sheet,
    dimensions: SheetDimensions,
    viewport: SheetViewport,
    zoom: f32,
) -> GridGeometry {
    grid_geometry_fit(sheet, dimensions, viewport, GRID_COL_WIDTH, zoom)
}

/// Geometry for a known unscaled default column width. Scrolling reuses the
/// width from the last full projection instead of rescanning the sheet.
pub(crate) fn grid_geometry_fit(
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
    let scaled_cols: BTreeMap<u32, f32> = sheet
        .col_widths
        .iter()
        .map(|(&col, &width)| (col, width * zoom))
        .collect();
    let scaled_rows = scaled_row_heights(sheet, zoom);
    let mat_cols = materialized_cols(viewport);
    let mat_rows = materialized_rows(sheet, viewport);
    let column_widths: Vec<f32> = mat_cols
        .iter()
        .map(|&col| dimension_size(col, default_col_width, &scaled_cols))
        .collect();
    let row_heights: Vec<f32> = mat_rows
        .iter()
        .map(|&row| dimension_size(row, default_row_height, &scaled_rows))
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
