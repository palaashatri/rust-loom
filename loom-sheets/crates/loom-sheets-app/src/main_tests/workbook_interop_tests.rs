use super::*;

fn longest_white_run(image: &image::RgbaImage, x: u32) -> u32 {
    let mut longest = 0;
    let mut current = 0;
    for y in 0..image.height() {
        if image.get_pixel(x, y) == &image::Rgba([255, 255, 255, 255]) {
            current += 1;
            longest = longest.max(current);
        } else {
            current = 0;
        }
    }
    longest
}

fn has_button_border_near_bottom(image: &image::RgbaImage, surface_height: u32) -> bool {
    let bottom = image
        .height()
        .saturating_sub((image.height().saturating_sub(surface_height)) / 2);
    let border = image::Rgba([215, 215, 222, 255]);
    (bottom.saturating_sub(50)..bottom.saturating_sub(39)).any(|y| {
        (550..780)
            .filter(|&x| image.get_pixel(x, y) == &border)
            .count()
            >= 100
    })
}

fn write_xlsx_with_unsupported_area_chart(path: &std::path::Path) {
    let mut source = Sheet::new("Imported");
    source.set_str("A1", "Quarter");
    source.set_str("B1", "Revenue");
    source.chart = Some(loom_sheets_core::SheetChart {
        kind: loom_sheets_core::ChartKind::Line,
        ..Default::default()
    });
    let exported = loom_sheets_core::export_xlsx_sheets(&[source]).expect("export base XLSX");
    let original = PackageArchive::from_bytes(&exported).expect("read base XLSX");
    let mut rebuilt = PackageArchive::new();
    for part in original.paths() {
        let bytes = original.get(part).expect("XLSX part");
        let bytes = match part {
            "xl/charts/chart1.xml" => String::from_utf8(bytes.to_vec())
                .expect("chart XML")
                .replace("<c:lineChart>", "<c:areaChart>")
                .replace("</c:lineChart>", "</c:areaChart>")
                .into_bytes(),
            _ => bytes.to_vec(),
        };
        rebuilt.add(part, bytes).expect("copy XLSX part");
    }
    std::fs::write(path, rebuilt.to_bytes().expect("build XLSX fixture"))
        .expect("write XLSX fixture");
}

fn write_xlsx_with_absolute_anchor_shape(path: &std::path::Path) {
    let mut source = Sheet::new("Imported");
    source.objects.push(loom_sheets_core::SheetObject::shape(
        CellRef { row: 0, col: 0 },
        "Absolute shape",
    ));
    let exported = loom_sheets_core::export_xlsx_sheets(&[source]).expect("export base XLSX");
    let original = PackageArchive::from_bytes(&exported).expect("read base XLSX");
    let mut rebuilt = PackageArchive::new();
    for part in original.paths() {
        let bytes = original.get(part).expect("XLSX part");
        let bytes = if part == "xl/drawings/drawing1.xml" {
            String::from_utf8(bytes.to_vec())
                .expect("drawing XML")
                .replace(
                    "<xdr:from><xdr:col>0</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>0</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from>",
                    "<xdr:pos x=\"9525\" y=\"19050\"/>",
                )
                .replace("<xdr:oneCellAnchor>", "<xdr:absoluteAnchor>")
                .replace("</xdr:oneCellAnchor>", "</xdr:absoluteAnchor>")
                .into_bytes()
        } else {
            bytes.to_vec()
        };
        rebuilt.add(part, bytes).expect("copy XLSX part");
    }
    std::fs::write(path, rebuilt.to_bytes().expect("build XLSX fixture"))
        .expect("write XLSX fixture");
}

