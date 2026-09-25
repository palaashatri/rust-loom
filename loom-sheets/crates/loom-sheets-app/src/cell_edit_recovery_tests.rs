use crate::cell_edit_recovery::{
    parse_checkpoint_identity, versioned_directory_for, CellAssignment, CellEditBatch,
    CellEditRecovery, RECORD_VERSION,
};
use crate::workbook_io::workbook_package_bytes;
use loom_production::{JournalRecord, RecoveryJournal};
use loom_sheets_core::Sheet;
use std::fs;
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
