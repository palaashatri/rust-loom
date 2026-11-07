//! Verification tests for Wave 3: Tables and charts as first-class slide objects.
#![cfg(test)]

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use loom_desktop::{FileDialogService, FileFilter, ScriptedFileDialogs};
use loom_objects::chart::ChartType;
use loom_present_core::{
    default_slide_chart, default_slide_table, export_pdf, export_pptx, import_pptx,
    inspect_pptx_unsupported, ElementType, PresentationDocument, PresentationSession, SlideElement,
};
use slint::{ComponentHandle, SharedString};

use crate::{
    dispatch_command, empty_session, master_palette, refresh, save_current_deck,
    table_actions::{self, get_active_table_cell, set_active_table_cell},
    DragState, GuiState, PaletteAction, PresentApp,
};

fn make_test_fixture() -> (PresentApp, Rc<GuiState>) {
    loom_test_support::capture::set_platform();
    let app = PresentApp::new().expect("create app");
    let mut initial = empty_session();
    if let Some(slide) = initial.document.active_slide_mut() {
        slide.elements.clear();
    }
    let state = Rc::new(GuiState {
        last_saved: RefCell::new(initial.document.clone()),
        last_saved_transitions: RefCell::default(),
        session: RefCell::new(initial),
        pending_replacement: std::cell::Cell::new(None),
        selected_element: std::cell::Cell::new(0),
        inspector_available: std::cell::Cell::new(true),
        save_path: RefCell::new(None),
        dialogs: Rc::new(ScriptedFileDialogs::new([], [])),
        deck_filter: FileFilter::new("Deck", ["loomdeck"]).expect("filter"),
        pdf_filter: FileFilter::new("PDF", ["pdf"]).expect("filter"),
        menu_service: None,
        drag_state: RefCell::new(DragState::default()),
    });
    refresh(&app, &state);
    table_actions::wire(&app, &state);
    crate::wire_app_callbacks(&app, &state);
    (app, state)
}

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new(label: &str) -> Self {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock after Unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "loom-present-{label}-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).expect("create temporary test directory");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct RecoveryScratch {
    directory: TestDirectory,
}

struct CheckpointPayloadFaultGuard {
    directory: PathBuf,
}

impl CheckpointPayloadFaultGuard {
    fn before_checkpoint_payload(directory: &Path) -> Self {
        loom_production::fault_injection::inject(
            directory,
            loom_production::fault_injection::FaultStep::CheckpointPayload,
            0,
            1,
        );
        Self {
            directory: directory.to_path_buf(),
        }
    }

    fn hits(&self) -> u32 {
        loom_production::fault_injection::hits(
            &self.directory,
            loom_production::fault_injection::FaultStep::CheckpointPayload,
        )
    }
}

impl Drop for CheckpointPayloadFaultGuard {
    fn drop(&mut self) {
        loom_production::fault_injection::clear(&self.directory);
    }
}

impl RecoveryScratch {
    fn new_checkpoint(payload: &[u8]) -> Self {
        let directory = TestDirectory::new("import-recovery");
        let mut recovery = loom_production::snapshot::SnapshotRecovery::open_at(directory.path())
            .expect("open isolated test recovery");
        recovery
            .checkpoint("loom.present/1", payload.to_vec())
            .expect("seed isolated recovery checkpoint");
        drop(recovery);
        let recovery = loom_production::snapshot::SnapshotRecovery::open_at(directory.path())
            .expect("reopen isolated test recovery");
        crate::PRESENT_RECOVERY.with(|slot| *slot.borrow_mut() = Some(recovery));
        Self { directory }
    }

    fn directory(&self) -> &Path {
        self.directory.path()
    }

    fn latest_payload(&self) -> Option<Vec<u8>> {
        loom_production::snapshot::SnapshotRecovery::open_at(self.directory())
            .expect("reopen isolated test recovery payload")
            .restored_payload()
            .map(ToOwned::to_owned)
    }
}

impl Drop for RecoveryScratch {
    fn drop(&mut self) {
        crate::PRESENT_RECOVERY.with(|slot| drop(slot.borrow_mut().take()));
        table_actions::PENDING_PPTX_IMPORT.with(|slot| drop(slot.borrow_mut().take()));
        crate::recovery_deferred::invalidate();
    }
}

fn lossy_smartart_pptx(directory: &TestDirectory) -> (PathBuf, PresentationSession) {
    let source = PresentationSession::new(PresentationDocument::new("doc-1", "Unsupported Test"));
    let pptx_bytes = export_pptx(&source).expect("export test PPTX");
    let archive = loom_package::zip::PackageArchive::from_bytes(&pptx_bytes).expect("open zip");
    let mut archive_with_smartart = loom_package::zip::PackageArchive::new();
    for path in archive.paths() {
        if let Some(bytes) = archive.get(path) {
            let _ = archive_with_smartart.add(path, bytes.to_vec());
        }
    }
    let _ = archive_with_smartart.add("ppt/diagrams/data1.xml", b"<xml></xml>".to_vec());
    let bytes = archive_with_smartart
        .to_bytes()
        .expect("write test package");
    let warnings = inspect_pptx_unsupported(&bytes).expect("inspect test package");
    assert!(warnings.iter().any(|warning| warning.contains("SmartArt")));
    let candidate = PresentationSession::new(import_pptx(&bytes).expect("import test package"));
    let path = directory.path().join("unsupported.pptx");
    std::fs::write(&path, bytes).expect("write test PPTX");
    (path, candidate)
}

