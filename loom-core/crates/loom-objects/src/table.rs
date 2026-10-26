//! Shared table object model for Loom documents, presentations, and sheets.

use serde::{Deserialize, Serialize};

/// Version tag for persisted table JSON.
pub const TABLE_SCHEMA_VERSION: u32 = 1;

/// Default column width in points (1 inch = 72 pt).
pub const DEFAULT_COLUMN_WIDTH_PT: f32 = 80.0;

/// Default row height in points.
pub const DEFAULT_ROW_HEIGHT_PT: f32 = 20.0;

/// A formatted table object containing rows, columns, styles, and cells.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Table {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default = "default_style_id")]
    pub style_id: String,
    #[serde(default = "default_true")]
    pub header_row: bool,
    #[serde(default)]
    pub total_row: bool,
    #[serde(default)]
    pub first_column: bool,
    #[serde(default)]
    pub last_column: bool,
    #[serde(default = "default_true")]
    pub banded_rows: bool,
    #[serde(default)]
    pub banded_columns: bool,
    pub columns: Vec<TableColumn>,
    pub rows: Vec<TableRow>,
}

fn default_schema_version() -> u32 {
    TABLE_SCHEMA_VERSION
}

fn default_style_id() -> String {
    "table-style-medium-1".to_string()
}

fn default_true() -> bool {
    true
}

impl Table {
    /// Creates a new table with the given dimensions and default column widths.
    pub fn new(num_rows: usize, num_cols: usize) -> Self {
        let cols = if num_cols == 0 { 1 } else { num_cols };
        let rows_count = if num_rows == 0 { 1 } else { num_rows };

        let columns = (0..cols)
            .map(|_| TableColumn {
                width_pt: DEFAULT_COLUMN_WIDTH_PT,
            })
            .collect();

        let rows = (0..rows_count)
            .map(|_| TableRow {
                height_pt: Some(DEFAULT_ROW_HEIGHT_PT),
                cells: (0..cols).map(|_| TableCell::default()).collect(),
            })
            .collect();

        Self {
            schema_version: TABLE_SCHEMA_VERSION,
            id: format!("tbl-{}", (rand_id() & 0xFFFF_FFFF)),
            name: None,
            style_id: default_style_id(),
            header_row: true,
            total_row: false,
            first_column: false,
            last_column: false,
            banded_rows: true,
            banded_columns: false,
            columns,
            rows,
        }
    }

    /// Number of rows in the table.
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// Number of columns in the table.
    pub fn col_count(&self) -> usize {
        self.columns.len()
    }

    /// Total width of the table in points.
    pub fn total_width_pt(&self) -> f32 {
        self.columns.iter().map(|c| c.width_pt).sum()
    }

    /// References a cell at (row, col).
    pub fn cell(&self, row: usize, col: usize) -> Option<&TableCell> {
        self.rows.get(row).and_then(|r| r.cells.get(col))
    }

    /// Mutably references a cell at (row, col).
    pub fn cell_mut(&mut self, row: usize, col: usize) -> Option<&mut TableCell> {
        self.rows.get_mut(row).and_then(|r| r.cells.get_mut(col))
    }

    /// Returns the text content of a cell at (row, col).
    pub fn cell_text(&self, row: usize, col: usize) -> Option<String> {
        self.cell(row, col).map(|c| c.content.text())
    }

    /// Sets the text of a cell at (row, col) as plain text.
    pub fn set_cell_text(&mut self, row: usize, col: usize, text: impl Into<String>) {
        if let Some(cell) = self.cell_mut(row, col) {
            cell.content = CellContent::plain(text);
        }
    }

    /// Sets rich text runs for a cell at (row, col).
    pub fn set_cell_rich_text(&mut self, row: usize, col: usize, runs: Vec<TextRun>) {
        if let Some(cell) = self.cell_mut(row, col) {
            cell.content = CellContent { runs };
        }
    }

    /// Inserts a new row at the specified index.
    pub fn insert_row(&mut self, index: usize) -> usize {
        let cols = self.col_count();
        let new_row = TableRow {
            height_pt: Some(DEFAULT_ROW_HEIGHT_PT),
            cells: (0..cols).map(|_| TableCell::default()).collect(),
        };
        let insert_idx = index.min(self.rows.len());
        self.rows.insert(insert_idx, new_row);
        self.ensure_grid_consistency();
        insert_idx
    }

    /// Deletes the row at the specified index.
    pub fn delete_row(&mut self, index: usize) -> bool {
        if index >= self.rows.len() || self.rows.len() <= 1 {
            return false;
        }
        self.rows.remove(index);
        self.ensure_grid_consistency();
        true
    }

