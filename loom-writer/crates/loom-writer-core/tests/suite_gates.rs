//! Suite-level gates verification for Loom Writer.
//!
//! Validates:
//! 1. Sheets range -> Clipboard -> Writer table block -> DOCX export & re-import round trip.
//! 2. Sheets chart -> Clipboard -> Writer chart block -> DOCX export.
//! 3. 100x20 Writer table typing keystroke latency stays within the 16.7 ms budget (60 fps).
//! 4. Writer table keyboard navigation (Tab, Arrow) and structural mutations.

use loom_clipboard::sheets_interop::{sheets_range_to_table, SheetsCellData};
use loom_clipboard::ClipboardData;
use loom_objects::chart::{Chart, ChartSeries, ChartType};
use loom_writer_core::docx::import_docx;
use loom_writer_core::export_docx;
use loom_writer_core::{
    compute_cell_ranges, RichBlock, TableArrowDirection, WriterDocument, WriterTable,
    TABLE_BLOCK_KIND,
};
use std::time::Instant;

#[test]
fn test_sheets_range_pastes_as_writer_table_and_exports_docx_and_reimports() {
    // 1. Build a rich Sheets range (3 rows x 3 columns)
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

    // 4. In Writer: paste table from clipboard
    let resolved_tbl = clipboard.to_table().expect("clipboard resolves table");
    let writer_tbl = WriterTable::from_loom_table(&resolved_tbl);
    assert_eq!(writer_tbl.rows.len(), 3);
    assert_eq!(writer_tbl.columns(), 3);
    assert_eq!(writer_tbl.rows[0], vec!["Product", "Qty", "Price"]);
    assert_eq!(writer_tbl.rows[1], vec!["Apples", "50", "$1.50"]);
    assert_eq!(writer_tbl.rows[2], vec!["Oranges", "30", "$2.00"]);

    // 5. Insert table into WriterDocument
    let mut doc = WriterDocument::new("suite-report", "Suite Report");
    doc.blocks
        .push(RichBlock::new(1, "heading1", "Financial Overview"));
    let table_block_id = doc.next_id();
    doc.blocks.push(RichBlock::new(
        table_block_id,
        TABLE_BLOCK_KIND,
        &writer_tbl.to_markdown(),
    ));
    doc.blocks
        .push(RichBlock::new(3, "paragraph", "End of report."));

    // 6. Export document to DOCX
    let docx_result = export_docx(&doc).expect("export docx succeeds");
    assert!(!docx_result.bytes.is_empty());
    if let Ok(dir) = std::env::var("LOOM_SUITE_FIXTURE_DIR") {
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(
            std::path::Path::new(&dir).join("suite-writer-table.docx"),
            &docx_result.bytes,
        );
    }

    // 7. Re-import DOCX and verify table fidelity
    let reimported = import_docx(&docx_result.bytes).expect("import docx succeeds");
    assert_eq!(reimported.title, "Suite Report");
    assert!(reimported.blocks.len() >= 3);

    // Find table block in reimported document
    let reimported_table_block = reimported
        .blocks
        .iter()
        .find(|b| b.kind == TABLE_BLOCK_KIND)
        .expect("table block present in reimported document");

    let parsed_table = reimported
        .table_from_block(reimported_table_block.id)
        .expect("parsed table from reimported block");

    assert_eq!(parsed_table.rows.len(), 3);
    assert_eq!(parsed_table.columns(), 3);
    assert_eq!(parsed_table.rows[0], vec!["Product", "Qty", "Price"]);
    assert_eq!(parsed_table.rows[1], vec!["Apples", "50", "$1.50"]);
    assert_eq!(parsed_table.rows[2], vec!["Oranges", "30", "$2.00"]);
}

