//! Dialogs, menus, drawers and the presenter window at 1.0/1.5/2.0 text scale:
//! every control must stay inside its window and no two text labels may
//! partly overlap. Each surface is opened by setting the real window state.
use super::*;
use i_slint_backend_testing::{AccessibleRole, ElementHandle};
use loom_test_support::capture::{set_platform, snapshot_component};

type Rect = (String, f32, f32, f32, f32);

const ROLES: [AccessibleRole; 7] = [
    AccessibleRole::Button,
    AccessibleRole::Checkbox,
    AccessibleRole::Combobox,
    AccessibleRole::Tab,
    AccessibleRole::TextInput,
    AccessibleRole::Switch,
    AccessibleRole::ListItem,
];

fn rects<C: slint::ComponentHandle>(component: &C, roles: &[AccessibleRole]) -> Vec<Rect> {
    let roots: Vec<_> = [
        "Rectangle",
        "PresentApp",
        "PresenterWindow",
        "Window",
        "VerticalLayout",
        "FocusScope",
    ]
    .iter()
    .flat_map(|name| ElementHandle::find_by_element_type_name(component, name).collect::<Vec<_>>())
    .collect();
    let mut out: Vec<Rect> = Vec::new();
    for role in roles {
        for element in roots.iter().flat_map(|root| {
            root.query_descendants()
                .match_accessible_role(*role)
                .find_all()
        }) {
            let (p, s) = (element.absolute_position(), element.size());
            if s.width > 0.5 && s.height > 0.5 {
                let item = (
                    element.accessible_label().unwrap_or_default().to_string(),
                    p.x,
                    p.y,
                    s.width,
                    s.height,
                );
                if !out.iter().any(|o| {
                    o.0 == item.0 && (o.1 - item.1).abs() < 0.5 && (o.2 - item.2).abs() < 0.5
                }) {
                    out.push(item);
                }
            }
        }
    }
    out
}

fn overlap(a: &Rect, b: &Rect) -> bool {
    let ox = (a.1 + a.3).min(b.1 + b.3) - a.1.max(b.1);
    let oy = (a.2 + a.4).min(b.2 + b.4) - a.2.max(b.2);
    let inside = |p: &Rect, q: &Rect| {
        p.1 <= q.1 + 1.0
            && p.2 <= q.2 + 1.0
            && p.1 + p.3 >= q.1 + q.3 - 1.0
            && p.2 + p.4 >= q.2 + q.4 - 1.0
    };
    ox > 1.0 && oy > 1.0 && !inside(a, b) && !inside(b, a)
}

fn assert_clean(items: &[Rect], background: &[Rect], width: f32, height: f32, what: &str) {
    for (label, x, y, w, h) in items {
        assert!(
            *x >= -1.0 && *y >= -1.0 && x + w <= width + 1.0 && y + h <= height + 1.0,
            "{what}: {label:?} leaves the {width}x{height} window: ({x}, {y}, {w}, {h})"
        );
    }
    for (i, a) in items.iter().enumerate() {
        for b in items.iter().skip(i + 1) {
            let behind = |r: &Rect| background.iter().any(|g| g == r);
            if !background.is_empty() && (behind(a) || behind(b)) {
                continue;
            }
            assert!(!overlap(a, b), "{what}: {:?} and {:?} overlap", a.0, b.0);
        }
    }
}

fn open_surface(app: &PresentApp, surface: &str) {
    match surface {
        "save-changes" => {
            app.set_save_changes_document("Quarterly review".into());
            app.set_save_changes_open(true);
        }
        "palette" => {
            let item = |label: &str, shortcut: &str| CommandPaletteItem {
                id: label.into(),
                label: label.into(),
                shortcut: shortcut.into(),
                enabled: true,
            };
            app.set_palette_commands(slint::ModelRc::new(slint::VecModel::from(vec![
                item("Add Slide", "Ctrl+M"),
                item("Export as PDF", "Ctrl+Shift+E"),
                item("Start or Exit Slideshow", "F5"),
            ])));
            app.set_palette_open(true);
        }
        "themes" => app.set_theme_chooser_open(true),
        "notes" => {
            app.set_slide_notes(
                "Remember to mention the pricing change and the new timeline.".into(),
            );
            app.set_show_notes_drawer(true);
        }
        "inspector" => app.set_show_inspector(true),
        "inspector-selection" => {
            app.set_show_inspector(true);
            app.set_selection_count(1);
            app.set_active_element_label("Title text with a long descriptive name".into());
            app.set_active_element_content("Create without compromise".into());
        }
        "view-menu" => app.set_view_menu_open(true),
        "zoom-menu" => app.set_zoom_menu_open(true),
        "overflow" => app.set_toolbar_overflow_open(true),
        _ => {}
    }
}

