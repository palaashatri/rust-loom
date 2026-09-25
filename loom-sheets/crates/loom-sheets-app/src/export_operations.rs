use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
#[cfg(test)]
use std::time::Duration;

use slint::SharedString;

use crate::{GuiState, SheetsApp};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExportFormat {
    Csv,
    Xlsx,
}

impl ExportFormat {
    fn label(self) -> &'static str {
        match self {
            Self::Csv => "CSV",
            Self::Xlsx => "Excel",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ExportOperation {
    pub(crate) operation_id: u64,
    pub(crate) document_generation: u64,
    pub(crate) target_revision: u64,
    pub(crate) format: ExportFormat,
    pub(crate) path: PathBuf,
    pub(crate) source_name: String,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ExportOutputSummary {
    Csv { sheet_name: String },
    Xlsx { sheet_count: usize },
}

pub(crate) struct ExportCompletion {
    pub(crate) completion_sequence: u64,
    pub(crate) operation: ExportOperation,
    pub(crate) result: Result<ExportOutputSummary, String>,
    #[cfg(test)]
    pub(crate) worker_duration: Duration,
}

struct ExportCompletionQueue {
    sender: Sender<ExportCompletion>,
    receiver: Receiver<ExportCompletion>,
}

impl ExportCompletionQueue {
    fn new() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self { sender, receiver }
    }

    fn sender(&self) -> Sender<ExportCompletion> {
        self.sender.clone()
    }

    fn drain(&self) -> Vec<ExportCompletion> {
        std::iter::from_fn(|| self.receiver.try_recv().ok()).collect()
    }
}

pub(crate) struct ExportOperations {
    next_operation_id: u64,
    accepted_operation_ids: HashSet<u64>,
    completions: ExportCompletionQueue,
}

impl Default for ExportOperations {
    fn default() -> Self {
        Self {
            next_operation_id: 0,
            accepted_operation_ids: HashSet::new(),
            completions: ExportCompletionQueue::new(),
        }
    }
}

impl ExportOperations {
    fn begin(
        &mut self,
        document_generation: u64,
        target_revision: u64,
        format: ExportFormat,
        path: PathBuf,
        source_name: String,
    ) -> ExportOperation {
        self.next_operation_id = self
            .next_operation_id
            .checked_add(1)
            .expect("Sheets Export operation id exhausted");
        ExportOperation {
            operation_id: self.next_operation_id,
            document_generation,
            target_revision,
            format,
            path,
            source_name,
        }
    }

    pub(crate) fn sender(&self) -> Sender<ExportCompletion> {
        self.completions.sender()
    }

    pub(crate) fn has_pending(&self) -> bool {
        !self.accepted_operation_ids.is_empty()
    }

    pub(crate) fn accept(&mut self, operation_id: u64) {
        self.accepted_operation_ids.insert(operation_id);
    }

    fn consume_accepted(&mut self, operation_id: u64) -> bool {
        self.accepted_operation_ids.remove(&operation_id)
    }

    pub(crate) fn drain(&mut self) -> Vec<(ExportCompletion, bool)> {
        self.completions
            .drain()
            .into_iter()
            .map(|completion| {
                let accepted = self.consume_accepted(completion.operation.operation_id);
                (completion, accepted)
            })
            .collect()
    }
}

pub(crate) fn export_with_picker(app: &SheetsApp, state: &GuiState, format: ExportFormat) {
    if crate::close_operations::reject_admission(app, state) {
        return;
    }
    if reject_unaccepted_worker_revision(app, state) {
        return;
    }
    if state.export_operations.borrow().has_pending() {
        app.set_status_left(
            "An Export is already in progress. Wait for it to finish before starting another."
                .into(),
        );
        return;
    }
    let request = match format {
        ExportFormat::Csv => super::export_request(state),
        ExportFormat::Xlsx => super::export_xlsx_request(state),
    };
    match state.dialogs.save_file(&request) {
        Ok(Some(path)) => queue_export(app, state, format, path),
        Ok(None) => app.set_status_left("Export cancelled".into()),
        Err(error) => {
            app.set_status_left(SharedString::from(format!("Export dialog failed: {error}")))
        }
    }
}

pub(crate) fn queue_export(app: &SheetsApp, state: &GuiState, format: ExportFormat, path: PathBuf) {
    if crate::close_operations::reject_admission(app, state)
        || reject_unaccepted_worker_revision(app, state)
    {
        return;
    }
    if state.export_operations.borrow().has_pending() {
        app.set_status_left(
            "An Export is already in progress. Wait for it to finish before starting another."
                .into(),
        );
        return;
    }
    let document_generation = state.open_operations.borrow().document_generation();
    let target_revision = state.last_queued_worker_revision.get();
    let source_name = super::workbook_display_name(state);
    let operation = state.export_operations.borrow_mut().begin(
        document_generation,
        target_revision,
        format,
        path,
        source_name,
    );
    let result = state
        .workbook_worker
        .borrow()
        .as_ref()
        .ok_or_else(|| "workbook worker is unavailable".to_string())
        .and_then(|worker| worker.queue_export(operation.clone()));
    match result {
        Ok(()) => {
            state
                .export_operations
                .borrow_mut()
                .accept(operation.operation_id);
            app.set_status_left(SharedString::from(format!(
                "Exporting {} from {} to {}…",
                operation.format.label(),
                operation.source_name,
                operation.path.display()
            )))
        }
        Err(error) => app.set_status_left(SharedString::from(format!(
            "{} export could not be queued for {} at {}: {error}",
            operation.format.label(),
            operation.source_name,
            operation.path.display()
        ))),
    }
}

fn reject_unaccepted_worker_revision(app: &SheetsApp, state: &GuiState) -> bool {
    let Some(message) = state.unaccepted_worker_revision_message() else {
        return false;
    };
    app.set_status_left(SharedString::from(format!(
        "Export unavailable because {message}"
    )));
    true
}

pub(super) fn handle_completion(
    completion: ExportCompletion,
    current_generation: u64,
    accepted: bool,
) -> crate::file_operation_completions::FileOperationStatus {
    let ExportCompletion {
        operation, result, ..
    } = completion;
    let unexpected_completion = if accepted {
        String::new()
    } else {
        format!(
            "Export completion {} had no matching accepted request. ",
            operation.operation_id
        )
    };
    let source_context = if operation.document_generation == current_generation {
        operation.source_name.clone()
    } else {
        format!(
            "previous workbook generation {} ({})",
            operation.document_generation, operation.source_name
        )
    };
    let (succeeded, message) = match result {
        Ok(ExportOutputSummary::Csv { sheet_name }) if operation.format == ExportFormat::Csv => (
            true,
            format!(
                "Exported CSV from {source_context} · {} (formulas preserved) to {}",
                sheet_name,
                operation.path.display()
            ),
        ),
        Ok(ExportOutputSummary::Xlsx { sheet_count }) if operation.format == ExportFormat::Xlsx => {
            (
                true,
                format!(
                    "Exported Excel from {source_context} · {} {} (formulas kept) to {}",
                    sheet_count,
                    if sheet_count == 1 { "sheet" } else { "sheets" },
                    operation.path.display()
                ),
            )
        }
        Ok(_) => (
            false,
            format!(
                "{} export failed for {} at {}: worker returned mismatched output metadata",
                operation.format.label(),
                operation.source_name,
                operation.path.display()
            ),
        ),
        Err(error) => (
            false,
            format!(
                "{} export failed for {source_context} at {}: {error}",
                operation.format.label(),
                operation.path.display()
            ),
        ),
    };
    let message = format!("{unexpected_completion}{message}");
    if succeeded && accepted {
        crate::file_operation_completions::FileOperationStatus::Success(message)
    } else {
        crate::file_operation_completions::FileOperationStatus::Failure(message)
    }
}