#[test]
fn test_sheets_chart_pastes_as_writer_chart_block_and_exports_docx() {
    // 1. Build a Chart model as produced by Sheets
    let chart = Chart::from_data(
        "chart-sales-q",
        Some("Quarterly Sales".into()),
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

    // 2. Put onto multi-format clipboard
    let clipboard = ClipboardData::from_chart(&chart);
    assert!(clipboard.loom_objects_json.is_some());
    assert!(clipboard.html.is_some());
    assert!(clipboard.tsv.is_some());

    // 3. In Writer: read chart and insert as chart block
    let resolved_chart = clipboard.to_chart().expect("clipboard resolves chart");
    let mut doc = WriterDocument::new("chart-doc", "Sales Report");
    doc.blocks
        .push(RichBlock::new(1, "heading1", "Sales Performance"));
    let chart_block_id = doc
        .insert_chart_block(usize::MAX, resolved_chart)
        .expect("insert chart block");

    // Verify chart block retrieval
    let retrieved_chart = doc
        .chart_from_block(chart_block_id)
        .expect("retrieved chart from block");
    assert_eq!(retrieved_chart.title.as_deref(), Some("Quarterly Sales"));
    assert_eq!(retrieved_chart.chart_type, ChartType::Column);
    assert_eq!(retrieved_chart.series.len(), 2);

    // 4. Export to DOCX
    let docx_result = export_docx(&doc).expect("export docx with chart");
    assert!(!docx_result.bytes.is_empty());
    if let Ok(dir) = std::env::var("LOOM_SUITE_FIXTURE_DIR") {
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(
            std::path::Path::new(&dir).join("suite-writer-chart.docx"),
            &docx_result.bytes,
        );
    }
}

#[test]
fn test_writer_100x20_table_keystroke_latency_under_16_7_ms() {
    // Construct a 100-row x 20-column table (2,000 cells)
    const ROWS: usize = 100;
    const COLS: usize = 20;

    let mut table = WriterTable::new("perf-table", ROWS, COLS);
    for r in 0..ROWS {
        for c in 0..COLS {
            table.set(r, c, format!("R{r}C{c}"));
        }
    }

    let mut doc = WriterDocument::new("perf-doc", "Performance Test Document");
    let table_block_id = doc.next_id();
    doc.blocks.push(RichBlock::new(
        table_block_id,
        TABLE_BLOCK_KIND,
        &table.to_markdown(),
    ));

    // Target cells across multiple regions: top, middle, bottom
    let sample_cells = [(0, 0), (25, 5), (50, 10), (75, 15), (99, 19)];
    let block_text = doc.blocks[0].text.as_str();
    let grid = compute_cell_ranges(block_text);

    let mut latencies_us = Vec::with_capacity(100);

    for &(row, col) in &sample_cells {
        let (cell_start, _) = grid[row][col];
        // Position caret inside cell
        doc.set_selection(loom_writer_core::TextSelection::caret(cell_start));

        // Type 20 characters in this cell
        for i in 0..20 {
            let ch = ((b'a' + (i % 26) as u8) as char).to_string();
            let start = Instant::now();
            let _ = doc
                .replace_selection_text(&ch)
                .expect("replace selection text succeeds");
            let elapsed = start.elapsed();
            latencies_us.push(elapsed.as_micros());
        }
    }

    assert_eq!(latencies_us.len(), 100);
    latencies_us.sort_unstable();

    let p50 = latencies_us[50];
    let p95 = latencies_us[95];
    let p99 = latencies_us[99];
    let max = latencies_us[99];

    println!(
        "100x20 Table (2,000 cells) Keystroke Latency (100 typing strokes): p50={}µs, p95={}µs, p99={}µs, max={}µs",
        p50, p95, p99, max
    );

    // The gate specifies: "typing in a 100x20 Writer table stays within the 16.7 ms keystroke budget"
    // 16.7 ms = 16,700 µs
    const BUDGET_US: u128 = 16_700;
    assert!(
        p95 < BUDGET_US,
        "p95 latency {}µs must be well within the 16.7ms ({}µs) 60fps frame budget",
        p95,
        BUDGET_US
    );
    assert!(
        max < BUDGET_US,
        "max latency {}µs must be within the 16.7ms ({}µs) frame budget",
        max,
        BUDGET_US
    );
}

#[test]
fn test_writer_table_keyboard_navigation_and_mutations() {
    let mut doc = WriterDocument::new("nav-doc", "Navigation Test Document");
    let table_block_id = doc.insert_table_block(0, 3, 3).expect("insert table block");

    // 1. Tab navigation between cells
    let caret0 = doc.selection.focus;
    let caret1 = doc
        .tab_navigate_table(caret0, false)
        .expect("tab navigate forward");
    assert!(caret1 > caret0);

    let caret_back = doc
        .tab_navigate_table(caret1, true)
        .expect("tab navigate backward");
    assert_eq!(
        doc.table_cell_at_offset(caret_back).map(|c| (c.row, c.col)),
        doc.table_cell_at_offset(caret0).map(|c| (c.row, c.col)),
        "back-tab returns to the first cell"
    );

    // 2. Tab navigation at end of table appends a new row
    // Navigate forward to final cell
    let mut current_caret = caret0;
    for _ in 0..8 {
        current_caret = doc
            .tab_navigate_table(current_caret, false)
            .expect("tab forward");
    }
    // Now at last cell (row 2, col 2); one more Tab appends row 3
    let caret_new_row = doc
        .tab_navigate_table(current_caret, false)
        .expect("tab at end appends row");
    assert!(caret_new_row > current_caret);

    let table = doc
        .table_from_block(table_block_id)
        .expect("table after append");
    assert_eq!(table.rows.len(), 4, "new row was appended");

    // 3. Arrow navigation
    let caret_down = doc
        .arrow_navigate_table(caret0, TableArrowDirection::Down)
        .expect("arrow down");
    assert!(caret_down > caret0);
    let caret_right = doc
        .arrow_navigate_table(caret_down, TableArrowDirection::Right)
        .expect("arrow right");
    assert!(caret_right > caret_down);

    // 4. Row insertion and deletion
    let caret_inserted_row = doc
        .insert_table_row_at_offset(caret_right, true)
        .expect("insert row below");
    let table_after_insert = doc.table_from_block(table_block_id).unwrap();
    assert_eq!(table_after_insert.rows.len(), 5);

    let _ = doc
        .delete_table_row_at_offset(caret_inserted_row)
        .expect("delete row");
    let table_after_delete = doc.table_from_block(table_block_id).unwrap();
    assert_eq!(table_after_delete.rows.len(), 4);

    // 5. Column insertion and deletion
    let caret_col_insert = doc
        .insert_table_column_at_offset(caret0, true)
        .expect("insert column right");
    let table_col_insert = doc.table_from_block(table_block_id).unwrap();
    assert_eq!(table_col_insert.columns(), 4);

    let _ = doc
        .delete_table_column_at_offset(caret_col_insert)
        .expect("delete column");
    let table_col_delete = doc.table_from_block(table_block_id).unwrap();
    assert_eq!(table_col_delete.columns(), 3);
}
