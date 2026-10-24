//! Recovery lifecycle for replacing the visible presentation.

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use crate::{GuiState, PresentApp};

thread_local! {
    /// Keep the last service location so Retry can reopen that same store if
    /// clearing succeeded but publishing the reopened handle did not.
    static REOPEN_DIRECTORY: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

/// Checkpoint a candidate through the live recovery service. Warning acceptance
/// must fail closed instead of treating an absent recovery slot as success.
pub(crate) fn checkpoint_required_recovery(payload: Vec<u8>) -> Result<(), String> {
    crate::PRESENT_RECOVERY.with(|slot| {
        let mut slot = slot.borrow_mut();
        let recovery = slot
            .as_mut()
            .ok_or_else(|| "recovery service is unavailable".to_string())?;
        recovery
            .checkpoint("loom.present/1", payload)
            .map_err(|error| error.to_string())
    })
}

/// Checkpoint a Save As result when recovery was active for this app. A service
/// lost during a failed reset must be reopened instead of silently treating the
/// checkpoint as a no-op.
pub(crate) fn checkpoint_saved_deck(payload: Vec<u8>) -> Result<(), String> {
    if !recovery_reset_required() {
        return Ok(());
    }
    reconnect_recovery_service()?;
    crate::checkpoint_snapshot_recovery(payload)
}

/// Reconnect the recovery slot, preferring the exact directory whose reopen
/// previously failed. This runs on the UI thread along with other store writes.
pub(crate) fn reconnect_recovery_service() -> Result<(), String> {
    if crate::PRESENT_RECOVERY.with(|slot| slot.borrow().is_some()) {
        return Ok(());
    }

    if let Some(directory) = REOPEN_DIRECTORY.with(|directory| directory.borrow().clone()) {
        let recovery = open_recovery_service(&directory)?;
        crate::PRESENT_RECOVERY.with(|slot| *slot.borrow_mut() = Some(recovery));
        REOPEN_DIRECTORY.with(|directory| directory.borrow_mut().take());
        return Ok(());
    }

    crate::initialize_snapshot_recovery()?;
    crate::PRESENT_RECOVERY
        .with(|slot| slot.borrow().is_some())
        .then_some(())
        .ok_or_else(|| "recovery service could not be opened".to_string())
}

/// Empties the recovery store and opens it again, so this session records from an
/// empty store. The caller has written anything it must keep first.
pub(crate) fn reset_recovery_store() -> Result<(), String> {
    crate::recovery_deferred::invalidate();
    reconnect_recovery_service()?;
    let recovery = crate::PRESENT_RECOVERY
        .with(|slot| slot.borrow_mut().take())
        .ok_or_else(|| "recovery service could not be opened".to_string())?;
    let directory = recovery.directory().to_path_buf();

    let clear_result = clear_recovery_service(recovery);
    let reopen_result = open_recovery_service(&directory);

    match (clear_result, reopen_result) {
        (Ok(()), Ok(reopened)) => {
            crate::PRESENT_RECOVERY.with(|slot| *slot.borrow_mut() = Some(reopened));
            REOPEN_DIRECTORY.with(|directory| directory.borrow_mut().take());
            Ok(())
        }
        (Err(clear), Ok(reopened)) => {
            crate::PRESENT_RECOVERY.with(|slot| *slot.borrow_mut() = Some(reopened));
            REOPEN_DIRECTORY.with(|directory| directory.borrow_mut().take());
            Err(clear)
        }
        (Ok(()), Err(reopen)) => {
            REOPEN_DIRECTORY.with(|saved| *saved.borrow_mut() = Some(directory));
            Err(format!(
                "recovery store was cleared, but the recovery service could not be reopened: {reopen}"
            ))
        }
        (Err(clear), Err(reopen)) => {
            REOPEN_DIRECTORY.with(|saved| *saved.borrow_mut() = Some(directory));
            Err(format!(
                "{clear}; the recovery service could not be reopened: {reopen}"
            ))
        }
    }
}

/// Whether closing has recovery state that must be reset. Headless and focused
/// UI tests can have no initialized service; closing those sessions stays a
/// no-op, while a failed reopen still retains its exact directory for Retry.
pub(crate) fn recovery_reset_required() -> bool {
    crate::PRESENT_RECOVERY.with(|slot| slot.borrow().is_some())
        || REOPEN_DIRECTORY.with(|directory| directory.borrow().is_some())
}

/// Reset recovery before a real close and close the presenter only after the
/// recovery store is safe to discard. On failure, keep the deck available.
pub(crate) fn prepare_to_close(app: &PresentApp) -> bool {
    match reset_for_close() {
        Ok(()) => {
            crate::presenter::close();
            true
        }
        Err(error) => {
            let message = format!(
                "Could not close because recovery cleanup failed: {error}. The window remains open. Use Retry or Save As to preserve the current presentation, then close again."
            );
            crate::set_status(app, message.clone());
            crate::recovery_deferred::report_checkpoint_failure(app, message);
            false
        }
    }
}

pub(crate) fn reset_for_close() -> Result<(), String> {
    if crate::table_actions::PENDING_PPTX_IMPORT.with(|slot| slot.borrow().is_some()) {
        crate::recovery_deferred::invalidate();
        return Ok(());
    }
    if !recovery_reset_required() {
        crate::recovery_deferred::invalidate();
        return Ok(());
    }
    reset_recovery_store()
}

pub(crate) fn clear_recovery_before_replacement(app: &PresentApp, state: &GuiState) -> bool {
    match reset_recovery_store() {
        Ok(()) => true,
        Err(error) => {
            let preserve_result = {
                let session = state.session.borrow();
                let source = state.save_path.borrow().clone();
                crate::recovery_draft::payload(&session, source.as_deref())
                    .and_then(checkpoint_required_recovery)
            };
            let message = match preserve_result {
                Ok(()) => format!(
                    "Could not reset recovery data: {error}. The current presentation was saved to recovery; retry the replacement to try again."
                ),
                Err(preserve_error) => {
                    let message = format!(
                        "Could not reset recovery data: {error}. The current presentation could not be saved to recovery: {preserve_error}. Use Retry or Save As to preserve it, then reissue the replacement."
                    );
                    crate::set_status(app, message.clone());
                    crate::recovery_deferred::report_checkpoint_failure(app, message.clone());
                    return false;
                }
            };
            crate::set_status(app, message);
            false
        }
    }
}

fn open_recovery_service(
    directory: &Path,
) -> Result<loom_production::snapshot::SnapshotRecovery, String> {
    #[cfg(test)]
    if let Some(error) = NEXT_RECOVERY_REOPEN_FAILURE.with(|failure| failure.borrow_mut().take()) {
        return Err(error);
    }

    loom_production::snapshot::SnapshotRecovery::open_at(directory)
        .map_err(|error| error.to_string())
}

pub(crate) fn clear_recovery_service(
    recovery: loom_production::snapshot::SnapshotRecovery,
) -> Result<(), String> {
    #[cfg(test)]
    if let Some(error) = NEXT_RECOVERY_CLEAR_FAILURE.with(|failure| failure.borrow_mut().take()) {
        return Err(error);
    }

    recovery.clear().map_err(|error| error.to_string())
}

#[cfg(test)]
thread_local! {
    static NEXT_RECOVERY_CLEAR_FAILURE: RefCell<Option<String>> = const { RefCell::new(None) };
    static NEXT_RECOVERY_REOPEN_FAILURE: RefCell<Option<String>> = const { RefCell::new(None) };
}

#[cfg(test)]
pub(crate) struct RecoveryClearFailureGuard;

#[cfg(test)]
impl Drop for RecoveryClearFailureGuard {
    fn drop(&mut self) {
        NEXT_RECOVERY_CLEAR_FAILURE.with(|failure| failure.borrow_mut().take());
    }
}

#[cfg(test)]
pub(crate) fn inject_recovery_clear_failure_for_test(
    message: impl Into<String>,
) -> RecoveryClearFailureGuard {
    NEXT_RECOVERY_CLEAR_FAILURE.with(|failure| *failure.borrow_mut() = Some(message.into()));
    RecoveryClearFailureGuard
}

#[cfg(test)]
pub(crate) struct RecoveryReopenFailureGuard;

#[cfg(test)]
impl Drop for RecoveryReopenFailureGuard {
    fn drop(&mut self) {
        NEXT_RECOVERY_REOPEN_FAILURE.with(|failure| failure.borrow_mut().take());
    }
}

#[cfg(test)]
pub(crate) fn inject_recovery_reopen_failure_for_test(
    message: impl Into<String>,
) -> RecoveryReopenFailureGuard {
    NEXT_RECOVERY_REOPEN_FAILURE.with(|failure| *failure.borrow_mut() = Some(message.into()));
    RecoveryReopenFailureGuard
}

#[cfg(test)]
pub(crate) fn forget_recovery_directory_for_test() {
    REOPEN_DIRECTORY.with(|directory| directory.borrow_mut().take());
}
