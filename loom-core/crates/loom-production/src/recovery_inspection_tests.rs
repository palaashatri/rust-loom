use super::{
    inspect_recovery, inspect_recovery_with_limits, lock_recovery_directory,
    RecoveryInspectionCursor, RecoveryInspectionLimits, RecoveryJournal,
};
use std::fs;
use std::io::Write;
use tempfile::TempDir;

fn append(journal: &mut RecoveryJournal, sequence: &str, payload: &[u8]) -> Vec<u8> {
    let record = journal
        .append(sequence, "edit", payload.to_vec())
        .expect("append operation");
    let mut line = serde_json::to_vec(&record).expect("serialize record");
    line.push(b'\n');
    line
}

fn snapshot_files(directory: &std::path::Path) -> Vec<(String, Vec<u8>)> {
    let mut files = fs::read_dir(directory)
        .expect("read recovery directory")
        .map(|entry| {
            let path = entry.expect("directory entry").path();
            let name = path
                .file_name()
                .expect("file name")
                .to_string_lossy()
                .into_owned();
            (name, fs::read(path).expect("read regular test file"))
        })
        .collect::<Vec<_>>();
    files.sort_by(|left, right| left.0.cmp(&right.0));
    files
}

#[test]
fn inspection_returns_selected_checkpoint_and_exact_journal_stats() {
    let temporary = TempDir::new().expect("temporary directory");
    let mut journal = RecoveryJournal::open(temporary.path()).expect("open journal");
    let first_line = append(&mut journal, "first", b"one");
    let second_line = append(&mut journal, "second", b"two");
    journal
        .checkpoint(1, "loom.test/1", b"checkpoint bytes")
        .expect("write checkpoint");

    let inspection = inspect_recovery(temporary.path()).expect("inspect recovery");

    assert_eq!(
        inspection.checkpoint.as_deref(),
        Some(b"checkpoint bytes".as_slice())
    );
    assert_eq!(
        inspection
            .checkpoint_metadata
            .as_ref()
            .map(|metadata| metadata.last_sequence),
        Some(1)
    );
    assert_eq!(inspection.records.len(), 2);
    let checkpoint_sequence = inspection
        .checkpoint_metadata
        .as_ref()
        .expect("selected checkpoint metadata")
        .last_sequence;
    let replay_sequences = inspection
        .records
        .iter()
        .filter(|record| record.sequence > checkpoint_sequence)
        .map(|record| record.sequence)
        .collect::<Vec<_>>();
    assert_eq!(replay_sequences, [2]);
    assert_eq!(
        inspection.journal_bytes,
        (first_line.len() + second_line.len()) as u64
    );
    assert_eq!(inspection.complete_record_count, 2);
    assert_eq!(
        inspection.max_complete_line_bytes,
        first_line.len().max(second_line.len())
    );
    assert!(!inspection.incomplete_tail);
}

#[test]
fn torn_tail_and_abandoned_temp_are_preserved_byte_for_byte() {
    let temporary = TempDir::new().expect("temporary directory");
    let mut journal = RecoveryJournal::open(temporary.path()).expect("open journal");
    let valid_line = append(&mut journal, "valid", b"kept");
    journal
        .checkpoint(1, "loom.test/1", b"checkpoint bytes")
        .expect("write checkpoint");
    drop(journal);

    let journal_path = temporary.path().join("operations.jsonl");
    let mut journal_file = fs::OpenOptions::new()
        .append(true)
        .open(&journal_path)
        .expect("open journal for torn tail");
    let torn_tail = b"{\"interrupted\"";
    journal_file.write_all(torn_tail).expect("write torn tail");
    drop(journal_file);
    let abandoned_temp_path = temporary.path().join(".atomicwrite-abandoned");
    fs::write(&abandoned_temp_path, b"temporary bytes must stay").expect("write temp");
    let before = snapshot_files(temporary.path());

    let inspection = inspect_recovery(temporary.path()).expect("inspect recovery");

    assert_eq!(inspection.records.len(), 1);
    assert_eq!(
        inspection.checkpoint.as_deref(),
        Some(b"checkpoint bytes".as_slice())
    );
    assert_eq!(
        inspection.journal_bytes,
        (valid_line.len() + torn_tail.len()) as u64
    );
    assert_eq!(inspection.complete_record_count, 1);
    assert_eq!(inspection.max_complete_line_bytes, valid_line.len());
    assert!(inspection.incomplete_tail);
    assert_eq!(snapshot_files(temporary.path()), before);
}

