//! Dialogs, menus, the find bar, the command palette and the inspector at
//! 1.0/1.5/2.0 text scale: every control must stay inside the window and no two
//! controls may partly overlap. Each surface is opened by setting real state.
use super::*;
use i_slint_backend_testing::{AccessibleRole, ElementHandle};

pub(super) type Rect = (String, f32, f32, f32, f32);

const ROLES: [AccessibleRole; 7] = [
    AccessibleRole::Button,
    AccessibleRole::Checkbox,
    AccessibleRole::Combobox,
    AccessibleRole::Tab,
    AccessibleRole::TextInput,
    AccessibleRole::Switch,
    AccessibleRole::ListItem,
];

pub(super) fn rects(app: &WriterApp) -> Vec<Rect> {
    let roots: Vec<_> = [
        "Rectangle",
        "WriterApp",
        "Window",
        "VerticalLayout",
        "FocusScope",
    ]
    .iter()
    .flat_map(|name| ElementHandle::find_by_element_type_name(app, name).collect::<Vec<_>>())
    .collect();
    let mut out: Vec<Rect> = Vec::new();
    for role in ROLES {
        for element in roots.iter().flat_map(|root| {
            root.query_descendants()
                .match_accessible_role(role)
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
                // The page is a scrolling surface and may extend past the window.
                if item.0.starts_with("Document body") {
                    continue;
                }
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
    // The template chooser is a scrolling list: cards below the fold are reached
    // by scrolling, so only their horizontal extent is checked.
    let scrolls = what.starts_with("templates");
    for (label, x, y, w, h) in items {
        assert!(
            *x >= -1.0 && *y >= -1.0 && x + w <= width + 1.0 && (scrolls || y + h <= height + 1.0),
            "{what}: {label:?} leaves the {width}x{height} window: ({x}, {y}, {w}, {h})"
        );
    }
    if scrolls {
        return;
    }
    for (i, a) in items.iter().enumerate() {
        for b in items.iter().skip(i + 1) {
            let behind = |r: &Rect| background.contains(r);
            if !background.is_empty() && (behind(a) || behind(b)) {
                continue;
            }
            assert!(!overlap(a, b), "{what}: {a:?} and {b:?} overlap");
        }
    }
}

pub(super) fn editor(width: f32, height: f32, scale: f32) -> WriterApp {
    set_platform();
    let app = WriterApp::new().expect("create app");
    app.window()
        .set_size(PhysicalSize::new(width as u32, height as u32));
    apply_theme(&app, "light");
    Theme::get(&app).set_text_scale(scale);
    apply_layout_breakpoints(&app, width as u32);
    app
}

pub(super) fn open_surface(app: &WriterApp, surface: &str) {
    match surface {
        "save-changes" => {
            app.set_save_changes_document("Quarterly report".into());
            app.set_save_changes_open(true);
        }
        "save-changes-closing" => {
            app.set_save_changes_document("Quarterly report".into());
            app.set_save_changes_closing(true);
            app.set_save_changes_open(true);
        }
        "templates" => app.set_template_chooser_open(true),
        "palette" => {
            let item = |label: &str, shortcut: &str| CommandPaletteItem {
                id: label.into(),
                label: label.into(),
                shortcut: shortcut.into(),
                enabled: true,
            };
            app.set_palette_commands(slint::ModelRc::new(VecModel::from(vec![
                item("Insert Table", ""),
                item("Export as PDF", "Ctrl+Shift+E"),
                item("Find and Replace", "Ctrl+H"),
            ])));
            app.set_palette_open(true);
        }
        "find" => {
            let find = app.global::<FindBar>();
            find.set_open(true);
            find.set_query("pricing".into());
            find.set_status("2 of 14".into());
        }
        "find-replace" => {
            let find = app.global::<FindBar>();
            find.set_open(true);
            find.set_show_replace(true);
            find.set_query("pricing".into());
            find.set_replacement("cost".into());
            find.set_status("14 replaced".into());
        }
        "inspector" => app.set_show_inspector(true),
        "inspector-document" => {
            app.set_show_inspector(true);
            app.set_inspector_tab(1);
            app.set_comment_entries(slint::ModelRc::new(VecModel::from(vec![
                WriterCommentEntry {
                    id: "c1".into(),
                    author: "Palaash".into(),
                    body: "Check the pricing figures against the latest quote.".into(),
                    resolved: false,
                    orphaned: false,
                },
            ])));
        }
        "navigator" => app.set_show_navigator(true),
        "menu-1" | "menu-2" | "menu-3" | "menu-4" | "menu-5" | "menu-6" | "menu-7" => {
            app.set_toolbar_menu(surface[5..].parse().expect("menu kind"));
        }
        "overflow" => app.set_toolbar_overflow_open(true),
        _ => {}
    }
}

#[test]
fn dialogs_menus_find_bar_and_palette_fit_at_every_text_scale() {
    for surface in [
        "none",
        "save-changes",
        "save-changes-closing",
        "templates",
        "palette",
        "find",
        "find-replace",
        "inspector",
        "inspector-document",
        "navigator",
        "menu-1",
        "menu-2",
        "menu-3",
        "menu-4",
        "menu-5",
        "menu-6",
        "menu-7",
        "overflow",
    ] {
        for (width, height) in [(1024.0f32, 720.0f32), (1280.0, 800.0)] {
            for scale in [1.0f32, 1.5, 2.0] {
                let app = editor(width, height, scale);
                open_surface(&app, surface);
                let image = snapshot_component(&app, width, height, 1.0).expect("render");
                if let Ok(dir) = std::env::var("LOOM_SCALE_DUMP") {
                    let _ = image.save(format!("{dir}/writer-{surface}-{width}-{scale}.png"));
                }
                let items = rects(&app);
                let docked = [
                    "none",
                    "inspector",
                    "inspector-document",
                    "navigator",
                    "find",
                    "find-replace",
                ]
                .contains(&surface);
                let background = if docked {
                    Vec::new()
                } else {
                    let plain = editor(width, height, scale);
                    let _ = snapshot_component(&plain, width, height, 1.0).expect("render");
                    rects(&plain)
                };
                assert!(
                    docked || items.len() > background.len() || surface == "overflow",
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
fn toolbar_captions_scale_and_the_row_grows_to_hold_them() {
    let mut previous = 0.0f32;
    for scale in [1.0f32, 1.5, 2.0] {
        let app = editor(1440.0, 900.0, scale);
        let _ = snapshot_component(&app, 1440.0, 900.0, 1.0).expect("render");
        let item = rects(&app)
            .iter()
            .filter(|i| i.2 < 80.0 && i.4 > 40.0)
            .map(|i| i.4)
            .fold(0.0f32, f32::max);
        // icon 18 + gap 2 + padding 8 + one caption line (about 1.2 em at 10 px * scale)
        let needed = 28.0 + 12.0 * scale;
        assert!(
            item + 0.5 >= needed,
            "caption clipped at {scale}x: item {item} < {needed}"
        );
        assert!(
            item > previous,
            "toolbar row must grow with the caption at {scale}x"
        );
        previous = item;
    }
}
