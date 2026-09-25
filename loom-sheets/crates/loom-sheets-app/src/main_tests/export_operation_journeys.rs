use super::*;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

static NEXT_EXPORT_TEST_ID: AtomicU64 = AtomicU64::new(0);

struct ScratchDirectory(PathBuf);

impl ScratchDirectory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "loom-sheets-export-journey-{}-{}",
            std::process::id(),
            NEXT_EXPORT_TEST_ID.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).expect("create export test directory");
        Self(path)
    }
}

impl Drop for ScratchDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn export_test_app(save_result: Option<PathBuf>) -> (SheetsApp, Rc<GuiState>) {
    loom_test_support::capture::set_platform();
    let app = SheetsApp::new().expect("create export test app");
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], [save_result]));
    let mut sheet = Sheet::new("Data");
    sheet.set_str("A1", "1");
    let state = Rc::new(GuiState::new(
        sheet,
        None,
        dialogs,
        FileFilter::new("Workbook", ["loomtable"]).expect("workbook filter"),
        FileFilter::new("CSV", ["csv"]).expect("import filter"),
        FileFilter::new("CSV", ["csv"]).expect("CSV filter"),
        FileFilter::new("Excel", ["xlsx"]).expect("XLSX filter"),
    ));
    (app, state)
}

fn attach_worker(app: &SheetsApp, state: &GuiState, recovery: &Path) {
    let (save_tx, _save_rx) = mpsc::channel();
    let export_tx = state.export_operations.borrow().sender();
    let (worker, startup) = workbook_worker::WorkbookWorker::start_at_with_file_completions(
        recovery.to_path_buf(),
        "loom.sheets/1",
        save_tx,
        export_tx,
    )
    .expect("start export journey worker");
    assert!(startup.recovery_error.is_none());
    let revision = state.next_worker_revision();
    let (sheets, active) = workbook_sheets(state);
    let model = worker
        .initialize_workbook(revision, active, sheets)
        .expect("initialize export journey workbook");
    state.last_queued_worker_revision.set(revision);
    state.install_workbook(model.sheets, model.active_sheet);
    project_current(app, state);
    state.mark_saved();
    let result = worker
        .wait_for_result(revision)
        .expect("initial worker result");
    assert!(apply_workbook_worker_result(app, state, result));
    *state.workbook_worker.borrow_mut() = Some(worker);
}

fn hold_worker(state: &GuiState) -> mpsc::Sender<()> {
    let (entered, release) = state
        .workbook_worker
        .borrow()
        .as_ref()
        .expect("workbook worker")
        .enqueue_test_gate();
    entered
        .recv_timeout(Duration::from_secs(5))
        .expect("worker entered deterministic gate");
    release
}

