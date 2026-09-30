use super::*;

#[test]
fn sheet_grid_projection_uses_viewport_offsets_for_headers_and_cells() {
    let mut sheet = Sheet::new("test");
    sheet.set_str("C11", "Bottom right");
    sheet.set_str("D12", "Tail");
    let viewport = loom_sheets_core::SheetViewport {
        first_row: 10,
        first_col: 2,
        visible_rows: 2,
        visible_cols: 2,
    };

    let projection = project_sheet_grid(&sheet, viewport);

    assert_eq!(projection.column_headers, ["C", "D"]);
    assert_eq!(projection.row_headers, ["11", "12"]);
    assert_eq!(projection.cells, ["Bottom right", "", "", "Tail"]);
}

#[test]
fn sheet_object_projection_tracks_anchor_and_visibility() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    let mut sheet = Sheet::new("objects");
    sheet.objects.push(loom_sheets_core::SheetObject::shape(
        CellRef { row: 1, col: 2 },
        "Callout",
    ));
    sheet.objects.push(loom_sheets_core::SheetObject::shape(
        CellRef { row: 40, col: 40 },
        "Offscreen",
    ));

    project_sheet(&app, &sheet);

    let objects = app.get_object_views();
    assert_eq!(objects.row_count(), 2);
    let first = objects.row_data(0).expect("first projected object");
    let second = objects.row_data(1).expect("second projected object");
    assert_eq!(first.kind.as_str(), "shape");
    assert_eq!(first.label.as_str(), "Callout");
    assert!(first.visible);
    assert!(!second.visible);
}

#[test]
fn wide_object_remains_visible_when_its_anchor_scrolls_out_of_view() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    app.window().set_size(PhysicalSize::new(1280, 800));
    app.set_grid_viewport_width(400.0);
    app.set_grid_viewport_height(200.0);
    app.set_grid_scroll_x(-400.0);
    let mut sheet = Sheet::new("Wide object viewport");
    let mut object = loom_sheets_core::SheetObject::shape(CellRef { row: 0, col: 0 }, "Wide");
    object.width = 800;
    object.height = 120;
    sheet.objects.push(object);

    project_sheet_without_reveal(&app, &sheet);

    assert!(
        app.get_object_views()
            .row_data(0)
            .is_some_and(|object| object.visible),
        "an object must remain projected while its rendered bounds or resize handle intersects the viewport"
    );
}

#[test]
fn small_object_remains_visible_at_minimum_render_size_and_low_zoom() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    app.window().set_size(PhysicalSize::new(1280, 800));
    app.set_grid_viewport_width(100.0);
    app.set_grid_viewport_height(100.0);
    app.set_zoom_factor(0.5);
    app.set_grid_scroll_x(-70.0);
    app.set_grid_scroll_y(-55.0);
    let mut sheet = Sheet::new("Small object viewport");
    let mut object = loom_sheets_core::SheetObject::shape(CellRef { row: 0, col: 0 }, "Small");
    object.width = 80;
    object.height = 48;
    sheet.objects.push(object);

    project_sheet_without_reveal(&app, &sheet);

    let object = app
        .get_object_views()
        .row_data(0)
        .expect("projected object");
    assert_eq!(
        (object.width, object.height),
        (
            object_layout::MIN_RENDERED_OBJECT_WIDTH,
            object_layout::MIN_RENDERED_OBJECT_HEIGHT
        )
    );
    assert!(
        object.visible,
        "culling must use the same minimum rendered size as the visible object"
    );
}

#[test]
fn embedded_image_object_projects_without_a_source_path() {
    const PNG_BYTES: &[u8] = &[
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 4,
        0, 0, 0, 181, 28, 12, 2, 0, 0, 0, 11, 73, 68, 65, 84, 120, 218, 99, 100, 248, 15, 0, 1, 5,
        1, 1, 39, 24, 227, 102, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ];
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    let mut sheet = Sheet::new("embedded");
    let mut object =
        loom_sheets_core::SheetObject::image(CellRef { row: 0, col: 0 }, "missing.png")
            .expect("image object");
    object.embedded = Some(PNG_BYTES.to_vec());
    sheet.objects.push(object);

    project_sheet(&app, &sheet);

    let image = app
        .get_object_views()
        .row_data(0)
        .expect("projected image")
        .image;
    assert_eq!(image.size().width, 1);
    assert_eq!(image.size().height, 1);
}

