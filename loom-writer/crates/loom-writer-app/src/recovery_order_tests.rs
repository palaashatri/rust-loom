//! Ordering, retry, and replacement rules of the deferred recovery writer.
//!
//! Each test runs in an isolated child process, as the other recovery tests do,
//! because the recovery store is process-wide on disk. Every assertion reads the
//! store back through a second handle, the way a restart would.

use super::*;
use crate::recovery_tests::{child_mode, isolated_root, make_test_state, run_child};

const WAIT: std::time::Duration = std::time::Duration::from_secs(5);

/// The editor text of the draft a restart would restore, if any.
fn stored_editor_text() -> Option<String> {
    let directory = loom_production::snapshot::application_state_directory("org.loom.writer")
        .expect("resolve recovery directory");
    let store =
        loom_production::snapshot::SnapshotRecovery::open_at(&directory).expect("open store");
    let payload = store.restored_payload()?;
    crate::recovery_draft::RecoveredDraft::from_payload(payload)
        .map(|draft| draft.document.editor_text())
}

const LATE_OLDER_TEST: &str =
    "recovery_order_tests::a_late_older_serialization_never_replaces_a_newer_recording";

#[test]
fn a_late_older_serialization_never_replaces_a_newer_recording() {
    match child_mode().as_deref() {
        Some("late-older") => {
            set_platform();
            assert!(recovery::initialize_editing_session(None)
                .expect("initialize isolated recovery")
                .draft
                .is_none());
            let app = WriterApp::new().expect("create Writer UI");
            let harness = recovery::DeferredWriteTestHarness::install();
            let mut older = WriterDocument::new("order", "Order");
            older.replace_paragraphs("older snapshot");
            let mut newer = WriterDocument::new("order", "Order");
            newer.replace_paragraphs("newest snapshot");

            // No edit separates the two, so both snapshots share one epoch.
            recovery::start_snapshot_for_test(&app, &older).expect("start older serializer");
            assert_eq!(
                harness
                    .started
                    .recv_timeout(WAIT)
                    .expect("older serializer starts"),
                0
            );
            recovery::start_snapshot_for_test(&app, &newer).expect("start newer serializer");
            assert_eq!(
                harness
                    .started
                    .recv_timeout(WAIT)
                    .expect("newer serializer starts"),
                1
            );

            // The newer serializer finishes first; the older one is released last.
            let newer_done = harness
                .completed
                .recv_timeout(WAIT)
                .expect("newer serializer completes first");
            newer_done();
            harness.release_first();
            let older_done = harness
                .completed
                .recv_timeout(WAIT)
                .expect("older serializer completes last");
            older_done();

            assert_eq!(
                stored_editor_text().as_deref(),
                Some("newest snapshot"),
                "a late completion of an older snapshot must not replace the newer recording"
            );
        }
        Some(mode) => panic!("unknown child mode: {mode}"),
        None => {
            let root = isolated_root();
            std::fs::create_dir_all(&root).expect("create isolated test root");
            let status = run_child(LATE_OLDER_TEST, "late-older", &root, None, None);
            let _ = std::fs::remove_dir_all(&root);
            assert!(
                status.success(),
                "late older serialization child failed: {status}"
            );
        }
    }
}

const FAILED_RECORDING_TEST: &str =
    "recovery_order_tests::a_failed_recording_schedules_its_own_retry";

