//! Grid chrome at 1.0 and 2.0 text scale: rows, cells and header bands grow
//! with the text, the formula bar icons grow, and neither scrollbar covers a
//! header band.
use super::keyboard_flow_tests::{launched, Session};
use super::*;
use i_slint_backend_testing::ElementHandle;

fn one(app: &SheetsApp, label: &str) -> ElementHandle {
    let found: Vec<_> = ElementHandle::find_by_accessible_label(app, label).collect();
    assert_eq!(found.len(), 1, "one element named {label:?}");
    found.into_iter().next().expect("the named element")
}

/// The 1280x800 grid, projected and laid out at the given text scale.
fn grid_at_text_scale(scale: f32) -> Session {
    let session = launched(&[("A1", "5")]);
    session.app.set_template_text_scale(scale);
    project_current(&session.app, &session.state);
    let _ = snapshot_component(&session.app, 1280.0, 800.0, 1.0).expect("render the grid");
    session
}

/// The geometry the assertions compare. Each scale is read completely before
/// the next session is created, so no measurement depends on a later session.
#[derive(Debug)]
struct Measured {
    cell_height: f32,
    row_band_width: f32,
    column_band_height: f32,
    check_icon_width: f32,
    row_band_right: f32,
    horizontal_bar_x: f32,
    column_band_bottom: f32,
    vertical_bar_y: f32,
}

fn measure(scale: f32) -> Measured {
    let session = grid_at_text_scale(scale);
    let app = &session.app;
    let size = |label: &str| one(app, label).size();
    let at = |label: &str| one(app, label).absolute_position();
    let row_band = at("Row 1");
    let column_band = at("Column A");
    Measured {
        cell_height: size("A1, value 5").height,
        row_band_width: size("Row 1").width,
        column_band_height: size("Column A").height,
        check_icon_width: size("Commit formula").width,
        row_band_right: row_band.x + size("Row 1").width,
        horizontal_bar_x: at("Horizontal sheet scroll").x,
        column_band_bottom: column_band.y + size("Column A").height,
        vertical_bar_y: at("Vertical sheet scroll").y,
    }
}

#[test]
fn rows_headers_and_formula_icons_grow_with_text_scale_and_scrollbars_leave_the_headers_clear() {
    let small = measure(1.0);
    let large = measure(2.0);

    assert!(
        large.cell_height >= 1.9 * small.cell_height,
        "rows must grow with the text: {} px at 1.0, {} px at 2.0",
        small.cell_height,
        large.cell_height
    );
    assert!(
        large.row_band_width >= 1.9 * small.row_band_width,
        "the row-number band must grow with the text: {} px at 1.0, {} px at 2.0",
        small.row_band_width,
        large.row_band_width
    );
    assert!(
        large.column_band_height >= 1.9 * small.column_band_height,
        "the column-letter band must grow with the text: {} px at 1.0, {} px at 2.0",
        small.column_band_height,
        large.column_band_height
    );
    assert!(
        large.check_icon_width >= 1.9 * small.check_icon_width,
        "the formula bar's check icon must grow with the text: {} px at 1.0, {} px at 2.0",
        small.check_icon_width,
        large.check_icon_width
    );
    assert!(
        large.horizontal_bar_x >= large.row_band_right - 0.5,
        "the horizontal scrollbar covers the row numbers ({} < {})",
        large.horizontal_bar_x,
        large.row_band_right
    );
    assert!(
        large.vertical_bar_y >= large.column_band_bottom - 0.5,
        "the vertical scrollbar covers the column letters ({} < {})",
        large.vertical_bar_y,
        large.column_band_bottom
    );
}
