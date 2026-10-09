use loom_sheets_core::{CellRef, Sheet, SheetDimensions};

pub(crate) const MIN_RENDERED_OBJECT_WIDTH: i32 = 80;
pub(crate) const MIN_RENDERED_OBJECT_HEIGHT: i32 = 48;

/// Keep an anchored object projected while any part of its body or selected
/// resize target intersects the scrollable canvas.
pub(crate) fn intersects_object_viewport(
    object_x: f32,
    object_y: f32,
    object_width: f32,
    object_height: f32,
    viewport_width: f32,
    viewport_height: f32,
) -> bool {
    const CANVAS_LEFT: f32 = 0.0;
    const CANVAS_TOP: f32 = 0.0;
    const RESIZE_HANDLE_OVERHANG: f32 = 12.0;
    object_x + object_width + RESIZE_HANDLE_OVERHANG > CANVAS_LEFT
        && object_x < CANVAS_LEFT + viewport_width
        && object_y + object_height + RESIZE_HANDLE_OVERHANG > CANVAS_TOP
        && object_y < CANVAS_TOP + viewport_height
}

pub(crate) fn rendered_object_extent(document_extent: u32, zoom: f32, minimum: i32) -> i32 {
    ((document_extent as f32 * zoom).round().max(1.0) as i32).max(minimum)
}

/// Extend the addressable grid through each object's resize affordance.
#[cfg(test)]
pub(crate) fn editor_dimensions_with_width(
    sheet: &Sheet,
    selected: CellRef,
    fill: Option<(u32, u32)>,
    default_col_width: f32,
) -> SheetDimensions {
    editor_dimensions_with_preview(sheet, selected, fill, default_col_width, 1.0, None)
}

pub(crate) fn editor_dimensions_with_preview(
    sheet: &Sheet,
    selected: CellRef,
    fill: Option<(u32, u32)>,
    default_col_width: f32,
    zoom: f32,
    preview: Option<(usize, CellRef, u32, u32)>,
) -> SheetDimensions {
    const SCROLL_AHEAD_COLS: u32 = 12;
    const SCROLL_AHEAD_ROWS: u32 = 30;
    const OBJECT_HANDLE_OVERHANG: f32 = 12.0;
    let zoom = if zoom.is_finite() && zoom > 0.0 {
        zoom.clamp(0.5, 3.0)
    } else {
        1.0
    };
    let dimensions = sheet.dimensions();
    let (fill_cols, fill_rows) =
        fill.unwrap_or((crate::DEFAULT_VISIBLE_COLS, crate::DEFAULT_VISIBLE_ROWS));
    let (ahead_cols, ahead_rows) = if fill.is_some() {
        (SCROLL_AHEAD_COLS, SCROLL_AHEAD_ROWS)
    } else {
        (0, 0)
    };
    // Room to keep scrolling past the last used or selected cell, so the
    // grid never ends exactly where the user last looked.
    let mut rows = dimensions
        .rows
        .max(selected.row.saturating_add(1))
        .max(crate::DEFAULT_VISIBLE_ROWS)
        .max(fill_rows)
        .saturating_add(ahead_rows);
    let mut cols = dimensions
        .cols
        .max(selected.col.saturating_add(1))
        .max(crate::DEFAULT_VISIBLE_COLS)
        .max(fill_cols)
        .saturating_add(ahead_cols);
    let custom_cols: std::collections::BTreeMap<u32, f32> = sheet
        .col_widths
        .iter()
        .map(|(&column, &width)| (column, width))
        .collect();
    let custom_rows: std::collections::BTreeMap<u32, f32> = sheet
        .row_heights
        .iter()
        .map(|(&row, &height)| (row, height))
        .collect();
    for (index, object) in sheet.objects.iter().enumerate() {
        let (anchor, width, height) = preview
            .filter(|(preview_index, _, _, _)| *preview_index == index)
            .map(|(_, anchor, width, height)| (anchor, width, height))
            .unwrap_or((object.anchor, object.width, object.height));
        rows = rows.max(count_for_pixel_extent(
            rows,
            anchor.row,
            rendered_object_extent(height, zoom, MIN_RENDERED_OBJECT_HEIGHT) as f32 / zoom
                + OBJECT_HANDLE_OVERHANG / zoom,
            crate::GRID_ROW_HEIGHT * zoom,
            &custom_rows,
            1_048_576,
        ));
        cols = cols.max(count_for_pixel_extent(
            cols,
            anchor.col,
            rendered_object_extent(width, zoom, MIN_RENDERED_OBJECT_WIDTH) as f32 / zoom
                + OBJECT_HANDLE_OVERHANG / zoom,
            default_col_width,
            &custom_cols,
            16_384,
        ));
    }
    SheetDimensions::new(rows, cols)
}

fn count_for_pixel_extent(
    base_count: u32,
    anchor: u32,
    extent: f32,
    default_size: f32,
    custom: &std::collections::BTreeMap<u32, f32>,
    maximum_count: u32,
) -> u32 {
    let base_count = base_count.max(anchor.saturating_add(1));
    let maximum_count = maximum_count.max(base_count);
    let target_end = crate::dimension_offset(anchor, default_size, custom) + extent.max(0.0);
    if crate::dimension_extent(base_count, default_size, custom) >= target_end {
        return base_count;
    }

    let mut high = base_count;
    while high < maximum_count && crate::dimension_extent(high, default_size, custom) < target_end {
        high = high
            .saturating_mul(2)
            .max(high.saturating_add(1))
            .min(maximum_count);
    }
    if crate::dimension_extent(high, default_size, custom) < target_end {
        return high;
    }
    let mut low = base_count;
    while low.saturating_add(1) < high {
        let middle = low + (high - low) / 2;
        if crate::dimension_extent(middle, default_size, custom) >= target_end {
            high = middle;
        } else {
            low = middle;
        }
    }
    high
}
