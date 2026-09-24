use std::collections::HashMap;
#[cfg(test)]
use std::path::PathBuf;
use std::sync::mpsc::{self, SyncSender};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
#[cfg(test)]
use std::time::{Duration, Instant};

use loom_production::snapshot::SnapshotRecovery;
use loom_sheets_core::workbook::evaluate_workbook;
use loom_sheets_core::{CellRef, Sheet, Value};

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

struct CheckpointRequest {
    payload: Vec<u8>,
    reply: SyncSender<Result<(), String>>,
}

#[derive(Default)]
struct Mailbox {
    initialization: Option<InitializationRequest>,
    pending: Option<PendingBatch>,
    checkpoint: Option<CheckpointRequest>,
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
    ) -> Result<(Self, WorkerStartup), String> {
        Self::spawn(
            RecoveryLocation::Application(application_id.into()),
            schema.into(),
        )
    }

    fn spawn(location: RecoveryLocation, schema: String) -> Result<(Self, WorkerStartup), String> {
        let shared = Arc::new(Shared::default());
        let worker_shared = Arc::clone(&shared);
        let (startup_tx, startup_rx) = mpsc::sync_channel(1);
        let worker_thread = thread::Builder::new()
            .name("loom-sheets-workbook".into())
            .spawn(move || {
                let (mut recovery, startup) = match location {
                    RecoveryLocation::Application(application_id) => {
                        open_recovery(SnapshotRecovery::open(&application_id))
                    }
                    #[cfg(test)]
                    RecoveryLocation::Directory(directory) => {
                        open_recovery(SnapshotRecovery::open_at(directory))
                    }
                };
                let startup_error = startup.recovery_error.clone();
                if startup_tx.send(startup).is_ok() {
                    run_worker(worker_shared, &mut recovery, &schema, startup_error);
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
        let (reply, response) = mpsc::sync_channel(1);
        let mut mailbox = self
            .shared
            .mailbox
            .lock()
            .map_err(|_| "workbook worker mailbox is unavailable".to_string())?;
        if mailbox.stopping {
            return Err("workbook worker is stopping".to_string());
        }
        if mailbox.initialization.is_some() {
            return Err("workbook initialization is already pending".to_string());
        }
        mailbox.initialization = Some(InitializationRequest {
            revision,
            active_sheet,
            sheets,
            reply,
        });
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
        mailbox
            .pending
            .get_or_insert_with(PendingBatch::default)
            .merge(update);
        drop(mailbox);
        self.shared.work_available.notify_one();
        Ok(())
    }

    pub(crate) fn take_latest_result(&self) -> Option<WorkbookResult> {
        self.shared.latest_result.lock().ok()?.take()
    }

    /// Flush a saved package through the worker-owned recovery journal. The
    /// desktop save operation is already synchronous, so waiting here also
    /// preserves the checkpoint's order relative to any pending edit batch.
    pub(crate) fn checkpoint(&self, payload: Vec<u8>) -> Result<(), String> {
        let (reply, response) = mpsc::sync_channel(1);
        let mut mailbox = self
            .shared
            .mailbox
            .lock()
            .map_err(|_| "workbook worker mailbox is unavailable".to_string())?;
        if mailbox.stopping {
            return Err("workbook worker is stopping".to_string());
        }
        if mailbox.checkpoint.is_some() {
            return Err("a save checkpoint is already pending".to_string());
        }
        mailbox.checkpoint = Some(CheckpointRequest { payload, reply });
        drop(mailbox);
        self.shared.work_available.notify_one();
        response
            .recv()
            .map_err(|error| format!("workbook recovery checkpoint failed: {error}"))?
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
        Self::spawn(RecoveryLocation::Directory(directory), schema.into())
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
    result: Result<SnapshotRecovery, loom_production::ProductionError>,
) -> (Option<SnapshotRecovery>, WorkerStartup) {
    match result {
        Ok(mut recovery) => {
            let restored_payload = recovery.take_restored_payload();
            (
                Some(recovery),
                WorkerStartup {
                    restored_payload,
                    recovery_error: None,
                },
            )
        }
        Err(error) => (
            None,
            WorkerStartup {
                restored_payload: None,
                recovery_error: Some(error.to_string()),
            },
        ),
    }
}

enum WorkerAction {
    Initialize(InitializationRequest),
    Batch(PendingBatch),
    Checkpoint(CheckpointRequest),
    Stop,
}

fn next_action(shared: &Shared) -> WorkerAction {
    let mut mailbox = shared.mailbox.lock().expect("workbook worker mailbox");
    loop {
        if let Some(initialization) = mailbox.initialization.take() {
            return WorkerAction::Initialize(initialization);
        }
        if let Some(batch) = mailbox.pending.take() {
            return WorkerAction::Batch(batch);
        }
        if let Some(checkpoint) = mailbox.checkpoint.take() {
            return WorkerAction::Checkpoint(checkpoint);
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

fn run_worker(
    shared: Arc<Shared>,
    recovery: &mut Option<SnapshotRecovery>,
    schema: &str,
    startup_error: Option<String>,
) {
    let mut sheets = Vec::new();
    let mut last_revision = 0;
    loop {
        match next_action(&shared) {
            WorkerAction::Initialize(initialization) => {
                if initialization.revision <= last_revision {
                    continue;
                }
                last_revision = initialization.revision;
                sheets = initialization.sheets;
                if sheets.is_empty() {
                    sheets.push(Sheet::new("Untitled"));
                }
                let active_sheet = initialization
                    .active_sheet
                    .min(sheets.len().saturating_sub(1));
                let model = WorkerModel {
                    sheets: sheets.clone(),
                    active_sheet,
                };
                let _ = initialization.reply.send(model);

                #[cfg(test)]
                let evaluation_started = Instant::now();
                let values = evaluate_workbook(&sheets)
                    .into_iter()
                    .nth(active_sheet)
                    .unwrap_or_default();
                #[cfg(test)]
                let evaluation_duration = evaluation_started.elapsed();
                let mut recovery_error = startup_error.clone();
                #[cfg(test)]
                let mut recovery_package_duration = Duration::ZERO;
                #[cfg(test)]
                let mut recovery_journal_duration = Duration::ZERO;
                if let Some(recovery) = recovery.as_mut() {
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
                            if let Err(error) = recovery.record("sheets state", payload) {
                                recovery_error = Some(error.to_string());
                            }
                        }
                        Err(error) => recovery_error = Some(error),
                    }
                    #[cfg(test)]
                    {
                        recovery_journal_duration = journal_started.elapsed();
                    }
                }
                publish_result(
                    &shared,
                    WorkbookResult {
                        revision: initialization.revision,
                        active_sheet,
                        values,
                        recovery_error,
                        input_error: None,
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
                if let Some(replacement) = batch.replacement {
                    sheets = replacement;
                    if sheets.is_empty() {
                        sheets.push(Sheet::new("Untitled"));
                    }
                }
                let mut input_error = None;
                for ((sheet_index, cell), raw) in batch.cells {
                    match sheets.get_mut(sheet_index) {
                        Some(sheet) => match raw {
                            Some(raw) => sheet.set_raw(cell, raw),
                            None => {
                                sheet.clear_cell(cell);
                            }
                        },
                        None => {
                            input_error.get_or_insert_with(|| {
                                format!("cell edit targets missing sheet {sheet_index}")
                            });
                        }
                    }
                }
                let active_sheet = batch.active_sheet.min(sheets.len().saturating_sub(1));
                #[cfg(test)]
                let evaluation_started = Instant::now();
                let values = evaluate_workbook(&sheets)
                    .into_iter()
                    .nth(active_sheet)
                    .unwrap_or_default();
                #[cfg(test)]
                let evaluation_duration = evaluation_started.elapsed();
                let mut recovery_error = startup_error.clone();
                #[cfg(test)]
                let mut recovery_package_duration = Duration::ZERO;
                #[cfg(test)]
                let mut recovery_journal_duration = Duration::ZERO;
                if let Some(recovery) = recovery.as_mut() {
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
                            if let Err(error) = recovery.record("sheets state", payload) {
                                recovery_error = Some(error.to_string());
                            }
                        }
                        Err(error) => recovery_error = Some(error),
                    }
                    #[cfg(test)]
                    {
                        recovery_journal_duration = journal_started.elapsed();
                    }
                }
                publish_result(
                    &shared,
                    WorkbookResult {
                        revision: batch.revision,
                        active_sheet,
                        values,
                        recovery_error,
                        input_error,
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
                let result = match recovery.as_mut() {
                    Some(recovery) => recovery
                        .checkpoint(schema, checkpoint.payload)
                        .map_err(|error| error.to_string()),
                    None => Err(startup_error
                        .clone()
                        .unwrap_or_else(|| "recovery writer is unavailable".to_string())),
                };
                let _ = checkpoint.reply.send(result);
            }
            WorkerAction::Stop => return,
        }
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
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

    struct ScratchDirectory(PathBuf);

    impl ScratchDirectory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "loom-sheets-worker-{}-{}",
                std::process::id(),
                NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).expect("create test recovery directory");
            Self(path)
        }

        fn path(&self) -> PathBuf {
            self.0.clone()
        }
    }

    impl Drop for ScratchDirectory {
        fn drop(&mut self) {
            if fs::remove_dir_all(&self.0).is_err() {
                let _ = fs::remove_file(&self.0);
            }
        }
    }

    fn cell(revision: u64, cell: CellRef, raw: &str) -> PendingUpdate {
        PendingUpdate::Cell(CellUpdate {
            revision,
            active_sheet: 0,
            sheet: 0,
            cell,
            raw: Some(raw.to_string()),
        })
    }

    fn value<'a>(result: &'a WorkbookResult, address: &str) -> &'a Value {
        result
            .values
            .get(&CellRef::parse(address).unwrap())
            .expect("calculated cell value")
    }

    #[test]
    fn pending_updates_keep_latest_value_per_cell_and_keep_other_cells() {
        let a1 = CellRef::parse("A1").unwrap();
        let b1 = CellRef::parse("B1").unwrap();
        let mut pending = PendingBatch::default();

        pending.merge(cell(1, a1, "old"));
        pending.merge(cell(2, a1, "new"));
        pending.merge(cell(3, b1, "kept"));

        assert_eq!(pending.revision, 3);
        assert_eq!(
            pending.cells.get(&(0, a1)).and_then(Option::as_deref),
            Some("new")
        );
        assert_eq!(
            pending.cells.get(&(0, b1)).and_then(Option::as_deref),
            Some("kept")
        );
    }

    #[test]
    fn replacement_drops_older_deltas_and_keeps_later_deltas() {
        let a1 = CellRef::parse("A1").unwrap();
        let b1 = CellRef::parse("B1").unwrap();
        let mut replacement = Sheet::new("replacement");
        replacement.set_str("A1", "replacement value");
        let mut pending = PendingBatch::default();

        pending.merge(cell(1, a1, "old A1"));
        pending.merge(cell(2, b1, "old B1"));
        pending.merge(PendingUpdate::Replace {
            revision: 3,
            active_sheet: 0,
            sheets: vec![replacement],
        });
        pending.merge(cell(4, b1, "new B1"));

        assert_eq!(pending.revision, 4);
        assert_eq!(
            pending.replacement.as_ref().unwrap()[0].raw(a1),
            Some("replacement value")
        );
        assert_eq!(pending.cells.len(), 1);
        assert_eq!(
            pending.cells.get(&(0, b1)).and_then(Option::as_deref),
            Some("new B1")
        );
        assert!(!pending.cells.contains_key(&(0, a1)));
    }

    #[test]
    fn latest_result_slot_never_replaces_a_newer_revision() {
        let shared = Shared::default();
        publish_result(
            &shared,
            WorkbookResult {
                revision: 2,
                active_sheet: 1,
                values: HashMap::new(),
                recovery_error: None,
                input_error: None,
                update_kind: WorkerUpdateKind::InitialModel,
                cell_updates: 0,
                evaluation_duration: Duration::ZERO,
                recovery_package_duration: Duration::ZERO,
                recovery_journal_duration: Duration::ZERO,
            },
        );
        publish_result(
            &shared,
            WorkbookResult {
                revision: 1,
                active_sheet: 0,
                values: HashMap::new(),
                recovery_error: None,
                input_error: None,
                update_kind: WorkerUpdateKind::InitialModel,
                cell_updates: 0,
                evaluation_duration: Duration::ZERO,
                recovery_package_duration: Duration::ZERO,
                recovery_journal_duration: Duration::ZERO,
            },
        );

        assert_eq!(
            shared
                .latest_result
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .revision,
            2
        );
    }

    #[test]
    fn worker_evaluates_replacement_then_a_cell_delta() {
        let temporary = ScratchDirectory::new();
        let (worker, startup) =
            WorkbookWorker::start_at(temporary.path(), "loom.sheets/1").expect("start worker");
        assert!(startup.recovery_error.is_none());
        let mut sheet = Sheet::new("worker");
        sheet.set_str("A1", "2");
        sheet.set_str("B1", "=A1+3");
        worker
            .submit_replacement(1, 0, vec![sheet])
            .expect("send initial workbook");
        let first = worker.wait_for_result(1).expect("first result");
        assert_eq!(value(&first, "B1"), &Value::Number(5.0));

        worker
            .submit_cell(CellUpdate {
                revision: 2,
                active_sheet: 0,
                sheet: 0,
                cell: CellRef::parse("A1").unwrap(),
                raw: Some("4".to_string()),
            })
            .expect("send cell edit");
        let second = worker.wait_for_result(2).expect("second result");
        assert_eq!(second.revision, 2);
        assert_eq!(value(&second, "B1"), &Value::Number(7.0));
        assert!(second.recovery_error.is_none());
    }

    #[test]
    fn startup_restores_the_saved_active_tab_and_worker_values_match_evaluation() {
        let temporary = ScratchDirectory::new();
        let mut source = Sheet::new("Data");
        source.set_str("B1", "4");
        let mut report = Sheet::new("Report");
        report.set_str("A1", "=Data!B1+3");
        let sheets = vec![source, report];
        let payload = workbook_package_bytes(&sheets, 1).expect("package workbook");
        let mut recovery = SnapshotRecovery::open_at(temporary.path()).expect("open recovery");
        recovery
            .record("startup fixture", payload)
            .expect("write recovered workbook");
        drop(recovery);

        let (worker, startup) =
            WorkbookWorker::start_at(temporary.path(), "loom.sheets/1").expect("start worker");
        let recovered = crate::restore_workbook_from_snapshot(
            startup
                .restored_payload
                .as_deref()
                .expect("startup recovery payload"),
        )
        .expect("restore startup workbook");
        assert_eq!(recovered.active, 1);
        let expected = evaluate_workbook(&recovered.sheets)[recovered.active].clone();
        worker
            .initialize_workbook(1, recovered.active, recovered.sheets)
            .expect("initialize restored workbook on worker");
        let result = worker.wait_for_result(1).expect("initial values");

        assert_eq!(result.active_sheet, recovered.active);
        assert_eq!(result.values, expected);
        assert_eq!(
            result.values.get(&CellRef::parse("A1").unwrap()),
            Some(&Value::Number(7.0))
        );
    }

    #[test]
    fn initialize_workbook_returns_worker_owned_ui_copy_and_calculates_values() {
        let temporary = ScratchDirectory::new();
        let (worker, startup) =
            WorkbookWorker::start_at(temporary.path(), "loom.sheets/1").expect("worker");
        assert!(startup.recovery_error.is_none());

        let mut sheet = Sheet::new("initialized");
        sheet.set_str("A1", "6");
        sheet.set_str("B1", "=A1+4");
        let expected_values = evaluate_workbook(std::slice::from_ref(&sheet))[0].clone();
        let model = worker
            .initialize_workbook(1, 0, vec![sheet])
            .expect("initialize workbook on worker");
        let result = worker.wait_for_result(1).expect("initial calculation");

        assert_eq!(model.active_sheet, 0);
        assert_eq!(model.sheets.len(), 1);
        assert_eq!(
            model.sheets[0].raw(CellRef::parse("B1").unwrap()),
            Some("=A1+4")
        );
        assert_eq!(result.values, expected_values);
        assert_eq!(value(&result, "B1"), &Value::Number(10.0));
    }

    #[test]
    fn save_checkpoint_flushes_pending_work_and_compacts_recovery() {
        let temporary = ScratchDirectory::new();
        let recovery_path = temporary.path();
        let (worker, _) =
            WorkbookWorker::start_at(recovery_path.clone(), "loom.sheets/1").expect("worker");
        let mut sheet = Sheet::new("checkpoint");
        sheet.set_str("A1", "12");
        let payload = workbook_package_bytes(std::slice::from_ref(&sheet), 0)
            .expect("saved workbook package");

        worker
            .submit_replacement(1, 0, vec![sheet])
            .expect("queue pending workbook");
        worker
            .checkpoint(payload.clone())
            .expect("worker checkpoint");

        let mut recovery = SnapshotRecovery::open_at(&recovery_path).expect("reopen recovery");
        assert_eq!(recovery.take_restored_payload(), Some(payload));
    }

    #[test]
    fn worker_reports_recovery_failure_and_keeps_evaluated_values() {
        let temporary = ScratchDirectory::new();
        let recovery_path = temporary.path();
        let (worker, startup) =
            WorkbookWorker::start_at(recovery_path.clone(), "loom.sheets/1").expect("start worker");
        assert!(startup.recovery_error.is_none());
        fs::remove_dir_all(&recovery_path).expect("remove recovery directory");
        fs::write(&recovery_path, "block recovery directory").expect("replace with a file");

        let mut sheet = Sheet::new("in memory");
        sheet.set_str("A1", "7");
        sheet.set_str("B1", "=A1+1");
        worker
            .submit_replacement(1, 0, vec![sheet])
            .expect("send workbook");
        let result = worker.wait_for_result(1).expect("calculated result");

        assert_eq!(value(&result, "B1"), &Value::Number(8.0));
        assert!(result.recovery_error.is_some());
    }

    #[test]
    fn shutdown_drains_the_last_pending_edit_to_recovery() {
        let temporary = ScratchDirectory::new();
        let recovery_path = temporary.path();
        let (worker, _) =
            WorkbookWorker::start_at(recovery_path.clone(), "loom.sheets/1").expect("start worker");
        let mut sheet = Sheet::new("shutdown");
        sheet.set_str("A1", "2");
        sheet.set_str("B1", "=A1+1");
        worker
            .submit_replacement(1, 0, vec![sheet])
            .expect("send initial workbook");
        worker
            .submit_cell(CellUpdate {
                revision: 2,
                active_sheet: 0,
                sheet: 0,
                cell: CellRef::parse("A1").unwrap(),
                raw: Some("9".to_string()),
            })
            .expect("send pending edit");
        drop(worker);

        let mut recovery = SnapshotRecovery::open_at(&recovery_path).expect("open recovery");
        let payload = recovery
            .take_restored_payload()
            .expect("shutdown edit was journaled");
        let workbook = crate::restore_workbook_from_snapshot(&payload).expect("recover workbook");
        assert_eq!(
            workbook.sheets[0].raw(CellRef::parse("A1").unwrap()),
            Some("9")
        );
        let values = evaluate_workbook(&workbook.sheets);
        assert_eq!(
            values[0].get(&CellRef::parse("B1").unwrap()),
            Some(&Value::Number(10.0))
        );
    }
}
