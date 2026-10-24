//! Table operations on `WriterDocument`, over typed table blocks.
//!
//! Every operation reads the block's [`Table`], changes it, and stores it back
//! with `RichBlock::set_table`, which rewrites the block's flat text. Carets are
//! placed from the same [`TableTextMap`] the editor uses, so they always land on
//! the cell the operation names.

use std::ops::Range;

use loom_objects::table::{
    HorizontalAlign, Table, TableCell, TableRow, TextRun, DEFAULT_ROW_HEIGHT_PT,
};

use super::editing::{apply_flat_text, cell_part, restyle_runs, runs_cover};
use super::store::TableTextMap;
use super::{block_insert_index, TableArrowDirection, TableCellLocation, TABLE_BLOCK_KIND};
use crate::{RichBlock, TextSelection, WriterDocument, WriterTable};

/// A new row like `template`: it keeps the template's cell alignment and fills
/// (`style_override`), and none of its text.
fn row_like(table: &Table, template: usize) -> TableRow {
    let cells = (0..table.col_count())
        .map(|column| TableCell {
            style_override: table
                .cell(template, column)
                .and_then(|cell| cell.style_override.clone()),
            ..Default::default()
        })
        .collect();
    TableRow {
        height_pt: Some(DEFAULT_ROW_HEIGHT_PT),
        cells,
    }
}

impl WriterDocument {
    /// The block index of a table block with this id.
    fn table_index(&self, block_id: u64) -> Option<usize> {
        self.blocks
            .iter()
            .position(|block| block.id == block_id && block.kind == TABLE_BLOCK_KIND)
    }

    /// Editor offset where block `index` starts.
    fn table_block_start(&self, index: usize) -> usize {
        self.blocks
            .iter()
            .take(index)
            .map(|block| block.text.len_bytes() + 1)
            .sum::<usize>()
    }

    /// The next block id that no block uses.
    fn next_table_block_id(&self) -> u64 {
        let mut id = self.blocks.iter().map(|block| block.id).max().unwrap_or(0) + 1;
        while self.blocks.iter().any(|block| block.id == id) {
            id += 1;
        }
        id
    }

    /// Stores `table` in block `index` and puts the caret at `(row, column)`,
    /// returning its offset (or the block start when that cell is missing).
    fn store_table_and_place_caret(
        &mut self,
        index: usize,
        table: Table,
        row: usize,
        column: usize,
    ) -> usize {
        self.blocks[index].set_table(table);
        let start = self.table_block_start(index);
        let caret = self.blocks[index]
            .typed_table()
            .and_then(|table| TableTextMap::new(table).cell_range(row, column))
            .map_or(start, |range| start + range.start);
        self.set_selection(TextSelection::caret(caret));
        caret
    }

    /// Inserts a new empty table block after the block containing `offset`
    /// (or appends it), returning its id. The caret moves to its first cell.
    pub fn insert_table_block(
        &mut self,
        offset: usize,
        rows: usize,
        columns: usize,
    ) -> Result<u64, String> {
        if rows == 0 || columns == 0 {
            return Err("table geometry must be non-empty".into());
        }
        let id = self.next_table_block_id();
        let mut table = Table::new(rows, columns);
        table.id = format!("table-{id}");
        self.insert_table_block_at(offset, RichBlock::table_block(id, table));
        Ok(id)
    }

    /// Inserts a table block from a `loom_objects::Table`, keeping its widths,
    /// fills and styles.
    pub fn insert_table_object(&mut self, offset: usize, table: &Table) -> Result<u64, String> {
        if table.row_count() == 0 || table.col_count() == 0 {
            return Err("table geometry must be non-empty".into());
        }
        let id = self.next_table_block_id();
        let mut table = table.clone();
        table.ensure_grid_consistency();
        self.insert_table_block_at(offset, RichBlock::table_block(id, table));
        Ok(id)
    }

