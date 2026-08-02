//! Deduplicating full-state recovery coordinator for application packages.

use crate::{ProductionError, RecoveryJournal};
use std::fs;
use std::path::{Path, PathBuf};

/// Default state directory for one reverse-DNS-style application id.
///
/// Linux uses `XDG_STATE_HOME` or `~/.local/state`; Windows uses
/// `LOCALAPPDATA`; macOS uses `~/Library/Application Support`. A relative local
/// directory is used only when no platform home variable is available.
pub fn application_state_directory(application_id: &str) -> Result<PathBuf, ProductionError> {
    if application_id.is_empty()
        || application_id.len() > 160
        || !application_id.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
    {
        return Err(ProductionError::InvalidData(
            "application id contains unsupported characters".into(),
        ));
    }
    let base = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .map(|home| home.join("Library").join("Application Support"))
    } else {
        std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(PathBuf::from)
                    .map(|home| home.join(".local").join("state"))
            })
    }
    .unwrap_or_else(|| PathBuf::from(".loom-state"));
    Ok(base.join("loom").join(application_id))
}

/// Application-facing coordinator that records complete native package bytes.
///
/// Repeated snapshots are deduplicated. On explicit save, a checkpoint is
/// written and older journal entries are compacted. The newest valid payload is
/// exposed for startup recovery.
#[derive(Debug)]
pub struct SnapshotRecovery {
    journal: RecoveryJournal,
    restored_payload: Option<Vec<u8>>,
    last_payload: Option<Vec<u8>>,
    last_sequence: u64,
}

impl SnapshotRecovery {
    /// Open a platform-default recovery location for an application id.
    pub fn open(application_id: &str) -> Result<Self, ProductionError> {
        Self::open_at(application_state_directory(application_id)?)
    }

    /// Open an explicit recovery directory, primarily for portable mode and tests.
    pub fn open_at(directory: impl AsRef<Path>) -> Result<Self, ProductionError> {
        let journal = RecoveryJournal::open(directory)?;
        let recovered = journal.recover()?;
        let checkpoint_sequence = recovered
            .checkpoint_metadata
            .as_ref()
            .map_or(0, |metadata| metadata.last_sequence);
        let newest_operation = recovered.operations.last();
        let restored_payload = newest_operation
            .map(|record| record.payload.clone())
            .or(recovered.checkpoint);
        let last_sequence = newest_operation.map_or(checkpoint_sequence, |record| record.sequence);
        Ok(Self {
            journal,
            last_payload: restored_payload.clone(),
            restored_payload,
            last_sequence,
        })
    }

    /// Directory containing checkpoint and operation journal files.
    pub fn directory(&self) -> &Path {
        self.journal.directory()
    }

    /// Newest valid payload found during startup recovery.
    pub fn restored_payload(&self) -> Option<&[u8]> {
        self.restored_payload.as_deref()
    }

    /// Consume and return the startup recovery payload exactly once.
    pub fn take_restored_payload(&mut self) -> Option<Vec<u8>> {
        self.restored_payload.take()
    }

    /// Record a complete state snapshot if it differs from the latest payload.
    /// Returns `true` when a durable journal record was written.
    pub fn record(&mut self, label: &str, payload: Vec<u8>) -> Result<bool, ProductionError> {
        if label.trim().is_empty() {
            return Err(ProductionError::InvalidData(
                "snapshot label must not be empty".into(),
            ));
        }
        if self.last_payload.as_deref() == Some(payload.as_slice()) {
            return Ok(false);
        }
        let operation_id = format!("snapshot-{}", self.last_sequence.saturating_add(1));
        let record = self.journal.append(operation_id, label, payload.clone())?;
        self.last_sequence = record.sequence;
        self.last_payload = Some(payload);
        Ok(true)
    }

