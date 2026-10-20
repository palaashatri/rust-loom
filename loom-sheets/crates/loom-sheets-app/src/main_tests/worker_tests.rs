use super::*;
use slint::platform::{Key, PointerEventButton, WindowEvent};
use slint::SharedString;

fn render_worker_test_window(app: &SheetsApp) {
    let _ = loom_test_support::capture::snapshot_component(app, 1280.0, 800.0, 1.0)
        .expect("render worker test window");
}

fn click_worker_test_point(app: &SheetsApp, point: slint::LogicalPosition) {
    let window = app.window();
    window.dispatch_event(WindowEvent::PointerMoved { position: point });
    window.dispatch_event(WindowEvent::PointerPressed {
        position: point,
        button: PointerEventButton::Left,
    });
    window.dispatch_event(WindowEvent::PointerReleased {
        position: point,
        button: PointerEventButton::Left,
    });
    render_worker_test_window(app);
}

fn click_worker_test_grid_cell(app: &SheetsApp, column_label: &str, row_label: &str) {
    let column =
        i_slint_backend_testing::ElementHandle::find_by_accessible_label(app, column_label)
            .next()
            .expect("column header");
    let row = i_slint_backend_testing::ElementHandle::find_by_accessible_label(app, row_label)
        .next()
        .expect("row header");
    let (column_position, column_size) = (column.absolute_position(), column.size());
    let (row_position, row_size) = (row.absolute_position(), row.size());
    click_worker_test_point(
        app,
        slint::LogicalPosition::new(
            column_position.x + column_size.width / 2.0,
            row_position.y + row_size.height / 2.0,
        ),
    );
}

fn click_worker_test_element(app: &SheetsApp, accessible_label: &str) {
    let element =
        i_slint_backend_testing::ElementHandle::find_by_accessible_label(app, accessible_label)
            .next()
            .expect("accessible element");
    let (position, size) = (element.absolute_position(), element.size());
    click_worker_test_point(
        app,
        slint::LogicalPosition::new(
            position.x + size.width / 2.0,
            position.y + size.height / 2.0,
        ),
    );
}

fn tap_worker_test_key(app: &SheetsApp, text: impl Into<SharedString>) {
    let text = text.into();
    let window = app.window();
    window.dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    window.dispatch_event(WindowEvent::KeyReleased { text });
    render_worker_test_window(app);
}

fn commit_worker_test_formula_with_return(
    app: &SheetsApp,
    column_label: &str,
    cell_address: &str,
    formula: &str,
) {
    click_worker_test_grid_cell(app, column_label, "Row 2");
    assert_eq!(app.get_selected_cell().as_str(), cell_address);
    click_worker_test_element(app, "Formula bar");
    for character in formula.chars() {
        tap_worker_test_key(app, character.to_string());
    }
    assert_eq!(app.get_formula_edit_buffer().as_str(), formula);
    tap_worker_test_key(app, SharedString::from(Key::Return));
}

#[test]
fn sheet_mutations_are_sent_to_the_background_workbook_worker() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    let mut sheet = Sheet::new("background");
    sheet.set_str("A1", "4");
    sheet.set_str("B1", "=A1+3");
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], []));
    let state = Rc::new(GuiState::new(
        sheet.clone(),
        None,
        dialogs,
        FileFilter::new("Workbook", ["loomtable"]).expect("filter"),
        FileFilter::new("CSV", ["csv"]).expect("filter"),
        FileFilter::new("CSV", ["csv"]).expect("filter"),
        FileFilter::new("Excel", ["xlsx"]).expect("filter"),
    ));
    state.install_workbook(vec![sheet], 0);
    state.mark_saved();

    let recovery_dir = std::env::temp_dir().join(format!(
        "loom-sheets-main-worker-test-{}",
        std::process::id()
    ));
    crate::cell_edit_recovery::remove_test_recovery_data(&recovery_dir);
    let (worker, startup) =
        workbook_worker::WorkbookWorker::start_at(recovery_dir.clone(), "loom.sheets/1")
            .expect("start workbook worker");
    assert!(startup.recovery_error.is_none());
    *state.workbook_worker.borrow_mut() = Some(worker);

    apply_sheet(&app, &state);

    let revision = state.worker_revision.get();
    assert_eq!(revision, 1, "sheet mutation must reach the worker mailbox");
    let result = state
        .workbook_worker
        .borrow()
        .as_ref()
        .unwrap()
        .wait_for_result(revision)
        .expect("worker result");
    assert_eq!(result.active_sheet, 0);
    assert_eq!(
        result.values.get(&CellRef::parse("B1").unwrap()),
        Some(&Value::Number(7.0))
    );
    assert!(apply_workbook_worker_result(&app, &state, result));
    assert_eq!(
        evaluate_current(&state).get(&CellRef::parse("B1").unwrap()),
        Some(&Value::Number(7.0))
    );

    drop(state);
    crate::cell_edit_recovery::remove_test_recovery_data(&recovery_dir);
}