    fn insert_table_block_at(&mut self, offset: usize, block: RichBlock) {
        let insert_at = block_insert_index(self, offset);
        self.blocks.insert(insert_at, block);
        let caret = self.table_block_start(insert_at);
        self.set_selection(TextSelection::caret(caret));
    }

    /// A table block as the `WriterTable` model: the cell text of each cell, its
    /// header flag, style and merges. The text is the cells' own, never Markdown.
    pub fn table_from_block(&self, block_id: u64) -> Option<WriterTable> {
        self.loom_table_from_block(block_id)
            .map(|table| WriterTable::from_loom_table(&table))
    }

    /// The text of one cell and, for each of its byte boundaries, the block-relative
    /// offset of that boundary in the table's editor text.
    pub fn table_cell_display_map(
        &self,
        block_id: u64,
        row: usize,
        column: usize,
    ) -> Option<(String, Vec<usize>)> {
        let index = self.table_index(block_id)?;
        let table = self.blocks[index].typed_table()?;
        let text = table.cell_text(row, column)?;
        let start = TableTextMap::new(table).cell_range(row, column)?.start;
        Some((text.clone(), (0..=text.len()).map(|i| start + i).collect()))
    }

    /// The typed table of a table block.
    pub fn loom_table_from_block(&self, block_id: u64) -> Option<Table> {
        let index = self.table_index(block_id)?;
        self.blocks[index].typed_table().cloned()
    }

    /// The byte range of each cell's text within the block, row by row.
    pub fn table_cell_ranges(&self, block_id: u64) -> Option<Vec<Vec<(usize, usize)>>> {
        let index = self.table_index(block_id)?;
        let table = self.blocks[index].typed_table()?;
        let map = TableTextMap::new(table);
        Some(
            (0..table.row_count())
                .map(|row| {
                    (0..table.col_count())
                        .filter_map(|column| map.cell_range(row, column))
                        .map(|range| (range.start, range.end))
                        .collect()
                })
                .collect(),
        )
    }

    /// Applies edited flat text to a table block. An edit that would change the
    /// rows or cells is refused and the document is left unchanged.
    pub fn edit_table_text(&mut self, block_id: u64, new_flat: &str) -> Result<(), String> {
        let index = self
            .table_index(block_id)
            .ok_or_else(|| "Not a table block".to_string())?;
        let table = self.blocks[index]
            .typed_table()
            .ok_or_else(|| "Table block has no table".to_string())?;
        let updated = apply_flat_text(table, new_flat)?;
        self.blocks[index].set_table(updated);
        Ok(())
    }

    /// Locates the table cell containing the given offset in the canonical editor text.
    /// A covered (merged) cell reports the cell that spans it.
    pub fn table_cell_at_offset(&self, offset: usize) -> Option<TableCellLocation> {
        let mut block_start = 0usize;
        for (index, block) in self.blocks.iter().enumerate() {
            let block_len = block.text.len_bytes();
            if let Some(table) = block.typed_table() {
                if offset >= block_start && offset <= block_start + block_len {
                    let map = TableTextMap::new(table);
                    if let Some((row, column)) = map.cell_at(offset - block_start) {
                        let (row, column) = table
                            .effective_cell(row, column)
                            .map_or((row, column), |(r, c, _)| (r, c));
                        let range = map.cell_range(row, column)?;
                        return Some(TableCellLocation {
                            block_id: block.id,
                            block_index: index,
                            row,
                            col: column,
                            cell_start: block_start + range.start,
                            cell_end: block_start + range.end,
                        });
                    }
                }
            }
            block_start += block_len + 1;
        }
        None
    }

