use super::*;
use i_slint_backend_testing::{AccessibleRole, ElementHandle};

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
    let label_pos = label.absolute_position();
    let field_pos = field.absolute_position();
    let beside = control.absolute_position().x >= label_pos.x + label_width - 1.0;
    let below = field_pos.y >= label_pos.y + label.size().height - 1.0;
    assert!(
        beside || below,
        "controls must follow the full label band, beside it or on a new line"
    );
    let panel = ElementHandle::find_by_element_id(&app, "SheetsApp::inspector-panel")
        .next()
        .expect("inspector panel");
    let right = panel.absolute_position().x + panel.size().width - 16.0;
    assert!(
        control.absolute_position().x + control.size().width <= right + 0.5
            && field_pos.x + field.size().width <= right + 0.5,
        "the label must not push the field or stepper outside panel padding"
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

/// At a normal 1280x800 window the Table tab's controls must end inside the
/// inspector's own padding; the capture showed the Name field and the row/column
/// steppers running into the window edge.
#[test]
fn table_inspector_controls_stay_inside_the_panel_padding() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    app.window().set_size(PhysicalSize::new(1280, 800));
    app.set_show_inspector(true);
    app.set_inspector_tab(0);
    let _ = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render the Table inspector");
    for label in [
        "Table name",
        "Delete selected row",
        "Delete selected column",
    ] {
        let found: Vec<_> = ElementHandle::find_by_accessible_label(&app, label).collect();
        assert_eq!(found.len(), 1, "one control named {label}");
        let el = &found[0];
        let right = el.absolute_position().x + el.size().width;
        assert!(
            right <= 1280.0 - 8.0 + 0.5,
            "{label} must end inside the 8 px panel padding, right edge is {right}"
        );
    }
}

/// Rows of the Cell inspector are one stack: each field row's top edge follows
/// the previous row at the same pitch, with no band of stray space between
/// them, and every control can be reached by scrolling the inspector.
#[test]
fn cell_inspector_rows_are_evenly_spaced_and_reachable_at_every_size() {
    set_platform();
    for (width, height) in [(1280.0f32, 800.0f32), (1440.0, 900.0), (1920.0, 1200.0)] {
        for scale in [1.0f32, 1.5] {
            let app = SheetsApp::new().expect("create SheetsApp");
            app.window()
                .set_size(PhysicalSize::new(width as u32, height as u32));
            app.set_template_text_scale(scale);
            app.set_show_inspector(true);
            app.set_inspector_tab(1);
            let _ = snapshot_component(&app, width, height, 1.0).expect("render Cell inspector");

            let top = |label: &str| {
                let found: Vec<_> = ElementHandle::find_by_accessible_label(&app, label).collect();
                assert_eq!(found.len(), 1, "one field named {label}");
                let el = found.into_iter().next().expect("field count checked");
                (el.absolute_position().y, el.size().height)
            };
            let (y1, h1) = top("Selected range");
            let (y2, _) = top("Displayed value");
            let (y3, _) = top("Raw formula");
            let gap_a = y2 - y1;
            let gap_b = y3 - y2;
            assert!(
                (gap_a - gap_b).abs() <= 2.0,
                "{width}x{height} x{scale}: row pitches differ ({gap_a} vs {gap_b})"
            );
            assert!(
                gap_a - h1 <= 16.0 * scale,
                "{width}x{height} x{scale}: stray space of {} px between rows",
                gap_a - h1
            );

            // The last control must be reachable: after the user scrolls the
            // inspector to its end, the Column width field lies inside the panel.
            // Controls below the fold are not in the tree until the inspector
            // scrolls, so scroll from a visible control the way a user would.
            let anchor: Vec<_> = ElementHandle::find_by_accessible_label(
                &app,
                "Toggle bold formatting (Cell inspector)",
            )
            .collect();
            assert_eq!(anchor.len(), 1, "one bold toggle in the Cell inspector");
            anchor
                .into_iter()
                .next()
                .expect("bold toggle")
                .scroll(0.0, -1_000.0);
            let _ = snapshot_component(&app, width, height, 1.0).expect("render scrolled");
            let field: Vec<_> =
                ElementHandle::find_by_accessible_label(&app, "Selected column width").collect();
            let field = field.into_iter().next().expect("column-width field");
            let panel = ElementHandle::find_by_element_id(&app, "SheetsApp::inspector-panel")
                .next()
                .expect("inspector panel");
            let panel_bottom = panel.absolute_position().y + panel.size().height;
            let field_bottom = field.absolute_position().y + field.size().height;
            assert!(
                field_bottom <= panel_bottom + 0.5,
                "{width}x{height} x{scale}: Column width field ends at {field_bottom}, panel at {panel_bottom}"
            );
        }
    }
}

