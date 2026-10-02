//! Discard changes on close: nothing is saved, this session's recovery stores
//! are cleared under their locks, and the window closes.

use super::close_operation_journeys::{
    attach_worker, close_test_app, pump_worker_until, start_test_worker_gate,
    submit_missing_sheet_update, wire_close_actions, ScratchDirectory,
};
use super::*;
use std::path::{Path, PathBuf};
use std::time::Duration;

fn recovery_entries(directory: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .map(|entry| entry.expect("recovery entry").file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| name != ".checkpoint.lock" && name != ".sheets-writer.lock")
        .collect();
    names.sort();
    names
}

fn versioned_recovery_directory(directory: &Path) -> PathBuf {
    crate::cell_edit_recovery::versioned_directory_for(directory).expect("versioned directory")
}

fn seed_legacy_recovery(directory: &Path) {
    let package = workbook_package_bytes(&[Sheet::new("Recovered")], 0).expect("package");
    let mut legacy = loom_production::snapshot::SnapshotRecovery::open_at(directory)
        .expect("open legacy recovery");
    legacy
        .record("legacy workbook", package.clone())
        .expect("record legacy workbook");
    legacy
        .checkpoint("legacy sheets/1", package)
        .expect("checkpoint legacy workbook");
}

/// Attach a worker that leaves any recovered or legacy store untouched, like a
/// startup that restored a draft, and return its startup report.
fn attach_recovered_worker(
    app: &SheetsApp,
    state: &GuiState,
    recovery_path: &Path,
) -> workbook_worker::WorkerStartup {
    let (worker, startup) = workbook_worker::WorkbookWorker::start_at_with_file_completions(
        recovery_path.to_path_buf(),
        "loom.sheets/1",
        state.save_operations.borrow().sender(),
        state.export_operations.borrow().sender(),
    )
    .expect("start recovered worker");
    let revision = state.next_worker_revision();
    let (sheets, active) = workbook_sheets(state);
    let model = worker
        .initialize_workbook_without_recovery(revision, active, sheets)
        .expect("initialize recovered worker");
    state.last_queued_worker_revision.set(revision);
    let generation = state.open_operations.borrow().document_generation();
    crate::worker_failure::mark_full_resync_accepted(state, generation, revision);
    state.install_workbook(model.sheets, model.active_sheet);
    project_current(app, state);
    state.mark_saved();
    let result = worker.wait_for_result(revision).expect("initial result");
    assert!(apply_workbook_worker_result(app, state, result));
    *state.workbook_worker.borrow_mut() = Some(worker);
    startup
}

fn open_close_decision(app: &SheetsApp, state: &GuiState) {
    app.window().show().expect("show root window");
    app.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    pump_worker_until(app, state, || {
        state.close_state.get() == close_operations::CloseState::DirtyDecision
    });
    assert!(app.get_save_changes_open() && app.get_save_changes_close_mode());
}

fn submit_edit(state: &GuiState, raw: &str) -> u64 {
    let revision = state.next_worker_revision();
    state
        .workbook_worker
        .borrow()
        .as_ref()
        .expect("worker")
        .submit_cell(workbook_worker::CellUpdate {
            revision,
            active_sheet: 0,
            sheet: 0,
            cell: CellRef::parse("A1").expect("cell reference"),
            raw: Some(raw.into()),
        })
        .expect("submit edit");
    state.last_queued_worker_revision.set(revision);
    revision
}

fn reopen_recovery(directory: &Path) -> Option<Vec<u8>> {
    let (reopened, restored) =
        crate::cell_edit_recovery::CellEditRecovery::open_at(directory).expect("reopen recovery");
    drop(reopened);
    restored
}

#[test]
fn close_dialog_offers_a_named_discard_changes_action() {
    i_slint_backend_testing::init_no_event_loop();
    let app = SheetsApp::new().expect("create SheetsApp");
    app.set_save_changes_document("Budget".into());
    app.set_save_changes_close_mode(true);
    app.set_save_changes_open(true);
    let find = |label: &str| {
        i_slint_backend_testing::ElementHandle::find_by_accessible_label(&app, label).count()
    };
    assert!(
        find("Discard changes") >= 1,
        "close dialog needs Discard changes"
    );
    assert!(find("Cancel") >= 1 && find("Save and close") >= 1);
    app.set_save_changes_close_mode(false);
    assert!(find("Discard") >= 1, "replace prompt keeps its Discard");
}

#[test]
fn discard_on_close_clears_the_versioned_store_and_the_next_startup_is_blank() {
    let recovery = ScratchDirectory::new();
    let (app, state) = close_test_app([]);
    attach_worker(&app, &state, &recovery.0, true);
    wire_close_actions(&app, &state);
    let edit = submit_edit(&state, "unsaved edit");
    pump_worker_until(&app, &state, || {
        state.applied_worker_result_revision.get() >= edit
    });
    state.mark_content_dirty();
    let versioned = versioned_recovery_directory(&recovery.0);
    assert!(
        !recovery_entries(&versioned).is_empty(),
        "the unsaved edit must be protected before Discard"
    );
    let save_target = recovery.0.join("must-stay-unwritten.loomtable");
    *state.save_path.borrow_mut() = Some(save_target.clone());

    open_close_decision(&app, &state);
    app.invoke_save_changes_discard();

    assert!(
        !app.window().is_visible(),
        "Discard changes closes the window"
    );
    assert_eq!(state.close_state.get(), close_operations::CloseState::Idle);
    assert!(!save_target.exists(), "Discard must not save");
    assert_eq!(recovery_entries(&versioned), Vec::<String>::new());
    drop(state.workbook_worker.borrow_mut().take());
    assert!(
        reopen_recovery(&recovery.0).is_none(),
        "next startup must be blank"
    );
}