    /// Navigates with Tab between cells in a table. At the final cell of a table,
    /// a forward Tab appends a new row and places the caret in the first cell of the new row.
    pub fn tab_navigate_table(&mut self, offset: usize, backwards: bool) -> Result<usize, String> {
        let loc = self
            .table_cell_at_offset(offset)
            .ok_or_else(|| "Offset is not in a table".to_string())?;
        let index = loc.block_index;
        let mut table = self.blocks[index]
            .typed_table()
            .cloned()
            .ok_or_else(|| "Failed to parse table".to_string())?;
        let rows = table.row_count();
        let columns = table.col_count();
        if rows == 0 || columns == 0 {
            return Err("Empty table".to_string());
        }
        let total = rows * columns;
        let linear = loc.row * columns + loc.col;
        let mut target = if backwards {
            linear.saturating_sub(1)
        } else if linear + 1 < total {
            linear + 1
        } else {
            // At the last cell of the table: append a row like the current one.
            let row = row_like(&table, rows - 1);
            table.rows.push(row);
            total
        };
        let grows = if backwards { -1isize } else { 1 };
        while target < table.row_count() * columns
            && table.is_covered(target / columns, target % columns)
        {
            let next = target as isize + grows;
            if next < 0 {
                break;
            }
            target = next as usize;
        }
        let (row, column) = (target / columns, target % columns);
        Ok(self.store_table_and_place_caret(index, table, row, column))
    }

    /// Navigates between table cells using arrow keys.
    pub fn arrow_navigate_table(
        &mut self,
        offset: usize,
        direction: TableArrowDirection,
    ) -> Option<usize> {
        let loc = self.table_cell_at_offset(offset)?;
        let table = self.blocks[loc.block_index].typed_table()?;
        let rows = table.row_count();
        let columns = table.col_count();
        let (row, column) = match direction {
            TableArrowDirection::Up if loc.row > 0 => (loc.row - 1, loc.col),
            TableArrowDirection::Down if loc.row + 1 < rows => (loc.row + 1, loc.col),
            TableArrowDirection::Left if loc.col > 0 => (loc.row, loc.col - 1),
            TableArrowDirection::Right if loc.col + 1 < columns => (loc.row, loc.col + 1),
            _ => return None,
        };
        let map = TableTextMap::new(table);
        let range = map.cell_range(row, column)?;
        let caret = self.table_block_start(loc.block_index) + range.start;
        self.set_selection(TextSelection::caret(caret));
        Some(caret)
    }

    /// Inserts a row into the table at the given offset, above or below the current row.
    pub fn insert_table_row_at_offset(
        &mut self,
        offset: usize,
        below: bool,
    ) -> Result<usize, String> {
        let loc = self
            .table_cell_at_offset(offset)
            .ok_or_else(|| "Offset is not in a table".to_string())?;
        let index = loc.block_index;
        let mut table = self.blocks[index]
            .typed_table()
            .cloned()
            .ok_or_else(|| "Failed to parse table".to_string())?;
        let at = if below { loc.row + 1 } else { loc.row };
        let row = row_like(&table, loc.row);
        table.rows.insert(at, row);
        Ok(self.store_table_and_place_caret(index, table, at, 0))
    }

    /// Deletes the table row containing the given offset.
    pub fn delete_table_row_at_offset(&mut self, offset: usize) -> Result<usize, String> {
        let loc = self
            .table_cell_at_offset(offset)
            .ok_or_else(|| "Offset is not in a table".to_string())?;
        let index = loc.block_index;
        let mut table = self.blocks[index]
            .typed_table()
            .cloned()
            .ok_or_else(|| "Failed to parse table".to_string())?;
        if table.row_count() <= 1 {
            return Err("Cannot delete the only row of a table".to_string());
        }
        table.delete_row(loc.row);
        let row = loc.row.min(table.row_count() - 1);
        let column = loc.col.min(table.col_count() - 1);
        Ok(self.store_table_and_place_caret(index, table, row, column))
    }

