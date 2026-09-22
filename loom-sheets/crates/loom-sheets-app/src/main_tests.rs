use super::*;
use loom_package::manifest::{json as pkg_json, Checksum, Manifest, ManifestEntry};
use loom_package::{MimeType, PackageArchive, PackageKind, SchemaVersion};
use loom_sheets_core::persistence::sheet_from_json;
use slint::Model;

#[test]
fn rtl_argument_is_parsed_and_applied_to_the_root() {
    let args = parse_args_from(["--rtl"] as [&str; 1]).expect("parse --rtl");
    assert!(args.rtl);

    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    configure_direction(&app, args.rtl);
    assert!(app.get_rtl());
}

#[test]
fn objects_argument_enables_the_native_object_fixture() {
    let args = parse_args_from(["--objects"] as [&str; 1]).expect("parse --objects");
    assert!(args.objects);
}

#[test]
fn example_argument_is_explicit() {
    let blank = parse_args_from(["--screenshot", "/tmp/blank.png"] as [&str; 2])
        .expect("parse blank screenshot");
    assert!(!blank.example);
    let example = parse_args_from(["--example"] as [&str; 1]).expect("parse --example");
    assert!(example.example);
}

#[test]
fn inspector_capture_flag_is_supported() {
    let args = parse_args_from(["--inspector"] as [&str; 1]).expect("parse --inspector");
    assert!(args.inspector);
}

#[test]
fn new_workbook_is_blank_and_named_untitled() {
    let sheet = blank_sheet();
    assert!(sheet.cells.is_empty());
    assert_eq!(sheet.name, "Untitled");
}

#[test]
fn example_workbook_uses_one_unit_and_live_formulas() {
    let sheet = starter_workbook();
    assert_eq!(sheet.name, "Example Budget");
    assert_eq!(
        sheet.raw(CellRef::parse("B1").unwrap()),
        Some("Amount (USD/month)")
    );
    assert_eq!(sheet.col_width(1), 190.0);
    assert_eq!(sheet.raw(CellRef::parse("C2").unwrap()), Some("Monthly"));
    assert_eq!(sheet.raw(CellRef::parse("C3").unwrap()), Some("Monthly"));
    assert_eq!(sheet.raw(CellRef::parse("C4").unwrap()), Some("Monthly"));
    assert_eq!(
        sheet.raw(CellRef::parse("B5").unwrap()),
        Some("=SUM(B2:B4)")
    );
    assert_eq!(
        sheet.raw(CellRef::parse("B6").unwrap()),
        Some("=AVERAGE(B2:B4)")
    );
    let values = evaluate(&sheet);
    assert_eq!(
        values.get(&CellRef::parse("B5").unwrap()),
        Some(&Value::Number(1800.0))
    );
    assert_eq!(
        values.get(&CellRef::parse("B6").unwrap()),
        Some(&Value::Number(600.0))
    );
}

#[test]
fn scripted_dialog_request_uses_current_workbook_directory() {
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new(
        [Some(PathBuf::from("/tmp/import.csv"))],
        [Some(PathBuf::from("/tmp/workbook.loomtable"))],
    ));
    let state = GuiState::new(
        starter_workbook(),
        Some(PathBuf::from("/tmp/current.loomtable")),
        dialogs,
        FileFilter::new("Workbook", ["loomtable"]).expect("filter"),
        FileFilter::new("CSV", ["csv"]).expect("filter"),
        FileFilter::new("CSV", ["csv"]).expect("filter"),
        FileFilter::new("Excel", ["xlsx"]).expect("filter"),
    );
    let request = open_request(&state);
    assert_eq!(request.initial_directory, Some(PathBuf::from("/tmp")));
    assert_eq!(
        state.dialogs.open_file(&request).expect("open"),
        Some(PathBuf::from("/tmp/import.csv"))
    );
}

#[test]
fn csv_import_does_not_become_native_save_target() {
    assert!(!is_native_workbook(Path::new("budget.csv")));
    assert!(is_native_workbook(Path::new("budget.loomtable")));
}

#[test]
fn formula_bar_draft_is_not_applied_before_commit() {
    let mut sheet = Sheet::new("test");
    let selected = CellRef::parse("B1").unwrap();
    sheet.set_str("A1", "2");
    sheet.set_str("B1", "3");

    let mut edit = CellEditTransaction::begin(sheet.raw(selected));
    edit.update("=A1+1");

    assert_eq!(selected.to_a1(), "B1");
    assert_eq!(edit.commit().unwrap().after(), "=A1+1");
    assert_eq!(sheet.raw(selected), Some("3"));
}

#[test]
fn formula_bar_commit_preserves_formula_raw_and_selected_cell() {
    let mut sheet = Sheet::new("test");
    let selected = CellRef::parse("B1").unwrap();
    sheet.set_str("A1", "2");
    sheet.set_str("B1", "3");
    let mut undo = Vec::new();
    let mut redo = Vec::new();

    assert!(commit_formula_edit(
        &mut sheet, &mut undo, &mut redo, selected, "=A1+1",
    ));

    assert_eq!(selected.to_a1(), "B1");
    assert_eq!(sheet.raw(selected), Some("=A1+1"));
    assert_eq!(evaluate(&sheet).get(&selected), Some(&Value::Number(3.0)));
}