    /// Inserts a new column at the specified index.
    pub fn insert_column(&mut self, index: usize, width_pt: Option<f32>) -> usize {
        let insert_idx = index.min(self.columns.len());
        let col = TableColumn {
            width_pt: width_pt.unwrap_or(DEFAULT_COLUMN_WIDTH_PT),
        };
        self.columns.insert(insert_idx, col);
        for row in &mut self.rows {
            row.cells.insert(insert_idx, TableCell::default());
        }
        self.ensure_grid_consistency();
        insert_idx
    }

    /// Deletes the column at the specified index.
    pub fn delete_column(&mut self, index: usize) -> bool {
        if index >= self.columns.len() || self.columns.len() <= 1 {
            return false;
        }
        self.columns.remove(index);
        for row in &mut self.rows {
            if index < row.cells.len() {
                row.cells.remove(index);
            }
        }
        self.ensure_grid_consistency();
        true
    }

    /// Merges a rectangular block of cells starting at (start_row, start_col).
    pub fn merge_cells(
        &mut self,
        start_row: usize,
        start_col: usize,
        row_span: usize,
        col_span: usize,
    ) -> Result<(), &'static str> {
        if row_span == 0 || col_span == 0 {
            return Err("Spans must be at least 1");
        }
        if row_span == 1 && col_span == 1 {
            return Ok(());
        }
        let end_row = start_row + row_span;
        let end_col = start_col + col_span;
        if end_row > self.row_count() || end_col > self.col_count() {
            return Err("Merge range exceeds table bounds");
        }

        // Collect text from non-origin cells to append to origin cell if desired
        let mut extra_runs = Vec::new();
        for r in start_row..end_row {
            for c in start_col..end_col {
                if r == start_row && c == start_col {
                    continue;
                }
                if let Some(cell) = self.cell(r, c) {
                    if !cell.content.is_empty() {
                        extra_runs.push(TextRun::plain(" "));
                        extra_runs.extend(cell.content.runs.clone());
                    }
                }
            }
        }

        // Update origin cell
        if let Some(origin) = self.cell_mut(start_row, start_col) {
            origin.row_span = row_span;
            origin.col_span = col_span;
            origin.is_covered = false;
            origin.content.runs.extend(extra_runs);
        }

        // Mark covered cells
        for r in start_row..end_row {
            for c in start_col..end_col {
                if r == start_row && c == start_col {
                    continue;
                }
                if let Some(cell) = self.cell_mut(r, c) {
                    cell.row_span = 1;
                    cell.col_span = 1;
                    cell.is_covered = true;
                    cell.content = CellContent::default();
                }
            }
        }

        Ok(())
    }

    /// Splits a previously merged cell back to 1x1 cells.
    pub fn split_cell(&mut self, row: usize, col: usize) -> Result<(), &'static str> {
        let (r_span, c_span) = match self.cell(row, col) {
            Some(cell) if !cell.is_covered && (cell.row_span > 1 || cell.col_span > 1) => {
                (cell.row_span, cell.col_span)
            }
            _ => return Err("Cell is not a merged origin"),
        };

        let end_row = (row + r_span).min(self.row_count());
        let end_col = (col + c_span).min(self.col_count());

        for r in row..end_row {
            for c in col..end_col {
                if let Some(cell) = self.cell_mut(r, c) {
                    cell.row_span = 1;
                    cell.col_span = 1;
                    cell.is_covered = false;
                }
            }
        }

        Ok(())
    }

    /// Returns the origin cell coordinates and cell reference if (row, col) is covered.
    pub fn effective_cell(&self, row: usize, col: usize) -> Option<(usize, usize, &TableCell)> {
        let cell = self.cell(row, col)?;
        if !cell.is_covered {
            return Some((row, col, cell));
        }
        // Search backwards/upwards for the spanning origin
        for r in (0..=row).rev() {
            for c in (0..=col).rev() {
                if let Some(candidate) = self.cell(r, c) {
                    if !candidate.is_covered
                        && r + candidate.row_span > row
                        && c + candidate.col_span > col
                    {
                        return Some((r, c, candidate));
                    }
                }
            }
        }
        None
    }

    /// Checks if a cell is covered by another cell's span.
    pub fn is_covered(&self, row: usize, col: usize) -> bool {
        self.cell(row, col).map(|c| c.is_covered).unwrap_or(false)
    }

    /// Ensures each row has the expected number of cells matching columns count.
    pub fn ensure_grid_consistency(&mut self) {
        let col_count = self.columns.len();
        for row in &mut self.rows {
            if row.cells.len() < col_count {
                row.cells.resize_with(col_count, TableCell::default);
            } else if row.cells.len() > col_count {
                row.cells.truncate(col_count);
            }
        }
    }

    /// Serializes the table to a JSON string.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Deserializes a table from a JSON string.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

/// Simple pseudo-random u64 generator for unique IDs without heavy rand dependency.
fn rand_id() -> u64 {
    use std::time::SystemTime;
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    (now as u64) ^ 0x517c_c1b7_2722_0a95
}

