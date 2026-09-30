use super::*;
use i_slint_backend_testing::ElementHandle;

#[test]
fn sheets_inspector_remains_available_and_remembers_the_user_choice_at_compact_width() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    assert!(!app.get_show_inspector());
    assert!(!app.get_inspector_preference());
    apply_layout_breakpoints(&app, 1024);
    assert!(app.get_overflow_toolbar());
    assert!(app.get_inspector_available());
    assert!(!app.get_show_inspector());
    apply_layout_breakpoints(&app, 1180);
    assert!(app.get_overflow_toolbar());
    assert!(app.get_inspector_available());
    assert!(!app.get_show_inspector());
    apply_layout_breakpoints(&app, 1280);
    assert!(app.get_overflow_toolbar());
    assert!(app.get_inspector_available() && !app.get_show_inspector());
    app.set_inspector_preference(true);
    app.set_show_inspector(true);
    apply_layout_breakpoints(&app, 1024);
    assert!(app.get_inspector_available() && app.get_show_inspector());
    apply_layout_breakpoints(&app, 1280);
    assert!(app.get_show_inspector());
    app.set_inspector_preference(false);
    app.set_show_inspector(false);
    apply_layout_breakpoints(&app, 1280);
    assert!(!app.get_show_inspector());
    apply_layout_breakpoints(&app, 1320);
    assert!(!app.get_overflow_toolbar());
}

#[test]
fn compact_inspector_escape_restores_keyboard_activation_to_format() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    apply_layout_breakpoints(&app, 1024);
    app.set_selected_cell("C4".into());
    app.set_selection_range("C4:D5".into());
    let weak = app.as_weak();
    app.on_toggle_inspector(move || {
        let app = weak.upgrade().expect("live window");
        app.set_show_inspector(!app.get_show_inspector());
    });
    app.set_show_inspector(true);
    let _ = snapshot_component(&app, 1024.0, 720.0, 1.0).expect("render compact inspector");
    app.window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed {
            text: slint::platform::Key::Escape.into(),
        });
    assert!(!app.get_show_inspector(), "Escape closes the inspector");
    let _ = snapshot_component(&app, 1024.0, 720.0, 1.0).expect("render closed inspector");
    app.window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed {
            text: slint::platform::Key::Return.into(),
        });
    app.window()
        .dispatch_event(slint::platform::WindowEvent::KeyReleased {
            text: slint::platform::Key::Return.into(),
        });
    assert!(
        app.get_show_inspector(),
        "Enter reopens through the focused Format trigger"
    );
    let _ = snapshot_component(&app, 1024.0, 720.0, 1.0).expect("render reopened inspector");
    app.window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed {
            text: slint::platform::Key::Tab.into(),
        });
    app.window()
        .dispatch_event(slint::platform::WindowEvent::KeyReleased {
            text: slint::platform::Key::Tab.into(),
        });
    app.window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed {
            text: slint::platform::Key::Return.into(),
        });
    app.window()
        .dispatch_event(slint::platform::WindowEvent::KeyReleased {
            text: slint::platform::Key::Return.into(),
        });
    assert!(!app.get_show_inspector(), "Tab then Enter activates Close");
    assert_eq!(app.get_selected_cell().as_str(), "C4");
    assert_eq!(app.get_selection_range().as_str(), "C4:D5");
}

/// UI-39: an inspector property label is interface language, so
/// `desktop-ui.toml [truncation] allow-control-label = false` forbids eliding
/// it. The band must start at `property-label-width-min` and still leave the
/// row's controls their full width at the docked inspector size.
#[test]
fn inspector_property_labels_are_readable_at_the_docked_width() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    app.window().set_size(PhysicalSize::new(1018, 728));
    app.set_show_inspector(true);
    app.set_inspector_tab(1);
    let _ = snapshot_component(&app, 1018.0, 728.0, 1.0).expect("render the Cell inspector");
    // The Cell tab's lower controls sit below the fold at this size, exactly as
    // in the reported capture. Scroll the way a user would before measuring.
    let anchor: Vec<_> =
        ElementHandle::find_by_accessible_label(&app, "Toggle bold formatting (Cell inspector)")
            .collect();
    assert_eq!(anchor.len(), 1, "one bold toggle in the Cell inspector");
    anchor
        .into_iter()
        .next()
        .expect("bold toggle")
        .scroll(0.0, -1_000.0);
    let _ = snapshot_component(&app, 1018.0, 728.0, 1.0).expect("render the scrolled inspector");

    // The reported defect: the visible label elided to "Column wi…".
    let label: Vec<_> = ElementHandle::find_by_accessible_label(&app, "Column width").collect();
    assert_eq!(
        label.len(),
        1,
        "the property label must be a real accessibility node, not bare decoration"
    );
    let label = label.into_iter().next().expect("label count checked");
    let label_width = label.size().width;
    assert!(
        label_width >= 84.0,
        "property labels must reach the contract minimum width, got {label_width}"
    );

    // A longer localized label must not push the row's controls out of reach.
    let control: Vec<_> =
        ElementHandle::find_by_accessible_label(&app, "Increase selected column width").collect();
    assert_eq!(control.len(), 1, "one column-width control");
    let control = control.into_iter().next().expect("control count checked");
    let field: Vec<_> =
        ElementHandle::find_by_accessible_label(&app, "Selected column width").collect();
    assert_eq!(field.len(), 1, "one column-width field");
    let field = field.into_iter().next().expect("field count checked");

    assert!(
        control.size().width >= 28.0,
        "the stepper must keep its full control target, got {}",
        control.size().width
    );
    assert!(
        field.size().width >= 100.0,
        "the value field must stay usable beside the label, got {}",
        field.size().width
    );
    assert!(
        control.absolute_position().x >= label.absolute_position().x + label_width - 1.0,
        "the control column must start after the full label band"
    );
}

/// UI-32: the sheet name already appears in the window title, the sheet tab,
/// and the table's accessible label. The extra centered heading above the grid
/// only consumed workspace height, so the table container must now be exactly
/// the grid viewport.
#[test]
fn the_worksheet_has_no_redundant_heading_above_the_grid() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    app.window().set_size(PhysicalSize::new(1018, 728));
    let _ = snapshot_component(&app, 1018.0, 728.0, 1.0).expect("render the worksheet");

    let surface: Vec<_> =
        ElementHandle::find_by_accessible_label(&app, "Untitled worksheet grid").collect();
    let table: Vec<_> = ElementHandle::find_by_accessible_label(&app, "Untitled table").collect();
    let canvas: Vec<_> =
        ElementHandle::find_by_accessible_label(&app, "Untitled cell canvas").collect();
    assert_eq!(surface.len(), 1, "one worksheet grid");
    assert_eq!(table.len(), 1, "one sheet table");
    assert_eq!(canvas.len(), 1, "one cell canvas");

    let table = table.into_iter().next().expect("table count checked");
    let canvas = canvas.into_iter().next().expect("canvas count checked");
    let band = table.size().height - canvas.size().height;
    assert!(
        band.abs() < 1.0,
        "the table must be the grid viewport, not a heading plus the grid: {band} px of redundant band"
    );

    // Worksheet identity must survive the removal.
    assert!(surface
        .into_iter()
        .next()
        .unwrap()
        .accessible_label()
        .is_some());
    assert!(table.accessible_label().is_some());
}
