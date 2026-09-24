use super::*;

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
fn xlsx_import_warning_blocks_shared_command_dispatch() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    let calls = Rc::new(std::cell::Cell::new(0));
    let calls_ref = calls.clone();
    app.on_new_sheet(move || calls_ref.set(calls_ref.get() + 1));
    app.set_xlsx_import_warning_open(true);

    assert!(!dispatch_command(&app, "file.new"));
    assert_eq!(
        calls.get(),
        0,
        "native and palette commands must not bypass the import decision"
    );
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
