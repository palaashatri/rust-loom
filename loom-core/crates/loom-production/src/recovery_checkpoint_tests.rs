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

fn recovery_tree_entries(directory: &std::path::Path) -> Vec<(String, bool, Vec<u8>)> {
    fn collect(
        root: &std::path::Path,
        directory: &std::path::Path,
        entries: &mut Vec<(String, bool, Vec<u8>)>,
    ) {
        let mut children = fs::read_dir(directory)
            .expect("read recovery directory tree")
            .map(|entry| entry.expect("recovery tree entry").path())
            .collect::<Vec<_>>();
        children.sort();
        for path in children {
            let relative = path
                .strip_prefix(root)
                .expect("tree entry stays under recovery root")
                .to_string_lossy()
                .replace('\\', "/");
            let metadata = fs::symlink_metadata(&path).expect("read recovery tree metadata");
            if metadata.file_type().is_dir() {
                entries.push((relative, true, Vec::new()));
                collect(root, &path, entries);
            } else {
                entries.push((
                    relative,
                    false,
                    fs::read(&path).expect("read recovery tree file"),
                ));
            }
        }
    }

    let mut entries = Vec::new();
    collect(directory, directory, &mut entries);
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
        .checkpoint_and_compact_with_preflight("loom.test/1", 1, b"checkpoint two", |projection| {
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

#[test]
fn future_covered_checkpoint_sequence_refuses_before_preflight_or_mutation() {
    let temporary = tempfile::tempdir().expect("tempdir");
    let mut journal = RecoveryJournal::open(temporary.path()).expect("open journal");
    journal
        .checkpoint(0, "loom.test/1", b"accepted checkpoint")
        .expect("publish accepted checkpoint");
    journal.compact(0).expect("compact accepted checkpoint");
    journal
        .append_bounded("edit", "Edit", b"newer durable edit".to_vec())
        .expect("append newer durable edit");
    let before = recovery_entry_bytes(temporary.path());

    let mut preflight_called = false;
    let error = journal
        .checkpoint_and_compact_with_preflight("loom.test/1", 2, b"future checkpoint", |_| {
            preflight_called = true;
            Ok(())
        })
        .expect_err("checkpoint cannot cover a sequence beyond the journal frontier");

    assert!(error.to_string().contains("sequence"));
    assert!(
        !preflight_called,
        "sequence validation must precede preflight"
    );
    assert_eq!(recovery_entry_bytes(temporary.path()), before);
    let recovered = journal.recover().expect("read unchanged recovery journal");
    assert_eq!(recovered.checkpoint, Some(b"accepted checkpoint".to_vec()));
    assert_eq!(recovered.operations.len(), 1);
    assert_eq!(recovered.operations[0].sequence, 1);
}

#[test]
fn covered_checkpoint_preserves_a_newer_record_appended_by_another_handle() {
    let temporary = tempfile::tempdir().expect("tempdir");
    let mut initial = RecoveryJournal::open(temporary.path()).expect("open initial journal");
    initial
        .checkpoint(0, "loom.test/1", b"baseline")
        .expect("publish baseline");
    initial.compact(0).expect("compact baseline");
    initial
        .append_bounded("edit-one", "Edit", b"record one".to_vec())
        .expect("append sequence one");
    drop(initial);

    let mut checkpointing =
        RecoveryJournal::open(temporary.path()).expect("open checkpoint handle");
    let mut concurrent = RecoveryJournal::open(temporary.path()).expect("open concurrent handle");
    let journal_path = temporary.path().join(JOURNAL_FILE);
    let before_sequence_two = fs::read(&journal_path).expect("read sequence one bytes");
    assert_eq!(
        concurrent
            .append_bounded("edit-two", "Edit", b"record two".to_vec())
            .expect("append sequence two concurrently")
            .sequence,
        2
    );
    let after_sequence_two = fs::read(&journal_path).expect("read sequence two bytes");
    let sequence_two_bytes = after_sequence_two
        .as_slice()
        .strip_prefix(before_sequence_two.as_slice())
        .expect("second append preserves the first journal line")
        .to_vec();

    let outcome = checkpointing
        .checkpoint_and_compact_with_preflight(
            "loom.test/1",
            1,
            b"checkpoint through sequence one",
            |projection| {
                assert_eq!(projection.last_sequence, 1);
                assert_eq!(
                    projection.compacted_journal_bytes,
                    sequence_two_bytes.len() as u64
                );
                Ok(())
            },
        )
        .expect("publish the covered sequence while preserving sequence two");
    assert_eq!(outcome.last_sequence, 1);
    assert_eq!(outcome.compaction_error, None);
    assert_eq!(
        fs::read(&journal_path).expect("read compacted journal"),
        sequence_two_bytes
    );

    let recovered = checkpointing
        .recover()
        .expect("recover checkpoint and sequence two");
    assert_eq!(
        recovered.checkpoint,
        Some(b"checkpoint through sequence one".to_vec())
    );
    assert_eq!(recovered.operations.len(), 1);
    assert_eq!(recovered.operations[0].sequence, 2);
    assert_eq!(
        checkpointing
            .append_bounded("edit-three", "Edit", b"record three".to_vec())
            .expect("continue appending after concurrent checkpoint")
            .sequence,
        3
    );
}

#[test]
fn covered_sequence_older_than_checkpoint_refuses_without_mutation() {
    let temporary = tempfile::tempdir().expect("tempdir");
    let mut journal = RecoveryJournal::open(temporary.path()).expect("open journal");
    journal
        .checkpoint(0, "loom.test/1", b"first checkpoint")
        .expect("publish first checkpoint");
    journal.compact(0).expect("compact first checkpoint");
    journal
        .append_bounded("edit", "Edit", b"sequence one".to_vec())
        .expect("append sequence one");
    journal
        .checkpoint(1, "loom.test/1", b"second checkpoint")
        .expect("publish second checkpoint");
    journal.compact(1).expect("compact second checkpoint");
    let before = recovery_entry_bytes(temporary.path());

    let mut preflight_called = false;
    let error = journal
        .checkpoint_and_compact_with_preflight("loom.test/1", 0, b"older checkpoint", |_| {
            preflight_called = true;
            Ok(())
        })
        .expect_err("checkpoint cannot move behind the current checkpoint sequence");

    assert!(error.to_string().contains("sequence"));
    assert!(!preflight_called);
    assert_eq!(recovery_entry_bytes(temporary.path()), before);
}

#[test]
fn checkpoint_refuses_over_depth_abandoned_tree_before_preflight_or_cleanup() {
    let temporary = tempfile::tempdir().expect("tempdir");
    let mut journal = RecoveryJournal::open(temporary.path()).expect("open journal");
    journal
        .checkpoint(0, "loom.test/1", b"accepted checkpoint")
        .expect("publish accepted checkpoint");
    journal.compact(0).expect("compact accepted checkpoint");

    let abandoned = temporary.path().join(".atomicwrite-over-depth");
    fs::create_dir(&abandoned).expect("create abandoned atomic-write tree");
    let mut nested = abandoned.clone();
    for level in 0..32 {
        nested.push(format!("level-{level}"));
        fs::create_dir(&nested).expect("create nested atomic-write directory");
    }
    fs::write(nested.join("partial.bin"), b"partial write")
        .expect("write abandoned atomic-write payload");

    let obsolete = checkpoint_generation_payload_path(temporary.path(), 99);
    fs::write(&obsolete, b"obsolete generation to prune").expect("create obsolete generation");
    let before = recovery_tree_entries(temporary.path());
    let mut preflight_called = false;

    let error = journal
        .checkpoint_and_compact_with_preflight(
            "loom.test/1",
            0,
            b"checkpoint that must be refused",
            |_| {
                preflight_called = true;
                Ok(())
            },
        )
        .expect_err("over-depth abandoned trees must be refused before preflight");

    assert!(error.to_string().to_lowercase().contains("depth"));
    assert!(!preflight_called, "the capacity callback must not run");
    assert_eq!(recovery_tree_entries(temporary.path()), before);
    assert_eq!(
        fs::read(&obsolete).expect("obsolete generation survives"),
        b"obsolete generation to prune"
    );
}

#[test]
fn reconciliation_refuses_recursive_entry_count_before_pruning() {
    let temporary = tempfile::tempdir().expect("tempdir");
    let journal = RecoveryJournal::open(temporary.path()).expect("open journal");
    journal
        .checkpoint(0, "loom.test/1", b"accepted checkpoint")
        .expect("publish accepted checkpoint");
    journal.compact(0).expect("compact accepted checkpoint");

    let abandoned = temporary.path().join(".atomicwrite-entry-count");
    fs::create_dir(&abandoned).expect("create abandoned atomic-write tree");
    for entry in 0..4 {
        fs::write(abandoned.join(format!("partial-{entry}.tmp")), b"")
            .expect("create abandoned tree entry");
    }
    let obsolete = checkpoint_generation_payload_path(temporary.path(), 99);
    fs::write(&obsolete, b"obsolete generation to prune").expect("create obsolete generation");

    let root_entry_count = fs::read_dir(temporary.path())
        .expect("read recovery root")
        .count();
    let limits = RecoveryInspectionLimits {
        max_directory_entries: root_entry_count + 1,
        ..RecoveryInspectionLimits::default()
    };
    let before = recovery_tree_entries(temporary.path());
    let state = read_checkpoint_state(temporary.path()).expect("read current checkpoint state");

    let error = reconcile_checkpoint_generations_with_limits(temporary.path(), &state, limits)
        .expect_err("recursive abandoned-tree entries must consume the root entry budget");

    assert!(error.to_string().contains("entry count"));
    assert_eq!(recovery_tree_entries(temporary.path()), before);
    assert_eq!(
        fs::read(&obsolete).expect("obsolete generation survives"),
        b"obsolete generation to prune"
    );
}

#[test]
fn checkpoint_entry_projection_accounts_for_two_old_generations_and_staging() {
    let temporary = tempfile::tempdir().expect("tempdir");
    let mut journal = RecoveryJournal::open(temporary.path()).expect("open journal");
    journal
        .checkpoint(0, "loom.test/1", b"generation one")
        .expect("publish generation one");
    journal.compact(0).expect("compact generation one");
    journal
        .checkpoint(0, "loom.test/1", b"generation two")
        .expect("publish generation two");
    journal.compact(0).expect("compact generation two");

    let before = recovery_tree_entries(temporary.path());
    let root_and_recursive_entries = before.len();
    let next_generation =
        crate::checkpoint_generations::next_checkpoint_generation(temporary.path())
            .expect("next generation");
    let pointer_path = crate::checkpoint_generations::checkpoint_pointer_target_path(
        temporary.path(),
        next_generation,
    )
    .expect("pointer path");
    let journal_path = temporary.path().join(JOURNAL_FILE);
    let expected_peak_entries = root_and_recursive_entries
        + 2 // New generation payload and metadata.
        + usize::from(!pointer_path.exists())
        + usize::from(!journal_path.exists())
        + 2; // atomicwrites staging directory and child file.
    assert!(expected_peak_entries <= 10_000);

    let mut preflight_called = false;
    let error = journal
        .checkpoint_and_compact_with_entry_limit_for_test(
            "loom.test/1",
            0,
            b"generation three",
            expected_peak_entries - 1,
            |_| {
                preflight_called = true;
                Ok(())
            },
        )
        .expect_err("one entry below the projected peak must be refused");
    assert!(error.to_string().contains("entry count"));
    assert!(!preflight_called, "capacity refusal must precede callback");
    assert_eq!(recovery_tree_entries(temporary.path()), before);

    let outcome = journal
        .checkpoint_and_compact_with_entry_limit_for_test(
            "loom.test/1",
            0,
            b"generation three",
            expected_peak_entries,
            |_| Ok(()),
        )
        .expect("exact projected peak must fit");
    assert_eq!(outcome.compaction_error, None);
    assert!(
        recovery_tree_entries(temporary.path()).len() <= expected_peak_entries,
        "post-publication retained entries must remain within the same ceiling"
    );
    assert!(!checkpoint_generation_payload_path(temporary.path(), 1).exists());
    assert!(checkpoint_generation_payload_path(temporary.path(), 2).exists());
    assert!(checkpoint_generation_payload_path(temporary.path(), 3).exists());
}

#[test]
fn checkpoint_generation_discovery_refuses_before_unbounded_scan() {
    let temporary = tempfile::tempdir().expect("tempdir");
    let journal = RecoveryJournal::open(temporary.path()).expect("open journal");
    journal
        .checkpoint(0, "loom.test/1", b"generation one")
        .expect("publish generation one");
    journal.compact(0).expect("compact generation one");
    let before = recovery_tree_entries(temporary.path());
    let entry_count = before.len();

    let error = crate::checkpoint_generations::next_checkpoint_generation_with_limit_for_test(
        temporary.path(),
        entry_count - 1,
    )
    .expect_err("discovery must stop one entry before scanning the directory");

    assert!(error.to_string().contains("entry count"));
    assert_eq!(recovery_tree_entries(temporary.path()), before);
    assert_eq!(
        crate::checkpoint_generations::next_checkpoint_generation_with_limit_for_test(
            temporary.path(),
            entry_count,
        )
        .expect("exact directory entry ceiling fits"),
        2
    );
}

#[test]
fn legacy_generation_discovery_refuses_before_scanning_past_entry_limit() {
    let temporary = tempfile::tempdir().expect("tempdir");
    let journal = RecoveryJournal::open(temporary.path()).expect("open journal");
    journal
        .checkpoint(0, "loom.test/1", b"generation one")
        .expect("publish generation one");
    journal.compact(0).expect("compact generation one");
    journal
        .checkpoint(0, "loom.test/1", b"generation two")
        .expect("publish generation two");
    journal.compact(0).expect("compact generation two");
    let before = recovery_tree_entries(temporary.path());
    let entry_count = before.len();

    let error = crate::checkpoint_generations::legacy_previous_generation_with_limit_for_test(
        temporary.path(),
        3,
        entry_count - 1,
    )
    .expect_err("legacy discovery must enforce its injected directory entry ceiling");

    assert!(error.to_string().contains("entry count"));
    assert_eq!(recovery_tree_entries(temporary.path()), before);
    assert_eq!(
        crate::checkpoint_generations::legacy_previous_generation_with_limit_for_test(
            temporary.path(),
            3,
            entry_count,
        )
        .expect("exact directory entry ceiling fits"),
        Some(2)
    );
}
