//! Right-to-left layout: the main window and each surface must be the mirror
//! image of the left-to-right layout, with nothing clipped or overlapping. The
//! page text itself is document content and keeps its own direction, so the
//! page body is exempt from the comparison.
use super::keyboard_flow_tests::launched;
use super::scale_surfaces_tests::{open_surface, rects, Rect};
use super::*;
use i_slint_backend_testing::ElementHandle;
use loom_test_support::capture::snapshot_component;
use loom_test_support::mirror::{inside_window, mirror_difference};

const SURFACES: [&str; 17] = [
    "none",
    "local-menu-0",
    "local-menu-2",
    "save-changes",
    "save-changes-closing",
    "templates",
    "palette",
    "find",
    "find-replace",
    "inspector",
    "inspector-document",
    "navigator",
    "menu-View",
    "menu-Zoom",
    "menu-Insert",
    "menu-Export",
    "menu-More actions",
];

fn render(surface: &str, width: f32, height: f32, rtl: bool, scale: f32) -> Vec<Rect> {
    let session = launched("");
    let app = session.app;
    configure_direction(&app, rtl);
    apply_theme(&app, "light");
    Theme::get(&app).set_text_scale(scale);
    apply_layout_breakpoints(&app, width as u32);
    if let Some(index) = surface.strip_prefix("local-menu-") {
        app.set_local_menu_open_index(index.parse().expect("menu index"));
    } else if let Some(label) = surface.strip_prefix("menu-") {
        // Open it the way a user does, so the menu anchors to the clicked item.
        let _ = snapshot_component(&app, width, height, 1.0).expect("render");
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
        let _ = image.save(format!("{dir}/writer-{surface}-{width}-{scale}-{side}.png"));
    }
    rects(&app)
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
                    if !surface.starts_with("templates") {
                        failures.push(format!("{surface} {width} x{scale}: {problem}"));
                    }
                }
                if let Some(problem) = mirror_difference(&ltr, &rtl, width, &[]) {
                    failures.push(format!("{surface} {width} x{scale}:\n  {problem}"));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
