//! Numbers-style multi-table freeform canvas model for a worksheet.
//!
//! A single [`Sheet`](crate::Sheet) can host multiple independent, named
//! [`SheetTable`] regions positioned anywhere on the sheet grid. Each table
//! carries its own style preset, title banner visibility, alternating row
//! shading, and header row/column counts while sharing the underlying cell
//! coordinate space so formulas and cross-table references work seamlessly.

use serde::{Deserialize, Serialize};

use crate::CellRef;

/// Visual preset for a canvas [`SheetTable`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TableStylePreset {
    #[default]
    Classic,
    Jade,
    Ocean,
    Coral,
    Charcoal,
}

impl TableStylePreset {
    /// All supported presets in UI picker order.
    pub const ALL: [TableStylePreset; 5] = [
        TableStylePreset::Classic,
        TableStylePreset::Jade,
        TableStylePreset::Ocean,
        TableStylePreset::Coral,
        TableStylePreset::Charcoal,
    ];

    /// Display label for menus and the inspector.
    pub fn as_str(self) -> &'static str {
        match self {
            TableStylePreset::Classic => "Classic",
            TableStylePreset::Jade => "Jade",
            TableStylePreset::Ocean => "Ocean",
            TableStylePreset::Coral => "Coral",
            TableStylePreset::Charcoal => "Charcoal",
        }
    }

    /// Zero-based UI index matching [`Self::ALL`].
    pub fn index(self) -> i32 {
        match self {
            TableStylePreset::Classic => 0,
            TableStylePreset::Jade => 1,
            TableStylePreset::Ocean => 2,
            TableStylePreset::Coral => 3,
            TableStylePreset::Charcoal => 4,
        }
    }

    /// Construct from a zero-based UI index, clamping out-of-range values to `Classic`.
    pub fn from_index(idx: i32) -> Self {
        Self::ALL.get(idx as usize).copied().unwrap_or_default()
    }

    /// Cycle to the next preset in [`Self::ALL`].
    pub fn next(self) -> Self {
        let next_idx = (self.index() + 1) as usize % Self::ALL.len();
        Self::ALL[next_idx]
    }
}

/// An independent named table positioned on a worksheet canvas.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SheetTable {
    pub id: String,
    pub name: String,
    pub start_row: u32,
    pub start_col: u32,
    pub rows: u32,
    pub cols: u32,
    #[serde(default)]
    pub style: TableStylePreset,
    #[serde(default = "default_true")]
    pub show_title: bool,
    #[serde(default = "default_true")]
    pub alternating_rows: bool,
    #[serde(default = "default_one")]
    pub header_rows: u32,
    #[serde(default)]
    pub header_cols: u32,
}

fn default_true() -> bool {
    true
}

fn default_one() -> u32 {
    1
}

impl SheetTable {
    /// Construct a table with sensible Numbers-style defaults.
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        start_row: u32,
        start_col: u32,
        rows: u32,
        cols: u32,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            start_row,
            start_col,
            rows: rows.max(1),
            cols: cols.max(1),
            style: TableStylePreset::Classic,
            show_title: true,
            alternating_rows: true,
            header_rows: 1,
            header_cols: 0,
        }
    }

    /// Inclusive last row index of this table.
    pub fn end_row(&self) -> u32 {
        self.start_row.saturating_add(self.rows.max(1) - 1)
    }

    /// Inclusive last column index of this table.
    pub fn end_col(&self) -> u32 {
        self.start_col.saturating_add(self.cols.max(1) - 1)
    }

    /// True if `(row, col)` falls inside this table's bounds.
    pub fn contains(&self, row: u32, col: u32) -> bool {
        row >= self.start_row
            && row <= self.end_row()
            && col >= self.start_col
            && col <= self.end_col()
    }

    /// True if this table overlaps the rectangle `(start_row, start_col, rows, cols)`.
    pub fn overlaps_rect(&self, start_row: u32, start_col: u32, rows: u32, cols: u32) -> bool {
        let end_row = start_row.saturating_add(rows.max(1) - 1);
        let end_col = start_col.saturating_add(cols.max(1) - 1);
        self.start_row <= end_row
            && start_row <= self.end_row()
            && self.start_col <= end_col
            && start_col <= self.end_col()
    }

    /// True if this table overlaps another table.
    pub fn overlaps(&self, other: &Self) -> bool {
        self.overlaps_rect(other.start_row, other.start_col, other.rows, other.cols)
    }

    /// A1 range label (e.g. `"A1:D5"`) for display in the inspector.
    pub fn range_label(&self) -> String {
        let start = CellRef {
            row: self.start_row,
            col: self.start_col,
        }
        .to_a1();
        let end = CellRef {
            row: self.end_row(),
            col: self.end_col(),
        }
        .to_a1();
        format!("{start}:{end}")
    }
}

/// Generate the next unique `"Table N"` name not already used on the sheet.
pub fn next_table_name(existing: &[SheetTable]) -> String {
    let mut n = 1usize;
    loop {
        let candidate = format!("Table {n}");
        if !existing
            .iter()
            .any(|t| t.name.eq_ignore_ascii_case(&candidate))
        {
            return candidate;
        }
        n += 1;
    }
}

