use crate::cell_edit_recovery::{
    parse_checkpoint_identity, read_legacy_records_after_with_package_limit,
    versioned_directory_for, BoundedLegacyPackage, CellAssignment, CellEditBatch, CellEditRecovery,
    RECORD_VERSION,
};
use crate::workbook_io::workbook_package_bytes;
use crate::workbook_worker::WorkbookWorker;
use fs2::FileExt;
use loom_production::snapshot::SnapshotRecovery;
use loom_production::{JournalRecord, RecoveryJournal};
use loom_sheets_core::{CellRef, Sheet};
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

struct RecoveryFixture(PathBuf);

impl RecoveryFixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "loom-sheets-edit-recovery-{}-{}",
            std::process::id(),
            NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create recovery fixture");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for RecoveryFixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
        if let Ok(versioned) = versioned_directory_for(&self.0) {
            let _ = fs::remove_dir_all(versioned);
        }
    }
}

fn checkpoint_fixture(base: &Path) -> Vec<u8> {
    let mut sheet = Sheet::new("Data");
    sheet.set_str("A1", "baseline");
    sheet.set_str("B1", "=A1+1");
    let package = workbook_package_bytes(&[sheet], 0).expect("baseline workbook package");
    let (mut recovery, restored) =
        CellEditRecovery::open_at(base).expect("open empty versioned recovery");
    assert!(restored.is_none());
    recovery
        .checkpoint_package(package.clone(), false)
        .expect("write baseline checkpoint");
    drop(recovery);
    package
}

fn append_batch(
    journal: &mut RecoveryJournal,
    identity: &crate::cell_edit_recovery::RecoveryIdentity,
    predecessor_sequence: u64,
    raw: &str,
) -> JournalRecord {
    let batch = CellEditBatch {
        format_version: RECORD_VERSION,
        session_id: identity.session_id.clone(),
        workbook_id: identity.workbook_id.clone(),
        baseline_id: identity.baseline_id.clone(),
        predecessor_sequence,
        active_sheet: 0,
        edits: vec![CellAssignment {
            sheet: 0,
            row: 0,
            col: 0,
            raw: Some(raw.to_string()),
        }],
    };
    journal
        .append(
            format!("test-{predecessor_sequence}"),
            "test edit",
            serde_json::to_vec(&batch).expect("encode test batch"),
        )
        .expect("append test batch")
}

fn package_with_recovery_content() -> Vec<u8> {
    let mut data = Sheet::new("Data");
    data.set_str("A1", "8");
    data.set_str("B1", "=A1*2");
    let mut image = loom_sheets_core::SheetObject::image(
        CellRef::parse("D1").expect("image anchor"),
        "chart.png",
    )
    .expect("image object");
    image.embedded = Some(vec![137, 80, 78, 71, 13, 10, 26, 10]);
    data.objects.push(image);
    let mut report = Sheet::new("Report");
    report.set_str("A1", "=Data!B1+1");
    workbook_package_bytes(&[data, report], 1).expect("complete workbook package")
}

fn legacy_entry_bytes(directory: &Path) -> BTreeMap<String, Vec<u8>> {
    fs::read_dir(directory)
        .expect("read legacy directory")
        .map(|entry| entry.expect("legacy entry"))
        .filter(|entry| entry.file_name() != ".checkpoint.lock")
        .filter(|entry| entry.file_type().expect("legacy entry type").is_file())
        .map(|entry| {
            (
                entry.file_name().to_string_lossy().into_owned(),
                fs::read(entry.path()).expect("read legacy entry bytes"),
            )
        })
        .collect()
}

fn versioned_entry_bytes(directory: &Path) -> BTreeMap<String, Vec<u8>> {
    fs::read_dir(directory)
        .expect("read versioned directory")
        .map(|entry| entry.expect("versioned entry"))
        .filter(|entry| {
            !matches!(
                entry.file_name().to_string_lossy().as_ref(),
                ".checkpoint.lock" | ".sheets-writer.lock"
            )
        })
        .filter(|entry| entry.file_type().expect("versioned entry type").is_file())
        .map(|entry| {
            (
                entry.file_name().to_string_lossy().into_owned(),
                fs::read(entry.path()).expect("read versioned entry bytes"),
            )
        })
        .collect()
}

fn seed_legacy_checkpoint(directory: &Path, package: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let mut legacy = SnapshotRecovery::open_at(directory).expect("open legacy recovery");
    legacy
        .record("legacy workbook", package.to_vec())
        .expect("record legacy workbook");
    legacy
        .checkpoint("legacy sheets/1", package.to_vec())
        .expect("checkpoint legacy workbook");
    drop(legacy);
    legacy_entry_bytes(directory)
}

#[test]
fn legacy_recovery_directory_lock_is_held_until_cell_recovery_drops() {
    let fixture = RecoveryFixture::new();
    let (recovery, _) =
        CellEditRecovery::open_at(fixture.path()).expect("open empty versioned recovery");
    let lock_file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(fixture.path().join(".checkpoint.lock"))
        .expect("open legacy recovery lock file");

    let while_open = FileExt::try_lock_exclusive(&lock_file)
        .expect_err("cell recovery retains the legacy directory lock");
    assert_eq!(
        while_open.kind(),
        std::io::ErrorKind::WouldBlock,
        "the retained lock must block another exclusive acquisition"
    );

    drop(recovery);
    FileExt::try_lock_exclusive(&lock_file)
        .expect("dropping cell recovery releases the legacy directory lock");
}

#[test]
fn over_limit_torn_journal_is_preserved_before_recovery_journal_open() {
    let fixture = RecoveryFixture::new();
    let versioned = versioned_directory_for(fixture.path()).expect("versioned directory path");
    fs::create_dir_all(&versioned).expect("create versioned recovery directory");
    let journal_path = versioned.join("operations.jsonl");
    let journal_bytes = vec![b'x'; 1024 * 1024 + 1];
    fs::write(&journal_path, &journal_bytes).expect("write over-limit torn journal tail");

    let result = CellEditRecovery::open_at(fixture.path());

    assert!(
        result.is_err(),
        "startup must refuse an over-limit journal before recovery can repair it"
    );
    assert_eq!(
        fs::read(&journal_path).expect("read preserved journal"),
        journal_bytes,
        "preflight refusal must preserve the over-limit journal bytes"
    );
}

#[test]
fn legacy_package_decoder_stops_before_collecting_bytes_over_its_limit() {
    let accepted = serde_json::from_str::<BoundedLegacyPackage<4>>("[0,1,2,3]");
    assert!(accepted.is_ok(), "the exact package limit must be accepted");

    let error = serde_json::from_str::<BoundedLegacyPackage<4>>("[0,1,2,3,4]")
        .expect_err("decoder must reject the first byte over the package limit");

    assert!(error.to_string().contains("4 byte limit"));
}