#[test]
fn active_tab_changes_are_revisioned_without_replacing_the_workbook() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    let state = cross_sheet_state();
    state.mark_saved();
    let recovery_dir = std::env::temp_dir().join(format!(
        "loom-sheets-main-active-worker-test-{}",
        std::process::id()
    ));
    crate::cell_edit_recovery::remove_test_recovery_data(&recovery_dir);
    let (worker, startup) =
        workbook_worker::WorkbookWorker::start_at(recovery_dir.clone(), "loom.sheets/1")
            .expect("start workbook worker");
    assert!(startup.recovery_error.is_none());
    *state.workbook_worker.borrow_mut() = Some(worker);

    record_workbook_snapshot(&state).expect("send startup workbook");
    state
        .workbook_worker
        .borrow()
        .as_ref()
        .unwrap()
        .wait_for_result(1)
        .expect("initial calculation");
    *state.active_sheet_index.borrow_mut() = 1;
    *state.current.borrow_mut() = state.sheets.borrow()[1].clone();
    apply_sheet_view_change(&app, &state);

    let revision = state.worker_revision.get();
    assert_eq!(revision, 2);
    let result = state
        .workbook_worker
        .borrow()
        .as_ref()
        .unwrap()
        .wait_for_result(revision)
        .expect("active tab calculation");
    assert_eq!(result.active_sheet, 1);
    assert_eq!(
        result.values.get(&CellRef::parse("A1").unwrap()),
        Some(&Value::Number(25.0))
    );

    drop(state);
    crate::cell_edit_recovery::remove_test_recovery_data(&recovery_dir);
}

#[test]
fn worker_result_rejects_old_revisions_and_other_tabs_without_changing_draft() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    let state = cross_sheet_state();
    let recovery_dir = attach_test_worker(&app, &state, "stale-results");
    let worker = state.workbook_worker.borrow();
    let current = worker
        .as_ref()
        .unwrap()
        .wait_for_result(1)
        .expect("current result");
    drop(worker);

    app.set_formula_edit_buffer("=A1*5".into());
    let draft = app.get_formula_edit_buffer();
    state.next_worker_revision();
    let mut old = current.clone();
    old.revision = 1;
    assert!(!apply_workbook_worker_result(&app, &state, old));
    let mut wrong_tab = current;
    wrong_tab.revision = state.worker_revision.get();
    wrong_tab.active_sheet = 1;
    assert!(!apply_workbook_worker_result(&app, &state, wrong_tab));

    assert_eq!(app.get_formula_edit_buffer(), draft);
    assert_eq!(
        state
            .evaluation_cache
            .borrow_mut()
            .cached_or_empty(0)
            .get(&CellRef::parse("B1").unwrap()),
        Some(&Value::Number(20.0))
    );

    drop(app);
    drop(state);
    crate::cell_edit_recovery::remove_test_recovery_data(&recovery_dir);
}

#[test]
fn stale_and_wrong_tab_circular_worker_results_preserve_feedback() {
    set_platform();
    let recovery_dir = std::env::temp_dir().join(format!(
        "loom-sheets-cell-worker-circular-feedback-guards-{}",
        std::process::id()
    ));
    struct RecoveryDataGuard(std::path::PathBuf);
    impl Drop for RecoveryDataGuard {
        fn drop(&mut self) {
            crate::cell_edit_recovery::remove_test_recovery_data(&self.0);
        }
    }
    let _recovery_data = RecoveryDataGuard(recovery_dir.clone());

    let app = SheetsApp::new().expect("create SheetsApp");
    let state = cross_sheet_state();
    state.current.borrow_mut().set_str("A1", "=Report!A1");
    state
        .sheets
        .borrow_mut()
        .get_mut(1)
        .expect("stored sibling tab")
        .set_str("A1", "=Data!A1");
    assert_eq!(
        attach_test_worker(&app, &state, "circular-feedback-guards"),
        recovery_dir
    );

    let circular_result = state
        .workbook_worker
        .borrow()
        .as_ref()
        .unwrap()
        .wait_for_result(state.worker_revision.get())
        .expect("worker result for cross-tab cycle");
    let cell = CellRef::parse("A1").unwrap();
    assert_eq!(circular_result.active_sheet, 0);
    assert_eq!(
        circular_result.values.get(&cell),
        Some(&Value::Error(loom_sheets_core::CalcError::Circular))
    );

    let stale_revision = circular_result.revision;
    let latest_revision = state.next_worker_revision();
    state.pending_cell_commit.set(Some((latest_revision, cell)));
    app.set_formula_feedback("Calculating…".into());
    app.set_status_left("Calculating…".into());
    let mut stale_result = circular_result.clone();
    stale_result.revision = stale_revision;
    assert!(!apply_workbook_worker_result(&app, &state, stale_result));
    assert_eq!(app.get_formula_feedback().as_str(), "Calculating…");
    assert_eq!(app.get_status_left().as_str(), "Calculating…");
    assert_eq!(
        state.pending_cell_commit.get(),
        Some((latest_revision, cell))
    );

    let current_revision = state.worker_revision.get();
    state
        .pending_cell_commit
        .set(Some((current_revision, cell)));
    app.set_formula_feedback("Calculating…".into());
    app.set_status_left("Calculating…".into());
    let mut wrong_tab_result = circular_result;
    wrong_tab_result.revision = current_revision;
    wrong_tab_result.active_sheet = 1;
    assert!(!apply_workbook_worker_result(
        &app,
        &state,
        wrong_tab_result
    ));
    assert_eq!(app.get_formula_feedback().as_str(), "Calculating…");
    assert_eq!(app.get_status_left().as_str(), "Calculating…");
    assert_eq!(
        state.pending_cell_commit.get(),
        Some((current_revision, cell))
    );

    drop(app);
    drop(state);
}

