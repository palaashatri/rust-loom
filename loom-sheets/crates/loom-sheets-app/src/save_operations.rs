use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;

use crate::open_operations::continue_pending_replacement_after_dialog;
use crate::{GuiState, SheetsApp};
use loom_sheets_core::Sheet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SaveOperation {
    pub(crate) operation_id: u64,
    pub(crate) document_generation: u64,
    pub(crate) target_revision: u64,
    pub(crate) pending_replacement_token: Option<u64>,
}

#[derive(Default)]
pub(crate) struct SaveOperationCoordinator {
    next_operation_id: u64,
    active: Option<SaveOperation>,
}

impl SaveOperationCoordinator {
    pub(crate) fn begin(
        &mut self,
        document_generation: u64,
        target_revision: u64,
        pending_replacement_token: Option<u64>,
    ) -> Result<SaveOperation, String> {
        if self.active.is_some() {
            return Err("a Save operation is already in progress".into());
        }
        self.next_operation_id = self
            .next_operation_id
            .checked_add(1)
            .expect("Sheets Save operation id exhausted");
        let operation = SaveOperation {
            operation_id: self.next_operation_id,
            document_generation,
            target_revision,
            pending_replacement_token,
        };
        self.active = Some(operation);
        Ok(operation)
    }

    pub(crate) fn is_current(&self, operation: SaveOperation, current_generation: u64) -> bool {
        self.active.is_some_and(|active| {
            active.operation_id == operation.operation_id
                && active.document_generation == operation.document_generation
                && operation.document_generation == current_generation
        })
    }

    pub(crate) fn clear(&mut self, operation: SaveOperation) {
        if self
            .active
            .is_some_and(|a| a.operation_id == operation.operation_id)
        {
            self.active = None;
        }
    }

    pub(crate) fn is_active(&self) -> bool {
        self.active.is_some()
    }
}

pub(crate) struct SaveCompletion {
    pub(crate) completion_sequence: u64,
    pub(crate) operation: SaveOperation,
    pub(crate) path: PathBuf,
    pub(crate) write_result: Result<(), String>,
    pub(crate) checkpoint_result: Option<Result<(), String>>,
    pub(crate) baseline: Option<(Vec<Sheet>, usize)>,
}

pub(crate) struct SaveCompletionQueue {
    sender: Sender<SaveCompletion>,
    receiver: Receiver<SaveCompletion>,
}

impl Default for SaveCompletionQueue {
    fn default() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self { sender, receiver }
    }
}

impl SaveCompletionQueue {
    pub(crate) fn sender(&self) -> Sender<SaveCompletion> {
        self.sender.clone()
    }

    pub(crate) fn try_receive(&self) -> Option<SaveCompletion> {
        self.receiver.try_recv().ok()
    }
}

#[derive(Default)]
pub(crate) struct SaveOperations {
    coordinator: SaveOperationCoordinator,
    completions: SaveCompletionQueue,
}

impl SaveOperations {
    pub(crate) fn begin_operation(
        &mut self,
        document_generation: u64,
        target_revision: u64,
        pending_replacement_token: Option<u64>,
    ) -> Result<SaveOperation, String> {
        self.coordinator.begin(
            document_generation,
            target_revision,
            pending_replacement_token,
        )
    }

    pub(crate) fn is_current(&self, operation: SaveOperation, current_generation: u64) -> bool {
        self.coordinator.is_current(operation, current_generation)
    }

    pub(crate) fn clear(&mut self, operation: SaveOperation) {
        self.coordinator.clear(operation);
    }

    pub(crate) fn is_active(&self) -> bool {
        self.coordinator.is_active()
    }

    pub(crate) fn drain(&self) -> Vec<SaveCompletion> {
        std::iter::from_fn(|| self.completions.try_receive()).collect()
    }

    pub(crate) fn sender(&self) -> Sender<SaveCompletion> {
        self.completions.sender()
    }
}

#[cfg(test)]
pub(super) fn process_completions(
    app: &SheetsApp,
    state: &GuiState,
    menu_service: &Arc<loom_desktop::NativeMenuBar>,
) -> usize {
    let completions = state.save_operations.borrow().drain();
    let count = completions.len();
    let mut outcomes = Vec::new();
    for completion in completions {
        let current_generation = state.open_operations.borrow().document_generation();
        if let Some(outcome) =
            handle_completion(app, state, menu_service, completion, current_generation)
        {
            outcomes.push(outcome);
        }
    }
    crate::file_operation_completions::publish_status(app, &outcomes);
    count
}

pub(super) fn handle_completion(
    app: &SheetsApp,
    state: &GuiState,
    menu_service: &Arc<loom_desktop::NativeMenuBar>,
    completion: SaveCompletion,
    current_generation: u64,
) -> Option<crate::file_operation_completions::FileOperationStatus> {
    if completion.write_result.is_ok() {
        state
            .open_operations
            .borrow_mut()
            .note_successful_native_write();
    }
    let is_current = state
        .save_operations
        .borrow()
        .is_current(completion.operation, current_generation);
    state
        .save_operations
        .borrow_mut()
        .clear(completion.operation);
    if !is_current {
        return None;
    }

    let SaveCompletion {
        operation,
        path,
        write_result,
        checkpoint_result,
        baseline,
        ..
    } = completion;

    match write_result {
        Ok(()) => {
            *state.save_path.borrow_mut() = Some(path.clone());
            if let Some(baseline) = baseline {
                *state.last_saved.borrow_mut() = Some(baseline);
                state
                    .worker_saved_baseline_generation
                    .set(Some(operation.document_generation));
                state
                    .worker_saved_baseline_revision
                    .set(Some(operation.target_revision));
                if state.worker_revision.get() == operation.target_revision {
                    state.clear_dirty();
                } else {
                    // The live state may have been undone against the previous
                    // baseline while this save was running. Keep it protected
                    // until a worker result compares it with the new baseline.
                    state.mark_content_dirty();
                }
                crate::sync_window_title(app, state);
            }

            let status_succeeded = matches!(&checkpoint_result, Some(Ok(())));
            let save_status = match checkpoint_result {
                Some(Ok(())) => format!("Saved {}", path.display()),
                Some(Err(error)) => format!(
                    "Saved {}, but recovery checkpoint failed: {error}",
                    path.display()
                ),
                None => format!(
                    "Saved {}, but recovery checkpoint was not completed",
                    path.display()
                ),
            };
            let pending_replacement_is_current =
                operation.pending_replacement_token.is_some_and(|token| {
                    token == state.pending_replacement_token.get()
                        && state.pending_replacement.get().is_some()
                        && app.get_save_changes_open()
                });
            let unsaved_changes_remain =
                state.is_dirty() || crate::open_operations::has_formula_draft(app);
            let status = if pending_replacement_is_current && unsaved_changes_remain {
                format!(
                    "{save_status}. Newer edits remain unsaved; recovery may still be catching up. Choose Save, Discard, or Cancel."
                )
            } else if unsaved_changes_remain {
                format!("{save_status}; newer edits remain unsaved and recovery may still be catching up")
            } else {
                save_status
            };
            if pending_replacement_is_current && !unsaved_changes_remain {
                app.set_save_changes_open(false);
                continue_pending_replacement_after_dialog(app, state, menu_service);
            }
            Some(if status_succeeded {
                crate::file_operation_completions::FileOperationStatus::Success(status)
            } else {
                crate::file_operation_completions::FileOperationStatus::Failure(status)
            })
        }
        Err(error) => Some(
            crate::file_operation_completions::FileOperationStatus::Failure(format!(
                "Save failed: {error} at {}",
                path.display()
            )),
        ),
    }
}