#[test]
fn a_failed_recording_schedules_its_own_retry() {
    match child_mode().as_deref() {
        Some("failed-recording") => {
            set_platform();
            assert!(recovery::initialize_editing_session(None)
                .expect("initialize isolated recovery")
                .draft
                .is_none());
            let app = WriterApp::new().expect("create Writer UI");
            let harness = recovery::DeferredWriteTestHarness::install();
            let mut document = WriterDocument::new("retry", "Retry");
            document.replace_paragraphs("recovery retry snapshot");

            recovery::DEFERRED.with(|state| {
                state.note_edit();
                assert!(state.fire());
            });
            assert_eq!(
                recovery::record_document_deferred(&app, &document, None)
                    .expect("start deferred recording"),
                recovery::DeferredDispatch::Started
            );
            assert_eq!(
                harness
                    .started
                    .recv_timeout(WAIT)
                    .expect("serializer starts"),
                0
            );
            harness.release_first();
            let failing = harness
                .completed
                .recv_timeout(WAIT)
                .expect("serializer completes");
            recovery::INJECT_FAILURE.with(|failure| failure.set(true));
            failing();
            recovery::INJECT_FAILURE.with(|failure| failure.set(false));
            assert_eq!(
                app.get_status_left(),
                "Recovery failed. Save a copy now; autosave may be out of date."
            );

            // No edit, keystroke, or save happens here. The failure itself must have
            // armed the next attempt, as the debounce timer would.
            let mut retry = recovery::take_scheduled_retry()
                .expect("a failed recording schedules its own retry");
            retry();
            let retried = harness
                .completed
                .recv_timeout(WAIT)
                .expect("the retry starts a serializer");
            retried();

            assert_eq!(
                stored_editor_text().as_deref(),
                Some("recovery retry snapshot"),
                "the retry must put the document into the store"
            );
            assert_eq!(
                app.get_status_left(),
                "",
                "a successful retry clears the recovery warning"
            );
        }
        Some(mode) => panic!("unknown child mode: {mode}"),
        None => {
            let root = isolated_root();
            std::fs::create_dir_all(&root).expect("create isolated test root");
            let status = run_child(FAILED_RECORDING_TEST, "failed-recording", &root, None, None);
            let _ = std::fs::remove_dir_all(&root);
            assert!(status.success(), "failed recording child failed: {status}");
        }
    }
}

const DISCARD_TEST: &str =
    "recovery_order_tests::a_discarded_draft_is_not_recoverable_after_its_replacement";

#[test]
fn a_discarded_draft_is_not_recoverable_after_its_replacement() {
    match child_mode().as_deref() {
        Some("discard") => {
            set_platform();
            assert!(recovery::initialize_editing_session(None)
                .expect("initialize isolated recovery")
                .draft
                .is_none());
            let mut draft = WriterDocument::new("discard", "Discard");
            draft.replace_paragraphs("discarded draft marker");
            let (app, state) = make_test_state(draft.clone(), None);
            recovery::record_document(&draft, None).expect("record the draft before Discard");
            assert_eq!(
                stored_editor_text().as_deref(),
                Some("discarded draft marker"),
                "precondition: the draft is recoverable before Discard"
            );

            let harness = recovery::DeferredWriteTestHarness::install();
            let mut typed = draft.clone();
            typed.replace_paragraphs("discarded draft marker, typed before Discard");
            assert_eq!(
                recovery::record_document_deferred(&app, &typed, None)
                    .expect("start in-flight recording"),
                recovery::DeferredDispatch::Started
            );
            assert_eq!(
                harness
                    .started
                    .recv_timeout(WAIT)
                    .expect("serializer starts"),
                0
            );

            // New, then Discard: the Quick Start sample replaces the document.
            open_quick_start_sample(&app, &state);
            harness.release_first();
            let in_flight = harness
                .completed
                .recv_timeout(WAIT)
                .expect("in-flight serializer completes");
            in_flight();

            let kept = stored_editor_text();
            assert!(
                !kept
                    .as_deref()
                    .unwrap_or_default()
                    .contains("discarded draft marker"),
                "a discarded draft must not stay recoverable after its replacement; store holds {kept:?}"
            );

            // Recording still works for the replacement document.
            let mut next = state.current.borrow().clone();
            next.replace_paragraphs("typed after the discard");
            recovery::record_document(&next, None).expect("record the replacement");
            assert_eq!(
                stored_editor_text().as_deref(),
                Some("typed after the discard")
            );
        }
        Some(mode) => panic!("unknown child mode: {mode}"),
        None => {
            let root = isolated_root();
            std::fs::create_dir_all(&root).expect("create isolated test root");
            let status = run_child(DISCARD_TEST, "discard", &root, None, None);
            let _ = std::fs::remove_dir_all(&root);
            assert!(
                status.success(),
                "discard replacement child failed: {status}"
            );
        }
    }
}
