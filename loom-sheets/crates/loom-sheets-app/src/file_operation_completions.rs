use std::collections::BTreeMap;
use std::sync::Arc;

use slint::SharedString;

use crate::save_operations::SaveCompletion;
use crate::{export_operations::ExportCompletion, GuiState, SheetsApp};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FileOperationStatus {
    Success(String),
    Failure(String),
}

pub(crate) struct CompletionDrain {
    #[cfg(test)]
    pub(crate) count: usize,
    pub(crate) outcomes: Vec<FileOperationStatus>,
    pub(crate) save_outcomes: Vec<FileOperationStatus>,
    pub(crate) successful_save_revisions: Vec<u64>,
}

enum WorkerFileCompletion {
    Save(SaveCompletion),
    Export(ExportCompletion, bool),
}

impl WorkerFileCompletion {
    fn sequence(&self) -> u64 {
        match self {
            Self::Save(completion) => completion.completion_sequence,
            Self::Export(completion, _) => completion.completion_sequence,
        }
    }
}

#[derive(Default)]
pub(crate) struct FileOperationCompletions {
    next_sequence: u64,
    pending: BTreeMap<u64, Vec<WorkerFileCompletion>>,
}

impl FileOperationCompletions {
    fn enqueue(&mut self, completion: WorkerFileCompletion) {
        self.pending
            .entry(completion.sequence())
            .or_default()
            .push(completion);
    }

    fn take_ready(
        &mut self,
        saves: Vec<SaveCompletion>,
        exports: Vec<(ExportCompletion, bool)>,
    ) -> Vec<WorkerFileCompletion> {
        for completion in saves {
            self.enqueue(WorkerFileCompletion::Save(completion));
        }
        for (completion, accepted) in exports {
            self.enqueue(WorkerFileCompletion::Export(completion, accepted));
        }

        let mut ready = Vec::new();
        loop {
            let next_sequence = self
                .next_sequence
                .checked_add(1)
                .expect("Sheets file completion sequence exhausted");
            let Some(mut completions) = self.pending.remove(&next_sequence) else {
                break;
            };
            ready.append(&mut completions);
            self.next_sequence = next_sequence;
        }
        ready
    }
}

#[cfg(test)]
pub(super) fn process_completions(
    app: &SheetsApp,
    state: &GuiState,
    menu_service: &Arc<loom_desktop::NativeMenuBar>,
) -> usize {
    drain_completions(app, state, menu_service).count
}

pub(super) fn drain_completions(
    app: &SheetsApp,
    state: &GuiState,
    menu_service: &Arc<loom_desktop::NativeMenuBar>,
) -> CompletionDrain {
    let saves = state.save_operations.borrow().drain();
    let exports = state.export_operations.borrow_mut().drain();
    #[cfg(test)]
    let count = saves.len() + exports.len();
    let ready = state
        .file_operation_completions
        .borrow_mut()
        .take_ready(saves, exports);

    let mut outcomes = Vec::new();
    let mut save_outcomes = Vec::new();
    let mut successful_save_revisions = Vec::new();
    for completion in ready {
        let current_generation = state.open_operations.borrow().document_generation();
        let outcome = match completion {
            WorkerFileCompletion::Save(completion) => {
                let target_revision = completion.operation.target_revision;
                let outcome = crate::save_operations::handle_completion(
                    app,
                    state,
                    menu_service,
                    completion,
                    current_generation,
                );
                if let Some(outcome) = outcome.as_ref() {
                    if matches!(outcome, FileOperationStatus::Success(_)) {
                        successful_save_revisions.push(target_revision);
                    }
                    save_outcomes.push(outcome.clone());
                }
                outcome
            }
            WorkerFileCompletion::Export(completion, accepted) => {
                Some(crate::export_operations::handle_completion(
                    completion,
                    current_generation,
                    accepted,
                ))
            }
        };
        if let Some(outcome) = outcome {
            outcomes.push(outcome);
        }
    }
    publish_status(app, &outcomes);
    CompletionDrain {
        #[cfg(test)]
        count,
        outcomes,
        save_outcomes,
        successful_save_revisions,
    }
}

pub(super) fn publish_status(app: &SheetsApp, outcomes: &[FileOperationStatus]) {
    if let Some(message) = combined_message(outcomes) {
        app.set_status_left(SharedString::from(message));
    }
}

pub(super) fn combined_message(outcomes: &[FileOperationStatus]) -> Option<String> {
    let messages = outcomes
        .iter()
        .map(|outcome| match outcome {
            FileOperationStatus::Success(message) | FileOperationStatus::Failure(message) => {
                message.as_str()
            }
        })
        .collect::<Vec<_>>();
    (!messages.is_empty()).then(|| messages.join(" · "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combined_status_keeps_every_outcome_in_order() {
        let outcomes = [
            FileOperationStatus::Failure("Export failed at one.csv: disk full".into()),
            FileOperationStatus::Success("Saved one.loomtable".into()),
            FileOperationStatus::Failure("Save failed: permission denied".into()),
            FileOperationStatus::Success("Exported second.csv".into()),
        ];
        assert_eq!(
            combined_message(&outcomes).as_deref(),
            Some(
                "Export failed at one.csv: disk full · Saved one.loomtable · Save failed: permission denied · Exported second.csv"
            )
        );
    }
}
