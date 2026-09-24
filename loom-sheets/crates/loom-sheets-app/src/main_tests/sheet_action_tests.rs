use super::*;

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
