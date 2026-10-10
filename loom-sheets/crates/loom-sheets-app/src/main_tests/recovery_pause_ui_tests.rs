use super::*;
use loom_production::fault_injection::{self, FaultStep};

pub(super) struct Session {
    pub(super) app: SheetsApp,
    pub(super) state: Rc<GuiState>,
    pub(super) recovery_dir: PathBuf,
    versioned: PathBuf,
    /// Another Loom window's recovery writer, when a test plays that window.
    other_window: Option<crate::cell_edit_recovery::CellEditRecovery>,
}

impl Session {
    pub(super) fn new(name: &str) -> Self {
        set_platform();
        let app = SheetsApp::new().expect("create SheetsApp");
        let state = cross_sheet_state();
        let recovery_dir = attach_test_worker(&app, &state, name);
        Self::finish(app, state, recovery_dir, None)
    }

    /// A window whose recovery store could not be opened because another Loom
    /// window already holds it. `leave_draft` makes that other window leave an
    /// unsaved draft behind when it goes away.
    fn with_store_held_elsewhere(name: &str, leave_draft: bool) -> Self {
        set_platform();
        let app = SheetsApp::new().expect("create SheetsApp");
        let state = cross_sheet_state();
        let recovery_dir =
            std::env::temp_dir().join(format!("loom-sheets-held-{name}-{}", std::process::id()));
        crate::cell_edit_recovery::remove_test_recovery_data(&recovery_dir);
        std::fs::create_dir_all(&recovery_dir).expect("create recovery directory");
        let (mut other, _) = crate::cell_edit_recovery::CellEditRecovery::open_at(&recovery_dir)
            .expect("the other window opens the store first");
        if leave_draft {
            let (sheets, active) = workbook_sheets(&state);
            let package =
                crate::workbook_io::workbook_package_bytes(&sheets, active).expect("package");
            other
                .checkpoint_package(package, false, None)
                .expect("the other window checkpoints a draft");
        }
        let (worker, startup) = workbook_worker::WorkbookWorker::start_at_with_completions(
            recovery_dir.clone(),
            "loom.sheets/1",
            state.save_operations.borrow().sender(),
        )
        .expect("start worker");
        assert!(
            startup
                .recovery_error
                .as_deref()
                .is_some_and(|error| error.contains("active writer")),
            "the store must be refused while another window holds it: {:?}",
            startup.recovery_error
        );
        let revision = state.next_worker_revision();
        let (sheets, active) = workbook_sheets(&state);
        let model = worker
            .initialize_workbook(revision, active, sheets)
            .expect("initialize workbook");
        state.last_queued_worker_revision.set(revision);
        state.install_workbook(model.sheets, model.active_sheet);
        state.mark_saved();
        let result = worker.wait_for_result(revision).expect("initial result");
        apply_workbook_worker_result(&app, &state, result);
        *state.workbook_worker.borrow_mut() = Some(worker);
        Self::finish(app, state, recovery_dir, Some(other))
    }

    /// The other window goes away, releasing its recovery locks.
    fn close_other_window(&mut self) {
        self.other_window = None;
    }

    fn finish(
        app: SheetsApp,
        state: Rc<GuiState>,
        recovery_dir: PathBuf,
        other_window: Option<crate::cell_edit_recovery::CellEditRecovery>,
    ) -> Self {
        let versioned = crate::cell_edit_recovery::versioned_directory_for(&recovery_dir)
            .expect("versioned recovery directory");
        let menu_service = std::sync::Arc::new(NativeMenuBar::new());
        register_cell_edit_action(&app, &state, &menu_service);
        crate::recovery_pause::wire(&app, &state);
        Self {
            app,
            state,
            recovery_dir,
            versioned,
            other_window,
        }
    }

    pub(super) fn commit(&self, address: &str, raw: &str) {
        self.app.set_selected_cell(address.into());
        self.app.invoke_commit_selected_cell(raw.into());
    }

    /// Deliver the newest worker result the way the 16 ms tick does.
    pub(super) fn settle(&self) {
        let revision = self.state.worker_revision.get();
        let result = self
            .state
            .workbook_worker
            .borrow()
            .as_ref()
            .unwrap()
            .wait_for_result(revision)
            .expect("worker result");
        apply_workbook_worker_result(&self.app, &self.state, result);
        crate::recovery_pause::sync(&self.app, &self.state);
    }

    pub(super) fn raw(&self, address: &str) -> Option<String> {
        self.state
            .current
            .borrow()
            .raw(CellRef::parse(address).unwrap())
            .map(str::to_owned)
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        fault_injection::clear(&self.versioned);
        *self.state.workbook_worker.borrow_mut() = None;
        self.other_window = None;
        crate::cell_edit_recovery::remove_test_recovery_data(&self.recovery_dir);
    }
}

pub(super) fn pause_recovery(session: &Session) {
    fault_injection::inject(&session.versioned, FaultStep::JournalAppend, 0, 1);
    session.commit("B2", "7");
    session.settle();
}

