use super::*;

fn complete_checkpoint_payloads(directory: &std::path::Path) -> Vec<Vec<u8>> {
    let mut generations = Vec::new();
    for entry in fs::read_dir(directory).expect("read checkpoint generations") {
        let entry = entry.expect("read checkpoint entry");
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        let Some(generation) = name
            .strip_prefix("checkpoint-generation-")
            .and_then(|name| name.strip_suffix(".bin"))
            .and_then(|generation| generation.parse::<u64>().ok())
        else {
            continue;
        };
        let metadata_path = directory.join(format!("checkpoint-generation-{generation}.json"));
        let payload_path = directory.join(format!("checkpoint-generation-{generation}.bin"));
        if !metadata_path.is_file() || !payload_path.is_file() {
            continue;
        }
        let payload = fs::read(&payload_path).expect("read checkpoint payload");
        let metadata: CheckpointMetadata =
            serde_json::from_slice(&fs::read(metadata_path).expect("read checkpoint metadata"))
                .expect("parse checkpoint metadata");
        assert_eq!(
            sha256_hex(&payload),
            metadata.sha256,
            "generation {generation} payload matches its metadata"
        );
        generations.push((generation, payload));
    }
    generations.sort_by_key(|(generation, _)| *generation);
    generations
        .into_iter()
        .map(|(_, payload)| payload)
        .collect()
}

#[test]
fn open_removes_abandoned_atomic_write_temp_after_recovering_checkpoint() {
    let temporary = tempfile::tempdir().expect("tempdir");
    {
        let journal = RecoveryJournal::open(temporary.path()).expect("journal");
        journal
            .checkpoint(0, "loom.test/1", b"valid checkpoint")
            .expect("publish valid checkpoint");
    }

    let abandoned = temporary.path().join(".atomicwrite-abandoned-test");
    fs::create_dir(&abandoned).expect("create abandoned atomic-write temp directory");
    fs::write(abandoned.join("tmpfile.tmp"), b"partial checkpoint")
        .expect("create abandoned atomic-write temp file");

    let reopened = RecoveryJournal::open(temporary.path()).expect("reopen");
    let state = reopened.recover().expect("recover valid checkpoint");
    assert_eq!(
        state.checkpoint.as_deref(),
        Some(b"valid checkpoint".as_slice())
    );
    assert_eq!(
        complete_checkpoint_payloads(temporary.path()),
        vec![b"valid checkpoint".to_vec()]
    );
    assert!(
        !abandoned.exists(),
        "startup removes the abandoned atomic-write temporary directory"
    );
}

#[test]
fn publishing_generations_preserves_direct_legacy_checkpoint_files() {
    let temporary = tempfile::tempdir().expect("tempdir");
    let legacy_payload = b"legacy checkpoint";
    let legacy_metadata = CheckpointMetadata {
        last_sequence: 0,
        sha256: sha256_hex(legacy_payload),
        timestamp_ms: unix_time_ms(),
        schema: "loom.test/legacy".into(),
    };
    let legacy_metadata_bytes =
        serde_json::to_vec_pretty(&legacy_metadata).expect("encode legacy checkpoint metadata");
    let legacy_payload_path = temporary.path().join(CHECKPOINT_FILE);
    let legacy_metadata_path = temporary.path().join(CHECKPOINT_META_FILE);
    fs::write(&legacy_payload_path, legacy_payload).expect("write legacy checkpoint");
    fs::write(&legacy_metadata_path, &legacy_metadata_bytes)
        .expect("write legacy checkpoint metadata");

    let journal = RecoveryJournal::open(temporary.path()).expect("open legacy checkpoint");
    journal
        .checkpoint(0, "loom.test/1", b"versioned checkpoint")
        .expect("publish a versioned checkpoint");
    journal.compact(0).expect("compact versioned checkpoint");
    drop(journal);

    let reopened = RecoveryJournal::open(temporary.path()).expect("reopen versioned checkpoint");
    assert_eq!(
        fs::read(&legacy_payload_path).expect("read preserved legacy payload"),
        legacy_payload
    );
    assert_eq!(
        fs::read(&legacy_metadata_path).expect("read preserved legacy metadata"),
        legacy_metadata_bytes
    );
    assert_eq!(
        reopened
            .recover()
            .expect("recover versioned checkpoint")
            .checkpoint
            .as_deref(),
        Some(b"versioned checkpoint".as_slice())
    );
}