#[test]
fn formula_bar_commit_preserves_literal_and_empty_raw_text() {
    let mut sheet = Sheet::new("test");
    let literal = CellRef::parse("A1").unwrap();
    let empty = CellRef::parse("B1").unwrap();
    sheet.set_raw(literal, "old");
    sheet.set_raw(empty, "old");
    let mut undo = Vec::new();
    let mut redo = Vec::new();

    assert!(commit_formula_edit(
        &mut sheet,
        &mut undo,
        &mut redo,
        literal,
        "  literal text  ",
    ));
    assert!(commit_formula_edit(
        &mut sheet, &mut undo, &mut redo, empty, "",
    ));

    assert_eq!(sheet.raw(literal), Some("  literal text  "));
    assert_eq!(sheet.raw(empty), Some(""));
    assert_eq!(evaluate(&sheet).get(&empty), Some(&Value::Empty));
}

#[test]
fn formula_bar_commit_records_one_transaction_and_noop_records_none() {
    let mut sheet = Sheet::new("test");
    let selected = CellRef::parse("A1").unwrap();
    sheet.set_str("A1", "old");
    let mut undo = Vec::new();
    let mut redo = vec![SheetTransaction::Range(RangeEdit::replace(
        &sheet,
        selected,
        Some("redo".to_string()),
    ))];

    assert!(commit_formula_edit(
        &mut sheet, &mut undo, &mut redo, selected, "new",
    ));
    assert_eq!(undo.len(), 1);
    assert!(redo.is_empty());

    assert!(!commit_formula_edit(
        &mut sheet, &mut undo, &mut redo, selected, "new",
    ));
    assert_eq!(undo.len(), 1);
}

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

    assert_eq!(app.get_object_kinds().row_count(), 2);
    assert_eq!(app.get_object_kinds().row_data(0).as_deref(), Some("shape"));
    assert_eq!(
        app.get_object_labels().row_data(0).as_deref(),
        Some("Callout")
    );
    assert!(app.get_visible_objects().row_data(0).unwrap_or(false));
    assert!(!app.get_visible_objects().row_data(1).unwrap_or(true));
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
        .get_object_images()
        .row_data(0)
        .expect("projected image");
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
    project_current(&app, &state);

    app.invoke_object_move_started(0);
    assert_eq!(app.get_selected_object(), 0);
    app.invoke_object_moved(0, 80.0, 24.0);
    assert_eq!(
        state.current.borrow().objects[0].anchor,
        CellRef { row: 1, col: 1 }
    );
    assert!(state.undo_stack.borrow().is_empty());
    app.invoke_object_move_ended(0);
    assert_eq!(state.undo_stack.borrow().len(), 1);

    app.invoke_undo();
    assert_eq!(
        state.current.borrow().objects[0].anchor,
        CellRef { row: 0, col: 0 }
    );

    app.invoke_object_resize_started(0);
    app.invoke_object_resized(0, 100.0, 100.0);
    assert_eq!(
        (
            state.current.borrow().objects[0].width,
            state.current.borrow().objects[0].height
        ),
        (340, 212)
    );
    app.invoke_object_resize_ended(0);
    assert_eq!(state.undo_stack.borrow().len(), 1);
    app.invoke_undo();
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
    let viewport = viewport_from_app(&app, &sheet);

    assert_eq!(sheet.dimensions(), SheetDimensions::new(1_000, 52));
    assert_eq!(viewport.first_row, 28);
    assert_eq!(viewport.first_col, 2);
    assert_eq!(viewport.visible_rows, 11);
    assert_eq!(viewport.visible_cols, 5);
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

    let row_headers = app.get_row_headers();
    assert_eq!(row_headers.row_data(0).as_deref(), Some("979"));
    assert_eq!(row_headers.row_data(21).as_deref(), Some("1000"));
    let cells = app.get_cells();
    assert_eq!(cells.row_data(16 * 8).as_deref(), Some("10"));
    assert_eq!(cells.row_data(17 * 8).as_deref(), Some("20"));
    assert_eq!(cells.row_data(21 * 8).as_deref(), Some("tail"));
    assert!((app.get_grid_scroll_y() + 23_478.0).abs() < 0.1);
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
fn workbook_file_roundtrip_preserves_tabs_styles_and_freeze() {
    let dir =
        std::env::temp_dir().join(format!("loom-sheets-workbook-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("test temp dir");
    let path = dir.join("tabs.loomtable");

    let mut first = Sheet::new("First");
    first.set_str("A1", "10");
    first.set_str("B1", "=A1+1");
    first.freeze_panes(1, 0);
    let mut second = Sheet::new("Second");
    second.set_str("A1", "styled");
    second.set_cell_alignment(
        CellRef::parse("A1").unwrap(),
        loom_sheets_core::style::CellAlignment::Center,
    );

    save_workbook(&path, &[first, second], 1).expect("save workbook");
    let workbook = load_workbook(&path).expect("load workbook");
    assert_eq!(workbook.sheets.len(), 2);
    assert_eq!(workbook.active, 1);
    assert_eq!(workbook.sheets[0].name, "First");
    assert_eq!(
        workbook.sheets[0].raw(CellRef::parse("B1").unwrap()),
        Some("=A1+1")
    );
    assert_eq!(workbook.sheets[0].freeze_rows, 1);
    assert_eq!(workbook.sheets[1].name, "Second");
    assert_eq!(
        workbook.sheets[1].cell_alignment(CellRef::parse("A1").unwrap()),
        loom_sheets_core::style::CellAlignment::Center
    );
    // Single-sheet convenience loader returns the active tab.
    let active = load_sheet(&path).expect("load active sheet");
    assert_eq!(active.name, "Second");
    std::fs::remove_file(&path).ok();
}

#[test]
fn loomsheet_embeds_image_payload_and_reopens_without_source_file() {
    const PNG_BYTES: &[u8] = &[
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 4,
        0, 0, 0, 181, 28, 12, 2, 0, 0, 0, 11, 73, 68, 65, 84, 120, 218, 99, 100, 248, 15, 0, 1, 5,
        1, 1, 39, 24, 227, 102, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ];
    let dir = std::env::temp_dir().join(format!(
        "loom-sheets-embedded-image-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("test temp dir");
    let source = dir.join("source.png");
    let package = dir.join("embedded.loomtable");
    std::fs::write(&source, PNG_BYTES).expect("source image");

    let mut sheet = Sheet::new("Images");
    sheet.objects.push(
        loom_sheets_core::SheetObject::image(
            CellRef { row: 1, col: 1 },
            source.to_string_lossy().into_owned(),
        )
        .expect("image object"),
    );
    save_workbook(&package, &[sheet], 0).expect("save package");

    let archive = PackageArchive::from_bytes(&std::fs::read(&package).expect("package bytes"))
        .expect("valid package");
    assert!(archive
        .paths()
        .iter()
        .any(|path| path.starts_with("content/assets/")));
    std::fs::remove_file(&source).expect("remove source");

    let workbook = load_workbook(&package).expect("load package without source");
    assert_eq!(
        workbook.sheets[0].objects[0].embedded.as_deref(),
        Some(PNG_BYTES)
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn recovery_snapshot_restores_embedded_image_without_original_path() {
    const PNG_BYTES: &[u8] = &[
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 4,
        0, 0, 0, 181, 28, 12, 2, 0, 0, 0, 11, 73, 68, 65, 84, 120, 218, 99, 100, 248, 15, 0, 1, 5,
        1, 1, 39, 24, 227, 102, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ];
    let mut sheet = Sheet::new("Recovered images");
    let mut image = loom_sheets_core::SheetObject::image(
        CellRef { row: 1, col: 1 },
        "/path/that/no-longer-exists/source.png",
    )
    .expect("image object");
    image.embedded = Some(PNG_BYTES.to_vec());
    sheet.objects.push(image);

    let snapshot = workbook_package_bytes(&[sheet], 0).expect("build recovery package");
    let recovered = restore_workbook_from_snapshot(&snapshot).expect("restore snapshot");
    assert_eq!(
        recovered.sheets[0].objects[0].embedded.as_deref(),
        Some(PNG_BYTES)
    );

    // A recovered image can still be exported even though its original path
    // cannot be read anymore.
    let exported = loom_sheets_core::export_xlsx_sheets(&recovered.sheets)
        .expect("recovered workbook exports");
    let archive = PackageArchive::from_bytes(&exported).expect("xlsx archive");
    assert_eq!(archive.get("xl/media/sheet1-object0.png"), Some(PNG_BYTES));
}

#[test]
fn xlsx_workbook_load_imports_all_tabs_and_formulas() {
    let dir = std::env::temp_dir().join(format!(
        "loom-sheets-xlsx-import-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("test temp dir");
    let path = dir.join("tabs.xlsx");

    let mut first = Sheet::new("First");
    first.set_str("A1", "10");
    first.set_str("B1", "=A1*2");
    let mut second = Sheet::new("Second");
    second.set_str("A1", "=First!B1+5");
    let sheets = vec![first.clone(), second.clone()];
    let bytes = loom_sheets_core::export_xlsx_sheets(&sheets).expect("xlsx export");
    std::fs::write(&path, bytes).expect("write xlsx");

    let workbook = load_workbook(&path).expect("xlsx import");
    assert_eq!(workbook.sheets.len(), 2);
    assert_eq!(workbook.sheets[0].name, "First");
    assert_eq!(workbook.sheets[1].name, "Second");
    assert_eq!(
        workbook.sheets[0].raw(CellRef::parse("B1").unwrap()),
        Some("=A1*2")
    );
    assert_eq!(
        workbook.sheets[1].raw(CellRef::parse("A1").unwrap()),
        Some("=First!B1+5")
    );

    std::fs::remove_file(&path).ok();
    std::fs::remove_dir(&dir).ok();
}

#[test]
fn legacy_single_sheet_package_loads_as_one_tab() {
    let dir = std::env::temp_dir().join(format!("loom-sheets-legacy-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("test temp dir");
    let path = dir.join("legacy.loomtable");

    let mut sheet = Sheet::new("Legacy");
    sheet.set_str("A1", "7");
    let json = sheet_to_json(&sheet);
    let mut arch = PackageArchive::new();
    arch.add("content/sheet.json", json.clone().into_bytes())
        .expect("legacy entry");
    let manifest = Manifest {
        schema: SchemaVersion::CURRENT,
        kind: PackageKind::Sheets,
        id: "sheets-doc".to_string(),
        title: sheet.name.clone(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        entries: vec![ManifestEntry {
            path: "content/sheet.json".into(),
            mime: MimeType::parse("application/vnd.loom.sheet-content").expect("mime"),
            size: json.len() as u64,
            sha256: Checksum::from_bytes(loom_package::zip::sha256(json.as_bytes())),
        }],
    };
    arch.add("manifest.json", pkg_json::write(&manifest).into_bytes())
        .expect("manifest entry");
    let bytes = arch.to_bytes().expect("archive bytes");
    std::fs::write(&path, &bytes).expect("write legacy package");

    let workbook = load_workbook(&path).expect("legacy loads");
    assert_eq!(workbook.sheets.len(), 1);
    assert_eq!(workbook.active, 0);
    assert_eq!(workbook.sheets[0].name, "Legacy");
    assert_eq!(
        workbook.sheets[0].raw(CellRef::parse("A1").unwrap()),
        Some("7")
    );
    std::fs::remove_file(&path).ok();
}

#[test]
fn native_menu_and_palette_share_sheets_callback_dispatch() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    let calls = Rc::new(std::cell::Cell::new(0));
    let calls_ref = calls.clone();
    app.on_save_sheet(move || calls_ref.set(calls_ref.get() + 1));

    assert!(dispatch_command(&app, "file.save"));
    assert!(dispatch_palette_action(&app, PaletteAction::SaveSheet));

    let menu = NativeMenuBar::new();
    let bar = build_standard_menu_bar("Loom Sheets", vec![], vec![], vec![], vec![]);
    menu.install_menu_bar(&bar).expect("install menu");
    let app_ref = app.as_weak();
    menu.register_action_sink(Arc::new(move |action: CommandAction| {
        schedule_menu_action(&app_ref, action)
    }))
    .expect("register menu sink");
    let error = menu
        .dispatch_action("file.save")
        .expect_err("capture platform has no event loop provider");
    assert!(error
        .to_string()
        .contains("failed to schedule Sheets menu command"));

    assert_eq!(calls.get(), 2);
}

#[test]
fn sheets_palette_undo_redo_follow_history_and_disabled_guard() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    app.set_can_undo(false);
    app.set_can_redo(false);
    rebuild_palette(&app, "undo");
    assert_eq!(app.get_palette_commands().row_count(), 0);
    rebuild_palette(&app, "redo");
    assert_eq!(app.get_palette_commands().row_count(), 0);
    assert!(!dispatch_palette_action(&app, PaletteAction::Undo));
    assert!(!dispatch_palette_action(&app, PaletteAction::Redo));

    let undo_calls = Rc::new(std::cell::Cell::new(0));
    let undo_calls_ref = undo_calls.clone();
    app.on_undo(move || undo_calls_ref.set(undo_calls_ref.get() + 1));
    app.set_can_undo(true);
    rebuild_palette(&app, "undo");
    assert!(
        app.get_palette_commands()
            .row_data(0)
            .expect("Undo command")
            .enabled
    );
    assert!(dispatch_palette_action(&app, PaletteAction::Undo));
    assert_eq!(undo_calls.get(), 1);
}

#[test]
fn palette_exposes_keyboard_paths_for_every_primary_command() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    // Mouse-only operations (decimals, sort-by-column, fill, charts) must be
    // reachable keyboard-first through the palette.
    for id in [
        "table.adjust_decimals_increase",
        "table.adjust_decimals_decrease",
        "sheets.organize",
        "table.fill_down",
        "sheets.insert_chart",
        "sheets.insert_shape",
        "sheets.insert_image",
        "sheets.cycle_chart_kind",
        "table.pivot_sum",
        "table.pivot_count",
        "table.pivot_average",
        "table.pivot_min",
        "table.pivot_max",
        "format.number",
        "format.borders",
        "format.fill_cycle",
        "format.font_increase",
        "format.font_decrease",
    ] {
        assert!(palette_action_for_id(id).is_some(), "palette resolves {id}");
        assert!(dispatch_command(&app, id), "dispatch routes {id}");
    }
    app.set_can_undo(true);
    app.set_can_redo(true);
    rebuild_palette(&app, "chart");
    assert!(
        app.get_palette_commands().row_count() >= 2,
        "chart commands are searchable"
    );
}

#[test]
fn sheets_palette_invocation_uses_rendered_row_when_history_changes() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    let redo_calls = Rc::new(std::cell::Cell::new(0));
    let redo_calls_ref = redo_calls.clone();
    app.on_redo(move || redo_calls_ref.set(redo_calls_ref.get() + 1));

    // Render both history commands, then invalidate Undo before the user
    // presses Enter. The visible Redo row is still at the last index;
    // resolving against a freshly filtered master list would incorrectly
    // look out of range or shift to the wrong command.
    app.set_can_undo(true);
    app.set_can_redo(true);
    wire_palette(&app);
    rebuild_palette(&app, "");
    assert_eq!(app.get_palette_commands().row_count(), 49);
    let redo_row = app.get_palette_commands().row_count() - 1;
    assert_eq!(
        app.get_palette_commands()
            .row_data(redo_row)
            .expect("rendered Redo row")
            .id
            .as_str(),
        "sheets.redo"
    );

    app.set_can_undo(false);
    app.invoke_palette_invoked(redo_row as i32);

    assert_eq!(redo_calls.get(), 1);
    assert!(!app.get_palette_open());
}

#[test]
fn sheets_menu_disables_unhandled_controller_commands() {
    set_platform();
    let mut menu = build_standard_menu_bar(
        "Loom Sheets",
        vec![
            MenuItem::action("file.new_template", "New from Template..."),
            MenuItem::action("file.export_csv", "Export to CSV..."),
            MenuItem::action("file.export_xlsx", "Export to Excel (.xlsx)..."),
        ],
        vec![],
        vec![MenuItem::check("view.inspector", "Format Inspector", false)],
        vec![Menu::new(
            "Table",
            [
                MenuItem::action("table.add_row", "Add Row"),
                MenuItem::action("table.delete_row", "Delete Row"),
                MenuItem::action("table.add_col", "Add Column"),
                MenuItem::action("table.delete_col", "Delete Column"),
                MenuItem::action("table.sort_asc", "Sort Ascending"),
                MenuItem::action("table.sort_desc", "Sort Descending"),
                MenuItem::action("table.freeze_header", "Freeze Header Row"),
                MenuItem::action("table.unfreeze_panes", "Unfreeze Panes"),
                MenuItem::action("table.pivot_sum", "Pivot Summary (Sum)"),
                MenuItem::action("sheets.insert_shape", "Insert Shape"),
                MenuItem::action("sheets.insert_image", "Insert Image"),
                MenuItem::action("sheets.delete_sheet", "Delete Sheet"),
            ],
        )],
    );
    menu.disable_items_except([
        "file.new",
        "file.new_template",
        "file.open",
        "file.save",
        "file.save_as",
        "file.export_csv",
        "file.export_xlsx",
        "edit.undo",
        "edit.redo",
        "edit.cut",
        "edit.copy",
        "edit.paste",
        "edit.select_all",
        "app.palette",
        "view.inspector",
        "view.zoom_in",
        "view.zoom_out",
        "view.zoom_actual",
        "table.add_row",
        "table.delete_row",
        "table.add_col",
        "table.delete_col",
        "table.sort_asc",
        "table.sort_desc",
        "table.freeze_header",
        "table.unfreeze_panes",
        "table.pivot_sum",
        "sheets.insert_shape",
        "sheets.insert_image",
        "sheets.delete_sheet",
    ]);
    for id in [
        "view.zoom_in",
        "view.zoom_out",
        "view.zoom_actual",
        "edit.cut",
        "edit.copy",
        "edit.paste",
        "edit.select_all",
        "table.add_row",
        "table.delete_row",
        "table.add_col",
        "table.delete_col",
        "table.sort_asc",
        "table.sort_desc",
        "table.freeze_header",
        "table.unfreeze_panes",
        "table.pivot_sum",
        "sheets.delete_sheet",
    ] {
        assert!(
            menu.find_item(id).expect("menu command").is_enabled(),
            "handled Sheets command {id} must be enabled"
        );
    }
}

#[test]
fn sheets_inspector_menu_check_tracks_live_window_state() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], []));
    let state = GuiState::new(
        starter_workbook(),
        None,
        dialogs,
        FileFilter::new("Workbook", ["loomtable"]).expect("filter"),
        FileFilter::new("CSV", ["csv"]).expect("filter"),
        FileFilter::new("CSV", ["csv"]).expect("filter"),
        FileFilter::new("Excel", ["xlsx"]).expect("filter"),
    );
    let menu = NativeMenuBar::new();
    let bar = build_standard_menu_bar(
        "Loom Sheets",
        vec![],
        vec![],
        vec![MenuItem::check("view.inspector", "Format Inspector", false)],
        vec![],
    );
    menu.install_menu_bar(&bar).expect("install menu");

    app.set_inspector_available(true);
    app.set_show_inspector(false);
    sync_menu_state(&menu, &app, &state);
    assert!(matches!(
        menu.installed_menu_bar()
            .and_then(|bar| bar.find_item("view.inspector").cloned()),
        Some(MenuItem::Check {
            checked: false,
            enabled: true,
            ..
        })
    ));

    app.set_show_inspector(true);
    sync_menu_state(&menu, &app, &state);
    assert!(matches!(
        menu.installed_menu_bar()
            .and_then(|bar| bar.find_item("view.inspector").cloned()),
        Some(MenuItem::Check {
            checked: true,
            enabled: true,
            ..
        })
    ));
}

#[test]
fn expanding_past_overflow_breakpoint_closes_menu() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    apply_layout_breakpoints(&app, 1024);
    assert!(app.get_overflow_toolbar());
    app.set_toolbar_overflow_open(true);

    apply_layout_breakpoints(&app, 1320);

    assert!(!app.get_overflow_toolbar());
    assert!(!app.get_toolbar_overflow_open());
}

#[test]
fn widening_window_preserves_palette_focus() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    apply_layout_breakpoints(&app, 1024);
    wire_responsive_layout(&app);
    let _ = snapshot_component(&app, 1024.0, 800.0, 1.0).expect("render compact window");

    app.invoke_open_palette();
    let _ = snapshot_component(&app, 1024.0, 800.0, 1.0).expect("render open palette");
    let focused_before =
        slint::private_unstable_api::re_exports::WindowInner::from_pub(app.window())
            .focus_item
            .borrow()
            .upgrade()
            .expect("palette should own focus");

    app.window().set_size(PhysicalSize::new(1280, 800));
    let _ = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render widened window");
    let focused_after =
        slint::private_unstable_api::re_exports::WindowInner::from_pub(app.window())
            .focus_item
            .borrow()
            .upgrade()
            .expect("palette focus should remain present");

    assert_eq!(focused_after, focused_before);
    assert!(app.get_palette_open());
}

#[test]
fn delete_sheet_is_undoable_and_restores_content() {
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
    let menu_arc = std::sync::Arc::new(NativeMenuBar::new());

    let mut second = Sheet::new("Second");
    second.set_str("A1", "keep-me");
    state.sheets.borrow_mut().push(second);
    state
        .sheet_histories
        .borrow_mut()
        .push((Vec::new(), Vec::new()));
    *state.active_sheet_index.borrow_mut() = 1;
    *state.current.borrow_mut() = state.sheets.borrow()[1].clone();

    assert!(delete_active_sheet(&app, &state, &menu_arc));
    assert_eq!(state.sheets.borrow().len(), 1);
    assert_eq!(state.undo_stack.borrow().len(), 1);

    // Undo through the recorded workbook transaction restores the tab.
    let tx = state.undo_stack.borrow_mut().pop().unwrap();
    match &tx {
        SheetTransaction::Workbook { before, .. } => before.restore(&state),
        _ => panic!("expected workbook transaction"),
    }
    assert_eq!(state.sheets.borrow().len(), 2);
    assert_eq!(
        state.sheets.borrow()[1].raw(CellRef::parse("A1").unwrap()),
        Some("keep-me")
    );

    // The only remaining sheet refuses deletion instead of emptying the book.
    *state.active_sheet_index.borrow_mut() = 0;
    *state.current.borrow_mut() = state.sheets.borrow()[0].clone();
    state.sheets.borrow_mut().truncate(1);
    assert!(!delete_active_sheet(&app, &state, &menu_arc));
    assert_eq!(state.sheets.borrow().len(), 1);
}

#[test]
fn chart_sync_projects_kind_paths_and_hides_without_spec() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");

    // No spec: sync hides the overlay instead of showing stale defaults.
    let mut bare = Sheet::new("Bare");
    bare.set_str("A2", "Q1");
    bare.set_str("B2", "5");
    app.set_chart_visible(true);
    sync_chart_to_app(&app, &bare);
    assert!(!app.get_chart_visible());

    // With a spec: kind, line path, and per-slice pie paths project live.
    let mut sheet = Sheet::new("Sales");
    sheet.set_str("A1", "Quarter");
    sheet.set_str("B1", "Revenue (USD)");
    sheet.set_str("A2", "Q1");
    sheet.set_str("B2", "15000");
    sheet.set_str("A3", "Q2");
    sheet.set_str("B3", "18500");
    sheet.chart = Some(loom_sheets_core::SheetChart {
        kind: loom_sheets_core::ChartKind::Pie,
        title: "Sales Chart".to_string(),
        cat_col: 0,
        val_col: 1,
        ..Default::default()
    });
    app.set_chart_visible(true);
    sync_chart_to_app(&app, &sheet);
    assert!(app.get_chart_visible());
    assert_eq!(app.get_chart_kind().as_str(), "pie");
    assert_eq!(app.get_chart_title().as_str(), "Sales Chart");
    assert_eq!(app.get_chart_source_range().as_str(), "A1:B3");
    assert_eq!(app.get_chart_series_label().as_str(), "Revenue");
    assert_eq!(app.get_chart_unit_label().as_str(), "USD");
    assert_eq!(app.get_chart_unit_label().as_str(), "USD");
    assert!(!app.get_chart_line_commands().is_empty());
    assert_eq!(app.get_chart_pie_commands().row_count(), 2);
    assert_eq!(app.get_chart_pie_opacities().row_count(), 2);
}

