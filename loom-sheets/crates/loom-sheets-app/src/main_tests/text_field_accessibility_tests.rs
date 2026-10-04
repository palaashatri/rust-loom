use super::*;
use i_slint_backend_testing::{AccessibleRole, ElementHandle};

const INTERACTIVE_ROLES: [AccessibleRole; 6] = [
    AccessibleRole::Button,
    AccessibleRole::TextInput,
    AccessibleRole::Checkbox,
    AccessibleRole::Combobox,
    AccessibleRole::Slider,
    AccessibleRole::Tab,
];

fn render(app: &SheetsApp) {
    let _ = snapshot_component(app, 1280.0, 800.0, 1.0).expect("render the accessibility tree");
}

fn test_app() -> SheetsApp {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    app.window().set_size(PhysicalSize::new(1280, 800));
    render(&app);
    app
}

/// Describe every interactive element that has no accessible name.
fn unnamed(app: &SheetsApp) -> Vec<String> {
    // Elements with a type name that is not unique are probed; the root that
    // reaches the most descendants is the main-window content tree.
    let root = [
        "Rectangle",
        "SheetsApp",
        "Window",
        "VerticalLayout",
        "FocusScope",
    ]
    .iter()
    .flat_map(|name| ElementHandle::find_by_element_type_name(app, name).collect::<Vec<_>>())
    .max_by_key(|candidate| candidate.query_descendants().find_all().len())
    .expect("root element");
    let mut reached = 0;
    let mut bad = Vec::new();
    for role in INTERACTIVE_ROLES {
        for element in root
            .query_descendants()
            .match_accessible_role(role)
            .find_all()
        {
            reached += 1;
            if element
                .accessible_label()
                .unwrap_or_default()
                .trim()
                .is_empty()
            {
                bad.push(format!(
                    "{role:?} {:?} at {:?} size {:?}",
                    element.type_name(),
                    element.absolute_position(),
                    element.size()
                ));
            }
        }
    }
    assert!(reached > 5, "the walk must reach the toolbar controls");
    bad
}

#[test]
fn name_box_and_formula_bar_are_named_and_described_for_assistive_technology() {
    let app = test_app();
    for (label, description) in [
        (
            "Name box",
            "Type a cell address such as D50 or B2:C3 and press Enter",
        ),
        (
            "Formula bar",
            "Edit the selected cell's value or formula. Enter commits, Escape cancels",
        ),
    ] {
        let matches: Vec<_> = ElementHandle::find_by_accessible_label(&app, label).collect();
        assert_eq!(matches.len(), 1, "exactly one {label:?} in the tree");
        let field = &matches[0];
        assert_eq!(field.accessible_role(), Some(AccessibleRole::TextInput));
        assert_eq!(
            field.accessible_description().unwrap().as_str(),
            description
        );
    }
}

#[test]
fn every_interactive_element_has_an_accessible_name() {
    let app = test_app();
    let bad = unnamed(&app);
    assert!(bad.is_empty(), "unnamed (default view): {bad:#?}");

    app.set_show_inspector(true);
    for tab in [0, 1] {
        app.set_inspector_tab(tab);
        render(&app);
        let bad = unnamed(&app);
        assert!(bad.is_empty(), "unnamed (inspector tab {tab}): {bad:#?}");
    }

    app.set_palette_open(true);
    render(&app);
    let bad = unnamed(&app);
    assert!(bad.is_empty(), "unnamed (palette open): {bad:#?}");
    app.set_palette_open(false);

    for kind in 1..=5 {
        app.set_toolbar_menu(kind);
        render(&app);
        let bad = unnamed(&app);
        assert!(
            bad.is_empty(),
            "unnamed (toolbar menu {kind} open): {bad:#?}"
        );
    }
    app.set_toolbar_menu(0);
    app.set_toolbar_overflow_open(true);
    render(&app);
    let bad = unnamed(&app);
    assert!(bad.is_empty(), "unnamed (More actions open): {bad:#?}");
}
