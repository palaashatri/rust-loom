//! Interoperability helpers that extend the compact legacy APIs.

use std::collections::BTreeMap;

use crate::{parse_csv_records, sniff_csv_dialect, Cell, CellRef, Sheet};

/// Shared-formula masters for one worksheet, keyed by OOXML's `si` value.
///
/// OOXML stores the formula text once on a shared-formula master and leaves
/// member cells with an empty `<f/>`. The member formula is the master's text
/// translated by the member's row/column offset. The index is scoped to one
/// worksheet by the XLSX reader, so the worksheet identity and shared ID form
/// the lookup boundary together.
#[derive(Debug, Default)]
pub struct SharedFormulaIndex {
    masters: BTreeMap<u32, (CellRef, String)>,
}

impl SharedFormulaIndex {
    /// Register a shared-formula master. Later registrations replace a
    /// duplicate ID, which is invalid OOXML and is rejected by the caller's
    /// duplicate-cell validation before this index is used.
    pub fn insert_master(&mut self, shared_id: u32, cell: CellRef, formula: &str) {
        let formula = if formula.starts_with('=') {
            formula.to_string()
        } else {
            format!("={formula}")
        };
        self.masters.insert(shared_id, (cell, formula));
    }

    /// Translate a shared-formula master to a member cell, preserving
    /// absolute and mixed references, ranges, quoted sheet names, and string
    /// literals through the existing formula reference shifter.
    pub fn resolve(&self, shared_id: u32, member: CellRef) -> Option<String> {
        let (master, formula) = self.masters.get(&shared_id)?;
        let delta_cols = i64::from(member.col) - i64::from(master.col);
        let delta_rows = i64::from(member.row) - i64::from(master.row);
        let delta_cols = i32::try_from(delta_cols).ok()?;
        let delta_rows = i32::try_from(delta_rows).ok()?;
        Some(crate::refs::shift_formula_references(
            formula, delta_cols, delta_rows,
        ))
    }
}

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
    fn shared_formula_index_translates_relative_mixed_and_quoted_refs() {
        let mut index = SharedFormulaIndex::default();
        index.insert_master(
            4,
            CellRef::parse("B1").unwrap(),
            "A1+$A1+B$2+$C$3+'My Sheet'!D4",
        );
        assert_eq!(
            index.resolve(4, CellRef::parse("B2").unwrap()).as_deref(),
            Some("=A2+$A2+B$2+$C$3+'My Sheet'!D5")
        );
        assert_eq!(index.resolve(99, CellRef::parse("B2").unwrap()), None);
    }

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