#[test]
fn three_checkpoints_retain_current_and_previous_after_reopen() {
    let temporary = tempfile::tempdir().expect("tempdir");
    {
        let journal = RecoveryJournal::open(temporary.path()).expect("journal");
        journal
            .checkpoint(0, "loom.test/1", b"checkpoint A")
            .expect("publish checkpoint A");
        journal.compact(0).expect("compact through checkpoint A");
        journal
            .checkpoint(0, "loom.test/1", b"checkpoint B")
            .expect("publish checkpoint B");
        journal.compact(0).expect("compact through checkpoint B");
        journal
            .checkpoint(0, "loom.test/1", b"checkpoint C")
            .expect("publish checkpoint C");
        journal.compact(0).expect("compact through checkpoint C");
    }

    let reopened = RecoveryJournal::open(temporary.path()).expect("reopen");
    let state = reopened.recover().expect("recover current checkpoint");
    assert_eq!(
        state.checkpoint.as_deref(),
        Some(b"checkpoint C".as_slice())
    );
    assert_eq!(
        complete_checkpoint_payloads(temporary.path()),
        vec![b"checkpoint B".to_vec(), b"checkpoint C".to_vec()]
    );
}

#[test]
fn failed_checkpoint_publication_preserves_state_and_retry_retains_previous() {
    let temporary = tempfile::tempdir().expect("tempdir");
    let mut journal = RecoveryJournal::open(temporary.path()).expect("journal");
    journal
        .append("one", "Edit", b"edit one".to_vec())
        .expect("append edit one");
    journal
        .checkpoint(1, "loom.test/1", b"checkpoint A")
        .expect("publish checkpoint A");
    journal.compact(1).expect("compact through checkpoint A");
    journal
        .append("two", "Edit", b"edit two".to_vec())
        .expect("append edit two");
    journal
        .checkpoint(2, "loom.test/1", b"checkpoint B")
        .expect("publish checkpoint B");
    journal.compact(2).expect("compact through checkpoint B");
    journal
        .append("three", "Edit", b"edit three".to_vec())
        .expect("append edit three");

    let blocker = temporary.path().join(format!(
        ".{CHECKPOINT_META_FILE}.{}.tmp",
        std::process::id()
    ));
    fs::create_dir(&blocker).expect("block checkpoint pointer publication");
    assert!(journal
        .checkpoint(3, "loom.test/1", b"checkpoint C")
        .is_err());
    assert_eq!(
        complete_checkpoint_payloads(temporary.path()),
        vec![
            b"checkpoint A".to_vec(),
            b"checkpoint B".to_vec(),
            b"checkpoint C".to_vec()
        ],
        "the failed publication leaves at most one complete orphan"
    );
    assert!(journal
        .checkpoint(3, "loom.test/1", b"checkpoint C")
        .is_err());
    assert_eq!(
        complete_checkpoint_payloads(temporary.path()),
        vec![
            b"checkpoint A".to_vec(),
            b"checkpoint B".to_vec(),
            b"checkpoint C".to_vec()
        ],
        "another failed retry must remove its previous orphan before writing"
    );
    fs::remove_dir(&blocker).expect("remove checkpoint pointer blocker");
    drop(journal);

    let reopened = RecoveryJournal::open(temporary.path()).expect("reopen");
    let state = reopened
        .recover()
        .expect("recover checkpoint B and later edit");
    assert_eq!(
        state.checkpoint.as_deref(),
        Some(b"checkpoint B".as_slice())
    );
    assert_eq!(state.operations.len(), 1);
    assert_eq!(state.operations[0].payload, b"edit three");
    assert_eq!(
        complete_checkpoint_payloads(temporary.path()),
        vec![b"checkpoint A".to_vec(), b"checkpoint B".to_vec()]
    );

    reopened
        .checkpoint(3, "loom.test/1", b"checkpoint C")
        .expect("retry checkpoint C");
    reopened.compact(3).expect("compact through checkpoint C");
    drop(reopened);

    let restarted = RecoveryJournal::open(temporary.path()).expect("restart after retry");
    let state = restarted.recover().expect("recover checkpoint C");
    assert_eq!(
        state.checkpoint.as_deref(),
        Some(b"checkpoint C".as_slice())
    );
    assert!(state.operations.is_empty());
    assert_eq!(
        complete_checkpoint_payloads(temporary.path()),
        vec![b"checkpoint B".to_vec(), b"checkpoint C".to_vec()]
    );
}