fn state_with_dialogs(
    app: &PresentApp,
    initial: PresentationSession,
    dialogs: Rc<dyn FileDialogService>,
) -> Rc<GuiState> {
    let state = Rc::new(GuiState {
        last_saved: RefCell::new(initial.document.clone()),
        last_saved_transitions: RefCell::new(initial.transitions.clone()),
        session: RefCell::new(initial),
        pending_replacement: std::cell::Cell::new(None),
        selected_element: std::cell::Cell::new(0),
        inspector_available: std::cell::Cell::new(true),
        save_path: RefCell::new(None),
        dialogs,
        deck_filter: FileFilter::new("Deck", ["loomdeck"]).expect("filter"),
        pdf_filter: FileFilter::new("PDF", ["pdf"]).expect("filter"),
        menu_service: None,
        drag_state: RefCell::new(DragState::default()),
    });
    refresh(app, &state);
    table_actions::wire(app, &state);
    state
}

#[test]
fn test_insert_table_and_chart_via_callbacks() {
    let (app, state) = make_test_fixture();

    // 1. Insert Table
    app.invoke_add_table();
    {
        let session = state.session.borrow();
        let slide = session.document.active_slide().expect("slide");
        assert_eq!(slide.elements.len(), 1);
        let table_elem = &slide.elements[0];
        assert_eq!(table_elem.element_type, ElementType::Table);
        let table = table_elem.table().expect("table object");
        assert_eq!(table.row_count(), 3);
        assert_eq!(table.col_count(), 3);
        assert_eq!(table.style_id, "Medium 1");
        assert!(table.header_row);
        assert!(table.banded_rows);
    }
    assert!(app.get_inspector_is_table());

    // 2. Insert Chart takes the chart the user copied.
    loom_clipboard::set_mock_clipboard_enabled(true);
    loom_clipboard::write_clipboard(&loom_clipboard::ClipboardData::from_chart(
        &default_slide_chart(),
    ))
    .expect("write the chart to the mock clipboard");
    app.invoke_add_chart();
    loom_clipboard::set_mock_clipboard_enabled(false);
    {
        let session = state.session.borrow();
        let slide = session.document.active_slide().expect("slide");
        assert_eq!(slide.elements.len(), 2);
        let chart_elem = &slide.elements[1];
        assert_eq!(chart_elem.element_type, ElementType::Chart);
        let chart = chart_elem.chart().expect("chart object");
        assert_eq!(chart.title.as_deref(), Some("Sales Overview"));
        assert_eq!(chart.chart_type, ChartType::Bar);
        assert_eq!(chart.categories.len(), 4);
        assert_eq!(chart.series.len(), 2);
    }
}

#[test]
fn test_insert_table_and_chart_via_dispatch_and_palette() {
    let (app, state) = make_test_fixture();

    assert!(dispatch_command(&app, "slide.insert_table"));
    assert_eq!(
        state
            .session
            .borrow()
            .document
            .active_slide()
            .unwrap()
            .elements
            .len(),
        1
    );

    let _guard = MOCK_CLIPBOARD.lock().unwrap_or_else(|e| e.into_inner());
    loom_clipboard::set_mock_clipboard_enabled(true);
    loom_clipboard::write_clipboard(&loom_clipboard::ClipboardData::from_chart(
        &default_slide_chart(),
    ))
    .expect("write the chart to the mock clipboard");
    assert!(dispatch_command(&app, "slide.insert_chart"));
    loom_clipboard::set_mock_clipboard_enabled(false);
    assert_eq!(
        state
            .session
            .borrow()
            .document
            .active_slide()
            .unwrap()
            .elements
            .len(),
        2
    );

    // Verify commands exist in master palette
    let palette = master_palette(&app);
    assert!(palette
        .iter()
        .any(|cmd| matches!(cmd.action, PaletteAction::AddTable)));
    assert!(palette
        .iter()
        .any(|cmd| matches!(cmd.action, PaletteAction::AddChart)));
}

#[test]
fn test_table_keyboard_navigation_and_row_append() {
    let (app, state) = make_test_fixture();
    app.invoke_add_table();

    let elem_id = state
        .session
        .borrow()
        .document
        .active_slide()
        .unwrap()
        .elements[0]
        .id
        .clone();

    // Initial active cell is (0, 0)
    assert_eq!(get_active_table_cell(&elem_id), (0, 0));

    // Tab -> (0, 1)
    let tab_key: SharedString = slint::platform::Key::Tab.into();
    let handled = table_actions::handle_table_key(&app, &state, tab_key.as_str(), false);
    assert!(handled);
    assert_eq!(get_active_table_cell(&elem_id), (0, 1));

    // Right Arrow -> (0, 2)
    let right_arrow: SharedString = slint::platform::Key::RightArrow.into();
    let handled = table_actions::handle_table_key(&app, &state, right_arrow.as_str(), false);
    assert!(handled);
    assert_eq!(get_active_table_cell(&elem_id), (0, 2));

    // Down Arrow -> (1, 2)
    let down_arrow: SharedString = slint::platform::Key::DownArrow.into();
    let handled = table_actions::handle_table_key(&app, &state, down_arrow.as_str(), false);
    assert!(handled);
    assert_eq!(get_active_table_cell(&elem_id), (1, 2));

    // Tab -> (2, 0)
    table_actions::handle_table_key(&app, &state, tab_key.as_str(), false);
    assert_eq!(get_active_table_cell(&elem_id), (2, 0));

    // Tab -> (2, 1)
    table_actions::handle_table_key(&app, &state, tab_key.as_str(), false);
    assert_eq!(get_active_table_cell(&elem_id), (2, 1));

    // Tab -> (2, 2) (last cell of initial 3x3 table)
    table_actions::handle_table_key(&app, &state, tab_key.as_str(), false);
    assert_eq!(get_active_table_cell(&elem_id), (2, 2));

    let initial_height = state
        .session
        .borrow()
        .document
        .active_slide()
        .unwrap()
        .elements[0]
        .height;

    // Tab from last cell -> appends new row and moves to (3, 0)
    table_actions::handle_table_key(&app, &state, tab_key.as_str(), false);
    assert_eq!(get_active_table_cell(&elem_id), (3, 0));

    {
        let session = state.session.borrow();
        let table_elem = &session.document.active_slide().unwrap().elements[0];
        let table = table_elem.table().unwrap();
        assert_eq!(table.row_count(), 4);
        assert!(table_elem.height > initial_height);
    }

    // Shift+Tab -> moves back to (2, 2)
    table_actions::handle_table_key(&app, &state, tab_key.as_str(), true);
    assert_eq!(get_active_table_cell(&elem_id), (2, 2));
}