#[test]
fn worker_result_refresh_preserves_scrolled_viewport_and_formula_draft() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    app.set_grid_viewport_width(360.0);
    app.set_grid_viewport_height(280.0);
    let state = cross_sheet_state();
    {
        let mut current = state.current.borrow_mut();
        current.set_str("AZ1000", "tail");
        state.sheets.borrow_mut()[0] = current.clone();
    }
    let recovery_dir = attach_test_worker(&app, &state, "preserve-viewport");
    let worker = state.workbook_worker.borrow();
    let result = worker
        .as_ref()
        .unwrap()
        .wait_for_result(1)
        .expect("current worker result");
    drop(worker);

    // The user has moved away from the selected cell while calculation runs.
    app.set_grid_scroll_x(-180.0);
    app.set_grid_scroll_y(-672.0);
    let scroll_before = (app.get_grid_scroll_x(), app.get_grid_scroll_y());
    assert!(scroll_before.0 < -100.0);
    assert!(scroll_before.1 < -600.0);
    app.set_formula_edit_buffer("=A1*5".into());
    let draft = app.get_formula_edit_buffer();

    assert!(apply_workbook_worker_result(&app, &state, result));

    assert_eq!(
        (app.get_grid_scroll_x(), app.get_grid_scroll_y()),
        scroll_before
    );
    assert_eq!(app.get_formula_edit_buffer(), draft);
    assert!(app.get_view_row_origin() > 1);
    assert!(app.get_view_col_origin() > 1);

    drop(app);
    drop(state);
    crate::cell_edit_recovery::remove_test_recovery_data(&recovery_dir);
}

#[test]
fn newer_workbook_result_clears_superseded_cell_calculating_feedback() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    let state = cross_sheet_state();
    let recovery_dir = attach_test_worker(&app, &state, "superseded-cell-feedback");
    let worker = state.workbook_worker.borrow();
    let mut result = worker
        .as_ref()
        .unwrap()
        .wait_for_result(1)
        .expect("initial worker result");
    drop(worker);

    let pending_revision = state.next_worker_revision();
    let cell = CellRef::parse("A1").unwrap();
    state
        .pending_cell_commit
        .set(Some((pending_revision, cell)));
    let revision = state.next_worker_revision();
    app.set_formula_edit_buffer("=A1*5".into());
    app.set_formula_feedback("Calculating…".into());
    app.set_status_left("Calculating…".into());
    result.revision = revision;

    assert!(apply_workbook_worker_result(&app, &state, result));

    assert_eq!(state.pending_cell_commit.get(), None);
    assert_eq!(app.get_formula_feedback().as_str(), "");
    assert_eq!(app.get_status_left().as_str(), "Ready");
    assert_eq!(app.get_formula_edit_buffer().as_str(), "=A1*5");

    drop(app);
    drop(state);
    crate::cell_edit_recovery::remove_test_recovery_data(&recovery_dir);
}