#[test]
fn dialogs_menus_and_drawers_fit_at_every_text_scale() {
    for surface in [
        "none",
        "save-changes",
        "palette",
        "themes",
        "notes",
        "inspector",
        "inspector-selection",
        "view-menu",
        "zoom-menu",
        "overflow",
    ] {
        for (width, height) in [(1024.0f32, 720.0f32), (1280.0, 800.0)] {
            for scale in [1.0f32, 1.5, 2.0] {
                set_platform();
                let app = PresentApp::new().expect("create app");
                app.window()
                    .set_size(slint::PhysicalSize::new(width as u32, height as u32));
                apply_theme(&app, "light");
                Theme::get(&app).set_text_scale(scale);
                configure_responsive_layout(&app, (width as u32, height as u32));
                if surface == "overflow" {
                    app.set_overflow_toolbar(true);
                }
                open_surface(&app, surface);
                let image = snapshot_component(&app, width, height, 1.0).expect("render");
                if let Ok(dir) = std::env::var("LOOM_SCALE_DUMP") {
                    let _ = image.save(format!("{dir}/present-{surface}-{width}-{scale}.png"));
                }
                let items = rects(&app, &ROLES);
                let modal =
                    !["none", "notes", "inspector", "inspector-selection"].contains(&surface);
                let background = if modal {
                    rects_without_surface(width, height, scale)
                } else {
                    Vec::new()
                };
                assert!(
                    items.len() > background.len() || surface == "none" || surface == "overflow",
                    "{surface} at {scale}x shows no controls of its own"
                );
                assert_clean(
                    &items,
                    &background,
                    width,
                    height,
                    &format!("{surface} at {scale}x"),
                );
            }
        }
    }
}

#[test]
fn presenter_window_inherits_text_scale_and_fits_its_minimum_size() {
    set_platform();
    let app = PresentApp::new().expect("create app");
    Theme::get(&app).set_text_scale(2.0);
    let window = PresenterWindow::new().expect("presenter window");
    Theme::get(&window).set_text_scale(Theme::get(&app).get_text_scale());
    window.set_slide_title("A fairly long slide title that has to wrap or elide".into());
    window
        .set_notes("Remember the pricing change, the new timeline and the open questions.".into());
    window.set_next_title("Next slide".into());
    window.set_has_next(true);
    window.set_has_previous(true);
    for scale in [1.0f32, 1.5, 2.0] {
        Theme::get(&window).set_text_scale(scale);
        for (w, h) in [(960.0f32, 600.0f32), (640.0, 420.0)] {
            let _ = snapshot_component(&window, w, h, 1.0).expect("render presenter");
            let items = rects(&window, &ROLES);
            assert!(items.len() >= 4, "presenter buttons exist");
            assert_clean(&items, &[], w, h, &format!("presenter {w}x{h} at {scale}x"));
        }
    }
}

/// Controls of the plain editor at the same size and scale, i.e. what a modal
/// surface covers.
fn rects_without_surface(width: f32, height: f32, scale: f32) -> Vec<Rect> {
    set_platform();
    let app = PresentApp::new().expect("create app");
    app.window()
        .set_size(slint::PhysicalSize::new(width as u32, height as u32));
    apply_theme(&app, "light");
    Theme::get(&app).set_text_scale(scale);
    configure_responsive_layout(&app, (width as u32, height as u32));
    let _ = snapshot_component(&app, width, height, 1.0).expect("render");
    rects(&app, &ROLES)
}

#[test]
fn toolbar_captions_scale_and_the_row_grows_to_hold_them() {
    let mut previous = 0.0f32;
    for scale in [1.0f32, 1.5, 2.0] {
        let items = rects_without_surface(1280.0, 800.0, scale);
        let view = items
            .iter()
            .find(|i| i.0 == "View")
            .expect("View toolbar item");
        // icon 18 + gap 2 + padding 8 + one caption line (about 1.2 em at 10 px * scale)
        let needed = 28.0 + 12.0 * scale;
        assert!(
            view.4 + 0.5 >= needed,
            "caption clipped at {scale}x: item {} < {needed}",
            view.4
        );
        assert!(
            view.4 > previous,
            "toolbar row must grow with the caption at {scale}x"
        );
        previous = view.4;
    }
}