#[test]
fn test_table_cell_text_editing_and_a11y_label() {
    let (app, state) = make_test_fixture();
    app.invoke_add_table();

    let elem_id = state
        .session
        .borrow()
        .document
        .active_slide()
        .unwrap()
        .elements[0]
        .id
        .clone();

    set_active_table_cell(&elem_id, 0, 0);
    app.invoke_update_element_content("Quarterly Revenue".into());

    {
        let session = state.session.borrow();
        let table_elem = &session.document.active_slide().unwrap().elements[0];
        let table = table_elem.table().unwrap();
        assert_eq!(
            table.cell(0, 0).map(|c| c.content.text()),
            Some("Quarterly Revenue".to_string())
        );

        let a11y = table_actions::format_table_a11y(table_elem);
        assert_eq!(
            a11y,
            Some("Table 3 by 3, cell A1: Quarterly Revenue".to_string())
        );
    }
}

#[test]
fn table_typing_undoes_per_cell_session_without_crossing_other_actions() {
    let (app, state) = make_test_fixture();
    app.invoke_add_table();
    let id = state
        .session
        .borrow()
        .document
        .active_slide()
        .unwrap()
        .elements[0]
        .id
        .clone();
    set_active_table_cell(&id, 0, 0);
    for text in [
        "h",
        "he",
        "hel",
        "hell",
        "hello",
        "hello ",
        "hello w",
        "hello wo",
        "hello wor",
        "hello worl",
        "hello world",
    ] {
        app.invoke_update_element_content(text.into());
    }
    let text = || {
        state
            .session
            .borrow()
            .document
            .active_slide()
            .unwrap()
            .elements[0]
            .table()
            .unwrap()
            .cell(0, 0)
            .unwrap()
            .content
            .text()
    };

    app.invoke_undo();
    assert_eq!(text(), "Header 1");
    app.invoke_redo();
    assert_eq!(text(), "hello world");

    set_active_table_cell(&id, 0, 1);
    app.invoke_update_element_content("x".into());
    app.invoke_update_element_content("xy".into());
    app.invoke_undo();
    assert_eq!(text(), "hello world");
    assert_eq!(
        state
            .session
            .borrow()
            .document
            .active_slide()
            .unwrap()
            .elements[0]
            .table()
            .unwrap()
            .cell(0, 1)
            .unwrap()
            .content
            .text(),
        "Header 2"
    );
    app.invoke_undo();
    assert_eq!(text(), "Header 1");
    assert_eq!(
        state
            .session
            .borrow()
            .document
            .active_slide()
            .unwrap()
            .elements[0]
            .table()
            .unwrap()
            .cell(0, 1)
            .unwrap()
            .content
            .text(),
        "Header 2"
    );
}

#[test]
fn table_cell_edit_sessions_flush_on_focus_loss_and_other_actions() {
    let (app, state) = make_test_fixture();
    app.invoke_add_table();
    let id = state
        .session
        .borrow()
        .document
        .active_slide()
        .unwrap()
        .elements[0]
        .id
        .clone();
    set_active_table_cell(&id, 0, 0);
    let text = || {
        state
            .session
            .borrow()
            .document
            .active_slide()
            .unwrap()
            .elements[0]
            .table()
            .unwrap()
            .cell(0, 0)
            .unwrap()
            .content
            .text()
    };

    app.invoke_update_element_content("Cell edit".into());
    app.invoke_update_element_content("Cell edit complete".into());
    app.invoke_end_table_cell_edit();
    app.invoke_update_element_content("After focus".into());
    app.invoke_update_element_content("After focus loss".into());

    app.invoke_undo();
    assert_eq!(text(), "Cell edit complete");
    app.invoke_undo();
    assert_eq!(text(), "Header 1");
    app.invoke_redo();
    app.invoke_redo();
    assert_eq!(text(), "After focus loss");

    app.invoke_add_text();
    assert_eq!(
        state
            .session
            .borrow()
            .document
            .active_slide()
            .unwrap()
            .elements
            .len(),
        2
    );
    app.invoke_undo();
    assert_eq!(
        state
            .session
            .borrow()
            .document
            .active_slide()
            .unwrap()
            .elements
            .len(),
        1
    );
    assert_eq!(text(), "After focus loss");
    app.invoke_undo();
    assert_eq!(text(), "Cell edit complete");
}

#[test]
fn saving_a_table_cell_edit_preserves_text_and_flushes_the_undo_session() {
    let (app, state) = make_test_fixture();
    let directory = TestDirectory::new("table-cell-undo-save");
    let path = directory.0.join("saved.loomdeck");
    app.invoke_add_table();
    let id = state
        .session
        .borrow()
        .document
        .active_slide()
        .unwrap()
        .elements[0]
        .id
        .clone();
    set_active_table_cell(&id, 0, 0);
    app.invoke_update_element_content("Saved".into());
    app.invoke_update_element_content("Saved value".into());
    *state.save_path.borrow_mut() = Some(path.clone());

    assert!(save_current_deck(&app, &state, false).expect("save table deck"));
    let loaded = loom_present_core::load_presentation_session(
        &std::fs::read(&path).expect("read saved table deck"),
    )
    .expect("load saved table deck");
    assert_eq!(
        loaded.document.active_slide().unwrap().elements[0]
            .table()
            .unwrap()
            .cell(0, 0)
            .unwrap()
            .content
            .text(),
        "Saved value"
    );

    app.invoke_update_element_content("Saved value after save".into());
    app.invoke_update_element_content("Saved value after save!".into());
    app.invoke_undo();
    assert_eq!(
        state
            .session
            .borrow()
            .document
            .active_slide()
            .unwrap()
            .elements[0]
            .table()
            .unwrap()
            .cell(0, 0)
            .unwrap()
            .content
            .text(),
        "Saved value"
    );
}

