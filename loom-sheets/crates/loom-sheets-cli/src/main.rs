//! Loom Sheets command-line interface (headless).
//!
//! Evaluates workbook fixtures and exports CSV. Used by Docker visual-QA and
//! CI to exercise the formula engine without a display.

use loom_sheets_core::{
    evaluate, from_csv, to_csv, CalculationCache, CellRange, CellRef, Sheet, SheetModel,
};
use std::collections::BTreeMap;

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: loom-sheets <command> [args]");
        eprintln!("  demo (prints an evaluated example)");
        eprintln!("  eval <csv-file> (evaluates a CSV as a sheet and prints values)");
        eprintln!("  to-csv <csv-file> <output.csv> (normalizes a CSV through the engine)");
        eprintln!("  create <file.loomsheet> <name>");
        eprintln!("  recalc <csv-file> <cell> <raw-value>");
        eprintln!("  sort <csv-file> <range> <column-offset> <asc|desc>");
        std::process::exit(2);
    }
    match args[1].as_str() {
        "demo" => cmd_demo(),
        "eval" => cmd_eval(args.get(2).ok_or("eval requires <csv-file>")?),
        "to-csv" => cmd_tocsv(
            args.get(2).ok_or("to-csv requires <csv-file>")?,
            args.get(3).ok_or("to-csv requires <output.csv>")?,
        ),
        "create" => cmd_create(
            args.get(2).ok_or("create requires <file.loomsheet>")?,
            args.get(3).ok_or("create requires <name>")?,
        ),
        "recalc" => cmd_recalc(
            args.get(2).ok_or("recalc requires <csv-file>")?,
            args.get(3).ok_or("recalc requires <cell>")?,
            args.get(4).ok_or("recalc requires <raw-value>")?,
        ),
        "sort" => cmd_sort(
            args.get(2).ok_or("sort requires <csv-file>")?,
            args.get(3).ok_or("sort requires <range>")?,
            args.get(4).ok_or("sort requires <column-offset>")?,
            args.get(5).ok_or("sort requires <asc|desc>")?,
        ),
        other => Err(format!("unknown command: {other}")),
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

fn cmd_create(path: &str, name: &str) -> Result<(), String> {
    use loom_package::manifest::{
        Checksum, Manifest, ManifestEntry, MimeType, PackageKind, SchemaVersion,
    };
    use loom_package::zip::{self, PackageArchive};

    let mut sheet = Sheet::new(name);
    sheet.set_str("A1", "Category");
    sheet.set_str("B1", "Amount");
    sheet.set_str("A2", "Development");
    sheet.set_str("B2", "15000");
    sheet.set_str("A3", "Design");
    sheet.set_str("B3", "8000");
    sheet.set_str("A4", "QA & Testing");
    sheet.set_str("B4", "5000");
    sheet.set_str("A5", "Total");
    sheet.set_str("B5", "=SUM(B2:B4)");

    let content_json = loom_sheets_core::sheet_to_json(&sheet);
    let mut arch = PackageArchive::new();
    arch.add("content/sheet.json", content_json.clone().into_bytes())
        .map_err(|e| e.to_string())?;

    let mime = MimeType::parse("application/vnd.loom.sheet-content")
        .map_err(|e| format!("invalid built-in sheet MIME type: {e}"))?;
    let manifest = Manifest {
        schema: SchemaVersion::CURRENT,
        kind: PackageKind::Sheets,
        id: "sample-sheet-1".into(),
        title: name.to_string(),
        app_version: "0.1.0".into(),
        entries: vec![ManifestEntry {
            path: "content/sheet.json".into(),
            mime,
            size: content_json.len() as u64,
            sha256: Checksum::from_bytes(zip::sha256(content_json.as_bytes())),
        }],
    };
    let manifest_str = loom_package::manifest::json::write(&manifest);
    arch.add("manifest.json", manifest_str.into_bytes())
        .map_err(|e| e.to_string())?;

    let bytes = arch.to_bytes().map_err(|e| e.to_string())?;
    std::fs::write(path, bytes).map_err(|e| e.to_string())?;
    println!("wrote {path}");
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
    for ((row, col), value) in rows {
        println!("{}: {}", CellRef { row, col }.to_a1(), value);
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

fn cmd_recalc(path: &str, cell: &str, raw: &str) -> Result<(), String> {
    let csv = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    let mut sheet = from_csv("input", &csv);
    let changed = CellRef::parse(cell).ok_or("invalid A1 cell reference")?;
    let mut cache = CalculationCache::default();
    cache.rebuild(&sheet);
    sheet.set_raw(changed, raw);
    let affected = cache.recalculate(&sheet, &[changed]);
    let mut cells = affected.into_iter().collect::<Vec<_>>();
    cells.sort_by_key(|cell| (cell.row, cell.col));
    println!("affected: {}", cells.len());
    for cell in cells {
        let value = cache
            .values
            .get(&cell)
            .cloned()
            .unwrap_or(loom_sheets_core::Value::Empty);
        println!("{}: {}", cell.to_a1(), value.display());
    }
    Ok(())
}

fn cmd_sort(path: &str, range: &str, column_offset: &str, order: &str) -> Result<(), String> {
    let csv = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    let sheet = from_csv("input", &csv);
    let range = CellRange::parse(range).ok_or("invalid A1 range")?;
    let column_offset = column_offset
        .parse::<u32>()
        .map_err(|_| "column offset must be an integer".to_string())?;
    let ascending = match order {
        "asc" => true,
        "desc" => false,
        _ => return Err("order must be asc or desc".into()),
    };
    let mut model = SheetModel::new(sheet);
    model.sort_rows(range, column_offset, ascending)?;
    print!("{}", to_csv(&model.sheet));
    Ok(())
}
