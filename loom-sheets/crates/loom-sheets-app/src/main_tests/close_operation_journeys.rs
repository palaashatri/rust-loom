use super::*;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

static NEXT_CLOSE_TEST_ID: AtomicU64 = AtomicU64::new(0);

struct ScratchDirectory(PathBuf);

impl ScratchDirectory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "loom-sheets-close-journey-{}-{}",
            std::process::id(),
            NEXT_CLOSE_TEST_ID.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).expect("create close journey directory");
        Self(path)
    }
}

impl Drop for ScratchDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn close_test_app(
    save_paths: impl IntoIterator<Item = Option<PathBuf>>,
) -> (SheetsApp, Rc<GuiState>) {
    set_platform();
    let app = SheetsApp::new().expect("create close journey app");
    let mut sheet = Sheet::new("Data");
    sheet.set_str("A1", "1");
    let state = Rc::new(GuiState::new(
        sheet,
        None,
        Rc::new(loom_desktop::ScriptedFileDialogs::new([], save_paths)),
        FileFilter::new("Workbook", ["loomtable"]).expect("workbook filter"),
        FileFilter::new("CSV", ["csv"]).expect("import filter"),
        FileFilter::new("CSV", ["csv"]).expect("CSV filter"),
        FileFilter::new("Excel", ["xlsx"]).expect("XLSX filter"),
    ));
    (app, state)
}

fn assert_status_is_accessible(app: &SheetsApp) {
    let status = app.get_status_left().to_string();
    let accessible_status: Vec<_> =
        i_slint_backend_testing::ElementHandle::find_by_accessible_label(app, &status).collect();
    assert_eq!(
        accessible_status.len(),
        1,
        "full status-left text must remain available through the accessible label: {status}"
    );
}

fn attach_worker(app: &SheetsApp, state: &GuiState, recovery_path: &Path, consume_initial: bool) {
    let save_sender = state.save_operations.borrow().sender();
    let export_sender = state.export_operations.borrow().sender();
    let (worker, startup) = workbook_worker::WorkbookWorker::start_at_with_file_completions(
        recovery_path.to_path_buf(),
        "loom.sheets/1",
        save_sender,
        export_sender,
    )
    .expect("start close journey worker");
    assert!(startup.recovery_error.is_none());
    let revision = state.next_worker_revision();
    let (sheets, active) = workbook_sheets(state);
    let model = worker
        .initialize_workbook(revision, active, sheets)
        .expect("initialize close journey worker");
    state.last_queued_worker_revision.set(revision);
    let generation = state.open_operations.borrow().document_generation();
    crate::worker_failure::mark_full_resync_accepted(state, generation, revision);
    state.install_workbook(model.sheets, model.active_sheet);
    project_current(app, state);
    state.mark_saved();
    if consume_initial {
        let result = worker
            .wait_for_result(revision)
            .expect("initial close journey result");
        assert!(apply_workbook_worker_result(app, state, result));
    }
    *state.workbook_worker.borrow_mut() = Some(worker);
}

fn wire_close_actions(app: &SheetsApp, state: &Rc<GuiState>) {
    let menu_service = std::sync::Arc::new(NativeMenuBar::new());
    close_operations::wire_save_changes_callbacks(app, state, &menu_service);
    close_operations::wire_window_close_handler(app, state);
}

