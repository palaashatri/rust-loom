use crate::GuiState;

#[derive(Clone)]
pub(crate) struct WorkerInputFailure {
    document_generation: u64,
    revision: u64,
    error: String,
}

pub(crate) fn mark_full_resync_accepted(state: &GuiState, document_generation: u64, revision: u64) {
    state
        .worker_full_resync_revision
        .set(Some((document_generation, revision)));
}

pub(crate) fn record_input_failure(
    state: &GuiState,
    document_generation: u64,
    revision: u64,
    error: String,
) {
    let mut pending = state.worker_input_failure.borrow_mut();
    if pending.as_ref().is_some_and(|failure| {
        failure.document_generation == document_generation && failure.revision <= revision
    }) {
        return;
    }
    *pending = Some(WorkerInputFailure {
        document_generation,
        revision,
        error,
    });
}

pub(crate) fn sync_pending_worker_input_failure(state: &GuiState) -> Result<(), String> {
    let pending = {
        let worker = state
            .workbook_worker
            .try_borrow()
            .map_err(|_| "workbook worker state is currently unavailable".to_string())?;
        match worker.as_ref() {
            Some(worker) => worker.pending_input_failure()?,
            None => None,
        }
    };
    if let Some(failure) = pending {
        let generation = state.open_operations.borrow().document_generation();
        let already_recorded =
            state
                .worker_input_failure
                .borrow()
                .as_ref()
                .is_some_and(|recorded| {
                    recorded.revision == failure.revision && recorded.error == failure.error
                });
        if !already_recorded {
            record_input_failure(state, generation, failure.revision, failure.error);
        }
    }
    Ok(())
}

pub(crate) fn clear_after_successful_resync(
    state: &GuiState,
    document_generation: u64,
    result_revision: u64,
) -> bool {
    let Some(failure) = state.worker_input_failure.borrow().as_ref().cloned() else {
        return false;
    };
    let Some((resync_generation, resync_revision)) = state.worker_full_resync_revision.get() else {
        return false;
    };
    if resync_generation != document_generation
        || result_revision < resync_revision
        || (failure.document_generation == document_generation
            && resync_revision < failure.revision)
    {
        return false;
    }
    state.worker_input_failure.borrow_mut().take();
    true
}

pub(crate) fn has_current_failure(state: &GuiState) -> bool {
    if sync_pending_worker_input_failure(state).is_err() {
        return true;
    }
    current_failure(state).is_some()
}

pub(crate) fn status_message(state: &GuiState) -> Option<String> {
    if let Err(error) = sync_pending_worker_input_failure(state) {
        return Some(format!(
            "Calculation worker status is unavailable: {error}. Saving, exporting, and closing are blocked."
        ));
    }
    current_failure(state).map(|failure| {
        format!(
            "Calculation worker rejected workbook revision {}: {}. Saving, exporting, and closing are blocked until a full workbook replacement succeeds.",
            failure.revision, failure.error
        )
    })
}

pub(crate) fn admission_message(state: &GuiState) -> Option<String> {
    current_failure(state).map(|failure| {
        format!(
            "workbook revision {} failed in the calculation worker: {}; a successful full workbook replacement is required",
            failure.revision, failure.error
        )
    })
}

impl GuiState {
    pub(crate) fn unaccepted_worker_revision_message(&self) -> Option<String> {
        let mut messages = Vec::new();
        if let Err(error) = sync_pending_worker_input_failure(self) {
            messages.push(format!(
                "calculation worker input status is unavailable: {error}"
            ));
        }
        if let Some((revision, error)) = self.worker_submission_failure.borrow().clone() {
            messages.push(format!(
                "workbook revision {revision} was not accepted by the calculation worker: {error}"
            ));
        }
        if let Some(message) = admission_message(self) {
            messages.push(message);
        }
        let allocated = self.worker_revision.get();
        let accepted = self.last_queued_worker_revision.get();
        if allocated > accepted {
            messages.push(format!(
                "workbook revision {allocated} was not accepted by the calculation worker"
            ));
        }
        (!messages.is_empty()).then(|| messages.join("; "))
    }
}

fn current_failure(state: &GuiState) -> Option<WorkerInputFailure> {
    let generation = state.open_operations.borrow().document_generation();
    state
        .worker_input_failure
        .borrow()
        .as_ref()
        .filter(|failure| failure.document_generation == generation)
        .cloned()
}
