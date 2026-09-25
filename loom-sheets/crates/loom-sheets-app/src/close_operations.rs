use std::rc::Rc;

use slint::{CloseRequestResponse, ComponentHandle, SharedString};

use crate::file_operation_completions::{CompletionDrain, FileOperationStatus};
use crate::{GuiState, SheetsApp};

const DIRTY_CLOSE_STATUS: &str = "Unsaved changes — choose Save or Cancel before closing.";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum CloseState {
    #[default]
    Idle,
    DirtyDecision,
    SavingForClose,
    Draining {
        target_revision: u64,
    },
}

pub(crate) fn wire_window_close_handler(app: &SheetsApp, state: &Rc<GuiState>) {
    let app_ref = app.as_weak();
    let state = Rc::clone(state);
    app.window().on_close_requested(move || {
        let Some(app) = app_ref.upgrade() else {
            return CloseRequestResponse::HideWindow;
        };
        request_close(&app, &state)
    });
}

pub(crate) fn wire_save_changes_callbacks(
    app: &SheetsApp,
    state: &Rc<GuiState>,
    menu_service: &std::sync::Arc<loom_desktop::NativeMenuBar>,
) {
    let state_ref = Rc::clone(state);
    let app_ref = app.as_weak();
    let menu_service_for_save = std::sync::Arc::clone(menu_service);
    app.on_save_changes_save(move || {
        if let Some(app) = app_ref.upgrade() {
            let result = if state_ref.close_state.get() == CloseState::DirtyDecision {
                save_for_close(&app, &state_ref)
            } else {
                crate::open_operations::save_changes_and_resume(
                    &app,
                    &state_ref,
                    &menu_service_for_save,
                )
            };
            if let Err(error) = result {
                app.set_status_left(SharedString::from(format!("Save failed: {error}")));
            }
        }
    });

    let state_ref = Rc::clone(state);
    let app_ref = app.as_weak();
    let menu_service_for_discard = std::sync::Arc::clone(menu_service);
    app.on_save_changes_discard(move || {
        if let Some(app) = app_ref.upgrade() {
            if state_ref.close_state.get() == CloseState::DirtyDecision {
                discard_for_close(&app, &state_ref);
            } else {
                crate::open_operations::discard_changes_and_resume(
                    &app,
                    &state_ref,
                    &menu_service_for_discard,
                );
            }
        }
    });

    let state_ref = Rc::clone(state);
    let app_ref = app.as_weak();
    app.on_save_changes_cancel(move || {
        if let Some(app) = app_ref.upgrade() {
            if matches!(
                state_ref.close_state.get(),
                CloseState::DirtyDecision | CloseState::SavingForClose
            ) {
                cancel_close(&app, &state_ref);
            } else {
                crate::open_operations::cancel_save_changes_dialog(&app, &state_ref);
            }
        }
    });
}

pub(crate) fn process_worker_tick(
    app: &SheetsApp,
    state: &GuiState,
    menu_service: &std::sync::Arc<loom_desktop::NativeMenuBar>,
) {
    let completions =
        crate::file_operation_completions::drain_completions(app, state, menu_service);
    let result = state
        .workbook_worker
        .borrow()
        .as_ref()
        .and_then(crate::workbook_worker::WorkbookWorker::take_latest_result);
    let mut worker_failures = result
        .as_ref()
        .and_then(|result| relevant_worker_failure(state, &completions, result));
    if let Some(failure) = deferred_recovery_failure(state, &completions) {
        worker_failures = Some(match worker_failures {
            Some(existing) => format!("{existing}; {failure}"),
            None => failure,
        });
    }
    if let Some(result) = result {
        crate::apply_workbook_worker_result(app, state, result);
    }
    process_tick(app, state, &completions, worker_failures.as_deref());
    preserve_worker_input_failure_status(app, state, &completions);
}

pub(crate) fn blocks_admission(state: &GuiState) -> bool {
    state.close_state.get() != CloseState::Idle
}

pub(crate) fn reject_admission(app: &SheetsApp, state: &GuiState) -> bool {
    if !blocks_admission(state) {
        return false;
    }
    app.set_status_left("Finishing accepted work before closing. Wait for close to finish.".into());
    true
}