#[test]
fn every_template_card_creates_its_advertised_sheet() {
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
    register_sheet_actions(&app, &state, &menu_service);

    assert_eq!(app.get_template_recents().row_count(), 0);

    // (template index, expected sheet name, probe cell, expected display)
    let cases = [
        (0, "Untitled", "A1", ""),
        (1, "Monthly Budget", "B4", "1600"),
        (2, "Invoice", "D2", "1700"),
        (3, "Checklist", "B7", "2"),
        (4, "Table and Chart", "B4", "33500"),
        (5, "Expense Summary", "B8", "405"),
        (6, "Sales Chart", "B6", "20400"),
        (7, "Budget", "B4", "2100"),
        (8, "Monthly Goal", "B4", "7000"),
        (9, "Portfolio", "D4", "5150"),
        (10, "Net Worth", "B6", "19300"),
    ];
    for (idx, name, probe, expected) in cases {
        app.invoke_create_template(idx);
        let current = state.current.borrow().clone();
        assert_eq!(current.name, name, "template {idx}");
        let vals = evaluate(&current);
        let cell = CellRef::parse(probe).unwrap();
        assert_eq!(
            vals.get(&cell).map(|value| value.display()).as_deref(),
            if expected.is_empty() {
                None
            } else {
                Some(expected)
            },
            "template {idx} probe {probe}"
        );
    }
    assert_eq!(
        app.get_template_recents().iter().collect::<Vec<_>>(),
        vec![10, 9, 8]
    );
    app.invoke_create_template(9);
    assert_eq!(
        app.get_template_recents().iter().collect::<Vec<_>>(),
        vec![9, 10, 8]
    );
    app.invoke_create_template(0);
    assert_eq!(
        app.get_template_recents().iter().collect::<Vec<_>>(),
        vec![0, 9, 10]
    );
}

