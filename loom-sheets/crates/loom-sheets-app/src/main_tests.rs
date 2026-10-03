use super::*;
use loom_package::manifest::{json as pkg_json, Checksum, Manifest, ManifestEntry};
use loom_package::{MimeType, PackageArchive, PackageKind, SchemaVersion};
use loom_sheets_core::persistence::sheet_from_json;
use slint::Model;

mod template_chooser_tests;

mod inspector_tests;

#[path = "main_tests/icon_accessibility_tests.rs"]
mod icon_accessibility_tests;

#[path = "main_tests/chart_accessibility_tests.rs"]
mod chart_accessibility_tests;
#[path = "main_tests/object_accessibility_tests.rs"]
mod object_accessibility_tests;

mod local_menu_tests;

#[path = "main_tests/app_startup_tests.rs"]
mod app_startup_tests;
#[path = "main_tests/clipboard_tests.rs"]
mod clipboard_tests;
#[path = "main_tests/close_operation_journeys.rs"]
mod close_operation_journeys;
#[path = "main_tests/command_dispatch_tests.rs"]
mod command_dispatch_tests;
#[path = "main_tests/export_operation_journeys.rs"]
mod export_operation_journeys;
#[path = "main_tests/grid_gesture_tests.rs"]
mod grid_gesture_tests;
#[path = "main_tests/grid_interaction_tests.rs"]
mod grid_interaction_tests;
#[path = "main_tests/grid_pointer_tests.rs"]
mod grid_pointer_tests;
#[path = "main_tests/grid_tab_enter_tests.rs"]
mod grid_tab_enter_tests;
#[path = "main_tests/layout_tests.rs"]
mod layout_tests;
#[path = "main_tests/open_operation_journeys.rs"]
mod open_operation_journeys;
#[path = "main_tests/save_operations_journeys.rs"]
mod save_operations_journeys;
#[path = "main_tests/sheet_action_tests.rs"]
mod sheet_action_tests;
#[path = "main_tests/workbook_interop_tests.rs"]
mod workbook_interop_tests;
#[path = "main_tests/workbook_state_tests.rs"]
mod workbook_state_tests;
#[path = "main_tests/worker_tests.rs"]
mod worker_tests;

fn cross_sheet_state() -> Rc<GuiState> {
    cross_sheet_state_with_dialogs(Rc::new(loom_desktop::ScriptedFileDialogs::new([], [])))
}

fn cross_sheet_state_with_dialogs(dialogs: Rc<dyn FileDialogService>) -> Rc<GuiState> {
    let mut data = Sheet::new("Data");
    data.set_str("A1", "10");
    data.set_str("B1", "=A1*2");
    let mut report = Sheet::new("Report");
    report.set_str("A1", "=Data!B1+5");
    report.set_str("A2", "=SUM(Data!A1:A1)");
    let state = Rc::new(GuiState::new(
        data,
        None,
        dialogs,
        FileFilter::new("Workbook", ["loomtable"]).expect("filter"),
        FileFilter::new("CSV", ["csv"]).expect("filter"),
        FileFilter::new("CSV", ["csv"]).expect("filter"),
        FileFilter::new("Excel", ["xlsx"]).expect("filter"),
    ));
    state.sheets.borrow_mut().push(report);
    state
        .sheet_histories
        .borrow_mut()
        .push((Vec::new(), Vec::new()));
    state
}

/// A single-sheet state whose only document identity is `sheet_name` plus an
/// optional save path. Used to prove the close prompt, replacement prompt, and
/// window title all name the same document.
fn state_with_identity(save_path: Option<PathBuf>, sheet_name: &str) -> GuiState {
    GuiState::new(
        Sheet::new(sheet_name),
        save_path,
        Rc::new(loom_desktop::ScriptedFileDialogs::new([], [])),
        FileFilter::new("Workbook", ["loomtable"]).expect("filter"),
        FileFilter::new("CSV", ["csv"]).expect("filter"),
        FileFilter::new("CSV", ["csv"]).expect("filter"),
        FileFilter::new("Excel", ["xlsx"]).expect("filter"),
    )
}

fn attach_test_worker(app: &SheetsApp, state: &Rc<GuiState>, name: &str) -> PathBuf {
    let recovery_dir = std::env::temp_dir().join(format!(
        "loom-sheets-cell-worker-{name}-{}",
        std::process::id()
    ));
    crate::cell_edit_recovery::remove_test_recovery_data(&recovery_dir);
    std::fs::create_dir_all(&recovery_dir).expect("create test recovery directory");
    let save_completions = state.save_operations.borrow().sender();
    let (worker, startup) = workbook_worker::WorkbookWorker::start_at_with_completions(
        recovery_dir.clone(),
        "loom.sheets/1",
        save_completions,
    )
    .expect("start test worker");
    assert!(
        startup.recovery_error.is_none(),
        "recovery startup failed: {:?}",
        startup.recovery_error
    );
    let revision = state.next_worker_revision();
    let (sheets, active) = workbook_sheets(state);
    let model = worker
        .initialize_workbook(revision, active, sheets)
        .expect("initialize test workbook");
    state.last_queued_worker_revision.set(revision);
    state.install_workbook(model.sheets, model.active_sheet);
    state.mark_saved();
    let result = worker
        .wait_for_result(revision)
        .expect("initial worker result");
    assert!(apply_workbook_worker_result(app, state, result));
    *state.workbook_worker.borrow_mut() = Some(worker);
    recovery_dir
}

fn recovered_worker_payload(directory: &std::path::Path) -> Option<Vec<u8>> {
    let (worker, startup) =
        workbook_worker::WorkbookWorker::start_at(directory.to_path_buf(), "loom.sheets/1")
            .expect("restart workbook worker for recovery inspection");
    assert!(
        startup.recovery_error.is_none(),
        "recovery restart failed: {:?}",
        startup.recovery_error
    );
    drop(worker);
    startup.restored_payload
}

fn wait_for_save_test_completion(
    app: &SheetsApp,
    state: &GuiState,
    menu_service: &std::sync::Arc<NativeMenuBar>,
) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while state.save_operations.borrow().is_active() {
        crate::save_operations::process_completions(app, state, menu_service);
        assert!(
            std::time::Instant::now() < deadline,
            "Save completion did not arrive"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    crate::save_operations::process_completions(app, state, menu_service);
}
