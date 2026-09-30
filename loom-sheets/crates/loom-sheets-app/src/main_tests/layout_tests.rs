use super::*;
use i_slint_backend_testing::{AccessibleRole, ElementHandle};

#[test]
fn layout_breakpoints_match_supported_width_boundaries() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    let policy = ResponsivePolicy::get(&app);
    assert_eq!(policy.get_priority_1_icon_only_below(), 1180.0);
    assert_eq!(policy.get_priority_2_overflow_below(), 1320.0);
    let expected = [
        (1179, true, true, false),
        (1180, false, true, false),
        (1279, false, true, false),
        (1280, false, true, false),
        (1319, false, true, false),
        (1320, false, false, true),
    ];
    for (width, icon_only, overflow, labeled) in expected {
        assert_eq!(
            layout_breakpoints(&app, width),
            ResponsiveToolbarState {
                icon_only,
                overflow,
                labeled,
            }
        );
        apply_layout_breakpoints(&app, width);
        assert_eq!(app.get_icon_only_toolbar(), icon_only);
        assert_eq!(app.get_overflow_toolbar(), overflow);
        assert_eq!(app.get_labeled_toolbar(), labeled);
    }
}

#[test]
fn layout_breakpoints_scale_with_accessibility_text() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    let policy = ResponsivePolicy::get(&app);
    for scale in [1.0f32, 1.25, 1.5, 2.0] {
        app.set_template_text_scale(scale);
        for (base_width, expected) in [
            (1179, (true, true, false)),
            (1180, (false, true, false)),
            (1279, (false, true, false)),
            (1280, (false, true, false)),
            (1319, (false, true, false)),
            (1320, (false, false, true)),
        ] {
            let width = (base_width as f32 * scale).floor() as u32;
            let state = layout_breakpoints(&app, width);
            assert_eq!(
                (state.icon_only, state.overflow, state.labeled),
                expected,
                "base width {base_width}, scale {scale}, physical width {width}"
            );
        }
        apply_layout_breakpoints(&app, (1400.0 * scale) as u32);
        apply_headless_viewport_size(&app, (1400.0 * scale) as u32, 800);
        // The inspector starts closed and the grid runs edge to edge, so the
        // whole window width is the grid's.
        assert_eq!(
            app.get_grid_viewport_width(),
            1400.0 * scale - TABLE_HORIZONTAL_MARGIN
        );
        app.set_show_inspector(true);
        apply_headless_viewport_size(&app, (1400.0 * scale) as u32, 800);
        assert_eq!(
            app.get_grid_viewport_width(),
            1400.0 * scale - INSPECTOR_WIDTH - TABLE_HORIZONTAL_MARGIN
        );
        app.set_show_inspector(false);
        assert!(!app.get_icon_only_toolbar());
    }
    assert_eq!(policy.get_priority_1_icon_only_below(), 1180.0);
    assert_eq!(policy.get_priority_2_overflow_below(), 1320.0);
}

#[test]
fn text_scale_change_reapplies_responsive_state_without_resizing() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    wire_responsive_layout(&app);
    app.window().set_size(PhysicalSize::new(1400, 800));
    let _ = snapshot_component(&app, 1400.0, 800.0, 1.0).expect("render the workspace");
    app.invoke_window_resized(1400.0);
    assert!(!app.get_icon_only_toolbar());
    assert!(!app.get_overflow_toolbar());

    app.set_template_text_scale(1.25);
    let _ = snapshot_component(&app, 1400.0, 800.0, 1.0).expect("render after text scale change");
    assert!(app.get_icon_only_toolbar());
    assert!(app.get_overflow_toolbar());
    assert!(!app.get_labeled_toolbar());

    app.set_template_text_scale(1.0);
    let _ =
        snapshot_component(&app, 1400.0, 800.0, 1.0).expect("render after restoring text scale");
    assert!(!app.get_icon_only_toolbar());
    assert!(!app.get_overflow_toolbar());
    assert!(app.get_labeled_toolbar());
}

