use super::*;

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

    let mut candidate = Sheet::new("Imported");
    candidate.set_str("A1", "replacement value");
    stage_xlsx_import(
        &app,
        &state,
        PathBuf::from("incoming.xlsx"),
        loom_sheets_core::persistence::WorkbookFile {
            sheets: vec![candidate],
            active: 0,
        },
        vec![loom_sheets_core::XlsxImportWarning::DefinedNames],
    );
    assert!(app.get_xlsx_import_warning_open());
    assert!(app
        .get_xlsx_import_warning_message()
        .contains("defined names and named ranges"));
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
    let mut recovery = loom_production::snapshot::SnapshotRecovery::open_at(&recovery_dir)
        .expect("open existing recovery after cancel");
    assert_eq!(
        recovery.take_restored_payload(),
        Some(original_recovery),
        "Cancel must leave the last durable workbook unchanged"
    );
    std::fs::remove_dir_all(recovery_dir).ok();
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

    let mut candidate = Sheet::new("Imported");
    candidate.set_str("A1", "replacement value");
    stage_xlsx_import(
        &app,
        &state,
        PathBuf::from("incoming.xlsx"),
        loom_sheets_core::persistence::WorkbookFile {
            sheets: vec![candidate],
            active: 0,
        },
        vec![loom_sheets_core::XlsxImportWarning::DefinedNames],
    );

    assert_eq!(state.current.borrow().name, "Data");
    assert!(state.is_dirty());
    let menu_service = std::sync::Arc::new(NativeMenuBar::new());
    continue_pending_xlsx_import(&app, &state, &menu_service);

    assert!(!app.get_xlsx_import_warning_open());
    assert!(state.pending_xlsx_import.borrow().is_none());
    assert_eq!(state.current.borrow().name, "Imported");
    assert_eq!(
        state.current.borrow().raw(CellRef { row: 0, col: 0 }),
        Some("replacement value")
    );
    assert_eq!(state.sheets.borrow().len(), 1);
    assert_eq!(*state.active_sheet_index.borrow(), 0);
    assert!(state.undo_stack.borrow().is_empty());
    assert!(state.redo_stack.borrow().is_empty());
    assert!(state.save_path.borrow().is_none());
    assert!(!state.is_dirty());
    assert!(app
        .get_status_left()
        .contains("dropped: defined names and named ranges"));
}

#[test]
fn startup_xlsx_warning_keeps_recovered_workbook_visible_until_confirmation() {
    let mut recovered = Sheet::new("Recovered");
    recovered.set_str("A1", "keep this workbook");
    let recovered_payload =
        workbook_package_bytes(&[recovered], 0).expect("build recovery package");

    let mut candidate = Sheet::new("Imported");
    candidate.set_str("A1", "incoming value");
    let loaded = LoadedWorkbook {
        workbook: loom_sheets_core::persistence::WorkbookFile {
            sheets: vec![candidate],
            active: 0,
        },
        warnings: vec![loom_sheets_core::XlsxImportWarning::DefinedNames],
    };

    let fallback = restore_workbook_from_snapshot(&recovered_payload)
        .expect("restore previous workbook for startup fallback");
    let (current, pending) =
        prepare_startup_import(PathBuf::from("incoming.xlsx"), loaded, fallback);

    assert_eq!(current.sheets[0].name, "Recovered");
    assert_eq!(
        current.sheets[0].raw(CellRef { row: 0, col: 0 }),
        Some("keep this workbook")
    );
    let pending = pending.expect("candidate should wait for confirmation");
    assert_eq!(pending.path, PathBuf::from("incoming.xlsx"));
    assert_eq!(pending.workbook.sheets[0].name, "Imported");
    assert_eq!(
        pending.workbook.sheets[0].raw(CellRef { row: 0, col: 0 }),
        Some("incoming value")
    );
    assert_eq!(
        pending.warnings,
        vec![loom_sheets_core::XlsxImportWarning::DefinedNames]
    );
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
