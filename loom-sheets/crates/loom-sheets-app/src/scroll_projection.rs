//! Scroll-time projection for the worksheet grid.
//!
//! A wheel tick used to rebuild fourteen models from scratch, which destroyed
//! and recreated every cell element and rescanned the whole sheet for its
//! dimensions and formula count. Scrolling now touches only what moved:
//! nothing when the visible window is unchanged, and in-place row updates when
//! it slides, so Slint keeps and re-binds the existing cell elements.

use super::*;
use slint::Model as _;

/// Update `current` in place when it is a `VecModel`, so repeaters keep their
/// elements. Returns a fresh model only when the property held something else.
fn refresh_model<T>(current: ModelRc<T>, next: Vec<T>) -> Option<ModelRc<T>>
where
    T: Clone + PartialEq + 'static,
{
    match current.as_any().downcast_ref::<VecModel<T>>() {
        Some(model) if model.row_count() == next.len() => {
            for (index, item) in next.into_iter().enumerate() {
                if model.row_data(index).as_ref() != Some(&item) {
                    model.set_row_data(index, item);
                }
            }
            None
        }
        Some(model) => {
            model.set_vec(next);
            None
        }
        None => Some(ModelRc::new(VecModel::from(next))),
    }
}

macro_rules! sync_model {
    ($app:expr, $get:ident, $set:ident, $next:expr) => {
        if let Some(model) = refresh_model($app.$get(), $next) {
            $app.$set(model);
        }
    };
}

fn shared(values: Vec<String>) -> Vec<SharedString> {
    values.into_iter().map(SharedString::from).collect()
}

/// Publish a projected window, its objects, and its geometry to the window.
pub(super) fn apply_grid(
    app: &SheetsApp,
    grid: ProjectedSheetGrid,
    objects: Option<Vec<SheetObjectView>>,
    geometry: &GridGeometry,
    zoom: f32,
) {
    sync_model!(app, get_cols, set_cols, grid.cols);
    sync_model!(app, get_rows, set_rows, grid.rows);
    sync_model!(
        app,
        get_column_headers,
        set_column_headers,
        shared(grid.column_headers)
    );
    sync_model!(
        app,
        get_row_headers,
        set_row_headers,
        shared(grid.row_headers)
    );
    sync_model!(app, get_cells, set_cells, shared(grid.cells));
    sync_model!(
        app,
        get_cell_alignments,
        set_cell_alignments,
        grid.cell_alignments
    );
    sync_model!(app, get_cell_bolds, set_cell_bolds, grid.cell_bolds);
    sync_model!(app, get_cell_italics, set_cell_italics, grid.cell_italics);
    sync_model!(
        app,
        get_cell_underlines,
        set_cell_underlines,
        grid.cell_underlines
    );
    sync_model!(app, get_cell_borders, set_cell_borders, grid.cell_borders);
    sync_model!(app, get_cell_fills, set_cell_fills, grid.cell_fills);
    sync_model!(
        app,
        get_cell_font_sizes,
        set_cell_font_sizes,
        grid.cell_font_sizes
    );
    // Object gestures rely on their elements being rebuilt after every
    // full projection; only scrolling updates them in place.
    if let Some(objects) = objects {
        app.set_object_views(ModelRc::new(VecModel::from(objects)));
    }
    app.set_grid_col_width(geometry.default_col_width);
    app.set_grid_row_height(GRID_ROW_HEIGHT * zoom);
    app.set_grid_col_offset(geometry.col_offset);
    app.set_grid_row_offset(geometry.row_offset);
    app.set_grid_visible_width(geometry.visible_width);
    app.set_grid_visible_height(geometry.visible_height);
    app.set_grid_content_width(geometry.content_width);
    app.set_grid_content_height(geometry.content_height);
    sync_model!(
        app,
        get_grid_column_widths,
        set_grid_column_widths,
        geometry.column_widths.clone()
    );
    sync_model!(
        app,
        get_grid_row_heights,
        set_grid_row_heights,
        geometry.row_heights.clone()
    );
}

/// The window that the current scroll offsets select, computed from the
/// dimensions and column width the last full projection published so it never
/// walks the sheet's cells.
fn scrolled_viewport(
    app: &SheetsApp,
    sheet: &Sheet,
    zoom: f32,
) -> (SheetViewport, SheetDimensions) {
    let mut dimensions = SheetDimensions::new(
        app.get_workbook_rows().max(1) as u32,
        app.get_workbook_cols().max(1) as u32,
    );
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
    let default_col_width = valid_dimension(app.get_grid_col_width(), GRID_COL_WIDTH * zoom);
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
    let viewport = viewport_from_dimensions(
        (
            (-app.get_grid_scroll_x()).max(0.0),
            (-app.get_grid_scroll_y()).max(0.0),
        ),
        (
            (viewport_width - GRID_ROW_HEADER_WIDTH).max(default_col_width),
            (viewport_height - GRID_COLUMN_HEADER_HEIGHT).max(default_row_height),
        ),
        dimensions,
        default_col_width,
        &scaled_rows,
        &scaled_cols,
    );
    // Scrolling toward the end of the sheet makes more of it: a spreadsheet
    // has no last row to hit.
    let grown = grown_dimensions(dimensions, viewport);
    if grown != dimensions {
        app.set_workbook_rows(grown.rows as i32);
        app.set_workbook_cols(grown.cols as i32);
        dimensions = grown;
    }
    (viewport, dimensions)
}