/// Each control sits under the heading of its own group: Font size under Font
/// Style, the fill swatches under Borders & Fill and before Data Format.
#[test]
fn the_number_format_choices_are_named_by_what_they_do() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    app.window().set_size(PhysicalSize::new(1280, 800));
    app.set_show_inspector(true);
    app.set_inspector_tab(1);
    let _ = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render the Cell inspector");
    // Only buttons count: a button's visible caption is also exposed as a text node.
    let buttons = |label: &str| -> Vec<ElementHandle> {
        ElementHandle::find_by_accessible_label(&app, label)
            .filter(|e| e.accessible_role() == Some(AccessibleRole::Button))
            .collect()
    };
    for name in ["Number", "Currency", "Percent", "Decimal places"] {
        assert_eq!(
            buttons(name).len(),
            1,
            "one data-format button named {name:?}"
        );
    }
    for glyph in ["123", "$", "%", "1.23"] {
        assert!(
            buttons(glyph).is_empty(),
            "a data-format button is still named only by its glyph {glyph:?}"
        );
    }
}

#[test]
fn the_border_control_is_a_switch_that_reports_and_runs_the_border_action() {
    use slint::platform::PointerEventButton;
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    app.window().set_size(PhysicalSize::new(1280, 800));
    app.set_show_inspector(true);
    app.set_inspector_tab(1);
    let runs = std::rc::Rc::new(std::cell::Cell::new(0));
    let counter = runs.clone();
    app.on_toggle_borders(move || counter.set(counter.get() + 1));
    let _ = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render the Cell inspector");
    let border = |app: &SheetsApp| {
        // The switch's caption is also exposed as a text node, so match the switch.
        let found: Vec<_> = ElementHandle::find_by_accessible_label(app, "Border")
            .filter(|e| e.accessible_role() == Some(AccessibleRole::Switch))
            .collect();
        assert_eq!(found.len(), 1, "one Border switch");
        found.into_iter().next().expect("Border switch")
    };

    // The inspector's border state is shown as a switch with an on/off value,
    // not as a plain button or a heading.
    app.set_cell_border(false);
    let _ = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render");
    assert_eq!(
        border(&app).accessible_role(),
        Some(i_slint_backend_testing::AccessibleRole::Switch)
    );
    assert_eq!(
        border(&app).accessible_value().as_deref(),
        Some("unchecked")
    );
    app.set_cell_border(true);
    let _ = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render");
    assert_eq!(border(&app).accessible_value().as_deref(), Some("checked"));

    // Activating it runs the same border action as the menu and palette.
    let target = border(&app);
    let origin = target.absolute_position();
    let size = target.size();
    let at = slint::LogicalPosition::new(origin.x + size.width / 2.0, origin.y + size.height / 2.0);
    app.window()
        .dispatch_event(slint::platform::WindowEvent::PointerPressed {
            position: at,
            button: PointerEventButton::Left,
        });
    app.window()
        .dispatch_event(slint::platform::WindowEvent::PointerReleased {
            position: at,
            button: PointerEventButton::Left,
        });
    assert_eq!(
        runs.get(),
        1,
        "the Border switch runs the border action once"
    );
}

#[test]
fn cell_inspector_controls_sit_under_their_own_headings() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    app.window().set_size(PhysicalSize::new(1920, 1200));
    app.set_show_inspector(true);
    app.set_inspector_tab(1);
    let _ = snapshot_component(&app, 1920.0, 1200.0, 1.0).expect("render Cell inspector");
    let y_of = |label: &str| -> f32 {
        let found: Vec<_> = ElementHandle::find_by_accessible_label(&app, label).collect();
        assert_eq!(found.len(), 1, "one element named {label}");
        found[0].absolute_position().y
    };
    let font_style = y_of("Font Style");
    let font_size = y_of("Font size in points");
    let borders = y_of("Borders & Fill");
    let fill = y_of("Fill red");
    let data_format = y_of("Data Format");
    assert!(
        font_style < font_size && font_size < borders,
        "Font size must sit under Font Style and above Borders & Fill ({font_style}, {font_size}, {borders})"
    );
    assert!(
        borders < fill && fill < data_format,
        "fill swatches must sit under Borders & Fill ({borders}, {fill}, {data_format})"
    );
}
