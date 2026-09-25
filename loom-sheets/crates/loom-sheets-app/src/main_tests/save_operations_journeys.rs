use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

static NEXT_SAVE_TEST_ID: AtomicU64 = AtomicU64::new(0);

fn attach_save_worker(app: &SheetsApp, state: &Rc<GuiState>) -> PathBuf {
    let id = NEXT_SAVE_TEST_ID.fetch_add(1, Ordering::Relaxed);
    let recovery_path = std::env::temp_dir().join(format!(
        "loom-sheets-save-journey-{}-{id}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&recovery_path);
    let sender = state.save_operations.borrow().sender();
    let (worker, startup) = workbook_worker::WorkbookWorker::start_at_with_completions(
        recovery_path.clone(),
        "loom.sheets/1",
        sender,
    )
    .expect("start Save journey worker");
    assert!(startup.recovery_error.is_none());
    let revision = state.next_worker_revision();
    let (sheets, active) = workbook_sheets(state);
    let model = worker
        .initialize_workbook(revision, active, sheets)
        .expect("initialize Save journey workbook");
    state.install_workbook(model.sheets, model.active_sheet);
    state.mark_saved();
    let result = worker
        .wait_for_result(revision)
        .expect("initial Save journey calculation");
    assert!(apply_workbook_worker_result(app, state, result));
    *state.workbook_worker.borrow_mut() = Some(worker);
    recovery_path
}

fn dirty_workbook(app: &SheetsApp, state: &GuiState) {
    state
        .current
        .borrow_mut()
        .set_str("C1", "accepted before Save");
    apply_sheet(app, state);
    assert!(state.is_dirty());
}

fn write_xlsx_with_defined_name(path: &std::path::Path) {
    let mut source = Sheet::new("Budget");
    source.set_str("A1", "candidate XLSX");
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
                    "<definedNames><definedName name=\"BudgetTotal\">Budget!$A$1</definedName></definedNames></workbook>",
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

fn wait_for_save_completion(app: &SheetsApp, state: &GuiState, menu_service: &Arc<NativeMenuBar>) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while state.save_operations.borrow().is_active() {
        crate::save_operations::process_completions(app, state, menu_service);
        assert!(Instant::now() < deadline, "Save completion did not arrive");
        std::thread::sleep(Duration::from_millis(5));
    }
    crate::save_operations::process_completions(app, state, menu_service);
}

fn deliver_save_completion(
    state: &GuiState,
    operation: crate::save_operations::SaveOperation,
    path: PathBuf,
    write_result: Result<(), String>,
    checkpoint_result: Option<Result<(), String>>,
    baseline: Option<(Vec<Sheet>, usize)>,
) {
    state
        .save_operations
        .borrow()
        .sender()
        .send(crate::save_operations::SaveCompletion {
            operation,
            path,
            write_result,
            checkpoint_result,
            baseline,
        })
        .expect("deliver deterministic Save completion");
}

fn new_test_app() -> (SheetsApp, Rc<GuiState>) {
    loom_test_support::capture::set_platform();
    let app = SheetsApp::new().expect("create Save journey SheetsApp");
    crate::template_navigation::wire_template_navigation(&app);
    (app, cross_sheet_state())
}

fn new_test_app_with_cancelled_save_picker() -> (SheetsApp, Rc<GuiState>) {
    loom_test_support::capture::set_platform();
    let app = SheetsApp::new().expect("create Save As cancellation SheetsApp");
    crate::template_navigation::wire_template_navigation(&app);
    let state = Rc::new(GuiState::new(
        Sheet::new("Data"),
        None,
        Rc::new(loom_desktop::ScriptedFileDialogs::new([], [None])),
        FileFilter::new("Workbook", ["loomtable"]).expect("workbook filter"),
        FileFilter::new("CSV", ["csv"]).expect("import filter"),
        FileFilter::new("CSV", ["csv"]).expect("CSV filter"),
        FileFilter::new("Excel", ["xlsx"]).expect("XLSX filter"),
    ));
    (app, state)
}

#[test]
fn save_changes_replaces_only_after_the_native_save_succeeds() {
    let (app, state) = new_test_app();
    let recovery_path = attach_save_worker(&app, &state);
    let save_path = recovery_path.join("saved-before-replacement.loomtable");
    *state.save_path.borrow_mut() = Some(save_path.clone());
    dirty_workbook(&app, &state);
    let menu_service = Arc::new(NativeMenuBar::new());
    register_cell_edit_action(&app, &state, &menu_service);
    {
        let sheet = state.current.borrow();
        let values = evaluate(&sheet);
        update_selection(&app, &sheet, &values, CellRef::parse("D1").unwrap());
    }
    app.set_formula_edit_buffer("formula draft".into());

    assert!(crate::open_operations::request_replacement_after_dialog(
        &app,
        &state,
        PendingReplacement::NewWorkbook,
    ));
    let pending_token = state.pending_replacement_token.get();
    assert!(
        crate::open_operations::save_changes_and_resume(&app, &state, &menu_service,)
            .expect("queue Save Changes")
    );
    assert!(
        app.get_save_changes_open(),
        "decision stays open until file write"
    );
    assert_eq!(state.pending_replacement_token.get(), pending_token);
    assert_eq!(
        state.current.borrow().raw(CellRef::parse("C1").unwrap()),
        Some("accepted before Save")
    );
    assert_eq!(
        state.current.borrow().raw(CellRef::parse("D1").unwrap()),
        Some("formula draft")
    );

    wait_for_save_completion(&app, &state, &menu_service);

    let saved = load_workbook(&save_path).expect("reopen saved original workbook");
    assert_eq!(
        saved.sheets[0].raw(CellRef::parse("C1").unwrap()),
        Some("accepted before Save")
    );
    assert_eq!(
        saved.sheets[0].raw(CellRef::parse("D1").unwrap()),
        Some("formula draft")
    );
    assert!(!app.get_save_changes_open());
    assert_eq!(state.pending_replacement.get(), None);
    assert_eq!(state.current.borrow().name, "Untitled");
    drop(state.workbook_worker.borrow_mut().take());
    let _ = std::fs::remove_dir_all(recovery_path);
}

#[test]
fn save_changes_keeps_newer_edit_open_for_an_explicit_decision() {
    let (app, state) = new_test_app();
    let recovery_path = attach_save_worker(&app, &state);
    let save_path = recovery_path.join("revision-n.loomtable");
    dirty_workbook(&app, &state);
    let menu_service = Arc::new(NativeMenuBar::new());
    assert!(crate::open_operations::request_replacement_after_dialog(
        &app,
        &state,
        PendingReplacement::NewWorkbook,
    ));
    let token = state.pending_replacement_token.get();
    let generation = state.open_operations.borrow().document_generation();
    let revision_n = state.worker_revision.get();
    let (saved_sheets, saved_active) = workbook_sheets(&state);
    save_workbook(&save_path, &saved_sheets, saved_active).expect("write revision N");
    let operation = state
        .save_operations
        .borrow_mut()
        .begin_operation(generation, revision_n, Some(token))
        .expect("begin deterministic Save N");

    state
        .current
        .borrow_mut()
        .set_str("D1", "accepted after Save started");
    apply_sheet(&app, &state);
    let revision_n_plus_one = state.worker_revision.get();
    assert!(revision_n_plus_one > revision_n);
    assert!(state.is_dirty());

    deliver_save_completion(
        &state,
        operation,
        save_path,
        Ok(()),
        Some(Ok(())),
        Some((saved_sheets, saved_active)),
    );
    crate::save_operations::process_completions(&app, &state, &menu_service);

    assert!(app.get_save_changes_open());
    assert_eq!(
        state.pending_replacement.get(),
        Some(PendingReplacement::NewWorkbook)
    );
    assert_eq!(state.current.borrow().name, "Data");
    assert_eq!(
        state.current.borrow().raw(CellRef::parse("D1").unwrap()),
        Some("accepted after Save started")
    );
    assert!(state.is_dirty());
    assert!(app.get_status_left().contains("Newer edits remain unsaved"));

    crate::open_operations::discard_changes_and_resume(&app, &state, &menu_service);
    assert!(!app.get_save_changes_open());
    assert_eq!(state.current.borrow().name, "Untitled");
    drop(state.workbook_worker.borrow_mut().take());
    let _ = std::fs::remove_dir_all(recovery_path);
}

#[test]
fn save_completion_reports_edits_newer_than_the_saved_revision() {
    let (app, state) = new_test_app();
    let recovery_path = attach_save_worker(&app, &state);
    let save_path = recovery_path.join("revision-n.loomtable");
    let menu_service = Arc::new(NativeMenuBar::new());
    dirty_workbook(&app, &state);
    let generation = state.open_operations.borrow().document_generation();
    let revision_n = state.worker_revision.get();
    let (saved_sheets, saved_active) = workbook_sheets(&state);
    let operation = state
        .save_operations
        .borrow_mut()
        .begin_operation(generation, revision_n, None)
        .expect("begin Save N");

    state
        .current
        .borrow_mut()
        .set_str("D1", "accepted after Save started");
    apply_sheet(&app, &state);
    assert!(state.worker_revision.get() > revision_n);

    deliver_save_completion(
        &state,
        operation,
        save_path,
        Ok(()),
        Some(Ok(())),
        Some((saved_sheets, saved_active)),
    );
    crate::save_operations::process_completions(&app, &state, &menu_service);

    assert!(state.is_dirty());
    assert!(app.get_status_left().contains("newer edits remain unsaved"));
    drop(state.workbook_worker.borrow_mut().take());
    let _ = std::fs::remove_dir_all(recovery_path);
}

#[test]
fn explicit_discard_keeps_reloaded_open_candidate_authorized() {
    let (app, state) = new_test_app();
    let recovery_path = attach_save_worker(&app, &state);
    let save_path = recovery_path.join("revision-n.loomtable");
    let candidate_path = recovery_path.join("replacement.loomtable");
    let mut candidate_sheet = Sheet::new("Replacement candidate");
    candidate_sheet.set_str("A1", "replacement data");
    save_workbook(&candidate_path, &[candidate_sheet], 0).expect("write replacement candidate");
    *state.save_path.borrow_mut() = Some(save_path.clone());
    let operation = state
        .open_operations
        .borrow_mut()
        .start_picker_load(candidate_path.clone(), state.worker_revision.get())
        .expect("start replacement Open");
    dirty_workbook(&app, &state);
    let menu_service = Arc::new(NativeMenuBar::new());
    let deadline = Instant::now() + Duration::from_secs(3);
    while !app.get_save_changes_open() {
        crate::open_operations::process_completions(&app, &state, &menu_service);
        assert!(
            Instant::now() < deadline,
            "candidate did not reach Save Changes"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(state.open_operations.borrow().is_current(operation));
    assert!(crate::open_operations::test_support::has_held_candidate(
        &state.open_operations.borrow()
    ));

    let token = state.pending_replacement_token.get();
    let generation = state.open_operations.borrow().document_generation();
    let revision_n = state.worker_revision.get();
    let (saved_sheets, saved_active) = workbook_sheets(&state);
    save_workbook(&save_path, &saved_sheets, saved_active).expect("write revision N");
    let save_operation = state
        .save_operations
        .borrow_mut()
        .begin_operation(generation, revision_n, Some(token))
        .expect("begin deterministic Save N");

    state
        .current
        .borrow_mut()
        .set_str("D1", "accepted after Save started");
    apply_sheet(&app, &state);
    deliver_save_completion(
        &state,
        save_operation,
        save_path,
        Ok(()),
        Some(Ok(())),
        Some((saved_sheets, saved_active)),
    );
    crate::save_operations::process_completions(&app, &state, &menu_service);

    assert!(app.get_save_changes_open());
    assert_eq!(
        state.pending_replacement.get(),
        Some(PendingReplacement::OpenCandidate)
    );
    assert_eq!(state.current.borrow().name, "Data");
    assert_eq!(
        state.current.borrow().raw(CellRef::parse("D1").unwrap()),
        Some("accepted after Save started")
    );

    crate::open_operations::discard_changes_and_resume(&app, &state, &menu_service);
    assert!(!state.save_operations.borrow().is_active());
    assert!(app.get_status_left().contains("reloading the saved file"));
    let deadline = Instant::now() + Duration::from_secs(5);
    while state.current.borrow().name != "Replacement candidate" {
        crate::open_operations::process_completions(&app, &state, &menu_service);
        assert!(
            Instant::now() < deadline,
            "discarded candidate did not reload; status={}; active={:?}; dirty={}",
            app.get_status_left(),
            crate::open_operations::test_support::current_operation(
                &state.open_operations.borrow()
            ),
            state.is_dirty()
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        state.current.borrow().raw(CellRef::parse("A1").unwrap()),
        Some("replacement data")
    );
    drop(state.workbook_worker.borrow_mut().take());
    let _ = std::fs::remove_dir_all(recovery_path);
}

#[test]
fn native_save_failure_keeps_the_pending_replacement_decision() {
    let (app, state) = new_test_app();
    let recovery_path = attach_save_worker(&app, &state);
    let blocked_path = recovery_path.join("blocked-save-target");
    std::fs::create_dir_all(&blocked_path).expect("create directory save target");
    *state.save_path.borrow_mut() = Some(blocked_path.clone());
    dirty_workbook(&app, &state);
    let menu_service = Arc::new(NativeMenuBar::new());

    assert!(crate::open_operations::request_replacement_after_dialog(
        &app,
        &state,
        PendingReplacement::OpenWorkbook,
    ));
    let pending_token = state.pending_replacement_token.get();
    assert!(
        crate::open_operations::save_changes_and_resume(&app, &state, &menu_service,)
            .expect("queue Save Changes")
    );

    crate::open_operations::discard_changes_and_resume(&app, &state, &menu_service);
    assert!(app.get_save_changes_open());
    assert_eq!(
        state.pending_replacement.get(),
        Some(PendingReplacement::OpenWorkbook),
        "Discard must not replace the workbook while Save is in progress"
    );

    wait_for_save_completion(&app, &state, &menu_service);

    assert!(app.get_save_changes_open());
    assert_eq!(
        state.pending_replacement.get(),
        Some(PendingReplacement::OpenWorkbook)
    );
    assert_eq!(state.pending_replacement_token.get(), pending_token);
    assert_eq!(
        state.save_path.borrow().as_deref(),
        Some(blocked_path.as_path())
    );
    assert_eq!(
        state.current.borrow().raw(CellRef::parse("C1").unwrap()),
        Some("accepted before Save")
    );
    assert!(app.get_status_left().as_str().starts_with("Save failed:"));

    crate::open_operations::cancel_pending_replacement(&app, &state);
    app.set_save_changes_open(false);
    drop(state.workbook_worker.borrow_mut().take());
    let _ = std::fs::remove_dir_all(recovery_path);
}

#[test]
fn opening_the_save_target_is_deferred_while_save_is_in_progress() {
    let (app, state) = new_test_app();
    let recovery_path = attach_save_worker(&app, &state);
    let save_path = recovery_path.join("same-path.loomtable");
    *state.save_path.borrow_mut() = Some(save_path.clone());
    save_workbook(&save_path, &state.sheets.borrow(), 0).expect("write existing target");
    let menu_service = Arc::new(NativeMenuBar::new());

    assert!(save_current_sheet(&app, &state, false).expect("queue Save"));
    let generation = state.open_operations.borrow().document_generation();
    assert!(crate::open_operations::request_replacement_after_dialog(
        &app,
        &state,
        PendingReplacement::OpenWorkbook,
    ));

    assert_eq!(state.pending_replacement.get(), None);
    assert_eq!(
        state.open_operations.borrow().document_generation(),
        generation
    );
    assert_eq!(state.current.borrow().name, "Data");
    assert!(app.get_status_left().contains("Save"));

    wait_for_save_completion(&app, &state, &menu_service);
    drop(state.workbook_worker.borrow_mut().take());
    let _ = std::fs::remove_dir_all(recovery_path);
}

#[test]
fn save_completion_keeps_undo_to_the_previous_baseline_dirty() {
    let (app, state) = new_test_app();
    let recovery_path = attach_save_worker(&app, &state);
    let save_path = recovery_path.join("saved-revision.loomtable");
    *state.save_path.borrow_mut() = Some(save_path);
    let menu_service = Arc::new(NativeMenuBar::new());
    register_cell_edit_action(&app, &state, &menu_service);
    register_history_actions(&app, &state, &menu_service);
    {
        let sheet = state.current.borrow();
        update_selection(
            &app,
            &sheet,
            &evaluate(&sheet),
            CellRef::parse("C1").unwrap(),
        );
    }

    app.invoke_commit_selected_cell("revision N".into());
    assert!(state.is_dirty());
    assert!(save_current_sheet(&app, &state, false).expect("queue Save N"));
    app.invoke_undo();
    assert!(
        !state.is_dirty(),
        "undo returns to the previous saved state"
    );

    wait_for_save_completion(&app, &state, &menu_service);

    assert!(
        state.is_dirty(),
        "the previous saved state differs from the newly saved revision N"
    );
    drop(state.workbook_worker.borrow_mut().take());
    let _ = std::fs::remove_dir_all(recovery_path);
}

#[test]
fn save_overwrite_reloads_an_open_candidate_that_read_old_bytes() {
    let (app, state) = new_test_app();
    let recovery_path = attach_save_worker(&app, &state);
    let target_path = recovery_path.join("open-race.loomtable");
    let mut old_file = Sheet::new("Old file contents");
    old_file.set_str("A1", "stale bytes");
    save_workbook(&target_path, &[old_file], 0).expect("write old target contents");
    *state.save_path.borrow_mut() = Some(target_path.clone());

    let (loaded_sender, loaded_receiver) = std::sync::mpsc::channel();
    let (release_sender, release_receiver) = std::sync::mpsc::channel();
    let operation = state
        .open_operations
        .borrow_mut()
        .start_picker_load_with(
            target_path.clone(),
            state.worker_revision.get(),
            move |path| {
                let loaded = crate::workbook_io::load_workbook_with_report(path)?;
                loaded_sender.send(()).map_err(|error| error.to_string())?;
                release_receiver.recv().map_err(|error| error.to_string())?;
                Ok(loaded)
            },
        )
        .expect("start gated Open");
    loaded_receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("loader read old target bytes before Save");
    assert!(state.open_operations.borrow().is_current(operation));

    dirty_workbook(&app, &state);
    let menu_service = Arc::new(NativeMenuBar::new());
    assert!(save_current_sheet(&app, &state, false).expect("queue Save over Open target"));
    wait_for_save_completion(&app, &state, &menu_service);

    let saved = load_workbook(&target_path).expect("read newly saved target");
    assert_eq!(
        saved.sheets[0].raw(CellRef::parse("C1").unwrap()),
        Some("accepted before Save")
    );
    let generation_after_save = state.open_operations.borrow().document_generation();

    release_sender
        .send(())
        .expect("release gated old Open result");
    let deadline = Instant::now() + Duration::from_secs(5);
    while state.open_operations.borrow().document_generation() == generation_after_save {
        crate::open_operations::process_completions(&app, &state, &menu_service);
        assert!(
            Instant::now() < deadline,
            "reloaded Open candidate did not complete"
        );
        std::thread::sleep(Duration::from_millis(5));
    }

    assert_eq!(state.current.borrow().name, "Data");
    assert_eq!(
        state.current.borrow().raw(CellRef::parse("C1").unwrap()),
        Some("accepted before Save"),
        "Open must reload the file after Save overwrites a candidate already read"
    );
    drop(state.workbook_worker.borrow_mut().take());
    let _ = std::fs::remove_dir_all(recovery_path);
}

#[test]
fn save_changes_reloads_an_xlsx_candidate_and_rechecks_its_warning() {
    let (app, state) = new_test_app();
    let recovery_path = attach_save_worker(&app, &state);
    let xlsx_path = recovery_path.join("candidate.xlsx");
    write_xlsx_with_defined_name(&xlsx_path);
    let save_path = recovery_path.join("current.loomtable");
    *state.save_path.borrow_mut() = Some(save_path.clone());

    let operation = state
        .open_operations
        .borrow_mut()
        .start_picker_load(xlsx_path, state.worker_revision.get())
        .expect("start XLSX Open");
    dirty_workbook(&app, &state);
    let menu_service = Arc::new(NativeMenuBar::new());
    let deadline = Instant::now() + Duration::from_secs(3);
    while !app.get_save_changes_open() {
        crate::open_operations::process_completions(&app, &state, &menu_service);
        assert!(
            Instant::now() < deadline,
            "XLSX candidate did not reach Save Changes"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(state.open_operations.borrow().is_current(operation));
    assert!(crate::open_operations::test_support::has_held_candidate(
        &state.open_operations.borrow()
    ));

    assert!(
        crate::open_operations::save_changes_and_resume(&app, &state, &menu_service,)
            .expect("queue Save Changes before XLSX import")
    );
    wait_for_save_completion(&app, &state, &menu_service);

    let deadline = Instant::now() + Duration::from_secs(3);
    while !app.get_xlsx_import_warning_open() {
        crate::open_operations::process_completions(&app, &state, &menu_service);
        assert!(
            Instant::now() < deadline,
            "reloaded XLSX warning did not appear"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(app
        .get_xlsx_import_warning_message()
        .contains("defined names"));
    assert_eq!(state.current.borrow().name, "Data");
    continue_pending_xlsx_import(&app, &state, &menu_service);
    assert_eq!(state.current.borrow().name, "Budget");
    assert_eq!(
        state.current.borrow().raw(CellRef::parse("A1").unwrap()),
        Some("candidate XLSX")
    );
    drop(state.workbook_worker.borrow_mut().take());
    let _ = std::fs::remove_dir_all(recovery_path);
}

#[test]
fn cancelling_save_changes_dialog_says_the_active_save_will_continue() {
    let (app, state) = new_test_app();
    let recovery_path = attach_save_worker(&app, &state);
    let target_path = recovery_path.join("cancel-replacement.loomtable");
    *state.save_path.borrow_mut() = Some(target_path.clone());
    dirty_workbook(&app, &state);
    let menu_service = Arc::new(NativeMenuBar::new());

    assert!(crate::open_operations::request_replacement_after_dialog(
        &app,
        &state,
        PendingReplacement::NewWorkbook,
    ));
    assert!(
        crate::open_operations::save_changes_and_resume(&app, &state, &menu_service,)
            .expect("queue Save Changes")
    );
    crate::open_operations::cancel_save_changes_dialog(&app, &state);

    assert!(!app.get_save_changes_open());
    assert_eq!(state.pending_replacement.get(), None);
    assert!(app
        .get_status_left()
        .contains("Save already in progress will continue"));
    wait_for_save_completion(&app, &state, &menu_service);

    let saved = load_workbook(&target_path).expect("Save continues after canceling replacement");
    assert_eq!(
        saved.sheets[0].raw(CellRef::parse("C1").unwrap()),
        Some("accepted before Save")
    );
    drop(state.workbook_worker.borrow_mut().take());
    let _ = std::fs::remove_dir_all(recovery_path);
}

#[test]
fn save_as_cancellation_keeps_the_save_changes_dialog_and_decision() {
    let (app, state) = new_test_app_with_cancelled_save_picker();
    dirty_workbook(&app, &state);
    let menu_service = Arc::new(NativeMenuBar::new());
    assert!(crate::open_operations::request_replacement_after_dialog(
        &app,
        &state,
        PendingReplacement::NewWorkbook,
    ));
    let pending_token = state.pending_replacement_token.get();

    assert!(
        !crate::open_operations::save_changes_and_resume(&app, &state, &menu_service,)
            .expect("cancel Save As")
    );

    assert!(app.get_save_changes_open());
    assert_eq!(
        state.pending_replacement.get(),
        Some(PendingReplacement::NewWorkbook)
    );
    assert_eq!(state.pending_replacement_token.get(), pending_token);
    assert!(!state.save_operations.borrow().is_active());
}

#[test]
fn stale_save_completion_does_not_install_path_or_baseline() {
    let (app, state) = new_test_app();
    let recovery_path = attach_save_worker(&app, &state);
    let old_path = recovery_path.join("old.loomtable");
    let target_path = recovery_path.join("new.loomtable");
    *state.save_path.borrow_mut() = Some(old_path.clone());
    dirty_workbook(&app, &state);
    let generation = state.open_operations.borrow().document_generation();
    let revision = state.worker_revision.get();
    let operation = state
        .save_operations
        .borrow_mut()
        .begin_operation(generation, revision, None)
        .expect("begin Save operation");
    state
        .workbook_worker
        .borrow()
        .as_ref()
        .expect("worker")
        .queue_save(
            operation.operation_id,
            operation.document_generation,
            operation.target_revision,
            operation.pending_replacement_token,
            target_path.clone(),
        )
        .expect("queue stale Save");
    state.open_operations.borrow_mut().document_replaced();
    let menu_service = Arc::new(NativeMenuBar::new());

    wait_for_save_completion(&app, &state, &menu_service);

    assert!(
        target_path.exists(),
        "worker completed the requested file write"
    );
    assert_eq!(
        state.save_path.borrow().as_deref(),
        Some(old_path.as_path())
    );
    assert!(!state.save_operations.borrow().is_active());
    drop(state.workbook_worker.borrow_mut().take());
    let _ = std::fs::remove_dir_all(recovery_path);
}

#[test]
fn cancelled_save_decision_cannot_resume_after_save_completion() {
    let (app, state) = new_test_app();
    let recovery_path = attach_save_worker(&app, &state);
    let save_path = recovery_path.join("stale-decision.loomtable");
    *state.save_path.borrow_mut() = Some(save_path);
    dirty_workbook(&app, &state);
    let menu_service = Arc::new(NativeMenuBar::new());
    assert!(crate::open_operations::request_replacement_after_dialog(
        &app,
        &state,
        PendingReplacement::NewWorkbook,
    ));
    let original_token = state.pending_replacement_token.get();
    assert!(
        crate::open_operations::save_changes_and_resume(&app, &state, &menu_service,)
            .expect("queue Save for original decision")
    );

    crate::open_operations::cancel_pending_replacement(&app, &state);
    app.set_save_changes_open(false);
    assert!(crate::open_operations::request_replacement_after_dialog(
        &app,
        &state,
        PendingReplacement::OpenWorkbook,
    ));
    assert_ne!(state.pending_replacement_token.get(), original_token);
    assert_eq!(state.pending_replacement.get(), None);
    assert!(!app.get_save_changes_open());

    wait_for_save_completion(&app, &state, &menu_service);

    assert!(!app.get_save_changes_open());
    assert_eq!(state.pending_replacement.get(), None);
    assert_eq!(state.current.borrow().name, "Data");
    drop(state.workbook_worker.borrow_mut().take());
    let _ = std::fs::remove_dir_all(recovery_path);
}

#[test]
fn save_coordinator_rejects_overlap_and_stale_generation() {
    let mut coordinator = crate::save_operations::SaveOperationCoordinator::default();
    let active = coordinator.begin(7, 12, Some(3)).expect("begin first save");
    assert_eq!(active.pending_replacement_token, Some(3));
    assert!(coordinator.begin(7, 12, None).is_err());
    assert!(coordinator.is_current(active, 7));
    assert!(!coordinator.is_current(active, 8));
    coordinator.clear(active);
    assert!(coordinator.begin(8, 13, None).is_ok());
}
