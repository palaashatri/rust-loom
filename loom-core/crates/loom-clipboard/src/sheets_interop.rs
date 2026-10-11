//! Sheets cell range interoperability with shared Table objects.

use loom_objects::table::{CellStyleOverride, HorizontalAlign, Table, TextRun};

/// Data representation of a cell in Sheets for clipboard operations.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SheetsCellData {
    pub value: String,
    pub formatted_text: String,
    pub background_color: Option<String>,
    pub bold: bool,
    pub italic: bool,
    pub text_color: Option<String>,
    pub alignment: Option<HorizontalAlign>,
    pub width_pt: f32,
}

/// Converts a 2D grid of Sheets cells into a styled `Table` for pasting into Writer or Present.
pub fn sheets_range_to_table(grid: &[Vec<SheetsCellData>]) -> Table {
    let rows_count = grid.len();
    if rows_count == 0 {
        return Table::new(1, 1);
    }
    let cols_count = grid.iter().map(|r| r.len()).max().unwrap_or(1).max(1);

    let mut table = Table::new(rows_count, cols_count);

    // Set column widths from first row or defaults
    if let Some(first_row) = grid.first() {
        for (c, cell_data) in first_row.iter().enumerate() {
            if c < table.columns.len() && cell_data.width_pt > 0.0 {
                table.columns[c].width_pt = cell_data.width_pt;
            }
        }
    }

    for (r, row) in grid.iter().enumerate() {
        for (c, cell_data) in row.iter().enumerate() {
            if c >= cols_count {
                break;
            }

            let text = if !cell_data.formatted_text.is_empty() {
                &cell_data.formatted_text
            } else {
                &cell_data.value
            };

            let mut run = TextRun::plain(text);
            run.bold = cell_data.bold;
            run.italic = cell_data.italic;
            run.color = cell_data.text_color.clone();

            let style_override = CellStyleOverride {
                background_color: cell_data.background_color.clone(),
                horizontal_align: cell_data.alignment,
                ..Default::default()
            };

            if let Some(cell) = table.cell_mut(r, c) {
                cell.content.runs = vec![run];
                cell.style_override = Some(style_override);
            }
        }
    }

    table
}

/// Converts a `Table` back into a 2D grid of `SheetsCellData` for pasting into Sheets.
pub fn table_to_sheets_range(table: &Table) -> Vec<Vec<SheetsCellData>> {
    let mut grid = Vec::with_capacity(table.row_count());

    for row in &table.rows {
        let mut row_data = Vec::with_capacity(table.col_count());
        for (c, cell) in row.cells.iter().enumerate() {
            let width_pt = table.columns.get(c).map(|col| col.width_pt).unwrap_or(80.0);
            let text = cell.content.text();
            let first_run = cell.content.runs.first();

            let bg = cell
                .style_override
                .as_ref()
                .and_then(|s| s.background_color.clone());
            let align = cell
                .style_override
                .as_ref()
                .and_then(|s| s.horizontal_align);

            row_data.push(SheetsCellData {
                value: text.clone(),
                formatted_text: text,
                background_color: bg,
                bold: first_run.is_some_and(|r| r.bold),
                italic: first_run.is_some_and(|r| r.italic),
                text_color: first_run.and_then(|r| r.color.clone()),
                alignment: align,
                width_pt,
            });
        }
        grid.push(row_data);
    }

    grid
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sheets_range_converts_to_table_and_back() {
        let grid = vec![vec![
            SheetsCellData {
                value: "Header A".to_string(),
                formatted_text: "Header A".to_string(),
                bold: true,
                background_color: Some("#2B579A".to_string()),
                width_pt: 120.0,
                ..Default::default()
            },
            SheetsCellData {
                value: "1000".to_string(),
                formatted_text: "$1,000.00".to_string(),
                alignment: Some(HorizontalAlign::Right),
                width_pt: 90.0,
                ..Default::default()
            },
        ]];

        let table = sheets_range_to_table(&grid);
        assert_eq!(table.row_count(), 1);
        assert_eq!(table.col_count(), 2);
        assert_eq!(table.columns[0].width_pt, 120.0);
        assert_eq!(table.columns[1].width_pt, 90.0);
        assert_eq!(table.cell_text(0, 0).as_deref(), Some("Header A"));
        assert_eq!(table.cell_text(0, 1).as_deref(), Some("$1,000.00"));

        let round_trip = table_to_sheets_range(&table);
        assert_eq!(round_trip[0][0].value, "Header A");
        assert_eq!(round_trip[0][1].value, "$1,000.00");
        assert_eq!(round_trip[0][1].alignment, Some(HorizontalAlign::Right));
    }
}
