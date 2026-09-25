use super::*;

fn start_open_with_formula_draft(
    app: &SheetsApp,
    state: &Rc<GuiState>,
    path: PathBuf,
    menu_service: &Arc<NativeMenuBar>,
) -> crate::open_operations::OpenOperation {
    app.set_selected_cell("A1".into());
    project_current(app, state);
    assert!(!crate::open_operations::has_formula_draft(app));
    let revision = state.worker_revision.get();
    assert!(!state.is_dirty());

    let operation = state
        .open_operations
        .borrow_mut()
        .start_picker_load(path, revision)
        .expect("queue background Open");
    app.set_formula_edit_buffer("=1+2".into());
    assert_eq!(state.worker_revision.get(), revision);
    assert!(!state.is_dirty());

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while !app.get_save_changes_open() {
        crate::open_operations::process_completions(app, state, menu_service);
        assert!(
            std::time::Instant::now() < deadline,
            "background Open with a formula draft should request a replacement decision"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(state.open_operations.borrow().is_current(operation));
    assert_eq!(
        state.pending_replacement.get(),
        Some(crate::open_operations::PendingReplacement::OpenCandidate)
    );
    operation
}

#[test]
fn xlsx_open_candidate_survives_warning_continue_and_cancel_invalidates_it() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    crate::template_navigation::wire_template_navigation(&app);
    let state = cross_sheet_state();
    state.mark_saved();
    let operation = state
        .open_operations
        .borrow_mut()
        .begin_operation(state.worker_revision.get());
    let mut candidate = Sheet::new("Imported");
    candidate.set_str("A1", "candidate data");
    crate::xlsx_import::stage_xlsx_import_candidate(
        &app,
        &state,
        PathBuf::from("incoming.xlsx"),
        loom_sheets_core::persistence::WorkbookFile {
            sheets: vec![candidate],
            active: 0,
        },
        vec![loom_sheets_core::XlsxImportWarning::DefinedNames],
        Some(operation),
        Some(crate::open_operations::StartupOpenOptions::new(
            false, false, true,
        )),
    );
    assert_eq!(
        state
            .pending_xlsx_import
            .borrow()
            .as_ref()
            .unwrap()
            .operation,
        Some(operation)
    );

    let menu_service = std::sync::Arc::new(NativeMenuBar::new());
    continue_pending_xlsx_import(&app, &state, &menu_service);

    assert_eq!(state.current.borrow().name, "Imported");
    assert!(app.get_template_chooser_open());
    assert!(!app.get_xlsx_import_warning_open());
    assert!(!state.open_operations.borrow().is_current(operation));
    app.window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed {
            text: slint::platform::Key::RightArrow.into(),
        });
    assert_eq!(
        app.get_template_selected(),
        3,
        "accepted startup options must leave keyboard focus in the template chooser"
    );

    set_platform();
    let cancel_app = SheetsApp::new().expect("create cancel SheetsApp");
    let cancel_state = cross_sheet_state();
    cancel_state.mark_saved();
    let cancel_operation = cancel_state
        .open_operations
        .borrow_mut()
        .begin_operation(cancel_state.worker_revision.get());
    let mut rejected = Sheet::new("Rejected");
    rejected.set_str("A1", "must not install");
    crate::xlsx_import::stage_xlsx_import_candidate(
        &cancel_app,
        &cancel_state,
        PathBuf::from("rejected.xlsx"),
        loom_sheets_core::persistence::WorkbookFile {
            sheets: vec![rejected],
            active: 0,
        },
        vec![loom_sheets_core::XlsxImportWarning::DefinedNames],
        Some(cancel_operation),
        None,
    );
    cancel_state
        .current
        .borrow_mut()
        .set_str("C1", "new unsaved edit");
    cancel_state.mark_content_dirty();
    cancel_state.next_worker_revision();
    let cancel_menu = std::sync::Arc::new(NativeMenuBar::new());
    continue_pending_xlsx_import(&cancel_app, &cancel_state, &cancel_menu);

    assert!(cancel_app.get_save_changes_open());
    assert!(!cancel_app.get_xlsx_import_warning_open());
    assert!(cancel_state.pending_xlsx_import.borrow().is_some());
    crate::open_operations::cancel_pending_replacement(&cancel_app, &cancel_state);
    cancel_app.set_save_changes_open(false);

    assert_eq!(cancel_state.current.borrow().name, "Data");
    assert_eq!(
        cancel_state
            .current
            .borrow()
            .raw(CellRef { row: 0, col: 2 }),
        Some("new unsaved edit")
    );
    assert!(cancel_state.pending_xlsx_import.borrow().is_none());
    assert!(!cancel_app.get_xlsx_import_warning_open());
    assert!(cancel_app.get_xlsx_import_warning_message().is_empty());
    assert!(!cancel_state
        .open_operations
        .borrow()
        .is_current(cancel_operation));
}