#[test]
fn oversized_final_legacy_record_is_not_tolerated_as_a_torn_tail() {
    let line = serde_json::to_vec(&serde_json::json!({
        "sequence": 1,
        "operation_id": "oversized-final-record",
        "label": "edit",
        "payload": [0, 1, 2, 3, 4],
        "timestamp_ms": 1,
        "payload_sha256": "unused-because-decoding-must-fail"
    }))
    .expect("encode oversized committed record");
    let mut committed_bytes = line;
    committed_bytes.push(b'\n');
    let fixture = RecoveryFixture::new();
    let journal_path = fixture.path().join("operations.jsonl");
    fs::write(&journal_path, &committed_bytes).expect("write oversized legacy journal record");

    let error = read_legacy_records_after_with_package_limit::<4>(fixture.path(), 0)
        .expect_err("an oversized final committed record must fail closed");

    assert!(error.contains("4 byte limit"), "unexpected error: {error}");
    assert_eq!(
        fs::read(&journal_path).expect("read preserved legacy journal"),
        committed_bytes,
        "refusing an oversized record must not alter legacy recovery data"
    );
}

#[test]
fn malformed_final_legacy_record_remains_tolerated_as_a_torn_tail() {
    let fixture = RecoveryFixture::new();
    let journal_path = fixture.path().join("operations.jsonl");
    let committed_bytes = b"{malformed final record}\n";
    fs::write(&journal_path, committed_bytes).expect("write malformed final journal record");

    let result = read_legacy_records_after_with_package_limit::<4>(fixture.path(), 0)
        .expect("malformed final JSON retains existing torn-tail behavior");

    assert!(result.is_none());
    assert_eq!(
        fs::read(&journal_path).expect("read unchanged legacy journal"),
        committed_bytes,
        "torn-tail tolerance is read-only until an explicit migration succeeds"
    );
}

#[cfg(unix)]
#[test]
fn symlinked_legacy_recovery_root_is_rejected_before_creating_locks() {
    use std::os::unix::fs::symlink;

    let fixture = RecoveryFixture::new();
    let target = fixture.path().join("target");
    fs::create_dir(&target).expect("create symlink target");
    let legacy_link = fixture.path().join("legacy-link");
    symlink(&target, &legacy_link).expect("create legacy recovery symlink");
    let versioned = versioned_directory_for(&legacy_link).expect("versioned directory path");

    let result = CellEditRecovery::open_at(&legacy_link);

    assert!(result.is_err(), "symlinked recovery roots must fail closed");
    assert!(
        !target.join(".checkpoint.lock").exists(),
        "path validation must run before a lock is created through the symlink"
    );
    assert!(
        !versioned.exists(),
        "path validation must run before creating a sibling recovery directory"
    );
}

#[test]
fn pending_migration_receipt_keeps_legacy_files_when_replay_fails() {
    let fixture = RecoveryFixture::new();
    let package = package_with_recovery_content();
    let legacy_before = seed_legacy_checkpoint(fixture.path(), &package);
    let versioned = versioned_directory_for(fixture.path()).expect("versioned directory path");
    fs::create_dir_all(&versioned).expect("create versioned recovery directory");
    let schema =
        "loom.sheets.recovery/1;session=test-session;workbook=test-workbook;baseline=test-baseline";
    let manifest = crate::legacy_migration::scan_legacy(fixture.path())
        .expect("scan legacy files for migration receipt");
    let prepared = crate::legacy_migration::prepared(manifest, 0, &package, schema);
    crate::legacy_migration::persist_receipt(&versioned, &prepared)
        .expect("write prepared migration receipt");
    let mut journal = RecoveryJournal::open(&versioned).expect("open versioned recovery journal");
    journal
        .checkpoint(0, schema, &package)
        .expect("publish the receipt's matching complete checkpoint");
    let invalid_batch = CellEditBatch {
        format_version: RECORD_VERSION,
        session_id: "wrong-session".into(),
        workbook_id: "test-workbook".into(),
        baseline_id: "test-baseline".into(),
        predecessor_sequence: 0,
        active_sheet: 0,
        edits: vec![CellAssignment {
            sheet: 0,
            row: 0,
            col: 0,
            raw: Some("must not replay".into()),
        }],
    };
    journal
        .append(
            "invalid-after-migration-checkpoint",
            "invalid recovery batch",
            serde_json::to_vec(&invalid_batch).expect("encode semantically invalid batch"),
        )
        .expect("append checksum-valid batch with wrong lineage");
    drop(journal);
    let receipt_path = versioned.join("legacy-migration.json");
    let receipt_before = fs::read(&receipt_path).expect("read prepared receipt before startup");

    let result = CellEditRecovery::open_at(fixture.path());

    assert!(
        result.is_err(),
        "startup must reject invalid recovery lineage"
    );
    assert_eq!(legacy_entry_bytes(fixture.path()), legacy_before);
    assert_eq!(
        fs::read(&receipt_path).expect("read receipt after failed startup"),
        receipt_before,
        "failed replay must leave the pending receipt unchanged"
    );
}

#[test]
fn legacy_migration_publishes_and_verifies_before_cleaning_complete_workbook() {
    let fixture = RecoveryFixture::new();
    let package = package_with_recovery_content();
    let legacy_before = seed_legacy_checkpoint(fixture.path(), &package);
    assert!(
        legacy_before.len() >= 2,
        "fixture must include multiple legacy files"
    );

    let (mut recovery, restored) =
        CellEditRecovery::open_at(fixture.path()).expect("open legacy recovery");
    assert_eq!(restored, Some(package.clone()));
    recovery
        .checkpoint_package(package.clone(), false)
        .expect("publish versioned baseline and migrate legacy files");
    drop(recovery);

    let versioned = versioned_directory_for(fixture.path()).expect("versioned path");
    let journal = RecoveryJournal::open(&versioned).expect("open migrated journal");
    let state = journal.recover().expect("read migrated checkpoint");
    assert_eq!(state.checkpoint.as_deref(), Some(package.as_slice()));
    let metadata = state.checkpoint_metadata.expect("checkpoint metadata");
    assert_eq!(metadata.last_sequence, 0);
    let identity = parse_checkpoint_identity(&metadata.schema).expect("checkpoint identity");
    assert!(!identity.baseline_id.is_empty());

    let restored = crate::restore_workbook_from_snapshot(&state.checkpoint.unwrap())
        .expect("restore verified package");
    assert_eq!(restored.active, 1);
    assert_eq!(restored.sheets.len(), 2);
    assert_eq!(
        restored.sheets[0].raw(CellRef::parse("B1").expect("formula cell")),
        Some("=A1*2")
    );
    assert_eq!(
        restored.sheets[1].raw(CellRef::parse("A1").expect("cross-sheet formula")),
        Some("=Data!B1+1")
    );
    assert_eq!(
        restored.sheets[0].objects[0].embedded.as_deref(),
        Some(&[137, 80, 78, 71, 13, 10, 26, 10][..])
    );
    assert!(legacy_entry_bytes(fixture.path()).is_empty());
    let receipt: serde_json::Value = serde_json::from_slice(
        &fs::read(versioned.join("legacy-migration.json")).expect("read migration marker"),
    )
    .expect("decode migration marker");
    assert_eq!(receipt["phase"], "complete");
}

