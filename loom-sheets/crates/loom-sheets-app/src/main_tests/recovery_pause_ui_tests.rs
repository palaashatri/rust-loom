use super::*;
use loom_production::fault_injection::{self, FaultStep};

struct Session {
    app: SheetsApp,
    state: Rc<GuiState>,
    recovery_dir: PathBuf,
    versioned: PathBuf,
}

impl Session {
    fn new(name: &str) -> Self {
        set_platform();
        let app = SheetsApp::new().expect("create SheetsApp");
        let state = cross_sheet_state();
        let recovery_dir = attach_test_worker(&app, &state, name);
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
        }
    }

    fn commit(&self, address: &str, raw: &str) {
        self.app.set_selected_cell(address.into());
        self.app.invoke_commit_selected_cell(raw.into());
    }

    /// Deliver the newest worker result the way the 16 ms tick does.
    fn settle(&self) {
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

    fn raw(&self, address: &str) -> Option<String> {
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
        crate::cell_edit_recovery::remove_test_recovery_data(&self.recovery_dir);
    }
}

fn pause_recovery(session: &Session) {
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