#[test]
fn a_recovery_failure_blocks_the_next_edit_with_a_clear_message() {
    let session = Session::new("pause-blocks-edit");
    session.commit("B2", "1");
    session.settle();
    assert!(!session.app.get_recovery_paused());

    pause_recovery(&session);
    assert!(session.app.get_recovery_paused());
    assert!(
        session.app.get_window_title().ends_with("recovery paused"),
        "the pause must stay visible in the title: {}",
        session.app.get_window_title()
    );
    assert!(session
        .app
        .get_status_left()
        .starts_with("Recovery paused:"));

    let revision_before = session.state.worker_revision.get();
    session.commit("C3", "must not be applied");
    assert_eq!(session.raw("C3"), None, "a refused edit leaves no trace");
    assert_eq!(
        session.state.worker_revision.get(),
        revision_before,
        "a refused edit must not reach the worker"
    );
    let status = session.app.get_status_left();
    assert!(status.starts_with("Edit not applied"));
    assert!(status.contains("Retry Recovery"));
    assert!(status.contains("Save As"));
}

#[test]
fn retry_recovery_checkpoints_the_whole_workbook_and_resumes_editing() {
    let session = Session::new("pause-retry-ok");
    pause_recovery(&session);
    assert_eq!(session.raw("B2").as_deref(), Some("7"));

    session.app.invoke_retry_recovery();
    assert_eq!(session.app.get_status_left().as_str(), "Retrying recovery…");
    session.settle();

    assert!(!session.app.get_recovery_paused());
    assert_eq!(
        session.app.get_status_left().as_str(),
        "Recovery restored. Your changes are protected again."
    );
    assert!(!session.app.get_window_title().contains("recovery paused"));

    session.commit("C3", "accepted again");
    assert_eq!(session.raw("C3").as_deref(), Some("accepted again"));
    session.settle();
    assert!(!session.app.get_recovery_paused());

    // What a restart would restore includes the edit made before the Retry.
    let payload = {
        *session.state.workbook_worker.borrow_mut() = None;
        recovered_worker_payload(&session.recovery_dir).expect("recovered payload")
    };
    let workbook = restore_workbook_from_snapshot(&payload).expect("decode");
    assert_eq!(
        workbook.sheets[0].raw(CellRef::parse("B2").unwrap()),
        Some("7")
    );
    assert_eq!(
        workbook.sheets[0].raw(CellRef::parse("C3").unwrap()),
        Some("accepted again")
    );
}

#[test]
fn a_failed_retry_is_reported_and_edits_stay_blocked() {
    let session = Session::new("pause-retry-fails");
    pause_recovery(&session);

    fault_injection::inject(&session.versioned, FaultStep::CheckpointPayload, 0, 1);
    session.app.invoke_retry_recovery();
    session.settle();

    assert!(session.app.get_recovery_paused());
    assert!(session
        .app
        .get_status_left()
        .starts_with("Recovery retry failed:"));
    session.commit("C3", "still blocked");
    assert_eq!(session.raw("C3"), None);
    assert!(session
        .app
        .get_status_left()
        .starts_with("Edit not applied"));

    // The disk recovers: a second retry succeeds and clears the block.
    session.app.invoke_retry_recovery();
    session.settle();
    assert!(!session.app.get_recovery_paused());
    session.commit("C3", "now accepted");
    assert_eq!(session.raw("C3").as_deref(), Some("now accepted"));
}

#[test]
fn retry_recovery_is_offered_in_the_palette_only_while_recovery_is_paused() {
    let session = Session::new("pause-palette");
    crate::palette::rebuild_palette(&session.app, "retry recovery");
    assert_eq!(session.app.get_palette_commands().row_count(), 0);

    pause_recovery(&session);
    crate::palette::rebuild_palette(&session.app, "retry recovery");
    assert_eq!(session.app.get_palette_commands().row_count(), 1);

    assert!(crate::palette::dispatch_palette_action(
        &session.app,
        crate::palette::PaletteAction::RetryRecovery
    ));
    assert_eq!(session.app.get_status_left().as_str(), "Retrying recovery…");
    session.settle();
    crate::palette::rebuild_palette(&session.app, "retry recovery");
    assert_eq!(session.app.get_palette_commands().row_count(), 0);
}

#[test]
fn retry_with_healthy_recovery_says_there_is_nothing_to_retry() {
    let session = Session::new("pause-nothing-to-retry");
    session.app.invoke_retry_recovery();
    assert_eq!(
        session.app.get_status_left().as_str(),
        "Recovery is working; there is nothing to retry."
    );
    assert_eq!(
        session.state.worker_revision.get(),
        1,
        "no checkpoint is queued when recovery is healthy"
    );
}