    /// Inserts a column into the table at the given offset, to the left or right of current column.
    pub fn insert_table_column_at_offset(
        &mut self,
        offset: usize,
        right: bool,
    ) -> Result<usize, String> {
        let loc = self
            .table_cell_at_offset(offset)
            .ok_or_else(|| "Offset is not in a table".to_string())?;
        let index = loc.block_index;
        let mut table = self.blocks[index]
            .typed_table()
            .cloned()
            .ok_or_else(|| "Failed to parse table".to_string())?;
        let at = if right { loc.col + 1 } else { loc.col };
        table.insert_column(at, None);
        // A new column is left-aligned, as it always was; the others keep theirs.
        table.ensure_grid_consistency();
        Ok(self.store_table_and_place_caret(index, table, loc.row, at))
    }

    /// Deletes the table column containing the given offset.
    pub fn delete_table_column_at_offset(&mut self, offset: usize) -> Result<usize, String> {
        let loc = self
            .table_cell_at_offset(offset)
            .ok_or_else(|| "Offset is not in a table".to_string())?;
        let index = loc.block_index;
        let mut table = self.blocks[index]
            .typed_table()
            .cloned()
            .ok_or_else(|| "Failed to parse table".to_string())?;
        if table.col_count() <= 1 {
            return Err("Cannot delete the only column of a table".to_string());
        }
        table.delete_column(loc.col);
        let column = loc.col.min(table.col_count() - 1);
        Ok(self.store_table_and_place_caret(index, table, loc.row, column))
    }

    /// Merges a rectangular block of cells starting at the cell at `offset`.
    pub fn merge_cells_at_offset(
        &mut self,
        offset: usize,
        row_span: usize,
        col_span: usize,
    ) -> Result<(), String> {
        let loc = self
            .table_cell_at_offset(offset)
            .ok_or_else(|| "Offset is not in a table".to_string())?;
        let index = loc.block_index;
        let mut table = self.blocks[index]
            .typed_table()
            .cloned()
            .ok_or_else(|| "Failed to parse table".to_string())?;
        table
            .merge_cells(loc.row, loc.col, row_span, col_span)
            .map_err(|e| e.to_string())?;
        self.blocks[index].set_table(table);
        Ok(())
    }

    /// Splits the merged cell containing the given offset.
    pub fn split_cell_at_offset(&mut self, offset: usize) -> Result<(), String> {
        let loc = self
            .table_cell_at_offset(offset)
            .ok_or_else(|| "Offset is not in a table".to_string())?;
        let index = loc.block_index;
        let mut table = self.blocks[index]
            .typed_table()
            .cloned()
            .ok_or_else(|| "Failed to parse table".to_string())?;
        table
            .split_cell(loc.row, loc.col)
            .map_err(|e| e.to_string())?;
        self.blocks[index].set_table(table);
        Ok(())
    }

    /// Sets the style ID for the table containing `offset`.
    pub fn set_table_style_at_offset(
        &mut self,
        offset: usize,
        style_id: &str,
    ) -> Result<(), String> {
        let loc = self
            .table_cell_at_offset(offset)
            .ok_or_else(|| "Offset is not in a table".to_string())?;
        let index = loc.block_index;
        let mut table = self.blocks[index]
            .typed_table()
            .cloned()
            .ok_or_else(|| "Failed to parse table".to_string())?;
        table.style_id = style_id.to_string();
        self.blocks[index].set_table(table);
        Ok(())
    }

    /// Toggles a table option (`header_row`, `banded_rows` or `first_column`)
    /// for the table containing `offset`.
    pub fn toggle_table_option_at_offset(
        &mut self,
        offset: usize,
        option: &str,
    ) -> Result<(), String> {
        let loc = self
            .table_cell_at_offset(offset)
            .ok_or_else(|| "Offset is not in a table".to_string())?;
        let index = loc.block_index;
        let mut table = self.blocks[index]
            .typed_table()
            .cloned()
            .ok_or_else(|| "Failed to parse table".to_string())?;
        match option {
            "header_row" => table.header_row = !table.header_row,
            "banded_rows" => table.banded_rows = !table.banded_rows,
            "first_column" => table.first_column = !table.first_column,
            _ => return Err(format!("Unknown table option '{option}'")),
        }
        self.blocks[index].set_table(table);
        Ok(())
    }
}