#[test]
fn addressable_grid_fills_window_and_keeps_legacy_minimums() {
    let mut sheet = Sheet::new("fill");
    sheet.set_str("B2", "x");
    let a1 = CellRef::parse("A1").unwrap();
    // No measured window: legacy 15x8 minimums exactly.
    assert_eq!(
        editor_dimensions(&sheet, a1, None),
        SheetDimensions::new(15, 8)
    );
    // A measured window fills plus scroll-ahead margin into the void.
    let dims = editor_dimensions(&sheet, a1, Some((11, 21)));
    assert_eq!((dims.cols, dims.rows), (11 + 12, 21 + 30));
    // Used cells still dominate sparse workbooks.
    let mut big = Sheet::new("big");
    big.set_str("AZ1000", "tail");
    let big_dims = editor_dimensions(&big, a1, Some((11, 21)));
    assert_eq!((big_dims.rows, big_dims.cols), (1_000, 52));
}

#[test]
fn history_stacks_evict_oldest_past_the_bound() {
    let cell = CellRef::parse("A1").unwrap();
    let mut stack = Vec::new();
    for n in 0..(MAX_HISTORY_ENTRIES + 5) {
        let edit = RangeEdit::replace(&Sheet::new("cap"), cell, Some(format!("v{n}")));
        push_history(&mut stack, SheetTransaction::Range(edit));
    }
    assert_eq!(stack.len(), MAX_HISTORY_ENTRIES);
    // The surviving oldest entry is v5: v0..v4 were evicted in order.
    match &stack[0] {
        SheetTransaction::Range(edit) => {
            let mut probe = Sheet::new("cap");
            edit.apply(&mut probe);
            assert_eq!(probe.raw(cell), Some("v5"));
        }
        _ => panic!("expected range transaction"),
    }
}

