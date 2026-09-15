//! Interoperability helpers that extend the compact legacy APIs.

use crate::{parse_csv_records, sniff_csv_dialect, Cell, CellRef, Sheet};

/// Export one sheet with raw cell text, including leading `=` formula text.
/// This is the formula-preserving CSV mode; values-only export remains
/// available through [`crate::to_csv_with_values`].
pub fn to_csv_with_formulas(sheet: &Sheet) -> String {
    let Some((_, _, max_col, max_row)) = sheet.used_range() else {
        return String::new();
    };
    let mut output = String::new();
    for row in 0..=max_row {
        let fields = (0..=max_col)
            .map(|col| {
                let cell = sheet.cells.get(&CellRef { row, col });
                csv_escape(cell.map(|cell| cell.raw.as_str()).unwrap_or(""))
            })
            .collect::<Vec<_>>();
        output.push_str(&fields.join(","));
        output.push('\n');
    }
    output
}

/// Import CSV records with an explicit delimiter while preserving raw text.
/// Formula cells remain formulas because their leading `=` is not evaluated
/// during import.
pub fn from_csv_with_dialect(name: &str, csv: &str, delimiter: char) -> Sheet {
    let mut sheet = Sheet::new(name);
    for (row, fields) in parse_csv_records(csv, delimiter).iter().enumerate() {
        for (col, raw) in fields.iter().enumerate() {
            sheet.cells.insert(
                CellRef {
                    row: row as u32,
                    col: col as u32,
                },
                Cell {
                    raw: (*raw).clone(),
                },
            );
        }
    }
    sheet
}

/// Import CSV after sniffing comma, semicolon, tab, or pipe dialects.
/// Empty input keeps the legacy empty-sheet behavior.
pub fn from_csv_sniffed(name: &str, csv: &str) -> Sheet {
    let delimiter = sniff_csv_dialect(csv)
        .map(|dialect| dialect.delimiter)
        .unwrap_or(',');
    from_csv_with_dialect(name, csv, delimiter)
}

fn csv_escape(value: &str) -> String {
    if value
        .chars()
        .any(|ch| matches!(ch, ',' | '"' | '\n' | '\r'))
    {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{from_csv_with_dialect, CellRef, Sheet};

    #[test]
    fn formula_csv_preserves_raw_formulas_and_sniffs_delimiters() {
        let mut sheet = Sheet::new("Formula CSV");
        sheet.set_str("A1", "Item");
        sheet.set_str("B1", "Amount");
        sheet.set_str("A2", "Total, net");
        sheet.set_str("B2", "=SUM(B3:B4)");

        let csv = to_csv_with_formulas(&sheet);
        assert!(csv.contains("=SUM(B3:B4)"));
        assert!(csv.contains("\"Total, net\""));

        let imported = from_csv_with_dialect("Imported", "Name;Amount\nAlpha;=A2*2\n", ';');
        assert_eq!(imported.raw(CellRef { row: 1, col: 1 }), Some("=A2*2"));
    }
}