fn wait_for_export(app: &SheetsApp, state: &GuiState) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if process_file_completions_in_timer_order(app, state) > 0 {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "export completion did not arrive"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn queue_export_completion(
    state: &GuiState,
    completion_sequence: u64,
    operation_id: u64,
    path: PathBuf,
    result: Result<crate::export_operations::ExportOutputSummary, String>,
) {
    let document_generation = state.open_operations.borrow().document_generation();
    let target_revision = state.worker_revision.get();
    state.export_operations.borrow_mut().accept(operation_id);
    state
        .export_operations
        .borrow()
        .sender()
        .send(crate::export_operations::ExportCompletion {
            completion_sequence,
            operation: crate::export_operations::ExportOperation {
                operation_id,
                document_generation,
                target_revision,
                format: crate::export_operations::ExportFormat::Csv,
                path,
                source_name: "Current workbook".into(),
            },
            result,
            worker_duration: Duration::ZERO,
        })
        .expect("queue export completion");
}

fn queue_save_completion(
    state: &GuiState,
    completion_sequence: u64,
    path: PathBuf,
    write_result: Result<(), String>,
) {
    let operation = state
        .save_operations
        .borrow_mut()
        .begin_operation(
            state.open_operations.borrow().document_generation(),
            state.worker_revision.get(),
            None,
        )
        .expect("begin test Save operation");
    state
        .save_operations
        .borrow()
        .sender()
        .send(crate::save_operations::SaveCompletion {
            completion_sequence,
            operation,
            path,
            write_result,
            checkpoint_result: Some(Ok(())),
            baseline: None,
        })
        .expect("queue Save completion");
}

fn process_file_completions_in_timer_order(app: &SheetsApp, state: &GuiState) -> usize {
    let menu_service = std::sync::Arc::new(loom_desktop::NativeMenuBar::new());
    crate::file_operation_completions::process_completions(app, state, &menu_service)
}

#[test]
fn export_then_save_completion_keeps_the_later_save_outcome_visible() {
    let (app, state) = export_test_app(None);
    let export_path = PathBuf::from("export-failed.csv");
    let save_path = PathBuf::from("saved-after-export.loomtable");
    queue_export_completion(
        &state,
        1,
        1,
        export_path.clone(),
        Err("CSV write failed: test failure".into()),
    );
    queue_save_completion(&state, 2, save_path.clone(), Ok(()));

    process_file_completions_in_timer_order(&app, &state);

    let status = app.get_status_left().to_string();
    let export_failure = status
        .find("CSV export failed")
        .expect("earlier export failure remains visible");
    let later_save_success = status
        .find(&format!("Saved {}", save_path.display()))
        .expect("later Save completion is reported");
    assert!(export_failure < later_save_success);
    assert!(status.contains(&export_path.display().to_string()));
}

#[test]
fn same_tick_success_does_not_hide_a_file_operation_failure() {
    let (app, state) = export_test_app(None);
    let save_path = PathBuf::from("save-failed.loomtable");
    let export_path = PathBuf::from("export-succeeded.csv");
    queue_save_completion(
        &state,
        1,
        save_path.clone(),
        Err("permission denied".into()),
    );
    queue_export_completion(
        &state,
        2,
        1,
        export_path.clone(),
        Ok(crate::export_operations::ExportOutputSummary::Csv {
            sheet_name: "Data".into(),
        }),
    );

    process_file_completions_in_timer_order(&app, &state);

    let status = app.get_status_left().to_string();
    assert!(status.contains("Save failed: permission denied"));
    assert!(status.contains("Exported CSV"));
    assert!(status.contains(&save_path.display().to_string()));
    assert!(status.contains(&export_path.display().to_string()));
}

#[test]
fn same_tick_file_outcomes_keep_every_success_and_failure_in_sequence() {
    let (app, state) = export_test_app(None);
    let first_export = PathBuf::from("first.csv");
    let save_failure = PathBuf::from("middle.loomtable");
    let second_export = PathBuf::from("second.csv");
    let export_success = crate::export_operations::ExportOutputSummary::Csv {
        sheet_name: "Data".into(),
    };
    queue_export_completion(&state, 1, 1, first_export.clone(), Ok(export_success));
    queue_save_completion(&state, 2, save_failure.clone(), Err("disk full".into()));
    queue_export_completion(
        &state,
        3,
        2,
        second_export.clone(),
        Ok(crate::export_operations::ExportOutputSummary::Csv {
            sheet_name: "Data".into(),
        }),
    );

    assert_eq!(process_file_completions_in_timer_order(&app, &state), 3);

    let status = app.get_status_left().to_string();
    assert_eq!(
        status,
        format!(
            "Exported CSV from Current workbook · Data (formulas preserved) to {} · Save failed: disk full at {} · Exported CSV from Current workbook · Data (formulas preserved) to {}",
            first_export.display(),
            save_failure.display(),
            second_export.display()
        ),
        "two successes and the failure must remain in worker completion order"
    );
}

#[test]
fn csv_export_callback_returns_before_worker_writes_and_then_reports_completion() {
    let output = std::env::temp_dir().join(format!(
        "loom-sheets-export-callback-{}-{}.csv",
        std::process::id(),
        NEXT_EXPORT_TEST_ID.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_file(&output);
    let (app, state) = export_test_app(Some(output.clone()));
    let recovery = ScratchDirectory::new();
    attach_worker(&app, &state, &recovery.0);
    let release = hold_worker(&state);
    crate::wire_export_callbacks(&app, &state);
    app.set_formula_edit_buffer("draft only".into());

    app.invoke_export_csv();

    assert!(app.get_status_left().contains("Exporting CSV"));
    assert_eq!(app.get_formula_edit_buffer(), "draft only");
    assert!(
        !output.exists(),
        "UI callback wrote the export before the worker ran"
    );
    assert_eq!(
        state
            .workbook_worker
            .borrow()
            .as_ref()
            .expect("workbook worker")
            .pending_exports_for_test(),
        1
    );
    release.send(()).expect("release export worker");
    wait_for_export(&app, &state);
    assert!(app.get_status_left().contains("Exported CSV"));
    assert!(app
        .get_status_left()
        .contains(&output.display().to_string()));
    let exported = std::fs::read_to_string(output).expect("read independently written CSV");
    assert_eq!(
        loom_sheets_core::from_csv("export", &exported).raw(CellRef::parse("A1").unwrap()),
        Some("1")
    );
}

#[test]
fn xlsx_export_callback_returns_before_worker_writes_and_exports_every_sheet() {
    let output = std::env::temp_dir().join(format!(
        "loom-sheets-export-callback-{}-{}.xlsx",
        std::process::id(),
        NEXT_EXPORT_TEST_ID.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_file(&output);
    let (app, state) = export_test_app(Some(output.clone()));
    let mut report = Sheet::new("Report");
    report.set_str("A1", "=Data!A1+1");
    state.sheets.borrow_mut().push(report);
    state
        .sheet_histories
        .borrow_mut()
        .push((Vec::new(), Vec::new()));
    let recovery = ScratchDirectory::new();
    attach_worker(&app, &state, &recovery.0);
    let release = hold_worker(&state);
    crate::wire_export_callbacks(&app, &state);

    app.invoke_export_xlsx();

    assert!(app.get_status_left().contains("Exporting Excel"));
    assert!(
        !output.exists(),
        "UI callback wrote the XLSX before the worker ran"
    );
    assert_eq!(
        state
            .workbook_worker
            .borrow()
            .as_ref()
            .expect("workbook worker")
            .pending_exports_for_test(),
        1
    );
    release.send(()).expect("release XLSX export worker");
    wait_for_export(&app, &state);
    assert!(app.get_status_left().contains("Exported Excel"));
    assert!(app
        .get_status_left()
        .contains(&output.display().to_string()));
    let bytes = std::fs::read(output).expect("read independently written XLSX");
    let exported = loom_sheets_core::extract_xlsx_sheets(&bytes).expect("inspect XLSX export");
    assert_eq!(exported.len(), 2);
    assert_eq!(exported[0].name, "Data");
    assert_eq!(exported[1].name, "Report");
    assert_eq!(
        exported[1].raw(CellRef::parse("A1").unwrap()),
        Some("=Data!A1+1")
    );
}

#[test]
fn pending_export_keeps_the_root_window_open_after_close_is_requested() {
    let output = std::env::temp_dir().join(format!(
        "loom-sheets-close-pending-export-{}-{}.csv",
        std::process::id(),
        NEXT_EXPORT_TEST_ID.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_file(&output);
    let (app, state) = export_test_app(Some(output));
    let recovery = ScratchDirectory::new();
    attach_worker(&app, &state, &recovery.0);
    let release = hold_worker(&state);
    crate::wire_export_callbacks(&app, &state);
    crate::close_operations::wire_window_close_handler(&app, &state);
    app.window().show().expect("show root window");

    app.invoke_export_csv();
    assert_eq!(
        state
            .workbook_worker
            .borrow()
            .as_ref()
            .expect("workbook worker")
            .pending_exports_for_test(),
        1
    );

    app.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    let stayed_visible = app.window().is_visible();

    release.send(()).expect("release pending export worker");
    let menu_service = std::sync::Arc::new(loom_desktop::NativeMenuBar::new());
    let deadline = Instant::now() + Duration::from_secs(5);
    while state.export_operations.borrow().has_pending() {
        crate::close_operations::process_worker_tick(&app, &state, &menu_service);
        assert!(Instant::now() < deadline, "close export did not finish");
        std::thread::sleep(Duration::from_millis(5));
    }
    crate::close_operations::process_worker_tick(&app, &state, &menu_service);

    assert!(
        stayed_visible,
        "root window must stay shown until the accepted export outcome is delivered"
    );
    assert!(
        !app.window().is_visible(),
        "settled successful close must hide the window"
    );
    assert!(app.get_status_left().contains("Exported CSV"));
}

#[test]
fn second_export_is_rejected_before_a_second_picker_or_worker_job() {
    let recovery = ScratchDirectory::new();
    let output = recovery.0.join("first.csv");
    let (app, state) = export_test_app(Some(output.clone()));
    attach_worker(&app, &state, &recovery.0);
    let release = hold_worker(&state);
    crate::wire_export_callbacks(&app, &state);

    app.invoke_export_csv();
    app.invoke_export_xlsx();
    assert!(app.get_status_left().contains("already in progress"));
    crate::export_operations::queue_export(
        &app,
        &state,
        crate::export_operations::ExportFormat::Xlsx,
        recovery.0.join("second.xlsx"),
    );
    assert!(app.get_status_left().contains("already in progress"));
    assert!(state.export_operations.borrow().has_pending());
    assert_eq!(
        state
            .workbook_worker
            .borrow()
            .as_ref()
            .expect("workbook worker")
            .pending_exports_for_test(),
        1
    );

    release.send(()).expect("release pending export worker");
    wait_for_export(&app, &state);
    let output_text = std::fs::read_to_string(&output).expect("read first export");
    assert!(output_text.contains("1"));
}

#[test]
fn picker_cancel_leaves_the_worker_mailbox_without_an_export() {
    let (app, state) = export_test_app(None);
    let recovery = ScratchDirectory::new();
    attach_worker(&app, &state, &recovery.0);
    let release = hold_worker(&state);
    crate::wire_export_callbacks(&app, &state);

    app.invoke_export_csv();

    assert_eq!(app.get_status_left(), "Export cancelled");
    assert_eq!(
        state
            .workbook_worker
            .borrow()
            .as_ref()
            .expect("workbook worker")
            .pending_exports_for_test(),
        0
    );
    release.send(()).expect("release idle worker");
    std::thread::sleep(Duration::from_millis(30));
    assert_eq!(process_file_completions_in_timer_order(&app, &state), 0);
}

#[test]
fn stale_export_completions_report_outcomes_without_changing_document_state() {
    let (app, state) = export_test_app(None);
    let recovery = ScratchDirectory::new();
    attach_worker(&app, &state, &recovery.0);
    let old_generation = state.open_operations.borrow().document_generation();
    state.open_operations.borrow_mut().document_replaced();
    let mut replacement = Sheet::new("Current");
    replacement.set_str("A1", "current workbook");
    state.install_workbook(vec![replacement], 0);
    *state.save_path.borrow_mut() = Some(PathBuf::from("current.loomtable"));
    state.mark_saved();
    state.mark_content_dirty();
    let before = document_snapshot(&state);

    let success_path = PathBuf::from("prior.csv");
    state
        .export_operations
        .borrow()
        .sender()
        .send(crate::export_operations::ExportCompletion {
            completion_sequence: 1,
            operation: crate::export_operations::ExportOperation {
                operation_id: 1,
                document_generation: old_generation,
                target_revision: 1,
                format: crate::export_operations::ExportFormat::Csv,
                path: success_path.clone(),
                source_name: "Prior workbook".into(),
            },
            result: Ok(crate::export_operations::ExportOutputSummary::Csv {
                sheet_name: "Old data".into(),
            }),
            worker_duration: Duration::from_millis(3),
        })
        .expect("send stale export success");
    assert_eq!(process_file_completions_in_timer_order(&app, &state), 1);
    assert!(app
        .get_status_left()
        .contains("previous workbook generation"));
    assert!(app.get_status_left().contains("Prior workbook"));
    assert!(app
        .get_status_left()
        .contains(&success_path.display().to_string()));
    assert!(app.get_status_left().contains("formulas preserved"));
    assert_document_unchanged(&state, &before);

    let failure_path = PathBuf::from("prior-failed.xlsx");
    state
        .export_operations
        .borrow()
        .sender()
        .send(crate::export_operations::ExportCompletion {
            completion_sequence: 2,
            operation: crate::export_operations::ExportOperation {
                operation_id: 2,
                document_generation: old_generation,
                target_revision: 1,
                format: crate::export_operations::ExportFormat::Xlsx,
                path: failure_path.clone(),
                source_name: "Prior workbook".into(),
            },
            result: Err("XLSX generation failed: invalid workbook".into()),
            worker_duration: Duration::from_millis(2),
        })
        .expect("send stale export failure");
    assert_eq!(process_file_completions_in_timer_order(&app, &state), 1);
    assert!(app.get_status_left().contains("Excel export failed"));
    assert!(app
        .get_status_left()
        .contains(&failure_path.display().to_string()));
    assert!(app.get_status_left().contains("invalid workbook"));
    assert_document_unchanged(&state, &before);
}

fn assert_document_unchanged(
    state: &GuiState,
    expected: &(Option<String>, Option<PathBuf>, Option<String>, bool),
) {
    assert_eq!(document_snapshot(state), *expected);
}

fn document_snapshot(state: &GuiState) -> (Option<String>, Option<PathBuf>, Option<String>, bool) {
    (
        state
            .current
            .borrow()
            .raw(CellRef::parse("A1").unwrap())
            .map(str::to_owned),
        state.save_path.borrow().clone(),
        state
            .last_saved
            .borrow()
            .as_ref()
            .map(|(sheets, active)| workbook_to_json(sheets, *active)),
        state.is_dirty(),
    )
}
