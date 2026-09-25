use super::*;

fn limits(
    max_record_line_bytes: usize,
    max_journal_bytes: u64,
    max_records: usize,
) -> JournalAppendLimits {
    JournalAppendLimits {
        max_record_line_bytes,
        max_journal_bytes,
        max_records,
    }
}

fn expected_line_len(sequence: u64, operation_id: &str, label: &str, payload: &[u8]) -> usize {
    let record = JournalRecord {
        sequence,
        operation_id: operation_id.into(),
        label: label.into(),
        payload: payload.to_vec(),
        timestamp_ms: unix_time_ms(),
        payload_sha256: sha256_hex(payload),
    };
    serde_json::to_vec(&record)
        .expect("serialize expected record")
        .len()
        + 1
}

#[test]
fn bounded_append_persists_a_verified_record() {
    let temporary = tempfile::tempdir().expect("temporary recovery directory");
    let mut journal = RecoveryJournal::open(temporary.path()).expect("open journal");

    let record = journal
        .append_bounded("bounded", "Edit", b"value".to_vec())
        .expect("append within approved bounds");

    assert_eq!(record.sequence, 1);
    let records = journal.records().expect("read bounded record");
    assert_eq!(records, vec![record]);
}

#[test]
fn rejects_record_line_one_byte_over_without_changing_file_or_sequence() {
    let temporary = tempfile::tempdir().expect("temporary recovery directory");
    let mut journal = RecoveryJournal::open(temporary.path()).expect("open journal");
    let path = temporary.path().join(JOURNAL_FILE);
    let payload = vec![7; 12];
    let line_len = expected_line_len(1, "large-record", "Edit", &payload);
    let before = fs::read(&path).ok();

    let result = journal.append_with_limits(
        "large-record",
        "Edit",
        payload,
        limits(line_len - 1, u64::MAX, usize::MAX),
    );

    let error = result.expect_err("line cap must reject one byte over");
    assert!(error.to_string().contains("record line"), "{error}");
    assert_eq!(
        fs::read(&path).ok(),
        before,
        "refusal must not create or alter the journal"
    );
    let accepted = journal
        .append("after-refusal", "Edit", b"small".to_vec())
        .expect("unrestricted append after refusal");
    assert_eq!(
        accepted.sequence, 1,
        "refusal must not advance the next sequence"
    );
}

#[test]
fn bounded_refusal_preserves_a_torn_tail_and_keeps_sequence_valid() {
    let temporary = tempfile::tempdir().expect("temporary recovery directory");
    let mut journal = RecoveryJournal::open(temporary.path()).expect("open journal");
    let path = temporary.path().join(JOURNAL_FILE);
    fs::write(&path, b"{torn").expect("write interrupted append tail after open");
    let before = fs::read(&path).expect("read torn journal");
    let payload = b"candidate";
    let line_len = expected_line_len(1, "candidate", "Edit", payload);

    let result = journal.append_with_limits(
        "candidate",
        "Edit",
        payload.to_vec(),
        limits(line_len - 1, u64::MAX, usize::MAX),
    );

    let error = result.expect_err("record line cap must reject candidate");
    assert!(error.to_string().contains("record line"), "{error}");
    assert_eq!(fs::read(&path).expect("read after refusal"), before);
    let accepted = journal
        .append("after-refusal", "Edit", b"small".to_vec())
        .expect("unrestricted append repairs torn tail");
    assert_eq!(
        accepted.sequence, 1,
        "rejection must leave next sequence valid"
    );
}

#[test]
fn bounded_append_rejects_an_over_limit_journal_before_repair() {
    let temporary = tempfile::tempdir().expect("temporary recovery directory");
    let mut journal = RecoveryJournal::open(temporary.path()).expect("open journal");
    let path = temporary.path().join(JOURNAL_FILE);
    fs::write(&path, b"\xff").expect("write invalid UTF-8 journal after open");
    let before = fs::read(&path).expect("read oversized journal");

    let result = journal.append_with_limits(
        "candidate",
        "Edit",
        b"small".to_vec(),
        limits(1024, before.len() as u64 - 1, usize::MAX),
    );

    let error = result.expect_err("existing journal already exceeds byte cap");
    assert!(error.to_string().contains("already total"), "{error}");
    assert_eq!(fs::read(&path).expect("read after refusal"), before);
}

#[test]
fn rejects_total_journal_one_byte_over_without_changing_file_or_sequence() {
    let temporary = tempfile::tempdir().expect("temporary recovery directory");
    let mut journal = RecoveryJournal::open(temporary.path()).expect("open journal");
    journal
        .append("seed", "Edit", b"saved".to_vec())
        .expect("seed journal");
    let path = temporary.path().join(JOURNAL_FILE);
    let before = fs::read(&path).expect("read seeded journal");
    let payload = b"next";
    let candidate_line_len = expected_line_len(2, "next", "Edit", payload);
    let max_journal_bytes = before.len() as u64 + candidate_line_len as u64 - 1;

    let result = journal.append_with_limits(
        "next",
        "Edit",
        payload.to_vec(),
        limits(1024, max_journal_bytes, 10),
    );

    let error = result.expect_err("journal cap must reject one byte over");
    assert!(error.to_string().contains("journal bytes"), "{error}");
    assert_eq!(fs::read(&path).expect("read journal after refusal"), before);
    let accepted = journal
        .append("after-refusal", "Edit", b"small".to_vec())
        .expect("unrestricted append after refusal");
    assert_eq!(
        accepted.sequence, 2,
        "refusal must not advance the next sequence"
    );
}

#[test]
fn rejects_record_count_one_over_without_changing_file_or_sequence() {
    let temporary = tempfile::tempdir().expect("temporary recovery directory");
    let mut journal = RecoveryJournal::open(temporary.path()).expect("open journal");
    journal
        .append("seed", "Edit", b"saved".to_vec())
        .expect("seed journal");
    let path = temporary.path().join(JOURNAL_FILE);
    let before = fs::read(&path).expect("read seeded journal");

    let result =
        journal.append_with_limits("next", "Edit", b"next".to_vec(), limits(1024, u64::MAX, 1));

    let error = result.expect_err("record cap must reject the second record");
    assert!(error.to_string().contains("records"), "{error}");
    assert_eq!(fs::read(&path).expect("read journal after refusal"), before);
    let accepted = journal
        .append("after-refusal", "Edit", b"small".to_vec())
        .expect("unrestricted append after refusal");
    assert_eq!(
        accepted.sequence, 2,
        "refusal must not advance the next sequence"
    );
}

#[test]
fn accepts_exact_record_journal_and_count_boundaries_including_newline() {
    let temporary = tempfile::tempdir().expect("temporary recovery directory");
    let mut journal = RecoveryJournal::open(temporary.path()).expect("open journal");
    let path = temporary.path().join(JOURNAL_FILE);
    let payload = b"edge";
    let line_len = expected_line_len(1, "edge", "Edit", payload);

    let record = journal
        .append_with_limits(
            "edge",
            "Edit",
            payload.to_vec(),
            limits(line_len, line_len as u64, 1),
        )
        .expect("equality with every cap is allowed");

    let bytes = fs::read(&path).expect("read boundary journal");
    assert_eq!(record.sequence, 1);
    assert_eq!(
        bytes.len(),
        line_len,
        "the JSONL newline is within the byte limit"
    );
    assert_eq!(bytes.last(), Some(&b'\n'));
    assert_eq!(journal.records().expect("read boundary record").len(), 1);
}
