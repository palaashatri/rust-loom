//! Edit admission while durable recovery cannot keep up.
//!
//! The workbook worker latches a recovery failure (a failed journal append, a
//! failed checkpoint, or a failed compaction). While it is latched Sheets does
//! not admit new cell edits, shows why, and offers a retry. Retry sends the
//! whole workbook as a complete checkpoint; only that checkpoint, a Save, or a
//! Save As that really reaches durable recovery clears the pause.

use std::rc::Rc;

use slint::{ComponentHandle, SharedString};

use crate::{GuiState, SheetsApp};

/// Appended to the window title for as long as edits are blocked, so the
/// pause stays visible when other status text changes.
pub(crate) const TITLE_SUFFIX: &str = " \u{2014} recovery paused";

/// Why recovery is paused, if it is.
pub(crate) fn paused_reason(state: &GuiState) -> Option<String> {
    // The title refresh can run while the worker slot is being replaced.
    state
        .workbook_worker
        .try_borrow()
        .ok()?
        .as_ref()
        .and_then(crate::workbook_worker::WorkbookWorker::recovery_pause)
}

fn rejection_message(reason: &str) -> String {
    format!(
        "Edit not applied: Loom cannot keep your changes safe right now ({reason}). \
         Choose Retry Recovery in the command palette, or Save As to keep your work."
    )
}

/// Refuse a new edit while recovery is paused. Call before changing the
/// workbook so a refused edit leaves no trace.
pub(crate) fn reject_edit(app: &SheetsApp, state: &GuiState) -> bool {
    let Some(reason) = paused_reason(state) else {
        return false;
    };
    app.set_recovery_paused(true);
    app.set_status_left(SharedString::from(rejection_message(&reason)));
    true
}

/// Send the whole workbook to the worker as a complete recovery checkpoint.
pub(crate) fn retry(app: &SheetsApp, state: &GuiState) -> bool {
    if state.workbook_worker.borrow().is_none() {
        app.set_status_left("Recovery is not running for this workbook.".into());
        return false;
    }
    if paused_reason(state).is_none() {
        app.set_recovery_paused(false);
        app.set_status_left("Recovery is working; there is nothing to retry.".into());
        return false;
    }
    if crate::close_operations::reject_admission(app, state) {
        return false;
    }
    match crate::record_workbook_snapshot(state) {
        Ok(()) => {
            state
                .recovery_retry_revision
                .set(Some(state.worker_revision.get()));
            app.set_status_left("Retrying recovery…".into());
            true
        }
        Err(error) => {
            app.set_status_left(SharedString::from(format!(
                "Recovery retry could not start: {error}"
            )));
            false
        }
    }
}

/// Mirror the worker's recovery state into the UI and report a finished retry.
/// Runs on every worker tick.
pub(crate) fn sync(app: &SheetsApp, state: &GuiState) {
    let paused = paused_reason(state);
    if app.get_recovery_paused() != paused.is_some() {
        app.set_recovery_paused(paused.is_some());
        crate::sync_window_title(app, state);
        if let (Some(reason), None) = (&paused, state.recovery_retry_revision.get()) {
            app.set_status_left(SharedString::from(format!(
                "Recovery paused: {reason}. New edits are blocked until recovery works \
                 again. Choose Retry Recovery in the command palette, or Save As."
            )));
        }
    }
    let Some(revision) = state.recovery_retry_revision.get() else {
        return;
    };
    if state.applied_worker_result_revision.get() < revision {
        return;
    }
    state.recovery_retry_revision.set(None);
    app.set_status_left(SharedString::from(match paused {
        None => "Recovery restored. Your changes are protected again.".to_string(),
        Some(reason) => format!(
            "Recovery retry failed: {reason}. Free disk space and retry, or use Save As to keep your work."
        ),
    }));
}

pub(crate) fn wire(app: &SheetsApp, state: &Rc<GuiState>) {
    let app_ref = app.as_weak();
    let state = Rc::clone(state);
    app.on_retry_recovery(move || {
        if let Some(app) = app_ref.upgrade() {
            retry(&app, &state);
        }
    });
}