/// A row in a table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableRow {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height_pt: Option<f32>,
    pub cells: Vec<TableCell>,
}

/// A column specification in a table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableColumn {
    pub width_pt: f32,
}

/// An individual cell in a table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableCell {
    pub content: CellContent,
    #[serde(default = "default_one")]
    pub row_span: usize,
    #[serde(default = "default_one")]
    pub col_span: usize,
    #[serde(default)]
    pub is_covered: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style_override: Option<CellStyleOverride>,
}

fn default_one() -> usize {
    1
}

impl Default for TableCell {
    fn default() -> Self {
        Self {
            content: CellContent::default(),
            row_span: 1,
            col_span: 1,
            is_covered: false,
            style_override: None,
        }
    }
}

/// Cell content consisting of formatted text runs.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CellContent {
    pub runs: Vec<TextRun>,
}

impl CellContent {
    /// Creates cell content with a single plain text run.
    pub fn plain(text: impl Into<String>) -> Self {
        let s = text.into();
        if s.is_empty() {
            Self { runs: Vec::new() }
        } else {
            Self {
                runs: vec![TextRun::plain(s)],
            }
        }
    }

    /// Concatenates all run text into a single String.
    pub fn text(&self) -> String {
        self.runs.iter().map(|r| r.text.as_str()).collect()
    }

    /// Returns true if the cell contains no text.
    pub fn is_empty(&self) -> bool {
        self.runs.is_empty() || self.runs.iter().all(|r| r.text.is_empty())
    }
}

/// A run of text with uniform styling.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextRun {
    pub text: String,
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub italic: bool,
    #[serde(default)]
    pub underline: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub strikethrough: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub superscript: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub subscript: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_size_pt: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_family: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

impl TextRun {
    /// Creates a plain unformatted text run.
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            bold: false,
            italic: false,
            underline: false,
            strikethrough: false,
            superscript: false,
            subscript: false,
            font_size_pt: None,
            font_family: None,
            color: None,
        }
    }

    /// Creates a bold text run.
    pub fn bold(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            bold: true,
            italic: false,
            underline: false,
            strikethrough: false,
            superscript: false,
            subscript: false,
            font_size_pt: None,
            font_family: None,
            color: None,
        }
    }
}

fn is_false(value: &bool) -> bool {
    !value
}

/// Explicit style overrides applied directly to an individual cell.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CellStyleOverride {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub borders: Option<CellBorders>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub horizontal_align: Option<HorizontalAlign>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vertical_align: Option<VerticalAlign>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub padding: Option<CellPadding>,
}

/// Borders configuration for a cell.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CellBorders {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top: Option<Border>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bottom: Option<Border>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left: Option<Border>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right: Option<Border>,
}

/// Specification for a single border edge.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Border {
    #[serde(default)]
    pub style: BorderStyle,
    pub width_pt: f32,
    pub color: String,
}

impl Border {
    /// Creates a solid border of the given width and color.
    pub fn solid(width_pt: f32, color: impl Into<String>) -> Self {
        Self {
            style: BorderStyle::Solid,
            width_pt,
            color: color.into(),
        }
    }
}

/// Border line styles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BorderStyle {
    None,
    #[default]
    Solid,
    Dashed,
    Dotted,
    Double,
}

/// Horizontal text alignment in a cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HorizontalAlign {
    #[default]
    Left,
    Center,
    Right,
    Justify,
}

/// Vertical alignment in a cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VerticalAlign {
    #[default]
    Top,
    Middle,
    Bottom,
}

/// Padding around cell content in points.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CellPadding {
    pub top_pt: f32,
    pub bottom_pt: f32,
    pub left_pt: f32,
    pub right_pt: f32,
}