#[test]
fn object_gestures_are_live_previewed_and_committed_as_one_undoable_change() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], []));
    let state = Rc::new(GuiState::new(
        starter_workbook(),
        None,
        dialogs,
        FileFilter::new("Workbook", ["loomtable"]).expect("filter"),
        FileFilter::new("CSV", ["csv"]).expect("filter"),
        FileFilter::new("CSV", ["csv"]).expect("filter"),
        FileFilter::new("Excel", ["xlsx"]).expect("filter"),
    ));
    let menu_service = std::sync::Arc::new(NativeMenuBar::new());
    register_history_actions(&app, &state, &menu_service);
    object_actions::register_object_actions(&app, &state, &menu_service);
    app.set_grid_viewport_width(640.0);

    let mut sheet = Sheet::new("Objects");
    sheet.objects.push(loom_sheets_core::SheetObject::shape(
        CellRef { row: 0, col: 0 },
        "Movable",
    ));
    state.install_workbook(vec![sheet], 0);
    state.mark_saved();
    project_current(&app, &state);

    app.invoke_object_move_started(0);
    assert_eq!(app.get_selected_object(), 0);
    app.invoke_object_moved(0, 80.0, 24.0);
    assert_eq!(
        state
            .object_gesture
            .borrow()
            .as_ref()
            .map(|gesture| gesture.preview_anchor),
        Some(CellRef { row: 1, col: 1 }),
        "pointer motion updates the visible preview geometry"
    );
    assert_eq!(
        state.current.borrow().objects[0].anchor,
        CellRef { row: 0, col: 0 },
        "live motion must not change the saved worksheet model"
    );
    assert!(state.undo_stack.borrow().is_empty());
    app.invoke_object_move_ended(0);
    assert_eq!(state.undo_stack.borrow().len(), 1);
    assert!(state.is_dirty());
    assert_eq!(
        state.current.borrow().objects[0].anchor,
        CellRef { row: 1, col: 1 },
        "ending the gesture commits the new anchor"
    );

    app.invoke_undo();
    assert!(!state.is_dirty());
    assert_eq!(
        state.current.borrow().objects[0].anchor,
        CellRef { row: 0, col: 0 }
    );

    app.invoke_object_resize_started(0);
    app.invoke_object_resized(0, 100.0, 100.0);
    assert_eq!(
        state
            .object_gesture
            .borrow()
            .as_ref()
            .map(|gesture| (gesture.preview_width, gesture.preview_height)),
        Some((340, 212)),
        "resize motion updates the preview dimensions"
    );
    assert_eq!(
        (
            state.current.borrow().objects[0].width,
            state.current.borrow().objects[0].height
        ),
        (240, 112),
        "resize preview stays outside the saved worksheet model"
    );
    app.invoke_object_resize_ended(0);
    assert_eq!(state.undo_stack.borrow().len(), 1);
    assert!(state.is_dirty());
    assert_eq!(
        (
            state.current.borrow().objects[0].width,
            state.current.borrow().objects[0].height
        ),
        (340, 212)
    );
    app.invoke_undo();
    assert!(!state.is_dirty());
    assert_eq!(
        (
            state.current.borrow().objects[0].width,
            state.current.borrow().objects[0].height
        ),
        (240, 112)
    );
}