#[test]
fn legacy_migration_does_not_run_for_temporary_startup_import() {
    let fixture = RecoveryFixture::new();
    let package = package_with_recovery_content();
    let legacy_before = seed_legacy_checkpoint(fixture.path(), &package);
    let (worker, startup) = WorkbookWorker::start_at(fixture.path().to_path_buf(), "loom.sheets/1")
        .expect("start worker");
    assert_eq!(startup.restored_payload, Some(package));
    worker
        .initialize_workbook_without_recovery(1, 0, vec![Sheet::new("Pending import")])
        .expect("show temporary startup import");
    worker.wait_for_result(1).expect("temporary calculation");
    drop(worker);

    assert_eq!(legacy_entry_bytes(fixture.path()), legacy_before);
    let versioned = versioned_directory_for(fixture.path()).expect("versioned path");
    assert!(!versioned.join("checkpoint.json").exists());
    assert!(!versioned.join("legacy-migration.json").exists());
}

#[test]
fn legacy_migration_reports_unsupported_only_entries_at_startup_and_preserves_them() {
    let fixture = RecoveryFixture::new();
    let unknown_path = fixture.path().join("future-recovery-format.bin");
    let unknown_bytes = b"unknown legacy recovery must remain intact";
    fs::write(&unknown_path, unknown_bytes).expect("write unsupported recovery entry");

    let error = match CellEditRecovery::open_at(fixture.path()) {
        Ok(_) => panic!("unsupported-only legacy recovery must be reported at startup"),
        Err(error) => error,
    };
    assert!(error.contains("unsupported"), "unexpected error: {error}");
    assert_eq!(
        fs::read(unknown_path).expect("read preserved entry"),
        unknown_bytes
    );
}

#[test]
fn legacy_migration_waits_for_accepted_startup_replacement() {
    let fixture = RecoveryFixture::new();
    let old_package = package_with_recovery_content();
    let legacy_before = seed_legacy_checkpoint(fixture.path(), &old_package);
    let (worker, startup) = WorkbookWorker::start_at(fixture.path().to_path_buf(), "loom.sheets/1")
        .expect("start worker");
    assert_eq!(startup.restored_payload, Some(old_package));
    worker
        .initialize_workbook_without_recovery(1, 0, vec![Sheet::new("Pending import")])
        .expect("show import pending confirmation");
    worker.wait_for_result(1).expect("temporary calculation");

    let mut selected = Sheet::new("Selected");
    selected.set_str("A1", "12");
    selected.set_str("B1", "=A1+3");
    let report = Sheet::new("Summary");
    let selected_package = workbook_package_bytes(&[selected.clone(), report.clone()], 1)
        .expect("selected workbook package");
    worker
        .submit_replacement(2, 1, vec![selected, report])
        .expect("accept startup replacement");
    let result = worker
        .wait_for_result(2)
        .expect("accepted replacement result");
    assert!(result.recovery_error.is_none());
    drop(worker);

    let versioned = versioned_directory_for(fixture.path()).expect("versioned path");
    let journal = RecoveryJournal::open(&versioned).expect("open migrated journal");
    assert_eq!(
        journal
            .recover()
            .expect("read accepted checkpoint")
            .checkpoint,
        Some(selected_package)
    );
    assert!(legacy_entry_bytes(fixture.path()).is_empty());
    assert!(!legacy_before.is_empty());
}

#[test]
fn legacy_migration_refuses_unmarked_legacy_state_alongside_a_versioned_checkpoint() {
    let fixture = RecoveryFixture::new();
    let versioned_package = checkpoint_fixture(fixture.path());
    let legacy_package = package_with_recovery_content();
    let legacy_before = seed_legacy_checkpoint(fixture.path(), &legacy_package);

    let error = CellEditRecovery::open_at(fixture.path())
        .err()
        .expect("unmarked versioned and legacy recovery must fail closed");
    assert!(
        error.contains("legacy"),
        "unexpected recovery error: {error}"
    );
    assert!(
        error.contains("versioned"),
        "unexpected recovery error: {error}"
    );
    assert_eq!(legacy_entry_bytes(fixture.path()), legacy_before);
    let versioned = versioned_directory_for(fixture.path()).expect("versioned path");
    assert!(!versioned.join("legacy-migration.json").exists());
    let journal = RecoveryJournal::open(&versioned).expect("open migrated journal");
    assert_eq!(
        journal
            .recover()
            .expect("read preserved versioned baseline")
            .checkpoint,
        Some(versioned_package)
    );
    assert_eq!(legacy_entry_bytes(fixture.path()), legacy_before);
}

#[test]
fn unmarked_coexisting_recovery_refuses_before_repairing_versioned_journal() {
    let fixture = RecoveryFixture::new();
    checkpoint_fixture(fixture.path());
    let versioned = versioned_directory_for(fixture.path()).expect("versioned path");
    let mut journal = RecoveryJournal::open(&versioned).expect("open versioned journal");
    let recovered = journal.recover().expect("read versioned checkpoint");
    let metadata = recovered
        .checkpoint_metadata
        .as_ref()
        .expect("versioned checkpoint metadata");
    let identity = parse_checkpoint_identity(&metadata.schema).expect("parse checkpoint identity");
    append_batch(
        &mut journal,
        &identity,
        metadata.last_sequence,
        "versioned edit",
    );
    drop(journal);

    let operations = versioned.join("operations.jsonl");
    let mut operations_file = fs::OpenOptions::new()
        .append(true)
        .open(&operations)
        .expect("open versioned journal tail");
    operations_file
        .write_all(b"{torn-tail")
        .expect("append torn versioned tail");
    operations_file
        .sync_all()
        .expect("sync torn versioned tail");
    drop(operations_file);

    let legacy_package = package_with_recovery_content();
    let legacy_before = seed_legacy_checkpoint(fixture.path(), &legacy_package);
    let versioned_before = versioned_entry_bytes(&versioned);

    let error = CellEditRecovery::open_at(fixture.path())
        .err()
        .expect("unmarked coexisting stores must fail closed");
    assert!(
        error.contains("legacy"),
        "unexpected recovery error: {error}"
    );
    assert_eq!(legacy_entry_bytes(fixture.path()), legacy_before);
    assert_eq!(versioned_entry_bytes(&versioned), versioned_before);
}