fn preserve_worker_input_failure_status(
    app: &SheetsApp,
    state: &GuiState,
    completions: &CompletionDrain,
) {
    let Some(failure_status) = crate::worker_failure::status_message(state) else {
        return;
    };
    let file_status = crate::file_operation_completions::combined_message(&completions.outcomes);
    let current_status = app.get_status_left().to_string();
    if file_status.is_none()
        && (current_status.contains(&failure_status)
            || current_status.contains("Workbook work failed before closing"))
    {
        return;
    }

    let mut parts = Vec::new();
    if let Some(file_status) = file_status {
        if !current_status.contains(&file_status) {
            parts.push(file_status);
        }
    }
    if current_status.contains("Workbook work failed before closing") {
        if !parts.iter().any(|part| part == &current_status) {
            parts.push(current_status);
        }
    } else if state.close_state.get() == CloseState::DirtyDecision {
        parts.push(DIRTY_CLOSE_STATUS.to_string());
    }
    if !parts.iter().any(|part| part.contains(&failure_status)) {
        parts.push(failure_status);
    }
    app.set_status_left(SharedString::from(parts.join(" · ")));
}

fn set_state(app: &SheetsApp, state: &GuiState, close_state: CloseState) {
    if close_state != CloseState::SavingForClose {
        state.deferred_close_recovery_error.borrow_mut().take();
    }
    state.close_state.set(close_state);
    app.set_close_draining(close_state != CloseState::Idle);
}

fn show_dirty_decision(app: &SheetsApp, state: &GuiState) {
    set_state(app, state, CloseState::DirtyDecision);
    app.set_save_changes_close_mode(true);
    app.set_save_changes_document(SharedString::from(crate::workbook_display_name(state)));
    app.set_save_changes_open(true);
    app.set_status_left(DIRTY_CLOSE_STATUS.into());
}

fn relevant_worker_failure(
    state: &GuiState,
    completions: &CompletionDrain,
    result: &crate::workbook_worker::WorkbookResult,
) -> Option<String> {
    let close_state = state.close_state.get();
    let target_revision = match close_state {
        CloseState::SavingForClose => state.last_queued_worker_revision.get(),
        CloseState::Draining { target_revision } => target_revision,
        CloseState::Idle | CloseState::DirtyDecision => return None,
    };
    if result.revision < target_revision {
        return None;
    }

    let input_failure = result.input_error.as_deref();
    let matching_checkpoint_succeeded = result.revision == target_revision
        && completions
            .successful_save_revisions
            .contains(&target_revision);
    let recovery_failure = result.recovery_error.as_deref().and_then(|error| {
        if matching_checkpoint_succeeded {
            return None;
        }
        if close_state == CloseState::SavingForClose
            && result.revision == target_revision
            && input_failure.is_none()
            && state.save_operations.borrow().is_active()
        {
            *state.deferred_close_recovery_error.borrow_mut() =
                Some((target_revision, error.to_string()));
            None
        } else {
            Some(error)
        }
    });

    [input_failure, recovery_failure]
        .into_iter()
        .flatten()
        .map(str::to_owned)
        .reduce(|combined, error| format!("{combined}; {error}"))
}

fn deferred_recovery_failure(state: &GuiState, completions: &CompletionDrain) -> Option<String> {
    let mut deferred = state.deferred_close_recovery_error.borrow_mut();
    let revision = deferred.as_ref()?.0;
    if completions.successful_save_revisions.contains(&revision) {
        deferred.take();
        return None;
    }
    if !state.save_operations.borrow().is_active() {
        let (revision, error) = deferred.take().expect("deferred recovery error exists");
        return Some(format!(
            "recovery checkpoint unavailable at workbook revision {revision}: {error}"
        ));
    }
    None
}

fn request_close(app: &SheetsApp, state: &GuiState) -> CloseRequestResponse {
    if state.close_state.get() != CloseState::Idle {
        return CloseRequestResponse::KeepWindowShown;
    }
    if app.get_save_changes_open() || app.get_xlsx_import_warning_open() {
        app.set_status_left("Resolve the open workbook dialog before closing.".into());
        return CloseRequestResponse::KeepWindowShown;
    }
    if state.open_operations.borrow().has_active_operation() {
        app.set_status_left(
            "Workbook Open is still processing. Wait for its result, then close again.".into(),
        );
        return CloseRequestResponse::KeepWindowShown;
    }
    if let Some(message) = state.unaccepted_worker_revision_message() {
        app.set_status_left(SharedString::from(format!(
            "Cannot close because {message}. Keep the workbook open and retry the edit."
        )));
        return CloseRequestResponse::KeepWindowShown;
    }
    if state.save_operations.borrow().is_active()
        || state.export_operations.borrow().has_pending()
        || state.applied_worker_result_revision.get() < state.last_queued_worker_revision.get()
    {
        let target_revision = state.last_queued_worker_revision.get();
        set_state(app, state, CloseState::Draining { target_revision });
        app.set_status_left("Finishing accepted workbook work before closing…".into());
        return CloseRequestResponse::KeepWindowShown;
    }
    if state.is_dirty() || crate::open_operations::has_formula_draft(app) {
        show_dirty_decision(app, state);
        return CloseRequestResponse::KeepWindowShown;
    }
    CloseRequestResponse::HideWindow
}