#[test]
fn sparse_viewport_projection_tracks_scroll_and_dimensions() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    app.set_grid_viewport_width(360.0);
    app.set_grid_viewport_height(280.0);
    app.set_grid_scroll_x(-180.0);
    app.set_grid_scroll_y(-672.0);

    let mut sheet = Sheet::new("sparse");
    sheet.set_str("AZ1000", "tail");
    let viewport = viewport_from_app(&app, &sheet, None);

    assert_eq!(sheet.dimensions(), SheetDimensions::new(1_000, 52));
    assert_eq!(viewport.first_row, 28);
    assert_eq!(viewport.first_col, 2);
    // Two extra rows and columns are materialized past the viewport edge so a
    // partial first cell never leaves a blank strip while scrolling.
    assert_eq!(viewport.visible_rows, 13);
    assert_eq!(viewport.visible_cols, 7);
    assert!(viewport.contains(CellRef::parse("C29").unwrap()));
    assert!(!viewport.contains(CellRef::parse("B29").unwrap()));
}

#[test]
fn custom_dimensions_drive_viewport_projection_and_offsets() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    app.set_grid_viewport_width(400.0);
    app.set_grid_viewport_height(200.0);
    app.set_grid_scroll_x(-170.0);
    app.set_grid_scroll_y(-50.0);

    let mut sheet = Sheet::new("custom viewport");
    sheet.set_col_width(0, 160.0);
    sheet.set_row_height(0, 48.0);
    sheet.set_str("B2", "target");

    project_sheet_without_reveal(&app, &sheet);

    assert_eq!(app.get_column_headers().row_data(0).as_deref(), Some("B"));
    assert_eq!(app.get_row_headers().row_data(0).as_deref(), Some("2"));
    assert_eq!(app.get_cells().row_data(0).as_deref(), Some("target"));
    assert_eq!(app.get_grid_col_offset(), 160.0);
    assert_eq!(app.get_grid_row_offset(), 48.0);
}

#[test]
fn sparse_tail_scroll_materializes_tail_headers_and_values() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    app.window().set_size(PhysicalSize::new(1280, 800));
    apply_layout_breakpoints(&app, 1280);
    apply_headless_viewport_size(&app, 1280, 800);
    app.set_grid_scroll_y(-26_600.0);

    let mut sheet = Sheet::new("Sparse 1000");
    sheet.set_str("A995", "10");
    sheet.set_str("A996", "20");
    sheet.set_str("A1000", "tail");
    project_sheet_without_reveal(&app, &sheet);

    // The last seeded row is 1000 and the window is 800 px tall, so the tail
    // must be the final visible row. Derive the other indices from that instead
    // of pinning magic numbers: removing the redundant heading above the grid
    // (UI-32) gave the grid 32 more pixels and therefore two more visible rows,
    // and this assertion follows the geometry rather than resisting it.
    let last = 1000;
    let first = last - (last_row_headers() - 1);
    let row_headers = app.get_row_headers();
    assert_eq!(
        row_headers.row_data((last - first) as usize).as_deref(),
        Some("1000"),
        "row 1000 must be the last visible row header"
    );
    assert_eq!(row_headers.row_data(0).as_deref(), Some("977"));
    let cells = app.get_cells();
    let cols = app.get_cols().row_count() as u32;
    assert_eq!(
        cells.row_data(((995 - first) * cols) as usize).as_deref(),
        Some("10")
    );
    assert_eq!(
        cells.row_data(((996 - first) * cols) as usize).as_deref(),
        Some("20")
    );
    assert_eq!(
        cells.row_data(((last - first) * cols) as usize).as_deref(),
        Some("tail")
    );
    // The requested -26_600 is clamped to the deepest scroll. That maximum is
    // content height minus viewport height, so reclaiming the 32 px heading
    // (UI-32) moved it up by exactly 32 px.
    assert!((app.get_grid_scroll_y() + 23_446.0).abs() < 0.1);
}

/// The number of row headers a 1280x800 headless viewport materializes. The
/// headless estimate removes the shell chrome, so this follows
/// `apply_headless_viewport_size` rather than the native resize event.
fn last_row_headers() -> u32 {
    24
}

