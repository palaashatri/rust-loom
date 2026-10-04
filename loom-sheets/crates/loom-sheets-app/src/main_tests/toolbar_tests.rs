//! The icon-over-label toolbar, its menus, the Format/Organize inspector tab
//! buttons and the sheet tabs under the toolbar all drive real window state.

use super::*;
use i_slint_backend_testing::{AccessibleRole, ElementHandle};

fn launched() -> SheetsApp {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    app.window().set_size(PhysicalSize::new(1280, 800));
    apply_layout_breakpoints(&app, 1280);
    toolbar_commands::start_with_inspector_open(&app);
    toolbar_commands::wire(&app);
    // The real window wires this callback to the inspector toggle.
    let weak = app.as_weak();
    app.on_toggle_inspector(move || {
        let app = weak.upgrade().expect("live window");
        let next = !app.get_show_inspector();
        app.set_inspector_preference(next);
        app.set_show_inspector(next);
    });
    let weak = app.as_weak();
    app.on_inspector_context_changed(move |tab| {
        weak.upgrade().expect("live window").set_inspector_tab(tab);
    });
    render(&app);
    app
}

fn render(app: &SheetsApp) {
    let _ = snapshot_component(app, 1280.0, 800.0, 1.0).expect("render");
}

fn press(app: &SheetsApp, role: AccessibleRole, label: &str) {
    render(app);
    let found = ElementHandle::find_by_accessible_label(app, label)
        .find(|e| e.accessible_role() == Some(role))
        .unwrap_or_else(|| panic!("no {role:?} named {label:?}"));
    found.invoke_accessible_default_action();
}

/// Press the toolbar button with this name. The menu bar above the toolbar has
/// its own "View" and "Table", so the toolbar item is picked by its height.
fn button(app: &SheetsApp, label: &str) {
    render(app);
    toolbar_button(app, label).invoke_accessible_default_action();
}

fn item(app: &SheetsApp, label: &str) {
    press(app, AccessibleRole::ListItem, label);
}

fn toolbar_button(app: &SheetsApp, label: &str) -> ElementHandle {
    let min_height = if label == "More actions" { 20.0 } else { 44.0 };
    ElementHandle::find_by_accessible_label(app, label)
        .find(|e| {
            // Toolbar items are at least 48 px tall (the overflow icon is 28 px).
            // The menu-bar entries above them are shorter than either.
            e.accessible_role() == Some(AccessibleRole::Button)
                && e.size().height >= min_height
                && e.absolute_position().y < 100.0
        })
        .unwrap_or_else(|| panic!("toolbar button named {label:?}"))
}

#[test]
fn every_toolbar_item_is_a_named_button_in_three_groups() {
    let app = launched();
    let x_of = |label: &str| {
        let el = toolbar_button(&app, label);
        assert!(el.size().width > 0.0, "{label} has a size");
        el.absolute_position().x
    };
    let left = ["View", "Zoom", "Add Sheet"].map(x_of);
    let centre = ["Chart", "Table", "Text", "Shape", "Image"].map(x_of);
    let right = ["Export", "Format", "Organize", "More actions"].map(x_of);
    assert!(left.windows(2).all(|w| w[0] < w[1]));
    assert!(centre.windows(2).all(|w| w[0] < w[1]));
    assert!(right.windows(2).all(|w| w[0] < w[1]));
    assert!(left[2] < centre[0] && centre[4] < right[0]);
}

#[test]
fn sheet_tabs_sit_directly_under_the_toolbar() {
    let app = launched();
    let toolbar_y = toolbar_button(&app, "View").absolute_position().y;
    let add_sheet = ElementHandle::find_by_accessible_label(&app, "Add sheet")
        .next()
        .expect("the sheet tab strip's add button");
    let formula = ElementHandle::find_by_accessible_label(&app, "Name box")
        .next()
        .expect("name box");
    let grid = ElementHandle::find_by_accessible_label(&app, "Untitled worksheet grid")
        .next()
        .expect("worksheet grid");
    let tabs_y = add_sheet.absolute_position().y;
    assert!(
        toolbar_y < tabs_y && tabs_y < formula.absolute_position().y,
        "toolbar {toolbar_y}, sheet tabs {tabs_y}, formula bar {}",
        formula.absolute_position().y
    );
    assert!(formula.absolute_position().y < grid.absolute_position().y);
}

#[test]
fn inspector_is_open_by_default_and_tab_buttons_switch_or_close_it() {
    let app = launched();
    assert!(app.get_show_inspector());
    assert_eq!(app.get_inspector_tab(), 1, "opens on Format");

    button(&app, "Organize");
    assert!(app.get_show_inspector());
    assert_eq!(app.get_inspector_tab(), 0);

    button(&app, "Format");
    assert!(app.get_show_inspector());
    assert_eq!(app.get_inspector_tab(), 1);

    button(&app, "Format");
    assert!(
        !app.get_show_inspector(),
        "second press closes the inspector"
    );

    button(&app, "Organize");
    assert!(app.get_show_inspector());
    assert_eq!(app.get_inspector_tab(), 0);
}