#[test]
fn an_edit_in_flight_when_recovery_fails_stays_visible_and_the_ui_says_it_is_unconfirmed() {
    let session = Session::new("pause-in-flight");
    session.commit("B2", "1");
    session.settle();
    assert!(!session.app.get_recovery_paused());

    // Hold the worker so two edits are accepted on screen and queued, and make
    // recovery fail while they are in flight.
    let (entered, release) = session
        .state
        .workbook_worker
        .borrow()
        .as_ref()
        .unwrap()
        .enqueue_test_gate();
    entered
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("worker is holding at the gate");
    fault_injection::inject(&session.versioned, FaultStep::JournalAppend, 0, 1);
    session.commit("C3", "in flight one");
    session.commit("D4", "in flight two");
    assert_eq!(session.raw("C3").as_deref(), Some("in flight one"));
    assert!(
        !session.app.get_recovery_paused(),
        "recovery has not failed yet"
    );
    release.send(()).expect("release the worker");
    session.settle();

    // Neither edit is dropped from the screen, the document is unsaved, and
    // the user is told plainly what is and is not protected.
    assert!(session.app.get_recovery_paused());
    assert_eq!(session.raw("C3").as_deref(), Some("in flight one"));
    assert_eq!(session.raw("D4").as_deref(), Some("in flight two"));
    assert!(session.state.is_dirty());
    let status = session.app.get_status_left();
    assert!(
        status.contains("still on screen") && status.contains("has not confirmed"),
        "the status must say the edits stayed and are unconfirmed: {status}"
    );
    assert!(status.contains("Retry Recovery") && status.contains("Save"));

    // Retry covers them: a restart restores both.
    session.app.invoke_retry_recovery();
    session.settle();
    assert!(!session.app.get_recovery_paused());
    *session.state.workbook_worker.borrow_mut() = None;
    let payload = recovered_worker_payload(&session.recovery_dir).expect("recovered payload");
    let workbook = restore_workbook_from_snapshot(&payload).expect("decode");
    assert_eq!(
        workbook.sheets[0].raw(CellRef::parse("C3").unwrap()),
        Some("in flight one")
    );
    assert_eq!(
        workbook.sheets[0].raw(CellRef::parse("D4").unwrap()),
        Some("in flight two")
    );
}

#[test]
fn a_store_that_never_opened_pauses_editing_and_names_the_other_window() {
    let session = Session::with_store_held_elsewhere("never-opened", false);
    crate::recovery_pause::sync(&session.app, &session.state);

    assert!(session.app.get_recovery_paused());
    assert!(session.app.get_window_title().ends_with("recovery paused"));
    let status = session.app.get_status_left();
    assert!(
        status.contains("another Loom window"),
        "a second instance must be told who holds recovery: {status}"
    );
    assert!(status.contains("Retry Recovery"));

    session.commit("C3", "must not be applied");
    assert_eq!(
        session.raw("C3"),
        None,
        "editing is blocked, not just warned"
    );
    assert!(session
        .app
        .get_status_left()
        .starts_with("Edit not applied"));
}

#[test]
fn retry_reopens_a_store_that_never_opened_once_the_other_window_is_gone() {
    let mut session = Session::with_store_held_elsewhere("reopen", false);
    crate::recovery_pause::sync(&session.app, &session.state);

    // Still held: the retry says so and editing stays blocked.
    session.app.invoke_retry_recovery();
    session.settle();
    assert!(session.app.get_recovery_paused());
    let status = session.app.get_status_left();
    assert!(
        status.starts_with("Recovery retry failed:") && status.contains("another Loom window"),
        "{status}"
    );

    session.close_other_window();
    session.app.invoke_retry_recovery();
    session.settle();
    assert!(!session.app.get_recovery_paused());
    assert_eq!(
        session.app.get_status_left().as_str(),
        "Recovery restored. Your changes are protected again."
    );
    assert!(!session.app.get_window_title().contains("recovery paused"));

    session.commit("C3", "accepted after the reopen");
    assert_eq!(
        session.raw("C3").as_deref(),
        Some("accepted after the reopen")
    );
    session.settle();
    assert!(!session.app.get_recovery_paused());
    *session.state.workbook_worker.borrow_mut() = None;
    let payload = recovered_worker_payload(&session.recovery_dir).expect("recovered payload");
    let workbook = restore_workbook_from_snapshot(&payload).expect("decode");
    assert_eq!(
        workbook.sheets[0].raw(CellRef::parse("C3").unwrap()),
        Some("accepted after the reopen")
    );
}

#[test]
fn retry_never_overwrites_an_unsaved_draft_it_did_not_restore() {
    let mut session = Session::with_store_held_elsewhere("stale-draft", true);
    crate::recovery_pause::sync(&session.app, &session.state);
    session.close_other_window();

    session.app.invoke_retry_recovery();
    session.settle();

    assert!(session.app.get_recovery_paused());
    let status = session.app.get_status_left();
    assert!(
        status.starts_with("Recovery retry failed:") && status.contains("earlier unsaved draft"),
        "{status}"
    );
    // The earlier draft is still on disk for the next start to restore.
    *session.state.workbook_worker.borrow_mut() = None;
    assert!(recovered_worker_payload(&session.recovery_dir).is_some());
}
