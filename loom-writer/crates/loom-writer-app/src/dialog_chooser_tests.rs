//! The template chooser's category icons, the find bar at double text and the
//! Save Changes buttons at double text. Each check renders the real window and
//! reads element geometry or the rendered pixels, never the source text.
use super::scale_surfaces_tests::{editor, open_surface, rects};
use super::*;
use i_slint_backend_testing::ElementHandle;
use loom_test_support::capture::snapshot_component;

/// x, y, width, height in window pixels.
type Box4 = (f32, f32, f32, f32);

fn rect_of(element: &ElementHandle) -> Box4 {
    let (p, s) = (element.absolute_position(), element.size());
    (p.x, p.y, s.width, s.height)
}

/// The window-level rectangle of the element with `label`, the lowest one when
/// the label appears more than once (a dialog sits below the toolbar).
fn lowest_with_label(app: &WriterApp, label: &str) -> Box4 {
    ElementHandle::find_by_accessible_label(app, label)
        .map(|element| rect_of(&element))
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .unwrap_or_else(|| panic!("no element labelled {label:?}"))
}

fn find_bar(app: &WriterApp) -> Box4 {
    lowest_with_label(app, "Find and replace")
}

/// The number of distinct rows that the controls inside `bar` occupy.
fn rows_inside(app: &WriterApp, bar: Box4) -> usize {
    let mut tops: Vec<f32> = rects(app)
        .into_iter()
        .filter(|(_, x, y, w, h)| {
            *x >= bar.0 - 0.5
                && *x + *w <= bar.0 + bar.2 + 0.5
                && *y >= bar.1 - 0.5
                && *y + *h <= bar.1 + bar.3 + 0.5
        })
        .map(|(_, _, y, _, _)| y)
        .collect();
    tops.sort_by(f32::total_cmp);
    let mut rows = 0;
    let mut last = f32::NEG_INFINITY;
    for top in tops {
        if top - last > 4.0 {
            rows += 1;
        }
        last = top;
    }
    rows
}

/// Each chooser category row draws a 16 px `Icon`. Its fallback shape, a bare
/// dash, puts ink on one or two pixel rows; a real glyph spans many more.
const CATEGORY_ICON: f32 = 16.0;
const MIN_GLYPH_ROWS: usize = 6;

/// How many rows of the icon box carry ink, judged against its own corner pixel.
fn ink_rows(image: &image::RgbaImage, x: f32, y: f32) -> usize {
    let (x0, y0) = (x.round() as u32, y.round() as u32);
    let size = CATEGORY_ICON as u32;
    let background = image.get_pixel(x0, y0).0;
    (0..size)
        .filter(|dy| {
            (0..size).any(|dx| {
                let pixel = image.get_pixel(x0 + dx, y0 + dy).0;
                let difference: i32 = (0..3)
                    .map(|c| (pixel[c] as i32 - background[c] as i32).abs())
                    .sum();
                difference > 90
            })
        })
        .count()
}

/// The 16 px icons of the chooser's category list: the sidebar sits at the
/// reading start of the centred 1160 px modal.
fn category_icons(app: &WriterApp, width: f32) -> Vec<(f32, f32)> {
    let modal_left = (width - 1160.0) / 2.0;
    ElementHandle::find_by_element_type_name(app, "Icon")
        .filter_map(|element| {
            let (p, s) = (element.absolute_position(), element.size());
            let is_category_icon = (s.width - CATEGORY_ICON).abs() < 0.5
                && (s.height - CATEGORY_ICON).abs() < 0.5
                && p.x >= modal_left
                && p.x < modal_left + 220.0
                && p.y >= 60.0;
            is_category_icon.then_some((p.x, p.y))
        })
        .collect()
}

#[test]
fn every_template_category_icon_is_a_real_glyph_not_a_dash() {
    let (width, height) = (1280.0f32, 800.0f32);
    let app = editor(width, height, 1.0);
    open_surface(&app, "templates");
    let image = snapshot_component(&app, width, height, 1.0).expect("render");
    let icons = category_icons(&app, width);
    assert_eq!(
        icons.len(),
        6,
        "the chooser shows six category icons, found {icons:?}"
    );
    for (x, y) in icons {
        let rows = ink_rows(&image, x, y);
        assert!(
            rows >= MIN_GLYPH_ROWS,
            "the category icon at ({x}, {y}) paints only {rows} pixel rows, the fallback dash"
        );
    }
}

#[test]
fn the_find_bar_keeps_the_match_case_toggle_clear_of_its_edge_at_double_text() {
    let (width, height) = (1280.0f32, 800.0f32);
    let app = editor(width, height, 2.0);
    open_surface(&app, "find");
    let _ = snapshot_component(&app, width, height, 1.0).expect("render");
    let bar = find_bar(&app);
    let toggle = lowest_with_label(&app, "Match case");
    assert!(
        toggle.0 - bar.0 >= 8.0,
        "the Match case toggle at x={} is only {} px from the find bar edge at x={}",
        toggle.0,
        toggle.0 - bar.0,
        bar.0
    );
    assert!(
        rows_inside(&app, bar) <= 2,
        "the find bar at 2x text takes {} rows, it may use one or two",
        rows_inside(&app, bar)
    );
}

#[test]
fn the_find_bar_with_replace_takes_at_most_three_rows_at_double_text() {
    let (width, height) = (1280.0f32, 800.0f32);
    let app = editor(width, height, 2.0);
    open_surface(&app, "find-replace");
    let _ = snapshot_component(&app, width, height, 1.0).expect("render");
    let bar = find_bar(&app);
    assert!(
        rows_inside(&app, bar) <= 3,
        "the find and replace bar at 2x text takes {} rows, the most is three",
        rows_inside(&app, bar)
    );
}

#[test]
fn save_changes_buttons_are_tall_enough_for_double_size_labels() {
    let (width, height) = (1280.0f32, 800.0f32);
    for scale in [1.0f32, 2.0] {
        let app = editor(width, height, scale);
        open_surface(&app, "save-changes");
        let _ = snapshot_component(&app, width, height, 1.0).expect("render");
        let needed = Theme::get(&app).get_scaled_control_height();
        for label in ["Save", "Discard", "Cancel"] {
            let (_, _, _, h) = lowest_with_label(&app, label);
            assert!(
                h >= needed - 0.5,
                "the {label} button at {scale}x is {h} px tall; its scaled label needs {needed} px"
            );
        }
    }
}
