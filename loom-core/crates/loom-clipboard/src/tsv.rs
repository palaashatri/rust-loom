//! TSV serialization and parsing for table clipboard operations.

use loom_objects::{Table, TableColumn, TableRow};

/// Converts a `Table` to tab-separated text.
pub fn table_to_tsv(table: &Table) -> String {
    let mut out = String::new();
    for (r, row) in table.rows.iter().enumerate() {
        for (c, cell) in row.cells.iter().enumerate() {
            if c > 0 {
                out.push('\t');
            }
            let cell_text = cell.content.text().replace(['\t', '\r', '\n'], " ");
            out.push_str(&cell_text);
        }
        if r + 1 < table.rows.len() {
            out.push('\n');
        }
    }
    out
}

/// Parses tab-separated text into a `Table`.
pub fn tsv_to_table(tsv: &str) -> Table {
    let raw_rows: Vec<Vec<String>> = tsv
        .lines()
        .map(|line| {
            line.trim_end_matches('\r')
                .split('\t')
                .map(|cell| cell.trim().to_string())
                .collect()
        })
        .filter(|row: &Vec<String>| !row.is_empty())
        .collect();

    if raw_rows.is_empty() {
        return Table::new(1, 1);
    }

    let cols = raw_rows.iter().map(|r| r.len()).max().unwrap_or(1).max(1);
    let rows_count = raw_rows.len();

    let mut table = Table::new(rows_count, cols);
    table.columns = (0..cols).map(|_| TableColumn { width_pt: 80.0 }).collect();

    table.rows = raw_rows
        .into_iter()
        .map(|cells| {
            let mut padded = cells;
            if padded.len() < cols {
                padded.resize(cols, String::new());
            }
            TableRow {
                height_pt: Some(20.0),
                cells: padded
                    .into_iter()
                    .map(|text| loom_objects::TableCell {
                        content: loom_objects::CellContent::plain(text),
                        ..Default::default()
                    })
                    .collect(),
            }
        })
        .collect();

    table
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tsv_round_trip() {
        let mut table = Table::new(2, 3);
        table.set_cell_text(0, 0, "Col1");
        table.set_cell_text(0, 1, "Col2");
        table.set_cell_text(0, 2, "Col3");
        table.set_cell_text(1, 0, "Val1");
        table.set_cell_text(1, 1, "Val2");
        table.set_cell_text(1, 2, "Val3");

        let tsv = table_to_tsv(&table);
        assert_eq!(tsv, "Col1\tCol2\tCol3\nVal1\tVal2\tVal3");

        let parsed = tsv_to_table(&tsv);
        assert_eq!(parsed.row_count(), 2);
        assert_eq!(parsed.col_count(), 3);
        assert_eq!(parsed.cell_text(0, 1).as_deref(), Some("Col2"));
        assert_eq!(parsed.cell_text(1, 2).as_deref(), Some("Val3"));
    }
}
