//! Moves of the active cell larger than one step.
//!
//! The grid's key handler reports every move as a (row, column) delta. Page
//! Up/Down, Home and Ctrl+Home/End are deltas too, using values far beyond any
//! real sheet that mean "a page", "the first column" and "the last used cell"
//! instead of a distance.

use loom_sheets_core::{CellRef, Sheet};

/// One page of rows down; its negative is one page up.
pub(crate) const PAGE: i32 = 2_000_000_000;
/// All the way to the last used row or column; its negative is back to the first.
pub(crate) const EDGE: i32 = 2_100_000_000;

fn step(value: u32, delta: i32, page: u32, last_used: u32) -> u32 {
    match delta {
        PAGE => value.saturating_add(page),
        d if d == -PAGE => value.saturating_sub(page),
        EDGE => last_used,
        d if d == -EDGE => 0,
        d if d < 0 => value.saturating_sub(d.unsigned_abs()),
        d => value.saturating_add(d as u32),
    }
}

/// Where a move of `(row_delta, col_delta)` from `from` lands. `page` is the
/// number of rows that fit in the window.
pub(crate) fn destination(
    sheet: &Sheet,
    from: CellRef,
    (row_delta, col_delta): (i32, i32),
    page: u32,
) -> CellRef {
    let used = sheet.dimensions();
    CellRef {
        row: step(
            from.row,
            row_delta,
            page.max(1),
            used.rows.saturating_sub(1),
        ),
        col: step(from.col, col_delta, 1, used.cols.saturating_sub(1)),
    }
}

use loom_sheets_core::SheetViewport;

use crate::{
    dimension_size, zoom_factor, SheetsApp, DEFAULT_VISIBLE_COLS, DEFAULT_VISIBLE_ROWS,
    GRID_COLUMN_HEADER_HEIGHT, GRID_COL_WIDTH, GRID_ROW_HEADER_WIDTH, GRID_ROW_HEIGHT,
};

/// Scroll `viewport` just far enough to show `selected` completely, counting
/// only the cells that really fit in the grid window.
pub(crate) fn reveal_selected(
    app: &SheetsApp,
    sheet: &Sheet,
    viewport: &mut SheetViewport,
    selected: CellRef,
) {
    let zoom = zoom_factor(app);
    let (width, height) =
        if app.get_grid_viewport_width() > 1.0 && app.get_grid_viewport_height() > 1.0 {
            (
                app.get_grid_viewport_width(),
                app.get_grid_viewport_height(),
            )
        } else {
            (
                GRID_COL_WIDTH * DEFAULT_VISIBLE_COLS as f32 + GRID_ROW_HEADER_WIDTH,
                GRID_ROW_HEIGHT * DEFAULT_VISIBLE_ROWS as f32 + GRID_COLUMN_HEADER_HEIGHT,
            )
        };
    let default_col_width = GRID_COL_WIDTH * zoom;
    let cols: std::collections::BTreeMap<u32, f32> = sheet
        .col_widths
        .iter()
        .map(|(&index, &size)| (index, size * zoom))
        .collect();
    let rows: std::collections::BTreeMap<u32, f32> = sheet
        .row_heights
        .iter()
        .map(|(&index, &size)| (index, size * zoom))
        .collect();
    let fit_cols = fit_count(
        viewport.first_col,
        (width - GRID_ROW_HEADER_WIDTH).max(default_col_width),
        |index| dimension_size(index, default_col_width, &cols),
    );
    let fit_rows = fit_count(
        viewport.first_row,
        (height - GRID_COLUMN_HEADER_HEIGHT).max(GRID_ROW_HEIGHT),
        |index| dimension_size(index, GRID_ROW_HEIGHT, &rows),
    );
    reveal_fully(viewport, selected, (fit_rows, fit_cols));
}