#[test]
fn grid_geometry_uses_core_defaults_and_fits_small_workbooks() {
    assert_eq!(GRID_COL_WIDTH, DEFAULT_COL_WIDTH);
    assert_eq!(GRID_ROW_HEIGHT, DEFAULT_ROW_HEIGHT);

    let mut small = Sheet::new("small");
    small.set_str("C3", "value");
    let dimensions = editor_dimensions(&small, CellRef::parse("A1").unwrap(), None);
    assert_eq!(dimensions, SheetDimensions::new(15, 8));

    let fitted = grid_default_col_width(&small, 1_000.0);
    assert_eq!(fitted, 120.5);
    let viewport = SheetViewport::new(4, 8);
    let geometry = grid_geometry(&small, dimensions, viewport, 1_000.0, 1.0);
    assert_eq!(geometry.column_widths.len(), 8);
    assert!(geometry.column_widths.iter().all(|width| *width == fitted));
    assert_eq!(geometry.content_width, 8.0 * fitted);

    let mut sparse = Sheet::new("sparse");
    sparse.set_str("AZ1000", "tail");
    let sparse_dimensions = editor_dimensions(&sparse, CellRef::parse("A1").unwrap(), None);
    assert_eq!(sparse_dimensions, SheetDimensions::new(1_000, 52));
    assert_eq!(grid_default_col_width(&sparse, 1_000.0), GRID_COL_WIDTH);
}

#[test]
fn object_geometry_and_resize_target_extend_the_grid_content_bounds() {
    let mut sheet = Sheet::new("Object bounds");
    let mut object =
        loom_sheets_core::SheetObject::shape(CellRef { row: 99, col: 25 }, "Far object");
    object.width = 300;
    object.height = 120;
    sheet.objects.push(object);

    let dimensions = editor_dimensions(&sheet, CellRef::parse("A1").unwrap(), None);

    assert!(
        dimensions.cols >= 29,
        "resize target needs horizontal headroom: {dimensions:?}"
    );
    assert!(
        dimensions.rows >= 105,
        "resize target needs vertical headroom: {dimensions:?}"
    );
}

#[test]
fn low_zoom_object_viewport_reserves_space_for_minimum_rendered_bounds() {
    let mut sheet = Sheet::new("Small object bounds");
    let mut object =
        loom_sheets_core::SheetObject::shape(CellRef { row: 15, col: 9 }, "Small object");
    object.width = 80;
    object.height = 48;
    sheet.objects.push(object);

    let dimensions = object_layout::editor_dimensions_with_preview(
        &sheet,
        CellRef::parse("A1").unwrap(),
        None,
        GRID_COL_WIDTH,
        0.5,
        None,
    );

    assert!(
        dimensions.cols >= 12,
        "zoomed minimum object width needs horizontal scroll room: {dimensions:?}"
    );
    assert!(
        dimensions.rows >= 20,
        "zoomed minimum object height needs vertical scroll room: {dimensions:?}"
    );
}

#[test]
fn grid_geometry_retains_persisted_row_and_column_dimensions() {
    let mut sheet = Sheet::new("custom");
    sheet.set_str("B3", "value");
    sheet.set_col_width(1, 140.0);
    sheet.set_row_height(2, 40.0);
    let dimensions = editor_dimensions(&sheet, CellRef::parse("A1").unwrap(), None);
    let viewport = SheetViewport::new(4, 3);
    let geometry = grid_geometry(&sheet, dimensions, viewport, 640.0, 1.0);
    assert_eq!(geometry.column_widths, vec![80.0, 140.0, 80.0]);
    assert_eq!(geometry.row_heights, vec![24.0, 24.0, 40.0, 24.0]);
    assert_eq!(geometry.content_width, 8.0 * 80.0 + 60.0);
    assert_eq!(geometry.content_height, 15.0 * 24.0 + 16.0);

    let json = sheet_to_json(&sheet);
    let reopened = sheet_from_json(&json).expect("dimension metadata round-trips");
    assert_eq!(reopened.col_width(1), 140.0);
    assert_eq!(reopened.row_height(2), 40.0);
}