fn cross_sheet_state() -> Rc<GuiState> {
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], []));
    let mut data = Sheet::new("Data");
    data.set_str("A1", "10");
    data.set_str("B1", "=A1*2");
    let mut report = Sheet::new("Report");
    report.set_str("A1", "=Data!B1+5");
    report.set_str("A2", "=SUM(Data!A1:A1)");
    let state = Rc::new(GuiState::new(
        data,
        None,
        dialogs,
        FileFilter::new("Workbook", ["loomtable"]).expect("filter"),
        FileFilter::new("CSV", ["csv"]).expect("filter"),
        FileFilter::new("CSV", ["csv"]).expect("filter"),
        FileFilter::new("Excel", ["xlsx"]).expect("filter"),
    ));
    state.sheets.borrow_mut().push(report);
    state
        .sheet_histories
        .borrow_mut()
        .push((Vec::new(), Vec::new()));
    state
}

#[test]
fn cross_sheet_formulas_evaluate_save_and_reopen() {
    let state = cross_sheet_state();
    *state.active_sheet_index.borrow_mut() = 1;
    *state.current.borrow_mut() = state.sheets.borrow()[1].clone();

    let vals = evaluate_current(&state);
    let a1 = CellRef::parse("A1").unwrap();
    let a2 = CellRef::parse("A2").unwrap();
    assert_eq!(vals.get(&a1).map(|v| v.display()).as_deref(), Some("25"));
    assert_eq!(vals.get(&a2).map(|v| v.display()).as_deref(), Some("10"));

    // Single-sheet evaluation of the same tab reports unresolvable refs.
    let solo = evaluate(&state.current.borrow());
    assert_eq!(solo.get(&a1).map(|v| v.display()).as_deref(), Some("#REF!"));

    // Save/reopen preserves the foreign formula text and re-resolves it.
    let dir = std::env::temp_dir().join(format!("loom-xsheet-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("x.loomtable");
    sync_current_to_tabs(&state);
    save_workbook(&path, &state.sheets.borrow(), 1).expect("save");
    let workbook = load_workbook(&path).expect("load");
    assert_eq!(workbook.sheets.len(), 2);
    assert_eq!(workbook.active, 1);
    assert_eq!(
        workbook.sheets[1].raw(a1),
        Some("=Data!B1+5"),
        "foreign formula text survives"
    );
    std::fs::remove_file(&path).ok();
}

#[test]
fn rename_rewrites_qualifiers_and_rejects_collisions() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    let state = cross_sheet_state();
    let menu_service = std::sync::Arc::new(NativeMenuBar::new());
    register_sheet_actions(&app, &state, &menu_service);

    // Rename the source tab: dependent formulas follow it.
    *state.active_sheet_index.borrow_mut() = 0;
    *state.current.borrow_mut() = state.sheets.borrow()[0].clone();
    app.invoke_rename_sheet("Figures".into());
    assert_eq!(state.current.borrow().name, "Figures");
    assert_eq!(
        state.sheets.borrow()[1].raw(CellRef::parse("A1").unwrap()),
        Some("=Figures!B1+5")
    );

    // Switch to the dependent tab and confirm values still resolve.
    *state.active_sheet_index.borrow_mut() = 1;
    *state.current.borrow_mut() = state.sheets.borrow()[1].clone();
    let vals = evaluate_current(&state);
    assert_eq!(
        vals.get(&CellRef::parse("A1").unwrap())
            .map(|v| v.display())
            .as_deref(),
        Some("25")
    );

    // Collision with an existing tab name is refused truthfully.
    app.invoke_rename_sheet("Figures".into());
    assert_eq!(state.current.borrow().name, "Report");
    assert!(app.get_status_left().as_str().contains("already exists"));

    // Undo restores the old tab name and the old qualifier text.
    let tx = state.undo_stack.borrow_mut().pop().unwrap();
    match &tx {
        SheetTransaction::Workbook { before, .. } => before.restore(&state),
        _ => panic!("expected workbook transaction"),
    }
    assert_eq!(state.sheets.borrow()[0].name, "Data");
    assert_eq!(
        state.sheets.borrow()[1].raw(CellRef::parse("A1").unwrap()),
        Some("=Data!B1+5")
    );
}

#[test]
fn repeated_workbook_renames_keep_document_snapshots_bounded() {
    let state = cross_sheet_state();
    for index in 0..100 {
        let mut after = state.sheets.borrow().clone();
        after[0].name = format!("Data {index}");
        commit_workbook_transaction(&state, after, 0, None);
    }

    let undo = state.undo_stack.borrow();
    assert_eq!(undo.len(), 100);
    assert!(history_bytes(&undo) <= MAX_HISTORY_BYTES);
    assert!(undo.iter().all(|transaction| matches!(
        transaction,
        SheetTransaction::Workbook { before, after }
            if before.sheets.len() == 2 && after.sheets.len() == 2
    )));
    // WorkbookUndoState intentionally has only document fields. If history
    // stacks were captured recursively, this source-level shape would be
    // impossible and the byte count would grow as 1, 4, 13, ... instead.
}

#[test]
fn dirty_state_clears_when_workbook_returns_to_last_saved_content() {
    let state = cross_sheet_state();
    state.mark_saved();
    assert!(!state.is_dirty());

    state.current.borrow_mut().set_str("A1", "unsaved");
    assert!(state.is_dirty());

    *state.current.borrow_mut() = state.sheets.borrow()[0].clone();
    assert!(!state.is_dirty());
}

#[test]
fn add_sheet_skips_taken_generated_names() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    let state = cross_sheet_state();
    // Rename tabs so "Sheet 3" is taken while only two tabs exist.
    state.sheets.borrow_mut()[0].name = "Sheet 1".to_string();
    state.sheets.borrow_mut()[1].name = "Sheet 3".to_string();
    let menu_service = std::sync::Arc::new(NativeMenuBar::new());
    register_sheet_actions(&app, &state, &menu_service);

    app.invoke_add_sheet();
    assert_eq!(state.sheets.borrow().len(), 3);
    assert_eq!(state.current.borrow().name, "Sheet 4");
}