#[test]
fn a_window_too_narrow_to_dock_the_inspector_starts_with_it_closed() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    apply_layout_breakpoints(&app, 1024);
    toolbar_commands::start_with_inspector_open(&app);
    assert!(!app.get_show_inspector());
    assert_eq!(app.get_inspector_tab(), 1);
}

#[test]
fn direct_toolbar_items_run_their_commands() {
    let app = launched();
    let calls = Rc::new(RefCell::new(Vec::new()));
    let record = |name: &'static str| {
        let calls = calls.clone();
        move || calls.borrow_mut().push(name)
    };
    app.on_add_sheet(record("sheet"));
    app.on_insert_chart(record("chart"));
    app.on_insert_shape(record("shape"));
    app.on_insert_image(record("image"));
    for (label, expected) in [
        ("Add Sheet", "sheet"),
        ("Chart", "chart"),
        ("Shape", "shape"),
        ("Image", "image"),
    ] {
        button(&app, label);
        assert_eq!(calls.borrow().last(), Some(&expected), "{label}");
    }
}

#[test]
fn view_and_zoom_menus_run_their_commands_and_close() {
    let app = launched();
    let calls = Rc::new(RefCell::new(Vec::new()));
    let record = |name: &'static str| {
        let calls = calls.clone();
        move || calls.borrow_mut().push(name)
    };
    app.on_zoom_in(record("in"));
    app.on_zoom_out(record("out"));
    app.on_zoom_actual(record("actual"));

    button(&app, "Zoom");
    assert_eq!(app.get_toolbar_menu(), 2);
    item(&app, "Zoom In");
    assert_eq!(app.get_toolbar_menu(), 0, "choosing a row closes the menu");
    button(&app, "Zoom");
    item(&app, "Zoom Out");
    button(&app, "Zoom");
    item(&app, "Actual Size");
    assert_eq!(*calls.borrow(), ["in", "out", "actual"]);

    button(&app, "View");
    assert_eq!(app.get_toolbar_menu(), 1);
    assert!(app.get_show_inspector());
    item(&app, "Format Inspector");
    assert!(!app.get_show_inspector(), "the row toggles the inspector");
}

#[test]
fn table_text_and_export_menus_run_their_commands() {
    let app = launched();
    let calls = Rc::new(RefCell::new(Vec::new()));
    let record = |name: &'static str| {
        let calls = calls.clone();
        move || calls.borrow_mut().push(name)
    };
    app.on_add_row(record("row"));
    app.on_add_table_col(record("col"));
    app.on_organize(record("sort"));
    app.on_toggle_bold(record("bold"));
    app.on_toggle_italic(record("italic"));
    app.on_toggle_underline(record("underline"));
    app.on_export_csv(record("csv"));
    app.on_export_xlsx(record("xlsx"));
    let alignments = Rc::new(RefCell::new(Vec::new()));
    let seen = alignments.clone();
    app.on_set_cell_alignment(move |a| seen.borrow_mut().push(a));

    for (menu, rows) in [
        ("Table", &["Add Row", "Add Column", "Sort"][..]),
        ("Text", &["Bold", "Italic", "Underline"][..]),
        ("Export", &["Export CSV", "Export Excel"][..]),
    ] {
        for row in rows {
            button(&app, menu);
            item(&app, row);
        }
    }
    assert_eq!(
        *calls.borrow(),
        [
            "row",
            "col",
            "sort",
            "bold",
            "italic",
            "underline",
            "csv",
            "xlsx"
        ]
    );
    for row in ["Align Left", "Align Center", "Align Right"] {
        button(&app, "Text");
        item(&app, row);
    }
    assert_eq!(*alignments.borrow(), [0, 1, 2]);
}

#[test]
fn menus_open_below_the_toolbar_and_keyboard_runs_a_row() {
    use slint::platform::{Key, WindowEvent};
    let app = launched();
    let calls = Rc::new(RefCell::new(Vec::new()));
    let seen = calls.clone();
    app.on_add_table_col(move || seen.borrow_mut().push("col"));
    button(&app, "Table");
    render(&app);
    let toolbar_bottom = {
        let el = toolbar_button(&app, "Table");
        el.absolute_position().y + el.size().height
    };
    let row = ElementHandle::find_by_accessible_label(&app, "Add Row")
        .find(|e| e.accessible_role() == Some(AccessibleRole::ListItem))
        .expect("first row");
    assert!(row.absolute_position().y >= toolbar_bottom - 1.0);

    let press_key = |key: Key| {
        let text: SharedString = key.into();
        app.window()
            .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
        app.window()
            .dispatch_event(WindowEvent::KeyReleased { text });
    };
    press_key(Key::DownArrow);
    press_key(Key::Return);
    assert_eq!(
        *calls.borrow(),
        ["col"],
        "Down then Return runs the second row"
    );
    assert_eq!(app.get_toolbar_menu(), 0);

    button(&app, "Zoom");
    press_key(Key::Escape);
    assert_eq!(app.get_toolbar_menu(), 0, "Escape closes the menu");
}