#[test]
fn test_table_inspector_style_and_options_and_undo() {
    let (app, state) = make_test_fixture();
    app.invoke_add_table();

    // Change style
    app.invoke_set_table_style("table-style-dark-2".into());
    {
        let session = state.session.borrow();
        let table = session.document.active_slide().unwrap().elements[0]
            .table()
            .unwrap();
        assert_eq!(table.style_id, "table-style-dark-2");
    }

    // Toggle options
    app.invoke_toggle_table_option("first-column".into());
    {
        let session = state.session.borrow();
        let table = session.document.active_slide().unwrap().elements[0]
            .table()
            .unwrap();
        assert!(table.first_column);
    }

    // Undo restores first-column to false
    app.invoke_undo();
    {
        let session = state.session.borrow();
        let table = session.document.active_slide().unwrap().elements[0]
            .table()
            .unwrap();
        assert!(!table.first_column);
    }
}

#[test]
fn test_table_row_and_column_mutations() {
    let (app, state) = make_test_fixture();
    app.invoke_add_table();

    // Insert row below -> 4 rows
    app.invoke_table_action("insert-row-below".into());
    {
        let session = state.session.borrow();
        let table = session.document.active_slide().unwrap().elements[0]
            .table()
            .unwrap();
        assert_eq!(table.row_count(), 4);
    }

    // Insert col right -> 4 cols
    app.invoke_table_action("insert-col-right".into());
    {
        let session = state.session.borrow();
        let table = session.document.active_slide().unwrap().elements[0]
            .table()
            .unwrap();
        assert_eq!(table.col_count(), 4);
    }

    // Delete row -> 3 rows
    app.invoke_table_action("delete-row".into());
    {
        let session = state.session.borrow();
        let table = session.document.active_slide().unwrap().elements[0]
            .table()
            .unwrap();
        assert_eq!(table.row_count(), 3);
    }

    // Delete col -> 3 cols
    app.invoke_table_action("delete-col".into());
    {
        let session = state.session.borrow();
        let table = session.document.active_slide().unwrap().elements[0]
            .table()
            .unwrap();
        assert_eq!(table.col_count(), 3);
    }
}

#[test]
fn test_pptx_export_import_roundtrip_with_tables_and_charts() {
    let mut doc = PresentationDocument::new("doc-1", "Slide Table & Chart Demo");
    let slide = doc.active_slide_mut().unwrap();
    slide.elements.clear();

    let table = default_slide_table(3, 3);
    let table_elem = SlideElement::new_table("elem-tbl", 50.0, 50.0, 400.0, 200.0, table);
    slide.elements.push(table_elem);

    let chart = default_slide_chart();
    let chart_elem = SlideElement::new_chart("elem-chart", 500.0, 50.0, 450.0, 300.0, chart);
    slide.elements.push(chart_elem);

    let session = PresentationSession::new(doc);
    let pptx_bytes = export_pptx(&session).expect("export pptx");
    assert!(!pptx_bytes.is_empty());

    let imported = import_pptx(&pptx_bytes).expect("import pptx");
    let imp_slide = imported.active_slide().expect("imported active slide");
    assert_eq!(imp_slide.elements.len(), 3);

    let imp_table_elem = imp_slide
        .elements
        .iter()
        .find(|e| e.element_type == ElementType::Table)
        .expect("table element");
    let imp_table = imp_table_elem.table().expect("table data");
    assert_eq!(imp_table.row_count(), 3);
    assert_eq!(imp_table.col_count(), 3);
    assert_eq!(
        imp_table.cell(0, 0).map(|c| c.content.text()),
        Some("Header 1".to_string())
    );

    let imp_chart_elem = imp_slide
        .elements
        .iter()
        .find(|e| e.element_type == ElementType::Chart)
        .expect("chart element");
    let imp_chart = imp_chart_elem.chart().expect("chart data");
    assert_eq!(imp_chart.title.as_deref(), Some("Sales Overview"));
    assert_eq!(imp_chart.chart_type, ChartType::Bar);
    assert_eq!(imp_chart.categories.len(), 4);
    assert_eq!(imp_chart.series.len(), 2);
}

#[test]
fn test_pptx_import_warning_dialog_flow() {
    let scratch = TestDirectory::new("pptx-warning");
    let (test_file, candidate) = lossy_smartart_pptx(&scratch);
    let prior = PresentationSession::new(PresentationDocument::new(
        "prior-checkpoint",
        "Prior saved checkpoint",
    ));
    let prior_package =
        loom_present_core::save_presentation_session(&prior).expect("package prior checkpoint");
    let prior_recovery =
        crate::recovery_draft::wrap(Some(Path::new("prior.loomdeck")), &prior_package);
    let recovery = RecoveryScratch::new_checkpoint(&prior_recovery);
    let (app, _state) = make_test_fixture();
    let dialogs = ScriptedFileDialogs::new([Some(test_file.clone()), Some(test_file.clone())], []);
    let mut empty_s = empty_session();
    if let Some(s) = empty_s.document.active_slide_mut() {
        s.elements.clear();
    }
    let original_session = empty_s.clone();
    let state_with_dialog = state_with_dialogs(&app, empty_s, Rc::new(dialogs));

    // The picker opens a candidate, but leaves the visible session and recovery
    // checkpoint untouched until the user chooses.
    crate::open_deck_from_picker(&app, &state_with_dialog);
    assert!(app.get_pptx_import_warning_open());
    assert!(app.get_pptx_import_warning_message().contains("SmartArt"));
    assert!(crate::deck_guard::content_matches(
        &state_with_dialog.session.borrow().document,
        &original_session.document
    ));
    assert_eq!(recovery.latest_payload(), Some(prior_recovery.clone()));

    // Cancel preserves current session
    app.invoke_pptx_import_cancel();
    assert!(!app.get_pptx_import_warning_open());
    assert!(crate::deck_guard::content_matches(
        &state_with_dialog.session.borrow().document,
        &original_session.document
    ));
    assert!(table_actions::PENDING_PPTX_IMPORT.with(|slot| slot.borrow().is_none()));
    assert_eq!(recovery.latest_payload(), Some(prior_recovery));

    // Reopen through the real picker path so Continue exercises the candidate
    // and warning callbacks rather than a hand-built pending-import value.
    crate::open_deck_from_picker(&app, &state_with_dialog);
    assert!(app.get_pptx_import_warning_open());

    app.invoke_pptx_import_continue();

    assert!(!app.get_pptx_import_warning_open());
    assert!(crate::deck_guard::content_matches(
        &state_with_dialog.session.borrow().document,
        &candidate.document
    ));
    assert!(app
        .get_status_left()
        .contains("Import completed with losses: SmartArt"));
    let recovered = recovery
        .latest_payload()
        .expect("accepted import has a recovery payload before deferred flush");
    let (draft, recovered_session) =
        crate::recovery_draft::read(&recovered).expect("read accepted import recovery");
    assert_eq!(draft.source.as_deref(), Some(test_file.as_path()));
    assert!(crate::deck_guard::content_matches(
        &recovered_session.document,
        &candidate.document
    ));
    assert_eq!(recovered_session.transitions, candidate.transitions);
}