#[test]
fn failed_checkpoint_cleanup_preserves_current_and_previous_until_restart_retry() {
    let temporary = tempfile::tempdir().expect("tempdir");
    let obsolete_payload = temporary.path().join("checkpoint-generation-1.bin");
    let obsolete_metadata = temporary.path().join("checkpoint-generation-1.json");
    {
        let journal = RecoveryJournal::open(temporary.path()).expect("journal");
        journal
            .checkpoint(0, "loom.test/1", b"checkpoint A")
            .expect("publish checkpoint A");
        journal.compact(0).expect("compact through checkpoint A");
        journal
            .checkpoint(0, "loom.test/1", b"checkpoint B")
            .expect("publish checkpoint B");
        journal.compact(0).expect("compact through checkpoint B");

        if obsolete_payload.exists() {
            fs::remove_file(&obsolete_payload).expect("remove obsolete payload");
        }
        fs::create_dir(&obsolete_payload).expect("obstruct obsolete payload path");
        let sentinel = obsolete_payload.join("keep.txt");
        fs::write(&sentinel, b"preserve the obstruction").expect("write obstruction sentinel");

        journal
            .checkpoint(0, "loom.test/1", b"checkpoint C")
            .expect("publish checkpoint C");
        assert!(
            journal.compact(0).is_err(),
            "cleanup must report the obstructed obsolete generation"
        );

        let state = journal.recover().expect("recover current checkpoint C");
        assert_eq!(
            state.checkpoint.as_deref(),
            Some(b"checkpoint C".as_slice())
        );
        assert_eq!(
            complete_checkpoint_payloads(temporary.path()),
            vec![b"checkpoint B".to_vec(), b"checkpoint C".to_vec()]
        );
        assert!(
            sentinel.is_file(),
            "cleanup must not recurse into a directory"
        );
    }

    fs::remove_dir_all(&obsolete_payload).expect("remove cleanup obstruction");
    for _ in 0..2 {
        let reopened = RecoveryJournal::open(temporary.path()).expect("reopen");
        let state = reopened
            .recover()
            .expect("recover checkpoint C after retry");
        assert_eq!(
            state.checkpoint.as_deref(),
            Some(b"checkpoint C".as_slice())
        );
        assert_eq!(
            complete_checkpoint_payloads(temporary.path()),
            vec![b"checkpoint B".to_vec(), b"checkpoint C".to_vec()]
        );
        drop(reopened);
    }
    assert!(
        !obsolete_metadata.exists(),
        "restart cleanup removes metadata for the obsolete generation"
    );
}

