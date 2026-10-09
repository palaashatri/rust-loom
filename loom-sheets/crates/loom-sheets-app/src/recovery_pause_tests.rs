//! Recovery admission pause, checkpoint cadence, retry, Save As, write-failure
//! injection at every recovery step, and restart comparison.

use super::*;
use crate::cell_edit_recovery::{versioned_directory_for, CellEditRecovery};
use crate::recovery_policy::CadenceLimits;
use crate::workbook_io::workbook_package_bytes;
use loom_production::fault_injection::{self, FaultStep};
use loom_production::RecoveryJournal;
use loom_sheets_core::workbook_to_json;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "loom-sheets-pause-{}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create scratch recovery base");
        Self(path)
    }

    fn base(&self) -> PathBuf {
        self.0.join("recovery")
    }

    fn versioned(&self) -> PathBuf {
        versioned_directory_for(&self.base()).expect("versioned recovery directory")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        fault_injection::clear(&versioned_directory_for(&self.0.join("recovery")).unwrap());
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn cell_ref(address: &str) -> CellRef {
    CellRef::parse(address).expect("cell address")
}

const PNG: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];

/// Two tabs, a formula, an embedded image, and the second tab active.
fn workbook() -> (Vec<Sheet>, usize) {
    let mut data = Sheet::new("Data");
    data.set_str("A1", "8");
    data.set_str("B1", "=A1*2");
    let mut image =
        loom_sheets_core::SheetObject::image(cell_ref("D1"), "chart.png").expect("image object");
    image.embedded = Some(PNG.to_vec());
    data.objects.push(image);
    let mut report = Sheet::new("Report");
    report.set_str("A1", "=Data!B1+1");
    (vec![data, report], 1)
}

/// Compare workbooks as the package round trip sees them (it assigns image
/// asset paths), including embedded image bytes and the active tab.
fn fingerprint(sheets: &[Sheet], active: usize) -> String {
    let package = workbook_package_bytes(sheets, active).expect("package workbook");
    let restored = crate::restore_workbook_from_snapshot(&package).expect("restore workbook");
    let (sheets, active) = (restored.sheets, restored.active);
    let images = sheets
        .iter()
        .flat_map(|sheet| sheet.objects.iter().map(|object| object.embedded.clone()))
        .collect::<Vec<_>>();
    format!("{}|{images:?}", workbook_to_json(&sheets, active))
}

fn edit(sheets: &mut [Sheet], address: &str, raw: &str) -> (usize, CellRef, Option<String>) {
    sheets[0].set_raw(cell_ref(address), raw.to_string());
    (0, cell_ref(address), Some(raw.to_string()))
}

fn cell_update(revision: u64, address: &str, raw: &str) -> CellUpdate {
    CellUpdate {
        revision,
        active_sheet: 1,
        sheet: 0,
        cell: cell_ref(address),
        raw: Some(raw.to_string()),
    }
}

/// Recover what a fresh process would see, as a comparable fingerprint.
fn recovered_fingerprint(base: &Path) -> String {
    let (_recovery, restored) = CellEditRecovery::open_at(base).expect("restart opens recovery");
    let payload = restored.expect("restart restores a workbook");
    let workbook = crate::restore_workbook_from_snapshot(&payload).expect("decode workbook");
    fingerprint(&workbook.sheets, workbook.active)
}

fn journal_state(versioned: &Path) -> loom_production::RecoveryState {
    RecoveryJournal::open(versioned)
        .expect("open journal for inspection")
        .recover()
        .expect("read recovery state")
}

fn limits(records: u64, bytes: u64, age: Duration) -> CadenceLimits {
    CadenceLimits {
        bytes,
        records,
        age,
    }
}

fn start_with_cadence(scratch: &Scratch, limits: CadenceLimits) -> WorkbookWorker {
    let (worker, startup) =
        WorkbookWorker::start_at_with_cadence(scratch.base(), "loom.sheets/1", limits)
            .expect("start worker");
    assert!(startup.recovery_error.is_none());
    worker
}

fn initialize(worker: &WorkbookWorker, sheets: &[Sheet], active: usize) {
    worker
        .initialize_workbook(1, active, sheets.to_vec())
        .expect("initialize workbook");
    let result = worker.wait_for_result(1).expect("initial result");
    assert!(result.recovery_error.is_none());
    assert!(worker.recovery_pause().is_none());
}