#[test]
fn picker_continue_keeps_the_warning_open_without_a_recovery_service() {
    let scratch = TestDirectory::new("pptx-warning-no-recovery");
    let (test_file, _candidate) = lossy_smartart_pptx(&scratch);
    let recovery = RecoveryScratch::new_checkpoint(b"prior recovery fixture");
    crate::PRESENT_RECOVERY.with(|slot| drop(slot.borrow_mut().take()));
    let (app, _initial_state) = make_test_fixture();
    let original = empty_session();
    let state = state_with_dialogs(
        &app,
        original.clone(),
        Rc::new(ScriptedFileDialogs::new([Some(test_file.clone())], [])),
    );
    app.show().expect("show test app");
    crate::wire_close_guard(&app, &state);

    crate::open_deck_from_picker(&app, &state);
    assert!(app.get_pptx_import_warning_open());
    app.invoke_pptx_import_continue();

    assert!(app.get_pptx_import_warning_open());
    assert!(app.get_status_left().contains("Import not completed"));
    assert!(crate::deck_guard::content_matches(
        &state.session.borrow().document,
        &original.document
    ));
    assert_eq!(
        table_actions::PENDING_PPTX_IMPORT
            .with(|slot| slot.borrow().as_ref().map(|pending| pending.path.clone())),
        Some(test_file)
    );
    assert!(crate::PRESENT_RECOVERY.with(|slot| slot.borrow().is_none()));

    app.invoke_pptx_import_cancel();

    assert!(app.get_pptx_import_warning_open());
    assert!(app
        .get_status_left()
        .contains("Cancel not completed: current presentation recovery could not be secured"));
    assert!(table_actions::PENDING_PPTX_IMPORT.with(|slot| slot.borrow().is_some()));
    app.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);

    assert!(
        !app.window().is_visible(),
        "a missing service before checkpointing must not strand the warning on close"
    );
    assert!(app.get_pptx_import_warning_open());
    assert!(table_actions::PENDING_PPTX_IMPORT.with(|slot| slot.borrow().is_some()));
    assert!(crate::deck_guard::content_matches(
        &state.session.borrow().document,
        &original.document
    ));
    assert_eq!(
        state.session.borrow().transitions,
        original.transitions,
        "the visible session must remain unchanged"
    );
    assert_eq!(
        recovery.latest_payload(),
        Some(b"prior recovery fixture".to_vec()),
        "closing with the candidate still pending preserves the previous recovery payload"
    );
}

#[test]
fn pre_checkpoint_quarantine_failure_does_not_block_close_or_change_recovery() {
    let scratch = TestDirectory::new("pptx-warning-quarantine-failure");
    let (test_file, _candidate) = lossy_smartart_pptx(&scratch);
    let _state_directory = crate::test_environment::StateDirectoryScratch::new();
    let kept_directory = crate::startup_recovery::kept_drafts_directory()
        .expect("compute isolated kept-drafts directory");
    std::fs::create_dir_all(kept_directory.parent().expect("kept-drafts parent"))
        .expect("create isolated state parent");
    std::fs::write(&kept_directory, b"block directory creation")
        .expect("place a file where the kept-drafts directory belongs");

    let raw_recovery = b"unreadable recovery fixture".to_vec();
    let recovery = RecoveryScratch::new_checkpoint(&raw_recovery);
    let plan = crate::startup_recovery::startup_sessions(Some(&raw_recovery), Some(&test_file))
        .expect("prepare startup with unreadable recovery and lossy PPTX");
    let candidate = plan.pending_import.expect("lossy startup candidate");
    let kept = plan.kept.expect("unreadable startup recovery to preserve");
    assert!(!kept.readable);
    assert_eq!(kept.bytes, raw_recovery);
    let original = plan.session;
    let pending = table_actions::PendingPptxImport {
        path: candidate.path,
        session: candidate.session,
        warnings: candidate.warnings,
        kept: Some(kept),
        preserved_path: None,
        cancel_must_checkpoint_visible_session: false,
    };

    let (app, _initial_state) = make_test_fixture();
    app.show().expect("show test app");
    let state = state_with_dialogs(
        &app,
        original.clone(),
        Rc::new(ScriptedFileDialogs::new([], [])),
    );
    crate::wire_close_guard(&app, &state);
    table_actions::PENDING_PPTX_IMPORT.with(|slot| *slot.borrow_mut() = Some(pending));
    crate::recovery_deferred::hold_for_import_warning();
    app.set_pptx_import_warning_open(true);

    app.invoke_pptx_import_continue();

    assert!(app.get_pptx_import_warning_open());
    assert!(app.get_status_left().contains("Import not completed"));
    assert!(
        app.get_status_left().contains("could not create"),
        "the kept-drafts directory blocker must fail quarantine before checkpointing: {}",
        app.get_status_left()
    );
    assert!(table_actions::PENDING_PPTX_IMPORT.with(|slot| slot.borrow().is_some()));
    assert!(crate::deck_guard::content_matches(
        &state.session.borrow().document,
        &original.document
    ));
    assert_eq!(
        recovery.latest_payload(),
        Some(raw_recovery.clone()),
        "quarantine refusal occurs before the import candidate reaches recovery"
    );

    app.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);

    assert!(
        !app.window().is_visible(),
        "a quarantine failure before checkpointing must allow a safe close"
    );
    assert!(app.get_pptx_import_warning_open());
    assert!(table_actions::PENDING_PPTX_IMPORT.with(|slot| slot.borrow().is_some()));
    assert!(crate::deck_guard::content_matches(
        &state.session.borrow().document,
        &original.document
    ));
    assert_eq!(
        recovery.latest_payload(),
        Some(raw_recovery),
        "closing preserves the exact prior recovery payload"
    );
}

