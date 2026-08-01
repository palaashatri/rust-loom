//! Loom Sheets command-line interface (headless).
//!
//! Evaluates workbook fixtures and exports CSV. Used by Docker visual-QA and
//! CI to exercise the formula engine without a display.

use loom_sheets_core::{evaluate, from_csv, to_csv, CellRef, Sheet};
use std::collections::BTreeMap;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: loom-sheets <command> [args]");
        eprintln!("  demo (prints an evaluated example)");
        eprintln!("  eval <csv-file> (evaluates a CSV as a sheet and prints values)");
        eprintln!("  to-csv <csv-file> (normalizes a CSV through the engine)");
        std::process::exit(2);
    }
    let result = match args[1].as_str() {
        "demo" => cmd_demo(),
        "eval" => cmd_eval(&args[2]),
        "to-csv" => cmd_tocsv(&args[2], &args[3]),
        other => {
            eprintln!("unknown command: {other}");
            std::process::exit(2);
        }
    };
    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn cmd_demo() -> Result<(), String> {
    let mut sheet = Sheet::new("Budget");
    sheet.set_str("A1", "Item");
    sheet.set_str("A2", "Rent");
    sheet.set_str("A3", "Food");
    sheet.set_str("A4", "Transport");
    sheet.set_str("B2", "1200");
    sheet.set_str("B3", "450");
    sheet.set_str("B4", "150");
    sheet.set_str("B5", "=SUM(B2:B4)");
    sheet.set_str("B6", "=AVERAGE(B2:B4)");
    let vals = evaluate(&sheet);
    // Print sheet as BTreeMap for deterministic output.
    let mut rows: BTreeMap<(u32, u32), String> = BTreeMap::new();
    for (r, c) in &sheet.cells {
        let v = vals
            .get(r)
            .cloned()
            .unwrap_or(loom_sheets_core::Value::Empty);
        rows.insert((r.row, r.col), format!("{} = {}", c.raw, v.display()));
    }
    for ((row, col), line) in rows {
        println!("{}: {}", CellRef { row, col }.to_a1(), line);
    }
    Ok(())
}

fn cmd_eval(path: &str) -> Result<(), String> {
    let csv = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let sheet = from_csv("input", &csv);
    let vals = evaluate(&sheet);
    let mut rows: BTreeMap<(u32, u32), String> = BTreeMap::new();
    for r in sheet.cells.keys() {
        let v = vals
            .get(r)
            .cloned()
            .unwrap_or(loom_sheets_core::Value::Empty);
        rows.insert((r.row, r.col), v.display());
    }
    for ((row, col), v) in rows {
        println!("{}{} ", CellRef { row, col }.to_a1(), v);
    }
    Ok(())
}

fn cmd_tocsv(path: &str, out: &str) -> Result<(), String> {
    let csv = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let sheet = from_csv("input", &csv);
    let result = to_csv(&sheet);
    std::fs::write(out, result).map_err(|e| e.to_string())?;
    println!("wrote {out}");
    Ok(())
}