#[test]
fn unsupported_legacy_entry_refuses_before_repairing_versioned_journal() {
    let fixture = RecoveryFixture::new();
    checkpoint_fixture(fixture.path());
    let versioned = versioned_directory_for(fixture.path()).expect("versioned path");
    let mut journal = RecoveryJournal::open(&versioned).expect("open versioned journal");
    let recovered = journal.recover().expect("read versioned checkpoint");
    let metadata = recovered
        .checkpoint_metadata
        .as_ref()
        .expect("versioned checkpoint metadata");
    let identity = parse_checkpoint_identity(&metadata.schema).expect("parse checkpoint identity");
    append_batch(
        &mut journal,
        &identity,
        metadata.last_sequence,
        "versioned edit",
    );
    drop(journal);

    let operations = versioned.join("operations.jsonl");
    let mut operations_file = fs::OpenOptions::new()
        .append(true)
        .open(&operations)
        .expect("open versioned journal tail");
    operations_file
        .write_all(b"{torn-tail")
        .expect("append torn versioned tail");
    operations_file
        .sync_all()
        .expect("sync torn versioned tail");
    drop(operations_file);

    let unknown_path = fixture.path().join("future-recovery-format.bin");
    let unknown_bytes = b"unknown legacy recovery must remain intact";
    fs::write(&unknown_path, unknown_bytes).expect("write unsupported recovery entry");
    let versioned_before = versioned_entry_bytes(&versioned);

    let error = CellEditRecovery::open_at(fixture.path())
        .err()
        .expect("unsupported legacy plus versioned recovery must fail closed");
    assert!(
        error.contains("legacy"),
        "unexpected recovery error: {error}"
    );
    assert_eq!(
        fs::read(unknown_path).expect("read unsupported entry"),
        unknown_bytes
    );
    assert_eq!(versioned_entry_bytes(&versioned), versioned_before);
}

#[test]
fn missing_migration_receipt_with_recreated_legacy_state_fails_closed() {
    let fixture = RecoveryFixture::new();
    let original_legacy_package = package_with_recovery_content();
    let original_legacy = seed_legacy_checkpoint(fixture.path(), &original_legacy_package);
    let (mut recovery, restored) =
        CellEditRecovery::open_at(fixture.path()).expect("open legacy-only recovery");
    assert_eq!(restored, Some(original_legacy_package.clone()));
    recovery
        .checkpoint_package(original_legacy_package.clone(), false)
        .expect("migrate legacy checkpoint");
    drop(recovery);
    assert!(legacy_entry_bytes(fixture.path()).is_empty());

    let versioned = versioned_directory_for(fixture.path()).expect("versioned path");
    fs::remove_file(versioned.join("legacy-migration.json"))
        .expect("simulate completion receipt lost on Windows");
    let mut newer_sheet = Sheet::new("Newer legacy state");
    newer_sheet.set_str("A1", "written after versioned migration");
    let newer_legacy_package =
        workbook_package_bytes(&[newer_sheet], 0).expect("newer legacy workbook package");
    let newer_legacy = seed_legacy_checkpoint(fixture.path(), &newer_legacy_package);

    let error = CellEditRecovery::open_at(fixture.path())
        .err()
        .expect("missing receipt with newer legacy data must fail closed");
    assert!(
        error.contains("legacy"),
        "unexpected recovery error: {error}"
    );
    assert!(
        error.contains("versioned"),
        "unexpected recovery error: {error}"
    );
    assert_eq!(legacy_entry_bytes(fixture.path()), newer_legacy);
    assert_ne!(newer_legacy, original_legacy);
    let journal = RecoveryJournal::open(&versioned).expect("open preserved versioned journal");
    assert_eq!(
        journal
            .recover()
            .expect("read preserved versioned checkpoint")
            .checkpoint,
        Some(original_legacy_package)
    );
}

#[cfg(unix)]
#[test]
fn legacy_migration_publication_failure_preserves_legacy_bytes_and_prepared_receipt() {
    let fixture = RecoveryFixture::new();
    let package = package_with_recovery_content();
    let legacy_before = seed_legacy_checkpoint(fixture.path(), &package);
    let (mut recovery, _) =
        CellEditRecovery::open_at(fixture.path()).expect("open legacy recovery");
    let versioned = versioned_directory_for(fixture.path()).expect("versioned path");
    let pointer_temporary = versioned.join(format!(".checkpoint.json.{}.tmp", std::process::id()));
    fs::create_dir(&pointer_temporary).expect("block checkpoint pointer publication");

    let error = recovery
        .checkpoint_package(package, false)
        .expect_err("blocked pointer publication must fail");
    assert!(error.contains("blocked"), "unexpected failure: {error}");
    assert_eq!(legacy_entry_bytes(fixture.path()), legacy_before);
    let receipt: serde_json::Value = serde_json::from_slice(
        &fs::read(versioned.join("legacy-migration.json")).expect("prepared receipt"),
    )
    .expect("decode prepared receipt");
    assert_eq!(receipt["phase"], "prepared");
    assert!(!versioned.join("checkpoint.json").exists());
}

#[test]
fn legacy_migration_refuses_a_changed_legacy_source_before_publication() {
    let fixture = RecoveryFixture::new();
    let package = package_with_recovery_content();
    let legacy_before = seed_legacy_checkpoint(fixture.path(), &package);
    let (mut recovery, _) =
        CellEditRecovery::open_at(fixture.path()).expect("open legacy recovery");
    let changed_name = legacy_before.keys().next().expect("legacy file").clone();
    fs::write(
        fixture.path().join(&changed_name),
        b"changed by older writer",
    )
    .expect("change legacy recovery file");
    let changed_files = legacy_entry_bytes(fixture.path());

    let error = recovery
        .checkpoint_package(package, false)
        .expect_err("changed source must refuse migration");
    assert!(error.contains("changed"), "unexpected failure: {error}");
    assert_eq!(legacy_entry_bytes(fixture.path()), changed_files);
    let versioned = versioned_directory_for(fixture.path()).expect("versioned path");
    assert!(!versioned.join("checkpoint.json").exists());
    assert!(!versioned.join("legacy-migration.json").exists());
}

#[test]
fn legacy_migration_refuses_unknown_legacy_entries_before_publication() {
    let fixture = RecoveryFixture::new();
    let package = package_with_recovery_content();
    seed_legacy_checkpoint(fixture.path(), &package);
    let (mut recovery, _) =
        CellEditRecovery::open_at(fixture.path()).expect("open legacy recovery");
    fs::write(fixture.path().join("unrecognized-state.bin"), b"preserve")
        .expect("write unknown entry");
    let entries_before = legacy_entry_bytes(fixture.path());

    let error = recovery
        .checkpoint_package(package, false)
        .expect_err("unknown entry must refuse migration");
    assert!(error.contains("unknown"), "unexpected failure: {error}");
    assert_eq!(legacy_entry_bytes(fixture.path()), entries_before);
    let versioned = versioned_directory_for(fixture.path()).expect("versioned path");
    assert!(!versioned.join("checkpoint.json").exists());
    assert!(!versioned.join("legacy-migration.json").exists());
}