#[test]
fn cancel_after_a_partial_import_checkpoint_restores_the_visible_deck() {
    let scratch = TestDirectory::new("pptx-warning-partial-checkpoint");
    let (test_file, candidate) = lossy_smartart_pptx(&scratch);
    let original = PresentationSession::new(PresentationDocument::new(
        "visible-before-import",
        "Visible before import",
    ));
    let original_package =
        loom_present_core::save_presentation_session(&original).expect("package visible deck");
    let original_recovery = crate::recovery_draft::wrap(None, &original_package);
    let recovery = RecoveryScratch::new_checkpoint(&original_recovery);
    assert!(recovery.directory().join("operations.jsonl").is_file());
    let (app, _initial_state) = make_test_fixture();
    app.show().expect("show test app");
    let state = state_with_dialogs(
        &app,
        original.clone(),
        Rc::new(ScriptedFileDialogs::new([Some(test_file.clone())], [])),
    );
    crate::wire_close_guard(&app, &state);
    crate::open_deck_from_picker(&app, &state);
    assert!(app.get_pptx_import_warning_open());

    // The candidate record appends before the checkpoint-generation payload
    // write fails, leaving a real partially published recovery state.
    let failpoint = CheckpointPayloadFaultGuard::before_checkpoint_payload(recovery.directory());
    app.invoke_pptx_import_continue();
    assert_eq!(
        failpoint.hits(),
        1,
        "the checkpoint payload write was injected"
    );
    drop(failpoint);

    assert!(
        app.get_pptx_import_warning_open(),
        "Continue should remain pending on checkpoint failure; status={}",
        app.get_status_left()
    );
    assert!(app.get_status_left().contains("Import not completed"));
    assert!(crate::deck_guard::content_matches(
        &state.session.borrow().document,
        &original.document
    ));
    let partially_published = recovery
        .latest_payload()
        .expect("candidate operation is retained after checkpoint-generation failure");
    let (partial_draft, partial_session) =
        crate::recovery_draft::read(&partially_published).expect("read candidate operation");
    assert_eq!(partial_draft.source.as_deref(), Some(test_file.as_path()));
    assert!(crate::deck_guard::content_matches(
        &partial_session.document,
        &candidate.document
    ));

    app.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);

    assert!(
        app.window().is_visible(),
        "close stays blocked until recovery is repaired"
    );
    assert!(app.get_pptx_import_warning_open());
    assert!(table_actions::PENDING_PPTX_IMPORT.with(|slot| slot.borrow().is_some()));
    assert!(app
        .get_status_left()
        .contains("Choose Cancel to keep this presentation or Continue to retry"));
    let still_partial = recovery
        .latest_payload()
        .expect("blocked close retains the journaled import candidate");
    let (still_partial_draft, still_partial_session) =
        crate::recovery_draft::read(&still_partial).expect("read retained candidate operation");
    assert_eq!(
        still_partial_draft.source.as_deref(),
        Some(test_file.as_path())
    );
    assert!(crate::deck_guard::content_matches(
        &still_partial_session.document,
        &candidate.document
    ));

    app.invoke_pptx_import_cancel();

    assert!(!app.get_pptx_import_warning_open());
    assert!(table_actions::PENDING_PPTX_IMPORT.with(|slot| slot.borrow().is_none()));
    let recovered = recovery
        .latest_payload()
        .expect("Cancel checkpoints the visible deck before closing the warning");
    let (draft, recovered_session) =
        crate::recovery_draft::read(&recovered).expect("read visible-deck recovery");
    assert_eq!(draft.source, None);
    assert!(crate::deck_guard::content_matches(
        &recovered_session.document,
        &original.document
    ));

    app.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    assert!(
        !app.window().is_visible(),
        "close proceeds after recovery is repaired"
    );
}

#[test]
fn test_pdf_export_with_table_and_chart() {
    let mut doc = PresentationDocument::new("doc-1", "PDF Table Chart Export");
    let slide = doc.active_slide_mut().unwrap();
    slide.elements.clear();

    let table = default_slide_table(3, 3);
    slide.elements.push(SlideElement::new_table(
        "t1", 50.0, 50.0, 400.0, 200.0, table,
    ));

    let chart = default_slide_chart();
    slide.elements.push(SlideElement::new_chart(
        "c1", 500.0, 50.0, 450.0, 300.0, chart,
    ));

    let pdf_bytes = export_pdf(&doc);
    assert!(!pdf_bytes.is_empty());
    let session = PresentationSession::new(doc.clone());
    if let Ok(pptx) = export_pptx(&session) {
        let _ = std::fs::write("/tmp/present_table.pptx", pptx);
    }

    // Inspect PDF text content
    let pages = loom_pdf::inspect::page_text(&pdf_bytes).expect("inspect pdf");
    assert_eq!(pages.len(), 1);
    let page_text = &pages[0];
    assert!(
        page_text.contains("Header 1"),
        "PDF should contain table text: {page_text}"
    );
    assert!(
        page_text.contains("Sales Overview"),
        "PDF should contain chart title: {page_text}"
    );
}