const NEVER: Duration = Duration::from_secs(3600);

#[test]
fn cadence_record_count_checkpoints_and_compacts_only_covered_records() {
    let scratch = Scratch::new();
    let worker = start_with_cadence(&scratch, limits(3, u64::MAX, NEVER));
    let (mut sheets, active) = workbook();
    initialize(&worker, &sheets, active);

    for (revision, address) in [(2, "A2"), (3, "A3")] {
        edit(&mut sheets, address, "x");
        worker
            .submit_cell(cell_update(revision, address, "x"))
            .unwrap();
        worker.wait_for_result(revision).expect("edit result");
    }
    assert_eq!(
        journal_state(&scratch.versioned()).operations.len(),
        2,
        "below the trigger the edits stay in the journal"
    );

    edit(&mut sheets, "A4", "x");
    worker.submit_cell(cell_update(4, "A4", "x")).unwrap();
    worker.wait_for_result(4).expect("third edit result");
    let state = journal_state(&scratch.versioned());
    assert!(state.operations.is_empty(), "covered records are compacted");
    assert_eq!(state.checkpoint_metadata.unwrap().last_sequence, 3);

    edit(&mut sheets, "A5", "newer");
    worker.submit_cell(cell_update(5, "A5", "newer")).unwrap();
    worker.wait_for_result(5).expect("newer edit result");
    assert_eq!(
        journal_state(&scratch.versioned()).operations.len(),
        1,
        "an edit after the checkpoint is kept, not compacted away"
    );
    assert!(worker.recovery_pause().is_none());
    drop(worker);
    assert_eq!(
        recovered_fingerprint(&scratch.base()),
        fingerprint(&sheets, active)
    );
}

#[test]
fn cadence_journal_byte_limit_triggers_a_checkpoint() {
    let scratch = Scratch::new();
    let worker = start_with_cadence(&scratch, limits(u64::MAX, 1, NEVER));
    let (mut sheets, active) = workbook();
    initialize(&worker, &sheets, active);

    edit(&mut sheets, "A2", "big enough");
    worker
        .submit_cell(cell_update(2, "A2", "big enough"))
        .unwrap();
    worker.wait_for_result(2).expect("edit result");

    let state = journal_state(&scratch.versioned());
    assert!(state.operations.is_empty());
    assert_eq!(state.checkpoint_metadata.unwrap().last_sequence, 1);
    drop(worker);
    assert_eq!(
        recovered_fingerprint(&scratch.base()),
        fingerprint(&sheets, active)
    );
}

#[test]
fn cadence_age_checkpoints_an_idle_worker() {
    let scratch = Scratch::new();
    let worker = start_with_cadence(
        &scratch,
        limits(u64::MAX, u64::MAX, Duration::from_millis(150)),
    );
    let (mut sheets, active) = workbook();
    initialize(&worker, &sheets, active);

    edit(&mut sheets, "A2", "idle edit");
    worker
        .submit_cell(cell_update(2, "A2", "idle edit"))
        .unwrap();
    worker.wait_for_result(2).expect("edit result");

    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let state = journal_state(&scratch.versioned());
        if state.operations.is_empty()
            && state
                .checkpoint_metadata
                .as_ref()
                .is_some_and(|metadata| metadata.last_sequence == 1)
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "an idle worker must checkpoint once the oldest uncovered edit is old enough"
        );
        thread::sleep(Duration::from_millis(50));
    }
    assert!(worker.recovery_pause().is_none());
    drop(worker);
    assert_eq!(
        recovered_fingerprint(&scratch.base()),
        fingerprint(&sheets, active)
    );
}

