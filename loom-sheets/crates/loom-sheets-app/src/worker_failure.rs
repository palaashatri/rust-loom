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
    *state.worker_input_failure.borrow_mut() = Some(WorkerInputFailure {
        document_generation,
        revision,
        error,
    });
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
    if failure.document_generation != document_generation
        || resync_generation != document_generation
        || resync_revision < failure.revision
        || result_revision < resync_revision
    {
        return false;
    }
    state.worker_input_failure.borrow_mut().take();
    true
}

pub(crate) fn has_current_failure(state: &GuiState) -> bool {
    current_failure(state).is_some()
}

pub(crate) fn status_message(state: &GuiState) -> Option<String> {
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

fn current_failure(state: &GuiState) -> Option<WorkerInputFailure> {
    let generation = state.open_operations.borrow().document_generation();
    state
        .worker_input_failure
        .borrow()
        .as_ref()
        .filter(|failure| failure.document_generation == generation)
        .cloned()
}