#[test]
fn legacy_migration_refuses_each_capacity_limit_before_writing_a_receipt() {
    for (limits, expected_error) in [
        ((1, u64::MAX, u64::MAX), "package limit"),
        ((u64::MAX, 1, u64::MAX), "retained-storage limit"),
        ((u64::MAX, u64::MAX, 1), "temporary-peak limit"),
    ] {
        assert_capacity_refusal(limits, expected_error);
    }
}

#[test]
fn ordinary_checkpoint_capacity_refusal_preserves_checkpoint_journal_and_sequence() {
    for (limits, expected_error) in [
        (
            crate::recovery_policy::RecoveryLimits {
                package_bytes: 1,
                retained_bytes: u64::MAX,
                temporary_peak_bytes: u64::MAX,
            },
            "package limit",
        ),
        (
            crate::recovery_policy::RecoveryLimits {
                package_bytes: u64::MAX,
                retained_bytes: 1,
                temporary_peak_bytes: u64::MAX,
            },
            "retained-storage limit",
        ),
        (
            crate::recovery_policy::RecoveryLimits {
                package_bytes: u64::MAX,
                retained_bytes: u64::MAX,
                temporary_peak_bytes: 1,
            },
            "temporary-peak limit",
        ),
    ] {
        let fixture = RecoveryFixture::new();
        let original_package = checkpoint_fixture(fixture.path());
        let (mut recovery, _) =
            CellEditRecovery::open_at(fixture.path()).expect("open checkpoint recovery");
        recovery
            .record_cells(
                0,
                [(
                    0,
                    CellRef::parse("A1").expect("cell"),
                    Some("edited".into()),
                )],
                || Ok(original_package.clone()),
            )
            .expect("write a durable edit before checkpoint refusal");

        let versioned = versioned_directory_for(fixture.path()).expect("versioned path");
        let journal_path = versioned.join("operations.jsonl");
        let mut torn = OpenOptions::new()
            .append(true)
            .open(&journal_path)
            .expect("open journal for interrupted tail");
        torn.write_all(b"{interrupted")
            .expect("append torn journal tail");
        drop(torn);
        let before = versioned_entry_bytes(&versioned);
        let sequence_before = recovery.last_sequence_for_test();
        recovery.set_recovery_limits_for_test(limits);

        let error = recovery
            .checkpoint_package(original_package, false)
            .expect_err("ordinary checkpoint must refuse injected capacity limit");

        assert!(
            error.contains(expected_error),
            "expected {expected_error:?}, got {error:?}"
        );
        assert_eq!(versioned_entry_bytes(&versioned), before);
        assert_eq!(recovery.last_sequence_for_test(), sequence_before);
    }
}

#[test]
fn ordinary_append_capacity_refusal_preserves_torn_journal_and_sequence() {
    let fixture = RecoveryFixture::new();
    let package = checkpoint_fixture(fixture.path());
    let (mut recovery, _) =
        CellEditRecovery::open_at(fixture.path()).expect("open checkpoint recovery");
    recovery
        .record_cells(
            0,
            [(0, CellRef::parse("A1").expect("cell"), Some("first".into()))],
            || Ok(package.clone()),
        )
        .expect("write first durable edit");
    let versioned = versioned_directory_for(fixture.path()).expect("versioned path");
    let journal_path = versioned.join("operations.jsonl");
    let mut torn = OpenOptions::new()
        .append(true)
        .open(&journal_path)
        .expect("open journal for interrupted tail");
    torn.write_all(b"{interrupted")
        .expect("append torn journal tail");
    drop(torn);
    let before = versioned_entry_bytes(&versioned);
    let sequence_before = recovery.last_sequence_for_test();
    recovery.set_recovery_limits_for_test(crate::recovery_policy::RecoveryLimits {
        package_bytes: u64::MAX,
        retained_bytes: 1,
        temporary_peak_bytes: u64::MAX,
    });

    let mut fallback_attempted = false;
    let error = recovery
        .record_cells(
            0,
            [(
                0,
                CellRef::parse("A2").expect("cell"),
                Some("second".into()),
            )],
            || {
                fallback_attempted = true;
                Ok(package)
            },
        )
        .expect_err("aggregate recovery limit must refuse before journal repair");

    assert!(
        fallback_attempted,
        "aggregate capacity refusal must attempt complete-checkpoint fallback"
    );
    assert!(
        error.contains("retained-storage limit"),
        "unexpected error: {error}"
    );
    assert_eq!(versioned_entry_bytes(&versioned), before);
    assert_eq!(recovery.last_sequence_for_test(), sequence_before);
}