#[test]
fn journal_failure_pauses_recovery_and_only_a_covering_checkpoint_clears_it() {
    let scratch = Scratch::new();
    let worker = start_with_cadence(&scratch, limits(u64::MAX, u64::MAX, NEVER));
    let (mut sheets, active) = workbook();
    initialize(&worker, &sheets, active);

    edit(&mut sheets, "A2", "accepted");
    worker
        .submit_cell(cell_update(2, "A2", "accepted"))
        .unwrap();
    assert!(worker.wait_for_result(2).unwrap().recovery_error.is_none());
    let accepted = fingerprint(&sheets, active);

    fault_injection::inject(&scratch.versioned(), FaultStep::JournalAppend, 0, 1);
    edit(&mut sheets, "A3", "lost until retry");
    worker
        .submit_cell(cell_update(3, "A3", "lost until retry"))
        .unwrap();
    let failed = worker.wait_for_result(3).unwrap();
    assert!(failed.recovery_error.is_some());
    assert!(
        worker.recovery_pause().is_some(),
        "edit admission must pause"
    );

    // The injected fault is spent, yet the latch holds: a later edit must not
    // be journaled behind a gap, so it cannot be reported as protected.
    edit(&mut sheets, "A4", "also unprotected");
    worker
        .submit_cell(cell_update(4, "A4", "also unprotected"))
        .unwrap();
    assert!(worker.wait_for_result(4).unwrap().recovery_error.is_some());
    assert!(worker.recovery_pause().is_some());
    assert_eq!(
        journal_state(&scratch.versioned()).operations.len(),
        1,
        "only the edit that was acknowledged is on disk"
    );

    // A retry whose checkpoint fails again must not clear the pause.
    fault_injection::inject(&scratch.versioned(), FaultStep::CheckpointPayload, 0, 1);
    worker
        .submit_replacement(5, active, sheets.clone())
        .unwrap();
    assert!(worker.wait_for_result(5).unwrap().recovery_error.is_some());
    assert!(worker.recovery_pause().is_some());

    worker
        .submit_replacement(6, active, sheets.clone())
        .unwrap();
    let healed = worker.wait_for_result(6).unwrap();
    assert!(healed.recovery_error.is_none());
    assert!(worker.recovery_pause().is_none());
    drop(worker);
    let restarted = recovered_fingerprint(&scratch.base());
    assert_eq!(restarted, fingerprint(&sheets, active));
    assert_ne!(restarted, accepted);
}

#[test]
fn disk_full_during_a_cadence_checkpoint_keeps_the_edit_journaled_and_pauses() {
    let scratch = Scratch::new();
    let worker = start_with_cadence(&scratch, limits(2, u64::MAX, NEVER));
    let (mut sheets, active) = workbook();
    initialize(&worker, &sheets, active);

    edit(&mut sheets, "A2", "one");
    worker.submit_cell(cell_update(2, "A2", "one")).unwrap();
    worker.wait_for_result(2).unwrap();
    fault_injection::inject(&scratch.versioned(), FaultStep::CheckpointPayload, 0, 1);
    edit(&mut sheets, "A3", "two");
    worker.submit_cell(cell_update(3, "A3", "two")).unwrap();
    let result = worker.wait_for_result(3).unwrap();

    assert!(result.recovery_error.is_some());
    assert!(worker.recovery_pause().is_some());
    assert_eq!(
        journal_state(&scratch.versioned()).operations.len(),
        2,
        "both edits stay durable in the journal when the checkpoint cannot be written"
    );
    drop(worker);
    assert_eq!(
        recovered_fingerprint(&scratch.base()),
        fingerprint(&sheets, active)
    );
}

#[test]
fn save_as_clears_the_pause_only_when_its_checkpoint_really_succeeds() {
    let scratch = Scratch::new();
    let (save_tx, save_rx) = std::sync::mpsc::channel();
    let (worker, startup) =
        WorkbookWorker::start_at_with_completions(scratch.base(), "loom.sheets/1", save_tx)
            .expect("start worker");
    assert!(startup.recovery_error.is_none());
    let (mut sheets, active) = workbook();
    initialize(&worker, &sheets, active);

    fault_injection::inject(&scratch.versioned(), FaultStep::JournalAppend, 0, 1);
    edit(&mut sheets, "A2", "unprotected");
    worker
        .submit_cell(cell_update(2, "A2", "unprotected"))
        .unwrap();
    worker.wait_for_result(2).unwrap();
    assert!(worker.recovery_pause().is_some());

    // Save As reaches the user's file even though recovery is down, but its
    // recovery checkpoint fails too, so the pause must stay.
    fault_injection::inject(&scratch.versioned(), FaultStep::CheckpointPayload, 0, 1);
    let first_path = scratch.0.join("first.loomtable");
    worker
        .queue_save(1, 1, 2, None, first_path.clone())
        .unwrap();
    let first = save_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(first.write_result.is_ok());
    assert!(first
        .checkpoint_result
        .is_some_and(|result| result.is_err()));
    assert!(first_path.exists());
    assert!(worker.recovery_pause().is_some());

    let second_path = scratch.0.join("second.loomtable");
    worker.queue_save(2, 1, 2, None, second_path).unwrap();
    let second = save_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    assert_eq!(second.checkpoint_result, Some(Ok(())));
    assert!(worker.recovery_pause().is_none());
    drop(worker);
    assert_eq!(
        recovered_fingerprint(&scratch.base()),
        fingerprint(&sheets, active)
    );
}