/// How many cells starting at `first` fit completely in `viewport_size`.
pub(crate) fn fit_count(first: u32, viewport_size: f32, size_of: impl Fn(u32) -> f32) -> u32 {
    let mut consumed = 0.0_f32;
    let mut count = 0_u32;
    while count < 100_000 {
        let size = size_of(first.saturating_add(count));
        if consumed + size > viewport_size + 0.5 {
            break;
        }
        consumed += size;
        count += 1;
    }
    count.max(1)
}

/// Move `viewport` just far enough that `cell` is fully on screen. The window
/// holds a couple of extra rows and columns past the visible edge so scrolling
/// never shows blank strips; those must not count as on screen here, or a
/// cell sitting in them would never be scrolled to.
pub(crate) fn reveal_fully(
    viewport: &mut SheetViewport,
    cell: CellRef,
    (fit_rows, fit_cols): (u32, u32),
) {
    let (rows, cols) = (viewport.visible_rows, viewport.visible_cols);
    viewport.visible_rows = fit_rows.clamp(1, rows);
    viewport.visible_cols = fit_cols.clamp(1, cols);
    viewport.reveal(cell);
    viewport.visible_rows = rows;
    viewport.visible_cols = cols;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sheet() -> Sheet {
        let mut sheet = Sheet::new("nav");
        sheet.set_str("D9", "x");
        sheet
    }

    fn at(a1: &str) -> CellRef {
        CellRef::parse(a1).unwrap()
    }

    #[test]
    fn single_steps_stay_single_steps() {
        let moved = destination(&sheet(), at("B2"), (1, -1), 20);
        assert_eq!(moved, at("A3"));
        assert_eq!(destination(&sheet(), at("A1"), (-1, -1), 20), at("A1"));
    }

    #[test]
    fn page_moves_by_the_rows_that_fit_and_stops_at_the_top() {
        assert_eq!(destination(&sheet(), at("A5"), (PAGE, 0), 20), at("A25"));
        assert_eq!(destination(&sheet(), at("A25"), (-PAGE, 0), 20), at("A5"));
        assert_eq!(destination(&sheet(), at("A5"), (-PAGE, 0), 20), at("A1"));
    }

    #[test]
    fn home_goes_to_column_a_and_ctrl_end_to_the_last_used_cell() {
        assert_eq!(destination(&sheet(), at("F7"), (0, -EDGE), 20), at("A7"));
        assert_eq!(destination(&sheet(), at("A1"), (EDGE, EDGE), 20), at("D9"));
        assert_eq!(
            destination(&sheet(), at("F7"), (-EDGE, -EDGE), 20),
            at("A1")
        );
    }
}

#[cfg(test)]
mod reveal_tests {
    use super::*;

    #[test]
    fn extra_materialized_rows_do_not_count_as_visible() {
        // 24 rows materialized, 22 fit on screen; the cell sits in row 23.
        let mut viewport = SheetViewport {
            first_row: 0,
            first_col: 0,
            visible_rows: 24,
            visible_cols: 12,
        };
        reveal_fully(&mut viewport, CellRef { row: 22, col: 0 }, (22, 10));
        assert_eq!(viewport.first_row, 1);
        assert_eq!((viewport.visible_rows, viewport.visible_cols), (24, 12));
        // A cell that already fits does not move the window.
        reveal_fully(&mut viewport, CellRef { row: 5, col: 3 }, (22, 10));
        assert_eq!((viewport.first_row, viewport.first_col), (1, 0));
    }

    #[test]
    fn fit_count_counts_only_whole_cells_and_follows_custom_sizes() {
        assert_eq!(fit_count(0, 100.0, |_| 24.0), 4);
        assert_eq!(fit_count(0, 100.0, |i| if i == 1 { 60.0 } else { 24.0 }), 2);
        assert_eq!(fit_count(0, 5.0, |_| 24.0), 1);
    }
}

