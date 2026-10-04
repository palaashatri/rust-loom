use super::grid_pointer_tests::projected;
use super::*;
use crate::cell_spill::{compute, CellFace};
use slint::Model;

const LONG: &str = "2024-01-05 quarterly planning review";

fn face<'a>(
    cells: &'a [String],
    aligns: &'a [i32],
    flags: &'a [bool],
    sizes: &'a [i32],
) -> CellFace<'a> {
    CellFace {
        cells,
        aligns,
        bolds: flags,
        italics: flags,
        sizes,
    }
}

fn row(cells: &[&str], aligns: &[i32]) -> (Vec<String>, Vec<i32>, Vec<bool>, Vec<i32>) {
    let n = cells.len();
    (
        cells.iter().map(|c| c.to_string()).collect(),
        aligns.to_vec(),
        vec![false; n],
        vec![14; n],
    )
}

fn run(cells: &[&str], aligns: &[i32]) -> crate::cell_spill::CellSpill {
    let (c, a, f, s) = row(cells, aligns);
    compute(
        &face(&c, &a, &f, &s),
        cells.len(),
        &[80.0; 8][..cells.len()],
        &[24.0],
        1.0,
    )
}

#[test]
fn left_text_runs_over_empty_cells_and_stops_at_a_filled_one() {
    let open = run(&[LONG, "", "", "", ""], &[0; 5]);
    assert_eq!(open.kinds[0], 1);
    let item = &open.items[0];
    assert_eq!(item.x, 0.0);
    assert!(item.width > 160.0, "spans past B: {}", item.width);
    let blocked = run(&[LONG, "x", "", "", ""], &[0; 5]);
    assert_eq!(blocked.kinds[0], 0, "B1 is filled, so A1 stays clipped");
    assert!(blocked.items.is_empty());
    let stopped = run(&[LONG, "", "x", "", ""], &[0; 5]);
    assert_eq!(stopped.items[0].width, 160.0, "stops at the A/B/C boundary");
}

#[test]
fn right_text_runs_left_and_centered_text_both_ways() {
    let right = run(&["", "", "", LONG, ""], &[0, 0, 0, 2, 0]);
    let item = &right.items[0];
    assert!(item.x < 240.0 && item.x + item.width == 320.0);
    let centered = run(&["", "", LONG, "", ""], &[0, 0, 1, 0, 0]);
    let item = &centered.items[0];
    assert!(item.x < 160.0 && item.x + item.width > 240.0, "{item:?}");
}

#[test]
fn numbers_never_overflow_and_are_clipped_not_elided() {
    let r = run(&["1234567890123456789", "", ""], &[2, 0, 0]);
    assert_eq!(r.kinds, vec![2, 0, 0]);
    assert!(r.items.is_empty());
}

fn dark_pixels(image: &image::RgbaImage, x0: u32, x1: u32, y0: u32, y1: u32) -> usize {
    (y0..y1.min(image.height()))
        .flat_map(|y| (x0..x1.min(image.width())).map(move |x| (x, y)))
        .filter(|&(x, y)| image.get_pixel(x, y).0[..3].iter().all(|&v| v < 110))
        .count()
}

/// Column bounds in window pixels, found from the layout the app published.
fn bounds(app: &SheetsApp, col: usize) -> (u32, u32) {
    let widths: Vec<f32> = app.get_grid_column_widths().iter().collect();
    let before: f32 = widths[..col].iter().sum();
    let left = 36.0 + app.get_grid_col_offset() + before;
    (left as u32, (left + widths[col]) as u32)
}

#[test]
fn rendered_text_crosses_empty_cells_but_is_clipped_at_a_filled_one() {
    let (open, _s) = projected(&[("A1", LONG)]);
    let (b0, b1) = bounds(&open, 1);
    let image = snapshot_component(&open, 1280.0, 800.0, 1.0).expect("render");
    let rows = row_band(&open);
    let spilled = dark_pixels(&image, b0 + 2, b1 - 2, rows.0, rows.1);
    assert!(
        spilled > 20,
        "text should be drawn over empty B1: {spilled}"
    );
    assert!(!open.get_cells().row_data(0).unwrap().is_empty());

    let (shut, _s) = projected(&[("A1", LONG), ("B1", "")]);
    drop(shut);
    let (blocked, _s) = projected(&[("A1", LONG), ("B1", "z")]);
    let image = snapshot_component(&blocked, 1280.0, 800.0, 1.0).expect("render");
    let (b0, b1) = bounds(&blocked, 1);
    let rows = row_band(&blocked);
    // Only B1's own single character may be dark; A1's text must not cross.
    let crossing = dark_pixels(&image, b0 + 2, b0 + 7, rows.0, rows.1);
    assert_eq!(crossing, 0, "A1 text must stop at the A/B boundary");
    assert!(
        dark_pixels(&image, b0, b1, rows.0, rows.1) > 0,
        "B1's z is visible"
    );
}

/// Row 1 in a 1280x800 window starts below the menu, toolbar, sheet tabs, formula bar and
/// column headers (inspected in a captured image).
fn row_band(_app: &SheetsApp) -> (u32, u32) {
    (180, 196)
}
