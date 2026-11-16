use std::rc::Rc;
use std::sync::Arc;

use loom_desktop::{FileFilter, MenuBarService, NativeMenuBar, ScriptedFileDialogs};
use loom_sheets_core::{CellRef, ChartKind, Sheet, SheetChart, SheetObject};
use slint::ComponentHandle;

use crate::{
    apply_sheet, chart_actions, command_dispatch, local_menu, object_actions, palette,
    register_history_actions, register_sheet_actions, sync_chart_to_app, GuiState,
    SheetChartActions, SheetChartConfig, SheetsApp,
};

fn test_state(sheet: Sheet) -> Rc<GuiState> {
    let dialogs = Rc::new(ScriptedFileDialogs::new([], []));
    Rc::new(GuiState::new(
        sheet,
        None,
        dialogs,
        FileFilter::new("Workbook", ["loomtable"]).expect("filter"),
        FileFilter::new("CSV", ["csv"]).expect("filter"),
        FileFilter::new("CSV", ["csv"]).expect("filter"),
        FileFilter::new("Excel", ["xlsx"]).expect("filter"),
    ))
}

#[test]
fn arrange_menu_and_palette_expose_z_order_and_pdf_export() {
    let bar = local_menu::sheets_menu_bar();
    assert!(bar.find_item("file.export_pdf").is_some());
    assert!(bar.menus.iter().any(|m| m.title == "Arrange"));
    for id in [
        "arrange.bring_forward",
        "arrange.send_backward",
        "arrange.bring_to_front",
        "arrange.send_to_back",
    ] {
        assert!(bar.find_item(id).is_some(), "missing menu item {id}");
        assert!(local_menu::SUPPORTED_COMMANDS.contains(&id));
    }

    i_slint_backend_testing::init_no_event_loop();
    let app = SheetsApp::new().expect("create SheetsApp");
    let commands = palette::master_palette(&app);
    for id in [
        "file.export_pdf",
        "arrange.bring_forward",
        "arrange.send_backward",
        "arrange.bring_to_front",
        "arrange.send_to_back",
    ] {
        assert!(
            commands.iter().any(|cmd| cmd.id == id),
            "missing palette command {id}"
        );
    }
}

#[test]
fn z_order_commands_reorder_objects_with_undo_redo_and_boundary_messages() {
    i_slint_backend_testing::init_no_event_loop();
    let mut sheet = Sheet::new("Arrange");
    sheet
        .objects
        .push(SheetObject::shape(CellRef { row: 0, col: 0 }, "First"));
    sheet
        .objects
        .push(SheetObject::shape(CellRef { row: 1, col: 0 }, "Second"));
    sheet
        .objects
        .push(SheetObject::shape(CellRef { row: 2, col: 0 }, "Third"));

    let app = SheetsApp::new().expect("create SheetsApp");
    let state = test_state(sheet);
    let menu_service = Arc::new(NativeMenuBar::new());
    menu_service
        .install_menu_bar(&local_menu::sheets_menu_bar())
        .expect("install menu");
    register_sheet_actions(&app, &state, &menu_service);
    register_history_actions(&app, &state, &menu_service);
    object_actions::register_object_actions(&app, &state, &menu_service);
    apply_sheet(&app, &state);

    app.set_selected_object(0);
    assert!(command_dispatch::dispatch_command(
        &app,
        "arrange.bring_forward"
    ));
    assert_eq!(app.get_selected_object(), 1);
    assert_eq!(state.current.borrow().objects[1].label, "First");

    assert!(command_dispatch::dispatch_command(
        &app,
        "arrange.bring_to_front"
    ));
    assert_eq!(app.get_selected_object(), 2);
    assert_eq!(state.current.borrow().objects[2].label, "First");

    assert!(command_dispatch::dispatch_command(
        &app,
        "arrange.bring_forward"
    ));
    assert!(app
        .get_status_left()
        .as_str()
        .contains("already at the front"));

    assert!(command_dispatch::dispatch_command(
        &app,
        "arrange.send_to_back"
    ));
    assert_eq!(app.get_selected_object(), 0);
    assert_eq!(state.current.borrow().objects[0].label, "First");

    // Undo restores previous order
    app.invoke_undo();
    assert_eq!(state.current.borrow().objects[2].label, "First");
    app.invoke_redo();
    assert_eq!(state.current.borrow().objects[0].label, "First");
}

#[test]
fn chart_inspector_customizes_title_kind_legend_axes_and_series_with_undo() {
    i_slint_backend_testing::init_no_event_loop();
    let mut sheet = Sheet::new("Sales");
    sheet.set_str("A1", "Month");
    sheet.set_str("B1", "Revenue");
    sheet.set_str("A2", "1");
    sheet.set_str("B2", "100");
    sheet.set_str("A3", "2");
    sheet.set_str("B3", "150");
    sheet.chart = Some(SheetChart {
        kind: ChartKind::Bar,
        title: "Sales Chart".to_string(),
        cat_col: 0,
        val_col: 1,
        start_row: 1,
        end_row: Some(2),
        ..Default::default()
    });

    let app = SheetsApp::new().expect("create SheetsApp");
    let state = test_state(sheet);
    let menu_service = Arc::new(NativeMenuBar::new());
    menu_service
        .install_menu_bar(&local_menu::sheets_menu_bar())
        .expect("install menu");
    register_sheet_actions(&app, &state, &menu_service);
    register_history_actions(&app, &state, &menu_service);
    chart_actions::register_chart_actions(&app, &state);
    chart_actions::register_chart_inspector_actions(&app, &state, &menu_service);
    app.set_chart_visible(true);
    apply_sheet(&app, &state);
    sync_chart_to_app(&app, &state.current.borrow());

    let actions = app.global::<SheetChartActions>();
    let cfg = app.global::<SheetChartConfig>();
    assert!(cfg.get_chart_exists());
    assert!(cfg.get_show_legend());

    assert!(actions.invoke_set_chart_title("Annual Revenue".into()));
    assert_eq!(cfg.get_title().as_str(), "Annual Revenue");
    assert_eq!(app.get_chart_title().as_str(), "Annual Revenue");
    assert!(!actions.invoke_set_chart_title("   ".into()));

    actions.invoke_set_chart_kind(3);
    assert_eq!(cfg.get_kind().as_str(), "scatter");

    actions.invoke_toggle_chart_legend();
    assert!(!cfg.get_show_legend());

    assert!(actions.invoke_set_x_axis_label("Period".into()));
    assert!(actions.invoke_set_y_axis_label("USD".into()));
    assert!(actions.invoke_set_series_name("Net Sales".into()));
    assert_eq!(cfg.get_x_axis_label().as_str(), "Period");
    assert_eq!(cfg.get_y_axis_label().as_str(), "USD");
    assert_eq!(cfg.get_series_name().as_str(), "Net Sales");
    assert_eq!(app.get_chart_series_label().as_str(), "Net Sales");

    app.invoke_undo();
    assert_eq!(
        state.current.borrow().chart.as_ref().unwrap().series_name,
        ""
    );
}