#[test]
fn inspector_reflects_borders_fill_and_font() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    let mut sheet = Sheet::new("styled");
    sheet.set_str("A1", "hi");
    let a1 = CellRef::parse("A1").unwrap();
    let mut style = sheet.cell_style(a1);
    style.border = true;
    style.fill = loom_sheets_core::style::FillColor::Blue;
    style.font_size = Some(18);
    sheet.set_cell_style(a1, style);

    update_selection_range(&app, &sheet, &evaluate(&sheet), GridSelection::new(a1, a1));
    assert!(app.get_cell_border());
    assert_eq!(app.get_cell_fill(), 4);
    assert_eq!(app.get_cell_font_size(), 18);
    assert_eq!(app.get_cell_font_label().as_str(), "18");
}

#[test]
fn chart_insert_requires_range_and_can_replace_source() {
    set_platform();
    let app = SheetsApp::new().unwrap();
    let state = Rc::new(GuiState::new(
        starter_workbook(),
        None,
        Rc::new(loom_desktop::ScriptedFileDialogs::new([], [])),
        FileFilter::new("Workbook", ["loomtable"]).unwrap(),
        FileFilter::new("CSV", ["csv"]).unwrap(),
        FileFilter::new("CSV", ["csv"]).unwrap(),
        FileFilter::new("Excel", ["xlsx"]).unwrap(),
    ));
    let menu_service = std::sync::Arc::new(NativeMenuBar::new());
    register_sheet_actions(&app, &state, &menu_service);
    register_cell_edit_action(&app, &state, &menu_service);
    register_history_actions(&app, &state, &menu_service);

    app.invoke_insert_chart();
    assert!(state.current.borrow().chart.is_none());
    assert!(app.get_status_left().contains("Select two columns"));

    // A1:B4 is the header plus the three expense rows, excluding Total and Average.
    {
        let sheet = state.current.borrow();
        let values = evaluate(&sheet);
        update_selection_range(
            &app,
            &sheet,
            &values,
            GridSelection::new(CellRef::parse("A1").unwrap(), CellRef::parse("B4").unwrap()),
        );
    }
    app.invoke_insert_chart();
    let chart = state.current.borrow().chart.clone().unwrap();
    assert_eq!(chart.start_row, 1);
    assert_eq!(chart.end_row, Some(3));
    assert_eq!(app.get_chart_source_range().as_str(), "A1:B4");
    assert_eq!(app.get_chart_series_label().as_str(), "Amount");
    assert_eq!(app.get_chart_unit_label().as_str(), "USD/month");
    let categories = app.get_chart_categories();
    assert_eq!(categories.row_count(), 3);
    assert_eq!(categories.row_data(0).unwrap().as_str(), "Rent");
    assert_eq!(categories.row_data(1).unwrap().as_str(), "Food");
    assert_eq!(categories.row_data(2).unwrap().as_str(), "Transport");

    // Editing a selected source cell updates the visible chart; undo restores it.
    {
        let sheet = state.current.borrow();
        let values = evaluate(&sheet);
        update_selection(&app, &sheet, &values, CellRef::parse("B3").unwrap());
    }
    app.invoke_commit_selected_cell("500".into());
    assert_eq!(
        app.get_chart_values_display().row_data(1).unwrap().as_str(),
        "500"
    );
    app.invoke_undo();
    assert_eq!(
        state.current.borrow().raw(CellRef::parse("B3").unwrap()),
        Some("450")
    );
    assert_eq!(
        app.get_chart_values_display().row_data(1).unwrap().as_str(),
        "450"
    );

    // Native save/open keeps the exact chart bounds in the workbook model.
    let path = std::env::temp_dir().join(format!(
        "loom-sheets-chart-range-{}.loomtable",
        std::process::id()
    ));
    save_workbook(&path, &[state.current.borrow().clone()], 0).unwrap();
    let reopened = load_workbook(&path).unwrap();
    assert_eq!(reopened.sheets[0].chart, Some(chart));

    // A deliberate selection that includes Total must include that row.
    {
        let sheet = state.current.borrow();
        let values = evaluate(&sheet);
        update_selection_range(
            &app,
            &sheet,
            &values,
            GridSelection::new(CellRef::parse("A1").unwrap(), CellRef::parse("B5").unwrap()),
        );
    }
    app.invoke_insert_chart();
    let categories = app.get_chart_categories();
    assert_eq!(categories.row_count(), 4);
    assert_eq!(categories.row_data(3).unwrap().as_str(), "Total");
    assert_eq!(app.get_chart_source_range().as_str(), "A1:B5");
    std::fs::remove_file(path).ok();
}