#[test]
fn formula_bar_commit_sends_a_cell_delta_and_keeps_old_values_until_result() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    let state = cross_sheet_state();
    let recovery_dir = attach_test_worker(&app, &state, "cell-delta");
    let menu_service = std::sync::Arc::new(NativeMenuBar::new());
    register_cell_edit_action(&app, &state, &menu_service);
    app.set_selected_cell("A1".into());

    app.invoke_commit_selected_cell("12".into());

    let immediate_status = app.get_status_left();
    let immediate_feedback = app.get_formula_feedback();
    let committed_raw = state
        .current
        .borrow()
        .raw(CellRef::parse("A1").unwrap())
        .map(str::to_owned);
    let previous_b1 = state
        .evaluation_cache
        .borrow_mut()
        .cached_or_empty(0)
        .get(&CellRef::parse("B1").unwrap())
        .cloned();
    let revision = state.worker_revision.get();

    assert_eq!(revision, 2);
    assert_eq!(committed_raw.as_deref(), Some("12"));
    assert_eq!(previous_b1, Some(Value::Number(20.0)));

    app.set_formula_edit_buffer("=A1*3".into());
    let result = state
        .workbook_worker
        .borrow()
        .as_ref()
        .unwrap()
        .wait_for_result(revision)
        .expect("calculated cell result");

    assert_eq!(
        result.update_kind,
        workbook_worker::WorkerUpdateKind::CellDelta
    );
    assert_eq!(result.cell_updates, 1);
    assert_eq!(
        result.values.get(&CellRef::parse("B1").unwrap()),
        Some(&Value::Number(24.0))
    );
    assert_eq!(immediate_status.as_str(), "Calculating…");
    assert_eq!(immediate_feedback.as_str(), "Calculating…");
    assert_eq!(
        state
            .evaluation_cache
            .borrow_mut()
            .cached_or_empty(0)
            .get(&CellRef::parse("B1").unwrap()),
        Some(&Value::Number(20.0))
    );
    assert!(apply_workbook_worker_result(&app, &state, result));
    assert_eq!(app.get_formula_edit_buffer().as_str(), "=A1*3");
    assert_eq!(app.get_formula_feedback().as_str(), "Cell A1 updated");
    assert_eq!(app.get_status_left().as_str(), "Ready");

    drop(app);
    drop(state);
    crate::cell_edit_recovery::remove_test_recovery_data(&recovery_dir);
}

#[test]
fn circular_reference_worker_result_keeps_ref_values_and_shows_polite_feedback() {
    set_platform();
    let recovery_dir = std::env::temp_dir().join(format!(
        "loom-sheets-cell-worker-circular-reference-feedback-{}",
        std::process::id()
    ));
    struct RecoveryDataGuard(std::path::PathBuf);
    impl Drop for RecoveryDataGuard {
        fn drop(&mut self) {
            crate::cell_edit_recovery::remove_test_recovery_data(&self.0);
        }
    }
    let _recovery_data = RecoveryDataGuard(recovery_dir.clone());
    let s = keyboard_flow_tests::launched(&[]);
    assert_eq!(
        attach_test_worker(&s.app, &s.state, "circular-reference-feedback"),
        recovery_dir
    );

    commit_worker_test_formula_with_return(&s.app, "Column F", "F2", "=G2");
    assert_eq!(s.app.get_selected_cell().as_str(), "F3");
    let first_revision = s.state.worker_revision.get();
    let first_result = s
        .state
        .workbook_worker
        .borrow()
        .as_ref()
        .unwrap()
        .wait_for_result(first_revision)
        .expect("worker result for F2");
    assert!(apply_workbook_worker_result(&s.app, &s.state, first_result));

    commit_worker_test_formula_with_return(&s.app, "Column G", "G2", "=F2");
    assert_eq!(s.app.get_selected_cell().as_str(), "G3");
    // The user can begin the next cell while the worker result is pending. The
    // result must preserve this in-progress formula draft.
    for character in "draft".chars() {
        tap_worker_test_key(&s.app, character.to_string());
    }
    assert!(s.app.get_is_editing());
    assert_eq!(s.app.get_formula_edit_buffer().as_str(), "draft");

    let second_revision = s.state.worker_revision.get();
    let second_result = s
        .state
        .workbook_worker
        .borrow()
        .as_ref()
        .unwrap()
        .wait_for_result(second_revision)
        .expect("worker result for G2");
    assert_eq!(second_result.revision, second_revision);
    for cell in [CellRef::parse("F2").unwrap(), CellRef::parse("G2").unwrap()] {
        let value = second_result
            .values
            .get(&cell)
            .expect("cycle result exists");
        assert_eq!(value, &Value::Error(loom_sheets_core::CalcError::Circular));
        assert!(matches!(value, Value::Error(error) if error.code() == "REF!"));
        assert_eq!(value.display(), "#REF!");
    }
    assert!(apply_workbook_worker_result(
        &s.app,
        &s.state,
        second_result
    ));

    let expected_message = "Circular reference in G2: #REF!";
    assert_eq!(s.app.get_status_left().as_str(), expected_message);
    assert_eq!(s.app.get_formula_edit_buffer().as_str(), "draft");
    assert!(s.app.get_is_editing());
    let messages: Vec<_> =
        i_slint_backend_testing::ElementHandle::find_by_accessible_label(&s.app, expected_message)
            .collect();
    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0].accessible_live_region(),
        Some(i_slint_backend_testing::AccessibleLiveness::Polite)
    );
}