#[test]
fn ordinary_append_aggregate_capacity_refusal_uses_fitting_checkpoint() {
    let fixture = RecoveryFixture::new();
    let baseline_package = checkpoint_fixture(fixture.path());
    let versioned = versioned_directory_for(fixture.path()).expect("versioned directory");
    let large_value = "x".repeat(100_000);
    let mut journal = RecoveryJournal::open(&versioned).expect("open baseline journal");
    let recovered = journal.recover().expect("read baseline checkpoint");
    assert_eq!(recovered.checkpoint, Some(baseline_package.clone()));
    let identity = parse_checkpoint_identity(
        &recovered
            .checkpoint_metadata
            .expect("baseline checkpoint metadata")
            .schema,
    )
    .expect("baseline checkpoint identity");
    append_batch(&mut journal, &identity, 0, &large_value);
    append_batch(&mut journal, &identity, 1, "compact replacement");
    drop(journal);

    let mut replayed_sheet = Sheet::new("Data");
    replayed_sheet.set_str("A1", "compact replacement");
    replayed_sheet.set_str("B1", "=A1+1");
    let replayed_package = workbook_package_bytes(&[replayed_sheet], 0)
        .expect("package accepted replayed historical workbook state");
    let (mut recovery, restored) =
        CellEditRecovery::open_at(fixture.path()).expect("open checkpoint recovery");
    assert_eq!(restored, Some(replayed_package));
    assert_eq!(recovery.last_sequence_for_test(), 2);

    let versioned = versioned_directory_for(fixture.path()).expect("versioned directory");
    let retained_before = crate::recovery_policy::scan_recovery_storage(&versioned, fixture.path())
        .expect("scan current recovery storage")
        .total_bytes()
        .expect("sum current recovery storage");
    recovery.set_recovery_limits_for_test(crate::recovery_policy::RecoveryLimits {
        package_bytes: u64::MAX,
        retained_bytes: retained_before,
        temporary_peak_bytes: u64::MAX,
    });

    let a1 = CellRef::parse("A1").expect("A1 cell");
    let a3 = CellRef::parse("A3").expect("A3 cell");
    let mut current_sheet = Sheet::new("Data");
    current_sheet.set_str("A1", "compact replacement");
    current_sheet.set_str("B1", "=A1+1");
    current_sheet.set_str("A3", "checkpoint after aggregate refusal");
    let current_package =
        workbook_package_bytes(&[current_sheet], 0).expect("package accepted final workbook state");
    let journal_bytes = fs::metadata(versioned.join("operations.jsonl"))
        .expect("inspect historical edit journal")
        .len();
    println!(
        "fallback fixture: journal={journal_bytes} bytes, package={} bytes, retained cap={retained_before} bytes",
        current_package.len()
    );
    assert!(
        journal_bytes > 250_000,
        "fixture must contain historical bytes"
    );
    assert!(
        current_package.len() < 16_384,
        "complete package must remain small"
    );
    let mut fallback_attempted = false;
    recovery
        .record_cells(
            0,
            [(0, a3, Some("checkpoint after aggregate refusal".into()))],
            || {
                fallback_attempted = true;
                Ok(current_package.clone())
            },
        )
        .expect("fit a complete checkpoint after aggregate append refusal");
    assert!(fallback_attempted, "complete-checkpoint fallback must run");

    assert_eq!(
        recovery.last_sequence_for_test(),
        2,
        "the refused append must not advance the durable sequence"
    );
    drop(recovery);

    let journal = RecoveryJournal::open(&versioned).expect("reopen recovery journal");
    let recovered = journal.recover().expect("read checkpointed recovery state");
    assert_eq!(recovered.checkpoint, Some(current_package.clone()));
    assert_eq!(
        recovered
            .checkpoint_metadata
            .as_ref()
            .expect("checkpoint metadata")
            .last_sequence,
        2
    );
    assert!(recovered.operations.is_empty());
    let workbook = crate::restore_workbook_from_snapshot(
        recovered.checkpoint.as_deref().expect("checkpoint package"),
    )
    .expect("restore checkpointed workbook");
    assert_eq!(workbook.sheets[0].raw(a1), Some("compact replacement"));
    assert_eq!(
        workbook.sheets[0].raw(a3),
        Some("checkpoint after aggregate refusal")
    );
}

fn assert_capacity_refusal(limits: (u64, u64, u64), expected_error: &str) {
    let fixture = RecoveryFixture::new();
    let package = package_with_recovery_content();
    let legacy_before = seed_legacy_checkpoint(fixture.path(), &package);
    let (mut recovery, _) =
        CellEditRecovery::open_at(fixture.path()).expect("open legacy recovery");
    recovery.set_migration_limits_for_test(limits.0, limits.1, limits.2);
    assert_eq!(recovery.migration_limits_for_test(), Some(limits));

    let error = recovery
        .checkpoint_package(package, false)
        .expect_err("capacity refusal must fail before publication");
    assert!(
        error.contains(expected_error),
        "expected {expected_error:?}, got {error:?}"
    );
    assert_eq!(legacy_entry_bytes(fixture.path()), legacy_before);
    let versioned = versioned_directory_for(fixture.path()).expect("versioned path");
    assert!(!versioned.join("checkpoint.json").exists());
    assert!(!versioned.join("legacy-migration.json").exists());
}

#[test]
fn legacy_migration_retries_verified_partial_cleanup() {
    let fixture = RecoveryFixture::new();
    let package = package_with_recovery_content();
    let legacy_before = seed_legacy_checkpoint(fixture.path(), &package);
    assert!(
        legacy_before.len() >= 2,
        "fixture must include multiple legacy files"
    );
    let (mut recovery, _) =
        CellEditRecovery::open_at(fixture.path()).expect("open legacy recovery");
    recovery
        .checkpoint_package(package.clone(), false)
        .expect("publish versioned baseline");
    drop(recovery);

    let versioned = versioned_directory_for(fixture.path()).expect("versioned path");
    let receipt_path = versioned.join("legacy-migration.json");
    let mut receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(&receipt_path).expect("read receipt"))
            .expect("decode receipt");
    receipt["phase"] = serde_json::Value::String("verified".into());
    fs::write(
        &receipt_path,
        serde_json::to_vec(&receipt).expect("encode receipt"),
    )
    .expect("simulate crash during cleanup");
    let (relative, bytes) = legacy_before.iter().next().expect("legacy file");
    fs::write(fixture.path().join(relative), bytes).expect("restore one undeleted file");

    let (reopened, restored) =
        CellEditRecovery::open_at(fixture.path()).expect("retry verified cleanup");
    assert_eq!(restored, Some(package));
    drop(reopened);
    assert!(legacy_entry_bytes(fixture.path()).is_empty());
    let receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(receipt_path).expect("read completed receipt"))
            .expect("decode completed receipt");
    assert_eq!(receipt["phase"], "complete");
}

#[test]
fn legacy_migration_marker_reports_files_recreated_by_an_older_binary() {
    let fixture = RecoveryFixture::new();
    let package = package_with_recovery_content();
    let legacy_before = seed_legacy_checkpoint(fixture.path(), &package);
    let (mut recovery, _) =
        CellEditRecovery::open_at(fixture.path()).expect("open legacy recovery");
    recovery
        .checkpoint_package(package, false)
        .expect("migrate legacy recovery");
    drop(recovery);
    let (name, bytes) = legacy_before.iter().next().expect("legacy file");
    fs::write(fixture.path().join(name), bytes).expect("simulate old binary write");

    let error = match CellEditRecovery::open_at(fixture.path()) {
        Ok(_) => panic!("reappeared legacy recovery must be reported"),
        Err(error) => error,
    };
    assert!(error.contains("reappeared"), "unexpected failure: {error}");
    assert_eq!(
        fs::read(fixture.path().join(name)).expect("preserved old file"),
        bytes.as_slice()
    );
}