#[test]
fn inspection_of_missing_directory_does_not_create_it() {
    let parent = TempDir::new().expect("temporary parent");
    let missing = parent.path().join("not-created");

    let inspection = inspect_recovery(&missing).expect("inspect missing recovery root");

    assert!(!missing.exists());
    assert!(inspection.checkpoint.is_none());
    assert!(inspection.checkpoint_metadata.is_none());
    assert!(inspection.records.is_empty());
    assert_eq!(inspection.journal_bytes, 0);
    assert_eq!(inspection.complete_record_count, 0);
    assert_eq!(inspection.max_complete_line_bytes, 0);
    assert!(!inspection.incomplete_tail);
}

#[test]
fn inspection_refuses_caller_ceiling_without_changing_journal_bytes() {
    let temporary = TempDir::new().expect("temporary directory");
    let mut journal = RecoveryJournal::open(temporary.path()).expect("open journal");
    append(&mut journal, "first", b"payload");
    drop(journal);
    let journal_path = temporary.path().join("operations.jsonl");
    let before = fs::read(&journal_path).expect("journal before inspection");
    let abandoned_temp_path = temporary.path().join(".atomicwrite-abandoned");
    fs::write(&abandoned_temp_path, b"do not remove on refusal").expect("write temp");
    let before_entries = snapshot_files(temporary.path());

    let limits = RecoveryInspectionLimits {
        max_checkpoint_bytes: 1024,
        max_journal_bytes: before.len() as u64 - 1,
        max_records: usize::MAX,
        max_record_line_bytes: usize::MAX,
        ..RecoveryInspectionLimits::default()
    };
    let result = inspect_recovery_with_limits(temporary.path(), limits);

    assert!(result.is_err(), "over-ceiling journal must be refused");
    assert_eq!(
        fs::read(journal_path).expect("journal after refusal"),
        before
    );
    assert_eq!(snapshot_files(temporary.path()), before_entries);
}

#[test]
fn inspection_refuses_oversized_checkpoint_without_changing_files() {
    let temporary = TempDir::new().expect("temporary directory");
    let journal = RecoveryJournal::open(temporary.path()).expect("open journal");
    let package = b"checkpoint payload";
    journal
        .checkpoint(0, "loom.test/1", package)
        .expect("write checkpoint");
    drop(journal);
    let before = snapshot_files(temporary.path());

    let limits = RecoveryInspectionLimits {
        max_checkpoint_bytes: package.len() as u64 - 1,
        max_journal_bytes: 1024,
        max_records: 10,
        max_record_line_bytes: 1024,
        ..RecoveryInspectionLimits::default()
    };
    let result = inspect_recovery_with_limits(temporary.path(), limits);

    assert!(result.is_err(), "oversized checkpoint must be refused");
    assert_eq!(snapshot_files(temporary.path()), before);
}

