//! Suite-level gates verification for Loom Present.
//!
//! Validates:
//! 1. Sheets range -> Clipboard -> Present slide table element -> PPTX export & re-import round trip.
//! 2. Sheets chart -> Clipboard -> Present slide chart element -> PPTX export & re-import round trip.
//! 3. Present slide table in-place editing, Tab navigation, Arrow navigation, and row/column mutations.

use loom_clipboard::sheets_interop::{sheets_range_to_table, SheetsCellData};
use loom_clipboard::ClipboardData;
use loom_objects::chart::{Chart, ChartSeries, ChartType};
use loom_present_core::{
    chart_element_from_clipboard, default_slide_table, export_pptx, import_pptx,
    table_element_from_clipboard, ElementType, PresentationDocument, PresentationSession, Slide,
    SlideElement, TableDirection,
};

#[test]
fn test_sheets_range_pastes_as_present_table_and_exports_pptx_and_reimports() {
    // 1. Build a rich Sheets range
    let grid = vec![
        vec![
            SheetsCellData {
                value: "Product".into(),
                formatted_text: "Product".into(),
                bold: true,
                background_color: Some("#E2E8F0".into()),
                width_pt: 120.0,
                ..Default::default()
            },
            SheetsCellData {
                value: "Qty".into(),
                formatted_text: "Qty".into(),
                bold: true,
                background_color: Some("#E2E8F0".into()),
                width_pt: 80.0,
                ..Default::default()
            },
            SheetsCellData {
                value: "Price".into(),
                formatted_text: "Price".into(),
                bold: true,
                background_color: Some("#E2E8F0".into()),
                width_pt: 90.0,
                ..Default::default()
            },
        ],
        vec![
            SheetsCellData {
                value: "Apples".into(),
                formatted_text: "Apples".into(),
                ..Default::default()
            },
            SheetsCellData {
                value: "50".into(),
                formatted_text: "50".into(),
                ..Default::default()
            },
            SheetsCellData {
                value: "$1.50".into(),
                formatted_text: "$1.50".into(),
                ..Default::default()
            },
        ],
        vec![
            SheetsCellData {
                value: "Oranges".into(),
                formatted_text: "Oranges".into(),
                ..Default::default()
            },
            SheetsCellData {
                value: "30".into(),
                formatted_text: "30".into(),
                ..Default::default()
            },
            SheetsCellData {
                value: "$2.00".into(),
                formatted_text: "$2.00".into(),
                ..Default::default()
            },
        ],
    ];

    // 2. Convert Sheets range to loom_objects::Table
    let loom_tbl = sheets_range_to_table(&grid);
    assert_eq!(loom_tbl.row_count(), 3);
    assert_eq!(loom_tbl.col_count(), 3);

    // 3. Serialize to multi-format ClipboardData
    let clipboard = ClipboardData::from_table(&loom_tbl);
    assert!(clipboard.loom_objects_json.is_some());
    assert!(clipboard.html.is_some());
    assert!(clipboard.tsv.is_some());

    // 4. In Present: paste table as slide element
    let resolved_tbl = clipboard.to_table().expect("clipboard resolves table");
    let table_elem = table_element_from_clipboard(resolved_tbl, "table-slide-elem");
    assert_eq!(table_elem.element_type, ElementType::Table);

    // 5. Build presentation deck
    let mut doc = PresentationDocument::new("deck-suite", "Quarterly Executive Review");
    let mut slide = Slide::new("slide-1", "Financial Performance", "TitleAndContent");
    slide.add_element(SlideElement {
        id: "title-1".into(),
        element_type: ElementType::Title,
        content: "Financial Performance".into(),
        x: 50.0,
        y: 40.0,
        width: 900.0,
        height: 60.0,
        rotation_deg: 0.0,
        action: None,
    });
    slide.add_element(table_elem);
    doc.slides.clear();
    doc.slides.push(slide);

    // 6. Export to PPTX
    let session = PresentationSession::new(doc);
    let pptx_bytes = export_pptx(&session).expect("export pptx succeeds");
    assert!(!pptx_bytes.is_empty());
    if let Ok(dir) = std::env::var("LOOM_SUITE_FIXTURE_DIR") {
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(
            std::path::Path::new(&dir).join("suite-present-table.pptx"),
            &pptx_bytes,
        );
    }

    // 7. Re-import PPTX and verify slide and table fidelity
    let reimported = import_pptx(&pptx_bytes).expect("import pptx succeeds");
    assert_eq!(reimported.title, "Quarterly Executive Review");
    assert_eq!(reimported.slides.len(), 1);

    let reimported_slide = &reimported.slides[0];
    let reimported_table_elem = reimported_slide
        .elements
        .iter()
        .find(|e| e.element_type == ElementType::Table)
        .expect("table element present on reimported slide");

    let parsed_table = reimported_table_elem
        .table()
        .expect("parsed table from slide element");
    assert_eq!(parsed_table.row_count(), 3);
    assert_eq!(parsed_table.col_count(), 3);
    assert_eq!(parsed_table.cell(0, 0).unwrap().content.text(), "Product");
    assert_eq!(parsed_table.cell(0, 1).unwrap().content.text(), "Qty");
    assert_eq!(parsed_table.cell(0, 2).unwrap().content.text(), "Price");
    assert_eq!(parsed_table.cell(1, 0).unwrap().content.text(), "Apples");
    assert_eq!(parsed_table.cell(1, 1).unwrap().content.text(), "50");
    assert_eq!(parsed_table.cell(1, 2).unwrap().content.text(), "$1.50");
    assert_eq!(parsed_table.cell(2, 0).unwrap().content.text(), "Oranges");
    assert_eq!(parsed_table.cell(2, 1).unwrap().content.text(), "30");
    assert_eq!(parsed_table.cell(2, 2).unwrap().content.text(), "$2.00");
}