#[test]
fn zoom_scales_geometry_and_cycles_through_presets() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    // Default window state is 100%.
    assert_eq!(zoom_factor(&app), 1.0);

    let mut sheet = Sheet::new("zoom");
    sheet.set_str("B3", "value");
    sheet.set_col_width(1, 140.0);
    sheet.set_row_height(2, 40.0);
    let dimensions = editor_dimensions(&sheet, CellRef::parse("A1").unwrap(), None);
    let viewport = SheetViewport::new(4, 3);
    let unscaled = grid_geometry(&sheet, dimensions, viewport, 640.0, 1.0);
    let scaled = grid_geometry(&sheet, dimensions, viewport, 640.0, 1.5);
    assert_eq!(
        scaled.column_widths,
        unscaled
            .column_widths
            .iter()
            .map(|width| width * 1.5)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        scaled.row_heights,
        unscaled
            .row_heights
            .iter()
            .map(|height| height * 1.5)
            .collect::<Vec<_>>()
    );
    assert_eq!(scaled.content_width, unscaled.content_width * 1.5);

    // Corrupt zoom values clamp instead of collapsing geometry.
    app.set_zoom_factor(f32::NAN);
    assert_eq!(zoom_factor(&app), 1.0);
    app.set_zoom_factor(99.0);
    assert_eq!(zoom_factor(&app), 3.0);

    // Dispatcher routes the standard View zoom commands.
    assert!(dispatch_command(&app, "view.zoom_in"));
    assert!(dispatch_command(&app, "view.zoom_out"));
    assert!(dispatch_command(&app, "view.zoom_actual"));
}

#[test]
fn sheet_chrome_fits_the_contract_viewport_scale_and_theme_matrix() {
    for theme in ["light", "dark", "high-contrast"] {
        for rtl in [false, true] {
            for (width, height) in [
                (1024.0, 720.0),
                (1280.0, 800.0),
                (1440.0, 900.0),
                (1920.0, 1200.0),
            ] {
                for scale in [1.0f32, 1.25, 1.5, 2.0] {
                    assert_inspector_and_toolbar_fit(width, height, scale, theme, rtl);
                }
            }
        }
    }
}

fn assert_inspector_and_toolbar_fit(
    width: f32,
    height: f32,
    text_scale: f32,
    theme: &str,
    rtl: bool,
) {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    app.window()
        .set_size(PhysicalSize::new(width as u32, height as u32));
    apply_theme(&app, theme);
    configure_direction(&app, rtl);
    app.set_template_text_scale(text_scale);
    apply_layout_breakpoints(&app, width as u32);
    app.set_show_inspector(true);

    app.set_inspector_tab(0);
    let _ = snapshot_component(&app, width, height, 1.0).expect("render the Table inspector");
    for label in [
        "Close",
        "Table name",
        "Add worksheet row",
        "Add worksheet column",
    ] {
        assert_control_inside(&app, width, height, text_scale, theme, label);
        assert_control_inside_component(
            &app,
            "SheetsApp::inspector-panel",
            label,
            width,
            height,
            text_scale,
            theme,
        );
    }

    app.set_inspector_tab(1);
    let _ = snapshot_component(&app, width, height, 1.0).expect("render the workspace");
    // The Cell tab's lower controls sit below the fold, so scroll the way a user
    // does before asserting that every control is reachable.
    let bold: Vec<_> =
        ElementHandle::find_by_accessible_label(&app, "Toggle bold formatting (Cell inspector)")
            .collect();
    assert_eq!(bold.len(), 1, "one bold toggle in the Cell inspector");
    assert_control_inside(
        &app,
        width,
        height,
        text_scale,
        theme,
        "Toggle bold formatting (Cell inspector)",
    );
    bold[0].clone().scroll(0.0, -1_000.0);
    let _ = snapshot_component(&app, width, height, 1.0).expect("render the scrolled inspector");

    for label in [
        "Close",
        "Selected row height",
        "Increase selected row height",
        "Selected column width",
        "Increase selected column width",
    ] {
        assert_control_inside(&app, width, height, text_scale, theme, label);
        assert_control_inside_component(
            &app,
            "SheetsApp::inspector-panel",
            label,
            width,
            height,
            text_scale,
            theme,
        );
    }

    let icon_only = width / text_scale.max(1.0) < 1180.0;
    let mut always_visible = vec![
        "Undo",
        "Redo",
        "Toggle bold formatting (toolbar)",
        "Toggle italic formatting (toolbar)",
        "Toggle underline formatting (toolbar)",
        "Align selected cells left",
        "Align selected cells center",
        "Align selected cells right",
        if icon_only {
            "Format inspector"
        } else {
            "Format"
        },
    ];
    if app.get_labeled_toolbar() {
        always_visible.extend(["New", "Open", "Save", "Save As", "Commands"]);
    } else {
        always_visible.extend([
            "New workbook",
            "Open workbook",
            "Save workbook",
            "Save workbook as",
            "Command palette",
        ]);
    }
    for label in always_visible {
        assert_control_inside(&app, width, height, text_scale, theme, label);
    }

    if app.get_overflow_toolbar() {
        assert_control_inside(&app, width, height, text_scale, theme, "More actions");
        app.set_toolbar_overflow_open(true);
        let _ = snapshot_component(&app, width, height, 1.0).expect("render toolbar overflow");
        let actions = [
            "Add row",
            "Add column",
            "Insert chart",
            "Export CSV",
            "Export Excel",
            "Sort",
        ];
        let mut previous_bottom = None;
        for label in actions {
            assert_control_inside(&app, width, height, text_scale, theme, label);
            let control = assert_control_inside_component(
                &app,
                "SheetToolbarOverflow::popup-panel",
                label,
                width,
                height,
                text_scale,
                theme,
            );
            let position = control.absolute_position();
            if let Some(bottom) = previous_bottom {
                assert!(
                    bottom <= position.y + 1.0,
                    "overflow action {label:?} overlaps the previous entry at {text_scale}x in {theme} at {width}x{height}"
                );
            }
            previous_bottom = Some(position.y + control.size().height);
        }
    } else {
        for label in ["Row", "Col", "Chart", "Export CSV", "Sort"] {
            assert_control_inside(&app, width, height, text_scale, theme, label);
        }
    }
}