#[test]
fn limited_journal_open_refuses_oversized_checkpoint_before_repairing_torn_tail() {
    let temporary = TempDir::new().expect("temporary directory");
    let journal = RecoveryJournal::open(temporary.path()).expect("open journal");
    let package = b"oversized checkpoint";
    journal
        .checkpoint(0, "loom.test/1", package)
        .expect("publish checkpoint");
    drop(journal);

    let journal_path = temporary.path().join("operations.jsonl");
    let mut journal_file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&journal_path)
        .expect("open journal for torn-tail fixture");
    journal_file.write_all(b"{torn").expect("append torn tail");
    drop(journal_file);
    let before = fs::read(&journal_path).expect("read journal before limited open");
    let limits = RecoveryInspectionLimits {
        max_checkpoint_bytes: package.len() as u64 - 1,
        ..RecoveryInspectionLimits::default()
    };
    let recovery_lock = lock_recovery_directory(temporary.path()).expect("lock recovery directory");

    let error = RecoveryJournal::open_with_recovery_lock_limited(&recovery_lock, limits)
        .expect_err("limited open must refuse the oversized checkpoint");

    assert!(
        error.to_string().contains("checkpoint"),
        "unexpected limited-open error: {error}"
    );
    assert_eq!(
        fs::read(journal_path).expect("read journal after limited refusal"),
        before,
        "a checkpoint refusal must happen before torn-tail repair"
    );
}

#[test]
fn limited_journal_open_validates_before_removing_abandoned_atomicwrite_directory() {
    let temporary = TempDir::new().expect("temporary directory");
    let journal = RecoveryJournal::open(temporary.path()).expect("open journal");
    journal
        .checkpoint(0, "loom.test/1", b"valid checkpoint")
        .expect("publish checkpoint");
    drop(journal);
    let abandoned = temporary.path().join(".atomicwrite-abandoned-test");
    fs::create_dir(&abandoned).expect("create abandoned atomic-write directory");
    fs::write(abandoned.join("temporary-file"), b"partial checkpoint")
        .expect("create abandoned atomic-write payload");
    let recovery_lock = lock_recovery_directory(temporary.path()).expect("lock recovery directory");

    let reopened = RecoveryJournal::open_with_recovery_lock_limited(
        &recovery_lock,
        RecoveryInspectionLimits::default(),
    )
    .expect("validate checkpoint then remove abandoned atomic-write directory");
    drop(recovery_lock);
    let recovered = reopened.recover().expect("recover valid checkpoint");

    assert_eq!(
        recovered.checkpoint.as_deref(),
        Some(b"valid checkpoint".as_slice())
    );
    assert!(!abandoned.exists());
}

#[test]
fn inspection_refuses_an_overdeep_abandoned_atomicwrite_tree() {
    let temporary = TempDir::new().expect("temporary directory");
    let abandoned = temporary.path().join(".atomicwrite-depth");
    fs::create_dir(&abandoned).expect("create abandoned atomic-write directory");
    let mut nested = abandoned;
    for _ in 0..33 {
        nested = nested.join("d");
        fs::create_dir(&nested).expect("create nested atomic-write directory");
    }
    fs::write(nested.join("temporary-file"), b"partial").expect("write temporary payload");

    let error = inspect_recovery(temporary.path())
        .expect_err("inspection must stop when abandoned directory depth is exceeded");

    assert!(
        error.to_string().contains("depth"),
        "unexpected error: {error}"
    );
}

#[test]
fn journal_finalization_rejects_changed_records_before_repairing_the_inspected_tail() {
    let temporary = TempDir::new().expect("temporary directory");
    let mut journal = RecoveryJournal::open(temporary.path()).expect("open journal");
    append(&mut journal, "first", b"original payload");
    drop(journal);

    let journal_path = temporary.path().join("operations.jsonl");
    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(&journal_path)
        .expect("open journal for torn tail");
    file.write_all(b"{torn").expect("append torn tail");
    drop(file);
    let before = fs::read(&journal_path).expect("read journal before finalization");
    let recovery_lock = lock_recovery_directory(temporary.path()).expect("lock recovery directory");
    let mut cursor = RecoveryInspectionCursor::open_with_limits(
        temporary.path(),
        RecoveryInspectionLimits::default(),
    )
    .expect("open bounded inspection cursor");
    let mut records = Vec::new();
    while let Some(record) = cursor.next_record().expect("read inspected record") {
        records.push(record);
    }
    assert!(
        cursor.stats().expect("EOF statistics").incomplete_tail,
        "fixture must contain a torn tail"
    );
    records[0].label.push_str(" changed after inspection");

    let error = RecoveryJournal::open_from_inspection(&recovery_lock, cursor, &records)
        .expect_err("finalization must reject records that differ from the inspected snapshot");

    assert!(
        error.to_string().contains("snapshot"),
        "unexpected error: {error}"
    );
    assert_eq!(
        fs::read(journal_path).expect("read journal after rejected finalization"),
        before,
        "changed caller records must not trigger torn-tail repair"
    );
}

