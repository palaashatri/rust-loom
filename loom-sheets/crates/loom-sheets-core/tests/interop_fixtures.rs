//! Gate 11 fixture producer: builds a representative workbook through the real
//! model and exporters and writes it where independent applications (Excel,
//! LibreOffice) can open it.
//!
//! Run with:
//! `LOOM_INTEROP_OUT=<dir> cargo test -p loom-sheets-core --test interop_fixtures -- --ignored`
//!
//! Outputs: `sheets.xlsx`, `sheets-<n>.csv` (one per sheet), and
//! `sheets-expected.json`, the values Loom itself calculated for every cell
//! (the same values the exporter caches in the package). The scripts under
//! `loom-sheets/tests/interop/` and the LibreOffice conversion check compare
//! the other applications' view of the file with this JSON.

use loom_sheets_core::style::FillColor;
use loom_sheets_core::{
    export_xlsx_sheets, import_xlsx_sheets, to_csv_with_values, CellAlignment, CellRef, CellStyle,
    ChartKind, NumberFormat, Sheet, SheetChart, SheetObject, Value,
};
use std::path::PathBuf;

fn at(a1: &str) -> CellRef {
    CellRef::parse(a1).expect("valid cell reference")
}

fn header_style() -> CellStyle {
    CellStyle {
        bold: true,
        border: true,
        fill: FillColor::Gray,
        ..CellStyle::new()
    }
}

/// The fixture workbook: "Q1 Sales" (data, styles, formulas, chart, shape,
/// frozen header) and "Rates" (lookup source for the cross-sheet formula).
fn fixture_workbook() -> Vec<Sheet> {
    let mut sales = Sheet::new("Q1 Sales");
    for (a1, raw) in [
        ("A1", "Item"),
        ("B1", "Qty"),
        ("C1", "Price"),
        ("D1", "Total"),
        ("E1", "Shipped"),
        ("F1", "Margin"),
        ("A2", "Apples"),
        ("A3", "Pears"),
        ("A4", "Caf\u{e9} Zo\u{eb}"),
        ("A5", "D\u{fc}sseldorf \u{2013} \u{201c}quoted\u{201d}"),
        ("B2", "3"),
        ("B3", "10"),
        ("B4", "7"),
        ("B5", "2"),
        ("C2", "1.25"),
        ("C3", "0.8"),
        ("C4", "4.5"),
        ("C5", "12"),
        ("D2", "=B2*C2"),
        ("D3", "=B3*C3"),
        ("D4", "=B4*C4"),
        ("D5", "=B5*C5"),
        ("E2", "45366"),
        ("E3", "45367"),
        ("E4", "45368"),
        ("E5", "45369"),
        ("F2", "0.256"),
        ("F3", "0.1"),
        ("F4", "0.333"),
        ("F5", "0.05"),
        ("A7", "Totals"),
        ("B7", "=SUM(B2:B5)"),
        ("D7", "=SUM(D2:D5)"),
        ("H1", "Check"),
        ("H2", "=IF(D2>5,\"big\",\"small\")"),
        ("H3", "=VLOOKUP(\"Pears\",A2:C5,3,FALSE)"),
        ("H4", "=UPPER(A2)&\"-\"&LEN(A3)"),
        ("H5", "=LEFT(A4,4)&\"|\"&TRIM(\"  a  b  \")"),
        ("H6", "=Rates!B2*B2"),
        ("H7", "=SUM(Rates!B2:B4)"),
        ("H8", "=ROUND(D7,1)"),
    ] {
        sales.set_str(a1, raw);
    }
    for col in 0..=5 {
        sales.set_cell_style(CellRef { row: 0, col }, header_style());
    }
    sales.set_range_style(
        at("C2"),
        at("C5"),
        CellStyle {
            number_format: NumberFormat::Currency,
            decimal_places: Some(2),
            ..CellStyle::new()
        },
    );
    sales.set_range_style(
        at("E2"),
        at("E5"),
        CellStyle {
            number_format: NumberFormat::DateIso,
            ..CellStyle::new()
        },
    );
    sales.set_range_style(
        at("F2"),
        at("F5"),
        CellStyle {
            number_format: NumberFormat::Percentage,
            decimal_places: Some(1),
            italic: true,
            ..CellStyle::new()
        },
    );
    sales.set_cell_style(
        at("D7"),
        CellStyle {
            bold: true,
            underline: true,
            fill: FillColor::Yellow,
            number_format: NumberFormat::Number,
            decimal_places: Some(2),
            ..CellStyle::new()
        },
    );
    sales.set_cell_alignment(at("A7"), CellAlignment::Right);
    sales.set_col_width(0, 150.0);
    sales.set_col_width(3, 90.0);
    sales.set_row_height(0, 30.0);
    sales.freeze_panes(1, 1);
    sales.chart = Some(SheetChart {
        kind: ChartKind::Bar,
        title: "Totals by item".to_string(),
        cat_col: 0,
        val_col: 3,
        start_row: 1,
        end_row: Some(4),
    });
    sales
        .objects
        .push(SheetObject::shape(at("J10"), "Interop shape"));

    let mut rates = Sheet::new("Rates");
    for (a1, raw) in [
        ("A1", "Region"),
        ("B1", "Rate"),
        ("A2", "North"),
        ("A3", "South"),
        ("A4", "East"),
        ("B2", "1.5"),
        ("B3", "2"),
        ("B4", "0.25"),
    ] {
        rates.set_str(a1, raw);
    }
    vec![sales, rates]
}