    /// Checkpoint a saved package and compact all journal operations represented
    /// by that package.
    pub fn checkpoint(&mut self, schema: &str, payload: Vec<u8>) -> Result<(), ProductionError> {
        if schema.trim().is_empty() {
            return Err(ProductionError::InvalidData(
                "checkpoint schema must not be empty".into(),
            ));
        }
        if self.last_payload.as_deref() != Some(payload.as_slice()) {
            let _ = self.record("save checkpoint", payload.clone())?;
        }
        self.journal
            .checkpoint(self.last_sequence, schema, &payload)?;
        self.journal.compact(self.last_sequence)?;
        self.last_payload = Some(payload);
        Ok(())
    }

    /// Delete all recovery data after a document is intentionally discarded.
    pub fn clear(self) -> Result<(), ProductionError> {
        let directory = self.journal.directory().to_path_buf();
        drop(self.journal);
        if directory.exists() {
            fs::remove_dir_all(directory)?;
        }
        Ok(())
    }
}

/// Define one thread-local recovery slot for a desktop application.
///
/// The generated helpers are intentionally private to the invoking binary:
/// `initialize_snapshot_recovery`, `record_snapshot_recovery`, and
/// `checkpoint_snapshot_recovery`. Headless render paths that never initialize
/// the slot safely treat recording as a no-op.
#[macro_export]
macro_rules! define_snapshot_recovery {
    ($slot:ident, $application_id:literal, $schema:literal) => {
        std::thread_local! {
            static $slot: std::cell::RefCell<Option<$crate::snapshot::SnapshotRecovery>> =
                std::cell::RefCell::new(None);
        }

        fn initialize_snapshot_recovery() -> Result<Option<Vec<u8>>, String> {
            let mut recovery = $crate::snapshot::SnapshotRecovery::open($application_id)
                .map_err(|error| error.to_string())?;
            let restored = recovery.take_restored_payload();
            $slot.with(|slot| {
                *slot.borrow_mut() = Some(recovery);
            });
            Ok(restored)
        }

        fn record_snapshot_recovery(label: &str, payload: Vec<u8>) -> Result<(), String> {
            $slot.with(|slot| {
                let mut slot = slot.borrow_mut();
                match slot.as_mut() {
                    Some(recovery) => recovery
                        .record(label, payload)
                        .map(|_| ())
                        .map_err(|error| error.to_string()),
                    None => Ok(()),
                }
            })
        }

        fn checkpoint_snapshot_recovery(payload: Vec<u8>) -> Result<(), String> {
            $slot.with(|slot| {
                let mut slot = slot.borrow_mut();
                match slot.as_mut() {
                    Some(recovery) => recovery
                        .checkpoint($schema, payload)
                        .map_err(|error| error.to_string()),
                    None => Ok(()),
                }
            })
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshots_deduplicate_recover_and_checkpoint() {
        let temporary = tempfile::tempdir().expect("tempdir");
        {
            let mut recovery = SnapshotRecovery::open_at(temporary.path()).expect("open");
            assert!(recovery.record("edit", b"one".to_vec()).expect("record"));
            assert!(!recovery.record("edit", b"one".to_vec()).expect("dedupe"));
            assert!(recovery.record("edit", b"two".to_vec()).expect("record"));
        }
        let mut recovery = SnapshotRecovery::open_at(temporary.path()).expect("reopen");
        assert_eq!(
            recovery.take_restored_payload().as_deref(),
            Some(b"two".as_slice())
        );
        recovery
            .checkpoint("loom.test/1", b"two".to_vec())
            .expect("checkpoint");
        let journal = RecoveryJournal::open(temporary.path()).expect("journal");
        assert!(journal.records().expect("records").is_empty());
        assert_eq!(
            journal.recover().expect("recover").checkpoint.as_deref(),
            Some(b"two".as_slice())
        );
    }

    #[test]
    fn application_ids_are_validated() {
        assert!(application_state_directory("org.loom.writer").is_ok());
        assert!(application_state_directory("../escape").is_err());
    }
}