#[test]
fn reverse_shift_extension_keeps_anchor_and_normalizes_range() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    let sheet = starter_workbook();
    update_selection(
        &app,
        &sheet,
        &evaluate(&sheet),
        CellRef::parse("C3").unwrap(),
    );

    extend_selection(&app, &sheet, -1, -1);

    assert_eq!(app.get_selected_cell().as_str(), "B2");
    assert_eq!(app.get_selection_anchor_row(), 2);
    assert_eq!(app.get_selection_anchor_col(), 2);
    assert_eq!(app.get_selection_range().as_str(), "B2:C3");
    assert_eq!(app.get_selection_count(), 4);

    // Moving back through the anchor contracts the range without
    // changing which cell was the original anchor.
    extend_selection(&app, &sheet, 1, 1);
    assert_eq!(app.get_selected_cell().as_str(), "C3");
    assert_eq!(app.get_selection_anchor_row(), 2);
    assert_eq!(app.get_selection_anchor_col(), 2);
    assert_eq!(app.get_selection_range().as_str(), "C3");
    assert_eq!(app.get_selection_count(), 1);
}

#[test]
fn fill_down_records_one_undo_operation_and_restores_sparse_cells() {
    let mut sheet = Sheet::new("fill");
    sheet.set_str("A1", "10");
    sheet.set_str("A2", "20");
    let mut undo = Vec::new();
    let mut redo = Vec::new();
    let selection =
        GridSelection::new(CellRef::parse("A1").unwrap(), CellRef::parse("A2").unwrap());

    assert!(fill_selection_down(
        &mut sheet, &mut undo, &mut redo, selection
    ));
    assert_eq!(undo.len(), 1);
    assert_eq!(sheet.raw(CellRef::parse("A3").unwrap()), Some("10"));
    assert_eq!(sheet.raw(CellRef::parse("A4").unwrap()), Some("20"));

    let previous = undo.pop().expect("fill undo operation");
    previous.revert(&mut sheet);
    assert_eq!(sheet.raw(CellRef::parse("A3").unwrap()), None);
    assert_eq!(sheet.raw(CellRef::parse("A4").unwrap()), None);
    assert!(!fill_selection_down(
        &mut sheet,
        &mut undo,
        &mut redo,
        GridSelection::new(CellRef::parse("A1").unwrap(), CellRef::parse("A1").unwrap()),
    ));
}

#[test]
fn selection_announcement_and_inspector_values_follow_live_cell_state() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    let mut sheet = Sheet::new("live");
    sheet.set_str("A1", "2");
    sheet.set_str("B1", "=A1+1");
    let values = evaluate(&sheet);

    update_selection_range(
        &app,
        &sheet,
        &values,
        GridSelection::new(CellRef::parse("B1").unwrap(), CellRef::parse("B1").unwrap()),
    );
    assert_eq!(app.get_selection_value().as_str(), "3");
    assert_eq!(
        app.get_selection_announcement().as_str(),
        "B1 selected; value: 3; formula: =A1+1"
    );
    assert_eq!(app.get_selection_formula().as_str(), "=A1+1");

    project_sheet(&app, &sheet);
    assert_eq!(app.get_sheet_name().as_str(), "live");
    assert_eq!(app.get_table_rows_label().as_str(), "1");
    assert_eq!(app.get_table_cols_label().as_str(), "2");
    assert_eq!(app.get_selection_value().as_str(), "3");
}

#[test]
fn inspector_search_filters_table_and_cell_sections() {
    assert_eq!(inspector_section_visibility(""), (true, true));
    assert_eq!(inspector_section_visibility(" rows "), (true, false));
    assert_eq!(inspector_section_visibility("formula"), (false, true));
    assert_eq!(
        inspector_section_visibility("does-not-exist"),
        (false, false)
    );
}

#[test]
fn inspector_context_switch_only_exposes_the_selected_context() {
    assert_eq!(inspector_tab_index(-1), 0);
    assert_eq!(inspector_tab_index(0), 0);
    assert_eq!(inspector_tab_index(1), 1);
    assert_eq!(inspector_tab_index(4), 1);

    assert!(inspector_context_matches(0, "rows"));
    assert!(!inspector_context_matches(1, "rows"));
    assert!(!inspector_context_matches(0, "formula"));
    assert!(inspector_context_matches(1, "formula"));
    assert!(!inspector_context_matches(1, "unknown"));
}

