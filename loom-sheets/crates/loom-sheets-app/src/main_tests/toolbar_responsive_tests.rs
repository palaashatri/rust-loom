//! Toolbar fit across window sizes and text scales, inspector icon scaling, and
//! the accessible state of the Border switch.

use super::*;
use i_slint_backend_testing::{AccessibleRole, ElementHandle};

const TOOLBAR_ITEMS: [&str; 11] = [
    "View",
    "Zoom",
    "Add Sheet",
    "Chart",
    "Table",
    "Text",
    "Shape",
    "Image",
    "Export",
    "Format",
    "Organize",
];

fn sheets_window(text_scale: f32) -> SheetsApp {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    app.set_template_text_scale(text_scale);
    toolbar_commands::start_with_inspector_open(&app);
    toolbar_commands::wire(&app);
    app
}

/// The toolbar button named `label`, told apart from the menu bar by its height.
fn toolbar_button(app: &SheetsApp, label: &str) -> Option<ElementHandle> {
    let min_height = if label == "More actions" { 20.0 } else { 44.0 };
    ElementHandle::find_by_accessible_label(app, label).find(|e| {
        e.accessible_role() == Some(AccessibleRole::Button) && e.size().height >= min_height
    })
}

fn first_named(app: &SheetsApp, label: &str) -> ElementHandle {
    ElementHandle::find_by_accessible_label(app, label)
        .next()
        .unwrap_or_else(|| panic!("no element named {label:?}"))
}

#[test]
fn toolbar_items_and_more_actions_stay_inside_the_window_at_every_size_and_text_scale() {
    for text_scale in [1.0_f32, 1.5, 2.0] {
        let app = sheets_window(text_scale);
        for width in (900_u32..=1920).step_by(20) {
            app.window().set_size(PhysicalSize::new(width, 800));
            apply_layout_breakpoints(&app, width);
            let _ = snapshot_component(&app, width as f32, 800.0, 1.0).expect("render");
            let inside = |item: &ElementHandle| {
                let left = item.absolute_position().x;
                left >= -0.5 && left + item.size().width <= width as f32 + 0.5
            };
            let more = toolbar_button(&app, "More actions").unwrap_or_else(|| {
                panic!("More actions is missing at {width}px, text scale {text_scale}")
            });
            assert!(
                inside(&more),
                "More actions is outside the {width}px window at text scale {text_scale}"
            );
            for label in TOOLBAR_ITEMS {
                if let Some(item) = toolbar_button(&app, label) {
                    assert!(
                        inside(&item),
                        "{label} is cut off at {width}px, text scale {text_scale}: x={} w={}",
                        item.absolute_position().x,
                        item.size().width
                    );
                }
            }
        }
    }
}

#[test]
fn inspector_icon_buttons_grow_with_text_scale_and_the_title_stays_on_one_line() {
    let app = sheets_window(1.0);
    app.set_inspector_tab(1);
    app.window().set_size(PhysicalSize::new(1280, 800));
    apply_layout_breakpoints(&app, 1280);
    let measure = |scale: f32| {
        app.set_template_text_scale(scale);
        let _ = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render");
        let bold = first_named(&app, "Toggle bold formatting (Cell inspector)");
        let title = first_named(&app, "A1 selection");
        (bold.size().height, title.size().height)
    };
    let (bold_1, title_1) = measure(1.0);
    let (bold_2, title_2) = measure(2.0);
    assert!(
        bold_2 >= bold_1 * 1.9,
        "the bold icon button ignores text scale: {bold_1} px -> {bold_2} px"
    );
    assert!(
        title_2 <= title_1 * 2.5,
        "the inspector title wraps at text scale 2: {title_1} px -> {title_2} px"
    );
}

#[test]
fn the_border_switch_reports_its_checked_state_to_assistive_technology() {
    let app = sheets_window(1.0);
    app.set_inspector_tab(1);
    app.window().set_size(PhysicalSize::new(1280, 800));
    apply_layout_breakpoints(&app, 1280);
    let _ = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render");
    let border = ElementHandle::find_by_accessible_label(&app, "Border")
        .find(|e| e.accessible_role() == Some(AccessibleRole::Switch))
        .expect("Border switch");
    assert_eq!(border.accessible_checkable(), Some(true));
    assert_eq!(border.accessible_checked(), Some(false));
}