#[test]
fn new_checkpoint_retains_its_named_predecessor_over_a_complete_lower_orphan() {
    let temporary = tempfile::tempdir().expect("tempdir");
    let journal = RecoveryJournal::open(temporary.path()).expect("journal");
    journal
        .checkpoint(0, "loom.test/1", b"checkpoint A")
        .expect("publish checkpoint A");
    journal.compact(0).expect("compact checkpoint A");

    let orphan_payload = b"unpublished checkpoint B";
    let orphan_metadata = CheckpointMetadata {
        last_sequence: 0,
        sha256: sha256_hex(orphan_payload),
        timestamp_ms: unix_time_ms(),
        schema: "loom.test/1".into(),
    };
    fs::write(
        checkpoint_generation_payload_path(temporary.path(), 2),
        orphan_payload,
    )
    .expect("write unpublished checkpoint payload");
    fs::write(
        checkpoint_generation_metadata_path(temporary.path(), 2),
        serde_json::to_vec(&orphan_metadata).expect("encode unpublished metadata"),
    )
    .expect("write unpublished checkpoint metadata");

    journal
        .checkpoint(0, "loom.test/1", b"checkpoint C")
        .expect("publish checkpoint C");
    journal.compact(0).expect("compact checkpoint C");

    assert_eq!(
        journal
            .recover()
            .expect("recover checkpoint C")
            .checkpoint
            .as_deref(),
        Some(b"checkpoint C".as_slice())
    );
    assert_eq!(
        complete_checkpoint_payloads(temporary.path()),
        vec![b"checkpoint A".to_vec(), b"checkpoint C".to_vec()]
    );
}

#[test]
fn recovery_selects_the_newest_valid_pointer_across_platform_formats() {
    let temporary = tempfile::tempdir().expect("tempdir");
    let journal = RecoveryJournal::open(temporary.path()).expect("journal");
    journal
        .checkpoint(0, "loom.test/1", b"checkpoint A")
        .expect("publish checkpoint A");

    let payload = b"checkpoint B";
    let metadata = CheckpointMetadata {
        last_sequence: 0,
        sha256: sha256_hex(payload),
        timestamp_ms: unix_time_ms(),
        schema: "loom.test/1".into(),
    };
    fs::write(
        checkpoint_generation_payload_path(temporary.path(), 2),
        payload,
    )
    .expect("write checkpoint B payload");
    fs::write(
        checkpoint_generation_metadata_path(temporary.path(), 2),
        serde_json::to_vec(&metadata).expect("encode checkpoint B metadata"),
    )
    .expect("write checkpoint B metadata");
    let pointer_path = if cfg!(windows) {
        temporary.path().join(CHECKPOINT_META_FILE)
    } else {
        temporary.path().join("checkpoint-commit-2.json")
    };
    fs::write(
        pointer_path,
        serde_json::to_vec(&serde_json::json!({
            "generation": 2,
            "last_sequence": 0,
            "sha256": metadata.sha256,
            "timestamp_ms": metadata.timestamp_ms,
            "schema": metadata.schema,
            "previous_generation": 1
        }))
        .expect("encode alternate-platform pointer"),
    )
    .expect("write newer alternate-platform pointer");
    drop(journal);

    let reopened = RecoveryJournal::open(temporary.path()).expect("reopen");
    let state = reopened.recover().expect("recover newest pointer");
    assert_eq!(
        state.checkpoint.as_deref(),
        Some(b"checkpoint B".as_slice())
    );
}

#[test]
fn conflicting_same_generation_pointer_formats_fail_integrity() {
    let temporary = tempfile::tempdir().expect("tempdir");
    let journal = RecoveryJournal::open(temporary.path()).expect("journal");
    journal
        .checkpoint(0, "loom.test/1", b"checkpoint A")
        .expect("publish checkpoint A");
    let pointer_path = if cfg!(windows) {
        temporary.path().join(CHECKPOINT_META_FILE)
    } else {
        temporary.path().join("checkpoint-commit-1.json")
    };
    fs::write(
        pointer_path,
        serde_json::to_vec(&serde_json::json!({
            "generation": 1,
            "last_sequence": 0,
            "sha256": sha256_hex(b"different payload"),
            "timestamp_ms": 1,
            "schema": "loom.test/conflict"
        }))
        .expect("encode conflicting pointer"),
    )
    .expect("write conflicting pointer format");
    drop(journal);

    let outcome = RecoveryJournal::open(temporary.path()).and_then(|reopened| reopened.recover());
    assert!(
        matches!(outcome, Err(ProductionError::Integrity(_))),
        "conflicting pointers for one generation must fail integrity: {outcome:?}"
    );
}
