//! Text-scale layout matrix: at every contract viewport and text scale, no
//! interactive element may leave the window and no two toolbar-row controls
//! may partly overlap. Rendering first forces a real layout pass.
use super::*;
use i_slint_backend_testing::{AccessibleRole, ElementHandle};
use loom_test_support::capture::{set_platform, snapshot_component};

const ROLES: [AccessibleRole; 6] = [
    AccessibleRole::Button,
    AccessibleRole::Checkbox,
    AccessibleRole::Combobox,
    AccessibleRole::Tab,
    AccessibleRole::TextInput,
    AccessibleRole::Switch,
];

fn interactive(app: &WriterApp) -> Vec<(String, f32, f32, f32, f32)> {
    let root = [
        "Rectangle",
        "WriterApp",
        "Window",
        "VerticalLayout",
        "FocusScope",
    ]
    .iter()
    .flat_map(|name| ElementHandle::find_by_element_type_name(app, name).collect::<Vec<_>>())
    .max_by_key(|candidate| candidate.query_descendants().find_all().len())
    .expect("root element");
    let mut out = Vec::new();
    for role in ROLES {
        for element in root
            .query_descendants()
            .match_accessible_role(role)
            .find_all()
        {
            let p = element.absolute_position();
            let s = element.size();
            if s.width > 0.5 && s.height > 0.5 {
                out.push((
                    element.accessible_label().unwrap_or_default().to_string(),
                    p.x,
                    p.y,
                    s.width,
                    s.height,
                ));
            }
        }
    }
    out
}

#[test]
fn chrome_fits_every_contract_viewport_and_text_scale() {
    for theme in ["light", "dark", "high-contrast"] {
        for (width, height) in [(1024.0f32, 720.0f32), (1280.0, 800.0)] {
            for scale in [1.0f32, 1.25, 1.5, 2.0] {
                set_platform();
                let app = WriterApp::new().expect("create app");
                app.window()
                    .set_size(slint::PhysicalSize::new(width as u32, height as u32));
                apply_theme(&app, theme);
                Theme::get(&app).set_text_scale(scale);
                apply_layout_breakpoints(&app, width as u32);
                let _ = snapshot_component(&app, width, height, 1.0).expect("render");
                let items = interactive(&app);
                assert!(!items.is_empty(), "controls exist");
                for (label, x, y, w, h) in &items {
                    // The page is a scrolling surface and may extend past the window.
                    if label.starts_with("Document body") {
                        continue;
                    }
                    assert!(
                        *x >= -1.0 && *y >= -1.0 && x + w <= width + 1.0 && y + h <= height + 1.0,
                        "{label:?} leaves the {width}x{height} window at {scale}x/{theme}: ({x}, {y}, {w}, {h})"
                    );
                }
                // Toolbar band: partial overlap between neighbours is a collision.
                let band: Vec<_> = items.iter().filter(|i| i.2 + i.4 <= 76.0).collect();
                for (i, a) in band.iter().enumerate() {
                    for b in band.iter().skip(i + 1) {
                        let ox = (a.1 + a.3).min(b.1 + b.3) - a.1.max(b.1);
                        let oy = (a.2 + a.4).min(b.2 + b.4) - a.2.max(b.2);
                        let contained = (a.1 <= b.1
                            && a.1 + a.3 >= b.1 + b.3
                            && a.2 <= b.2
                            && a.2 + a.4 >= b.2 + b.4)
                            || (b.1 <= a.1
                                && b.1 + b.3 >= a.1 + a.3
                                && b.2 <= a.2
                                && b.2 + b.4 >= a.2 + a.4);
                        assert!(
                            contained || ox <= 1.0 || oy <= 1.0,
                            "toolbar controls {:?} and {:?} overlap at {scale}x/{theme} {width}x{height}",
                            a.0,
                            b.0
                        );
                    }
                }
            }
        }
    }
}
