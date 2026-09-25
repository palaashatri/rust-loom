use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use loom_sheets_core::persistence::WorkbookFile;
use loom_sheets_core::Sheet;

use super::{OpenCompletionQueue, OpenFileCompletion, OpenOperationCoordinator, OpenOperations};
use crate::workbook_io::LoadedWorkbook;

#[test]
fn newer_open_and_document_replacement_make_older_operations_stale() {
    let mut coordinator = OpenOperationCoordinator::default();
    let first = coordinator.begin(7);
    assert!(coordinator.is_current(first));

    let second = coordinator.begin(8);
    assert!(!coordinator.is_current(first));
    assert!(coordinator.is_current(second));

    coordinator.document_replaced();
    assert!(!coordinator.is_current(second));
}

#[test]
fn delayed_open_returns_only_after_the_background_loader_finishes() {
    let mut coordinator = OpenOperationCoordinator::default();
    let operation = coordinator.begin(11);
    let queue = OpenCompletionQueue::new();
    let (release, wait_for_release) = mpsc::channel();
    queue
        .start_load_with(operation, PathBuf::from("delayed.loomtable"), move |_| {
            wait_for_release.recv().expect("release delayed loader");
            Ok(loaded_workbook("Delayed"))
        })
        .expect("start background load");

    assert!(queue.try_receive().is_none());
    release.send(()).expect("release background loader");
    let completion = queue
        .receive_timeout(Duration::from_secs(2))
        .expect("load completion");
    assert_eq!(completion.path, PathBuf::from("delayed.loomtable"));
    assert_eq!(
        completion.result.unwrap().workbook.sheets[0].name,
        "Delayed"
    );
}

#[test]
fn file_completions_keep_every_result_in_fifo_send_order() {
    let mut coordinator = OpenOperationCoordinator::default();
    let older = coordinator.begin(20);
    let newer = coordinator.begin(21);
    let queue = OpenCompletionQueue::new();
    queue
        .sender
        .send(completion(older, "older.loomtable", "Older"))
        .expect("queue older file result");
    queue
        .sender
        .send(completion(newer, "newer.loomtable", "Newer"))
        .expect("queue newer file result");
    let first = queue
        .receive_timeout(Duration::from_secs(2))
        .expect("first file result stays queued");
    let second = queue
        .receive_timeout(Duration::from_secs(2))
        .expect("second file result is not coalesced");

    assert_eq!(first.operation, older);
    assert_eq!(second.operation, newer);
    assert!(!coordinator.is_current(first.operation));
    assert!(coordinator.is_current(second.operation));
}

#[test]
fn loader_keeps_one_running_job_and_only_the_newest_waiting_job() {
    let mut coordinator = OpenOperationCoordinator::default();
    let running = coordinator.begin(22);
    let queue = OpenCompletionQueue::new();
    let (entered, wait_for_entered) = mpsc::channel();
    let (release, wait_for_release) = mpsc::channel();
    let (superseded_ran, observe_superseded) = mpsc::channel();

    queue
        .start_load_with(running, PathBuf::from("running.loomtable"), move |_| {
            entered.send(()).expect("signal running loader");
            wait_for_release.recv().expect("release running loader");
            Ok(loaded_workbook("Running"))
        })
        .expect("start running load");
    wait_for_entered
        .recv_timeout(Duration::from_secs(2))
        .expect("loader started before queuing more work");

    let superseded = coordinator.begin(23);
    queue
        .start_load_with(
            superseded,
            PathBuf::from("superseded.loomtable"),
            move |_| {
                superseded_ran.send(()).expect("report superseded loader");
                Ok(loaded_workbook("Superseded"))
            },
        )
        .expect("queue an open behind the running load");
    let newest = coordinator.begin(24);
    queue
        .start_load_with(newest, PathBuf::from("newest.loomtable"), |_| {
            Ok(loaded_workbook("Newest"))
        })
        .expect("replace the waiting load with the newest request");

    release.send(()).expect("release running loader");
    let first = queue
        .receive_timeout(Duration::from_secs(2))
        .expect("running load completes");
    let second = queue
        .receive_timeout(Duration::from_secs(2))
        .expect("newest waiting load completes");
    assert_eq!(first.operation, running);
    assert_eq!(second.operation, newest);
    assert!(!coordinator.is_current(superseded));
    assert!(observe_superseded.try_recv().is_err());
}

#[test]
fn candidate_stays_attached_until_replacement_is_resumed() {
    let mut operations = OpenOperations::default();
    let operation = operations.begin_operation(30);
    operations.hold_candidate(OpenFileCompletion {
        operation,
        path: PathBuf::from("candidate.loomtable"),
        result: Ok(loaded_workbook("Candidate")),
        startup_options: None,
    });

    let resumed = operations
        .acknowledge_revision(operation, 31)
        .expect("current candidate can be resumed after Save or Discard");
    assert_eq!(resumed.target_revision, 31);
    assert!(operations.pending_candidate.is_some());

    let candidate = operations
        .take_candidate()
        .expect("resume takes the candidate only when ready");
    assert_eq!(candidate.path, PathBuf::from("candidate.loomtable"));
    assert_eq!(
        candidate
            .result
            .expect("candidate load succeeded")
            .workbook
            .sheets[0]
            .name,
        "Candidate"
    );
    assert!(operations.pending_candidate.is_none());
}

#[test]
fn accepted_document_replacement_invalidates_and_discards_held_candidate() {
    let mut operations = OpenOperations::default();
    let operation = operations.begin_operation(40);
    operations.hold_candidate(OpenFileCompletion {
        operation,
        path: PathBuf::from("stale.xlsx"),
        result: Ok(loaded_workbook("Stale")),
        startup_options: None,
    });

    operations.document_replaced();

    assert!(!operations.is_current(operation));
    assert!(operations.pending_candidate.is_none());
    assert!(operations.acknowledge_revision(operation, 41).is_none());
}

fn loaded_workbook(name: &str) -> LoadedWorkbook {
    LoadedWorkbook {
        workbook: WorkbookFile {
            sheets: vec![Sheet::new(name)],
            active: 0,
        },
        warnings: Vec::new(),
    }
}

fn completion(operation: super::OpenOperation, path: &str, sheet: &str) -> OpenFileCompletion {
    OpenFileCompletion {
        operation,
        path: PathBuf::from(path),
        result: Ok(loaded_workbook(sheet)),
        startup_options: None,
    }
}
