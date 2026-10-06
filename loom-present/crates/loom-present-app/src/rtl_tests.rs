//! Right-to-left layout: the main window and each surface must be the mirror
//! image of the left-to-right layout, with nothing clipped or overlapping.
//! Document content (the slide itself) is symmetric by construction, so it is
//! compared like everything else.
use super::keyboard_flow_tests::launched;
use super::scale_surfaces_tests::{open_surface, rects, ROLES};
use super::*;
use i_slint_backend_testing::ElementHandle;
use loom_test_support::capture::snapshot_component;
use loom_test_support::mirror::{inside_window, mirror_difference};

/// Slide content is the document, not application chrome: a slide keeps its own
/// left-to-right geometry, so the text boxes drawn on the canvas and in the
/// thumbnails are compared by the dedicated test below instead.
const SLIDE_CONTENT: [&str; 7] = [
    "Create without compromise",
    "A private, native creative studio",
    "The creative system",
    "Writer, Sheets, Present",
    "Built around ownership",
    "No required account",
    "Body text",
];

const SURFACES: [&str; 13] = [
    "none",
    "local-menu-0",
    "local-menu-2",
    "local-menu-3",
    "save-changes",
    "palette",
    "themes",
    "notes",
    "inspector",
    "inspector-selection",
    "view-menu",
    "zoom-menu",
    "overflow",
];

fn render(
    surface: &str,
    width: f32,
    height: f32,
    rtl: bool,
    scale: f32,
) -> Vec<scale_surfaces_tests::Rect> {
    let session = launched();
    let app = session.app;
    configure_direction(&app, rtl);
    apply_theme(&app, "light");
    Theme::get(&app).set_text_scale(scale);
    configure_responsive_layout(&app, (width as u32, height as u32));
    if surface == "overflow" {
        app.set_overflow_toolbar(true);
    }
    if let Some(index) = surface.strip_prefix("local-menu-") {
        app.set_local_menu_open_index(index.parse().expect("menu index"));
    } else if surface == "view-menu" || surface == "zoom-menu" {
        // Open it the way a user does, so the menu anchors to the clicked item.
        let _ = snapshot_component(&app, width, height, 1.0).expect("render");
        let label = if surface == "view-menu" {
            "View"
        } else {
            "Zoom"
        };
        let item = ElementHandle::find_by_accessible_label(&app, label)
            .find(|e| e.absolute_position().y > 30.0)
            .expect("toolbar item");
        item.invoke_accessible_default_action();
    } else {
        open_surface(&app, surface);
    }
    let image = snapshot_component(&app, width, height, 1.0).expect("render");
    if let Ok(dir) = std::env::var("LOOM_RTL_DUMP") {
        let side = if rtl { "rtl" } else { "ltr" };
        let _ = image.save(format!(
            "{dir}/present-{surface}-{width}-{scale}-{side}.png"
        ));
    }
    rects(&app, &ROLES)
}

#[test]
fn every_surface_is_the_mirror_image_in_right_to_left() {
    let mut failures = Vec::new();
    for surface in SURFACES {
        for (width, height) in [(1024.0f32, 720.0f32), (1280.0, 800.0)] {
            for scale in [1.0f32, 1.5] {
                let ltr = render(surface, width, height, false, scale);
                let rtl = render(surface, width, height, true, scale);
                if let Some(problem) = inside_window(&rtl, width, height) {
                    failures.push(format!("{surface} {width} x{scale}: {problem}"));
                }
                if let Some(problem) = mirror_difference(&ltr, &rtl, width, &SLIDE_CONTENT) {
                    failures.push(format!("{surface} {width} x{scale}:\n  {problem}"));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