#[test]
fn oversized_invalid_complete_tail_is_rejected_before_tail_tolerance() {
    let temporary = TempDir::new().expect("temporary directory");
    let recovery_root = temporary.path().join("recovery");
    fs::create_dir(&recovery_root).expect("create recovery root");
    let journal_path = recovery_root.join("operations.jsonl");
    let mut oversized_invalid_line = vec![b'x'; 128];
    oversized_invalid_line.push(b'\n');
    fs::write(&journal_path, &oversized_invalid_line).expect("write oversized invalid tail");

    let limits = RecoveryInspectionLimits {
        max_checkpoint_bytes: 1024,
        max_journal_bytes: 1024,
        max_records: 10,
        max_record_line_bytes: 64,
        ..RecoveryInspectionLimits::default()
    };
    let result = inspect_recovery_with_limits(&recovery_root, limits);

    assert!(
        result.is_err(),
        "oversized line must be rejected before JSON parsing"
    );
    assert_eq!(
        fs::read(journal_path).expect("journal after rejected inspection"),
        oversized_invalid_line
    );
}

#[test]
fn many_blank_lines_do_not_consume_the_record_count_budget() {
    let temporary = TempDir::new().expect("temporary directory");
    let recovery_root = temporary.path().join("recovery");
    fs::create_dir(&recovery_root).expect("create recovery root");
    let journal_path = recovery_root.join("operations.jsonl");
    let blank_lines = vec![b'\n'; 32 * 1024];
    fs::write(&journal_path, &blank_lines).expect("write blank journal lines");

    let limits = RecoveryInspectionLimits {
        max_checkpoint_bytes: 1024,
        max_journal_bytes: blank_lines.len() as u64,
        max_records: 0,
        max_record_line_bytes: 1,
        ..RecoveryInspectionLimits::default()
    };
    let inspection =
        inspect_recovery_with_limits(&recovery_root, limits).expect("blank lines are not records");

    assert_eq!(inspection.complete_record_count, 0);
    assert_eq!(inspection.journal_bytes, blank_lines.len() as u64);
    assert!(!inspection.incomplete_tail);
}

#[cfg(unix)]
#[test]
fn inspection_rejects_symlink_journal_without_reading_target() {
    use std::os::unix::fs::symlink;

    let temporary = TempDir::new().expect("temporary directory");
    let target = temporary.path().join("outside.jsonl");
    fs::write(&target, b"outside bytes").expect("write target");
    let recovery_root = temporary.path().join("recovery");
    fs::create_dir(&recovery_root).expect("create recovery root");
    symlink(&target, recovery_root.join("operations.jsonl")).expect("link journal");

    assert!(inspect_recovery(&recovery_root).is_err());
    assert_eq!(fs::read(target).expect("read target"), b"outside bytes");
}