#[test]
fn legacy_migration_rejects_receipt_path_traversal_before_touching_outside_files() {
    let fixture = RecoveryFixture::new();
    let package = package_with_recovery_content();
    seed_legacy_checkpoint(fixture.path(), &package);
    let (mut recovery, _) =
        CellEditRecovery::open_at(fixture.path()).expect("open legacy recovery");
    recovery
        .checkpoint_package(package, false)
        .expect("migrate legacy recovery");
    drop(recovery);

    let versioned = versioned_directory_for(fixture.path()).expect("versioned path");
    let receipt_path = versioned.join("legacy-migration.json");
    let mut receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(&receipt_path).expect("read completed receipt"))
            .expect("decode receipt");
    let sentinel = fixture.path().with_file_name(format!(
        "{}-outside-sentinel",
        fixture.path().file_name().unwrap().to_string_lossy()
    ));
    fs::write(&sentinel, b"must remain untouched").expect("write outside sentinel");
    let sentinel_name = sentinel
        .file_name()
        .expect("sentinel basename")
        .to_string_lossy();
    receipt["manifest"]["files"][0]["path"] =
        serde_json::Value::String(format!("../{sentinel_name}"));
    fs::write(
        &receipt_path,
        serde_json::to_vec(&receipt).expect("encode tampered receipt"),
    )
    .expect("write tampered receipt");

    let error = match CellEditRecovery::open_at(fixture.path()) {
        Ok(_) => panic!("unsafe receipt path must be rejected"),
        Err(error) => error,
    };
    assert!(
        error.contains("unsafe file manifest"),
        "unexpected error: {error}"
    );
    assert_eq!(
        fs::read(&sentinel).expect("read outside sentinel"),
        b"must remain untouched"
    );
    fs::remove_file(sentinel).expect("remove outside sentinel");
}

#[derive(Clone, Copy, Debug)]
enum InvalidBatchField {
    Version,
    Session,
    Workbook,
    Baseline,
    Predecessor,
    DuplicateCell,
}

#[test]
fn replay_rejects_wrong_lineage_and_does_not_return_a_partial_candidate() {
    for invalid_field in [
        InvalidBatchField::Version,
        InvalidBatchField::Session,
        InvalidBatchField::Workbook,
        InvalidBatchField::Baseline,
        InvalidBatchField::Predecessor,
        InvalidBatchField::DuplicateCell,
    ] {
        let fixture = RecoveryFixture::new();
        let package = checkpoint_fixture(fixture.path());
        let versioned = versioned_directory_for(fixture.path()).expect("versioned directory");
        let mut journal = RecoveryJournal::open(&versioned).expect("open versioned journal");
        let checkpoint = journal.recover().expect("read baseline checkpoint");
        let identity = parse_checkpoint_identity(
            &checkpoint
                .checkpoint_metadata
                .expect("checkpoint metadata")
                .schema,
        )
        .expect("checkpoint identity");
        append_batch(&mut journal, &identity, 0, "first edit");

        let mut invalid = CellEditBatch {
            format_version: RECORD_VERSION,
            session_id: identity.session_id.clone(),
            workbook_id: identity.workbook_id.clone(),
            baseline_id: identity.baseline_id.clone(),
            predecessor_sequence: 1,
            active_sheet: 0,
            edits: vec![CellAssignment {
                sheet: 0,
                row: 0,
                col: 0,
                raw: Some("must not be returned".into()),
            }],
        };
        match invalid_field {
            InvalidBatchField::Version => invalid.format_version += 1,
            InvalidBatchField::Session => invalid.session_id.push_str("-wrong"),
            InvalidBatchField::Workbook => invalid.workbook_id.push_str("-wrong"),
            InvalidBatchField::Baseline => invalid.baseline_id.push_str("-wrong"),
            InvalidBatchField::Predecessor => invalid.predecessor_sequence = 0,
            InvalidBatchField::DuplicateCell => invalid.edits.push(invalid.edits[0].clone()),
        }
        journal
            .append(
                "invalid-test-batch",
                "invalid test edit",
                serde_json::to_vec(&invalid).expect("encode invalid test batch"),
            )
            .expect("append invalid test batch");
        drop(journal);

        assert!(
            CellEditRecovery::open_at(fixture.path()).is_err(),
            "replay must reject {invalid_field:?} without returning a candidate workbook"
        );

        let journal = RecoveryJournal::open(&versioned).expect("reopen rejected journal");
        let unchanged = journal.recover().expect("read unchanged checkpoint");
        assert_eq!(unchanged.checkpoint.as_deref(), Some(package.as_slice()));
        assert_eq!(unchanged.operations.len(), 2);
    }
}

#[test]
fn replay_rejects_a_gap_in_durable_sequences() {
    let fixture = RecoveryFixture::new();
    let package = checkpoint_fixture(fixture.path());
    let versioned = versioned_directory_for(fixture.path()).expect("versioned directory");
    let mut journal = RecoveryJournal::open(&versioned).expect("open versioned journal");
    let checkpoint = journal.recover().expect("read baseline checkpoint");
    let identity = parse_checkpoint_identity(
        &checkpoint
            .checkpoint_metadata
            .expect("checkpoint metadata")
            .schema,
    )
    .expect("checkpoint identity");
    append_batch(&mut journal, &identity, 0, "first edit");
    append_batch(&mut journal, &identity, 1, "second edit");
    let mut records = journal.records().expect("read journal records");
    records[1].sequence = 3;
    drop(journal);

    let mut encoded = Vec::new();
    for record in &records {
        serde_json::to_writer(&mut encoded, record).expect("encode recovery record");
        encoded.push(b'\n');
    }
    let mut file = fs::File::create(versioned.join("operations.jsonl"))
        .expect("rewrite durable sequence fixture");
    file.write_all(&encoded)
        .expect("write durable sequence fixture");
    drop(file);

    assert!(CellEditRecovery::open_at(fixture.path()).is_err());
    let journal = RecoveryJournal::open(&versioned).expect("reopen sequence-gap journal");
    assert_eq!(
        journal
            .recover()
            .expect("read unchanged checkpoint")
            .checkpoint,
        Some(package)
    );
}