fn write_xlsx_with_multiple_series_line_chart(path: &std::path::Path) {
    let mut source = Sheet::new("Imported");
    source.set_str("A1", "Quarter");
    source.set_str("A2", "Q1");
    source.set_str("B1", "Revenue");
    source.set_str("B2", "10");
    source.set_str("B3", "20");
    source.set_str("C1", "Costs");
    source.set_str("C2", "5");
    source.set_str("C3", "8");
    source.chart = Some(loom_sheets_core::SheetChart {
        kind: loom_sheets_core::ChartKind::Line,
        end_row: Some(2),
        ..Default::default()
    });
    let exported = loom_sheets_core::export_xlsx_sheets(&[source]).expect("export base XLSX");
    let original = PackageArchive::from_bytes(&exported).expect("read base XLSX");
    let mut rebuilt = PackageArchive::new();
    for part in original.paths() {
        let bytes = original.get(part).expect("XLSX part");
        let bytes = if part == "xl/charts/chart1.xml" {
            let xml = String::from_utf8(bytes.to_vec())
                .expect("chart XML")
                .replacen(
                "<c:order val=\"0\"/>",
                "<c:order val=\"0\"/><c:tx><c:strRef><c:f>'Imported'!$B$1</c:f></c:strRef></c:tx>",
                1,
            );
            let series_start = xml.find("<c:ser>").expect("first series starts");
            let series_end = xml[series_start..]
                .find("</c:ser>")
                .map(|offset| series_start + offset + "</c:ser>".len())
                .expect("first series ends");
            let second_series = xml[series_start..series_end]
                .replace("<c:idx val=\"0\"/>", "<c:idx val=\"1\"/>")
                .replace("<c:order val=\"0\"/>", "<c:order val=\"1\"/>")
                .replace("'Imported'!$B$1", "'Imported'!$C$1")
                .replace("'Imported'!$B$2:$B$3", "'Imported'!$C$2:$C$3");
            xml.replacen(
                "</c:lineChart>",
                &format!("{second_series}</c:lineChart>"),
                1,
            )
            .into_bytes()
        } else {
            bytes.to_vec()
        };
        rebuilt.add(part, bytes).expect("copy XLSX part");
    }
    std::fs::write(path, rebuilt.to_bytes().expect("build XLSX fixture"))
        .expect("write XLSX fixture");
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
fn app_import_loader_returns_supported_workbook_and_loss_report_together() {
    let mut source = Sheet::new("Budget");
    source.set_str("A1", "Rent");
    source.set_str("B1", "1200");
    let exported = loom_sheets_core::export_xlsx_sheets(&[source]).expect("export base XLSX");
    let original = PackageArchive::from_bytes(&exported).expect("read base XLSX");
    let mut rebuilt = PackageArchive::new();
    for part in original.paths() {
        let bytes = original.get(part).expect("XLSX part");
        let bytes = if part == "xl/workbook.xml" {
            String::from_utf8(bytes.to_vec())
                .expect("workbook XML")
                .replace(
                    "</workbook>",
                    "<definedNames><definedName name=\"BudgetTotal\">Budget!$B$1</definedName></definedNames></workbook>",
                )
                .into_bytes()
        } else {
            bytes.to_vec()
        };
        rebuilt.add(part, bytes).expect("copy XLSX part");
    }
    let xlsx = rebuilt.to_bytes().expect("build XLSX fixture");
    let directory =
        std::env::temp_dir().join(format!("loom-sheets-import-report-{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("create temp directory");
    let path = directory.join("budget.xlsx");
    std::fs::write(&path, xlsx).expect("write XLSX fixture");

    let loaded = load_workbook_with_report(&path).expect("load XLSX with import report");
    assert_eq!(loaded.workbook.sheets[0].name, "Budget");
    assert_eq!(
        loaded.workbook.sheets[0].raw(CellRef { row: 0, col: 0 }),
        Some("Rent")
    );
    assert_eq!(
        loaded.warnings,
        vec![loom_sheets_core::XlsxImportWarning::DefinedNames]
    );

    std::fs::remove_dir_all(&directory).ok();
}

#[test]
fn cancel_xlsx_import_warning_preserves_current_workbook_and_recovery_state() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    let state = cross_sheet_state();
    let recovery_dir = attach_test_worker(&app, &state, "cancel-import-warning");
    let original_path = std::env::temp_dir().join("current-workbook.loomtable");
    *state.save_path.borrow_mut() = Some(original_path.clone());
    state
        .current
        .borrow_mut()
        .set_str("C1", "unsaved current value");
    state.mark_content_dirty();
    state
        .undo_stack
        .borrow_mut()
        .push(SheetTransaction::Range(RangeEdit::replace(
            &state.current.borrow(),
            CellRef { row: 0, col: 2 },
            Some("before edit".to_string()),
        )));
    apply_sheet(&app, &state);
    let revision = state.worker_revision.get();
    let result = state
        .workbook_worker
        .borrow()
        .as_ref()
        .expect("recovery worker")
        .wait_for_result(revision)
        .expect("persist current workbook before warning");
    assert!(apply_workbook_worker_result(&app, &state, result));
    let original_current = sheet_to_json(&state.current.borrow());
    let original_sheets = workbook_sheets(&state).0;
    let original_active = *state.active_sheet_index.borrow();
    let original_workbook = workbook_to_json(&original_sheets, original_active);
    let original_recovery = workbook_package_bytes(&original_sheets, original_active)
        .expect("package current recovery");
    let original_undo_len = state.undo_stack.borrow().len();
    let original_revision = state.worker_revision.get();
    let original_dirty = state.is_dirty();

    let incoming_dir = std::env::temp_dir().join(format!(
        "loom-sheets-unsupported-chart-import-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&incoming_dir).expect("create incoming workbook directory");
    let incoming_path = incoming_dir.join("incoming.xlsx");
    write_xlsx_with_unsupported_area_chart(&incoming_path);
    let loaded = load_workbook_with_report(&incoming_path).expect("load area chart workbook");
    assert!(loaded.workbook.sheets[0].chart.is_none());
    assert_eq!(
        loaded
            .warnings
            .iter()
            .map(|warning| warning.label())
            .collect::<Vec<_>>(),
        vec!["unsupported area charts"],
        "the single unsupported chart must be the only reason for the warning"
    );
    stage_xlsx_import(
        &app,
        &state,
        incoming_path,
        loaded.workbook,
        loaded.warnings,
    );
    assert!(app.get_xlsx_import_warning_open());
    assert!(app
        .get_xlsx_import_warning_message()
        .contains("unsupported area charts"));
    assert!(app
        .get_xlsx_import_warning_message()
        .contains("Continue replaces the workbook that is open now"));
    assert_eq!(sheet_to_json(&state.current.borrow()), original_current);

    cancel_pending_xlsx_import(&app, &state);

    assert!(!app.get_xlsx_import_warning_open());
    assert!(state.pending_xlsx_import.borrow().is_none());
    assert_eq!(sheet_to_json(&state.current.borrow()), original_current);
    let (sheets_after_cancel, active_after_cancel) = workbook_sheets(&state);
    assert_eq!(
        workbook_to_json(&sheets_after_cancel, active_after_cancel),
        original_workbook
    );
    assert_eq!(*state.active_sheet_index.borrow(), original_active);
    assert_eq!(*state.save_path.borrow(), Some(original_path));
    assert_eq!(state.undo_stack.borrow().len(), original_undo_len);
    assert_eq!(state.worker_revision.get(), original_revision);
    assert_eq!(state.is_dirty(), original_dirty);

    drop(state.workbook_worker.borrow_mut().take());
    assert_eq!(
        recovered_worker_payload(&recovery_dir),
        Some(original_recovery),
        "Cancel must leave the last durable workbook unchanged"
    );
    crate::cell_edit_recovery::remove_test_recovery_data(&recovery_dir);
    std::fs::remove_dir_all(&incoming_dir).ok();
}

#[test]
fn absolute_anchor_warning_cancel_preserves_workbook_and_recovery() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    let state = cross_sheet_state();
    let recovery_dir = attach_test_worker(&app, &state, "cancel-absolute-anchor-import-warning");
    state
        .current
        .borrow_mut()
        .set_str("C1", "unsaved current value");
    state.mark_content_dirty();
    apply_sheet(&app, &state);
    let revision = state.worker_revision.get();
    let result = state
        .workbook_worker
        .borrow()
        .as_ref()
        .expect("recovery worker")
        .wait_for_result(revision)
        .expect("persist current workbook before warning");
    assert!(apply_workbook_worker_result(&app, &state, result));
    let original_sheets = workbook_sheets(&state).0;
    let original_workbook = workbook_to_json(&original_sheets, *state.active_sheet_index.borrow());
    let original_recovery =
        workbook_package_bytes(&original_sheets, *state.active_sheet_index.borrow())
            .expect("package current recovery");

    let incoming_dir = std::env::temp_dir().join(format!(
        "loom-sheets-absolute-anchor-import-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&incoming_dir).expect("create incoming workbook directory");
    let incoming_path = incoming_dir.join("incoming.xlsx");
    write_xlsx_with_absolute_anchor_shape(&incoming_path);
    let loaded = load_workbook_with_report(&incoming_path).expect("load absolute-anchor workbook");
    assert!(loaded.workbook.sheets[0].objects.is_empty());
    assert_eq!(
        loaded
            .warnings
            .iter()
            .map(|warning| warning.label())
            .collect::<Vec<_>>(),
        vec!["objects positioned with absolute anchors"]
    );
    stage_xlsx_import(
        &app,
        &state,
        incoming_path,
        loaded.workbook,
        loaded.warnings,
    );
    assert!(app.get_xlsx_import_warning_open());
    assert!(app
        .get_xlsx_import_warning_message()
        .contains("objects positioned with absolute anchors"));

    cancel_pending_xlsx_import(&app, &state);

    assert!(!app.get_xlsx_import_warning_open());
    assert_eq!(
        workbook_to_json(
            &workbook_sheets(&state).0,
            *state.active_sheet_index.borrow()
        ),
        original_workbook
    );
    drop(state.workbook_worker.borrow_mut().take());
    assert_eq!(
        recovered_worker_payload(&recovery_dir),
        Some(original_recovery),
        "Cancel must leave the last durable workbook unchanged"
    );
    crate::cell_edit_recovery::remove_test_recovery_data(&recovery_dir);
    std::fs::remove_dir_all(&incoming_dir).ok();
}

#[test]
fn continue_xlsx_import_replaces_the_workbook_only_after_confirmation() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    let state = cross_sheet_state();
    state
        .current
        .borrow_mut()
        .set_str("C1", "unsaved current value");
    state.mark_content_dirty();

    let incoming_dir = std::env::temp_dir().join(format!(
        "loom-sheets-unsupported-chart-continue-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&incoming_dir).expect("create incoming workbook directory");
    let incoming_path = incoming_dir.join("incoming.xlsx");
    write_xlsx_with_unsupported_area_chart(&incoming_path);
    let loaded = load_workbook_with_report(&incoming_path).expect("load area chart workbook");
    assert!(loaded.workbook.sheets[0].chart.is_none());
    assert_eq!(
        loaded
            .warnings
            .iter()
            .map(|warning| warning.label())
            .collect::<Vec<_>>(),
        vec!["unsupported area charts"],
        "the single unsupported chart must be the only reason for the warning"
    );
    stage_xlsx_import(
        &app,
        &state,
        incoming_path,
        loaded.workbook,
        loaded.warnings,
    );

    assert_eq!(state.current.borrow().name, "Data");
    assert!(state.is_dirty());
    assert!(app.get_xlsx_import_warning_open());
    assert!(app
        .get_xlsx_import_warning_message()
        .contains("unsupported area charts"));
    let menu_service = std::sync::Arc::new(NativeMenuBar::new());
    continue_pending_xlsx_import(&app, &state, &menu_service);

    assert!(!app.get_xlsx_import_warning_open());
    assert!(state.pending_xlsx_import.borrow().is_none());
    assert_eq!(state.current.borrow().name, "Imported");
    assert_eq!(
        state.current.borrow().raw(CellRef { row: 0, col: 0 }),
        Some("Quarter")
    );
    assert_eq!(
        state.current.borrow().raw(CellRef { row: 0, col: 1 }),
        Some("Revenue")
    );
    assert!(state.current.borrow().chart.is_none());
    assert_eq!(state.sheets.borrow().len(), 1);
    assert_eq!(*state.active_sheet_index.borrow(), 0);
    assert!(state.undo_stack.borrow().is_empty());
    assert!(state.redo_stack.borrow().is_empty());
    assert!(state.save_path.borrow().is_none());
    assert!(!state.is_dirty());
    assert!(app
        .get_status_left()
        .contains("dropped: unsupported area charts"));
    std::fs::remove_dir_all(&incoming_dir).ok();
}

#[test]
fn multi_series_xlsx_warning_cancels_safely_and_continue_reports_dropped_data() {
    set_platform();
    let incoming_dir = std::env::temp_dir().join(format!(
        "loom-sheets-multi-series-warning-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&incoming_dir).expect("create incoming workbook directory");
    let incoming_path = incoming_dir.join("incoming.xlsx");
    write_xlsx_with_multiple_series_line_chart(&incoming_path);
    if let Some(path) = std::env::var_os("LOOM_SHEETS_TEST_XLSX_COPY") {
        std::fs::copy(&incoming_path, path).expect("copy multi-series XLSX fixture for inspection");
    }

    let cancel_app = SheetsApp::new().expect("create cancel SheetsApp");
    let cancel_state = cross_sheet_state();
    let recovery_dir = attach_test_worker(&cancel_app, &cancel_state, "cancel-multiseries-import");
    let original_path = std::env::temp_dir().join("current-multiseries.loomtable");
    *cancel_state.save_path.borrow_mut() = Some(original_path.clone());
    cancel_state
        .current
        .borrow_mut()
        .set_str("C1", "unsaved current value");
    cancel_state.mark_content_dirty();
    apply_sheet(&cancel_app, &cancel_state);
    let revision = cancel_state.worker_revision.get();
    let result = cancel_state
        .workbook_worker
        .borrow()
        .as_ref()
        .expect("recovery worker")
        .wait_for_result(revision)
        .expect("persist current workbook before warning");
    assert!(apply_workbook_worker_result(
        &cancel_app,
        &cancel_state,
        result
    ));
    let original_sheets = workbook_sheets(&cancel_state).0;
    let original_active = *cancel_state.active_sheet_index.borrow();
    let original_workbook = workbook_to_json(&original_sheets, original_active);
    let original_recovery = workbook_package_bytes(&original_sheets, original_active)
        .expect("package current recovery");
    let original_current = sheet_to_json(&cancel_state.current.borrow());
    let original_revision = cancel_state.worker_revision.get();

    let loaded = load_workbook_with_report(&incoming_path).expect("load multi-series workbook");
    assert_eq!(
        loaded
            .warnings
            .iter()
            .map(|warning| warning.label())
            .collect::<Vec<_>>(),
        vec!["additional line chart series"],
        "the additional series warning must be the only import loss"
    );
    let retained = loaded.workbook.sheets[0]
        .chart
        .as_ref()
        .expect("first line series remains available");
    assert_eq!(
        retained.cat_col, 0,
        "the first series keeps A-column categories"
    );
    assert_eq!(
        retained.val_col, 1,
        "the first series keeps B-column values"
    );
    stage_xlsx_import(
        &cancel_app,
        &cancel_state,
        incoming_path.clone(),
        loaded.workbook,
        loaded.warnings,
    );
    assert!(cancel_app.get_xlsx_import_warning_open());
    assert!(cancel_app
        .get_xlsx_import_warning_message()
        .contains("additional line chart series"));
    assert_eq!(
        sheet_to_json(&cancel_state.current.borrow()),
        original_current,
        "the active workbook must remain visible while the warning is open"
    );

    cancel_pending_xlsx_import(&cancel_app, &cancel_state);
    assert!(!cancel_app.get_xlsx_import_warning_open());
    assert!(cancel_state.pending_xlsx_import.borrow().is_none());
    let (sheets_after_cancel, active_after_cancel) = workbook_sheets(&cancel_state);
    assert_eq!(
        workbook_to_json(&sheets_after_cancel, active_after_cancel),
        original_workbook
    );
    assert_eq!(*cancel_state.save_path.borrow(), Some(original_path));
    assert_eq!(cancel_state.worker_revision.get(), original_revision);
    drop(cancel_state.workbook_worker.borrow_mut().take());
    assert_eq!(
        recovered_worker_payload(&recovery_dir),
        Some(original_recovery),
        "Cancel must leave the last durable workbook unchanged"
    );
    crate::cell_edit_recovery::remove_test_recovery_data(&recovery_dir);

    set_platform();
    let continue_app = SheetsApp::new().expect("create Continue SheetsApp");
    let continue_state = cross_sheet_state();
    let loaded = load_workbook_with_report(&incoming_path).expect("reload multi-series workbook");
    stage_xlsx_import(
        &continue_app,
        &continue_state,
        incoming_path.clone(),
        loaded.workbook,
        loaded.warnings,
    );
    assert!(continue_app.get_xlsx_import_warning_open());
    let menu_service = std::sync::Arc::new(NativeMenuBar::new());
    continue_pending_xlsx_import(&continue_app, &continue_state, &menu_service);
    assert!(!continue_app.get_xlsx_import_warning_open());
    assert_eq!(continue_state.current.borrow().name, "Imported");
    let retained = continue_state
        .current
        .borrow()
        .chart
        .clone()
        .expect("continue imports the retained chart series");
    assert_eq!(retained.kind, loom_sheets_core::ChartKind::Line);
    assert_eq!(retained.cat_col, 0);
    assert_eq!(retained.val_col, 1);
    assert!(continue_app
        .get_status_left()
        .contains("dropped: additional line chart series"));

    std::fs::remove_dir_all(&incoming_dir).ok();
}

#[test]
fn multi_series_picker_and_startup_open_warn_before_replacement() {
    set_platform();
    let incoming_dir = std::env::temp_dir().join(format!(
        "loom-sheets-multi-series-open-warning-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&incoming_dir).expect("create incoming workbook directory");
    let incoming_path = incoming_dir.join("incoming.xlsx");
    write_xlsx_with_multiple_series_line_chart(&incoming_path);

    let picker_app = SheetsApp::new().expect("create picker SheetsApp");
    let picker_state = cross_sheet_state_with_dialogs(Rc::new(
        loom_desktop::ScriptedFileDialogs::new([Some(incoming_path.clone())], []),
    ));
    picker_state.mark_saved();
    let picker_menu = std::sync::Arc::new(NativeMenuBar::new());
    crate::open_operations::open_workbook_from_picker(
        &picker_app,
        &picker_state,
        &picker_menu,
        None,
    );
    assert!(picker_app.get_status_left().starts_with("Opening "));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while !picker_app.get_xlsx_import_warning_open() {
        crate::open_operations::process_completions(&picker_app, &picker_state, &picker_menu);
        assert!(
            std::time::Instant::now() < deadline,
            "picker-selected candidate should stage the multi-series warning"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(picker_state.current.borrow().name, "Data");
    assert!(picker_app
        .get_xlsx_import_warning_message()
        .contains("additional line chart series"));
    continue_pending_xlsx_import(&picker_app, &picker_state, &picker_menu);
    assert_eq!(picker_state.current.borrow().name, "Imported");
    assert_eq!(
        picker_state
            .current
            .borrow()
            .chart
            .as_ref()
            .unwrap()
            .cat_col,
        0,
        "picker import must retain the first series category column"
    );
    assert_eq!(
        picker_state
            .current
            .borrow()
            .chart
            .as_ref()
            .unwrap()
            .val_col,
        1,
        "picker import must retain the first series value column"
    );
    assert!(picker_app
        .get_status_left()
        .contains("dropped: additional line chart series"));

    set_platform();
    let startup_app = SheetsApp::new().expect("create startup SheetsApp");
    let startup_state = cross_sheet_state();
    startup_state.mark_saved();
    crate::open_operations::start_startup_open(
        &startup_app,
        &startup_state,
        incoming_path.clone(),
        crate::open_operations::StartupOpenOptions::new(false, false, false),
    );
    let startup_menu = std::sync::Arc::new(NativeMenuBar::new());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while !startup_app.get_xlsx_import_warning_open() {
        crate::open_operations::process_completions(&startup_app, &startup_state, &startup_menu);
        assert!(
            std::time::Instant::now() < deadline,
            "startup candidate should stage the multi-series warning"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(startup_state.current.borrow().name, "Data");
    assert!(startup_app
        .get_xlsx_import_warning_message()
        .contains("additional line chart series"));
    continue_pending_xlsx_import(&startup_app, &startup_state, &startup_menu);
    assert_eq!(startup_state.current.borrow().name, "Imported");
    assert_eq!(
        startup_state
            .current
            .borrow()
            .chart
            .as_ref()
            .unwrap()
            .cat_col,
        0,
        "startup import must retain the first series category column"
    );
    assert_eq!(
        startup_state
            .current
            .borrow()
            .chart
            .as_ref()
            .unwrap()
            .val_col,
        1,
        "startup import must retain the first series value column"
    );
    assert!(startup_app
        .get_status_left()
        .contains("dropped: additional line chart series"));
    assert!(!startup_app.get_xlsx_import_warning_open());

    std::fs::remove_dir_all(&incoming_dir).ok();
}

#[test]
fn startup_xlsx_warning_keeps_recovered_workbook_visible_until_confirmation() {
    let mut recovered = Sheet::new("Recovered");
    recovered.set_str("A1", "keep this workbook");
    let recovered_payload =
        workbook_package_bytes(&[recovered], 0).expect("build recovery package");

    let incoming_dir = std::env::temp_dir().join(format!(
        "loom-sheets-unsupported-chart-startup-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&incoming_dir).expect("create incoming workbook directory");
    let incoming_path = incoming_dir.join("incoming.xlsx");
    write_xlsx_with_unsupported_area_chart(&incoming_path);
    let loaded = load_workbook_with_report(&incoming_path).expect("load area chart workbook");
    assert_eq!(
        loaded
            .warnings
            .iter()
            .map(|warning| warning.label())
            .collect::<Vec<_>>(),
        vec!["unsupported area charts"],
        "the single unsupported chart must be the only reason for the warning"
    );

    let fallback = restore_workbook_from_snapshot(&recovered_payload)
        .expect("restore previous workbook for startup fallback");
    let (current, pending) = prepare_startup_import(incoming_path.clone(), loaded, fallback);

    assert_eq!(current.sheets[0].name, "Recovered");
    assert_eq!(
        current.sheets[0].raw(CellRef { row: 0, col: 0 }),
        Some("keep this workbook")
    );
    let pending = pending.expect("candidate should wait for confirmation");
    assert_eq!(pending.path, incoming_path);
    assert_eq!(pending.workbook.sheets[0].name, "Imported");
    assert_eq!(
        pending.workbook.sheets[0].raw(CellRef { row: 0, col: 0 }),
        Some("Quarter")
    );
    assert_eq!(
        pending.warnings,
        vec![loom_sheets_core::XlsxImportWarning::UnsupportedChart(
            loom_sheets_core::XlsxChartType::Area
        )]
    );
    std::fs::remove_dir_all(&incoming_dir).ok();
}

#[test]
fn xlsx_import_warning_renders_and_escape_uses_cancel_callback() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    app.set_xlsx_import_warning_message(
        "Loom will drop conditional formatting and named ranges.".into(),
    );
    let closed = snapshot_component(&app, 1024.0, 720.0, 1.0).expect("render without warning");

    let cancelled = Rc::new(Cell::new(false));
    let cancelled_ref = cancelled.clone();
    app.on_xlsx_import_cancel(move || cancelled_ref.set(true));
    app.set_local_menu_visible(true);
    app.set_local_menu_open_index(0);
    app.set_xlsx_import_warning_open(true);
    app.invoke_focus_xlsx_import_warning();
    let open = snapshot_component(&app, 1024.0, 720.0, 1.0).expect("render import warning");
    assert_eq!(
        app.get_local_menu_open_index(),
        -1,
        "rendering the warning must close any menu behind the modal"
    );
    assert_ne!(
        open.as_raw(),
        closed.as_raw(),
        "warning layer must be visible"
    );

    app.window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed {
            text: slint::platform::Key::Tab.into(),
        });
    app.window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed {
            text: slint::platform::Key::Escape.into(),
        });
    assert!(
        cancelled.get(),
        "Escape must invoke cancel instead of import"
    );
}

#[test]
fn xlsx_import_warning_fits_short_copy_and_keeps_actions_visible_for_long_copy() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    app.set_xlsx_import_warning_open(true);
    app.set_xlsx_import_warning_message("Loom will drop one unsupported feature.".into());

    let short = snapshot_component(&app, 1024.0, 720.0, 1.0).expect("render short warning");
    loom_test_support::png::save_png(
        &std::env::temp_dir().join("loom-sheets-ui28-short-warning.png"),
        &short,
    )
    .expect("save short warning capture");
    let short_height = longest_white_run(&short, 238);
    assert!(
        short_height < 320,
        "short warning should fit its content instead of reserving a tall blank area; got {short_height}px"
    );

    app.set_template_text_scale(2.0);
    let large_text =
        snapshot_component(&app, 1024.0, 720.0, 1.0).expect("render short warning at 2x text size");
    loom_test_support::png::save_png(
        &std::env::temp_dir().join("loom-sheets-ui28-short-warning-2x.png"),
        &large_text,
    )
    .expect("save 2x short warning capture");
    let large_text_height = longest_white_run(&large_text, 238);
    assert!(
        large_text_height > short_height,
        "2x text should make the dialog grow to fit its larger copy"
    );

    let long_message = (1..=30)
        .map(|number| format!("Unsupported feature {number}: this data will not be imported."))
        .collect::<Vec<_>>()
        .join("\n");
    app.set_xlsx_import_warning_message(long_message.into());
    let long_text =
        snapshot_component(&app, 1024.0, 720.0, 1.0).expect("render long warning at 2x text size");
    let long_height = longest_white_run(&long_text, 238);
    eprintln!("long warning dialog surface height: {long_height}px");
    let diagnostic = std::env::temp_dir().join("loom-sheets-ui28-long-warning.png");
    loom_test_support::png::save_png(&diagnostic, &long_text)
        .expect("save long warning diagnostic capture");
    assert!(
        long_height <= 688,
        "long warning must stay inside the window; got {long_height}px"
    );
    assert!(
        has_button_border_near_bottom(&long_text, long_height),
        "the import action buttons must remain visible below a long warning"
    );
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
