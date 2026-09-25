//! Interoperability helpers that extend the compact legacy APIs.

use std::collections::BTreeMap;

use crate::{evaluate, Cell, CellRef, Sheet, Value};

/// Export a sheet to CSV.
pub fn to_csv(sheet: &Sheet) -> String {
    to_csv_with_values(sheet, &evaluate(sheet))
}

/// Export a sheet to CSV from precomputed values (e.g. workbook-resolved so
/// cross-sheet references export their displayed values, not `#REF!`).
pub fn to_csv_with_values(
    sheet: &Sheet,
    vals: &std::collections::HashMap<CellRef, Value>,
) -> String {
    let mut max_row = 0u32;
    let mut max_col = 0u32;
    for r in sheet.cells.keys() {
        max_row = max_row.max(r.row);
        max_col = max_col.max(r.col);
    }
    let mut out = String::new();
    for row in 0..=max_row {
        let mut line = Vec::new();
        for col in 0..=max_col {
            let cr = CellRef { row, col };
            let v = vals.get(&cr).cloned().unwrap_or(Value::Empty);
            line.push(csv_escape_values(&v.display()));
        }
        out.push_str(&line.join(","));
        out.push('\n');
    }
    out
}

fn csv_escape_values(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Robust RFC 4180 CSV parser supporting multiline quoted fields and configurable delimiters.
pub fn parse_csv_records(csv: &str, delimiter: char) -> Vec<Vec<String>> {
    let mut records = Vec::new();
    let mut current_row = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut chars = csv.chars().peekable();

    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    field.push('"');
                    chars.next();
                } else {
                    in_quotes = false;
                }
            } else {
                field.push(c);
            }
        } else if c == '"' {
            in_quotes = true;
        } else if c == delimiter {
            current_row.push(field.clone());
            field.clear();
        } else if c == '\r' {
            if chars.peek() == Some(&'\n') {
                chars.next();
            }
            current_row.push(field.clone());
            field.clear();
            records.push(current_row.clone());
            current_row.clear();
        } else if c == '\n' {
            current_row.push(field.clone());
            field.clear();
            records.push(current_row.clone());
            current_row.clear();
        } else {
            field.push(c);
        }
    }

    if !field.is_empty() || !current_row.is_empty() {
        current_row.push(field);
        records.push(current_row);
    }

    records
}

/// A detected CSV dialect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CsvDialect {
    pub delimiter: char,
    /// True when fields were observed wrapped in double quotes.
    pub quoted: bool,
}

/// Sniffs the CSV dialect used by `sample`.
///
/// The delimiter is detected among `','`, `';'`, `'\t'`, and `'|'` by counting,
/// for each candidate, its occurrences outside double quotes in the first ten
/// non-empty lines. A candidate is *consistent* when every sampled line contains
/// the same number of occurrences, and the winner is the consistent candidate
/// with the highest count.
///
/// Exact tie-breaking: candidates are visited in the fixed order `','`, `';'`,
/// `'\t'`, `'|'`, and a candidate only replaces the current winner on a
/// strictly greater count, so ties always prefer the earlier candidate in that
/// list. When every candidate scores zero (or none is consistent) the dialect
/// defaults to `','`.
///
/// `quoted` is true when any sampled field starts and ends with `'"'` after
/// splitting on the detected delimiter. Empty input (no non-empty lines)
/// returns `Err`.
pub fn sniff_csv_dialect(sample: &str) -> Result<CsvDialect, String> {
    let mut lines = Vec::new();
    for line in sample.lines() {
        if !line.trim().is_empty() {
            lines.push(line);
            if lines.len() >= CSV_SNIFF_SAMPLE_LINES {
                break;
            }
        }
    }
    if lines.is_empty() {
        return Err("cannot sniff CSV dialect from empty input".to_string());
    }

    let csv_delimiter_candidates = [',', ';', '\t', '|'];
    let mut best_count = 0usize;
    let mut delimiter = ',';
    for candidate in csv_delimiter_candidates {
        let counts: Vec<usize> = lines
            .iter()
            .map(|line| {
                csv_fields_outside_quotes(line, candidate)
                    .len()
                    .saturating_sub(1)
            })
            .collect();
        let first = counts[0];
        let consistent = counts.iter().all(|&count| count == first);
        if consistent && first > best_count {
            best_count = first;
            delimiter = candidate;
        }
    }

    let quoted = lines.iter().any(|line| {
        csv_fields_outside_quotes(line, delimiter)
            .into_iter()
            .any(|field| field.len() > 1 && field.starts_with('"') && field.ends_with('"'))
    });

    Ok(CsvDialect { delimiter, quoted })
}

const CSV_SNIFF_SAMPLE_LINES: usize = 10;

/// Split `line` on `delimiter`, treating double-quoted spans as opaque, mirroring
/// the quoting conventions of [`parse_csv_records`]. Delimiters inside quotes are
/// not field boundaries, so the number of boundaries is `fields.len() - 1`.
fn csv_fields_outside_quotes(line: &str, delimiter: char) -> Vec<&str> {
    let mut fields = Vec::new();
    let mut field_start = 0usize;
    let mut in_quotes = false;
    let mut chars = line.char_indices().peekable();
    while let Some((idx, c)) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek().map(|&(_, next)| next) == Some('"') {
                    chars.next();
                } else {
                    in_quotes = false;
                }
            }
        } else if c == '"' {
            in_quotes = true;
        } else if c == delimiter {
            fields.push(&line[field_start..idx]);
            field_start = idx + c.len_utf8();
        }
    }
    fields.push(&line[field_start..]);
    fields
}

/// Import a CSV into a sheet.
pub fn from_csv(name: &str, csv: &str) -> Sheet {
    let mut sheet = Sheet::new(name);
    let records = parse_csv_records(csv, ',');
    for (row, fields) in records.iter().enumerate() {
        for (col, f) in fields.iter().enumerate() {
            let cr = CellRef {
                row: row as u32,
                col: col as u32,
            };
            sheet.cells.insert(cr, Cell { raw: f.clone() });
        }
    }
    sheet
}

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
