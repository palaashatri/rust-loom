use super::*;
use crate::startup_session::{self, StartupChoices, StartupSession};

fn draft_directory(name: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!(
        "loom-sheets-startup-draft-{name}-{}",
        std::process::id()
    ));
    crate::cell_edit_recovery::remove_test_recovery_data(&directory);
    std::fs::create_dir_all(&directory).expect("create draft test directory");
    directory
}

/// The store a window leaves when it is force-killed after one unsaved edit:
/// the workbook as it was saved to `source` (if it has a file), then one
/// journaled cell change.
fn write_force_killed_draft(directory: &Path, source: Option<&Path>, edited_value: &str) {
    let (worker, startup) =
        crate::workbook_worker::WorkbookWorker::start_at(directory.to_path_buf(), "loom.sheets/1")
            .expect("start the first session's worker");
    assert!(startup.restored_payload.is_none());
    let mut sheet = Sheet::new("Data");
    sheet.set_str("A1", "10");
    worker
        .submit_replacement(1, 0, vec![sheet], source.map(Path::to_path_buf))
        .expect("queue the saved workbook");
    worker.wait_for_result(1).expect("saved workbook result");
    worker
        .submit_cell(crate::workbook_worker::CellUpdate {
            revision: 2,
            active_sheet: 0,
            sheet: 0,
            cell: CellRef::parse("A1").unwrap(),
            raw: Some(edited_value.to_string()),
        })
        .expect("queue the unsaved edit");
    worker.wait_for_result(2).expect("edit result");
    // Dropping the worker is the force-kill: the journal keeps the edit.
}

fn stored_cell_a1(directory: &Path) -> Option<String> {
    let payload = recovered_worker_payload(directory)?;
    let workbook = crate::workbook_io::restore_workbook_from_snapshot(&payload)?;
    workbook.sheets[0]
        .raw(CellRef::parse("A1").unwrap())
        .map(str::to_string)
}

fn cell_a1(state: &GuiState) -> Option<String> {
    state
        .current
        .borrow()
        .raw(CellRef::parse("A1").unwrap())
        .map(str::to_string)
}

/// What a relaunched window starts with.
struct Relaunched {
    state: GuiState,
    recovered_unsaved: bool,
    opens_file: bool,
}