#[cfg(unix)]
#[test]
fn inspection_rejects_symlink_recovery_root() {
    use std::os::unix::fs::symlink;

    let temporary = TempDir::new().expect("temporary directory");
    let real_root = temporary.path().join("real-recovery");
    let linked_root = temporary.path().join("linked-recovery");
    fs::create_dir(&real_root).expect("create real recovery root");
    let journal_path = real_root.join("operations.jsonl");
    fs::write(&journal_path, b"preserve").expect("write journal target");
    symlink(&real_root, &linked_root).expect("link recovery root");

    assert!(inspect_recovery(&linked_root).is_err());
    assert_eq!(
        fs::read(journal_path).expect("read target journal"),
        b"preserve"
    );
}

#[cfg(unix)]
#[test]
fn inspection_rejects_special_journal_entry_without_blocking() {
    use std::os::unix::net::UnixListener;

    let temporary = TempDir::new().expect("temporary directory");
    let recovery_root = temporary.path().join("recovery");
    fs::create_dir(&recovery_root).expect("create recovery root");
    let socket_path = recovery_root.join("operations.jsonl");
    let _listener = UnixListener::bind(&socket_path).expect("bind special journal socket");

    assert!(inspect_recovery(&recovery_root).is_err());
    assert!(socket_path.exists());
}

#[test]
fn inspection_enforces_exact_and_one_over_byte_count_and_line_ceilings() {
    let temporary = TempDir::new().expect("temporary directory");
    let mut journal = RecoveryJournal::open(temporary.path()).expect("open journal");
    let line = append(&mut journal, "first", b"payload");
    let second_line = append(&mut journal, "second", b"a longer payload");
    drop(journal);
    let total_bytes = (line.len() + second_line.len()) as u64;
    let maximum_line = line.len().max(second_line.len());
    let before_entries = snapshot_files(temporary.path());

    let exact_limits = RecoveryInspectionLimits {
        max_checkpoint_bytes: 1024,
        max_journal_bytes: total_bytes,
        max_records: 2,
        max_record_line_bytes: maximum_line,
        ..RecoveryInspectionLimits::default()
    };
    assert_eq!(
        inspect_recovery_with_limits(temporary.path(), exact_limits)
            .expect("exact ceilings are accepted")
            .complete_record_count,
        2
    );

    let byte_limited = RecoveryInspectionLimits {
        max_checkpoint_bytes: 1024,
        max_journal_bytes: total_bytes - 1,
        max_records: usize::MAX,
        max_record_line_bytes: usize::MAX,
        ..RecoveryInspectionLimits::default()
    };
    assert!(inspect_recovery_with_limits(temporary.path(), byte_limited).is_err());
    assert_eq!(snapshot_files(temporary.path()), before_entries);

    let count_limited = RecoveryInspectionLimits {
        max_checkpoint_bytes: 1024,
        max_journal_bytes: u64::MAX,
        max_records: 1,
        max_record_line_bytes: usize::MAX,
        ..RecoveryInspectionLimits::default()
    };
    assert!(inspect_recovery_with_limits(temporary.path(), count_limited).is_err());

    let line_limited = RecoveryInspectionLimits {
        max_checkpoint_bytes: 1024,
        max_journal_bytes: u64::MAX,
        max_records: usize::MAX,
        max_record_line_bytes: maximum_line - 1,
        ..RecoveryInspectionLimits::default()
    };
    assert!(inspect_recovery_with_limits(temporary.path(), line_limited).is_err());
    assert_eq!(snapshot_files(temporary.path()), before_entries);
}