#[test]
fn oversized_cell_batch_uses_checkpoint_without_advancing_durable_sequence() {
    let fixture = RecoveryFixture::new();
    let baseline_package = checkpoint_fixture(fixture.path());
    let (mut recovery, restored) =
        CellEditRecovery::open_at(fixture.path()).expect("reopen baseline recovery");
    assert_eq!(restored, Some(baseline_package));

    let a1 = CellRef::parse("A1").expect("A1 cell");
    recovery
        .record_cells(0, [(0, a1, Some("small edit".into()))], || {
            Err("unexpected checkpoint fallback for small cell batch".into())
        })
        .expect("append first small edit");

    let large_value = "x".repeat(300_000);
    let a2 = CellRef::parse("A2").expect("A2 cell");
    let mut current_sheet = Sheet::new("Data");
    current_sheet.set_str("A1", "small edit");
    current_sheet.set_str("B1", "=A1+1");
    current_sheet.set_str("A2", &large_value);
    let current_package = workbook_package_bytes(&[current_sheet], 0)
        .expect("package accepted workbook state for checkpoint fallback");
    recovery
        .record_cells(0, [(0, a2, Some(large_value.clone()))], || {
            Ok(current_package.clone())
        })
        .expect("use a complete checkpoint for the oversized cell batch");

    drop(recovery);
    let versioned = versioned_directory_for(fixture.path()).expect("versioned recovery path");
    let journal = RecoveryJournal::open(&versioned).expect("reopen recovery journal");
    let recovered = journal
        .recover()
        .expect("read recovery state after oversized batch");
    assert!(
        recovered.checkpoint.as_deref() == Some(current_package.as_slice()),
        "complete checkpoint must contain the accepted oversized cell edit"
    );
    assert_eq!(
        recovered
            .checkpoint_metadata
            .as_ref()
            .expect("checkpoint metadata")
            .last_sequence,
        1,
        "checkpoint must retain the prior durable sequence"
    );
    assert!(
        recovered.operations.is_empty(),
        "oversized transaction must not be split into journal records"
    );
    let workbook = crate::restore_workbook_from_snapshot(
        recovered.checkpoint.as_deref().expect("checkpoint package"),
    )
    .expect("restore complete checkpoint");
    assert_eq!(workbook.sheets[0].raw(a2), Some(large_value.as_str()));

    drop(journal);
    let (mut recovery, restored) =
        CellEditRecovery::open_at(fixture.path()).expect("reopen checkpointed recovery");
    assert_eq!(restored, Some(current_package));
    let a3 = CellRef::parse("A3").expect("A3 cell");
    recovery
        .record_cells(0, [(0, a3, Some("after checkpoint".into()))], || {
            Err("unexpected checkpoint fallback for small cell batch".into())
        })
        .expect("append small edit after checkpoint fallback");
    drop(recovery);
    let journal = RecoveryJournal::open(&versioned).expect("reopen journal after next edit");
    let recovered = journal
        .recover()
        .expect("read recovery state after next small edit");
    assert_eq!(recovered.operations.len(), 1);
    assert_eq!(recovered.operations[0].sequence, 2);
    let next_batch: CellEditBatch = serde_json::from_slice(&recovered.operations[0].payload)
        .expect("decode next small edit batch");
    assert_eq!(next_batch.predecessor_sequence, 1);
}

#[cfg(unix)]
#[test]
fn unix_legacy_checkpoint_pointer_wins_over_a_stale_windows_commit_pointer() {
    let fixture = RecoveryFixture::new();
    let mut old_sheet = Sheet::new("Old");
    old_sheet.set_str("A1", "stale Windows checkpoint");
    let old_package = workbook_package_bytes(&[old_sheet], 0).expect("old package");
    let mut current_sheet = Sheet::new("Current");
    current_sheet.set_str("A1", "authoritative Unix checkpoint");
    let current_package = workbook_package_bytes(&[current_sheet], 0).expect("current package");

    let journal = RecoveryJournal::open(fixture.path()).expect("open legacy recovery journal");
    let old_metadata = journal
        .checkpoint(0, "legacy sheets state", &old_package)
        .expect("publish first checkpoint generation");
    let stale_commit_pointer = serde_json::json!({
        "generation": 1,
        "last_sequence": old_metadata.last_sequence,
        "sha256": old_metadata.sha256,
        "timestamp_ms": old_metadata.timestamp_ms,
        "schema": old_metadata.schema,
    });
    fs::write(
        fixture.path().join("checkpoint-commit-1.json"),
        serde_json::to_vec(&stale_commit_pointer).expect("encode stale Windows pointer"),
    )
    .expect("write stale Windows commit pointer");
    journal
        .checkpoint(0, "legacy sheets state", &current_package)
        .expect("publish newer Unix checkpoint generation");
    drop(journal);

    let (_recovery, restored) =
        CellEditRecovery::open_at(fixture.path()).expect("read legacy checkpoint");
    assert_eq!(
        restored,
        Some(current_package),
        "Unix checkpoint.json is authoritative over a stale Windows commit pointer"
    );
}

#[test]
fn ordinary_capacity_fallback_build_error_is_cause_neutral() {
    let fixture = RecoveryFixture::new();
    checkpoint_fixture(fixture.path());
    let (mut recovery, _) = CellEditRecovery::open_at(fixture.path()).expect("open recovery");
    recovery.set_recovery_limits_for_test(crate::recovery_policy::RecoveryLimits {
        package_bytes: u64::MAX,
        retained_bytes: 1,
        temporary_peak_bytes: u64::MAX,
    });
    let versioned = versioned_directory_for(fixture.path()).expect("versioned directory");
    let before = versioned_entry_bytes(&versioned);
    let sequence_before = recovery.last_sequence_for_test();
    let mut fallback_attempted = false;

    let error = recovery
        .record_cells(
            0,
            [(
                0,
                CellRef::parse("A2").expect("cell"),
                Some("edit that triggers capacity refusal".into()),
            )],
            || {
                fallback_attempted = true;
                Err("fixture package builder failure".into())
            },
        )
        .expect_err("failed complete-package construction remains a hard error");

    assert!(
        fallback_attempted,
        "capacity refusal must attempt checkpoint fallback"
    );
    assert!(
        error.contains("while preparing a complete Sheets recovery checkpoint"),
        "unexpected error: {error}"
    );
    assert!(
        !error.contains("journal limit") && !error.contains("capacity refusal"),
        "unexpected error: {error}"
    );
    assert_eq!(versioned_entry_bytes(&versioned), before);
    assert_eq!(recovery.last_sequence_for_test(), sequence_before);
}

#[test]
fn ordinary_append_unsupported_legacy_entry_does_not_attempt_fallback() {
    let fixture = RecoveryFixture::new();
    let package = checkpoint_fixture(fixture.path());
    let (mut recovery, _) = CellEditRecovery::open_at(fixture.path()).expect("open recovery");
    let versioned = versioned_directory_for(fixture.path()).expect("versioned directory");
    let versioned_before = versioned_entry_bytes(&versioned);
    let sequence_before = recovery.last_sequence_for_test();
    let unknown_path = fixture.path().join("future-recovery-format.bin");
    let unknown_bytes = b"unsupported legacy entry added after open";
    fs::write(&unknown_path, unknown_bytes).expect("add unsupported legacy entry");
    let mut fallback_attempted = false;

    let error = recovery
        .record_cells(
            0,
            [(
                0,
                CellRef::parse("A2").expect("cell"),
                Some("edit blocked by unsupported legacy state".into()),
            )],
            || {
                fallback_attempted = true;
                Ok(package.clone())
            },
        )
        .expect_err("unsupported legacy state must block the append");

    assert!(
        error.contains("unsupported entries"),
        "unexpected error: {error}"
    );
    assert!(
        !fallback_attempted,
        "unsupported-store failures must not invoke fallback"
    );
    assert_eq!(versioned_entry_bytes(&versioned), versioned_before);
    assert_eq!(recovery.last_sequence_for_test(), sequence_before);
    assert_eq!(
        fs::read(&unknown_path).expect("unsupported entry survives"),
        unknown_bytes
    );
}
