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
            dirty: true,
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
            dirty: true,
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
fn initializing_a_cancelable_startup_import_does_not_replace_recovery() {
    let temporary = ScratchDirectory::new();
    let recovery_dir = temporary.path();
    let mut previous = Sheet::new("Recovered");
    previous.set_str("A1", "keep this recovery");
    let previous_payload = workbook_package_bytes(std::slice::from_ref(&previous), 0)
        .expect("package previous recovery");
    let mut recovery = SnapshotRecovery::open_at(&recovery_dir).expect("open recovery");
    recovery
        .record("previous workbook", previous_payload.clone())
        .expect("write previous recovery");
    drop(recovery);

    let (worker, startup) =
        WorkbookWorker::start_at(recovery_dir.clone(), "loom.sheets/1").expect("start worker");
    assert_eq!(startup.restored_payload, Some(previous_payload.clone()));
    worker
        .initialize_workbook_without_recovery(1, 0, vec![Sheet::new("Untitled")])
        .expect("show blank workbook until import confirmation");
    worker.wait_for_result(1).expect("blank initial values");
    drop(worker);

    let mut reopened = SnapshotRecovery::open_at(&recovery_dir).expect("reopen recovery");
    assert_eq!(
        reopened.take_restored_payload(),
        Some(previous_payload),
        "cancelling startup import must not replace recoverable work"
    );
}

#[test]
fn save_barrier_flushes_pending_work_and_compacts_recovery() {
    let temporary = ScratchDirectory::new();
    let recovery_path = temporary.path();
    let (save_sender, save_receiver) = std::sync::mpsc::channel();
    let (worker, _) = WorkbookWorker::start_at_with_completions(
        recovery_path.clone(),
        "loom.sheets/1",
        save_sender,
    )
    .expect("worker");
    let mut sheet = Sheet::new("checkpoint");
    sheet.set_str("A1", "12");
    let payload =
        workbook_package_bytes(std::slice::from_ref(&sheet), 0).expect("saved workbook package");

    worker
        .submit_replacement(1, 0, vec![sheet])
        .expect("queue pending workbook");
    worker
        .queue_save(1, 1, 1, None, recovery_path.join("saved.loomtable"))
        .expect("queue save barrier");
    let completion = save_receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("save completion");
    assert!(completion.write_result.is_ok());
    assert_eq!(completion.checkpoint_result, Some(Ok(())));
    let (baseline, active_sheet) = completion.baseline.expect("saved baseline");
    assert_eq!(active_sheet, 0);
    assert_eq!(baseline[0].raw(CellRef::parse("A1").unwrap()), Some("12"));

    let mut recovery = SnapshotRecovery::open_at(&recovery_path).expect("reopen recovery");
    assert_eq!(recovery.take_restored_payload(), Some(payload));
}

#[test]
fn checkpoint_at_revision_n_preserves_later_n_plus_one_recovery() {
    let temporary = ScratchDirectory::new();
    let recovery_path = temporary.path();
    let (save_sender, save_receiver) = std::sync::mpsc::channel();
    let (worker, _) = WorkbookWorker::start_at_with_completions(
        recovery_path.clone(),
        "loom.sheets/1",
        save_sender,
    )
    .expect("worker");
    let a1 = CellRef::parse("A1").unwrap();
    let mut saved_n = Sheet::new("checkpoint ordering");
    saved_n.set_str("A1", "saved N");
    worker
        .initialize_workbook(1, 0, vec![saved_n.clone()])
        .expect("initialize revision N");
    worker.wait_for_result(1).expect("revision N recovery");

    worker
        .queue_save(1, 1, 1, None, temporary.path().join("saved-n.loomtable"))
        .expect("queue checkpoint revision N");
    worker
        .submit_cell(CellUpdate {
            revision: 2,
            active_sheet: 0,
            sheet: 0,
            cell: a1,
            raw: Some("unsaved N+1".to_string()),
        })
        .expect("queue revision N+1 after save barrier");
    let completion = save_receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("revision N save completion");
    assert!(completion.write_result.is_ok());
    assert_eq!(completion.checkpoint_result, Some(Ok(())));
    worker.wait_for_result(2).expect("revision N+1 recovery");
    drop(worker);

    let mut recovery = SnapshotRecovery::open_at(&recovery_path).expect("reopen recovery");
    let payload = recovery
        .take_restored_payload()
        .expect("revision N+1 recovery payload");
    let workbook = crate::restore_workbook_from_snapshot(&payload).expect("restore workbook");
    assert_eq!(
        workbook.sheets[0].raw(a1),
        Some("unsaved N+1"),
        "checkpoint N must not erase recovery for N+1"
    );
}

#[test]
fn save_reports_native_file_write_failure_without_checkpointing() {
    let temporary = ScratchDirectory::new();
    let recovery_path = temporary.path();
    let (save_sender, save_receiver) = std::sync::mpsc::channel();
    let (worker, _) = WorkbookWorker::start_at_with_completions(
        recovery_path.clone(),
        "loom.sheets/1",
        save_sender,
    )
    .expect("worker");
    let mut sheet = Sheet::new("write failure");
    sheet.set_str("A1", "saved data");
    worker
        .initialize_workbook(1, 0, vec![sheet])
        .expect("initialize workbook");
    worker.wait_for_result(1).expect("initial result");
    worker
        .queue_save(1, 1, 1, None, recovery_path.clone())
        .expect("queue save to a directory path");

    let completion = save_receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("failed save completion");
    assert!(completion.write_result.is_err());
    assert!(completion.checkpoint_result.is_none());
    assert!(completion.baseline.is_none());
}

#[test]
fn save_reports_recovery_checkpoint_failure_after_native_write() {
    let temporary = ScratchDirectory::new();
    let recovery_path = temporary.path();
    let (save_sender, save_receiver) = std::sync::mpsc::channel();
    let (worker, _) = WorkbookWorker::start_at_with_completions(
        recovery_path.clone(),
        "loom.sheets/1",
        save_sender,
    )
    .expect("worker");
    let mut sheet = Sheet::new("recovery failure");
    sheet.set_str("A1", "saved data");
    worker
        .initialize_workbook(1, 0, vec![sheet])
        .expect("initialize workbook");
    worker.wait_for_result(1).expect("initial result");

    fs::remove_dir_all(&recovery_path).expect("remove recovery directory");
    fs::write(&recovery_path, "block recovery directory").expect("block recovery directory");
    let saved_path = recovery_path.with_extension("loomtable");
    worker
        .queue_save(1, 1, 1, None, saved_path.clone())
        .expect("queue save with broken recovery writer");

    let completion = save_receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("save completion");
    assert!(completion.write_result.is_ok());
    assert!(completion
        .checkpoint_result
        .is_some_and(|result| result.is_err()));
    assert!(completion.baseline.is_some());
    assert!(saved_path.exists());
    let _ = fs::remove_file(saved_path);
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
