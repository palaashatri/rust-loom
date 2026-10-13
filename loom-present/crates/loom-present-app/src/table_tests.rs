//! Verification tests for Wave 3: Tables and charts as first-class slide objects.
#![cfg(test)]

use std::cell::RefCell;
use std::rc::Rc;

use loom_desktop::{FileFilter, ScriptedFileDialogs};
use loom_objects::chart::ChartType;
use loom_present_core::{
    default_slide_chart, default_slide_table, export_pdf, export_pptx, import_pptx,
    inspect_pptx_unsupported, ElementType, PresentationDocument, PresentationSession, SlideElement,
};
use slint::SharedString;

use crate::{
    dispatch_command, empty_session, master_palette, refresh,
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

    // 2. Insert Chart
    app.invoke_add_chart();
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

    assert!(dispatch_command(&app, "slide.insert_chart"));
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
    let (app, _state) = make_test_fixture();

    // Create a mock PPTX containing unsupported SmartArt
    let doc = PresentationDocument::new("doc-1", "Unsupported Test");
    let session = PresentationSession::new(doc);
    let pptx_bytes = export_pptx(&session).expect("export pptx");

    // Add an unsupported SmartArt part
    let archive = loom_package::zip::PackageArchive::from_bytes(&pptx_bytes).expect("open zip");
    let mut new_archive = loom_package::zip::PackageArchive::new();
    for path in archive.paths() {
        if let Some(bytes) = archive.get(path) {
            let _ = new_archive.add(path, bytes.to_vec());
        }
    }
    // Inject SmartArt diagram
    let _ = new_archive.add("ppt/diagrams/data1.xml", b"<xml></xml>".to_vec());
    let buf = new_archive.to_bytes().expect("write package");

    let warnings = inspect_pptx_unsupported(&buf).expect("inspect");
    assert!(warnings.iter().any(|w| w.contains("SmartArt")));

    let temp_dir = std::env::temp_dir().join(format!("loom-pptx-warn-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();
    let test_file = temp_dir.join("test.pptx");
    std::fs::write(&test_file, &buf).unwrap();

    let dialogs = ScriptedFileDialogs::new([Some(test_file.clone())], []);
    let mut empty_s = empty_session();
    if let Some(s) = empty_s.document.active_slide_mut() {
        s.elements.clear();
    }
    let state_with_dialog = Rc::new(GuiState {
        last_saved: RefCell::new(empty_s.document.clone()),
        last_saved_transitions: RefCell::default(),
        session: RefCell::new(empty_s),
        pending_replacement: std::cell::Cell::new(None),
        selected_element: std::cell::Cell::new(0),
        inspector_available: std::cell::Cell::new(true),
        save_path: RefCell::new(None),
        dialogs: Rc::new(dialogs),
        deck_filter: FileFilter::new("Deck", ["loomdeck"]).expect("filter"),
        pdf_filter: FileFilter::new("PDF", ["pdf"]).expect("filter"),
        menu_service: None,
        drag_state: RefCell::new(DragState::default()),
    });
    table_actions::wire(&app, &state_with_dialog);

    // Open through picker triggers warning
    crate::open_deck_from_picker(&app, &state_with_dialog);
    assert!(app.get_pptx_import_warning_open());
    assert!(app.get_pptx_import_warning_message().contains("SmartArt"));

    // Cancel preserves current session
    app.invoke_pptx_import_cancel();
    assert!(!app.get_pptx_import_warning_open());
    assert_eq!(state_with_dialog.session.borrow().document.slides.len(), 1);

    let _ = std::fs::remove_dir_all(&temp_dir);
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

#[test]
#[cfg(windows)]
fn test_powerpoint_com_live_verification() {
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
