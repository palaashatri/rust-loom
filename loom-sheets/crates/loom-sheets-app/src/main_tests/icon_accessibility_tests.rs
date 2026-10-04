use super::*;
use i_slint_backend_testing::{AccessibleRole, ElementHandle};
use std::collections::HashSet;

// The toolbar is icon-over-label items and menu rows; its accessible names
// are checked in `toolbar_tests.rs`. These are the remaining icon buttons.
const SHARED_ACTIONS: &[(&str, &str)] = &[
    ("Add sheet", "Add a worksheet to this workbook."),
    (
        "Commit formula",
        "Apply the formula bar entry to the selected cell.",
    ),
    (
        "Cancel formula edit",
        "Discard formula bar changes and keep the current cell value.",
    ),
    (
        "Hide chart panel",
        "Hide the chart preview and keep the chart in the worksheet.",
    ),
];

const TABLE_ACTIONS: &[(&str, &str)] = &[
    (
        "Add worksheet row",
        "Add a row after the last used worksheet row.",
    ),
    (
        "Delete selected row",
        "Remove the row containing the selected cell and shift later rows up.",
    ),
    (
        "Add worksheet column",
        "Add a column after the last used worksheet column.",
    ),
    (
        "Delete selected column",
        "Remove the column containing the selected cell and shift later columns left.",
    ),
];

const CELL_ACTIONS: &[(&str, &str)] = &[
    (
        "Toggle bold formatting (Cell inspector)",
        "Turn bold formatting on or off for selected cells in the Cell inspector.",
    ),
    (
        "Toggle italic formatting (Cell inspector)",
        "Turn italic formatting on or off for selected cells in the Cell inspector.",
    ),
    (
        "Toggle underline formatting (Cell inspector)",
        "Turn underline formatting on or off for selected cells in the Cell inspector.",
    ),
    (
        "Decrease selected cell font size",
        "Reduce the font size of selected cells by one step.",
    ),
    (
        "Increase selected cell font size",
        "Increase the font size of selected cells by one step.",
    ),
    (
        "Decrease displayed decimal places",
        "Show one fewer decimal place in the selected cells.",
    ),
    (
        "Increase displayed decimal places",
        "Show one more decimal place in the selected cells.",
    ),
    (
        "Decrease selected row height",
        "Reduce the selected row height by 4 px, down to 16 px.",
    ),
    (
        "Increase selected row height",
        "Increase the selected row height by 4 px, up to 160 px.",
    ),
    (
        "Decrease selected column width",
        "Reduce the selected column width by 10 px, down to 32 px.",
    ),
    (
        "Increase selected column width",
        "Increase the selected column width by 10 px, up to 400 px.",
    ),
];

fn accessibility_test_app() -> SheetsApp {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    app.window().set_size(PhysicalSize::new(1280, 720));
    app.set_show_inspector(true);
    app.set_chart_visible(true);
    let _ = snapshot_component(&app, 1280.0, 720.0, 1.0)
        .expect("render the accessibility tree at the normal window size");
    app
}

fn check_icon_button_tree(
    app: &SheetsApp,
    expected: &[(&str, &str)],
    seen_labels: &mut HashSet<String>,
) {
    let mut tree_labels = HashSet::new();

    for (expected_label, expected_description) in expected {
        let matches: Vec<_> =
            ElementHandle::find_by_accessible_label(app, expected_label).collect();
        assert_eq!(
            matches.len(),
            1,
            "each expected icon action must appear once in the live tree: {expected_label:?}"
        );
        let button = matches.into_iter().next().expect("button count checked");
        let label = button
            .accessible_label()
            .expect("icon action must have an accessible label")
            .to_string();
        assert_eq!(label, *expected_label);

        assert_eq!(button.accessible_role(), Some(AccessibleRole::Button));
        assert!(
            tree_labels.insert(label.clone()),
            "icon action label must be unique across the live Sheets tree: {label:?}"
        );
        seen_labels.insert(label.clone());
        let description = button
            .accessible_description()
            .expect("icon action must have an accessible description");
        assert!(
            !description.is_empty(),
            "{label:?} has an empty description"
        );
        assert_ne!(
            description.as_str(),
            label,
            "{label:?} needs an action description beyond its name"
        );
        assert_eq!(description.as_str(), *expected_description, "{label:?}");
    }
}

fn icon_button(app: &SheetsApp, label: &str) -> ElementHandle {
    let matches: Vec<_> = ElementHandle::find_by_accessible_label(app, label).collect();
    assert_eq!(matches.len(), 1, "expected exactly one {label:?} button");
    matches.into_iter().next().expect("button count checked")
}

fn scroll_cell_inspector_to_bottom(app: &SheetsApp) {
    // The normal-size window clips the lower controls until the inspector is
    // scrolled. Send a wheel event over a visible button, as a user would.
    icon_button(app, "Toggle bold formatting (Cell inspector)").scroll(0.0, -1_000.0);
    let _ = snapshot_component(app, 1280.0, 720.0, 1.0)
        .expect("render the scrolled Cell inspector accessibility tree");
}

