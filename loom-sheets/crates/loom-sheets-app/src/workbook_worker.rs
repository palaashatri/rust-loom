use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{self, SyncSender};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
#[cfg(test)]
use std::time::Duration;
#[cfg(test)]
use std::time::Instant;

use loom_sheets_core::workbook::evaluate_workbook;
use loom_sheets_core::{workbook_to_json, CellRef, Sheet, Value};

use crate::cell_edit_recovery::CellEditRecovery;
use crate::export_operations::{
    ExportCompletion, ExportFormat, ExportOperation, ExportOutputSummary,
};
use crate::save_operations::SaveCompletion;
use crate::workbook_io::workbook_package_bytes;

#[derive(Debug, Clone)]
pub(crate) struct CellUpdate {
    pub(crate) revision: u64,
    pub(crate) active_sheet: usize,
    pub(crate) sheet: usize,
    pub(crate) cell: CellRef,
    pub(crate) raw: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct WorkbookResult {
    pub(crate) revision: u64,
    pub(crate) active_sheet: usize,
    pub(crate) values: HashMap<CellRef, Value>,
    pub(crate) recovery_error: Option<String>,
    pub(crate) input_error: Option<String>,
    pub(crate) dirty: bool,
    #[cfg(test)]
    pub(crate) update_kind: WorkerUpdateKind,
    #[cfg(test)]
    pub(crate) cell_updates: usize,
    #[cfg(test)]
    pub(crate) evaluation_duration: Duration,
    #[cfg(test)]
    pub(crate) recovery_package_duration: Duration,
    #[cfg(test)]
    pub(crate) recovery_journal_duration: Duration,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkerUpdateKind {
    InitialModel,
    CellDelta,
    FullReplacement,
    ActiveTab,
    Mixed,
}

pub(crate) struct WorkerStartup {
    pub(crate) restored_payload: Option<Vec<u8>>,
    pub(crate) recovery_error: Option<String>,
}

pub(crate) struct WorkerModel {
    pub(crate) sheets: Vec<Sheet>,
    pub(crate) active_sheet: usize,
}

struct InitializationRequest {
    revision: u64,
    active_sheet: usize,
    sheets: Vec<Sheet>,
    record_recovery: bool,
    reply: SyncSender<WorkerModel>,
}

enum PendingUpdate {
    Cell(CellUpdate),
    Replace {
        revision: u64,
        active_sheet: usize,
        sheets: Vec<Sheet>,
    },
    Active {
        revision: u64,
        active_sheet: usize,
    },
}

#[derive(Default)]
struct PendingBatch {
    revision: u64,
    active_sheet: usize,
    replacement: Option<Vec<Sheet>>,
    cells: HashMap<(usize, CellRef), Option<String>>,
}

impl PendingBatch {
    fn merge(&mut self, update: PendingUpdate) {
        let revision = match &update {
            PendingUpdate::Cell(update) => update.revision,
            PendingUpdate::Replace { revision, .. } | PendingUpdate::Active { revision, .. } => {
                *revision
            }
        };
        if revision < self.revision {
            return;
        }
        self.revision = revision;

        match update {
            PendingUpdate::Cell(update) => {
                self.active_sheet = update.active_sheet;
                self.cells.insert((update.sheet, update.cell), update.raw);
            }
            PendingUpdate::Replace {
                active_sheet,
                sheets,
                ..
            } => {
                self.active_sheet = active_sheet;
                self.replacement = Some(sheets);
                self.cells.clear();
            }
            PendingUpdate::Active { active_sheet, .. } => {
                self.active_sheet = active_sheet;
            }
        }
    }
}

pub(crate) struct CheckpointRequest {
    pub(crate) operation_id: u64,
    pub(crate) document_generation: u64,
    pub(crate) revision: u64,
    pub(crate) pending_replacement_token: Option<u64>,
    pub(crate) path: PathBuf,
}

enum WorkerMessage {
    Initialize(InitializationRequest),
    Batch(PendingBatch),
    Checkpoint(CheckpointRequest),
    Export(crate::export_operations::ExportOperation),
    #[cfg(test)]
    TestGate {
        entered: mpsc::Sender<()>,
        release: mpsc::Receiver<()>,
    },
}

#[derive(Default)]
struct Mailbox {
    queue: std::collections::VecDeque<WorkerMessage>,
    stopping: bool,
}

#[derive(Default)]
struct Shared {
    mailbox: Mutex<Mailbox>,
    work_available: Condvar,
    latest_result: Mutex<Option<WorkbookResult>>,
    result_available: Condvar,
}

enum RecoveryLocation {
    Application(String),
    #[cfg(test)]
    Directory(PathBuf),
}

pub(crate) struct WorkbookWorker {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl WorkbookWorker {
    pub(crate) fn start(
        application_id: impl Into<String>,
        schema: impl Into<String>,
        save_completions: mpsc::Sender<SaveCompletion>,
        export_completions: mpsc::Sender<ExportCompletion>,
    ) -> Result<(Self, WorkerStartup), String> {
        Self::spawn(
            RecoveryLocation::Application(application_id.into()),
            schema.into(),
            save_completions,
            export_completions,
        )
    }

    fn spawn(
        location: RecoveryLocation,
        schema: String,
        save_completions: mpsc::Sender<SaveCompletion>,
        export_completions: mpsc::Sender<ExportCompletion>,
    ) -> Result<(Self, WorkerStartup), String> {
        let shared = Arc::new(Shared::default());
        let worker_shared = Arc::clone(&shared);
        let (startup_tx, startup_rx) = mpsc::sync_channel(1);
        let worker_thread = thread::Builder::new()
            .name("loom-sheets-workbook".into())
            .spawn(move || {
                let (mut recovery, startup) = match location {
                    RecoveryLocation::Application(application_id) => {
                        open_recovery(CellEditRecovery::open(&application_id))
                    }
                    #[cfg(test)]
                    RecoveryLocation::Directory(directory) => {
                        open_recovery(CellEditRecovery::open_at(directory))
                    }
                };
                let startup_error = startup.recovery_error.clone();
                if startup_tx.send(startup).is_ok() {
                    run_worker(
                        worker_shared,
                        &mut recovery,
                        &schema,
                        startup_error,
                        save_completions,
                        export_completions,
                    );
                }
            })
            .map_err(|error| format!("start workbook worker: {error}"))?;
        let startup = startup_rx
            .recv()
            .map_err(|error| format!("start workbook worker: {error}"))?;
        Ok((
            Self {
                shared,
                thread: Some(worker_thread),
            },
            startup,
        ))
    }

    pub(crate) fn submit_cell(&self, update: CellUpdate) -> Result<(), String> {
        self.queue(PendingUpdate::Cell(update))
    }

    /// Move the startup model to the worker and receive its UI-owned copy.
    /// The worker keeps the original as its mirror for later cell deltas.
    pub(crate) fn initialize_workbook(
        &self,
        revision: u64,
        active_sheet: usize,
        sheets: Vec<Sheet>,
    ) -> Result<WorkerModel, String> {
        self.initialize_workbook_inner(revision, active_sheet, sheets, true)
    }

    /// Install a temporary startup model without replacing a previous
    /// recovery payload. Used when an import is waiting for user confirmation.
    pub(crate) fn initialize_workbook_without_recovery(
        &self,
        revision: u64,
        active_sheet: usize,
        sheets: Vec<Sheet>,
    ) -> Result<WorkerModel, String> {
        self.initialize_workbook_inner(revision, active_sheet, sheets, false)
    }

    fn initialize_workbook_inner(
        &self,
        revision: u64,
        active_sheet: usize,
        sheets: Vec<Sheet>,
        record_recovery: bool,
    ) -> Result<WorkerModel, String> {
        let (reply, response) = mpsc::sync_channel(1);
        let mut mailbox = self
            .shared
            .mailbox
            .lock()
            .map_err(|_| "workbook worker mailbox is unavailable".to_string())?;
        if mailbox.stopping {
            return Err("workbook worker is stopping".to_string());
        }
        if mailbox
            .queue
            .iter()
            .any(|m| matches!(m, WorkerMessage::Initialize(_)))
        {
            return Err("workbook initialization is already pending".to_string());
        }
        mailbox
            .queue
            .push_back(WorkerMessage::Initialize(InitializationRequest {
                revision,
                active_sheet,
                sheets,
                record_recovery,
                reply,
            }));
        drop(mailbox);
        self.shared.work_available.notify_one();
        response
            .recv()
            .map_err(|error| format!("workbook initialization failed: {error}"))
    }

    pub(crate) fn submit_replacement(
        &self,
        revision: u64,
        active_sheet: usize,
        sheets: Vec<Sheet>,
    ) -> Result<(), String> {
        self.queue(PendingUpdate::Replace {
            revision,
            active_sheet,
            sheets,
        })
    }

    pub(crate) fn submit_active(&self, revision: u64, active_sheet: usize) -> Result<(), String> {
        self.queue(PendingUpdate::Active {
            revision,
            active_sheet,
        })
    }

    fn queue(&self, update: PendingUpdate) -> Result<(), String> {
        let mut mailbox = self
            .shared
            .mailbox
            .lock()
            .map_err(|_| "workbook worker mailbox is unavailable".to_string())?;
        if mailbox.stopping {
            return Err("workbook worker is stopping".to_string());
        }
        if let Some(WorkerMessage::Batch(batch)) = mailbox.queue.back_mut() {
            batch.merge(update);
        } else {
            let mut batch = PendingBatch::default();
            batch.merge(update);
            mailbox.queue.push_back(WorkerMessage::Batch(batch));
        }
        drop(mailbox);
        self.shared.work_available.notify_one();
        Ok(())
    }

    pub(crate) fn take_latest_result(&self) -> Option<WorkbookResult> {
        self.shared.latest_result.lock().ok()?.take()
    }

    /// Queue a Save barrier after all updates currently accepted by the worker.
    pub(crate) fn queue_save(
        &self,
        operation_id: u64,
        document_generation: u64,
        revision: u64,
        pending_replacement_token: Option<u64>,
        path: PathBuf,
    ) -> Result<(), String> {
        let mut mailbox = self
            .shared
            .mailbox
            .lock()
            .map_err(|_| "workbook worker mailbox is unavailable".to_string())?;
        if mailbox.stopping {
            return Err("workbook worker is stopping".to_string());
        }
        if mailbox
            .queue
            .iter()
            .any(|m| matches!(m, WorkerMessage::Checkpoint(_)))
        {
            return Err("a save operation is already pending".to_string());
        }
        mailbox
            .queue
            .push_back(WorkerMessage::Checkpoint(CheckpointRequest {
                operation_id,
                document_generation,
                revision,
                pending_replacement_token,
                path,
            }));
        drop(mailbox);
        self.shared.work_available.notify_one();
        Ok(())
    }

    /// Queue an export barrier at its accepted worker revision.
    pub(crate) fn queue_export(&self, operation: ExportOperation) -> Result<(), String> {
        let mut mailbox = self
            .shared
            .mailbox
            .lock()
            .map_err(|_| "workbook worker mailbox is unavailable".to_string())?;
        if mailbox.stopping {
            return Err("workbook worker is stopping".to_string());
        }
        mailbox.queue.push_back(WorkerMessage::Export(operation));
        drop(mailbox);
        self.shared.work_available.notify_one();
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn enqueue_test_gate(&self) -> (mpsc::Receiver<()>, mpsc::Sender<()>) {
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        self.shared
            .mailbox
            .lock()
            .expect("workbook worker mailbox")
            .queue
            .push_back(WorkerMessage::TestGate {
                entered: entered_tx,
                release: release_rx,
            });
        self.shared.work_available.notify_one();
        (entered_rx, release_tx)
    }

    #[cfg(test)]
    pub(super) fn reject_submissions_for_test(&self) {
        let mut mailbox = self.shared.mailbox.lock().expect("workbook worker mailbox");
        mailbox.stopping = true;
        drop(mailbox);
        self.shared.work_available.notify_one();
    }

    #[cfg(test)]
    pub(super) fn pending_exports_for_test(&self) -> usize {
        self.shared
            .mailbox
            .lock()
            .expect("workbook worker mailbox")
            .queue
            .iter()
            .filter(|message| matches!(message, WorkerMessage::Export(_)))
            .count()
    }

    #[cfg(test)]
    pub(super) fn wait_for_result(&self, revision: u64) -> Option<WorkbookResult> {
        self.wait_for_result_timeout(revision, Duration::from_secs(5))
    }

    #[cfg(test)]
    pub(super) fn wait_for_result_timeout(
        &self,
        revision: u64,
        timeout: Duration,
    ) -> Option<WorkbookResult> {
        let deadline = Instant::now() + timeout;
        let mut latest = self.shared.latest_result.lock().ok()?;
        loop {
            if latest
                .as_ref()
                .is_some_and(|result| result.revision >= revision)
            {
                return latest.clone();
            }
            let remaining = deadline.checked_duration_since(Instant::now())?;
            let (next, timeout) = self
                .shared
                .result_available
                .wait_timeout(latest, remaining)
                .ok()?;
            latest = next;
            if timeout.timed_out() {
                return None;
            }
        }
    }

    #[cfg(test)]
    pub(super) fn start_at(
        directory: PathBuf,
        schema: impl Into<String>,
    ) -> Result<(Self, WorkerStartup), String> {
        let (save_completions, _receiver) = mpsc::channel();
        Self::start_at_with_completions(directory, schema, save_completions)
    }

    #[cfg(test)]
    pub(super) fn start_at_with_completions(
        directory: PathBuf,
        schema: impl Into<String>,
        save_completions: mpsc::Sender<SaveCompletion>,
    ) -> Result<(Self, WorkerStartup), String> {
        let (export_completions, _receiver) = mpsc::channel();
        Self::start_at_with_file_completions(
            directory,
            schema,
            save_completions,
            export_completions,
        )
    }

    #[cfg(test)]
    pub(super) fn start_at_with_file_completions(
        directory: PathBuf,
        schema: impl Into<String>,
        save_completions: mpsc::Sender<SaveCompletion>,
        export_completions: mpsc::Sender<ExportCompletion>,
    ) -> Result<(Self, WorkerStartup), String> {
        Self::spawn(
            RecoveryLocation::Directory(directory),
            schema.into(),
            save_completions,
            export_completions,
        )
    }
}

impl Drop for WorkbookWorker {
    fn drop(&mut self) {
        if let Ok(mut mailbox) = self.shared.mailbox.lock() {
            mailbox.stopping = true;
        }
        self.shared.work_available.notify_one();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn open_recovery(
    result: Result<(CellEditRecovery, Option<Vec<u8>>), String>,
) -> (Option<CellEditRecovery>, WorkerStartup) {
    match result {
        Ok((recovery, restored_payload)) => (
            Some(recovery),
            WorkerStartup {
                restored_payload,
                recovery_error: None,
            },
        ),
        Err(error) => (
            None,
            WorkerStartup {
                restored_payload: None,
                recovery_error: Some(error),
            },
        ),
    }
}

enum WorkerAction {
    Initialize(InitializationRequest),
    Batch(PendingBatch),
    Checkpoint(CheckpointRequest),
    Export(ExportOperation),
    #[cfg(test)]
    TestGate {
        entered: mpsc::Sender<()>,
        release: mpsc::Receiver<()>,
    },
    Stop,
}

fn next_action(shared: &Shared) -> WorkerAction {
    let mut mailbox = shared.mailbox.lock().expect("workbook worker mailbox");
    loop {
        if let Some(msg) = mailbox.queue.pop_front() {
            return match msg {
                WorkerMessage::Initialize(req) => WorkerAction::Initialize(req),
                WorkerMessage::Batch(batch) => WorkerAction::Batch(batch),
                WorkerMessage::Checkpoint(req) => WorkerAction::Checkpoint(req),
                WorkerMessage::Export(operation) => WorkerAction::Export(operation),
                #[cfg(test)]
                WorkerMessage::TestGate { entered, release } => {
                    WorkerAction::TestGate { entered, release }
                }
            };
        }
        if mailbox.stopping {
            return WorkerAction::Stop;
        }
        mailbox = shared
            .work_available
            .wait(mailbox)
            .expect("workbook worker mailbox");
    }
}

fn next_completion_sequence(sequence: &mut u64) -> u64 {
    *sequence = sequence
        .checked_add(1)
        .expect("Sheets file completion sequence exhausted");
    *sequence
}

fn run_worker(
    shared: Arc<Shared>,
    recovery: &mut Option<CellEditRecovery>,
    _schema: &str,
    startup_error: Option<String>,
    save_completions: mpsc::Sender<crate::save_operations::SaveCompletion>,
    export_completions: mpsc::Sender<crate::export_operations::ExportCompletion>,
) {
    let mut sheets = Vec::new();
    let mut active_sheet = 0;
    let mut last_revision = 0;
    let mut baseline: Option<(Vec<Sheet>, usize)> = None;
    let mut completion_sequence = 0;
    loop {
        match next_action(&shared) {
            WorkerAction::Initialize(initialization) => {
                if initialization.revision <= last_revision {
                    continue;
                }
                last_revision = initialization.revision;
                let record_recovery = initialization.record_recovery;
                sheets = initialization.sheets;
                if sheets.is_empty() {
                    sheets.push(Sheet::new("Untitled"));
                }
                active_sheet = initialization
                    .active_sheet
                    .min(sheets.len().saturating_sub(1));
                baseline = Some((sheets.clone(), active_sheet));
                let model = WorkerModel {
                    sheets: sheets.clone(),
                    active_sheet,
                };
                let _ = initialization.reply.send(model);

                let mut recovery_error = startup_error.clone();
                #[cfg(test)]
                let mut recovery_package_duration = Duration::ZERO;
                #[cfg(test)]
                let mut recovery_journal_duration = Duration::ZERO;
                if record_recovery {
                    if let Some(recovery) = recovery.as_mut() {
                        if recovery.needs_initial_checkpoint() {
                            #[cfg(test)]
                            let package_started = Instant::now();
                            let package = workbook_package_bytes(&sheets, active_sheet);
                            #[cfg(test)]
                            {
                                recovery_package_duration = package_started.elapsed();
                            }
                            #[cfg(test)]
                            let journal_started = Instant::now();
                            match package {
                                Ok(payload) => {
                                    if let Err(error) = recovery.checkpoint_package(payload, false)
                                    {
                                        recovery_error = Some(error);
                                    }
                                }
                                Err(error) => {
                                    recovery.mark_failed(error.clone());
                                    recovery_error = Some(error);
                                }
                            }
                            #[cfg(test)]
                            {
                                recovery_journal_duration = journal_started.elapsed();
                            }
                        }
                    }
                }

                #[cfg(test)]
                let evaluation_started = Instant::now();
                let values = evaluate_workbook(&sheets)
                    .into_iter()
                    .nth(active_sheet)
                    .unwrap_or_default();
                #[cfg(test)]
                let evaluation_duration = evaluation_started.elapsed();

                let dirty = workbook_differs_from_baseline(&baseline, &sheets, active_sheet);

                publish_result(
                    &shared,
                    WorkbookResult {
                        revision: initialization.revision,
                        active_sheet,
                        values,
                        recovery_error,
                        input_error: None,
                        dirty,
                        #[cfg(test)]
                        update_kind: WorkerUpdateKind::InitialModel,
                        #[cfg(test)]
                        cell_updates: 0,
                        #[cfg(test)]
                        evaluation_duration,
                        #[cfg(test)]
                        recovery_package_duration,
                        #[cfg(test)]
                        recovery_journal_duration,
                    },
                );
            }
            WorkerAction::Batch(batch) => {
                if batch.revision <= last_revision {
                    continue;
                }
                last_revision = batch.revision;
                #[cfg(test)]
                let cell_updates = batch.cells.len();
                #[cfg(test)]
                let update_kind = match (batch.replacement.is_some(), cell_updates > 0) {
                    (true, true) => WorkerUpdateKind::Mixed,
                    (true, false) => WorkerUpdateKind::FullReplacement,
                    (false, true) => WorkerUpdateKind::CellDelta,
                    (false, false) => WorkerUpdateKind::ActiveTab,
                };
                let is_replacement = batch.replacement.is_some();
                if let Some(replacement) = batch.replacement {
                    sheets = replacement;
                    if sheets.is_empty() {
                        sheets.push(Sheet::new("Untitled"));
                    }
                }
                let mut input_error = None;
                let mut accepted_cell_edits = Vec::new();
                for ((sheet_index, cell), raw) in batch.cells {
                    match sheets.get_mut(sheet_index) {
                        Some(sheet) => match raw {
                            Some(raw) => {
                                sheet.set_raw(cell, raw.clone());
                                accepted_cell_edits.push((sheet_index, cell, Some(raw)));
                            }
                            None => {
                                sheet.clear_cell(cell);
                                accepted_cell_edits.push((sheet_index, cell, None));
                            }
                        },
                        None => {
                            input_error.get_or_insert_with(|| {
                                format!("cell edit targets missing sheet {sheet_index}")
                            });
                        }
                    }
                }
                active_sheet = batch.active_sheet.min(sheets.len().saturating_sub(1));
                let mut recovery_error = startup_error.clone();
                #[cfg(test)]
                let mut recovery_package_duration = Duration::ZERO;
                #[cfg(test)]
                let mut recovery_journal_duration = Duration::ZERO;
                if let Some(recovery) = recovery.as_mut() {
                    if is_replacement {
                        #[cfg(test)]
                        let package_started = Instant::now();
                        let package = workbook_package_bytes(&sheets, active_sheet);
                        #[cfg(test)]
                        {
                            recovery_package_duration = package_started.elapsed();
                        }
                        #[cfg(test)]
                        let journal_started = Instant::now();
                        match package {
                            Ok(payload) => {
                                if let Err(error) = recovery.checkpoint_package(payload, true) {
                                    recovery_error = Some(error);
                                }
                            }
                            Err(error) => {
                                recovery.mark_failed(error.clone());
                                recovery_error = Some(error);
                            }
                        }
                        #[cfg(test)]
                        {
                            recovery_journal_duration = journal_started.elapsed();
                        }
                    } else {
                        #[cfg(test)]
                        let journal_started = Instant::now();
                        if let Err(error) =
                            recovery.record_cells(active_sheet, accepted_cell_edits, || {
                                workbook_package_bytes(&sheets, active_sheet)
                            })
                        {
                            recovery_error = Some(error);
                        }
                        #[cfg(test)]
                        {
                            recovery_journal_duration = journal_started.elapsed();
                        }
                    }
                }
                #[cfg(test)]
                let evaluation_started = Instant::now();
                let values = evaluate_workbook(&sheets)
                    .into_iter()
                    .nth(active_sheet)
                    .unwrap_or_default();
                #[cfg(test)]
                let evaluation_duration = evaluation_started.elapsed();
                let dirty = workbook_differs_from_baseline(&baseline, &sheets, active_sheet);

                publish_result(
                    &shared,
                    WorkbookResult {
                        revision: batch.revision,
                        active_sheet,
                        values,
                        recovery_error,
                        input_error,
                        dirty,
                        #[cfg(test)]
                        update_kind,
                        #[cfg(test)]
                        cell_updates,
                        #[cfg(test)]
                        evaluation_duration,
                        #[cfg(test)]
                        recovery_package_duration,
                        #[cfg(test)]
                        recovery_journal_duration,
                    },
                );
            }
            WorkerAction::Checkpoint(checkpoint) => {
                if last_revision != checkpoint.revision {
                    let _ = save_completions.send(crate::save_operations::SaveCompletion {
                        completion_sequence: next_completion_sequence(&mut completion_sequence),
                        operation: crate::save_operations::SaveOperation {
                            operation_id: checkpoint.operation_id,
                            document_generation: checkpoint.document_generation,
                            target_revision: checkpoint.revision,
                            pending_replacement_token: checkpoint.pending_replacement_token,
                        },
                        path: checkpoint.path,
                        write_result: Err(format!(
                            "Save revision {} is unavailable; worker is at revision {last_revision}",
                            checkpoint.revision
                        )),
                        checkpoint_result: None,
                        baseline: None,
                    });
                    continue;
                }
                let package_result = workbook_package_bytes(&sheets, active_sheet);
                let (write_result, checkpoint_result, saved_baseline) = match package_result {
                    Ok(payload) => {
                        let write_result = loom_storage::atomic_write(&checkpoint.path, &payload)
                            .map_err(|error| error.to_string());
                        if write_result.is_ok() {
                            let saved_baseline = (sheets.clone(), active_sheet);
                            baseline = Some(saved_baseline.clone());
                            let checkpoint_result = Some(match recovery.as_mut() {
                                Some(recovery) => recovery.checkpoint_package(payload, false),
                                None => Err(startup_error.clone().unwrap_or_else(|| {
                                    "recovery writer is unavailable".to_string()
                                })),
                            });
                            (write_result, checkpoint_result, Some(saved_baseline))
                        } else {
                            (write_result, None, None)
                        }
                    }
                    Err(error) => (Err(error), None, None),
                };

                let _ = save_completions.send(crate::save_operations::SaveCompletion {
                    completion_sequence: next_completion_sequence(&mut completion_sequence),
                    operation: crate::save_operations::SaveOperation {
                        operation_id: checkpoint.operation_id,
                        document_generation: checkpoint.document_generation,
                        target_revision: checkpoint.revision,
                        pending_replacement_token: checkpoint.pending_replacement_token,
                    },
                    path: checkpoint.path,
                    write_result,
                    checkpoint_result,
                    baseline: saved_baseline,
                });
            }
            WorkerAction::Export(operation) => {
                #[cfg(test)]
                let started = Instant::now();
                let result = export_at_revision(&sheets, active_sheet, last_revision, &operation);
                #[cfg(test)]
                let worker_duration = started.elapsed();
                let _ = export_completions.send(ExportCompletion {
                    completion_sequence: next_completion_sequence(&mut completion_sequence),
                    operation,
                    result,
                    #[cfg(test)]
                    worker_duration,
                });
            }
            #[cfg(test)]
            WorkerAction::TestGate { entered, release } => {
                let _ = entered.send(());
                let _ = release.recv();
            }
            WorkerAction::Stop => return,
        }
    }
}

fn export_at_revision(
    sheets: &[Sheet],
    active_sheet: usize,
    last_revision: u64,
    operation: &ExportOperation,
) -> Result<ExportOutputSummary, String> {
    if last_revision != operation.target_revision {
        return Err(format!(
            "revision {} is unavailable; worker is at revision {last_revision}",
            operation.target_revision
        ));
    }
    match operation.format {
        ExportFormat::Csv => {
            let sheet = sheets
                .get(active_sheet)
                .ok_or_else(|| format!("active sheet {} is unavailable", active_sheet + 1))?;
            let csv = loom_sheets_core::to_csv_with_formulas(sheet);
            loom_storage::atomic_write(&operation.path, csv.as_bytes())
                .map_err(|error| format!("CSV write failed: {error}"))?;
            Ok(ExportOutputSummary::Csv {
                sheet_name: sheet.name.clone(),
            })
        }
        ExportFormat::Xlsx => {
            let bytes = loom_sheets_core::export_xlsx_sheets(sheets)
                .map_err(|error| format!("XLSX generation failed: {error}"))?;
            loom_storage::atomic_write(&operation.path, &bytes)
                .map_err(|error| format!("XLSX write failed: {error}"))?;
            Ok(ExportOutputSummary::Xlsx {
                sheet_count: sheets.len(),
            })
        }
    }
}

fn workbook_differs_from_baseline(
    baseline: &Option<(Vec<Sheet>, usize)>,
    sheets: &[Sheet],
    active_sheet: usize,
) -> bool {
    match baseline {
        Some((saved_sheets, saved_active)) => {
            workbook_to_json(saved_sheets, *saved_active) != workbook_to_json(sheets, active_sheet)
        }
        None => true,
    }
}

fn publish_result(shared: &Shared, result: WorkbookResult) {
    if let Ok(mut latest) = shared.latest_result.lock() {
        if latest
            .as_ref()
            .map_or(true, |previous| previous.revision <= result.revision)
        {
            *latest = Some(result);
        }
    }
    shared.result_available.notify_all();
}

#[cfg(test)]
#[path = "workbook_worker_tests.rs"]
mod tests;
