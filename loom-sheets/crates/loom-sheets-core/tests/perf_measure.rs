use loom_sheets_core::persistence::{workbook_from_json, workbook_to_json};
use loom_sheets_core::{evaluate, Sheet};
use std::time::Instant;

#[test]
fn perf_measure_large_workbook() {
    // 200 rows x 20 cols of chained formulas + aggregations.
    let mut sheet = Sheet::new("Perf");
    for row in 1..=200 {
        sheet.set_str(&format!("A{row}"), &format!("{row}"));
        sheet.set_str(&format!("B{row}"), &format!("=A{row}*2"));
        sheet.set_str(&format!("C{row}"), &format!("=SUM(A{row}:B{row})"));
        sheet.set_str(
            &format!("D{row}"),
            &format!("=IF(C{row}>100,\"big\",\"small\")"),
        );
    }
    for col in [
        'E', 'F', 'G', 'H', 'I', 'J', 'K', 'L', 'M', 'N', 'O', 'P', 'Q', 'R', 'S', 'T',
    ] {
        sheet.set_str(&format!("{col}1"), &format!("=SUM(A1:A200)+{}", col as u32));
    }
    sheet.set_str("A201", "=SUM(A1:A200)");
    sheet.set_str("B201", "=AVERAGE(B1:B200)");
    sheet.set_str("C201", "=VLOOKUP(150,A1:B200,2,1)");

    let start = Instant::now();
    let vals = evaluate(&sheet);
    let eval_ms = start.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(vals.len(), 200 * 4 + 16 + 3);
    eprintln!("PERF evaluate 10k-cell sheet: {eval_ms:.1}ms");

    let start = Instant::now();
    let json = workbook_to_json(&[sheet.clone(), Sheet::new("Empty")], 0);
    let json_ms = start.elapsed().as_secs_f64() * 1000.0;
    eprintln!(
        "PERF workbook_to_json bytes={} time={json_ms:.1}ms",
        json.len()
    );

    let start = Instant::now();
    let back = workbook_from_json(&json).expect("round-trip");
    let parse_ms = start.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(back.sheets.len(), 2);
    eprintln!("PERF workbook_from_json time={parse_ms:.1}ms");
}