impl WriterTable {
    /// Converts this `WriterTable` into a structured `loom_objects::Table`.
    pub fn to_loom_table(&self) -> Table {
        let columns = self.rows.iter().map(Vec::len).max().unwrap_or(0);
        let mut table = Table::new(self.rows.len(), columns);
        table.id = self.id.clone();
        table.header_row = self.header_row;
        table.style_id = self.style_id.clone();
        table.banded_rows = self.banded_rows;
        table.first_column = self.first_column;
        for (row, cells) in self.rows.iter().enumerate() {
            for (column, text) in cells.iter().enumerate() {
                table.set_cell_text(row, column, text);
            }
        }
        for &(row, column, row_span, column_span) in &self.merges {
            let _ = table.merge_cells(row, column, row_span, column_span);
        }
        table
    }

    /// Migrates a `loom_objects::Table` into a `WriterTable`, cell text only.
    pub fn from_loom_table(table: &Table) -> Self {
        let rows = table
            .rows
            .iter()
            .map(|row| row.cells.iter().map(|cell| cell.content.text()).collect())
            .collect();
        let mut merges = Vec::new();
        for (row, table_row) in table.rows.iter().enumerate() {
            for (column, cell) in table_row.cells.iter().enumerate() {
                if !cell.is_covered && (cell.row_span > 1 || cell.col_span > 1) {
                    merges.push((row, column, cell.row_span, cell.col_span));
                }
            }
        }
        Self {
            id: table.id.clone(),
            rows,
            header_row: table.header_row,
            style_id: table.style_id.clone(),
            banded_rows: table.banded_rows,
            first_column: table.first_column,
            merges,
        }
    }
}

/// Typed cell formatting. Bold, italic and underline are stored on each cell's
/// runs, so the page, PDF and DOCX draw the same runs. Alignment is stored on
/// each cell's style override, the place both exporters read it from.
impl WriterDocument {
    /// The index and start offset of the table block whose text holds all of
    /// `range`. `None` when the range is not inside one table block.
    fn table_block_span(&self, range: &Range<usize>) -> Option<(usize, usize)> {
        let mut start = 0usize;
        for (index, block) in self.blocks.iter().enumerate() {
            let end = start + block.text.len_bytes();
            if block.typed_table().is_some() && range.start >= start && range.end <= end {
                return Some((index, start));
            }
            start = end + 1;
        }
        None
    }

    /// The table block that holds every offset of `range`, when the whole range
    /// lies inside one table block.
    pub fn table_block_id_for_range(&self, range: Range<usize>) -> Option<u64> {
        self.table_block_span(&range)
            .map(|(index, _)| self.blocks[index].id)
    }

    /// Whether every typed run of the cells in `rows` x `columns` satisfies `has`.
    /// Merged-away cells are skipped. `None` when `block_id` is not a table block.
    pub fn table_cells_have(
        &self,
        block_id: u64,
        rows: Range<usize>,
        columns: Range<usize>,
        has: impl Fn(&TextRun) -> bool,
    ) -> Option<bool> {
        let index = self.table_index(block_id)?;
        let table = self.blocks[index].typed_table()?;
        for row in rows {
            for column in columns.clone() {
                let Some(cell) = table.cell(row, column) else {
                    continue;
                };
                if cell.is_covered {
                    continue;
                }
                let len: usize = cell.content.runs.iter().map(|run| run.text.len()).sum();
                if !runs_cover(&cell.content.runs, 0..len, &has) {
                    return Some(false);
                }
            }
        }
        Some(true)
    }

