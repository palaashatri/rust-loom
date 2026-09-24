use super::*;
use loom_package::manifest::{json as pkg_json, Checksum, Manifest, ManifestEntry};
use loom_package::{MimeType, PackageArchive, PackageKind, SchemaVersion};
use loom_sheets_core::persistence::sheet_from_json;
use slint::Model;

mod template_chooser_tests;

mod inspector_tests;

mod local_menu_tests;

#[path = "main_tests/app_startup_tests.rs"]
mod app_startup_tests;
#[path = "main_tests/command_dispatch_tests.rs"]
mod command_dispatch_tests;
#[path = "main_tests/grid_interaction_tests.rs"]
mod grid_interaction_tests;
#[path = "main_tests/layout_tests.rs"]
mod layout_tests;
#[path = "main_tests/sheet_action_tests.rs"]
mod sheet_action_tests;
#[path = "main_tests/workbook_interop_tests.rs"]
mod workbook_interop_tests;
#[path = "main_tests/workbook_state_tests.rs"]
mod workbook_state_tests;
#[path = "main_tests/worker_tests.rs"]
mod worker_tests;

fn cross_sheet_state() -> Rc<GuiState> {
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], []));
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

fn attach_test_worker(app: &SheetsApp, state: &Rc<GuiState>, name: &str) -> PathBuf {
    let recovery_dir = std::env::temp_dir().join(format!(
        "loom-sheets-cell-worker-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&recovery_dir);
    let (worker, startup) =
        workbook_worker::WorkbookWorker::start_at(recovery_dir.clone(), "loom.sheets/1")
            .expect("start test worker");
    assert!(startup.recovery_error.is_none());
    let revision = state.next_worker_revision();
    let (sheets, active) = workbook_sheets(state);
    let model = worker
        .initialize_workbook(revision, active, sheets)
        .expect("initialize test workbook");
    state.install_workbook(model.sheets, model.active_sheet);
    state.mark_saved();
    let result = worker
        .wait_for_result(revision)
        .expect("initial worker result");
    assert!(apply_workbook_worker_result(app, state, result));
    *state.workbook_worker.borrow_mut() = Some(worker);
    recovery_dir
}