#[test]
fn cancel_on_close_keeps_every_recovery_file() {
    let recovery = ScratchDirectory::new();
    let (app, state) = close_test_app([]);
    attach_worker(&app, &state, &recovery.0, true);
    wire_close_actions(&app, &state);
    let edit = submit_edit(&state, "keep me");
    pump_worker_until(&app, &state, || {
        state.applied_worker_result_revision.get() >= edit
    });
    state.mark_content_dirty();
    let versioned = versioned_recovery_directory(&recovery.0);
    let before = recovery_entries(&versioned);
    assert!(!before.is_empty());

    open_close_decision(&app, &state);
    app.invoke_save_changes_cancel();

    assert!(app.window().is_visible());
    assert_eq!(recovery_entries(&versioned), before);
    drop(state.workbook_worker.borrow_mut().take());
    assert!(
        reopen_recovery(&recovery.0).is_some(),
        "Cancel keeps the recoverable edit"
    );
}

#[test]
fn discard_clears_a_recovered_legacy_draft_so_it_does_not_return() {
    let recovery = ScratchDirectory::new();
    seed_legacy_recovery(&recovery.0);
    assert!(!recovery_entries(&recovery.0).is_empty());
    let (app, state) = close_test_app([]);
    let startup = attach_recovered_worker(&app, &state, &recovery.0);
    assert!(startup.restored_payload.is_some(), "draft is restored");
    wire_close_actions(&app, &state);
    state.mark_content_dirty();

    open_close_decision(&app, &state);
    app.invoke_save_changes_discard();

    assert!(!app.window().is_visible());
    assert_eq!(recovery_entries(&recovery.0), Vec::<String>::new());
    assert_eq!(
        recovery_entries(&versioned_recovery_directory(&recovery.0)),
        Vec::<String>::new()
    );
    drop(state.workbook_worker.borrow_mut().take());
    assert!(
        reopen_recovery(&recovery.0).is_none(),
        "a discarded recovered draft must not return"
    );
}

#[test]
fn discard_waits_for_in_flight_calculation_then_closes_without_stale_writes() {
    let recovery = ScratchDirectory::new();
    let (app, state) = close_test_app([]);
    attach_worker(&app, &state, &recovery.0, true);
    wire_close_actions(&app, &state);
    let (entered, release) = start_test_worker_gate(&state);
    entered
        .recv_timeout(Duration::from_secs(5))
        .expect("worker entered gate");
    submit_edit(&state, "in flight");
    state.mark_content_dirty();
    app.window().show().expect("show root window");
    app.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    assert!(matches!(
        state.close_state.get(),
        close_operations::CloseState::Draining { .. }
    ));
    assert!(
        !app.get_save_changes_open(),
        "no decision while work is in flight"
    );
    assert!(app.window().is_visible());
    release.send(()).expect("release gate");
    pump_worker_until(&app, &state, || {
        state.close_state.get() == close_operations::CloseState::DirtyDecision
    });
    app.invoke_save_changes_discard();

    assert!(!app.window().is_visible());
    assert_eq!(
        recovery_entries(&versioned_recovery_directory(&recovery.0)),
        Vec::<String>::new(),
        "the in-flight edit's recovery data must be gone"
    );
}

#[test]
fn discard_closes_a_worker_failed_workbook_while_save_stays_refused() {
    let recovery = ScratchDirectory::new();
    let (app, state) = close_test_app([]);
    attach_worker(&app, &state, &recovery.0, true);
    wire_close_actions(&app, &state);
    let attempted = recovery.0.join("must-not-save-worker-failed.loomtable");
    *state.save_path.borrow_mut() = Some(attempted.clone());
    let failed = submit_missing_sheet_update(&state);
    pump_worker_until(&app, &state, || {
        state.applied_worker_result_revision.get() >= failed
    });
    assert!(state.unaccepted_worker_revision_message().is_some());

    app.window().show().expect("show root window");
    app.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    assert_eq!(
        state.close_state.get(),
        close_operations::CloseState::DirtyDecision
    );
    app.invoke_save_changes_save();
    assert!(
        app.window().is_visible(),
        "Save is refused for incomplete state"
    );
    assert!(app.get_save_changes_open(), "the decision stays available");
    assert!(!attempted.exists());

    app.invoke_save_changes_discard();
    assert!(!app.window().is_visible(), "Discard changes may abandon it");
    assert!(!attempted.exists());
    assert_eq!(
        recovery_entries(&versioned_recovery_directory(&recovery.0)),
        Vec::<String>::new()
    );
}

#[test]
fn discard_still_closes_when_recovery_never_initialised() {
    let scratch = ScratchDirectory::new();
    // A regular file where the recovery directory should be makes startup fail.
    let blocked = scratch.0.join("recovery-is-a-file");
    std::fs::write(&blocked, b"not a directory").expect("create blocker");
    let (app, state) = close_test_app([]);
    let startup = attach_recovered_worker(&app, &state, &blocked);
    assert!(
        startup.recovery_error.is_some(),
        "recovery must fail to start"
    );
    wire_close_actions(&app, &state);
    state.mark_content_dirty();

    open_close_decision(&app, &state);
    app.invoke_save_changes_discard();

    assert!(!app.window().is_visible(), "Discard still closes");
    assert_eq!(state.close_state.get(), close_operations::CloseState::Idle);
    assert!(blocked.is_file(), "an unrelated file is never touched");
}