/// Where Ctrl+Arrow takes the active cell, as in Excel. `(row_dir, col_dir)`
/// is one of (-1,0), (1,0), (0,-1), (0,1). From a filled cell whose neighbour
/// is filled it runs to the last filled cell of that block; otherwise it goes
/// to the next filled cell in that direction. With none, it stops at the sheet
/// edge: row 1 or column A going up or left, the used extent going down or
/// right.
pub(crate) fn data_edge(sheet: &Sheet, from: CellRef, (row_dir, col_dir): (i32, i32)) -> CellRef {
    let used = sheet.dimensions();
    let vertical = row_dir != 0;
    let forward = if vertical { row_dir > 0 } else { col_dir > 0 };
    let at = |position: u32| {
        if vertical {
            CellRef {
                row: position,
                col: from.col,
            }
        } else {
            CellRef {
                row: from.row,
                col: position,
            }
        }
    };
    let filled = |position: u32| sheet.raw(at(position)).is_some_and(|raw| !raw.is_empty());
    let start = if vertical { from.row } else { from.col };
    let last = if vertical { used.rows } else { used.cols }
        .saturating_sub(1)
        .max(start);
    let edge = if forward { last } else { 0 };
    if start == edge {
        return from;
    }
    let step = |position: u32| if forward { position + 1 } else { position - 1 };
    let mut position = step(start);
    if filled(start) && filled(position) {
        while position != edge && filled(step(position)) {
            position = step(position);
        }
        return at(position);
    }
    while !filled(position) && position != edge {
        position = step(position);
    }
    at(position)
}

#[cfg(test)]
mod data_edge_tests {
    use super::*;

    fn at(a1: &str) -> CellRef {
        CellRef::parse(a1).unwrap()
    }

    fn sheet() -> Sheet {
        let mut sheet = Sheet::new("edge");
        for cell in ["B2", "B3", "B4", "B7", "B8", "E2", "G2", "H2", "B12"] {
            sheet.set_str(cell, "x");
        }
        sheet
    }

    #[test]
    fn a_block_runs_to_its_last_filled_cell_down_and_back_up() {
        assert_eq!(data_edge(&sheet(), at("B2"), (1, 0)), at("B4"));
        assert_eq!(data_edge(&sheet(), at("B4"), (-1, 0)), at("B2"));
        assert_eq!(data_edge(&sheet(), at("B3"), (1, 0)), at("B4"));
    }

    #[test]
    fn from_the_end_of_a_block_it_jumps_to_the_next_block() {
        assert_eq!(data_edge(&sheet(), at("B4"), (1, 0)), at("B7"));
        assert_eq!(data_edge(&sheet(), at("B8"), (1, 0)), at("B12"));
        assert_eq!(data_edge(&sheet(), at("B7"), (-1, 0)), at("B4"));
    }

    #[test]
    fn from_an_empty_cell_it_finds_the_next_filled_cell_or_the_edge() {
        assert_eq!(data_edge(&sheet(), at("B5"), (1, 0)), at("B7"));
        assert_eq!(data_edge(&sheet(), at("B5"), (-1, 0)), at("B4"));
        // Nothing below B12 in column B: the last used row.
        assert_eq!(data_edge(&sheet(), at("B12"), (1, 0)), at("B12"));
        assert_eq!(data_edge(&sheet(), at("D5"), (1, 0)), at("D12"));
        // Nothing above in column D: row 1.
        assert_eq!(data_edge(&sheet(), at("D5"), (-1, 0)), at("D1"));
    }

    #[test]
    fn sideways_moves_follow_the_same_rules() {
        assert_eq!(data_edge(&sheet(), at("A2"), (0, 1)), at("B2"));
        assert_eq!(data_edge(&sheet(), at("B2"), (0, 1)), at("E2"));
        assert_eq!(data_edge(&sheet(), at("G2"), (0, 1)), at("H2"));
        assert_eq!(data_edge(&sheet(), at("H2"), (0, -1)), at("G2"));
        assert_eq!(data_edge(&sheet(), at("H2"), (0, 1)), at("H2"));
        assert_eq!(data_edge(&sheet(), at("B5"), (0, -1)), at("A5"));
        assert_eq!(data_edge(&sheet(), at("C5"), (0, 1)), at("H5"));
    }
}