/// Rows and columns the grid keeps beyond the last visible one.
const TAIL_ROWS: u32 = 30;
const TAIL_COLS: u32 = 12;
const MAX_GRID_ROWS: u32 = 1_048_576;
const MAX_GRID_COLS: u32 = 16_384;

/// `dimensions`, extended so at least a tail's worth of rows and columns lies
/// past the visible window, up to the sheet limits.
fn grown_dimensions(dimensions: SheetDimensions, viewport: SheetViewport) -> SheetDimensions {
    let rows_needed = (viewport.first_row + viewport.visible_rows)
        .saturating_add(TAIL_ROWS)
        .min(MAX_GRID_ROWS);
    let cols_needed = (viewport.first_col + viewport.visible_cols)
        .saturating_add(TAIL_COLS)
        .min(MAX_GRID_COLS);
    SheetDimensions::new(
        dimensions.rows.max(rows_needed),
        dimensions.cols.max(cols_needed),
    )
}

/// Handle a scroll event. Cell text is re-projected only when the set of
/// visible rows or columns changed; floating objects always follow the scroll
/// but keep their decoded images.
pub(crate) fn project_scroll(app: &SheetsApp, state: &GuiState) {
    let sheet = state.current.borrow();
    let zoom = zoom_factor(app);
    let (viewport, dimensions) = scrolled_viewport(app, &sheet, zoom);
    let window_moved = viewport.first_row as i32 != app.get_view_row_origin()
        || viewport.first_col as i32 != app.get_view_col_origin()
        || viewport.visible_rows as usize != app.get_rows().row_count()
        || viewport.visible_cols as usize != app.get_cols().row_count();
    let fit_col_width = valid_dimension(app.get_grid_col_width() / zoom, GRID_COL_WIDTH);
    let geometry = grid_geometry_fit(&sheet, dimensions, viewport, fit_col_width, zoom);
    let existing = app.get_object_views();
    let reusable = existing.row_count() == sheet.objects.len();
    let objects = project_sheet_objects(
        &sheet,
        &geometry,
        zoom,
        app.get_grid_scroll_x(),
        app.get_grid_scroll_y(),
        object_actions::preview_geometry(state),
        &|index, object| match existing.row_data(index) {
            Some(view) if reusable => view.image,
            _ => load_sheet_object_image(object),
        },
    );
    if !window_moved {
        sync_model!(app, get_object_views, set_object_views, objects.views);
        return;
    }
    let values = values_for_projection(state);
    let viewport_geometry = (app.get_grid_scroll_x(), app.get_grid_scroll_y());
    app.set_view_row_origin(viewport.first_row as i32);
    app.set_view_col_origin(viewport.first_col as i32);
    let grid = project_sheet_grid_with_values(&sheet, &values, viewport);
    apply_grid(app, grid, None, &geometry, zoom);
    sync_model!(app, get_object_views, set_object_views, objects.views);
    // The window changed underneath a live scroll; keep the pointer's offsets.
    app.set_grid_scroll_x(viewport_geometry.0);
    app.set_grid_scroll_y(viewport_geometry.1);
}

#[cfg(test)]
mod tail_tests {
    use super::*;

    fn window(first_row: u32, first_col: u32) -> SheetViewport {
        SheetViewport {
            first_row,
            first_col,
            visible_rows: 24,
            visible_cols: 10,
        }
    }

    #[test]
    fn scrolling_near_the_end_adds_rows_and_columns() {
        let grown = grown_dimensions(SheetDimensions::new(60, 20), window(40, 12));
        assert_eq!((grown.rows, grown.cols), (40 + 24 + 30, 12 + 10 + 12));
    }

    #[test]
    fn a_window_well_inside_the_sheet_changes_nothing() {
        let dims = SheetDimensions::new(500, 80);
        assert_eq!(grown_dimensions(dims, window(10, 0)), dims);
    }

    #[test]
    fn growth_stops_at_the_sheet_limits() {
        let grown = grown_dimensions(
            SheetDimensions::new(MAX_GRID_ROWS, MAX_GRID_COLS),
            window(MAX_GRID_ROWS - 24, MAX_GRID_COLS - 10),
        );
        assert_eq!((grown.rows, grown.cols), (MAX_GRID_ROWS, MAX_GRID_COLS));
    }
}