impl Default for CellPadding {
    fn default() -> Self {
        Self {
            top_pt: 4.0,
            bottom_pt: 4.0,
            left_pt: 6.0,
            right_pt: 6.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_table_creates_requested_dimensions() {
        let table = Table::new(3, 4);
        assert_eq!(table.row_count(), 3);
        assert_eq!(table.col_count(), 4);
        assert_eq!(table.total_width_pt(), 4.0 * DEFAULT_COLUMN_WIDTH_PT);
        assert_eq!(table.schema_version, TABLE_SCHEMA_VERSION);
    }

    #[test]
    fn cell_text_and_set_cell_text() {
        let mut table = Table::new(2, 2);
        assert_eq!(table.cell_text(0, 0), Some(String::new()));
        table.set_cell_text(0, 0, "Header A");
        table.set_cell_text(0, 1, "Header B");
        table.set_cell_text(1, 0, "Value 1");
        assert_eq!(table.cell_text(0, 0).as_deref(), Some("Header A"));
        assert_eq!(table.cell_text(0, 1).as_deref(), Some("Header B"));
        assert_eq!(table.cell_text(1, 0).as_deref(), Some("Value 1"));
    }

    #[test]
    fn insert_and_delete_row() {
        let mut table = Table::new(2, 2);
        table.set_cell_text(0, 0, "R0");
        table.set_cell_text(1, 0, "R1");

        let inserted = table.insert_row(1);
        assert_eq!(inserted, 1);
        assert_eq!(table.row_count(), 3);
        assert_eq!(table.cell_text(0, 0).as_deref(), Some("R0"));
        assert_eq!(table.cell_text(1, 0).as_deref(), Some(""));
        assert_eq!(table.cell_text(2, 0).as_deref(), Some("R1"));

        assert!(table.delete_row(1));
        assert_eq!(table.row_count(), 2);
        assert_eq!(table.cell_text(1, 0).as_deref(), Some("R1"));
    }

    #[test]
    fn insert_and_delete_column() {
        let mut table = Table::new(2, 2);
        table.set_cell_text(0, 0, "C0");
        table.set_cell_text(0, 1, "C1");

        let inserted = table.insert_column(1, Some(90.0));
        assert_eq!(inserted, 1);
        assert_eq!(table.col_count(), 3);
        assert_eq!(table.columns[1].width_pt, 90.0);
        assert_eq!(table.cell_text(0, 0).as_deref(), Some("C0"));
        assert_eq!(table.cell_text(0, 2).as_deref(), Some("C1"));

        assert!(table.delete_column(1));
        assert_eq!(table.col_count(), 2);
        assert_eq!(table.cell_text(0, 1).as_deref(), Some("C1"));
    }

    #[test]
    fn merge_and_split_cells() {
        let mut table = Table::new(3, 3);
        table.set_cell_text(0, 0, "A");
        table.set_cell_text(0, 1, "B");
        table.set_cell_text(1, 0, "C");
        table.set_cell_text(1, 1, "D");

        table.merge_cells(0, 0, 2, 2).expect("merge successful");
        assert_eq!(table.cell(0, 0).unwrap().row_span, 2);
        assert_eq!(table.cell(0, 0).unwrap().col_span, 2);
        assert!(!table.is_covered(0, 0));
        assert!(table.is_covered(0, 1));
        assert!(table.is_covered(1, 0));
        assert!(table.is_covered(1, 1));

        // Effective cell resolves to origin
        let (r, c, _) = table.effective_cell(1, 1).expect("effective cell");
        assert_eq!((r, c), (0, 0));

        // Split restores cells
        table.split_cell(0, 0).expect("split successful");
        assert!(!table.is_covered(0, 1));
        assert!(!table.is_covered(1, 0));
        assert!(!table.is_covered(1, 1));
        assert_eq!(table.cell(0, 0).unwrap().row_span, 1);
    }

    #[test]
    fn json_serialization_round_trip() {
        let mut table = Table::new(2, 2);
        table.set_cell_text(0, 0, "Test");
        table.columns[0].width_pt = 120.0;

        let json = table.to_json().expect("serialize");
        let parsed: Table = Table::from_json(&json).expect("deserialize");

        assert_eq!(table, parsed);
        assert_eq!(parsed.schema_version, TABLE_SCHEMA_VERSION);
    }

    #[test]
    fn text_run_decoration_flags_survive_table_json_round_trip() {
        let mut table = Table::new(1, 1);
        table.set_cell_rich_text(0, 0, vec![TextRun::plain("styled")]);

        let old_json = table.to_json().expect("serialize legacy plain run");
        let old_value: serde_json::Value =
            serde_json::from_str(&old_json).expect("parse legacy table JSON");
        let old_run = &old_value["rows"][0]["cells"][0]["content"]["runs"][0];
        assert!(old_run.get("strikethrough").is_none());
        assert!(old_run.get("superscript").is_none());
        assert!(old_run.get("subscript").is_none());

        let mut decorated: serde_json::Value = old_value;
        let run = &mut decorated["rows"][0]["cells"][0]["content"]["runs"][0];
        run["strikethrough"] = true.into();
        run["superscript"] = true.into();
        run["subscript"] = true.into();

        let loaded = Table::from_json(&decorated.to_string()).expect("load decorated run");
        let serialized = loaded.to_json().expect("serialize decorated run");
        let round_trip: serde_json::Value =
            serde_json::from_str(&serialized).expect("parse round-trip table JSON");
        let round_trip_run = &round_trip["rows"][0]["cells"][0]["content"]["runs"][0];
        assert_eq!(round_trip_run["strikethrough"], true);
        assert_eq!(round_trip_run["superscript"], true);
        assert_eq!(round_trip_run["subscript"], true);
    }
}