#[test]
fn test_sheets_chart_pastes_as_present_chart_and_exports_pptx_and_reimports() {
    // 1. Build a Chart model from Sheets
    let chart = Chart::from_data(
        "chart-exec-sales",
        Some("Quarterly Revenue".into()),
        ChartType::Column,
        vec!["Q1".into(), "Q2".into(), "Q3".into()],
        vec![
            ChartSeries {
                name: "Product A".into(),
                values: vec![100.0, 150.0, 180.0],
                color: Some("#3B82F6".into()),
            },
            ChartSeries {
                name: "Product B".into(),
                values: vec![80.0, 120.0, 160.0],
                color: Some("#10B981".into()),
            },
        ],
    );

    // 2. Multi-format clipboard transfer
    let clipboard = ClipboardData::from_chart(&chart);
    assert!(clipboard.loom_objects_json.is_some());
    assert!(clipboard.html.is_some());
    assert!(clipboard.tsv.is_some());

    // 3. Paste into Present slide element
    let resolved_chart = clipboard.to_chart().expect("clipboard resolves chart");
    let chart_elem = chart_element_from_clipboard(resolved_chart, "chart-slide-elem");
    assert_eq!(chart_elem.element_type, ElementType::Chart);

    // 4. Build presentation deck
    let mut doc = PresentationDocument::new("deck-chart-suite", "Executive Revenue Report");
    let mut slide = Slide::new("slide-chart", "Revenue Trends", "TitleAndContent");
    slide.add_element(SlideElement {
        id: "title-chart".into(),
        element_type: ElementType::Title,
        content: "Revenue Trends".into(),
        x: 50.0,
        y: 40.0,
        width: 900.0,
        height: 60.0,
        rotation_deg: 0.0,
        action: None,
    });
    slide.add_element(chart_elem);
    doc.slides.clear();
    doc.slides.push(slide);

    // 5. Export to PPTX
    let session = PresentationSession::new(doc);
    let pptx_bytes = export_pptx(&session).expect("export pptx with chart succeeds");
    assert!(!pptx_bytes.is_empty());
    if let Ok(dir) = std::env::var("LOOM_SUITE_FIXTURE_DIR") {
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(
            std::path::Path::new(&dir).join("suite-present-chart.pptx"),
            &pptx_bytes,
        );
    }

    // 6. Re-import PPTX and verify chart fidelity
    let reimported = import_pptx(&pptx_bytes).expect("import pptx succeeds");
    assert_eq!(reimported.title, "Executive Revenue Report");
    assert_eq!(reimported.slides.len(), 1);

    let reimported_slide = &reimported.slides[0];
    let reimported_chart_elem = reimported_slide
        .elements
        .iter()
        .find(|e| e.element_type == ElementType::Chart)
        .expect("chart element present on reimported slide");

    let parsed_chart = reimported_chart_elem
        .chart()
        .expect("parsed chart from slide element");
    assert_eq!(parsed_chart.title.as_deref(), Some("Quarterly Revenue"));
    assert_eq!(parsed_chart.series.len(), 2);
    assert_eq!(parsed_chart.series[0].name, "Product A");
    assert_eq!(parsed_chart.series[1].name, "Product B");
}

#[test]
fn test_present_table_keyboard_navigation_and_mutations() {
    let table = default_slide_table(3, 3);
    let mut elem = table_element_from_clipboard(table, "interactive-table");

    // 1. In-place cell text editing
    assert!(elem.update_table_cell_text(0, 0, "Custom Header"));
    let current_table = elem.table().unwrap();
    assert_eq!(
        current_table.cell(0, 0).unwrap().content.text(),
        "Custom Header"
    );

    // 2. Tab navigation forward and backward
    let (r1, c1) = elem.tab_navigate_table(0, 0, false);
    assert_eq!((r1, c1), (0, 1));
    let (r0, c0) = elem.tab_navigate_table(0, 1, true);
    assert_eq!((r0, c0), (0, 0));

    // 3. Tab at end appends a new row
    let (r_end, c_end) = elem.tab_navigate_table(2, 2, false);
    assert_eq!((r_end, c_end), (3, 0));
    let table_appended = elem.table().unwrap();
    assert_eq!(table_appended.row_count(), 4);

    // 4. Arrow navigation
    let (ar_down, ac_down) = elem.arrow_navigate_table(0, 0, TableDirection::Down);
    assert_eq!((ar_down, ac_down), (1, 0));
    let (ar_right, ac_right) = elem.arrow_navigate_table(1, 0, TableDirection::Right);
    assert_eq!((ar_right, ac_right), (1, 1));

    // 5. Row insertion and deletion
    assert!(elem.insert_table_row(1, true));
    let table_after_row = elem.table().unwrap();
    assert_eq!(table_after_row.row_count(), 5);

    assert!(elem.delete_table_row(2));
    let table_after_del_row = elem.table().unwrap();
    assert_eq!(table_after_del_row.row_count(), 4);

    // 6. Column insertion and deletion
    assert!(elem.insert_table_column(0, true));
    let table_after_col = elem.table().unwrap();
    assert_eq!(table_after_col.col_count(), 4);

    assert!(elem.delete_table_column(1));
    let table_after_del_col = elem.table().unwrap();
    assert_eq!(table_after_del_col.col_count(), 3);
}