/// Relaunches the window the way main.rs does: the store is restored, the first
/// model comes from `startup_session::begin`, and the window is installed.
fn relaunch(app: &SheetsApp, directory: &Path, requested: Option<&Path>) -> Relaunched {
    // The window's own completion channels, so saves and exports report back to it.
    let state = state_with_identity(None, "Sheet 1");
    let (worker, startup) = crate::workbook_worker::WorkbookWorker::start_at_with_file_completions(
        directory.to_path_buf(),
        "loom.sheets/1",
        state.save_operations.borrow().sender(),
        state.export_operations.borrow().sender(),
    )
    .expect("restart the worker");
    assert!(
        startup.recovery_error.is_none(),
        "recovery restart failed: {:?}",
        startup.recovery_error
    );
    let revision = state.next_worker_revision();
    let session = startup_session::begin(
        &worker,
        &startup,
        revision,
        &StartupChoices {
            example: false,
            objects: false,
            chart: false,
            requested: requested.map(Path::to_path_buf),
        },
    )
    .expect("begin the restored session");
    state.last_queued_worker_revision.set(revision);
    let generation = state.open_operations.borrow().document_generation();
    crate::worker_failure::mark_full_resync_accepted(&state, generation, revision);
    let facts = Relaunched {
        state,
        recovered_unsaved: session.recovered_unsaved,
        opens_file: session.opens_file,
    };
    let StartupSession {
        model,
        recovered_unsaved,
        save_path,
        ..
    } = session;
    facts
        .state
        .install_workbook(model.sheets, model.active_sheet);
    *facts.state.save_path.borrow_mut() = save_path;
    facts.state.set_startup_baseline(recovered_unsaved);
    let result = worker.wait_for_result(revision).expect("initial result");
    assert!(crate::apply_workbook_worker_result(
        app,
        &facts.state,
        result
    ));
    *facts.state.workbook_worker.borrow_mut() = Some(worker);
    facts
}
/// Drives the background load of the startup file until it has either been
/// applied or has asked the user for a replacement decision.
fn finish_startup_open(app: &SheetsApp, state: &GuiState) {
    let operation =
        crate::open_operations::test_support::current_operation(&state.open_operations.borrow())
            .expect("startup Open operation is active");
    let menu_service = std::sync::Arc::new(NativeMenuBar::new());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while state.open_operations.borrow().is_current(operation) && !app.get_save_changes_open() {
        crate::open_operations::process_completions(app, state, &menu_service);
        assert!(
            std::time::Instant::now() < deadline,
            "startup Open should finish or ask for a decision"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

fn saved_file_with_a1(path: &Path, value: &str) {
    let mut sheet = Sheet::new("Data");
    sheet.set_str("A1", value);
    save_workbook(path, &[sheet], 0).expect("save a workbook file");
}

#[test]
fn opening_a_saved_file_after_a_force_kill_never_silently_replaces_the_unsaved_draft() {
    set_platform();
    let directory = draft_directory("silent-replacement");
    // The user's file lives beside the store, never inside it.
    let files = draft_directory("silent-replacement-files");
    let saved = files.join("saved.loomtable");
    saved_file_with_a1(&saved, "7");
    write_force_killed_draft(&directory, None, "42");

    let app = SheetsApp::new().expect("create SheetsApp");
    let Relaunched {
        state,
        recovered_unsaved,
        opens_file,
    } = relaunch(&app, &directory, Some(&saved));
    assert!(recovered_unsaved, "the edit is still in the store");
    assert!(opens_file, "another file waits for the user's decision");
    assert_eq!(cell_a1(&state).as_deref(), Some("42"));

    crate::open_operations::start_startup_open(
        &app,
        &state,
        saved.clone(),
        crate::open_operations::StartupOpenOptions::new(false, false, false),
    );
    finish_startup_open(&app, &state);
    // Dropping the worker flushes its queued replacement before the store is read.
    drop(state.workbook_worker.borrow_mut().take());

    let draft_survives = stored_cell_a1(&directory).as_deref() == Some("42");
    assert!(
        app.get_save_changes_open() || draft_survives,
        "the unsaved edit was replaced without a Save, Discard, or Cancel decision"
    );
    crate::cell_edit_recovery::remove_test_recovery_data(&directory);
    let _ = std::fs::remove_dir_all(&files);
}

#[test]
fn a_draft_saved_to_a_file_restores_under_that_file_without_reloading_it() {
    set_platform();
    let directory = draft_directory("same-file");
    let files = draft_directory("same-file-files");
    let saved = files.join("budget.loomtable");
    saved_file_with_a1(&saved, "7");
    write_force_killed_draft(&directory, Some(&saved), "42");

    let app = SheetsApp::new().expect("create SheetsApp");
    let Relaunched {
        state,
        recovered_unsaved,
        opens_file,
    } = relaunch(&app, &directory, Some(&saved));
    assert!(
        !opens_file,
        "the draft already holds this file's newer contents"
    );
    assert_eq!(cell_a1(&state).as_deref(), Some("42"));
    assert_eq!(state.save_path.borrow().as_deref(), Some(saved.as_path()));
    assert!(recovered_unsaved, "the store still holds the unsaved draft");
    assert!(state.is_dirty(), "a restored draft is unsaved");
    let title = crate::workbook_window_title(
        state.save_path.borrow().as_deref(),
        None,
        "Data",
        state.is_dirty(),
    );
    assert_eq!(title, "budget.loomtable *");
    assert_eq!(
        startup_session::restored_status("budget.loomtable"),
        "Restored unsaved changes to budget.loomtable"
    );

    drop(state.workbook_worker.borrow_mut().take());
    let _ = std::fs::remove_dir_all(&files);
    crate::cell_edit_recovery::remove_test_recovery_data(&directory);
}

#[test]
fn saving_a_restored_draft_writes_the_file_it_was_saved_to_without_a_picker() {
    set_platform();
    let directory = draft_directory("save-original");
    let files = draft_directory("save-original-files");
    let saved = files.join("ledger.loomtable");
    saved_file_with_a1(&saved, "7");
    write_force_killed_draft(&directory, Some(&saved), "42");

    let app = SheetsApp::new().expect("create SheetsApp");
    let Relaunched { state, .. } = relaunch(&app, &directory, Some(&saved));
    let menu_service = std::sync::Arc::new(NativeMenuBar::new());
    crate::save_current_sheet(&app, &state, false).expect("save the restored draft");
    wait_for_save_test_completion(&app, &state, &menu_service);

    assert!(!state.is_dirty(), "the saved draft is clean");
    let bytes = std::fs::read(&saved).expect("read the saved file");
    let on_disk = crate::workbook_io::restore_workbook_from_snapshot(&bytes)
        .expect("the saved file is a workbook package");
    assert_eq!(
        on_disk.sheets[0].raw(CellRef::parse("A1").unwrap()),
        Some("42"),
        "Save writes the restored draft back to the original file"
    );
    drop(state.workbook_worker.borrow_mut().take());
    let _ = std::fs::remove_dir_all(&files);
    crate::cell_edit_recovery::remove_test_recovery_data(&directory);
}

#[test]
fn a_draft_for_another_file_waits_behind_the_prompt_and_cancel_keeps_it() {
    set_platform();
    let directory = draft_directory("other-file-cancel");
    let files = draft_directory("other-file-cancel-files");
    let draft_file = files.join("draft.loomtable");
    let requested = files.join("requested.loomtable");
    saved_file_with_a1(&draft_file, "7");
    saved_file_with_a1(&requested, "9");
    write_force_killed_draft(&directory, Some(&draft_file), "42");

    let app = SheetsApp::new().expect("create SheetsApp");
    let Relaunched {
        state, opens_file, ..
    } = relaunch(&app, &directory, Some(&requested));
    assert!(opens_file, "the requested file waits for a decision");
    assert_eq!(
        state.save_path.borrow().as_deref(),
        Some(draft_file.as_path())
    );
    crate::open_operations::start_startup_open(
        &app,
        &state,
        requested.clone(),
        crate::open_operations::StartupOpenOptions::new(false, false, false),
    );
    finish_startup_open(&app, &state);
    assert!(app.get_save_changes_open(), "the unsaved draft asks first");

    crate::open_operations::cancel_pending_replacement(&app, &state);
    assert_eq!(
        cell_a1(&state).as_deref(),
        Some("42"),
        "Cancel keeps the draft"
    );
    assert_eq!(
        state.save_path.borrow().as_deref(),
        Some(draft_file.as_path())
    );
    drop(state.workbook_worker.borrow_mut().take());
    assert_eq!(stored_cell_a1(&directory).as_deref(), Some("42"));
    crate::cell_edit_recovery::remove_test_recovery_data(&directory);
    let _ = std::fs::remove_dir_all(&files);
}

#[test]
fn an_untitled_draft_waits_behind_the_prompt_and_discard_opens_the_file() {
    set_platform();
    let directory = draft_directory("untitled-discard");
    let files = draft_directory("untitled-discard-files");
    let requested = files.join("requested.loomtable");
    saved_file_with_a1(&requested, "9");
    write_force_killed_draft(&directory, None, "42");

    let app = SheetsApp::new().expect("create SheetsApp");
    let Relaunched {
        state, opens_file, ..
    } = relaunch(&app, &directory, Some(&requested));
    assert!(opens_file);
    assert!(state.save_path.borrow().is_none(), "the draft is untitled");
    crate::open_operations::start_startup_open(
        &app,
        &state,
        requested.clone(),
        crate::open_operations::StartupOpenOptions::new(false, false, false),
    );
    finish_startup_open(&app, &state);
    assert!(app.get_save_changes_open(), "the untitled draft asks first");
    assert_eq!(cell_a1(&state).as_deref(), Some("42"));

    crate::open_operations::discard_changes_and_resume(
        &app,
        &state,
        &std::sync::Arc::new(NativeMenuBar::new()),
    );
    assert_eq!(
        cell_a1(&state).as_deref(),
        Some("9"),
        "Discard opens the file"
    );
    assert_eq!(
        state.save_path.borrow().as_deref(),
        Some(requested.as_path())
    );
    drop(state.workbook_worker.borrow_mut().take());
    crate::cell_edit_recovery::remove_test_recovery_data(&directory);
    let _ = std::fs::remove_dir_all(&files);
}

#[test]
fn a_requested_file_is_recognised_through_another_spelling_of_its_path() {
    let files = draft_directory("spelling");
    let saved = files.join("same.loomtable");
    saved_file_with_a1(&saved, "1");
    let other_spelling = files
        .join("..")
        .join(files.file_name().expect("directory name"))
        .join("same.loomtable");
    assert!(startup_session::same_document(&saved, &other_spelling));
    assert!(!startup_session::same_document(
        &saved,
        &files.join("different.loomtable")
    ));
    let _ = std::fs::remove_dir_all(&files);
}

#[test]
fn the_requested_file_loads_only_when_no_draft_already_holds_it() {
    let saved = Path::new("/work/saved.loomtable");
    let other = Path::new("/work/other.loomtable");
    // No file on the command line: nothing loads.
    assert!(!startup_session::opens_requested_file(false, None, None));
    assert!(!startup_session::opens_requested_file(
        true,
        Some(saved),
        None
    ));
    // A fresh start loads the file at once.
    assert!(startup_session::opens_requested_file(
        false,
        None,
        Some(saved)
    ));
    // A draft for the same file wins; the file is not reloaded over it.
    assert!(!startup_session::opens_requested_file(
        true,
        Some(saved),
        Some(saved)
    ));
    // A draft for another file, or an untitled one, waits behind the prompt.
    assert!(startup_session::opens_requested_file(
        true,
        Some(other),
        Some(saved)
    ));
    assert!(startup_session::opens_requested_file(
        true,
        None,
        Some(saved)
    ));
}
