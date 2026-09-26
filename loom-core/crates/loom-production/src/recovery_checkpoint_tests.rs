use super::*;
use fs2::FileExt;
use std::fs::{self, OpenOptions};
use std::io::Write;

fn recovery_entry_bytes(directory: &std::path::Path) -> Vec<(String, Vec<u8>)> {
    let mut entries = fs::read_dir(directory)
        .expect("read recovery directory")
        .map(|entry| {
            let entry = entry.expect("recovery entry");
            let name = entry.file_name().to_string_lossy().into_owned();
            (name, fs::read(entry.path()).expect("read recovery file"))
        })
        .filter(|(name, _)| name != CHECKPOINT_LOCK_FILE)
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    entries
}

fn assert_checkpoint_lock_is_held(directory: &std::path::Path) {
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .open(directory.join(CHECKPOINT_LOCK_FILE))
        .expect("open checkpoint lock from callback");
    let error = lock
        .try_lock_exclusive()
        .expect_err("preflight callback runs under the shared checkpoint lock");
    assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock);
}

#[test]
fn append_preflight_runs_under_lock_before_torn_tail_repair() {
    let temporary = tempfile::tempdir().expect("tempdir");
    let mut journal = RecoveryJournal::open(temporary.path()).expect("open journal");
    journal
        .append_bounded("first", "Edit", b"first".to_vec())
        .expect("append committed record");
    let journal_path = temporary.path().join(JOURNAL_FILE);
    let mut tail = OpenOptions::new()
        .append(true)
        .open(&journal_path)
        .expect("open journal for torn tail");
    tail.write_all(b"{interrupted").expect("write torn tail");
    drop(tail);
    let before = recovery_entry_bytes(temporary.path());

    let error = journal
        .append_bounded_with_preflight("second", "Edit", b"second".to_vec(), |projection| {
            assert_checkpoint_lock_is_held(temporary.path());
            assert_eq!(
                projection.current_journal_bytes,
                fs::metadata(&journal_path).expect("journal metadata").len()
            );
            assert!(projection.appended_line_bytes > 0);
            Err(ProductionError::InvalidData(
                "recovery capacity refusal: test limit".into(),
            ))
        })
        .expect_err("preflight refusal must preserve the torn tail");

    assert!(error.to_string().contains("recovery capacity refusal"));
    assert_eq!(recovery_entry_bytes(temporary.path()), before);
    assert_eq!(
        journal
            .append_bounded("second", "Edit", b"second".to_vec())
            .expect("append after explicit refusal")
            .sequence,
        2
    );
}

#[test]
fn checkpoint_preflight_runs_under_lock_before_torn_tail_repair_or_publication() {
    let temporary = tempfile::tempdir().expect("tempdir");
    let mut journal = RecoveryJournal::open(temporary.path()).expect("open journal");
    journal
        .checkpoint(0, "loom.test/1", b"checkpoint one")
        .expect("publish first checkpoint");
    journal.compact(0).expect("compact first checkpoint");
    journal
        .append_bounded("edit", "Edit", b"one".to_vec())
        .expect("append committed record");
    let journal_path = temporary.path().join(JOURNAL_FILE);
    let mut tail = OpenOptions::new()
        .append(true)
        .open(&journal_path)
        .expect("open journal for torn tail");
    tail.write_all(b"{interrupted").expect("write torn tail");
    drop(tail);
    let before = recovery_entry_bytes(temporary.path());

    let error = journal
        .checkpoint_and_compact_with_preflight("loom.test/1", b"checkpoint two", |projection| {
            assert_checkpoint_lock_is_held(temporary.path());
            assert_eq!(projection.last_sequence, 1);
            assert!(projection.package_bytes > 0);
            Err(ProductionError::InvalidData(
                "recovery capacity refusal: test limit".into(),
            ))
        })
        .expect_err("preflight refusal must not repair or publish");

    assert!(error.to_string().contains("recovery capacity refusal"));
    assert_eq!(recovery_entry_bytes(temporary.path()), before);
}