#[test]
fn compact_overflow_entries_dispatch_their_actions() {
    use std::{cell::RefCell, rc::Rc};

    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    let calls = Rc::new(RefCell::new(Vec::new()));
    let action_calls = calls.clone();
    app.on_add_row(move || action_calls.borrow_mut().push("row"));
    let action_calls = calls.clone();
    app.on_add_table_col(move || action_calls.borrow_mut().push("column"));
    let action_calls = calls.clone();
    app.on_insert_chart(move || action_calls.borrow_mut().push("chart"));
    let action_calls = calls.clone();
    app.on_export_csv(move || action_calls.borrow_mut().push("csv"));
    let action_calls = calls.clone();
    app.on_export_xlsx(move || action_calls.borrow_mut().push("xlsx"));
    let action_calls = calls.clone();
    app.on_organize(move || action_calls.borrow_mut().push("sort"));

    app.set_template_text_scale(2.0);
    apply_layout_breakpoints(&app, 1024);
    assert!(app.get_overflow_toolbar());
    for (label, expected) in [
        ("Add row", "row"),
        ("Add column", "column"),
        ("Insert chart", "chart"),
        ("Export CSV", "csv"),
        ("Export Excel", "xlsx"),
        ("Sort", "sort"),
    ] {
        app.set_toolbar_overflow_open(true);
        let _ = snapshot_component(&app, 1024.0, 720.0, 1.0).expect("render toolbar overflow");
        let action = ElementHandle::find_by_accessible_label(&app, label)
            .next()
            .unwrap_or_else(|| panic!("overflow entry {label:?} is accessible"));
        action.invoke_accessible_default_action();
        assert_eq!(calls.borrow().last(), Some(&expected));
        assert!(
            !app.get_toolbar_overflow_open(),
            "{label:?} closes the menu"
        );
    }
}

