use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{self, SyncSender};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
#[cfg(test)]
use std::time::Duration;
use std::time::Instant;

use loom_sheets_core::persistence::workbook_states_match;
use loom_sheets_core::workbook::evaluate_workbook;
use loom_sheets_core::{CellRef, Sheet, Value};

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

#[derive(Debug, Clone)]
pub(crate) struct WorkerInputError {
    pub(crate) revision: u64,
    pub(crate) error: String,
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
    /// The file the restored draft saves to; `None` for an untitled draft.
    pub(crate) restored_source: Option<PathBuf>,
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
    /// Discard the recovery store's contents first: they are an untouched
    /// starter, so `sheets` becomes the only checkpoint.
    replace_stored_checkpoint: bool,
    reply: SyncSender<WorkerModel>,
}

enum PendingUpdate {
    Cell(CellUpdate),
    Replace {
        revision: u64,
        active_sheet: usize,
        sheets: Vec<Sheet>,
        source: Option<PathBuf>,
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
    replacement_source: Option<PathBuf>,
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
                source,
                ..
            } => {
                self.active_sheet = active_sheet;
                self.replacement = Some(sheets);
                self.replacement_source = source;
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
    DiscardRecovery(SyncSender<Result<(), String>>),
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
    pending_input_failure: Mutex<Option<WorkerInputError>>,
    /// The latched recovery failure, mirrored from the recovery writer. While
    /// it is set the UI stops admitting edits; a successful complete
    /// checkpoint (Retry, Save, or Save As) clears it.
    recovery_pause: Mutex<Option<String>>,
    result_available: Condvar,
}

#[derive(Clone)]
enum RecoveryLocation {
    Application(String),
    #[cfg(test)]
    Directory(PathBuf),
    #[cfg(test)]
    DirectoryWithCadence(PathBuf, crate::recovery_policy::CadenceLimits),
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
                let (mut recovery, startup) = open_recovery(open_location(&location));
                let startup_error = startup.recovery_error.clone();
                if startup_tx.send(startup).is_ok() {
                    run_worker(
                        worker_shared,
                        &mut recovery,
                        &location,
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
        self.initialize_workbook_inner(revision, active_sheet, sheets, true, false)
    }

    /// Start a fresh workbook whose recovery store currently holds an untouched
    /// starter. The stored starter is replaced by a checkpoint of `sheets`.
    pub(crate) fn initialize_workbook_replacing_stored_draft(
        &self,
        revision: u64,
        active_sheet: usize,
        sheets: Vec<Sheet>,
    ) -> Result<WorkerModel, String> {
        self.initialize_workbook_inner(revision, active_sheet, sheets, true, true)
    }

    /// Install a temporary startup model without replacing a previous
    /// recovery payload. Used when an import is waiting for user confirmation.
    pub(crate) fn initialize_workbook_without_recovery(
        &self,
        revision: u64,
        active_sheet: usize,
        sheets: Vec<Sheet>,
    ) -> Result<WorkerModel, String> {
        self.initialize_workbook_inner(revision, active_sheet, sheets, false, false)
    }

    fn initialize_workbook_inner(
        &self,
        revision: u64,
        active_sheet: usize,
        sheets: Vec<Sheet>,
        record_recovery: bool,
        replace_stored_checkpoint: bool,
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
                replace_stored_checkpoint,
                reply,
            }));
        drop(mailbox);
        self.shared.work_available.notify_one();
        response
            .recv()
            .map_err(|error| format!("workbook initialization failed: {error}"))
    }

    /// Replace the whole workbook. `source` is the file it now saves to.
    pub(crate) fn submit_replacement(
        &self,
        revision: u64,
        active_sheet: usize,
        sheets: Vec<Sheet>,
        source: Option<PathBuf>,
    ) -> Result<(), String> {
        self.queue(PendingUpdate::Replace {
            revision,
            active_sheet,
            sheets,
            source,
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

    pub(crate) fn pending_input_failure(&self) -> Result<Option<WorkerInputError>, String> {
        read_pending_input_failure(&self.shared)
    }

    /// The reason durable recovery can no longer keep up, if it cannot.
    pub(crate) fn recovery_pause(&self) -> Option<String> {
        self.shared.recovery_pause.lock().map_or_else(
            |_| Some("recovery state is unavailable".into()),
            |p| p.clone(),
        )
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

    /// Delete this session's recovery stores after every accepted edit has been
    /// journaled (the request queues behind them) and wait for the result. Used
    /// only on an intentional close; a crash never reaches it.
    pub(crate) fn discard_recovery(&self) -> Result<(), String> {
        let (reply, receiver) = mpsc::sync_channel(1);
        {
            let mut mailbox = self
                .shared
                .mailbox
                .lock()
                .map_err(|_| "workbook worker mailbox is unavailable".to_string())?;
            mailbox
                .queue
                .push_back(WorkerMessage::DiscardRecovery(reply));
        }
        self.shared.work_available.notify_one();
        receiver
            .recv_timeout(std::time::Duration::from_secs(30))
            .map_err(|error| format!("clear Sheets recovery: {error}"))?
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
    pub(super) fn start_at_with_cadence(
        directory: PathBuf,
        schema: impl Into<String>,
        limits: crate::recovery_policy::CadenceLimits,
    ) -> Result<(Self, WorkerStartup), String> {
        let (save_completions, _save) = mpsc::channel();
        let (export_completions, _export) = mpsc::channel();
        Self::spawn(
            RecoveryLocation::DirectoryWithCadence(directory, limits),
            schema.into(),
            save_completions,
            export_completions,
        )
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
        Ok((recovery, restored_payload)) => {
            let restored_source = recovery.current_source();
            (
                Some(recovery),
                WorkerStartup {
                    restored_payload,
                    restored_source,
                    recovery_error: None,
                },
            )
        }
        Err(error) => (
            None,
            WorkerStartup {
                restored_payload: None,
                restored_source: None,
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
    DiscardRecovery(SyncSender<Result<(), String>>),
    #[cfg(test)]
    TestGate {
        entered: mpsc::Sender<()>,
        release: mpsc::Receiver<()>,
    },
    /// The five-minute checkpoint trigger fired while the worker was idle.
    CadenceTick,
    Stop,
}

/// Open the recovery store at its location, with the test cadence if one was
/// asked for. Used at start-up and again by Retry Recovery.
fn open_location(
    location: &RecoveryLocation,
) -> Result<(CellEditRecovery, Option<Vec<u8>>), String> {
    match location {
        RecoveryLocation::Application(application_id) => CellEditRecovery::open(application_id),
        #[cfg(test)]
        RecoveryLocation::Directory(directory) => CellEditRecovery::open_at(directory),
        #[cfg(test)]
        RecoveryLocation::DirectoryWithCadence(directory, limits) => {
            CellEditRecovery::open_at(directory).map(|(mut recovery, restored)| {
                recovery.set_cadence_limits_for_test(*limits);
                (recovery, restored)
            })
        }
    }
}

/// Retry for a store that never opened. A draft left by an earlier session is
/// not this window's work, and publishing this workbook over it would destroy
/// it, so a store that still holds one stays closed.
fn reopen_recovery(location: &RecoveryLocation) -> Result<CellEditRecovery, String> {
    let (recovery, restored) = open_location(location)?;
    if restored.is_some() {
        return Err(crate::recovery_pause::EARLIER_DRAFT_WAITING.to_string());
    }
    Ok(recovery)
}

fn sync_recovery_pause(
    shared: &Shared,
    recovery: &Option<CellEditRecovery>,
    startup_error: &Option<String>,
) {
    // A store that never opened has no writer to latch a failure, so its open
    // error is the pause: nothing can be journaled until it opens.
    let failure = match recovery {
        Some(recovery) => recovery.failure().map(str::to_owned),
        None => startup_error
            .as_deref()
            .map(crate::recovery_pause::describe_open_failure),
    };
    if let Ok(mut pause) = shared.recovery_pause.lock() {
        *pause = failure;
    }
}

fn next_action(shared: &Shared, deadline: Option<Instant>) -> WorkerAction {
    let mut mailbox = shared.mailbox.lock().expect("workbook worker mailbox");
    loop {
        if let Some(msg) = mailbox.queue.pop_front() {
            return match msg {
                WorkerMessage::Initialize(req) => WorkerAction::Initialize(req),
                WorkerMessage::Batch(batch) => WorkerAction::Batch(batch),
                WorkerMessage::Checkpoint(req) => WorkerAction::Checkpoint(req),
                WorkerMessage::Export(operation) => WorkerAction::Export(operation),
                WorkerMessage::DiscardRecovery(reply) => WorkerAction::DiscardRecovery(reply),
                #[cfg(test)]
                WorkerMessage::TestGate { entered, release } => {
                    WorkerAction::TestGate { entered, release }
                }
            };
        }
        if mailbox.stopping {
            return WorkerAction::Stop;
        }
        mailbox = match deadline {
            None => shared
                .work_available
                .wait(mailbox)
                .expect("workbook worker mailbox"),
            Some(deadline) => {
                let now = Instant::now();
                if now >= deadline {
                    return WorkerAction::CadenceTick;
                }
                shared
                    .work_available
                    .wait_timeout(mailbox, deadline - now)
                    .expect("workbook worker mailbox")
                    .0
            }
        };
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
    location: &RecoveryLocation,
    _schema: &str,
    mut startup_error: Option<String>,
    save_completions: mpsc::Sender<crate::save_operations::SaveCompletion>,
    export_completions: mpsc::Sender<crate::export_operations::ExportCompletion>,
) {
    let mut sheets = Vec::new();
    let mut active_sheet = 0;
    let mut last_revision = 0;
    let mut baseline: Option<(Vec<Sheet>, usize)> = None;
    let mut completion_sequence = 0;
    // True once the worker's model is the workbook that recovery protects. A
    // temporary import awaiting confirmation (or a worker that has not been
    // initialized yet) must never be written over the recovery checkpoint.
    let mut mirror_is_recoverable = false;
    loop {
        let cadence_deadline = if mirror_is_recoverable {
            recovery
                .as_ref()
                .and_then(CellEditRecovery::checkpoint_deadline)
        } else {
            None
        };
        match next_action(&shared, cadence_deadline) {
            WorkerAction::CadenceTick => {
                if let Some(recovery) = recovery.as_mut() {
                    if recovery.checkpoint_due_at(Instant::now()) {
                        match workbook_package_bytes(&sheets, active_sheet) {
                            Ok(package) => {
                                // A failure latches inside the recovery writer
                                // and pauses edit admission below.
                                let source = recovery.current_source();
                                let _ =
                                    recovery.checkpoint_package(package, false, source.as_deref());
                            }
                            Err(error) => recovery.mark_failed(error),
                        }
                    }
                }
                sync_recovery_pause(&shared, recovery, &startup_error);
            }
            WorkerAction::Initialize(initialization) => {
                if initialization.revision <= last_revision {
                    continue;
                }
                last_revision = initialization.revision;
                let record_recovery = initialization.record_recovery;
                let replace_stored_checkpoint = initialization.replace_stored_checkpoint;
                sheets = initialization.sheets;
                if sheets.is_empty() {
                    sheets.push(crate::workbook_io::blank_sheet());
                }
                mirror_is_recoverable = record_recovery;
                active_sheet = initialization
                    .active_sheet
                    .min(sheets.len().saturating_sub(1));
                baseline = Some((sheets.clone(), active_sheet));
                clear_pending_input_failure(&shared);
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
                        if replace_stored_checkpoint {
                            // The stored checkpoint is an untouched starter, not a
                            // draft: discard it so the fresh blank is the only base.
                            if let Err(error) = recovery.discard_all() {
                                recovery.mark_failed(error.clone());
                                recovery_error = Some(error);
                            }
                        }
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
                                    // A fresh store starts from the blank or example
                                    // workbook, which has no file.
                                    if let Err(error) =
                                        recovery.checkpoint_package(payload, false, None)
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

                sync_recovery_pause(&shared, recovery, &startup_error);
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
                let replacement_source = batch.replacement_source;
                if let Some(replacement) = batch.replacement {
                    sheets = replacement;
                    if sheets.is_empty() {
                        sheets.push(crate::workbook_io::blank_sheet());
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
                update_pending_input_failure(
                    &shared,
                    batch.revision,
                    input_error.as_deref(),
                    is_replacement && input_error.is_none(),
                );
                active_sheet = batch.active_sheet.min(sheets.len().saturating_sub(1));
                // A full replacement is how Retry Recovery reaches the worker:
                // try to open a store that never opened before checkpointing.
                if is_replacement && recovery.is_none() && startup_error.is_some() {
                    match reopen_recovery(location) {
                        Ok(reopened) => {
                            *recovery = Some(reopened);
                            startup_error = None;
                        }
                        Err(error) => startup_error = Some(error),
                    }
                }
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
                            Ok(payload) => match recovery.checkpoint_package(
                                payload,
                                true,
                                replacement_source.as_deref(),
                            ) {
                                Ok(()) => mirror_is_recoverable = true,
                                Err(error) => recovery_error = Some(error),
                            },
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
                sync_recovery_pause(&shared, recovery, &startup_error);
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
                let input_failure = match read_pending_input_failure(&shared) {
                    Ok(Some(failure)) => Some(format!(
                        "Save blocked until the workbook is fully resynchronized after revision {} failed: {}",
                        failure.revision, failure.error
                    )),
                    Ok(None) => None,
                    Err(error) => Some(format!("Save blocked: {error}")),
                };
                if let Some(error) = input_failure {
                    let _ = save_completions.send(crate::save_operations::SaveCompletion {
                        completion_sequence: next_completion_sequence(&mut completion_sequence),
                        operation: crate::save_operations::SaveOperation {
                            operation_id: checkpoint.operation_id,
                            document_generation: checkpoint.document_generation,
                            target_revision: checkpoint.revision,
                            pending_replacement_token: checkpoint.pending_replacement_token,
                        },
                        path: checkpoint.path,
                        write_result: Err(error),
                        checkpoint_result: None,
                        baseline: None,
                    });
                    continue;
                }
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
                                Some(recovery) => recovery.checkpoint_package(
                                    payload,
                                    false,
                                    Some(checkpoint.path.as_path()),
                                ),
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
                sync_recovery_pause(&shared, recovery, &startup_error);

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
                let result = match read_pending_input_failure(&shared) {
                    Ok(Some(failure)) => Err(format!(
                        "Export blocked until the workbook is fully resynchronized after revision {} failed: {}",
                        failure.revision, failure.error
                    )),
                    Ok(None) => export_at_revision(&sheets, active_sheet, last_revision, &operation),
                    Err(error) => Err(format!("Export blocked: {error}")),
                };
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
            WorkerAction::DiscardRecovery(reply) => {
                let result = match recovery.as_mut() {
                    Some(store) => store.discard_all(),
                    None => Ok(()),
                };
                if result.is_ok() {
                    // The store is gone on purpose; nothing may recreate it.
                    *recovery = None;
                }
                sync_recovery_pause(&shared, recovery, &startup_error);
                let _ = reply.send(result);
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
            let csv = crate::workbook_io::csv_file_bytes(sheet);
            loom_storage::atomic_write(&operation.path, &csv)
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
            !workbook_states_match(saved_sheets, *saved_active, sheets, active_sheet)
        }
        None => true,
    }
}

fn update_pending_input_failure(
    shared: &Shared,
    revision: u64,
    input_error: Option<&str>,
    successful_full_resync: bool,
) {
    let Ok(mut pending) = shared.pending_input_failure.lock() else {
        return;
    };
    if let Some(error) = input_error {
        if pending.is_none() {
            *pending = Some(WorkerInputError {
                revision,
                error: error.to_string(),
            });
        }
    } else if successful_full_resync {
        pending.take();
    }
}

fn read_pending_input_failure(shared: &Shared) -> Result<Option<WorkerInputError>, String> {
    shared
        .pending_input_failure
        .lock()
        .map(|failure| failure.clone())
        .map_err(|_| "workbook worker input-failure mailbox is unavailable".to_string())
}

fn clear_pending_input_failure(shared: &Shared) {
    if let Ok(mut pending) = shared.pending_input_failure.lock() {
        pending.take();
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

#[cfg(test)]
#[path = "recovery_pause_tests.rs"]
mod pause_tests;

#[cfg(test)]
#[path = "recovery_bench_tests.rs"]
mod bench_tests;
