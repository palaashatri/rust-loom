//! Write-failure injection at every recovery step, then restart comparison.
//!
//! Each case fails exactly one write, drops the journal handle (a crash after
//! the failed call), reopens the directory, and checks that recovery returns
//! the last state that was acknowledged, never a half-published one. These run
//! on every platform so Windows commit-record and Unix pointer replacement are
//! both exercised.

use crate::fault_injection::{self, FaultStep};
use crate::{JournalRecord, RecoveryJournal};

fn allow(_: &crate::CheckpointWriteProjection) -> Result<(), crate::ProductionError> {
    Ok(())
}

struct Recovered {
    checkpoint: Option<Vec<u8>>,
    checkpoint_sequence: Option<u64>,
    operations: Vec<Vec<u8>>,
}

fn recover(directory: &std::path::Path) -> Recovered {
    let state = RecoveryJournal::open(directory)
        .expect("reopen after failure")
        .recover()
        .expect("recover after failure");
    Recovered {
        checkpoint: state.checkpoint,
        checkpoint_sequence: state
            .checkpoint_metadata
            .map(|metadata| metadata.last_sequence),
        operations: state
            .operations
            .into_iter()
            .map(|record: JournalRecord| record.payload)
            .collect(),
    }
}

fn scenario(step: FaultStep) {
    let temporary = tempfile::tempdir().expect("tempdir");
    let directory = temporary.path();
    let mut journal = RecoveryJournal::open(directory).expect("journal");
    journal
        .checkpoint_and_compact_with_preflight("loom.test/1", 0, b"base", allow)
        .expect("baseline checkpoint");
    journal
        .append("one", "Edit", b"edit one".to_vec())
        .expect("acknowledged append");

    fault_injection::inject(directory, step, 0, 1);
    let compaction_error = match step {
        FaultStep::JournalAppend => {
            assert!(journal.append("two", "Edit", b"edit two".to_vec()).is_err());
            None
        }
        FaultStep::CheckpointPayload
        | FaultStep::CheckpointMetadata
        | FaultStep::CheckpointPointer => {
            assert!(journal
                .checkpoint_and_compact_with_preflight("loom.test/1", 1, b"state b", allow)
                .is_err());
            None
        }
        FaultStep::JournalRewrite => {
            journal
                .checkpoint_and_compact_with_preflight("loom.test/1", 1, b"state b", allow)
                .expect("the pointer is published before compaction")
                .compaction_error
        }
    };
    assert!(
        fault_injection::hits(directory, step) >= 1,
        "{step:?}: injection point was never reached"
    );
    drop(journal);
    fault_injection::clear(directory);

    let recovered = recover(directory);
    match step {
        FaultStep::JournalAppend
        | FaultStep::CheckpointPayload
        | FaultStep::CheckpointMetadata
        | FaultStep::CheckpointPointer => {
            assert_eq!(recovered.checkpoint.as_deref(), Some(b"base".as_slice()));
            assert_eq!(recovered.checkpoint_sequence, Some(0));
            assert_eq!(
                recovered.operations,
                vec![b"edit one".to_vec()],
                "{step:?}: the acknowledged edit must survive"
            );
        }
        FaultStep::JournalRewrite => {
            assert!(compaction_error.is_some(), "compaction failure is reported");
            assert_eq!(recovered.checkpoint.as_deref(), Some(b"state b".as_slice()));
            assert_eq!(recovered.checkpoint_sequence, Some(1));
            assert!(
                recovered.operations.is_empty(),
                "covered records must not replay on top of the new checkpoint"
            );
        }
    }

    // After the fault clears the same directory accepts and recovers new work.
    let mut reopened = RecoveryJournal::open(directory).expect("reopen for new work");
    reopened
        .append("three", "Edit", b"edit three".to_vec())
        .expect("append after fault clears");
    drop(reopened);
    assert_eq!(
        recover(directory).operations.last().map(Vec::as_slice),
        Some(b"edit three".as_slice())
    );
}

#[test]
fn failed_journal_append_keeps_every_acknowledged_record() {
    scenario(FaultStep::JournalAppend);
}

#[test]
fn failed_checkpoint_payload_write_keeps_the_previous_generation() {
    scenario(FaultStep::CheckpointPayload);
}

#[test]
fn failed_checkpoint_metadata_write_keeps_the_previous_generation() {
    scenario(FaultStep::CheckpointMetadata);
}

#[test]
fn failed_pointer_replacement_keeps_the_previous_generation() {
    scenario(FaultStep::CheckpointPointer);
}

#[test]
fn failed_compaction_keeps_the_published_checkpoint_authoritative() {
    scenario(FaultStep::JournalRewrite);
}

#[test]
fn injection_skips_then_fails_exactly_the_requested_writes() {
    let temporary = tempfile::tempdir().expect("tempdir");
    let directory = temporary.path();
    let mut journal = RecoveryJournal::open(directory).expect("journal");
    journal
        .append("one", "Edit", b"first".to_vec())
        .expect("first append creates the journal");
    fault_injection::inject(directory, FaultStep::JournalAppend, 1, 2);
    assert!(journal.append("two", "Edit", b"ok".to_vec()).is_ok());
    assert!(journal.append("three", "Edit", b"x".to_vec()).is_err());
    assert!(journal.append("four", "Edit", b"x".to_vec()).is_err());
    assert!(journal.append("five", "Edit", b"after".to_vec()).is_ok());
    fault_injection::clear(directory);
    let payloads = journal
        .records()
        .expect("records")
        .into_iter()
        .map(|record| record.payload)
        .collect::<Vec<_>>();
    assert_eq!(
        payloads,
        vec![b"first".to_vec(), b"ok".to_vec(), b"after".to_vec()]
    );
}