#[test]
fn focused_grid_routes_arrow_keys_to_selection_navigation() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    let calls = Rc::new(std::cell::Cell::new((0, 0)));
    let calls_ref = calls.clone();
    app.on_navigate_selection(move |row_delta, col_delta| {
        calls_ref.set((row_delta, col_delta));
    });

    app.invoke_focus_grid();
    app.window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed {
            text: slint::platform::Key::DownArrow.into(),
        });
    assert_eq!(calls.get(), (1, 0));

    app.window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed {
            text: slint::platform::Key::RightArrow.into(),
        });
    assert_eq!(calls.get(), (0, 1));
}

#[test]
fn focused_grid_starts_formula_edits_and_tab_navigates_selection() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    app.set_selection_formula("=A1+1".into());
    let app_ref = app.as_weak();
    app.on_begin_edit(move |initial_text| {
        if let Some(app) = app_ref.upgrade() {
            app.set_formula_edit_buffer(initial_text);
            app.invoke_focus_formula_bar();
        }
    });
    let cancels = Rc::new(std::cell::Cell::new(0));
    let cancels_ref = cancels.clone();
    app.on_cancel_selected_cell(move || cancels_ref.set(cancels_ref.get() + 1));
    app.invoke_focus_grid();
    app.window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed {
            text: slint::platform::Key::Return.into(),
        });
    assert_eq!(app.get_formula_edit_buffer().as_str(), "=A1+1");

    app.window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed {
            text: slint::platform::Key::Escape.into(),
        });
    assert_eq!(cancels.get(), 1);

    app.invoke_focus_grid();
    app.window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed { text: "x".into() });
    assert_eq!(app.get_formula_edit_buffer().as_str(), "x");

    let moves = Rc::new(std::cell::Cell::new((0, 0)));
    let moves_ref = moves.clone();
    app.on_navigate_selection(move |row_delta, col_delta| {
        moves_ref.set((row_delta, col_delta));
    });
    app.invoke_focus_grid();
    app.window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed {
            text: slint::platform::Key::Tab.into(),
        });
    assert_eq!(moves.get(), (0, 1));
}

#[test]
fn focused_grid_rejects_non_printable_edit_keys() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    let begins = Rc::new(std::cell::Cell::new(0));
    let begins_ref = begins.clone();
    app.on_begin_edit(move |_| begins_ref.set(begins_ref.get() + 1));
    let moves = Rc::new(std::cell::Cell::new((0, 0)));
    let moves_ref = moves.clone();
    app.on_navigate_selection(move |row_delta, col_delta| {
        moves_ref.set((row_delta, col_delta));
    });

    for text in [
        slint::platform::Key::Backspace.into(),
        slint::platform::Key::Delete.into(),
        slint::platform::Key::F1.into(),
        slint::platform::Key::Home.into(),
        slint::platform::Key::PageUp.into(),
    ] {
        app.invoke_focus_grid();
        app.window()
            .dispatch_event(slint::platform::WindowEvent::KeyPressed { text });
    }
    assert_eq!(begins.get(), 0);
    assert_eq!(moves.get(), (0, 0));

    app.invoke_focus_grid();
    app.window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed {
            text: slint::platform::Key::Backtab.into(),
        });
    assert_eq!(begins.get(), 0);
    assert_eq!(moves.get(), (0, -1));
}

#[test]
fn typed_history_undo_redo_restores_exact_raw_values() {
    let mut sheet = Sheet::new("history");
    let cell = CellRef::parse("A1").unwrap();
    sheet.set_raw(cell, "old");
    let mut undo = Vec::new();
    let mut redo = Vec::new();

    assert!(commit_formula_edit(
        &mut sheet, &mut undo, &mut redo, cell, ""
    ));
    assert_eq!(sheet.raw(cell), Some(""));
    let edit = undo.pop().expect("typed edit");
    edit.revert(&mut sheet);
    assert_eq!(sheet.raw(cell), Some("old"));
    redo.push(edit);
    let edit = redo.pop().expect("redo edit");
    edit.apply(&mut sheet);
    assert_eq!(sheet.raw(cell), Some(""));

    let absent = CellRef::parse("B1").unwrap();
    assert!(commit_formula_edit(
        &mut sheet, &mut undo, &mut redo, absent, ""
    ));
    assert_eq!(sheet.raw(absent), Some(""));
    undo.pop().expect("absent edit").revert(&mut sheet);
    assert_eq!(sheet.raw(absent), None);
}