fn is_powerpoint_com_available() -> bool {
    if !cfg!(windows) {
        return false;
    }
    let status = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "try { $p = New-Object -ComObject PowerPoint.Application; $p.Quit(); [System.Runtime.InteropServices.Marshal]::ReleaseComObject($p) | Out-Null; exit 0 } catch { exit 1 }",
        ])
        .status();
    matches!(status, Ok(s) if s.success())
}

#[test]
fn test_powerpoint_com_live_verification() {
    if !is_powerpoint_com_available() {
        println!("Skipping: Microsoft PowerPoint COM (or powershell.exe) not available on this platform/environment");
        return;
    }
    let mut doc = PresentationDocument::new("doc-1", "PowerPoint COM Live Verification");
    let slide = doc.active_slide_mut().unwrap();
    slide.elements.clear();

    let table = default_slide_table(3, 3);
    slide.elements.push(SlideElement::new_table(
        "t1", 50.0, 50.0, 400.0, 200.0, table,
    ));

    let chart = default_slide_chart();
    slide.elements.push(SlideElement::new_chart(
        "c1", 500.0, 50.0, 450.0, 300.0, chart,
    ));

    let session = PresentationSession::new(doc);
    let pptx_bytes = export_pptx(&session).expect("export pptx");

    let temp_dir = std::env::temp_dir().join(format!("loom-ppt-com-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();
    let pptx_path = temp_dir.join("live_verification.pptx");
    std::fs::write(&pptx_path, &pptx_bytes).unwrap();

    let path_str = pptx_path.to_str().unwrap().replace('\\', "/");
    let script = format!(
        r#"
$ppt = New-Object -ComObject PowerPoint.Application
try {{
    $pres = $ppt.Presentations.Open('{path_str}', 0, 0, 0)
    if ($pres.Slides.Count -ne 1) {{ throw "Expected 1 slide, got $($pres.Slides.Count)" }}
    $slide = $pres.Slides.Item(1)
    
    $hasTable = $false
    $hasChart = $false
    
    foreach ($shape in $slide.Shapes) {{
        if ($shape.HasTable -eq -1) {{
            $hasTable = $true
            if ($shape.Table.Rows.Count -ne 3) {{ throw "Expected 3 rows, got $($shape.Table.Rows.Count)" }}
            if ($shape.Table.Columns.Count -ne 3) {{ throw "Expected 3 cols, got $($shape.Table.Columns.Count)" }}
            $cellText = $shape.Table.Cell(1, 1).Shape.TextFrame.TextRange.Text.Trim()
            if ($cellText -ne "Header 1") {{ throw "Expected 'Header 1', got '$cellText'" }}
        }}
        if ($shape.HasChart -eq -1) {{
            $hasChart = $true
            $chartTitle = $shape.Chart.ChartTitle.Text.Trim()
            if ($chartTitle -ne "Sales Overview") {{ throw "Expected 'Sales Overview', got '$chartTitle'" }}
        }}
    }}
    
    if (-not $hasTable) {{ throw "Slide missing Table shape" }}
    if (-not $hasChart) {{ throw "Slide missing Chart shape" }}
    Write-Host "POWERPOINT_COM_SUCCESS"
    $pres.Close()
}} finally {{
    $ppt.Quit()
    [System.Runtime.InteropServices.Marshal]::ReleaseComObject($ppt) | Out-Null
}}
"#
    );

    let output = std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command", &script])
        .output()
        .expect("run powershell");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let _ = std::fs::remove_dir_all(&temp_dir);

    assert!(
        output.status.success() && stdout.contains("POWERPOINT_COM_SUCCESS"),
        "PowerPoint COM verification failed: status: {:?}, stdout: {}, stderr: {}",
        output.status,
        stdout,
        stderr
    );
}

#[test]
fn test_t4a_present_table_gaps() {
    let (app, state) = make_test_fixture();
    app.invoke_add_table();

    // 1. Inspector toggles with Slint names (underscore): header_row, banded_rows, first_column
    app.invoke_toggle_table_option("header_row".into());
    {
        let session = state.session.borrow();
        let table = session.document.active_slide().unwrap().elements[0]
            .table()
            .unwrap();
        assert!(!table.header_row, "header_row should toggle to false");
    }
    app.invoke_toggle_table_option("banded_rows".into());
    {
        let session = state.session.borrow();
        let table = session.document.active_slide().unwrap().elements[0]
            .table()
            .unwrap();
        assert!(!table.banded_rows, "banded_rows should toggle to false");
    }
    app.invoke_toggle_table_option("first_column".into());
    {
        let session = state.session.borrow();
        let table = session.document.active_slide().unwrap().elements[0]
            .table()
            .unwrap();
        assert!(table.first_column, "first_column should toggle to true");
    }

    // 2. Table action with Slint names: del_row, del_col
    app.invoke_table_action("del_row".into());
    {
        let session = state.session.borrow();
        let table = session.document.active_slide().unwrap().elements[0]
            .table()
            .unwrap();
        assert_eq!(
            table.row_count(),
            2,
            "del_row should decrease rows from 3 to 2"
        );
    }
    app.invoke_table_action("del_col".into());
    {
        let session = state.session.borrow();
        let table = session.document.active_slide().unwrap().elements[0]
            .table()
            .unwrap();
        assert_eq!(
            table.col_count(),
            2,
            "del_col should decrease cols from 3 to 2"
        );
    }

    // 3. Insert table with custom dimensions
    app.invoke_insert_table_size(4, 5);
    {
        let session = state.session.borrow();
        let slide = session.document.active_slide().unwrap();
        let last_elem = slide.elements.last().unwrap();
        let table = last_elem.table().unwrap();
        assert_eq!(table.row_count(), 4);
        assert_eq!(table.col_count(), 5);
    }

    // 3b. Insert Table dialog open/close and properties
    app.set_insert_table_dialog_open(true);
    assert!(app.get_insert_table_dialog_open());
    app.set_insert_table_dialog_rows(5);
    app.set_insert_table_dialog_cols(4);
    assert_eq!(app.get_insert_table_dialog_rows(), 5);
    assert_eq!(app.get_insert_table_dialog_cols(), 4);
    app.set_insert_table_dialog_open(false);
    assert!(!app.get_insert_table_dialog_open());

    // 4. Delete row and column from command dispatch and palette
    assert!(dispatch_command(&app, "table.delete_row"));
    assert!(dispatch_command(&app, "table.delete_column"));
    let palette = master_palette(&app);
    assert!(palette
        .iter()
        .any(|cmd| matches!(cmd.action, PaletteAction::DeleteTableRow)));
    assert!(palette
        .iter()
        .any(|cmd| matches!(cmd.action, PaletteAction::DeleteTableColumn)));

    // 5. Table keyboard operation: Enter to edit cell, Escape
    let enter_key: SharedString = slint::platform::Key::Return.into();
    let esc_key: SharedString = slint::platform::Key::Escape.into();
    assert!(table_actions::handle_table_key(
        &app,
        &state,
        enter_key.as_str(),
        false
    ));
    assert!(table_actions::is_table_editing_cell());
    assert!(table_actions::handle_table_key(
        &app,
        &state,
        esc_key.as_str(),
        false
    ));
    assert!(!table_actions::is_table_editing_cell());
}

#[test]
fn pressing_a_table_cell_makes_that_cell_the_active_one() {
    let (app, state) = make_test_fixture();
    app.invoke_add_table();
    let (id, rows, cols) = {
        let session = state.session.borrow();
        let element = session
            .document
            .active_slide()
            .unwrap()
            .elements
            .iter()
            .find(|e| e.element_type == loom_present_core::ElementType::Table)
            .expect("a table was inserted")
            .clone();
        let table = element.table().unwrap();
        (element.id.clone(), table.rows.len(), table.columns.len())
    };
    assert!(rows >= 2 && cols >= 2);
    let index = state
        .session
        .borrow()
        .document
        .active_slide()
        .unwrap()
        .elements
        .iter()
        .position(|e| e.id == id)
        .unwrap() as i32;
    // The pointer lands in the middle of the last row and the last column.
    use slint::ComponentHandle as _;
    let parts = app.global::<crate::ObjectParts>();
    parts.set_press_fx(1.0 - 0.5 / cols as f32);
    parts.set_press_fy(1.0 - 0.5 / rows as f32);
    app.invoke_element_pressed(index, false);
    assert_eq!(
        table_actions::get_active_table_cell(&id),
        (rows - 1, cols - 1),
        "the cell under the pointer is the active cell"
    );
    // And a press on the first cell moves it back.
    parts.set_press_fx(0.5 / cols as f32);
    parts.set_press_fy(0.5 / rows as f32);
    app.invoke_element_pressed(index, false);
    assert_eq!(table_actions::get_active_table_cell(&id), (0, 0));
}

#[test]
fn insert_chart_builds_the_chart_from_a_copied_range_and_never_invents_data() {
    let _guard = MOCK_CLIPBOARD.lock().unwrap_or_else(|e| e.into_inner());
    loom_clipboard::set_mock_clipboard_enabled(true);
    let (app, state) = make_test_fixture();
    let charts = |state: &Rc<GuiState>| {
        state
            .session
            .borrow()
            .document
            .active_slide()
            .unwrap()
            .elements
            .iter()
            .filter(|e| e.element_type == ElementType::Chart)
            .count()
    };

    // The command names its Sheets prerequisite before the user invokes it.
    match super::build_present_menu_bar().find_item("slide.insert_chart") {
        Some(loom_desktop::MenuItem::Action { label, .. }) => {
            assert_eq!(label, "Paste Chart from Sheets...")
        }
        other => panic!("chart menu command missing: {other:?}"),
    }
    assert!(master_palette(&app).iter().any(|command| matches!(
        command.action,
        PaletteAction::AddChart
    ) && command.label
        == "Paste Chart from Sheets"));

    // Nothing useful on the clipboard: no chart, and the status says what to do.
    loom_clipboard::write_clipboard(&loom_clipboard::ClipboardData {
        plain_text: Some("just some words".into()),
        ..Default::default()
    })
    .unwrap();
    app.invoke_add_chart();
    assert_eq!(charts(&state), 0, "no data, no chart");
    assert!(
        app.get_status_left().contains("Copy a chart")
            && app.get_status_left().contains("from Sheets"),
        "{}",
        app.get_status_left()
    );

    // A copied range with a header row and a label column becomes the chart.
    loom_clipboard::write_clipboard(&loom_clipboard::ClipboardData {
        plain_text: Some(
            "Region	Q1	Q2
North	10	20
South	30	40"
                .into(),
        ),
        ..Default::default()
    })
    .unwrap();
    app.invoke_add_chart();
    assert_eq!(charts(&state), 1);
    let session = state.session.borrow();
    let chart = session
        .document
        .active_slide()
        .unwrap()
        .elements
        .iter()
        .find(|e| e.element_type == ElementType::Chart)
        .unwrap()
        .chart()
        .unwrap();
    assert_eq!(chart.categories, vec!["North", "South"]);
    assert_eq!(chart.series.len(), 2);
    assert_eq!(chart.series[0].name, "Q1");
    assert_eq!(chart.series[1].values, vec![20.0, 40.0]);
    drop(session);
    loom_clipboard::set_mock_clipboard_enabled(false);
}

static MOCK_CLIPBOARD: std::sync::Mutex<()> = std::sync::Mutex::new(());