    /// Changes the typed runs of every cell in `rows` x `columns`, as one undo
    /// step. Merged-away cells are left alone.
    pub fn set_table_cells_runs(
        &mut self,
        block_id: u64,
        rows: Range<usize>,
        columns: Range<usize>,
        change: impl Fn(&mut TextRun),
    ) -> Result<(), String> {
        let index = self
            .table_index(block_id)
            .ok_or_else(|| "Not a table block".to_string())?;
        let mut table = self.blocks[index]
            .typed_table()
            .cloned()
            .ok_or_else(|| "Table block has no table".to_string())?;
        for row in rows {
            for column in columns.clone() {
                let Some(cell) = table.cell_mut(row, column) else {
                    continue;
                };
                if cell.is_covered {
                    continue;
                }
                let len: usize = cell.content.runs.iter().map(|run| run.text.len()).sum();
                cell.content.runs = restyle_runs(&cell.content.runs, 0..len, &change);
            }
        }
        self.blocks[index].set_table(table);
        Ok(())
    }

    /// Whether the selected table text (global editor offsets) all satisfies
    /// `has`. `None` when the range is not inside one table block.
    pub fn table_text_has(
        &self,
        range: Range<usize>,
        has: impl Fn(&TextRun) -> bool,
    ) -> Option<bool> {
        let (index, block_start) = self.table_block_span(&range)?;
        let table = self.blocks[index].typed_table()?;
        let scope = (range.start - block_start)..(range.end - block_start);
        let map = TableTextMap::new(table);
        for row in 0..table.row_count() {
            for column in 0..table.col_count() {
                let (Some(cell_range), Some(cell)) =
                    (map.cell_range(row, column), table.cell(row, column))
                else {
                    continue;
                };
                if cell.is_covered {
                    continue;
                }
                if let Some(part) = cell_part(cell_range.start, cell_range.len(), &scope) {
                    if !runs_cover(&cell.content.runs, part, &has) {
                        return Some(false);
                    }
                }
            }
        }
        Some(true)
    }

    /// Changes the typed runs of the cell text inside `range` (global editor
    /// offsets). `Ok(false)` when the range is not inside one table block.
    pub fn set_table_text_runs(
        &mut self,
        range: Range<usize>,
        change: impl Fn(&mut TextRun),
    ) -> Result<bool, String> {
        let Some((index, block_start)) = self.table_block_span(&range) else {
            return Ok(false);
        };
        let mut table = self.blocks[index]
            .typed_table()
            .cloned()
            .ok_or_else(|| "Table block has no table".to_string())?;
        let scope = (range.start - block_start)..(range.end - block_start);
        let map = TableTextMap::new(&table);
        let mut parts = Vec::new();
        for row in 0..table.row_count() {
            for column in 0..table.col_count() {
                let (Some(cell_range), Some(cell)) =
                    (map.cell_range(row, column), table.cell(row, column))
                else {
                    continue;
                };
                if cell.is_covered {
                    continue;
                }
                if let Some(part) = cell_part(cell_range.start, cell_range.len(), &scope) {
                    parts.push((row, column, part));
                }
            }
        }
        for (row, column, part) in parts {
            if let Some(cell) = table.cell_mut(row, column) {
                cell.content.runs = restyle_runs(&cell.content.runs, part, &change);
            }
        }
        self.blocks[index].set_table(table);
        Ok(true)
    }

    /// Sets the horizontal alignment of every cell in the given columns of a table.
    pub fn set_table_column_alignment(
        &mut self,
        block_id: u64,
        columns: Range<usize>,
        align: HorizontalAlign,
    ) -> Result<(), String> {
        let index = self
            .table_index(block_id)
            .ok_or_else(|| "Not a table block".to_string())?;
        let mut table = self.blocks[index]
            .typed_table()
            .cloned()
            .ok_or_else(|| "Table block has no table".to_string())?;
        for row in 0..table.row_count() {
            for column in columns.clone() {
                if let Some(cell) = table.cell_mut(row, column) {
                    cell.style_override
                        .get_or_insert_with(Default::default)
                        .horizontal_align = Some(align);
                }
            }
        }
        self.blocks[index].set_table(table);
        Ok(())
    }
}
