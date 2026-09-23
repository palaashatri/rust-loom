use loom_sheets_core::persistence::{workbook_from_json, workbook_to_json};
use loom_sheets_core::{evaluate, CellRef, Sheet, Value};
use std::time::Instant;

#[test]
fn perf_measure_large_workbook() {
    // 500 rows x 20 chained formulas, matching the 10,000-formula design gate.
    const ROWS: u32 = 500;
    const FORMULAS_PER_ROW: usize = 20;
    const COLUMNS: [&str; 21] = [
        "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R",
        "S", "T", "U",
    ];
    const FORMULA_COUNT: usize = ROWS as usize * FORMULAS_PER_ROW;

    let mut sheet = Sheet::new("Perf");
    for row in 1..=ROWS {
        sheet.set_str(&format!("A{row}"), &format!("{row}"));
        for column in 1..=FORMULAS_PER_ROW {
            sheet.set_str(
                &format!("{}{row}", COLUMNS[column]),
                &format!("={}{}+1", COLUMNS[column - 1], row),
            );
        }
    }

    let start = Instant::now();
    let vals = evaluate(&sheet);
    let eval_ms = start.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(vals.len(), ROWS as usize * (FORMULAS_PER_ROW + 1));
    assert_eq!(
        vals.get(&CellRef::parse("U500").unwrap()),
        Some(&Value::Number(520.0))
    );
    eprintln!(
        "PERF evaluate formulas={FORMULA_COUNT} populated_cells={} elapsed_ms={eval_ms:.1}",
        vals.len()
    );
    if std::env::var_os("LOOM_ENFORCE_PERF_BUDGET").is_some() {
        assert!(
            eval_ms < 200.0,
            "10,000-formula recalculation exceeded the 200 ms budget: {eval_ms:.1} ms"
        );
    }

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
    assert_eq!(back.sheets[0].cells.len(), vals.len());
    eprintln!("PERF workbook_from_json time={parse_ms:.1}ms");
}