#[test]
fn quick_formula_insert_evaluation() {
    let mut sheet = Sheet::new("test");
    for (c, v) in [
        ("A1", "10"),
        ("A2", "20"),
        ("A3", "30"),
        ("A4", "40"),
        ("A5", "50"),
    ] {
        sheet.set_str(c, v);
    }

    let target = CellRef::parse("B1").unwrap();
    let mut undo = Vec::new();
    let mut redo = Vec::new();

    assert!(commit_formula_edit(
        &mut sheet,
        &mut undo,
        &mut redo,
        target,
        "=SUM(A1:A5)",
    ));

    let vals = evaluate(&sheet);
    assert_eq!(vals.get(&target), Some(&Value::Number(150.0)));
}

#[test]
fn typing_is_mirrored_inside_the_active_cell() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    app.window().set_size(PhysicalSize::new(1280, 800));
    apply_layout_breakpoints(&app, 1280);
    apply_headless_viewport_size(&app, 1280, 800);
    project_sheet(&app, &Sheet::new("Empty"));

    let cell_pixels = |image: &image::RgbaImage| -> Vec<[u8; 4]> {
        // Only A1's pixels: below the pinned column header, right of the row
        // header, and clear of the formula bar so its own text cannot count.
        let mut pixels = Vec::new();
        for y in 132..156 {
            for x in 38..114 {
                pixels.push(image.get_pixel(x, y).0);
            }
        }
        pixels
    };
    let idle = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("idle frame");
    app.set_is_editing(true);
    app.set_formula_edit_buffer("typed text".into());
    let editing = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("editing frame");
    assert_ne!(
        cell_pixels(&idle),
        cell_pixels(&editing),
        "the active cell must show the text being typed"
    );
}

#[test]
fn a_pointer_click_on_a_cell_reports_that_cell() {
    use slint::platform::{PointerEventButton, WindowEvent};
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    app.window().set_size(PhysicalSize::new(1280, 800));
    apply_layout_breakpoints(&app, 1280);
    apply_headless_viewport_size(&app, 1280, 800);
    project_sheet(&app, &Sheet::new("Empty"));
    snapshot_component(&app, 1280.0, 800.0, 1.0).expect("lay out the window");

    let clicked = Rc::new(std::cell::Cell::new(None));
    {
        let clicked = clicked.clone();
        app.on_cell_clicked(move |row, col| clicked.set(Some((row, col))));
    }
    let cell = i_slint_backend_testing::ElementHandle::find_by_accessible_label(&app, "C3")
        .next()
        .expect("cell C3 is in the accessibility tree");
    let position = cell.absolute_position();
    let size = cell.size();
    let point = slint::LogicalPosition::new(
        position.x + size.width / 2.0,
        position.y + size.height / 2.0,
    );
    eprintln!("CLICK cell at {position:?} size {size:?} -> {point:?}");
    let window = app.window();
    window.dispatch_event(WindowEvent::PointerMoved { position: point });
    window.dispatch_event(WindowEvent::PointerPressed {
        position: point,
        button: PointerEventButton::Left,
    });
    // Let the delayed press fire, as a real click's duration would.
    std::thread::sleep(std::time::Duration::from_millis(250));
    slint::platform::update_timers_and_animations();
    window.dispatch_event(WindowEvent::PointerReleased {
        position: point,
        button: PointerEventButton::Left,
    });

    assert_eq!(
        clicked.get(),
        Some((2, 2)),
        "clicking C3 must select row 2, column 2"
    );
}