#[test]
fn async_xlsx_warning_focus_routes_escape_to_cancel() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    let state = cross_sheet_state();
    let cancelled = std::rc::Rc::new(std::cell::Cell::new(false));
    let cancelled_ref = cancelled.clone();
    app.on_xlsx_import_cancel(move || cancelled_ref.set(true));

    crate::xlsx_import::stage_xlsx_import_candidate(
        &app,
        &state,
        PathBuf::from("incoming.xlsx"),
        loom_sheets_core::persistence::WorkbookFile {
            sheets: vec![Sheet::new("Imported")],
            active: 0,
        },
        vec![loom_sheets_core::XlsxImportWarning::DefinedNames],
        None,
        None,
    );
    app.window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed {
            text: slint::platform::Key::Escape.into(),
        });

    assert!(
        cancelled.get(),
        "opening an asynchronous XLSX warning must move keyboard focus into it"
    );
}

#[test]
fn background_open_holds_candidate_until_dirty_document_choice() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    let state = cross_sheet_state();
    state.mark_saved();
    let dir =
        std::env::temp_dir().join(format!("loom-sheets-open-candidate-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create test directory");
    let path = dir.join("candidate.loomtable");
    let mut candidate = Sheet::new("Candidate");
    candidate.set_str("A1", "loaded once");
    save_workbook(&path, &[candidate], 0).expect("save candidate workbook");

    let operation = state
        .open_operations
        .borrow_mut()
        .start_picker_load(path.clone(), state.worker_revision.get())
        .expect("queue background open");
    state
        .current
        .borrow_mut()
        .set_str("C1", "unsaved current work");
    state.mark_content_dirty();
    state.next_worker_revision();

    let menu_service = std::sync::Arc::new(NativeMenuBar::new());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while !app.get_save_changes_open() {
        crate::open_operations::process_completions(&app, &state, &menu_service);
        assert!(
            std::time::Instant::now() < deadline,
            "background candidate should reach the Save Changes decision"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(state.current.borrow().name, "Data");
    assert!(state.open_operations.borrow().is_current(operation));
    assert_eq!(
        state.pending_replacement.get(),
        Some(crate::open_operations::PendingReplacement::OpenCandidate)
    );

    crate::open_operations::discard_changes_and_resume(&app, &state, &menu_service);

    assert_eq!(state.current.borrow().name, "Candidate");
    assert_eq!(
        state.current.borrow().raw(CellRef { row: 0, col: 0 }),
        Some("loaded once")
    );
    assert!(!state.open_operations.borrow().is_current(operation));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn formula_draft_is_gated_and_resolved_before_background_open_replaces_workbook() {
    set_platform();
    let dir = std::env::temp_dir().join(format!(
        "loom-sheets-formula-draft-open-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("create test directory");
    let candidate_path = dir.join("candidate.loomtable");
    let mut candidate = Sheet::new("Draft candidate");
    candidate.set_str("A1", "candidate value");
    save_workbook(&candidate_path, &[candidate], 0).expect("save candidate workbook");

    let cancel_app = SheetsApp::new().expect("create cancel SheetsApp");
    let cancel_state = cross_sheet_state();
    cancel_state.mark_saved();
    let cancel_menu = std::sync::Arc::new(NativeMenuBar::new());
    let cancel_operation = start_open_with_formula_draft(
        &cancel_app,
        &cancel_state,
        candidate_path.clone(),
        &cancel_menu,
    );
    crate::open_operations::cancel_pending_replacement(&cancel_app, &cancel_state);
    cancel_app.set_save_changes_open(false);
    assert_eq!(cancel_state.current.borrow().name, "Data");
    assert_eq!(cancel_app.get_formula_edit_buffer(), "=1+2");
    assert!(!cancel_state
        .open_operations
        .borrow()
        .is_current(cancel_operation));

    let save_app = SheetsApp::new().expect("create save SheetsApp");
    let save_state = cross_sheet_state();
    save_state.mark_saved();
    let saved_path = dir.join("saved-before-replacement.loomtable");
    *save_state.save_path.borrow_mut() = Some(saved_path.clone());
    let recovery_path = attach_test_worker(&save_app, &save_state, "formula-draft-save");
    let save_menu = std::sync::Arc::new(NativeMenuBar::new());
    let save_operation =
        start_open_with_formula_draft(&save_app, &save_state, candidate_path.clone(), &save_menu);
    register_cell_edit_action(&save_app, &save_state, &save_menu);
    assert!(
        crate::open_operations::save_changes_and_resume(&save_app, &save_state, &save_menu)
            .expect("save current workbook before replacement")
    );
    wait_for_save_test_completion(&save_app, &save_state, &save_menu);
    let saved = load_workbook(&saved_path).expect("reopen saved original workbook");
    assert_eq!(
        saved.sheets[0].raw(CellRef { row: 0, col: 0 }),
        Some("=1+2")
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while save_state.current.borrow().name != "Draft candidate" {
        crate::open_operations::process_completions(&save_app, &save_state, &save_menu);
        assert!(
            std::time::Instant::now() < deadline,
            "candidate did not reload after the Save completed"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(save_state.current.borrow().name, "Draft candidate");
    assert!(!save_state
        .open_operations
        .borrow()
        .is_current(save_operation));
    drop(save_state.workbook_worker.borrow_mut().take());
    std::fs::remove_dir_all(&recovery_path).ok();

    let discard_app = SheetsApp::new().expect("create discard SheetsApp");
    let discard_state = cross_sheet_state();
    discard_state.mark_saved();
    let discard_menu = std::sync::Arc::new(NativeMenuBar::new());
    let discard_operation =
        start_open_with_formula_draft(&discard_app, &discard_state, candidate_path, &discard_menu);
    crate::open_operations::discard_changes_and_resume(&discard_app, &discard_state, &discard_menu);
    assert_eq!(discard_state.current.borrow().name, "Draft candidate");
    assert_eq!(
        discard_app.get_formula_edit_buffer(),
        discard_app.get_selection_formula()
    );
    assert!(!discard_state
        .open_operations
        .borrow()
        .is_current(discard_operation));

    std::fs::remove_dir_all(&dir).ok();
}

fn continue_xlsx_warning_with_formula_draft(
    app: &SheetsApp,
    state: &GuiState,
    menu_service: &Arc<NativeMenuBar>,
    candidate_name: &str,
    directory: &std::path::Path,
) -> crate::open_operations::OpenOperation {
    app.set_selected_cell("A1".into());
    project_current(app, state);
    let operation = state
        .open_operations
        .borrow_mut()
        .begin_operation(state.worker_revision.get());
    let candidate_path = directory.join(format!("{candidate_name}.xlsx"));
    write_xlsx_with_defined_name(&candidate_path, candidate_name);
    crate::xlsx_import::stage_xlsx_import_candidate(
        app,
        state,
        candidate_path,
        loom_sheets_core::persistence::WorkbookFile {
            sheets: vec![Sheet::new(candidate_name)],
            active: 0,
        },
        vec![loom_sheets_core::XlsxImportWarning::DefinedNames],
        Some(operation),
        None,
    );
    app.set_formula_edit_buffer("=1+2".into());
    continue_pending_xlsx_import(app, state, menu_service);

    assert!(app.get_save_changes_open());
    assert_eq!(
        state.pending_replacement.get(),
        Some(crate::open_operations::PendingReplacement::OpenCandidate)
    );
    assert!(state.pending_xlsx_import.borrow().is_some());
    assert_eq!(state.current.borrow().name, "Data");
    assert_eq!(app.get_formula_edit_buffer(), "=1+2");
    operation
}

fn write_xlsx_with_defined_name(path: &std::path::Path, sheet_name: &str) {
    let mut source = Sheet::new(sheet_name);
    source.set_str("A1", "candidate value");
    let exported = loom_sheets_core::export_xlsx_sheets(&[source]).expect("export base XLSX");
    let original = loom_package::PackageArchive::from_bytes(&exported).expect("read base XLSX");
    let mut rebuilt = loom_package::PackageArchive::new();
    for part in original.paths() {
        let bytes = original.get(part).expect("XLSX part");
        let bytes = if part == "xl/workbook.xml" {
            String::from_utf8(bytes.to_vec())
                .expect("workbook XML")
                .replace(
                    "</workbook>",
                    "<definedNames><definedName name=\"CandidateValue\">1</definedName></definedNames></workbook>",
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
fn xlsx_warning_continue_with_formula_draft_waits_for_save_discard_or_cancel() {
    set_platform();
    let dir = std::env::temp_dir().join(format!(
        "loom-sheets-xlsx-warning-formula-draft-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("create test directory");

    let cancel_app = SheetsApp::new().expect("create cancel SheetsApp");
    let cancel_state = cross_sheet_state();
    cancel_state.mark_saved();
    let cancel_menu = std::sync::Arc::new(NativeMenuBar::new());
    let cancel_operation = continue_xlsx_warning_with_formula_draft(
        &cancel_app,
        &cancel_state,
        &cancel_menu,
        "XLSX Cancel Candidate",
        &dir,
    );
    crate::open_operations::cancel_pending_replacement(&cancel_app, &cancel_state);
    cancel_app.set_save_changes_open(false);
    assert_eq!(cancel_state.current.borrow().name, "Data");
    assert_eq!(cancel_app.get_formula_edit_buffer(), "=1+2");
    assert!(cancel_state.pending_xlsx_import.borrow().is_none());
    assert!(!cancel_state
        .open_operations
        .borrow()
        .is_current(cancel_operation));

    let save_app = SheetsApp::new().expect("create save SheetsApp");
    let save_state = cross_sheet_state();
    save_state.mark_saved();
    let saved_path = dir.join("saved-before-xlsx-import.loomtable");
    *save_state.save_path.borrow_mut() = Some(saved_path.clone());
    let recovery_path = attach_test_worker(&save_app, &save_state, "xlsx-formula-draft-save");
    let save_menu = std::sync::Arc::new(NativeMenuBar::new());
    let save_operation = continue_xlsx_warning_with_formula_draft(
        &save_app,
        &save_state,
        &save_menu,
        "XLSX Save Candidate",
        &dir,
    );
    register_cell_edit_action(&save_app, &save_state, &save_menu);
    assert!(
        crate::open_operations::save_changes_and_resume(&save_app, &save_state, &save_menu)
            .expect("save formula draft before accepting XLSX")
    );
    wait_for_save_test_completion(&save_app, &save_state, &save_menu);
    let saved = load_workbook(&saved_path).expect("reopen saved workbook");
    assert_eq!(
        saved.sheets[0].raw(CellRef { row: 0, col: 0 }),
        Some("=1+2")
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while !save_app.get_xlsx_import_warning_open() {
        crate::open_operations::process_completions(&save_app, &save_state, &save_menu);
        assert!(
            std::time::Instant::now() < deadline,
            "reloaded XLSX candidate did not restore its loss warning"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(save_app
        .get_xlsx_import_warning_message()
        .contains("defined names"));
    continue_pending_xlsx_import(&save_app, &save_state, &save_menu);
    assert_eq!(save_state.current.borrow().name, "XLSX Save Candidate");
    assert!(!save_state
        .open_operations
        .borrow()
        .is_current(save_operation));
    drop(save_state.workbook_worker.borrow_mut().take());
    std::fs::remove_dir_all(&recovery_path).ok();

    let discard_app = SheetsApp::new().expect("create discard SheetsApp");
    let discard_state = cross_sheet_state();
    discard_state.mark_saved();
    let discard_menu = std::sync::Arc::new(NativeMenuBar::new());
    let discard_operation = continue_xlsx_warning_with_formula_draft(
        &discard_app,
        &discard_state,
        &discard_menu,
        "XLSX Discard Candidate",
        &dir,
    );
    crate::open_operations::discard_changes_and_resume(&discard_app, &discard_state, &discard_menu);
    assert_eq!(
        discard_state.current.borrow().name,
        "XLSX Discard Candidate"
    );
    assert_eq!(
        discard_app.get_formula_edit_buffer(),
        discard_app.get_selection_formula()
    );
    assert!(!discard_state
        .open_operations
        .borrow()
        .is_current(discard_operation));

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn second_replacement_requests_preserve_held_open_candidate_until_cancel() {
    set_platform();
    let dir = std::env::temp_dir().join(format!(
        "loom-sheets-held-open-candidate-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("create test directory");
    let candidate_path = dir.join("candidate.loomtable");
    let mut candidate = Sheet::new("Held candidate");
    candidate.set_str("A1", "candidate data");
    save_workbook(&candidate_path, &[candidate], 0).expect("save candidate workbook");

    let app = SheetsApp::new().expect("create SheetsApp");
    let state = cross_sheet_state();
    state.mark_saved();
    let menu_service = std::sync::Arc::new(NativeMenuBar::new());
    let operation = start_open_with_formula_draft(&app, &state, candidate_path, &menu_service);
    let original_workbook = sheet_to_json(&state.current.borrow());
    assert!(crate::open_operations::test_support::has_held_candidate(
        &state.open_operations.borrow()
    ));

    assert!(crate::open_operations::request_replacement_after_dialog(
        &app,
        &state,
        crate::open_operations::PendingReplacement::NewWorkbook,
    ));
    assert_eq!(
        state.pending_replacement.get(),
        Some(crate::open_operations::PendingReplacement::OpenCandidate),
        "a second New request must not replace the held Open decision"
    );
    assert!(crate::open_operations::request_replacement_after_dialog(
        &app,
        &state,
        crate::open_operations::PendingReplacement::OpenWorkbook,
    ));
    assert_eq!(
        state.pending_replacement.get(),
        Some(crate::open_operations::PendingReplacement::OpenCandidate),
        "a second Open request must not replace the held Open decision"
    );
    assert!(crate::open_operations::test_support::has_held_candidate(
        &state.open_operations.borrow()
    ));

    crate::open_operations::cancel_pending_replacement(&app, &state);
    app.set_save_changes_open(false);

    assert_eq!(state.pending_replacement.get(), None);
    assert!(!state.open_operations.borrow().is_current(operation));
    assert!(!crate::open_operations::test_support::has_held_candidate(
        &state.open_operations.borrow()
    ));
    assert_eq!(sheet_to_json(&state.current.borrow()), original_workbook);
    assert_eq!(app.get_formula_edit_buffer(), "=1+2");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn second_requests_and_cancel_preserve_held_xlsx_candidate_and_recovery() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    let state = cross_sheet_state();
    state.mark_saved();
    let recovery_dir = attach_test_worker(&app, &state, "held-xlsx-candidate");
    let candidate_dir = recovery_dir.with_file_name(format!(
        "{}-candidates",
        recovery_dir
            .file_name()
            .expect("test recovery directory name")
            .to_string_lossy()
    ));
    std::fs::create_dir_all(&candidate_dir).expect("create XLSX candidate directory");
    let previous_path = std::env::temp_dir().join("before-xlsx-candidate.loomtable");
    *state.save_path.borrow_mut() = Some(previous_path.clone());
    let (before_sheets, before_active) = workbook_sheets(&state);
    let before_workbook = before_sheets.iter().map(sheet_to_json).collect::<Vec<_>>();
    let before_package =
        workbook_package_bytes(&before_sheets, before_active).expect("package recovery snapshot");
    let menu_service = std::sync::Arc::new(NativeMenuBar::new());
    let operation = continue_xlsx_warning_with_formula_draft(
        &app,
        &state,
        &menu_service,
        "Held XLSX Candidate",
        &candidate_dir,
    );

    assert!(!app.get_xlsx_import_warning_open());
    assert!(!app.get_xlsx_import_warning_message().is_empty());
    for replacement in [
        crate::open_operations::PendingReplacement::NewWorkbook,
        crate::open_operations::PendingReplacement::OpenWorkbook,
    ] {
        assert!(crate::open_operations::request_replacement_after_dialog(
            &app,
            &state,
            replacement,
        ));
        assert_eq!(
            state.pending_replacement.get(),
            Some(crate::open_operations::PendingReplacement::OpenCandidate)
        );
        assert!(state.pending_xlsx_import.borrow().is_some());
        assert!(state.open_operations.borrow().is_current(operation));
    }

    crate::open_operations::cancel_pending_replacement(&app, &state);
    app.set_save_changes_open(false);

    assert_eq!(state.pending_replacement.get(), None);
    assert!(state.pending_xlsx_import.borrow().is_none());
    assert!(!state.open_operations.borrow().is_current(operation));
    assert!(!app.get_xlsx_import_warning_open());
    assert!(app.get_xlsx_import_warning_message().is_empty());
    assert_eq!(app.get_formula_edit_buffer(), "=1+2");
    assert_eq!(*state.save_path.borrow(), Some(previous_path));
    let (after_sheets, after_active) = workbook_sheets(&state);
    assert_eq!(after_active, before_active);
    assert_eq!(
        after_sheets.iter().map(sheet_to_json).collect::<Vec<_>>(),
        before_workbook
    );

    drop(state.workbook_worker.borrow_mut().take());
    assert_eq!(
        recovered_worker_payload(&recovery_dir),
        Some(before_package),
        "cancelling the held XLSX candidate must preserve durable recovery"
    );
    std::fs::remove_dir_all(&candidate_dir).expect("remove XLSX candidate directory");
    crate::cell_edit_recovery::remove_test_recovery_data(&recovery_dir);
}

#[test]
fn formula_draft_preflight_gates_open_and_new_before_their_replacement_actions() {
    set_platform();
    let open_app = SheetsApp::new().expect("create Open SheetsApp");
    let open_state = cross_sheet_state();
    open_state.mark_saved();
    open_app.set_selected_cell("A1".into());
    project_current(&open_app, &open_state);
    open_app.set_formula_edit_buffer("=1+2".into());
    let open_menu = std::sync::Arc::new(NativeMenuBar::new());

    if !crate::open_operations::request_replacement_after_dialog(
        &open_app,
        &open_state,
        crate::open_operations::PendingReplacement::OpenWorkbook,
    ) {
        crate::open_operations::open_workbook_from_picker(&open_app, &open_state, &open_menu, None);
    }

    assert!(open_app.get_save_changes_open());
    assert_eq!(
        open_state.pending_replacement.get(),
        Some(crate::open_operations::PendingReplacement::OpenWorkbook)
    );
    assert_eq!(open_state.current.borrow().name, "Data");
    assert_eq!(open_app.get_formula_edit_buffer(), "=1+2");
    assert!(open_app.get_status_left().contains("Unsaved changes"));

    set_platform();
    let new_app = SheetsApp::new().expect("create New SheetsApp");
    let new_state = cross_sheet_state();
    new_state.mark_saved();
    new_app.set_selected_cell("A1".into());
    project_current(&new_app, &new_state);
    new_app.set_formula_edit_buffer("=1+2".into());
    let new_menu = std::sync::Arc::new(NativeMenuBar::new());

    if !crate::open_operations::request_replacement_after_dialog(
        &new_app,
        &new_state,
        crate::open_operations::PendingReplacement::NewWorkbook,
    ) {
        crate::open_operations::begin_new_workbook(&new_app, &new_state, &new_menu);
    }

    assert!(new_app.get_save_changes_open());
    assert_eq!(
        new_state.pending_replacement.get(),
        Some(crate::open_operations::PendingReplacement::NewWorkbook)
    );
    assert_eq!(new_state.current.borrow().name, "Data");
    assert_eq!(new_app.get_formula_edit_buffer(), "=1+2");
    assert!(new_app.get_status_left().contains("Unsaved changes"));
}

#[test]
fn async_startup_open_applies_options_only_after_candidate_completion() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    crate::template_navigation::wire_template_navigation(&app);
    let state = cross_sheet_state();
    state.mark_saved();
    let dir = std::env::temp_dir().join(format!("loom-sheets-startup-open-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create test directory");
    let path = dir.join("startup.loomtable");
    let mut candidate = Sheet::new("Startup candidate");
    candidate.set_str("A1", "startup value");
    save_workbook(&path, &[candidate], 0).expect("save startup workbook");

    crate::open_operations::start_startup_open(
        &app,
        &state,
        path,
        crate::open_operations::StartupOpenOptions::new(false, false, true),
    );
    assert_eq!(state.current.borrow().name, "Data");
    assert!(!app.get_template_chooser_open());

    let menu_service = std::sync::Arc::new(NativeMenuBar::new());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while !app.get_template_chooser_open() {
        crate::open_operations::process_completions(&app, &state, &menu_service);
        assert!(
            std::time::Instant::now() < deadline,
            "startup candidate should complete in the background"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }

    assert_eq!(state.current.borrow().name, "Startup candidate");
    assert_eq!(
        state.current.borrow().raw(CellRef { row: 0, col: 0 }),
        Some("startup value")
    );
    app.window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed {
            text: slint::platform::Key::RightArrow.into(),
        });
    assert_eq!(
        app.get_template_selected(),
        3,
        "async startup must focus the template chooser after the window was shown"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn failed_background_open_preserves_workbook_path_and_recovery() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    let state = cross_sheet_state();
    state.mark_saved();
    let recovery_dir = attach_test_worker(&app, &state, "failed-background-open");
    let previous_path = std::env::temp_dir().join("previous-workbook.loomtable");
    *state.save_path.borrow_mut() = Some(previous_path.clone());
    let previous_sheet = sheet_to_json(&state.current.borrow());
    let (previous_sheets, previous_active) = workbook_sheets(&state);
    let previous_package =
        workbook_package_bytes(&previous_sheets, previous_active).expect("package prior recovery");

    let missing_path = recovery_dir.join("missing.loomtable");
    let operation = state
        .open_operations
        .borrow_mut()
        .start_picker_load(missing_path.clone(), state.worker_revision.get())
        .expect("queue missing workbook open");
    let menu_service = std::sync::Arc::new(NativeMenuBar::new());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while state.open_operations.borrow().is_current(operation) {
        crate::open_operations::process_completions(&app, &state, &menu_service);
        assert!(
            std::time::Instant::now() < deadline,
            "failed open completion should return to the UI queue"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }

    assert_eq!(sheet_to_json(&state.current.borrow()), previous_sheet);
    assert_eq!(*state.save_path.borrow(), Some(previous_path));
    assert!(app.get_status_left().contains("Open failed:"));
    assert!(
        !state.open_operations.borrow().is_current(operation),
        "failed operation must be invalidated"
    );

    drop(state.workbook_worker.borrow_mut().take());
    assert_eq!(
        recovered_worker_payload(&recovery_dir),
        Some(previous_package),
        "parse failure must leave durable recovery untouched"
    );
    crate::cell_edit_recovery::remove_test_recovery_data(&recovery_dir);
}

#[test]
fn failed_async_startup_open_preserves_fallback_path_and_recovery() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    let state = cross_sheet_state();
    state.mark_saved();
    let recovery_dir = attach_test_worker(&app, &state, "failed-startup-open");
    let fallback_path = std::env::temp_dir().join("fallback-startup.loomtable");
    *state.save_path.borrow_mut() = Some(fallback_path.clone());
    let fallback_sheet = sheet_to_json(&state.current.borrow());
    let (fallback_sheets, fallback_active) = workbook_sheets(&state);
    let fallback_package = workbook_package_bytes(&fallback_sheets, fallback_active)
        .expect("package fallback recovery");

    let missing_path = recovery_dir.join("missing-startup.loomtable");
    crate::open_operations::start_startup_open(
        &app,
        &state,
        missing_path,
        crate::open_operations::StartupOpenOptions::new(false, false, true),
    );
    let operation =
        crate::open_operations::test_support::current_operation(&state.open_operations.borrow())
            .expect("startup Open operation is active");
    let menu_service = std::sync::Arc::new(NativeMenuBar::new());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while state.open_operations.borrow().is_current(operation) {
        crate::open_operations::process_completions(&app, &state, &menu_service);
        assert!(
            std::time::Instant::now() < deadline,
            "failed startup load should return its completion to the UI queue"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }

    assert_eq!(sheet_to_json(&state.current.borrow()), fallback_sheet);
    assert_eq!(*state.save_path.borrow(), Some(fallback_path));
    assert!(!app.get_template_chooser_open());
    assert!(app.get_status_left().contains("Open failed:"));
    let (actual_sheets, actual_active) = workbook_sheets(&state);
    assert_eq!(
        workbook_package_bytes(&actual_sheets, actual_active).expect("package unchanged fallback"),
        fallback_package
    );
    assert!(!state.open_operations.borrow().is_current(operation));

    drop(state.workbook_worker.borrow_mut().take());
    assert_eq!(
        recovered_worker_payload(&recovery_dir),
        Some(fallback_package),
        "failed startup Open must leave durable fallback recovery unchanged"
    );
    crate::cell_edit_recovery::remove_test_recovery_data(&recovery_dir);
}
