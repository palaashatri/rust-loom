//! The worksheet grid as assistive technology sees it: every visible cell is
//! named by its address, value and formula, every header by its column or row,
//! and the high-contrast active-cell outline stands out from the white grid.
use super::keyboard_flow_tests::launched;
use super::*;
use i_slint_backend_testing::{ElementHandle, ElementRoot};

/// Accessible names of every element currently in the window.
fn accessible_names(app: &SheetsApp) -> Vec<String> {
    app.root_element()
        .query_descendants()
        .match_predicate(|_| true)
        .find_all()
        .into_iter()
        .filter_map(|element| element.accessible_label().map(|label| label.to_string()))
        .collect()
}

#[test]
fn every_visible_cell_and_header_has_an_accessible_name() {
    let s = launched(&[("A1", "5"), ("A2", "7"), ("B3", "12"), ("B4", "=A1+A2")]);
    let _ = snapshot_component(&s.app, 1280.0, 800.0, 1.0).expect("render the grid");
    let names = accessible_names(&s.app);
    // The active cell A1 and the filled cells in its row and column are named,
    // along with every header.
    for expected in ["A1, value 5", "A2, value 7", "Column B", "Row 3"] {
        assert!(
            names.iter().any(|name| name == expected),
            "no grid element is named {expected:?}"
        );
    }
    // Moving the selection to B3 names it and the filled cell below it, formula included.
    s.app.set_selected_row(2);
    s.app.set_selected_col(1);
    let _ = snapshot_component(&s.app, 1280.0, 800.0, 1.0).expect("render the grid");
    let names = accessible_names(&s.app);
    for expected in ["B3, value 12", "B4, value 12, formula =A1+A2"] {
        assert!(
            names.iter().any(|name| name == expected),
            "no grid element is named {expected:?} once B3 is selected"
        );
    }
}

#[test]
fn blank_cells_are_exposed_only_while_selected() {
    // Naming every blank cell made each scroll rebind hundreds of accessibility
    // nodes, so a blank cell is a node only when it is the active cell.
    let s = launched(&[("A1", "5")]);
    let _ = snapshot_component(&s.app, 1280.0, 800.0, 1.0).expect("render the grid");
    assert!(
        !accessible_names(&s.app)
            .iter()
            .any(|name| name == "C5, empty"),
        "an unselected blank cell is still a node of its own"
    );
    s.app.set_selected_row(4);
    s.app.set_selected_col(2);
    let _ = snapshot_component(&s.app, 1280.0, 800.0, 1.0).expect("render the grid");
    assert!(
        accessible_names(&s.app)
            .iter()
            .any(|name| name == "C5, empty"),
        "the selected blank cell is not announced as empty"
    );
}

/// WCAG relative luminance of an sRGB pixel.
fn luminance(pixel: &image::Rgba<u8>) -> f64 {
    let channel = |value: u8| {
        let v = f64::from(value) / 255.0;
        if v <= 0.03928 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(pixel[0]) + 0.7152 * channel(pixel[1]) + 0.0722 * channel(pixel[2])
}

/// WCAG contrast ratio between two pixels, from 1:1 to 21:1.
fn contrast(a: &image::Rgba<u8>, b: &image::Rgba<u8>) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

#[test]
fn the_high_contrast_active_cell_outline_is_visible_against_the_grid() {
    let s = launched(&[("A1", "5")]);
    apply_theme(&s.app, "high-contrast");
    let image = snapshot_component(&s.app, 1280.0, 800.0, 1.0).expect("render high contrast");
    let cell = ElementHandle::find_by_accessible_label(&s.app, "A1, value 5")
        .next()
        .expect("the active cell is named");
    let origin = cell.absolute_position();
    let size = cell.size();
    let x = (origin.x + size.width / 2.0) as u32;
    // The active outline is the 2 px border along the cell's top edge; the
    // cell's interior below it is the white grid.
    let outline = *image.get_pixel(x, (origin.y + 1.0) as u32);
    let grid = *image.get_pixel(x, (origin.y + 7.0) as u32);
    let ratio = contrast(&outline, &grid);
    assert!(
        ratio >= 3.0,
        "the high-contrast active-cell outline is {ratio:.2}:1 against the grid, below 3:1 \
         (outline {outline:?}, grid {grid:?})"
    );
}
