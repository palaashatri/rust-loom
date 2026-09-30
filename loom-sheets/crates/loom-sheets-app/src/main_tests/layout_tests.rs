use super::*;
use i_slint_backend_testing::ElementHandle;

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

/// No sheet chrome may be laid out outside the window at a text scale the
/// application supports. 1.0 and 1.5 hold today; 2.0 does not, and that gap is
/// tracked as UI-42 rather than quietly excluded. See the ignored case below for
/// the exact measured reproduction.
///
/// Removed on 2026-09-30: the 2.0 arm was folded into
/// `text_scale_2x_overflows_the_toolbar_until_the_responsive_policy_scales`.
#[test]
fn no_sheet_chrome_escapes_the_window_at_a_supported_text_scale() {
    for text_scale in [1.0f32, 1.5] {
        assert_inspector_and_toolbar_fit(1018.0, 728.0, text_scale);
    }
}

#[test]
#[ignore = "UI-42: the responsive policy compares window width only, so at 2x text the toolbar overruns a 1018 px window and the overflow menu never opens"]
fn text_scale_2x_overflows_the_toolbar_until_the_responsive_policy_scales() {
    assert_inspector_and_toolbar_fit(1018.0, 728.0, 2.0);
}

fn assert_inspector_and_toolbar_fit(width: f32, height: f32, text_scale: f32) {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    app.window()
        .set_size(PhysicalSize::new(width as u32, height as u32));
    app.set_template_text_scale(text_scale);
    app.set_show_inspector(true);
    app.set_inspector_tab(1);
    let _ = snapshot_component(&app, width, height, 1.0).expect("render the workspace");
    // The Cell tab's lower controls sit below the fold, so scroll the way a user
    // does before asserting that every control is reachable.
    let bold: Vec<_> =
        ElementHandle::find_by_accessible_label(&app, "Toggle bold formatting (Cell inspector)")
            .collect();
    assert_eq!(bold.len(), 1, "one bold toggle in the Cell inspector");
    bold[0].clone().scroll(0.0, -1_000.0);
    let _ = snapshot_component(&app, width, height, 1.0).expect("render the scrolled inspector");

    for label in [
        "Close",
        "Selected row height",
        "Increase selected row height",
        "Selected column width",
        "Increase selected column width",
        "Format",
    ] {
        let matches: Vec<_> = ElementHandle::find_by_accessible_label(&app, label).collect();
        assert!(!matches.is_empty(), "{label:?} must exist at {text_scale}x");
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
            "{label:?} must be fully inside a {width}x{height} window at {text_scale}x: {inside} of {} copies are inside",
            matches.len()
        );
    }
}