/// Run `record_cells` or a checkpoint with `step` failing once, restart, and
/// compare against the last state the writer acknowledged.
fn run_fault_scenario(step: FaultStep) {
    let scratch = Scratch::new();
    let base = scratch.base();
    let versioned = scratch.versioned();
    let (mut sheets, active) = workbook();
    let (mut recovery, restored) = CellEditRecovery::open_at(&base).expect("open recovery");
    assert!(restored.is_none());
    recovery
        .checkpoint_package(workbook_package_bytes(&sheets, active).unwrap(), false)
        .expect("baseline checkpoint");
    let accepted_edit = edit(&mut sheets, "A2", "accepted edit");
    recovery
        .record_cells(active, [accepted_edit], || {
            workbook_package_bytes(&sheets, active)
        })
        .expect("first edit is acknowledged");
    let acknowledged = fingerprint(&sheets, active);

    fault_injection::inject(&versioned, step, 0, 1);
    let failing_edit = edit(&mut sheets, "A3", "failing edit");
    let expected = match step {
        FaultStep::JournalAppend => {
            let result = recovery.record_cells(active, [failing_edit], || {
                workbook_package_bytes(&sheets, active)
            });
            assert!(
                result.is_err(),
                "{step:?}: a failed append must be reported"
            );
            acknowledged
        }
        FaultStep::CheckpointPayload
        | FaultStep::CheckpointMetadata
        | FaultStep::CheckpointPointer => {
            let package = workbook_package_bytes(&sheets, active).unwrap();
            assert!(
                recovery.checkpoint_package(package, false).is_err(),
                "{step:?}: a failed checkpoint must be reported"
            );
            // The previous generation and its journal must still be intact.
            acknowledged
        }
        FaultStep::JournalRewrite => {
            let package = workbook_package_bytes(&sheets, active).unwrap();
            assert!(
                recovery.checkpoint_package(package, false).is_err(),
                "{step:?}: a failed compaction must be reported"
            );
            // The new pointer was already published, so it is authoritative and
            // the stale covered records must not replay on top of it.
            fingerprint(&sheets, active)
        }
    };
    assert!(
        fault_injection::hits(&versioned, step) >= 1,
        "{step:?}: the injection point was never reached"
    );
    assert!(recovery.failure().is_some(), "{step:?}: failure must latch");
    drop(recovery);
    fault_injection::clear(&versioned);

    assert_eq!(
        recovered_fingerprint(&base),
        expected,
        "{step:?}: restart must match the last acknowledged workbook"
    );

    // Recovery is usable again after the fault clears.
    let (mut reopened, _) = CellEditRecovery::open_at(&base).expect("reopen after fault");
    let package = workbook_package_bytes(&sheets, active).unwrap();
    reopened
        .checkpoint_package(package, false)
        .expect("a fresh checkpoint succeeds once the disk recovers");
    assert!(reopened.failure().is_none());
    drop(reopened);
    assert_eq!(recovered_fingerprint(&base), fingerprint(&sheets, active));
}

#[test]
fn write_failure_at_journal_append_loses_nothing_acknowledged() {
    run_fault_scenario(FaultStep::JournalAppend);
}

#[test]
fn write_failure_at_checkpoint_payload_keeps_the_previous_generation() {
    run_fault_scenario(FaultStep::CheckpointPayload);
}

#[test]
fn write_failure_at_checkpoint_metadata_keeps_the_previous_generation() {
    run_fault_scenario(FaultStep::CheckpointMetadata);
}

#[test]
fn write_failure_at_pointer_replacement_keeps_the_previous_generation() {
    run_fault_scenario(FaultStep::CheckpointPointer);
}

#[test]
fn write_failure_at_journal_compaction_keeps_the_published_checkpoint() {
    run_fault_scenario(FaultStep::JournalRewrite);
}

