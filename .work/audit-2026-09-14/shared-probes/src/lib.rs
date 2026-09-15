#[cfg(test)]
mod tests {
    use loom_production::snapshot::SnapshotRecovery;
    use std::fs;
    use std::path::PathBuf;

    fn scratch(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("loom-audit-{name}-{}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn edits_after_checkpoint_and_restart_must_recover() {
        let dir = scratch("sequence");
        {
            let mut recovery = SnapshotRecovery::open_at(&dir).unwrap();
            recovery.record("first edit", b"saved contents".to_vec()).unwrap();
            recovery.checkpoint("test/1", b"saved contents".to_vec()).unwrap();
        }
        {
            let mut recovery = SnapshotRecovery::open_at(&dir).unwrap();
            recovery.record("edit after restart", b"new unsaved edit".to_vec()).unwrap();
        }
        let recovery = SnapshotRecovery::open_at(&dir).unwrap();
        let actual = recovery.restored_payload().unwrap().to_vec();
        fs::remove_dir_all(&dir).unwrap();
        assert_eq!(actual, b"new unsaved edit", "recovery silently rolled back to saved contents");
    }

    #[test]
    fn metadata_write_failure_must_preserve_recoverable_state() {
        let dir = scratch("checkpoint");
        let mut recovery = SnapshotRecovery::open_at(&dir).unwrap();
        recovery.record("first edit", b"old contents".to_vec()).unwrap();
        recovery.checkpoint("test/1", b"old contents".to_vec()).unwrap();
        recovery.record("second edit", b"latest contents".to_vec()).unwrap();
        // A deterministic I/O failure at metadata-temp creation, after payload replacement.
        let blocker = dir.join(format!(".checkpoint.json.{}.tmp", std::process::id()));
        fs::create_dir(&blocker).unwrap();
        assert!(recovery.checkpoint("test/1", b"latest contents".to_vec()).is_err());
        fs::remove_dir(&blocker).unwrap();
        drop(recovery);
        let reopened = SnapshotRecovery::open_at(&dir);
        fs::remove_dir_all(&dir).unwrap();
        assert!(reopened.is_ok(), "valid journal is inaccessible after partial checkpoint: {reopened:?}");
    }

    #[test]
    fn failed_journal_append_must_not_consume_sequence() {
        let dir = scratch("append");
        let path = dir.join("operations.bin");
        let mut journal = loom_storage::RecoveryJournal::open(&path).unwrap();
        fs::create_dir(&path).unwrap();
        assert!(journal.append("edit", b"failed write".to_vec()).is_err());
        fs::remove_dir(&path).unwrap();
        journal.append("edit", b"durable write".to_vec()).unwrap();
        drop(journal);
        let reopened = loom_storage::RecoveryJournal::open(&path);
        fs::remove_dir_all(&dir).unwrap();
        assert!(reopened.is_ok(), "successful retry left an unreadable journal: {reopened:?}");
    }
}
