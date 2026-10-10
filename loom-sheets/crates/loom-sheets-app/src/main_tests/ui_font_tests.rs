//! Window chrome takes the interface face; the grid keeps the bundled face.
//!
//! The same window is captured twice: once with the bundled face and once with a
//! family no machine installs, which Slint replaces with its fallback. Toolbar
//! text must change between the two captures and cell text must not.
use super::scale_surfaces_tests::editor;
use super::*;
use i_slint_backend_testing::ElementHandle;
use loom_test_support::region::{region_changed, region_is_drawn, PixelRect};

/// A family no machine installs, so text drawn in it differs from the bundled face.
const OTHER_FACE: &str = "Loom Test Face That Is Not Installed";

fn pixel_rect(app: &SheetsApp, id: &str) -> PixelRect {
    let element = ElementHandle::find_by_element_id(app, id)
        .next()
        .unwrap_or_else(|| panic!("no element {id}"));
    let (p, s) = (element.absolute_position(), element.size());
    (
        p.x.floor() as u32,
        p.y.floor() as u32,
        s.width.ceil() as u32,
        s.height.ceil() as u32,
    )
}

#[test]
fn chrome_takes_the_interface_face_and_cells_keep_the_bundled_face() {
    let app = editor(1280.0, 800.0, 1.0);
    project_sheet(&app, &starter_workbook());
    let bundled = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render bundled face");
    app.global::<Theme>().set_ui_font_family(OTHER_FACE.into());
    let other = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render other face");

    let toolbar = pixel_rect(&app, "SheetsApp::action-toolbar");
    let cells = inside_header_bands(pixel_rect(&app, "SheetsApp::grid-surface"));
    assert!(
        region_is_drawn(&bundled, cells),
        "the grid must show the starter cells"
    );
    assert!(
        region_changed(&bundled, &other, toolbar),
        "toolbar text must follow the window's interface face"
    );
    assert!(
        !region_changed(&bundled, &other, cells),
        "cell text must keep the bundled document face"
    );
}

/// The grid surface holds the pinned column and row header bands as well as the
/// cells. At text scale 1.0 those bands are 36 px wide and 26 px tall (SheetHeaders),
/// and their labels use the window's chrome face, so the cells are measured
/// inside them.
fn inside_header_bands(grid: PixelRect) -> PixelRect {
    const BAND_W: u32 = 36;
    const BAND_H: u32 = 26;
    let (x, y, w, h) = grid;
    (
        x + BAND_W,
        y + BAND_H,
        w.saturating_sub(BAND_W),
        h.saturating_sub(BAND_H),
    )
}