fn pump_worker_until(app: &SheetsApp, state: &GuiState, done: impl Fn() -> bool) {
    let menu_service = std::sync::Arc::new(NativeMenuBar::new());
    let deadline = Instant::now() + Duration::from_secs(5);
    while !done() {
        close_operations::process_worker_tick(app, state, &menu_service);
        assert!(
            Instant::now() < deadline,
            "close worker did not settle: state={:?}, applied={}, queued={}, visible={}, status={}",
            state.close_state.get(),
            state.applied_worker_result_revision.get(),
            state.last_queued_worker_revision.get(),
            app.window().is_visible(),
            app.get_status_left()
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    close_operations::process_worker_tick(app, state, &menu_service);
}

fn enqueue_successful_export_completion(state: &GuiState, operation_id: u64, path: PathBuf) {
    let operation = crate::export_operations::ExportOperation {
        operation_id,
        document_generation: state.open_operations.borrow().document_generation(),
        target_revision: state.worker_revision.get(),
        format: crate::export_operations::ExportFormat::Csv,
        path,
        source_name: "Current workbook".into(),
    };
    state.export_operations.borrow_mut().accept(operation_id);
    state
        .export_operations
        .borrow()
        .sender()
        .send(crate::export_operations::ExportCompletion {
            completion_sequence: 1,
            operation,
            result: Ok(crate::export_operations::ExportOutputSummary::Csv {
                sheet_name: "Data".into(),
            }),
            worker_duration: Duration::ZERO,
        })
        .expect("queue export completion");
}

fn start_test_worker_gate(state: &GuiState) -> (mpsc::Receiver<()>, mpsc::Sender<()>) {
    state
        .workbook_worker
        .borrow()
        .as_ref()
        .expect("workbook worker")
        .enqueue_test_gate()
}

fn submit_missing_sheet_update(state: &GuiState) -> u64 {
    let revision = state.next_worker_revision();
    state
        .workbook_worker
        .borrow()
        .as_ref()
        .expect("workbook worker")
        .submit_cell(workbook_worker::CellUpdate {
            revision,
            active_sheet: 0,
            sheet: 99,
            cell: CellRef::parse("A1").expect("cell reference"),
            raw: Some("invalid update".into()),
        })
        .expect("submit invalid worker update");
    state.last_queued_worker_revision.set(revision);
    revision
}

#[test]
fn dirty_close_dialog_exposes_save_or_cancel_with_close_copy() {
    i_slint_backend_testing::init_no_event_loop();
    let app = SheetsApp::new().expect("create SheetsApp");
    app.set_save_changes_document("Budget".into());
    app.set_save_changes_close_mode(true);
    app.set_save_changes_open(true);

    let expected_prompt = "Save changes to \"Budget\" before closing?";
    assert_eq!(app.get_save_changes_prompt().as_str(), expected_prompt);
    let prompt: Vec<_> =
        i_slint_backend_testing::ElementHandle::find_by_accessible_label(&app, expected_prompt)
            .collect();
    assert_eq!(
        prompt.len(),
        1,
        "close prompt must be available to assistive tech"
    );

    let discard: Vec<_> =
        i_slint_backend_testing::ElementHandle::find_by_accessible_label(&app, "Discard").collect();
    assert!(
        discard.is_empty(),
        "close dialog must not offer recovery-unsafe Discard"
    );
    let save: Vec<_> =
        i_slint_backend_testing::ElementHandle::find_by_accessible_label(&app, "Save and close")
            .collect();
    let cancel: Vec<_> =
        i_slint_backend_testing::ElementHandle::find_by_accessible_label(&app, "Cancel").collect();
    assert!(!save.is_empty(), "close dialog must expose Save and close");
    assert!(!cancel.is_empty(), "close dialog must expose Cancel");
}

#[test]
fn close_waits_for_the_real_initial_worker_result() {
    let (app, state) = close_test_app([]);
    let recovery = ScratchDirectory::new();
    attach_worker(&app, &state, &recovery.0, false);
    wire_close_actions(&app, &state);
    app.window().show().expect("show root window");

    assert_eq!(state.applied_worker_result_revision.get(), 0);
    assert_eq!(state.last_queued_worker_revision.get(), 1);
    app.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    assert!(app.window().is_visible());

    pump_worker_until(&app, &state, || !app.window().is_visible());
    assert_eq!(state.applied_worker_result_revision.get(), 1);
}

#[test]
fn stale_worker_failure_does_not_abort_close_before_newer_accepted_result() {
    let recovery = ScratchDirectory::new();
    let (app, state) = close_test_app([]);
    attach_worker(&app, &state, &recovery.0, true);

    let (first_entered, release_first) = start_test_worker_gate(&state);
    first_entered
        .recv_timeout(Duration::from_secs(5))
        .expect("worker entered first deterministic gate");
    let stale_revision = submit_missing_sheet_update(&state);
    let (second_entered, release_second) = start_test_worker_gate(&state);
    let current_revision = state.next_worker_revision();
    state
        .workbook_worker
        .borrow()
        .as_ref()
        .expect("workbook worker")
        .submit_cell(workbook_worker::CellUpdate {
            revision: current_revision,
            active_sheet: 0,
            sheet: 0,
            cell: CellRef::parse("A1").expect("cell reference"),
            raw: Some("1".into()),
        })
        .expect("submit newer accepted update");
    state.last_queued_worker_revision.set(current_revision);
    release_first.send(()).expect("release first worker gate");
    second_entered
        .recv_timeout(Duration::from_secs(5))
        .expect("worker reached gate after stale failure");

    wire_close_actions(&app, &state);
    app.window().show().expect("show root window");
    app.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    assert_eq!(
        state.close_state.get(),
        close_operations::CloseState::Draining {
            target_revision: current_revision
        }
    );

    let menu_service = std::sync::Arc::new(NativeMenuBar::new());
    close_operations::process_worker_tick(&app, &state, &menu_service);
    let waited_for_current_revision = state.close_state.get()
        == close_operations::CloseState::Draining {
            target_revision: current_revision,
        };
    let visible_after_stale_error = app.window().is_visible();
    let applied_stale_revision = state.applied_worker_result_revision.get();
    release_second
        .send(())
        .expect("release current worker gate");

    assert!(
        waited_for_current_revision,
        "stale worker failure at revision {stale_revision} aborted close before accepted revision {current_revision}"
    );
    assert!(visible_after_stale_error);
    assert_eq!(applied_stale_revision, stale_revision);
    pump_worker_until(&app, &state, || !app.window().is_visible());
    assert_eq!(state.applied_worker_result_revision.get(), current_revision);
}

#[test]
fn dirty_close_modal_keeps_same_tick_export_outcome_accessible() {
    let (app, state) = close_test_app([]);
    let path = PathBuf::from("export-before-dirty-close.csv");
    state.mark_content_dirty();
    state
        .close_state
        .set(close_operations::CloseState::Draining {
            target_revision: state.last_queued_worker_revision.get(),
        });
    app.set_close_draining(true);
    app.window().show().expect("show root window");
    enqueue_successful_export_completion(&state, 1, path.clone());

    let menu_service = std::sync::Arc::new(NativeMenuBar::new());
    close_operations::process_worker_tick(&app, &state, &menu_service);

    assert_eq!(
        state.close_state.get(),
        close_operations::CloseState::DirtyDecision
    );
    assert!(app.get_save_changes_open());
    let status = app.get_status_left().to_string();
    assert!(
        status.contains("Exported CSV") && status.contains(&path.display().to_string()),
        "dirty-close prompt replaced the completed export outcome: {status}"
    );
    let status_accessibility: Vec<_> =
        i_slint_backend_testing::ElementHandle::find_by_accessible_label(&app, &status).collect();
    assert_eq!(
        status_accessibility.len(),
        1,
        "full file outcome should remain exposed through the accessible status label"
    );
}

#[test]
fn same_tick_worker_failure_preserves_file_outcome_accessibly() {
    let recovery = ScratchDirectory::new();
    let (app, state) = close_test_app([]);
    attach_worker(&app, &state, &recovery.0, true);

    let (first_entered, release_first) = start_test_worker_gate(&state);
    first_entered
        .recv_timeout(Duration::from_secs(5))
        .expect("worker entered first deterministic gate");
    let target_revision = submit_missing_sheet_update(&state);
    let (second_entered, release_second) = start_test_worker_gate(&state);
    release_first.send(()).expect("release first worker gate");
    second_entered
        .recv_timeout(Duration::from_secs(5))
        .expect("worker published target failure before second gate");

    let path = PathBuf::from("export-alongside-worker-failure.csv");
    enqueue_successful_export_completion(&state, 1, path.clone());
    wire_close_actions(&app, &state);
    app.window().show().expect("show root window");
    app.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    assert_eq!(
        state.close_state.get(),
        close_operations::CloseState::Draining { target_revision }
    );

    let menu_service = std::sync::Arc::new(NativeMenuBar::new());
    close_operations::process_worker_tick(&app, &state, &menu_service);
    let status = app.get_status_left().to_string();
    let visible = app.window().is_visible();
    release_second.send(()).expect("release second worker gate");

    assert!(visible, "target worker failure must keep the window open");
    let export_outcome = format!(
        "Exported CSV from Current workbook · Data (formulas preserved) to {}",
        path.display()
    );
    assert_eq!(
        status.matches(&export_outcome).count(),
        1,
        "same-tick worker failure duplicated or hid the complete export outcome: {status}"
    );
    assert!(
        status.contains("Exported CSV") && status.contains(&path.display().to_string()),
        "worker close failure replaced the same-tick file outcome: {status}"
    );
    assert!(
        status.contains("Workbook work failed before closing")
            && status.contains("cell edit targets missing sheet 99"),
        "status omitted the complete target worker failure: {status}"
    );
    let status_accessibility: Vec<_> =
        i_slint_backend_testing::ElementHandle::find_by_accessible_label(&app, &status).collect();
    assert_eq!(
        status_accessibility.len(),
        1,
        "combined file and worker failures should remain available through the accessible status label"
    );
}

#[test]
fn checkpoint_failure_during_close_keeps_dialog_and_full_error() {
    let (app, state) = close_test_app([]);
    let path = PathBuf::from("checkpoint-failed.loomtable");
    let operation = state
        .save_operations
        .borrow_mut()
        .begin_operation(
            state.open_operations.borrow().document_generation(),
            state.last_queued_worker_revision.get(),
            None,
        )
        .expect("begin close Save");
    state
        .close_state
        .set(close_operations::CloseState::SavingForClose);
    app.set_close_draining(true);
    app.set_save_changes_close_mode(true);
    app.set_save_changes_open(true);
    app.window().show().expect("show root window");
    state
        .save_operations
        .borrow()
        .sender()
        .send(crate::save_operations::SaveCompletion {
            completion_sequence: 1,
            operation,
            path: path.clone(),
            write_result: Ok(()),
            checkpoint_result: Some(Err("recovery journal is full".into())),
            baseline: None,
        })
        .expect("queue failed recovery checkpoint");

    let menu_service = std::sync::Arc::new(NativeMenuBar::new());
    close_operations::process_worker_tick(&app, &state, &menu_service);

    assert!(app.window().is_visible());
    assert!(app.get_save_changes_open());
    assert_eq!(
        state.close_state.get(),
        close_operations::CloseState::DirtyDecision
    );
    assert!(app.get_status_left().contains("recovery checkpoint failed"));
    assert!(app.get_status_left().contains("recovery journal is full"));
    assert!(app.get_status_left().contains(&path.display().to_string()));
}

#[test]
fn dirty_close_cancel_keeps_the_workbook_open_and_dirty() {
    let (app, state) = close_test_app([]);
    let recovery = ScratchDirectory::new();
    attach_worker(&app, &state, &recovery.0, true);
    wire_close_actions(&app, &state);
    state.mark_content_dirty();
    app.window().show().expect("show root window");

    app.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    pump_worker_until(&app, &state, || {
        state.close_state.get() == close_operations::CloseState::DirtyDecision
    });
    assert!(app.get_save_changes_open());
    assert!(app.get_save_changes_close_mode());
    app.invoke_save_changes_cancel();

    assert!(!app.get_save_changes_open());
    assert!(!app.get_save_changes_close_mode());
    assert_eq!(state.close_state.get(), close_operations::CloseState::Idle);
    assert!(state.is_dirty());
    assert!(app.window().is_visible());
}

#[test]
fn dirty_close_save_waits_for_worker_checkpoint_before_hiding() {
    let recovery = ScratchDirectory::new();
    let output = recovery.0.join("saved.loomtable");
    let (app, state) = close_test_app([]);
    attach_worker(&app, &state, &recovery.0, true);
    let menu_service = std::sync::Arc::new(NativeMenuBar::new());
    register_cell_edit_action(&app, &state, &menu_service);
    close_operations::wire_save_changes_callbacks(&app, &state, &menu_service);
    close_operations::wire_window_close_handler(&app, &state);
    *state.save_path.borrow_mut() = Some(output.clone());
    app.invoke_commit_selected_cell("2".into());
    assert_eq!(
        state.current.borrow().raw(CellRef::parse("A1").unwrap()),
        Some("2")
    );
    app.window().show().expect("show root window");

    app.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    pump_worker_until(&app, &state, || {
        state.close_state.get() == close_operations::CloseState::DirtyDecision
    });
    assert!(app.get_save_changes_open());
    app.invoke_save_changes_save();
    assert_eq!(
        state.close_state.get(),
        close_operations::CloseState::SavingForClose
    );

    pump_worker_until(&app, &state, || !app.window().is_visible());
    assert!(output.exists(), "close Save must write the workbook");
    assert!(
        !state.is_dirty(),
        "completed Save must establish the clean baseline"
    );
    assert!(app.get_status_left().contains("Saved"));
}

#[test]
fn save_failure_during_close_keeps_dialog_open_with_full_error() {
    let recovery = ScratchDirectory::new();
    let blocking_parent = recovery.0.join("regular-file");
    std::fs::write(&blocking_parent, b"not a directory").expect("create blocking parent file");
    let destination = blocking_parent.join("failed.loomtable");
    let (app, state) = close_test_app([]);
    attach_worker(&app, &state, &recovery.0, true);
    let menu_service = std::sync::Arc::new(NativeMenuBar::new());
    register_cell_edit_action(&app, &state, &menu_service);
    close_operations::wire_save_changes_callbacks(&app, &state, &menu_service);
    close_operations::wire_window_close_handler(&app, &state);
    *state.save_path.borrow_mut() = Some(destination.clone());
    app.invoke_commit_selected_cell("2".into());
    app.window().show().expect("show root window");

    app.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    pump_worker_until(&app, &state, || {
        state.close_state.get() == close_operations::CloseState::DirtyDecision
    });
    app.invoke_save_changes_save();
    pump_worker_until(&app, &state, || !state.save_operations.borrow().is_active());

    assert!(app.window().is_visible());
    assert!(app.get_save_changes_open());
    assert_eq!(
        state.close_state.get(),
        close_operations::CloseState::DirtyDecision
    );
    assert!(
        app.get_status_left().to_lowercase().contains("failed"),
        "status omitted the complete save failure: {}",
        app.get_status_left()
    );
    assert!(app
        .get_status_left()
        .contains(&destination.display().to_string()));
}

#[test]
fn close_request_defers_while_an_accepted_open_is_loading() {
    let (app, state) = close_test_app([]);
    state.mark_saved();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let mut candidate = Sheet::new("Loaded");
    candidate.set_str("A1", "candidate workbook");
    let loaded = workbook_io::LoadedWorkbook {
        workbook: loom_sheets_core::persistence::WorkbookFile {
            sheets: vec![candidate],
            active: 0,
        },
        warnings: Vec::new(),
    };
    state
        .open_operations
        .borrow_mut()
        .start_picker_load_with(
            PathBuf::from("accepted.loomtable"),
            state.worker_revision.get(),
            move |_| {
                entered_tx.send(()).expect("signal gated Open loader");
                release_rx.recv().expect("release gated Open loader");
                Ok(loaded)
            },
        )
        .expect("accept background Open");
    entered_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("Open loader reached its gate");
    wire_close_actions(&app, &state);
    app.window().show().expect("show root window");

    app.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    assert!(app.window().is_visible());
    assert_eq!(state.close_state.get(), close_operations::CloseState::Idle);
    assert_eq!(state.current.borrow().name, "Data");
    assert!(app.get_status_left().contains("Wait for its result"));

    release_tx.send(()).expect("release accepted Open");
    let menu_service = std::sync::Arc::new(NativeMenuBar::new());
    let deadline = Instant::now() + Duration::from_secs(5);
    while state.open_operations.borrow().has_active_operation() {
        open_operations::process_completions(&app, &state, &menu_service);
        assert!(
            Instant::now() < deadline,
            "Open completion was not delivered"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(state.current.borrow().name, "Loaded");
    assert!(app.window().is_visible());

    app.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    assert!(
        !app.window().is_visible(),
        "a second close can proceed after Open finishes"
    );
}

#[test]
fn unaccepted_worker_revision_blocks_save_export_and_close() {
    let recovery = ScratchDirectory::new();
    let export_path = recovery.0.join("must-not-export-stale-worker-state.csv");
    let (app, state) = close_test_app([Some(export_path.clone())]);
    attach_worker(&app, &state, &recovery.0, true);
    let menu_service = std::sync::Arc::new(NativeMenuBar::new());
    register_cell_edit_action(&app, &state, &menu_service);
    let queued_before = state.last_queued_worker_revision.get();
    state
        .workbook_worker
        .borrow()
        .as_ref()
        .expect("worker")
        .reject_submissions_for_test();

    app.invoke_commit_selected_cell("2".into());
    assert!(state.worker_revision.get() > queued_before);
    assert_eq!(state.last_queued_worker_revision.get(), queued_before);
    assert!(app.get_status_left().contains("Calculation unavailable"));
    let attempted_save = recovery
        .0
        .join("must-not-save-stale-worker-state.loomtable");
    *state.save_path.borrow_mut() = Some(attempted_save.clone());
    close_operations::wire_window_close_handler(&app, &state);
    app.window().show().expect("show root window");

    app.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    assert!(app.window().is_visible());
    assert!(
        !app.get_save_changes_open(),
        "a failed worker submission must be resolved before offering Save and close"
    );
    assert!(
        app.get_status_left().contains("workbook revision"),
        "status did not explain the unaccepted revision: {}",
        app.get_status_left()
    );
    let save_error = crate::save_current_sheet(&app, &state, false)
        .expect_err("Save must refuse a stale worker barrier");
    assert!(save_error.contains("workbook revision"), "{save_error}");
    assert!(!attempted_save.exists());
    crate::wire_export_callbacks(&app, &state);
    app.invoke_export_csv();
    assert!(!state.export_operations.borrow().has_pending());
    assert!(app.get_status_left().contains("workbook revision"));
    assert!(!export_path.exists());
    assert_eq!(
        state.applied_worker_result_revision.get(),
        queued_before,
        "close barrier uses the last accepted worker revision"
    );
}

#[test]
fn idle_worker_input_failure_blocks_save_and_close_until_full_resync() {
    let recovery = ScratchDirectory::new();
    let attempted_save = recovery
        .0
        .join("must-not-save-incomplete-worker-state.loomtable");
    let (app, state) = close_test_app([]);
    attach_worker(&app, &state, &recovery.0, true);

    let failed_revision = submit_missing_sheet_update(&state);
    pump_worker_until(&app, &state, || {
        state.applied_worker_result_revision.get() >= failed_revision
    });

    let failure = state
        .unaccepted_worker_revision_message()
        .expect("consumed worker input failure must remain an admission barrier");
    assert!(
        failure.contains("cell edit targets missing sheet 99"),
        "{failure}"
    );
    assert!(
        state.is_dirty(),
        "worker input failure must keep the document protected"
    );
    *state.save_path.borrow_mut() = Some(attempted_save.clone());
    let save_error = crate::save_current_sheet(&app, &state, false)
        .expect_err("Save must not serialize worker state after an accepted update failed");
    assert!(
        save_error.contains("cell edit targets missing sheet 99"),
        "{save_error}"
    );
    assert!(!attempted_save.exists());

    crate::wire_export_callbacks(&app, &state);
    app.invoke_export_csv();
    assert!(
        app.get_status_left()
            .contains("cell edit targets missing sheet 99"),
        "CSV must be refused before opening the picker: {}",
        app.get_status_left()
    );
    assert_status_is_accessible(&app);
    app.invoke_export_xlsx();
    assert!(
        app.get_status_left()
            .contains("cell edit targets missing sheet 99"),
        "XLSX must be refused before opening the picker: {}",
        app.get_status_left()
    );
    assert_status_is_accessible(&app);
    assert!(!state.export_operations.borrow().has_pending());

    wire_close_actions(&app, &state);
    app.window().show().expect("show root window");
    app.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    assert!(
        app.window().is_visible(),
        "close must preserve a worker-failed workbook"
    );
    assert!(
        !app.get_save_changes_open(),
        "close must not offer Save for incomplete worker state"
    );
    assert!(
        app.get_status_left()
            .contains("cell edit targets missing sheet 99"),
        "accessible status omitted the worker input failure: {}",
        app.get_status_left()
    );
    assert_status_is_accessible(&app);

    let cell_revision = state.next_worker_revision();
    state
        .workbook_worker
        .borrow()
        .as_ref()
        .expect("worker")
        .submit_cell(workbook_worker::CellUpdate {
            revision: cell_revision,
            active_sheet: 0,
            sheet: 0,
            cell: CellRef::parse("A1").expect("cell reference"),
            raw: Some("2".into()),
        })
        .expect("submit later valid cell update");
    state.last_queued_worker_revision.set(cell_revision);
    pump_worker_until(&app, &state, || {
        state.applied_worker_result_revision.get() >= cell_revision
    });
    assert!(
        state.unaccepted_worker_revision_message().is_some(),
        "a later cell result cannot prove a full worker resynchronization"
    );

    crate::record_workbook_snapshot(&state).expect("queue full workbook resynchronization");
    let resync_revision = state.last_queued_worker_revision.get();
    assert!(
        state.unaccepted_worker_revision_message().is_some(),
        "accepting a full replacement cannot clear the marker before its result arrives"
    );
    pump_worker_until(&app, &state, || {
        state.applied_worker_result_revision.get() >= resync_revision
    });
    assert_eq!(state.unaccepted_worker_revision_message(), None);
}

#[test]
fn idle_worker_input_failure_is_cleared_only_by_successful_full_resync() {
    let recovery = ScratchDirectory::new();
    let (app, state) = close_test_app([]);
    attach_worker(&app, &state, &recovery.0, true);

    let failed_revision = submit_missing_sheet_update(&state);
    pump_worker_until(&app, &state, || {
        state.applied_worker_result_revision.get() >= failed_revision
    });
    assert!(
        state.unaccepted_worker_revision_message().is_some(),
        "consuming an accepted worker input failure must retain its error"
    );

    let cell_revision = state.next_worker_revision();
    state
        .workbook_worker
        .borrow()
        .as_ref()
        .expect("worker")
        .submit_cell(workbook_worker::CellUpdate {
            revision: cell_revision,
            active_sheet: 0,
            sheet: 0,
            cell: CellRef::parse("A1").expect("cell reference"),
            raw: Some("2".into()),
        })
        .expect("submit later valid cell update");
    state.last_queued_worker_revision.set(cell_revision);
    pump_worker_until(&app, &state, || {
        state.applied_worker_result_revision.get() >= cell_revision
    });
    assert!(
        state.unaccepted_worker_revision_message().is_some(),
        "a cell result cannot prove a full worker resynchronization"
    );

    let active_revision = state.next_worker_revision();
    state
        .workbook_worker
        .borrow()
        .as_ref()
        .expect("worker")
        .submit_active(active_revision, 0)
        .expect("submit later active-tab result");
    state.last_queued_worker_revision.set(active_revision);
    pump_worker_until(&app, &state, || {
        state.applied_worker_result_revision.get() >= active_revision
    });
    assert!(
        state.unaccepted_worker_revision_message().is_some(),
        "an active-tab result cannot prove a full worker resynchronization"
    );

    crate::record_workbook_snapshot(&state).expect("queue full workbook resynchronization");
    let resync_revision = state.last_queued_worker_revision.get();
    pump_worker_until(&app, &state, || {
        state.applied_worker_result_revision.get() >= resync_revision
    });
    assert_eq!(
        state.unaccepted_worker_revision_message(),
        None,
        "only a successful covering full replacement result clears the barrier"
    );

    let later_failure_revision = submit_missing_sheet_update(&state);
    pump_worker_until(&app, &state, || {
        state.applied_worker_result_revision.get() >= later_failure_revision
    });
    let later_failure = state
        .unaccepted_worker_revision_message()
        .expect("a later accepted worker input error must restore the barrier");
    assert!(
        later_failure.contains(&later_failure_revision.to_string()),
        "{later_failure}"
    );
    assert!(
        later_failure.contains("cell edit targets missing sheet 99"),
        "{later_failure}"
    );
}

#[test]
fn idle_worker_input_failure_requires_confirmation_before_new_or_open() {
    let recovery = ScratchDirectory::new();
    let (app, state) = close_test_app([]);
    attach_worker(&app, &state, &recovery.0, true);
    let generation = state.open_operations.borrow().document_generation();

    let failed_revision = submit_missing_sheet_update(&state);
    pump_worker_until(&app, &state, || {
        state.applied_worker_result_revision.get() >= failed_revision
    });

    assert!(
        crate::open_operations::request_replacement_after_dialog(
            &app,
            &state,
            crate::open_operations::PendingReplacement::NewWorkbook,
        ),
        "New must ask before replacing a workbook with worker input failure"
    );
    assert!(app.get_save_changes_open());
    assert_eq!(
        state.pending_replacement.get(),
        Some(crate::open_operations::PendingReplacement::NewWorkbook)
    );
    assert_eq!(
        state.open_operations.borrow().document_generation(),
        generation,
        "the new workbook must not replace the current generation before confirmation"
    );
    assert_eq!(state.current.borrow().name, "Data");
    assert!(
        app.get_status_left()
            .contains("cell edit targets missing sheet 99"),
        "replacement prompt must keep the worker failure accessible: {}",
        app.get_status_left()
    );
    assert_status_is_accessible(&app);

    crate::open_operations::cancel_save_changes_dialog(&app, &state);
    assert!(
        crate::open_operations::request_replacement_after_dialog(
            &app,
            &state,
            crate::open_operations::PendingReplacement::OpenWorkbook,
        ),
        "Open must ask before replacing a workbook with worker input failure"
    );
    assert!(app.get_save_changes_open());
    assert_eq!(
        state.pending_replacement.get(),
        Some(crate::open_operations::PendingReplacement::OpenWorkbook)
    );
    assert!(
        app.get_status_left()
            .contains("cell edit targets missing sheet 99"),
        "Open replacement prompt must retain the worker failure: {}",
        app.get_status_left()
    );
    assert_status_is_accessible(&app);

    crate::open_operations::cancel_save_changes_dialog(&app, &state);
    assert!(crate::open_operations::request_replacement_after_dialog(
        &app,
        &state,
        crate::open_operations::PendingReplacement::NewWorkbook,
    ));
    let menu_service = std::sync::Arc::new(NativeMenuBar::new());
    crate::open_operations::discard_changes_and_resume(&app, &state, &menu_service);
    let replacement_generation = state.open_operations.borrow().document_generation();
    assert!(replacement_generation > generation);
    let replacement_revision = state.last_queued_worker_revision.get();
    assert_eq!(crate::worker_failure::status_message(&state), None);
    pump_worker_until(&app, &state, || {
        state.applied_worker_result_revision.get() >= replacement_revision
    });
    assert_eq!(state.unaccepted_worker_revision_message(), None);
}