#[test]
fn every_sheets_icon_action_has_a_unique_name_and_useful_description_in_the_accessibility_tree() {
    let app = accessibility_test_app();
    let source_instances = [
        include_str!("../../ui/components.slint"),
        include_str!("../../ui/toolbar.slint"),
        include_str!("../../ui/chart.slint"),
        include_str!("../../ui/inspector.slint"),
    ]
    .iter()
    .map(|source| {
        source
            .match_indices("LoomIconButton")
            .filter(|(index, token)| {
                let end = index + token.len();
                source[end..].trim_start().starts_with('{')
            })
            .count()
    })
    .sum::<usize>();
    let semantic_action_count = SHARED_ACTIONS.len() + TABLE_ACTIONS.len() + CELL_ACTIONS.len();
    assert_eq!(
        source_instances, semantic_action_count,
        "update this accessibility contract when adding or removing a Sheets icon action"
    );

    let mut seen_labels = HashSet::new();
    app.set_inspector_tab(0);
    let mut table_expected = SHARED_ACTIONS.to_vec();
    table_expected.extend_from_slice(TABLE_ACTIONS);
    check_icon_button_tree(&app, &table_expected, &mut seen_labels);

    app.set_inspector_tab(1);
    let mut cell_top_expected = SHARED_ACTIONS.to_vec();
    cell_top_expected.extend_from_slice(&CELL_ACTIONS[..5]);
    check_icon_button_tree(&app, &cell_top_expected, &mut seen_labels);

    scroll_cell_inspector_to_bottom(&app);
    check_icon_button_tree(&app, &CELL_ACTIONS[5..], &mut seen_labels);

    app.set_inspector_tab(0);
    let _ = snapshot_component(&app, 1280.0, 720.0, 1.0)
        .expect("render the Table inspector after switching from its scrolled Cell tab");
    check_icon_button_tree(&app, &table_expected, &mut seen_labels);

    assert_eq!(
        seen_labels.len(),
        semantic_action_count,
        "all {semantic_action_count} Sheets icon actions must have a distinct accessible name"
    );
}

#[derive(Debug, PartialEq, Eq)]
enum IconActionEvent {
    AddRow,
    DeleteRow,
    AddColumn,
    DeleteColumn,
    Font(i32),
    Decimals(i32),
    RowHeight(i32),
    ColumnWidth(i32),
    CloseChart,
}

#[test]
fn inspector_steppers_and_chart_close_support_their_accessible_default_action() {
    let app = accessibility_test_app();

    let events = Rc::new(RefCell::new(Vec::new()));
    {
        let events = events.clone();
        app.on_add_row(move || events.borrow_mut().push(IconActionEvent::AddRow));
    }
    {
        let events = events.clone();
        app.on_delete_row(move || events.borrow_mut().push(IconActionEvent::DeleteRow));
    }
    {
        let events = events.clone();
        app.on_add_table_col(move || events.borrow_mut().push(IconActionEvent::AddColumn));
    }
    {
        let events = events.clone();
        app.on_delete_col(move || events.borrow_mut().push(IconActionEvent::DeleteColumn));
    }
    {
        let events = events.clone();
        app.on_adjust_font(move |delta| events.borrow_mut().push(IconActionEvent::Font(delta)));
    }
    {
        let events = events.clone();
        app.on_adjust_decimals(move |delta| {
            events.borrow_mut().push(IconActionEvent::Decimals(delta))
        });
    }
    {
        let events = events.clone();
        app.on_adjust_row_height(move |delta| {
            events.borrow_mut().push(IconActionEvent::RowHeight(delta))
        });
    }
    {
        let events = events.clone();
        app.on_adjust_col_width(move |delta| {
            events
                .borrow_mut()
                .push(IconActionEvent::ColumnWidth(delta))
        });
    }
    {
        let events = events.clone();
        app.on_close_chart(move || events.borrow_mut().push(IconActionEvent::CloseChart));
    }

    app.set_inspector_tab(0);
    for (label, expected) in [
        ("Add worksheet row", IconActionEvent::AddRow),
        ("Delete selected row", IconActionEvent::DeleteRow),
        ("Add worksheet column", IconActionEvent::AddColumn),
        ("Delete selected column", IconActionEvent::DeleteColumn),
    ] {
        icon_button(&app, label).invoke_accessible_default_action();
        assert_eq!(events.borrow().last(), Some(&expected));
    }

    app.set_inspector_tab(1);
    for (label, expected) in [
        (
            "Decrease selected cell font size",
            IconActionEvent::Font(-1),
        ),
        ("Increase selected cell font size", IconActionEvent::Font(1)),
    ] {
        icon_button(&app, label).invoke_accessible_default_action();
        assert_eq!(events.borrow().last(), Some(&expected));
    }

    scroll_cell_inspector_to_bottom(&app);
    for (label, expected) in [
        (
            "Decrease displayed decimal places",
            IconActionEvent::Decimals(-1),
        ),
        (
            "Increase displayed decimal places",
            IconActionEvent::Decimals(1),
        ),
        (
            "Decrease selected row height",
            IconActionEvent::RowHeight(-4),
        ),
        (
            "Increase selected row height",
            IconActionEvent::RowHeight(4),
        ),
        (
            "Decrease selected column width",
            IconActionEvent::ColumnWidth(-10),
        ),
        (
            "Increase selected column width",
            IconActionEvent::ColumnWidth(10),
        ),
    ] {
        icon_button(&app, label).invoke_accessible_default_action();
        assert_eq!(events.borrow().last(), Some(&expected));
    }

    icon_button(&app, "Hide chart panel").invoke_accessible_default_action();
    assert!(
        !app.get_chart_visible(),
        "accessible Close should hide the chart panel"
    );
    assert_eq!(events.borrow().last(), Some(&IconActionEvent::CloseChart));
}