/// Generate the next unique `"table-N"` identifier not already used on the sheet.
pub fn next_table_id(existing: &[SheetTable]) -> String {
    let mut n = 1usize;
    loop {
        let candidate = format!("table-{n}");
        if !existing.iter().any(|t| t.id == candidate) {
            return candidate;
        }
        n += 1;
    }
}

fn rect_is_free(existing: &[SheetTable], row: u32, col: u32, rows: u32, cols: u32) -> bool {
    !existing
        .iter()
        .any(|table| table.overlaps_rect(row, col, rows, cols))
}

/// Find a non-overlapping `(start_row, start_col)` for a new table of size `(rows, cols)`.
///
/// Prefers `(preferred_row, preferred_col)` when free; otherwise tries placing to
/// the right of existing tables with a 1-column gutter, or below existing tables
/// with a 2-row gutter (to leave room for the title banner).
pub fn find_free_placement(
    existing: &[SheetTable],
    preferred_row: u32,
    preferred_col: u32,
    rows: u32,
    cols: u32,
) -> (u32, u32) {
    let rows = rows.max(1);
    let cols = cols.max(1);
    if rect_is_free(existing, preferred_row, preferred_col, rows, cols) {
        return (preferred_row, preferred_col);
    }

    // Try placing to the right of each existing table at its start_row (with a 1-col gap).
    let mut candidates: Vec<(u32, u32)> = Vec::new();
    for table in existing {
        candidates.push((table.start_row, table.end_col().saturating_add(2)));
        candidates.push((table.end_row().saturating_add(3), table.start_col));
    }
    // Also try directly below the lowest table at column 0 / preferred_col.
    if let Some(max_end_row) = existing.iter().map(SheetTable::end_row).max() {
        candidates.push((max_end_row.saturating_add(3), preferred_col));
        candidates.push((max_end_row.saturating_add(3), 0));
    }

    for (row, col) in candidates {
        if rect_is_free(existing, row, col, rows, cols) {
            return (row, col);
        }
    }

    // Fallback: step downward until free.
    let mut row = existing
        .iter()
        .map(SheetTable::end_row)
        .max()
        .map(|r| r.saturating_add(3))
        .unwrap_or(preferred_row);
    while !rect_is_free(existing, row, preferred_col, rows, cols) {
        row = row.saturating_add(rows + 2);
    }
    (row, preferred_col)
}

/// Insert a new non-overlapping [`SheetTable`] into `tables` and return a clone of it.
pub fn insert_table(
    tables: &mut Vec<SheetTable>,
    preferred_row: u32,
    preferred_col: u32,
    rows: u32,
    cols: u32,
) -> SheetTable {
    let (start_row, start_col) =
        find_free_placement(tables, preferred_row, preferred_col, rows, cols);
    let id = next_table_id(tables);
    let name = next_table_name(tables);
    let table = SheetTable::new(id, name, start_row, start_col, rows, cols);
    tables.push(table.clone());
    table
}

/// Return the index of the first table containing `(row, col)`, if any.
pub fn table_at(tables: &[SheetTable], row: u32, col: u32) -> Option<usize> {
    tables.iter().position(|table| table.contains(row, col))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{sheet_from_json, sheet_to_json, Sheet};

    #[test]
    fn non_overlapping_placement_picks_free_regions() {
        let mut tables = Vec::new();
        let first = insert_table(&mut tables, 0, 0, 5, 4);
        assert_eq!((first.start_row, first.start_col), (0, 0));
        assert_eq!(first.name, "Table 1");
        assert_eq!(first.range_label(), "A1:D5");

        // Requesting the same origin must not overlap the first table.
        let second = insert_table(&mut tables, 1, 1, 5, 4);
        assert!(!first.overlaps(&second));
        assert_eq!(second.name, "Table 2");

        let third = insert_table(&mut tables, 0, 0, 5, 4);
        assert!(!first.overlaps(&third));
        assert!(!second.overlaps(&third));
        assert_eq!(third.name, "Table 3");
    }

    #[test]
    fn table_at_finds_containing_table() {
        let mut tables = Vec::new();
        let t1 = insert_table(&mut tables, 0, 0, 4, 3);
        let t2 = insert_table(&mut tables, 10, 5, 3, 3);
        assert_eq!(
            table_at(&tables, t1.start_row + 1, t1.start_col + 1),
            Some(0)
        );
        assert_eq!(table_at(&tables, t2.start_row, t2.start_col), Some(1));
        assert_eq!(table_at(&tables, 7, 0), None);
    }

    #[test]
    fn sheet_tables_persist_and_legacy_json_loads_cleanly() {
        let mut sheet = Sheet::new("Canvas");
        let mut t = insert_table(&mut sheet.tables, 2, 1, 6, 4);
        t.style = TableStylePreset::Jade;
        t.show_title = false;
        t.alternating_rows = false;
        sheet.tables[0] = t.clone();

        let json = sheet_to_json(&sheet);
        let decoded = sheet_from_json(&json).expect("decode sheet with tables");
        assert_eq!(decoded.tables, vec![t]);

        // Legacy JSON without `tables` field must load with an empty `tables` vec.
        let legacy_json = r#"{"name":"Legacy","cells":[]}"#;
        let legacy = sheet_from_json(legacy_json).expect("decode legacy sheet");
        assert!(legacy.tables.is_empty());
    }
}