#[test]
fn cursor_exposes_checkpoint_before_stream_and_final_stats_after_torn_tail() {
    let temporary = TempDir::new().expect("temporary directory");
    let mut journal = RecoveryJournal::open(temporary.path()).expect("open journal");
    let first_line = append(&mut journal, "first", b"one");
    let second_line = append(&mut journal, "second", b"two");
    journal
        .checkpoint(1, "loom.test/1", b"checkpoint bytes")
        .expect("write checkpoint");
    drop(journal);

    let journal_path = temporary.path().join("operations.jsonl");
    let torn_tail = b"{\"interrupted\"";
    let mut journal_file = fs::OpenOptions::new()
        .append(true)
        .open(&journal_path)
        .expect("open journal for torn tail");
    journal_file.write_all(torn_tail).expect("write torn tail");
    drop(journal_file);
    let before = snapshot_files(temporary.path());

    let limits = RecoveryInspectionLimits {
        max_checkpoint_bytes: 1024,
        max_journal_bytes: 1024,
        max_records: 4,
        max_record_line_bytes: 1024,
        ..RecoveryInspectionLimits::default()
    };
    let mut cursor = RecoveryInspectionCursor::open_with_limits(temporary.path(), limits)
        .expect("open read-only cursor");

    assert_eq!(cursor.checkpoint(), Some(b"checkpoint bytes".as_slice()));
    assert_eq!(
        cursor
            .checkpoint_metadata()
            .map(|metadata| metadata.last_sequence),
        Some(1)
    );
    assert!(cursor.stats().is_none(), "stats are final only after EOF");
    assert_eq!(
        cursor
            .next_record()
            .expect("first record")
            .unwrap()
            .sequence,
        1
    );
    assert!(cursor.stats().is_none());
    assert_eq!(
        cursor
            .next_record()
            .expect("second record")
            .unwrap()
            .sequence,
        2
    );
    assert!(cursor.next_record().expect("EOF").is_none());

    let stats = cursor.stats().expect("final stats after EOF");
    assert_eq!(
        stats.journal_bytes,
        (first_line.len() + second_line.len() + torn_tail.len()) as u64
    );
    assert_eq!(stats.complete_record_count, 2);
    assert_eq!(
        stats.max_complete_line_bytes,
        first_line.len().max(second_line.len())
    );
    assert!(stats.incomplete_tail);
    assert_eq!(snapshot_files(temporary.path()), before);
}

#[test]
fn cursor_streams_past_a_smaller_collector_record_ceiling() {
    let temporary = TempDir::new().expect("temporary directory");
    let journal_path = temporary.path().join("operations.jsonl");
    let empty_payload_sha256 = super::sha256_hex(&[]);
    let mut bytes = Vec::new();
    for sequence in 1..=3 {
        let record = super::JournalRecord {
            sequence,
            operation_id: format!("stream-{sequence}"),
            label: "edit".into(),
            payload: Vec::new(),
            timestamp_ms: 0,
            payload_sha256: empty_payload_sha256.clone(),
        };
        serde_json::to_writer(&mut bytes, &record).expect("encode journal record");
        bytes.push(b'\n');
    }
    fs::write(&journal_path, &bytes).expect("write streamed records");

    // The small policy ceiling stands in for Sheets' 10,000-record write cap.
    let policy_record_ceiling = 2;
    let collector_limits = RecoveryInspectionLimits {
        max_records: policy_record_ceiling,
        ..RecoveryInspectionLimits::default()
    };
    assert!(inspect_recovery_with_limits(temporary.path(), collector_limits).is_err());

    let mut cursor_limits = collector_limits;
    cursor_limits.max_records = policy_record_ceiling + 1;
    let mut cursor = RecoveryInspectionCursor::open_with_limits(temporary.path(), cursor_limits)
        .expect("open cursor with larger finite read ceiling");
    let mut observed = 0;
    while let Some(record) = cursor.next_record().expect("stream next record") {
        observed += 1;
        assert_eq!(record.sequence, observed as u64);
    }

    assert_eq!(observed, 3);
    assert_eq!(
        cursor
            .stats()
            .expect("stats are complete after EOF")
            .complete_record_count,
        3
    );
}