pub(crate) fn save_for_close(app: &SheetsApp, state: &GuiState) -> Result<bool, String> {
    if state.close_state.get() != CloseState::DirtyDecision {
        return Err("close is no longer waiting for a Save choice".into());
    }

    // Formula-bar text is an edit only when the user accepts Save. Let this
    // synchronous commit enter the same worker FIFO before its Save barrier.
    set_state(app, state, CloseState::Idle);
    match crate::save_current_sheet(app, state, false) {
        Ok(true) => {
            set_state(app, state, CloseState::SavingForClose);
            app.set_status_left("Saving workbook and recovery checkpoint before closing…".into());
            Ok(true)
        }
        Ok(false) => {
            show_dirty_decision(app, state);
            Ok(false)
        }
        Err(error) => {
            show_dirty_decision(app, state);
            Err(error)
        }
    }
}

pub(crate) fn cancel_close(app: &SheetsApp, state: &GuiState) {
    if !matches!(
        state.close_state.get(),
        CloseState::DirtyDecision | CloseState::SavingForClose
    ) {
        return;
    }
    app.set_save_changes_open(false);
    app.set_save_changes_close_mode(false);
    set_state(app, state, CloseState::Idle);
    app.set_status_left("Close cancelled; the workbook remains open.".into());
}

pub(crate) fn discard_for_close(app: &SheetsApp, state: &GuiState) {
    if state.close_state.get() == CloseState::DirtyDecision {
        app.set_status_left("Discard is unavailable when closing; choose Save or Cancel.".into());
    }
}

pub(crate) fn process_tick(
    app: &SheetsApp,
    state: &GuiState,
    completions: &CompletionDrain,
    worker_failure: Option<&str>,
) {
    let close_state = state.close_state.get();
    if close_state == CloseState::Idle || close_state == CloseState::DirtyDecision {
        return;
    }

    let operation_failed = completions
        .outcomes
        .iter()
        .any(|outcome| matches!(outcome, FileOperationStatus::Failure(_)));
    if let Some(error) = worker_failure {
        publish_close_status(
            app,
            &format!("Workbook work failed before closing: {error}"),
            &completions.outcomes,
        );
        fail_close(app, state, close_state);
        return;
    }
    if operation_failed {
        fail_close(app, state, close_state);
        return;
    }

    if close_state == CloseState::SavingForClose {
        if !completions.save_outcomes.is_empty() {
            let save_succeeded = completions
                .save_outcomes
                .iter()
                .all(|outcome| matches!(outcome, FileOperationStatus::Success(_)));
            if !save_succeeded {
                fail_close(app, state, close_state);
                return;
            }
            app.set_save_changes_open(false);
            app.set_save_changes_close_mode(false);
            set_state(
                app,
                state,
                CloseState::Draining {
                    target_revision: state.last_queued_worker_revision.get(),
                },
            );
        } else {
            return;
        }
    }

    let CloseState::Draining { target_revision } = state.close_state.get() else {
        return;
    };
    if state.applied_worker_result_revision.get() < target_revision
        || state.save_operations.borrow().is_active()
        || state.export_operations.borrow().has_pending()
    {
        return;
    }

    if state.is_dirty() || crate::open_operations::has_formula_draft(app) {
        show_dirty_decision(app, state);
        if !completions.outcomes.is_empty() {
            publish_close_status(app, DIRTY_CLOSE_STATUS, &completions.outcomes);
        }
        return;
    }

    set_state(app, state, CloseState::Idle);
    match app.window().hide() {
        Ok(()) => {}
        Err(error) => app.set_status_left(SharedString::from(format!(
            "Close failed: the window could not be hidden: {error}"
        ))),
    }
}

fn publish_close_status(app: &SheetsApp, close_message: &str, outcomes: &[FileOperationStatus]) {
    let status = crate::file_operation_completions::combined_message(outcomes)
        .map(|file_status| format!("{file_status} · {close_message}"))
        .unwrap_or_else(|| close_message.to_string());
    app.set_status_left(SharedString::from(status));
}

fn fail_close(app: &SheetsApp, state: &GuiState, close_state: CloseState) {
    if close_state == CloseState::SavingForClose {
        set_state(app, state, CloseState::DirtyDecision);
        app.set_save_changes_close_mode(true);
        app.set_save_changes_open(true);
    } else {
        app.set_save_changes_close_mode(false);
        set_state(app, state, CloseState::Idle);
    }
}
