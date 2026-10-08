//! Right-to-left layout: the main window and each surface must be the mirror
//! image of the left-to-right layout, with nothing clipped or overlapping. The
//! grid itself (column A at the left, cells in sheet order) is document content
//! and keeps its order, so the grid's own controls are exempt.
use super::keyboard_flow_tests::launched;
use super::scale_surfaces_tests::{open_surface, rects, Rect};
use super::*;
use i_slint_backend_testing::ElementHandle;
use loom_test_support::capture::snapshot_component;
use loom_test_support::mirror::{inside_window, mirror_difference};

/// The grid draws document content: column A stays at the left edge and cells
/// keep the order of the sheet, so the controls that belong to cells (the fill
/// handle and the table-select corner) are not mirrored.
const GRID_CONTENT: [&str; 2] = ["Selected cell", "Table select"];

const SURFACES: [&str; 14] = [
    "none",
    "local-menu-0",
    "local-menu-3",
    "save-changes",
    "save-changes-closing",
    "xlsx-warning",
    "templates",
    "palette",
    "inspector",
    "menu-View",
    "menu-Zoom",
    "menu-Table",
    "menu-Export",
    "menu-More actions",
];

fn render(surface: &str, width: f32, height: f32, rtl: bool, scale: f32) -> Vec<Rect> {
    let session = launched(&[
        ("A1", "Item"),
        ("B1", "Amount"),
        ("A2", "Rent"),
        ("B2", "1200"),
    ]);
    let app = session.app;
    configure_direction(&app, rtl);
    apply_theme(&app, "light");
    app.set_template_text_scale(scale);
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
        let _ = image.save(format!("{dir}/sheets-{surface}-{width}-{scale}-{side}.png"));
    }
    rects(&app)
}

/// Every surface is checked at 1280x800 and normal text size. A launch of the
/// whole window is slow in a debug build, so the other sizes and 1.5x text
/// check the main window and the surfaces that change shape with them.
const OTHER_SIZES: [&str; 4] = ["none", "inspector", "templates", "menu-Export"];

#[test]
fn every_surface_is_the_mirror_image_in_right_to_left() {
    let mut failures = Vec::new();
    for surface in SURFACES {
        let mut configs = vec![(1280.0f32, 800.0f32, 1.0f32)];
        if OTHER_SIZES.contains(&surface) {
            configs.extend([(1024.0, 720.0, 1.0), (1280.0, 800.0, 1.5)]);
        }
        for (width, height, scale) in configs {
            let ltr = render(surface, width, height, false, scale);
            let rtl = render(surface, width, height, true, scale);
            if let Some(problem) = inside_window(&rtl, width, height) {
                if !surface.starts_with("templates") {
                    failures.push(format!("{surface} {width} x{scale}: {problem}"));
                }
            }
            if let Some(problem) = mirror_difference(&ltr, &rtl, width, &GRID_CONTENT) {
                failures.push(format!(
                    "{surface} {width} x{scale}:
  {problem}"
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{}",
        failures.join(
            "
"
        )
    );
}

/// Geometry of the bottom status band and the sheet-tab rules, in logical
/// pixels: each text in the status band, and each one-pixel rule that sits in
/// the tab strip's band.
fn status_and_tab_geometry(rtl: bool, width: f32, height: f32) -> (Vec<Rect>, Vec<Rect>, Rect) {
    let session = launched(&[("A1", "Item"), ("B1", "Amount")]);
    let app = session.app;
    configure_direction(&app, rtl);
    apply_theme(&app, "light");
    apply_layout_breakpoints(&app, width as u32);
    let _ = snapshot_component(&app, width, height, 1.0).expect("render");
    let band_top = height - 40.0;
    let mut status = Vec::new();
    for element in ElementHandle::find_by_element_type_name(&app, "Text") {
        let (p, s) = (element.absolute_position(), element.size());
        if p.y > band_top && s.width > 0.5 && p.x > -1.0 {
            status.push((format!("text{:.0}", s.width), p.x, p.y, s.width, s.height));
        }
    }
    let tabs = ElementHandle::find_by_accessible_label(&app, "Tabs")
        .map(|e| {
            let (p, s) = (e.absolute_position(), e.size());
            ("Tabs".to_string(), p.x, p.y, s.width, s.height)
        })
        .find(|r| r.4 > 20.0)
        .expect("sheet tab strip");
    let mut rules = Vec::new();
    for element in ElementHandle::find_by_element_type_name(&app, "Rectangle") {
        let (p, s) = (element.absolute_position(), element.size());
        // Strictly inside the strip: the toolbar's own border sits one pixel above it.
        let in_tab_band = p.y >= tabs.2 && p.y + s.height <= tabs.2 + tabs.4 + 0.5;
        if in_tab_band && s.height > 0.5 && s.height < 1.5 && s.width > 0.5 {
            rules.push(("rule".to_string(), p.x, p.y, s.width, s.height));
        }
    }
    (status, rules, tabs)
}

/// The status band reads Ready at the reading start, counts in the middle and
/// the connection state at the reading end. In right-to-left it must be the
/// mirror image, and the sheet-tab rule may only span the tab strip itself.
#[test]
fn status_bar_and_sheet_tab_rule_mirror_in_right_to_left() {
    let (width, height) = (1280.0f32, 800.0f32);
    let (ltr_status, ltr_rules, ltr_tabs) = status_and_tab_geometry(false, width, height);
    let (rtl_status, rtl_rules, rtl_tabs) = status_and_tab_geometry(true, width, height);
    // One sheet is one tab: the strip, and so its rule, must not stretch across
    // the window. A 640 px rule under a single tab reads as a stray line.
    for (label, tabs) in [("LTR", ltr_tabs.clone()), ("RTL", rtl_tabs.clone())] {
        assert!(
            tabs.3 <= 200.0,
            "{label}: the tab strip spans {} px for one sheet and should hug its tab",
            tabs.3
        );
    }

    if let Some(problem) = mirror_difference(&ltr_status, &rtl_status, width, &[]) {
        panic!("status bar does not mirror in right-to-left:\n  {problem}");
    }
    for (label, rules, tabs) in [("LTR", &ltr_rules, ltr_tabs), ("RTL", &rtl_rules, rtl_tabs)] {
        for rule in rules.iter() {
            let (x, w) = (rule.1, rule.3);
            assert!(
                x >= tabs.1 - 1.0 && x + w <= tabs.1 + tabs.3 + 1.0,
                "{label}: tab rule spans x {x}..{} outside the tab strip {}..{}",
                x + w,
                tabs.1,
                tabs.1 + tabs.3
            );
        }
    }
    if let Some(problem) = mirror_difference(&ltr_rules, &rtl_rules, width, &[]) {
        panic!("sheet-tab rule does not mirror in right-to-left:\n  {problem}");
    }
}