#[test]
fn cursor_rejects_oversized_invalid_complete_line_before_json_parsing() {
    let temporary = TempDir::new().expect("temporary directory");
    let journal_path = temporary.path().join("operations.jsonl");
    let mut oversized_invalid_line = vec![b'x'; 128];
    oversized_invalid_line.push(b'\n');
    fs::write(&journal_path, &oversized_invalid_line).expect("write oversized invalid line");
    let before = fs::read(&journal_path).expect("read journal before inspection");

    let limits = RecoveryInspectionLimits {
        max_checkpoint_bytes: 1024,
        max_journal_bytes: 1024,
        max_records: 10,
        max_record_line_bytes: 64,
        ..RecoveryInspectionLimits::default()
    };
    let mut cursor = RecoveryInspectionCursor::open_with_limits(temporary.path(), limits)
        .expect("open cursor before reading journal");
    let error = cursor
        .next_record()
        .expect_err("oversized line must be rejected");

    assert!(
        error.to_string().contains("journal record line"),
        "line limit must be reported before JSON syntax: {error}"
    );
    assert_eq!(
        fs::read(journal_path).expect("journal after refusal"),
        before
    );
}

#[test]
fn cursor_cannot_resume_after_a_corrupt_interior_record() {
    let temporary = TempDir::new().expect("temporary directory");
    let mut journal = RecoveryJournal::open(temporary.path()).expect("open journal");
    let first_line = append(&mut journal, "first", b"one");
    let later_line = append(&mut journal, "later", b"must not be yielded");
    drop(journal);
    let journal_path = temporary.path().join("operations.jsonl");
    let mut bytes = first_line;
    bytes.extend_from_slice(b"{invalid}\n");
    bytes.extend_from_slice(&later_line);
    fs::write(&journal_path, bytes).expect("write interior corruption");

    let mut cursor = RecoveryInspectionCursor::open(temporary.path()).expect("open cursor");
    assert_eq!(
        cursor
            .next_record()
            .expect("verified prefix")
            .unwrap()
            .sequence,
        1
    );
    assert!(
        cursor.next_record().is_err(),
        "interior corruption is rejected"
    );
    assert!(
        cursor.next_record().is_err(),
        "cursor must remain failed instead of exposing records after corruption"
    );
}

#[test]
fn inspection_refuses_aggregate_checkpoint_metadata_bytes_before_parsing_candidates() {
    let temporary = TempDir::new().expect("temporary directory");
    let metadata_path = temporary.path().join("checkpoint-commit-1.json");
    let invalid_metadata = b"not-json";
    fs::write(&metadata_path, invalid_metadata).expect("write invalid pointer candidate");
    let before = snapshot_files(temporary.path());
    let limits = RecoveryInspectionLimits {
        max_checkpoint_metadata_bytes: 4,
        max_directory_entries: 10,
        ..RecoveryInspectionLimits::default()
    };

    let error = inspect_recovery_with_limits(temporary.path(), limits)
        .expect_err("aggregate metadata ceiling must reject inspection");

    assert!(
        error.to_string().contains("checkpoint metadata total"),
        "metadata ceiling must run before parsing invalid JSON: {error}"
    );
    assert_eq!(snapshot_files(temporary.path()), before);
}

#[test]
fn inspection_refuses_root_entry_count_before_parsing_checkpoint_candidates() {
    let temporary = TempDir::new().expect("temporary directory");
    fs::write(
        temporary.path().join("checkpoint-commit-1.json"),
        b"not-json",
    )
    .expect("write invalid pointer candidate");
    fs::write(temporary.path().join(".atomicwrite-abandoned"), b"temp")
        .expect("write non-metadata recovery entry");
    let before = snapshot_files(temporary.path());
    let limits = RecoveryInspectionLimits {
        max_checkpoint_metadata_bytes: 1024,
        max_directory_entries: 1,
        ..RecoveryInspectionLimits::default()
    };

    let error = inspect_recovery_with_limits(temporary.path(), limits)
        .expect_err("metadata entry ceiling must reject inspection");

    assert!(
        error.to_string().contains("recovery directory entry count"),
        "every direct entry must be counted before parsing invalid JSON: {error}"
    );
    assert_eq!(snapshot_files(temporary.path()), before);
}