#[test]
fn later_write_failures_after_earlier_successes_are_also_reported() {
    let scratch = Scratch::new();
    let versioned = scratch.versioned();
    let (mut sheets, active) = workbook();
    let (mut recovery, _) = CellEditRecovery::open_at(scratch.base()).unwrap();
    recovery
        .checkpoint_package(workbook_package_bytes(&sheets, active).unwrap(), false)
        .unwrap();
    // Let two appends through, then fail the third.
    fault_injection::inject(&versioned, FaultStep::JournalAppend, 2, 1);
    for (index, address) in ["B2", "B3", "B4"].into_iter().enumerate() {
        let change = edit(&mut sheets, address, "v");
        let result =
            recovery.record_cells(active, [change], || workbook_package_bytes(&sheets, active));
        assert_eq!(result.is_err(), index == 2, "append {index}");
    }
    drop(recovery);
    fault_injection::clear(&versioned);
    let mut acknowledged = sheets.clone();
    acknowledged[0].clear_cell(cell_ref("B4"));
    assert_eq!(
        recovered_fingerprint(&scratch.base()),
        fingerprint(&acknowledged, active)
    );
}

fn recovery_with_three_edits(scratch: &Scratch) {
    let (mut sheets, active) = workbook();
    let (mut recovery, _) = CellEditRecovery::open_at(scratch.base()).unwrap();
    recovery
        .checkpoint_package(workbook_package_bytes(&sheets, active).unwrap(), false)
        .unwrap();
    for address in ["C1", "C2", "C3"] {
        let change = edit(&mut sheets, address, "v");
        recovery
            .record_cells(active, [change], || workbook_package_bytes(&sheets, active))
            .unwrap();
    }
}

#[test]
fn a_damaged_journal_record_fails_closed_without_rewriting_recovery() {
    let scratch = Scratch::new();
    recovery_with_three_edits(&scratch);
    let journal = scratch.versioned().join("operations.jsonl");
    let original = fs::read_to_string(&journal).unwrap();
    let mut lines = original.lines().map(str::to_owned).collect::<Vec<_>>();
    assert_eq!(lines.len(), 3);
    lines[1] = lines[1].replacen("\"payload_sha256\":\"", "\"payload_sha256\":\"0", 1);
    let damaged = format!("{}\n", lines.join("\n"));
    fs::write(&journal, &damaged).unwrap();

    assert!(
        CellEditRecovery::open_at(scratch.base()).is_err(),
        "a damaged middle record must stop recovery, not silently drop edits"
    );
    assert_eq!(
        fs::read_to_string(&journal).unwrap(),
        damaged,
        "failing closed must not repair or delete recovery data"
    );
}

#[test]
fn a_damaged_checkpoint_payload_fails_closed_without_rewriting_recovery() {
    let scratch = Scratch::new();
    recovery_with_three_edits(&scratch);
    let payload = fs::read_dir(scratch.versioned())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    name.starts_with("checkpoint-generation-") && name.ends_with(".bin")
                })
        })
        .expect("checkpoint payload file");
    let mut bytes = fs::read(&payload).unwrap();
    let middle = bytes.len() / 2;
    bytes[middle] ^= 0xFF;
    fs::write(&payload, &bytes).unwrap();

    assert!(CellEditRecovery::open_at(scratch.base()).is_err());
    assert_eq!(fs::read(&payload).unwrap(), bytes);
}

#[test]
fn an_idle_checkpoint_never_overwrites_recovery_with_an_unconfirmed_import() {
    let scratch = Scratch::new();
    let (mut sheets, active) = workbook();
    {
        let worker = start_with_cadence(&scratch, limits(u64::MAX, u64::MAX, NEVER));
        initialize(&worker, &sheets, active);
        edit(&mut sheets, "A2", "unsaved work worth keeping");
        worker
            .submit_cell(cell_update(2, "A2", "unsaved work worth keeping"))
            .unwrap();
        worker.wait_for_result(2).unwrap();
    }
    let protected = fingerprint(&sheets, active);

    // The next session restores that work, then a startup import is staged for
    // confirmation without replacing recovery. The age trigger fires while the
    // user is still deciding.
    let worker = start_with_cadence(
        &scratch,
        limits(u64::MAX, u64::MAX, Duration::from_millis(100)),
    );
    let mut staged = Sheet::new("Staged import");
    staged.set_str("A1", "not confirmed yet");
    worker
        .initialize_workbook_without_recovery(1, 0, vec![staged])
        .expect("stage import");
    worker.wait_for_result(1).unwrap();
    thread::sleep(Duration::from_millis(700));
    drop(worker);

    assert_eq!(recovered_fingerprint(&scratch.base()), protected);
}