#[test]
fn compact_overflow_keyboard_activation_and_escape_restore_trigger_focus() {
    use std::{cell::Cell, rc::Rc};

    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    app.set_template_text_scale(2.0);
    apply_layout_breakpoints(&app, 1024);
    let rows = Rc::new(Cell::new(0));
    let row_calls = rows.clone();
    app.on_add_row(move || row_calls.set(row_calls.get() + 1));
    let render =
        || snapshot_component(&app, 1024.0, 720.0, 1.0).expect("render keyboard overflow journey");
    let press = |key: slint::platform::Key| {
        let text: SharedString = key.into();
        app.window()
            .dispatch_event(slint::platform::WindowEvent::KeyPressed { text: text.clone() });
        app.window()
            .dispatch_event(slint::platform::WindowEvent::KeyReleased { text });
    };
    let _ = render();
    ElementHandle::find_by_accessible_label(&app, "More actions")
        .next()
        .expect("overflow trigger")
        .invoke_accessible_default_action();
    let _ = render();
    assert!(app.get_toolbar_overflow_open());
    press(slint::platform::Key::Return);
    assert_eq!(rows.get(), 1, "Return activates the first popup entry");
    assert!(!app.get_toolbar_overflow_open());

    let _ = render();
    press(slint::platform::Key::Return);
    let _ = render();
    assert!(
        app.get_toolbar_overflow_open(),
        "focus returns to More actions"
    );
    press(slint::platform::Key::Tab);
    press(slint::platform::Key::Escape);
    assert!(!app.get_toolbar_overflow_open(), "Escape works after Tab");
    let _ = render();
    press(slint::platform::Key::Return);
    let _ = render();
    assert!(
        app.get_toolbar_overflow_open(),
        "Escape restores trigger focus"
    );
    assert_eq!(rows.get(), 1, "reopening must not activate a popup entry");
}

fn assert_control_inside(
    app: &SheetsApp,
    width: f32,
    height: f32,
    text_scale: f32,
    theme: &str,
    label: &str,
) {
    let matches: Vec<_> = ElementHandle::find_by_accessible_label(app, label).collect();
    assert!(
        !matches.is_empty(),
        "{label:?} must exist at {text_scale}x in {theme} at {width}x{height}"
    );
    let inside = matches
        .iter()
        .filter(|element| {
            let position = element.absolute_position();
            let size = element.size();
            position.x >= -1.0
                && position.y >= -1.0
                && position.x + size.width <= width + 1.0
                && position.y + size.height <= height + 1.0
        })
        .count();
    assert_eq!(
        inside,
        matches.len(),
        "{label:?} must be fully inside a {width}x{height} window at {text_scale}x in {theme}: {inside} of {} copies are inside",
        matches.len()
    );
}

fn assert_control_inside_component(
    app: &SheetsApp,
    component_id: &str,
    label: &str,
    width: f32,
    height: f32,
    text_scale: f32,
    theme: &str,
) -> ElementHandle {
    let component = ElementHandle::find_by_element_id(app, component_id)
        .next()
        .unwrap_or_else(|| panic!("{component_id:?} exists at {text_scale}x in {theme}"));
    let parent_position = component.absolute_position();
    let parent_size = component.size();
    let all_matches: Vec<_> = ElementHandle::find_by_accessible_label(app, label).collect();
    let controls: Vec<_> = all_matches
        .iter()
        .filter(|control| {
            let position = control.absolute_position();
            let size = control.size();
            control.accessible_role() != Some(AccessibleRole::Text)
                && position.x >= parent_position.x - 1.0
                && position.y >= parent_position.y - 1.0
                && position.x + size.width <= parent_position.x + parent_size.width + 1.0
                && position.y + size.height <= parent_position.y + parent_size.height + 1.0
        })
        .cloned()
        .collect();
    assert_eq!(
        controls.len(),
        1,
        "{label:?} should have one visible control at {text_scale}x in {theme} at {width}x{height}; panel=({}, {}, {}, {}), matches={:?}",
        parent_position.x,
        parent_position.y,
        parent_size.width,
        parent_size.height,
        all_matches.iter().map(|item| {
            let position = item.absolute_position();
            let size = item.size();
            format!("id={:?} role={:?} bounds=({}, {}, {}, {})", item.id(), item.accessible_role(), position.x, position.y, size.width, size.height)
        }).collect::<Vec<_>>(),
    );
    let control = controls.into_iter().next().expect("one visible control");
    let position = control.absolute_position();
    let size = control.size();
    assert!(
        position.x >= parent_position.x - 1.0
            && position.y >= parent_position.y - 1.0
            && position.x + size.width <= parent_position.x + parent_size.width + 1.0
            && position.y + size.height <= parent_position.y + parent_size.height + 1.0,
        "{label:?} must fit inside {component_id:?} at {text_scale}x in {theme} at {width}x{height}: control ({}, {}, {}, {}), parent ({}, {}, {}, {})",
        position.x,
        position.y,
        size.width,
        size.height,
        parent_position.x,
        parent_position.y,
        parent_size.width,
        parent_size.height,
    );
    control
}
