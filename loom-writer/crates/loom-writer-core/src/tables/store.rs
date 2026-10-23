//! Typed table storage: the flat editor text a table block shows, the map from
//! editor offsets to cells, and the `RichBlock` accessors.
//!
//! A table block keeps its cells in `RichBlock::table`. Its `text` is derived
//! from them: cells are joined by a unit separator and rows by a record
//! separator, so the whole table is one editor paragraph and every offset in
//! it belongs to exactly one cell.

use std::ops::Range;
use std::sync::Arc;

use loom_objects::table::Table;

use crate::tables::TABLE_BLOCK_KIND;
use crate::RichBlock;

/// Separates two cells of one row in a table block's editor text (ASCII unit
/// separator). It is one byte, is never a line break, and is never typed.
pub const TABLE_CELL_SEPARATOR: char = '\u{1f}';

/// Separates two rows in a table block's editor text (ASCII record separator).
pub const TABLE_ROW_SEPARATOR: char = '\u{1e}';

/// Whether `c` is one of the two separators of a table's editor text.
pub fn is_table_separator(c: char) -> bool {
    c == TABLE_CELL_SEPARATOR || c == TABLE_ROW_SEPARATOR
}

/// A character as the table's editor text shows it. Line breaks and separators
/// inside a cell become spaces, so a cell never splits its table. The mapping is
/// one character for one character, so byte and character offsets stay aligned
/// with the cell's runs.
pub(crate) fn flat_char(c: char) -> char {
    if c == '\n' || c == '\r' || is_table_separator(c) {
        ' '
    } else {
        c
    }
}

/// The editor text of one cell.
pub(crate) fn cell_flat_text(table: &Table, row: usize, column: usize) -> String {
    table
        .cell(row, column)
        .map(|cell| {
            cell.content
                .runs
                .iter()
                .flat_map(|run| run.text.chars())
                .map(flat_char)
                .collect()
        })
        .unwrap_or_default()
}

/// The editor text of a whole table: cells joined by [`TABLE_CELL_SEPARATOR`],
/// rows joined by [`TABLE_ROW_SEPARATOR`]. Covered (merged) cells are empty
/// slots, so the grid stays rectangular.
pub fn flat_table_text(table: &Table) -> String {
    let mut out = String::new();
    for row in 0..table.row_count() {
        if row > 0 {
            out.push(TABLE_ROW_SEPARATOR);
        }
        for column in 0..table.col_count() {
            if column > 0 {
                out.push(TABLE_CELL_SEPARATOR);
            }
            out.push_str(&cell_flat_text(table, row, column));
        }
    }
    out
}

/// Maps editor offsets inside a table block to cells. Each cell owns the byte
/// range of its text, and the offset just after the text (where its separator
/// sits) belongs to that cell too. The next cell starts one byte later, so every
/// offset from zero to [`TableTextMap::len`] belongs to exactly one cell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableTextMap {
    columns: usize,
    /// `(start, end)` of every cell in row-major order; `end` is exclusive.
    cells: Vec<(usize, usize)>,
}

impl TableTextMap {
    /// Builds the map for a table's flat text, in O(cells).
    pub fn new(table: &Table) -> Self {
        let columns = table.col_count();
        let mut cells = Vec::with_capacity(table.row_count() * columns);
        let mut position = 0usize;
        for row in 0..table.row_count() {
            for column in 0..columns {
                let len = cell_flat_text(table, row, column).len();
                cells.push((position, position + len));
                position += len + 1;
            }
        }
        Self { columns, cells }
    }

    /// The length of the table's flat text.
    pub fn len(&self) -> usize {
        self.cells.last().map_or(0, |&(_, end)| end)
    }

    /// Whether the table has no cells.
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    /// The `(row, column)` that owns `offset`, or `None` past the table's end.
    pub fn cell_at(&self, offset: usize) -> Option<(usize, usize)> {
        let index = self
            .cells
            .partition_point(|&(start, _)| start <= offset)
            .checked_sub(1)?;
        let (_, end) = self.cells[index];
        (offset <= end).then(|| (index / self.columns, index % self.columns))
    }

    /// The flat-text byte range of one cell's text, end exclusive.
    pub fn cell_range(&self, row: usize, column: usize) -> Option<Range<usize>> {
        if column >= self.columns {
            return None;
        }
        self.cells
            .get(row * self.columns + column)
            .map(|&(start, end)| start..end)
    }
}

impl RichBlock {
    /// A table block holding `table`. Its text is the table's flat text.
    pub fn table_block(id: u64, table: Table) -> Self {
        let mut block = RichBlock::new(id, TABLE_BLOCK_KIND, "");
        block.set_table(table);
        block
    }

    /// Replaces the table of a block, keeping its id and paragraph style. The
    /// block's kind and text follow the table.
    pub fn set_table(&mut self, table: Table) {
        self.kind = TABLE_BLOCK_KIND.to_string();
        self.text = crate::Text::from_str(&flat_table_text(&table));
        self.table = Some(Arc::new(table));
    }

    /// The typed table of a table block; `None` for every other block.
    pub fn typed_table(&self) -> Option<&Table> {
        if self.kind == TABLE_BLOCK_KIND {
            self.table.as_deref()
        } else {
            None
        }
    }
}

/// The byte range of each row of a table block's editor text, the separators
/// between rows excluded. A table is laid out one line per row.
pub(crate) fn table_row_spans(text: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut start = 0usize;
    for (end, _) in text.match_indices(TABLE_ROW_SEPARATOR) {
        spans.push((start, end));
        start = end + TABLE_ROW_SEPARATOR.len_utf8();
    }
    spans.push((start, text.len()));
    spans
}