fn json_for(sheets: &[Sheet]) -> String {
    let values = loom_sheets_core::workbook::evaluate_workbook(sheets);
    let mut out = serde_json::Map::new();
    for (sheet, vals) in sheets.iter().zip(&values) {
        let mut cells = serde_json::Map::new();
        let mut keys: Vec<_> = sheet.cells.keys().copied().collect();
        keys.sort();
        for key in keys {
            let value = vals.get(&key).cloned().unwrap_or(Value::Empty);
            let json = match &value {
                Value::Number(n) => serde_json::json!({"kind": "num", "value": n}),
                Value::Bool(b) => serde_json::json!({"kind": "bool", "value": b}),
                Value::Empty => serde_json::json!({"kind": "empty", "value": ""}),
                other => serde_json::json!({"kind": "text", "value": other.display()}),
            };
            cells.insert(key.to_a1(), json);
        }
        out.insert(sheet.name.clone(), serde_json::Value::Object(cells));
    }
    serde_json::to_string_pretty(&out).unwrap()
}

#[test]
#[ignore = "writes interop fixtures; set LOOM_INTEROP_OUT and run with --ignored"]
fn write_sheets_interop_fixtures() {
    let dir = PathBuf::from(std::env::var("LOOM_INTEROP_OUT").expect("set LOOM_INTEROP_OUT"));
    std::fs::create_dir_all(&dir).unwrap();
    let sheets = fixture_workbook();

    let xlsx = export_xlsx_sheets(&sheets).expect("xlsx export");
    std::fs::write(dir.join("sheets.xlsx"), &xlsx).unwrap();

    // Loom's own reader must accept what Loom wrote, or the check below is moot.
    let import = import_xlsx_sheets(&xlsx).expect("Loom reads its own export");
    assert_eq!(import.sheets.len(), 2);

    let values = loom_sheets_core::workbook::evaluate_workbook(&sheets);
    for (index, sheet) in sheets.iter().enumerate() {
        let csv = to_csv_with_values(sheet, &values[index]);
        std::fs::write(
            dir.join(format!("sheets-{}.csv", index + 1)),
            csv.as_bytes(),
        )
        .unwrap();
    }
    std::fs::write(dir.join("sheets-expected.json"), json_for(&sheets)).unwrap();
}
