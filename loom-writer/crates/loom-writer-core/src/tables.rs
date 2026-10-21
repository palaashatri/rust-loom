//! Table blocks: markdown-native tables on `WriterDocument`.
//!
//! A table is a block whose kind is [`TABLE_BLOCK_KIND`] and whose text is a
//! Markdown table written by [`WriterTable::to_markdown`]. The block text is
//! the single source of truth: editing it edits the table, so no separate
//! model can desync. This module lives outside `lib.rs` to keep the crate
//! root within its registered byte ceiling.

use crate::comments::TextEdit;
use crate::{RichBlock, WriterDocument, WriterTable};

/// Block kind for markdown table blocks.
pub const TABLE_BLOCK_KIND: &str = "table";

/// Default geometry for a table inserted from the UI.
pub const INSERT_ROWS: usize = 3;
pub const INSERT_COLUMNS: usize = 3;

impl WriterDocument {
    /// Inserts a new empty table block after the block containing `offset`
    /// in the canonical editor stream (or appends when the offset is past the
    /// end), returning the new block id. The caret moves to the table start.
    pub fn insert_table_block(
        &mut self,
        offset: usize,
        rows: usize,
        columns: usize,
    ) -> Result<u64, String> {
        if rows == 0 || columns == 0 {
            return Err("table geometry must be non-empty".into());
        }
        let mut next_id = self.blocks.iter().map(|block| block.id).max().unwrap_or(0) + 1;
        while self.blocks.iter().any(|block| block.id == next_id) {
            next_id += 1;
        }
        let table = WriterTable::new(format!("table-{next_id}"), rows, columns);
        let text = table.to_markdown();
        let block = RichBlock::new(next_id, TABLE_BLOCK_KIND, &text);
        let insert_at = block_insert_index(self, offset);
        self.blocks.insert(insert_at, block);
        let caret = self
            .blocks
            .iter()
            .take(insert_at)
            .map(|block| block.text.len_bytes() + 1)
            .sum::<usize>()
            + 2;
        self.set_selection(crate::TextSelection::caret(caret));
        Ok(next_id)
    }

    /// Inserts a table block from a `loom_objects::Table` object, converting it
    /// into a `WriterTable`.
    pub fn insert_table_object(
        &mut self,
        offset: usize,
        table: &loom_objects::Table,
    ) -> Result<u64, String> {
        if table.rows.is_empty() || table.columns.is_empty() {
            return Err("table geometry must be non-empty".into());
        }
        let mut next_id = self.blocks.iter().map(|block| block.id).max().unwrap_or(0) + 1;
        while self.blocks.iter().any(|block| block.id == next_id) {
            next_id += 1;
        }
        let writer_table = WriterTable::from_loom_table(table);
        let text = writer_table.to_markdown();
        let block = RichBlock::new(next_id, TABLE_BLOCK_KIND, &text);
        let insert_at = block_insert_index(self, offset);
        self.blocks.insert(insert_at, block);
        let caret = self
            .blocks
            .iter()
            .take(insert_at)
            .map(|block| block.text.len_bytes() + 1)
            .sum::<usize>()
            + 2;
        self.set_selection(crate::TextSelection::caret(caret));
        Ok(next_id)
    }

    /// Parses the markdown table carried by the given block. Returns `None`
    /// when the block is not a table or its text does not parse.
    pub fn table_from_block(&self, block_id: u64) -> Option<WriterTable> {
        let block = self.blocks.iter().find(|block| block.id == block_id)?;
        if block.kind != TABLE_BLOCK_KIND {
            return None;
        }
        Some(parse_table_markdown(block.text.as_str()))
    }

    /// Returns the parsed cell text and maps each displayed UTF-8 byte
    /// boundary to a byte boundary in the table block's Markdown source.
    pub fn table_cell_display_map(
        &self,
        block_id: u64,
        row: usize,
        col: usize,
    ) -> Option<(String, Vec<usize>)> {
        let block = self
            .blocks
            .iter()
            .find(|block| block.id == block_id && block.kind == TABLE_BLOCK_KIND)?;
        let rows = table_data_rows(block.text.as_str());
        let cell = rows.get(row)?.get(col)?;
        Some((cell.text.clone(), cell.source_offsets.clone()))
    }

    /// Locates the table cell containing the given offset in the canonical editor text.
    pub fn table_cell_at_offset(&self, offset: usize) -> Option<TableCellLocation> {
        let mut block_start = 0usize;
        for (index, block) in self.blocks.iter().enumerate() {
            let block_len = block.text.len_bytes();
            if block.kind == TABLE_BLOCK_KIND
                && offset >= block_start
                && offset <= block_start + block_len
            {
                let offset_in_block = offset - block_start;
                let grid = compute_cell_ranges(block.text.as_str());
                // Prefer an actual cell-text boundary over the padded range
                // used to make pipes and surrounding whitespace navigable.
                // Otherwise an empty cell's padding can claim the first byte
                // of a populated cell that follows it.
                for exact_range in [true, false] {
                    for (r, row_ranges) in grid.iter().enumerate() {
                        for (c, &(start, end)) in row_ranges.iter().enumerate() {
                            let matches = if exact_range {
                                offset_in_block >= start && offset_in_block <= end
                            } else {
                                offset_in_block >= start.saturating_sub(2)
                                    && offset_in_block <= end + 2
                            };
                            if !matches {
                                continue;
                            }
                            return Some(TableCellLocation {
                                block_id: block.id,
                                block_index: index,
                                row: r,
                                col: c,
                                cell_start: block_start + start,
                                cell_end: block_start + end,
                            });
                        }
                    }
                }
                if let Some(first_range) = grid.first().and_then(|row| row.first()) {
                    return Some(TableCellLocation {
                        block_id: block.id,
                        block_index: index,
                        row: 0,
                        col: 0,
                        cell_start: block_start + first_range.0,
                        cell_end: block_start + first_range.1,
                    });
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
        let block_idx = loc.block_index;
        let mut table = self
            .table_from_block(loc.block_id)
            .ok_or_else(|| "Failed to parse table".to_string())?;
        let rows = table.rows.len();
        let cols = table.columns();
        if rows == 0 || cols == 0 {
            return Err("Empty table".to_string());
        }

        let (target_row, target_col) = if backwards {
            if loc.col > 0 {
                (loc.row, loc.col - 1)
            } else if loc.row > 0 {
                (loc.row - 1, cols - 1)
            } else {
                (0, 0)
            }
        } else if loc.col + 1 < cols {
            (loc.row, loc.col + 1)
        } else if loc.row + 1 < rows {
            (loc.row + 1, 0)
        } else {
            // At the last cell of the table: append a new row
            table.rows.push(vec![String::new(); cols]);
            let new_markdown = table.to_markdown();
            self.blocks[block_idx].text = crate::Text::from_str(&new_markdown);
            (rows, 0)
        };

        let block_start = self
            .blocks
            .iter()
            .take(block_idx)
            .map(|b| b.text.len_bytes() + 1)
            .sum::<usize>();
        let block_text = self.blocks[block_idx].text.as_str();
        let grid = compute_cell_ranges(block_text);
        if let Some((start, _)) = grid.get(target_row).and_then(|row| row.get(target_col)) {
            let caret = block_start + start;
            self.set_selection(crate::TextSelection::caret(caret));
            Ok(caret)
        } else {
            Ok(offset)
        }
    }

    /// Navigates between table cells using arrow keys.
    pub fn arrow_navigate_table(
        &mut self,
        offset: usize,
        direction: TableArrowDirection,
    ) -> Option<usize> {
        let loc = self.table_cell_at_offset(offset)?;
        let block_idx = loc.block_index;
        let table = self.table_from_block(loc.block_id)?;
        let rows = table.rows.len();
        let cols = table.columns();
        if rows == 0 || cols == 0 {
            return None;
        }

        let (target_row, target_col) = match direction {
            TableArrowDirection::Up if loc.row > 0 => (loc.row - 1, loc.col),
            TableArrowDirection::Down if loc.row + 1 < rows => (loc.row + 1, loc.col),
            TableArrowDirection::Left if loc.col > 0 => (loc.row, loc.col - 1),
            TableArrowDirection::Right if loc.col + 1 < cols => (loc.row, loc.col + 1),
            _ => return None,
        };

        let block_start = self
            .blocks
            .iter()
            .take(block_idx)
            .map(|b| b.text.len_bytes() + 1)
            .sum::<usize>();
        let block_text = self.blocks[block_idx].text.as_str();
        let grid = compute_cell_ranges(block_text);
        let (start, _) = grid.get(target_row).and_then(|row| row.get(target_col))?;
        let caret = block_start + start;
        self.set_selection(crate::TextSelection::caret(caret));
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
        let block_idx = loc.block_index;
        let mut table = self
            .table_from_block(loc.block_id)
            .ok_or_else(|| "Failed to parse table".to_string())?;
        let cols = table.columns();
        let insert_idx = if below { loc.row + 1 } else { loc.row };
        table.rows.insert(insert_idx, vec![String::new(); cols]);
        let new_markdown = table.to_markdown();
        self.blocks[block_idx].text = crate::Text::from_str(&new_markdown);

        let block_start = self
            .blocks
            .iter()
            .take(block_idx)
            .map(|b| b.text.len_bytes() + 1)
            .sum::<usize>();
        let block_text = self.blocks[block_idx].text.as_str();
        let grid = compute_cell_ranges(block_text);
        if let Some((start, _)) = grid.get(insert_idx).and_then(|row| row.first()) {
            let caret = block_start + start;
            self.set_selection(crate::TextSelection::caret(caret));
            Ok(caret)
        } else {
            Ok(offset)
        }
    }

    /// Deletes the table row containing the given offset.
    pub fn delete_table_row_at_offset(&mut self, offset: usize) -> Result<usize, String> {
        let loc = self
            .table_cell_at_offset(offset)
            .ok_or_else(|| "Offset is not in a table".to_string())?;
        let block_idx = loc.block_index;
        let mut table = self
            .table_from_block(loc.block_id)
            .ok_or_else(|| "Failed to parse table".to_string())?;
        if table.rows.len() <= 1 {
            return Err("Cannot delete the only row of a table".to_string());
        }
        table.rows.remove(loc.row);
        let new_markdown = table.to_markdown();
        self.blocks[block_idx].text = crate::Text::from_str(&new_markdown);

        let block_start = self
            .blocks
            .iter()
            .take(block_idx)
            .map(|b| b.text.len_bytes() + 1)
            .sum::<usize>();
        let block_text = self.blocks[block_idx].text.as_str();
        let grid = compute_cell_ranges(block_text);
        let target_row = loc.row.min(table.rows.len() - 1);
        let target_col = loc.col.min(table.columns() - 1);
        if let Some((start, _)) = grid.get(target_row).and_then(|row| row.get(target_col)) {
            let caret = block_start + start;
            self.set_selection(crate::TextSelection::caret(caret));
            Ok(caret)
        } else {
            Ok(block_start)
        }
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
        let block_idx = loc.block_index;
        let mut table = self
            .table_from_block(loc.block_id)
            .ok_or_else(|| "Failed to parse table".to_string())?;
        let insert_col = if right { loc.col + 1 } else { loc.col };
        for row in &mut table.rows {
            let idx = insert_col.min(row.len());
            row.insert(idx, String::new());
        }
        let new_markdown = table.to_markdown();
        self.blocks[block_idx].text = crate::Text::from_str(&new_markdown);

        let block_start = self
            .blocks
            .iter()
            .take(block_idx)
            .map(|b| b.text.len_bytes() + 1)
            .sum::<usize>();
        let block_text = self.blocks[block_idx].text.as_str();
        let grid = compute_cell_ranges(block_text);
        if let Some((start, _)) = grid.get(loc.row).and_then(|row| row.get(insert_col)) {
            let caret = block_start + start;
            self.set_selection(crate::TextSelection::caret(caret));
            Ok(caret)
        } else {
            Ok(offset)
        }
    }

    /// Deletes the table column containing the given offset.
    pub fn delete_table_column_at_offset(&mut self, offset: usize) -> Result<usize, String> {
        let loc = self
            .table_cell_at_offset(offset)
            .ok_or_else(|| "Offset is not in a table".to_string())?;
        let block_idx = loc.block_index;
        let mut table = self
            .table_from_block(loc.block_id)
            .ok_or_else(|| "Failed to parse table".to_string())?;
        if table.columns() <= 1 {
            return Err("Cannot delete the only column of a table".to_string());
        }
        for row in &mut table.rows {
            if loc.col < row.len() {
                row.remove(loc.col);
            }
        }
        let new_markdown = table.to_markdown();
        self.blocks[block_idx].text = crate::Text::from_str(&new_markdown);

        let block_start = self
            .blocks
            .iter()
            .take(block_idx)
            .map(|b| b.text.len_bytes() + 1)
            .sum::<usize>();
        let block_text = self.blocks[block_idx].text.as_str();
        let grid = compute_cell_ranges(block_text);
        let target_col = loc.col.min(table.columns().saturating_sub(1));
        if let Some((start, _)) = grid.get(loc.row).and_then(|row| row.get(target_col)) {
            let caret = block_start + start;
            self.set_selection(crate::TextSelection::caret(caret));
            Ok(caret)
        } else {
            Ok(block_start)
        }
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
        let block_idx = loc.block_index;
        let table = self
            .table_from_block(loc.block_id)
            .ok_or_else(|| "Failed to parse table".to_string())?;
        let mut loom_tbl = table.to_loom_table();
        loom_tbl
            .merge_cells(loc.row, loc.col, row_span, col_span)
            .map_err(|e| e.to_string())?;
        let updated = WriterTable::from_loom_table(&loom_tbl);
        self.blocks[block_idx].text = crate::Text::from_str(&updated.to_markdown());
        Ok(())
    }

    /// Splits a previously merged cell at `offset`.
    pub fn split_cell_at_offset(&mut self, offset: usize) -> Result<(), String> {
        let loc = self
            .table_cell_at_offset(offset)
            .ok_or_else(|| "Offset is not in a table".to_string())?;
        let block_idx = loc.block_index;
        let table = self
            .table_from_block(loc.block_id)
            .ok_or_else(|| "Failed to parse table".to_string())?;
        let mut loom_tbl = table.to_loom_table();
        loom_tbl
            .split_cell(loc.row, loc.col)
            .map_err(|e| e.to_string())?;
        let updated = WriterTable::from_loom_table(&loom_tbl);
        self.blocks[block_idx].text = crate::Text::from_str(&updated.to_markdown());
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
        let block_idx = loc.block_index;
        let mut table = self
            .table_from_block(loc.block_id)
            .ok_or_else(|| "Failed to parse table".to_string())?;
        table.style_id = style_id.to_string();
        self.blocks[block_idx].text = crate::Text::from_str(&table.to_markdown());
        Ok(())
    }

    /// Toggles a table option ("header_row", "banded_rows", "first_column") for the table containing `offset`.
    pub fn toggle_table_option_at_offset(
        &mut self,
        offset: usize,
        option: &str,
    ) -> Result<(), String> {
        let loc = self
            .table_cell_at_offset(offset)
            .ok_or_else(|| "Offset is not in a table".to_string())?;
        let block_idx = loc.block_index;
        let mut table = self
            .table_from_block(loc.block_id)
            .ok_or_else(|| "Failed to parse table".to_string())?;
        match option {
            "header_row" => table.header_row = !table.header_row,
            "banded_rows" => table.banded_rows = !table.banded_rows,
            "first_column" => table.first_column = !table.first_column,
            _ => return Err(format!("Unknown table option '{option}'")),
        }
        self.blocks[block_idx].text = crate::Text::from_str(&table.to_markdown());
        Ok(())
    }
}

/// Direction for arrow key navigation within table cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableArrowDirection {
    Up,
    Down,
    Left,
    Right,
}

/// Identifies the cell location and byte range of an offset within a table block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TableCellLocation {
    pub block_id: u64,
    pub block_index: usize,
    pub row: usize,
    pub col: usize,
    pub cell_start: usize,
    pub cell_end: usize,
}

/// Computes the canonical Markdown byte range for each parsed table cell.
pub fn compute_cell_ranges(text: &str) -> Vec<Vec<(usize, usize)>> {
    table_data_rows(text)
        .into_iter()
        .map(|row| row.into_iter().map(|cell| cell.source_range()).collect())
        .collect()
}

/// Index in `document.blocks` after which a table inserted at `offset` lands.
pub(crate) fn block_insert_index(document: &WriterDocument, offset: usize) -> usize {
    let mut cursor = 0usize;
    let total = document.editor_text().len();
    for (index, block) in document.blocks.iter().enumerate() {
        cursor += block.text.len_bytes();
        if offset <= cursor && cursor < total {
            return index + 1;
        }
        cursor += 1; // paragraph separator
    }
    document.blocks.len()
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedTableCellText {
    text: String,
    source_offsets: Vec<usize>,
}

impl ParsedTableCellText {
    fn source_range(&self) -> (usize, usize) {
        (
            self.source_offsets.first().copied().unwrap_or(0),
            self.source_offsets.last().copied().unwrap_or(0),
        )
    }
}

struct ParsedTableLine {
    cells: Vec<ParsedTableCellText>,
    is_delimiter: bool,
}

/// Splits cell source spans on unescaped pipes. A pipe is escaped when it is
/// preceded by an odd run of backslashes, matching Markdown table serialization.
fn split_cell_ranges(inner: &str) -> Vec<std::ops::Range<usize>> {
    let mut cells = Vec::new();
    let mut start = 0;
    let mut backslashes = 0usize;
    for (index, character) in inner.char_indices() {
        if character == '|' && backslashes % 2 == 0 {
            cells.push(start..index);
            start = index + character.len_utf8();
        }
        if character == '\\' {
            backslashes += 1;
        } else {
            backslashes = 0;
        }
    }
    cells.push(start..inner.len());
    cells
}

fn mapped_plain_text(text: &str, source_start: usize) -> ParsedTableCellText {
    let mut source_offsets = Vec::with_capacity(text.len() + 1);
    source_offsets.push(source_start);
    for (index, character) in text.char_indices() {
        source_offsets.extend((1..=character.len_utf8()).map(|byte| source_start + index + byte));
    }
    ParsedTableCellText {
        text: text.to_string(),
        source_offsets,
    }
}

/// Applies the parser's trim and `\\|` decoding while retaining canonical
/// source offsets for every byte boundary in the resulting UTF-8 display text.
fn mapped_cell_text(source: &str, source_start: usize) -> ParsedTableCellText {
    let leading_trim = source.len() - source.trim_start().len();
    let trimmed = source.trim();
    let source_start = source_start + leading_trim;
    let mut text = String::with_capacity(trimmed.len());
    let mut source_offsets = Vec::with_capacity(trimmed.len() + 1);
    source_offsets.push(source_start);
    let mut characters = trimmed.char_indices().peekable();

    while let Some((index, character)) = characters.next() {
        if character == '\\' && characters.peek().is_some_and(|(_, next)| *next == '|') {
            let (pipe_index, pipe) = characters.next().expect("peeked escaped pipe");
            text.push(pipe);
            source_offsets.push(source_start + pipe_index + pipe.len_utf8());
        } else {
            text.push(character);
            source_offsets
                .extend((1..=character.len_utf8()).map(|byte| source_start + index + byte));
        }
    }

    ParsedTableCellText {
        text,
        source_offsets,
    }
}

fn append_mapped_cell(left: &mut ParsedTableCellText, right: ParsedTableCellText) {
    if right.text.is_empty() {
        return;
    }
    if left.text.is_empty() {
        *left = right;
        return;
    }

    left.text.push(' ');
    left.source_offsets
        .push(right.source_offsets.first().copied().unwrap_or(0));
    left.text.push_str(&right.text);
    left.source_offsets
        .extend(right.source_offsets.into_iter().skip(1));
}

fn is_table_metadata_line(trimmed: &str) -> bool {
    trimmed.starts_with("<!-- loom-table:") && trimmed.ends_with("-->")
}

fn parse_table_line(line: &str, line_offset: usize) -> Option<ParsedTableLine> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    let trimmed_offset = line_offset + line.len() - line.trim_start().len();
    let Some(first_pipe) = trimmed.find('|') else {
        return Some(ParsedTableLine {
            cells: vec![mapped_plain_text(trimmed, trimmed_offset)],
            is_delimiter: false,
        });
    };
    let Some(last_pipe) = trimmed.rfind('|') else {
        unreachable!("a found pipe is also the last pipe")
    };
    if first_pipe >= last_pipe {
        return Some(ParsedTableLine {
            cells: vec![mapped_plain_text(trimmed, trimmed_offset)],
            is_delimiter: false,
        });
    }

    let prefix = &trimmed[..first_pipe];
    let suffix = &trimmed[last_pipe + 1..];
    let inner_start = trimmed_offset + first_pipe + 1;
    let inner = &trimmed[first_pipe + 1..last_pipe];
    let mut cells = split_cell_ranges(inner)
        .into_iter()
        .map(|range| mapped_cell_text(&inner[range.clone()], inner_start + range.start))
        .collect::<Vec<_>>();
    let is_delimiter = cells.iter().all(|cell| {
        let compact = cell.text.replace(['-', ':', ' '], "");
        compact.is_empty() && !cell.text.is_empty()
    });
    if is_delimiter {
        return Some(ParsedTableLine {
            cells,
            is_delimiter: true,
        });
    }

    if !prefix.trim().is_empty() {
        let prefix_start = trimmed_offset;
        let prefix = mapped_cell_text(prefix, prefix_start);
        if let Some(first_cell) = cells.first_mut() {
            if first_cell.text.is_empty() {
                *first_cell = prefix;
            } else {
                let mut combined = prefix;
                append_mapped_cell(&mut combined, first_cell.clone());
                *first_cell = combined;
            }
        }
    }
    if !suffix.trim().is_empty() {
        let suffix_start = trimmed_offset + last_pipe + 1;
        let suffix = mapped_cell_text(suffix, suffix_start);
        if let Some(last_cell) = cells.last_mut() {
            if last_cell.text.is_empty() {
                *last_cell = suffix;
            } else {
                append_mapped_cell(last_cell, suffix);
            }
        }
    }

    Some(ParsedTableLine {
        cells,
        is_delimiter: false,
    })
}

fn for_each_markdown_line(markdown: &str, mut visit: impl FnMut(&str, usize)) {
    let mut line_offset = 0;
    for line_with_ending in markdown.split_inclusive('\n') {
        let line = line_with_ending
            .strip_suffix('\n')
            .unwrap_or(line_with_ending);
        let line = line.strip_suffix('\r').unwrap_or(line);
        visit(line, line_offset);
        line_offset += line_with_ending.len();
    }
}

fn table_data_rows(markdown: &str) -> Vec<Vec<ParsedTableCellText>> {
    let mut rows = Vec::new();
    for_each_markdown_line(markdown, |line, line_offset| {
        let trimmed = line.trim();
        if is_table_metadata_line(trimmed) {
            return;
        }
        if let Some(parsed) = parse_table_line(line, line_offset) {
            if !parsed.is_delimiter {
                rows.push(parsed.cells);
            }
        }
    });
    rows
}

/// Parses a Markdown table written by [`WriterTable::to_markdown`]. The
/// header separator row is skipped; escaped pipes restore to literal pipes.
pub fn parse_table_markdown(markdown: &str) -> WriterTable {
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut header_row = false;
    let mut style_id = "table-style-medium-1".to_string();
    let mut banded_rows = true;
    let mut first_column = false;
    let mut merges = Vec::new();

    for_each_markdown_line(markdown, |line, line_offset| {
        let trimmed = line.trim();
        if is_table_metadata_line(trimmed) {
            let inner = trimmed
                .trim_start_matches("<!-- loom-table:")
                .trim_end_matches("-->")
                .trim();
            for part in inner.split_whitespace() {
                if let Some((k, v)) = part.split_once('=') {
                    match k {
                        "style" => style_id = v.to_string(),
                        "banded" => banded_rows = v == "1",
                        "first_col" => first_column = v == "1",
                        "merges" => {
                            for m in v.split(';') {
                                let nums: Vec<usize> =
                                    m.split(',').filter_map(|s| s.parse().ok()).collect();
                                if nums.len() == 4 {
                                    merges.push((nums[0], nums[1], nums[2], nums[3]));
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            return;
        }
        let Some(parsed) = parse_table_line(line, line_offset) else {
            return;
        };
        if parsed.is_delimiter {
            header_row = true;
            return;
        }
        rows.push(parsed.cells.into_iter().map(|cell| cell.text).collect());
    });
    WriterTable {
        id: String::new(),
        rows,
        header_row,
        style_id,
        banded_rows,
        first_column,
        merges,
    }
}

impl WriterTable {
    /// Converts this `WriterTable` into a structured `loom_objects::Table`.
    pub fn to_loom_table(&self) -> loom_objects::Table {
        let cols = self.rows.iter().map(|r| r.len()).max().unwrap_or(0);
        let mut table = loom_objects::Table::new(self.rows.len(), cols);
        table.id = self.id.clone();
        table.header_row = self.header_row;
        table.style_id = self.style_id.clone();
        table.banded_rows = self.banded_rows;
        table.first_column = self.first_column;
        for (r, row) in self.rows.iter().enumerate() {
            for (c, cell_text) in row.iter().enumerate() {
                table.set_cell_text(r, c, cell_text);
            }
        }
        for &(r, c, rs, cs) in &self.merges {
            let _ = table.merge_cells(r, c, rs, cs);
        }
        table
    }

    /// Migrates a `loom_objects::Table` into a `WriterTable`.
    pub fn from_loom_table(table: &loom_objects::Table) -> Self {
        let mut rows = Vec::with_capacity(table.row_count());
        for row in &table.rows {
            let cells = row.cells.iter().map(|c| c.content.text()).collect();
            rows.push(cells);
        }
        let mut merges = Vec::new();
        for (r, row) in table.rows.iter().enumerate() {
            for (c, cell) in row.cells.iter().enumerate() {
                if !cell.is_covered && (cell.row_span > 1 || cell.col_span > 1) {
                    merges.push((r, c, cell.row_span, cell.col_span));
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

/// Calls `visit` with each piece of a block's text that counts toward the word
/// and character totals. A paragraph counts whole. A table counts each cell's
/// text, so its pipes, padding and header separator are not words.
pub(crate) fn for_each_counted_text(kind: &str, text: &str, mut visit: impl FnMut(&str)) {
    if kind == TABLE_BLOCK_KIND {
        for cell in parse_table_markdown(text).rows.iter().flatten() {
            visit(cell.as_str());
        }
    } else if kind == crate::IMAGE_BLOCK_KIND || kind == crate::CHART_BLOCK_KIND {
        // A picture is one object, not words.
    } else {
        visit(text);
    }
}

/// A table block's place in the new editor text: the old block index it came
/// from and the byte range it now occupies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MappedTable {
    pub old_index: usize,
    pub start: usize,
    pub end: usize,
}

/// Maps every table block of `old_blocks` through `edit` onto `new_text`.
///
/// A table keeps its identity when the edit lies in it or at its edges, so a
/// typed or deleted character changes the table's text instead of turning its
/// lines into paragraphs. Its span is then snapped to whole lines, so the text
/// around it keeps its own lines. A table the edit removes entirely has no span.
pub(crate) fn map_tables_through_edit(
    old_blocks: &[RichBlock],
    new_text: &str,
    edit: TextEdit,
) -> Vec<MappedTable> {
    let mut mapped: Vec<MappedTable> = Vec::new();
    let mut offset = 0usize;
    for (old_index, block) in old_blocks.iter().enumerate() {
        let len = block.text.len_bytes();
        if block.kind == TABLE_BLOCK_KIND {
            if let Some((start, end)) = map_span(offset, offset + len, edit, new_text) {
                // Spans come out in order; an overlap can only be introduced by
                // an edit that merges two tables' lines, and the later table is
                // then rebuilt from its lines.
                let overlaps = mapped.last().is_some_and(|previous| start <= previous.end);
                if !overlaps {
                    mapped.push(MappedTable {
                        old_index,
                        start,
                        end,
                    });
                }
            }
        }
        offset += len + 1;
    }
    mapped
}

/// The new byte range of the old span `start..end` after `edit`, snapped to
/// whole lines of `new_text`, or `None` when the edit removed the span.
fn map_span(start: usize, end: usize, edit: TextEdit, new_text: &str) -> Option<(usize, usize)> {
    let TextEdit {
        old_start,
        old_end,
        new_start,
        new_end,
    } = edit;
    let inserted = new_text.get(new_start..new_end)?;
    let delta = new_end as isize - new_start as isize - (old_end as isize - old_start as isize);
    let shift = |position: usize| (position as isize + delta) as usize;

    // The start moves past any newline typed at its edge, so that newline
    // becomes a paragraph before the table.
    let mapped_start = if start < old_start {
        start
    } else if start > old_end {
        shift(start)
    } else if start == old_start {
        match inserted.rfind('\n') {
            Some(index) => new_start + index + 1,
            None => new_start,
        }
    } else {
        new_end
    };
    // The end stops before a newline typed at its edge, so that newline becomes
    // a paragraph after the table. Anything typed inside the table stays in it.
    let mapped_end = if end < old_start {
        end
    } else if end > old_end {
        shift(end)
    } else if end == old_start {
        match inserted.find('\n') {
            Some(index) => new_start + index,
            None => new_end,
        }
    } else {
        new_end
    };
    if mapped_start >= mapped_end || !new_text.is_char_boundary(mapped_start) {
        return None;
    }
    let region = &new_text[mapped_start..mapped_end];
    if region.trim().is_empty() {
        return None;
    }
    let line_start = new_text[..mapped_start]
        .rfind('\n')
        .map_or(0, |index| index + 1);
    let line_end = new_text[mapped_end..]
        .find('\n')
        .map_or(new_text.len(), |index| mapped_end + index);
    Some((line_start, line_end))
}

/// Splits `text` into paragraphs at its newlines, except inside the mapped
/// table spans, which each form one paragraph. Returns each paragraph's text
/// and, for a table, the old block index it must keep. Without tables this is
/// the same split as `text.split('\n')`.
pub(crate) fn split_editor_text(
    text: &str,
    tables: &[MappedTable],
) -> (Vec<String>, Vec<Option<usize>>) {
    let mut paragraphs = Vec::new();
    let mut forced = Vec::new();
    let mut tables = tables.iter().peekable();
    let mut position = 0usize;
    loop {
        if let Some(table) = tables.next_if(|table| table.start == position) {
            paragraphs.push(text[table.start..table.end].to_string());
            forced.push(Some(table.old_index));
            if table.end >= text.len() {
                break;
            }
            // The newline after a table is the separator, not part of it.
            position = table.end + 1;
            continue;
        }
        match text[position..].find('\n') {
            Some(offset) => {
                paragraphs.push(text[position..position + offset].to_string());
                forced.push(None);
                position += offset + 1;
            }
            None => {
                paragraphs.push(text[position..].to_string());
                forced.push(None);
                break;
            }
        }
    }
    (paragraphs, forced)
}

/// Keeps table blocks matched to their own paragraphs: a table paragraph takes
/// its table block, and no ordinary paragraph inherits a table block's
/// identity, kind or style.
pub(crate) fn pin_table_matches(
    old_blocks: &[RichBlock],
    forced: &[Option<usize>],
    matches: &mut [Option<usize>],
) {
    for (index, found) in matches.iter_mut().enumerate() {
        match forced[index] {
            Some(old_index) => *found = Some(old_index),
            None => {
                if found.is_some_and(|old_index| old_blocks[old_index].kind == TABLE_BLOCK_KIND) {
                    *found = None;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TextSelection;

    #[test]
    fn escaped_pipes_stay_inside_their_cell() {
        let mut table = WriterTable::new("t", 2, 2);
        table.set(0, 0, "a | b");
        table.set(1, 1, "c\\d");
        let parsed = parse_table_markdown(&table.to_markdown());
        assert_eq!(parsed.rows[0], vec!["a | b".to_string(), String::new()]);
        assert_eq!(parsed.rows[1][1], "c\\d");
        assert_eq!(parsed.columns(), 2);
    }

    fn document() -> WriterDocument {
        let mut document = WriterDocument::new("doc", "Doc");
        document
            .blocks
            .push(RichBlock::new(1, "paragraph", "First"));
        document
            .blocks
            .push(RichBlock::new(2, "paragraph", "Second"));
        document
    }

    #[test]
    fn insert_table_appends_markdown_block_and_moves_caret() {
        let mut document = document();
        let id = document
            .insert_table_block(usize::MAX, 3, 3)
            .expect("insert succeeds");
        assert_eq!(id, 3);
        let table = document.table_from_block(id).expect("table parses");
        assert_eq!(table.rows.len(), 3);
        assert_eq!(table.rows[0].len(), 3);
        assert!(table.header_row);
        // The table lands at the end and the caret moves onto it.
        assert_eq!(
            document.blocks.last().expect("block").kind,
            TABLE_BLOCK_KIND
        );
        assert!(document.selection().anchor > 0);
    }

    #[test]
    fn insert_table_after_offset_places_block_between_paragraphs() {
        let mut document = document();
        let first_len = document.blocks[0].text.len_bytes();
        document
            .insert_table_block(first_len, 2, 2)
            .expect("insert succeeds");
        assert_eq!(document.blocks[1].kind, TABLE_BLOCK_KIND);
        assert_eq!(document.blocks[2].text.as_str(), "Second");
    }

    #[test]
    fn insert_rejects_empty_geometry() {
        let mut document = document();
        assert!(document.insert_table_block(0, 0, 3).is_err());
        assert!(document.insert_table_block(0, 3, 0).is_err());
    }

    #[test]
    fn table_markdown_round_trips_through_parse() {
        let mut document = document();
        let id = document.insert_table_block(usize::MAX, 2, 2).unwrap();
        let table = document.table_from_block(id).unwrap();
        let markdown = table.to_markdown();
        let parsed = parse_table_markdown(&markdown);
        assert_eq!(parsed.rows, table.rows);
        assert_eq!(parsed.header_row, table.header_row);
    }

    #[test]
    fn table_blocks_export_verbatim_to_markdown() {
        let mut document = document();
        document
            .insert_table_block(usize::MAX, 2, 2)
            .expect("insert succeeds");
        let markdown = document.to_markdown();
        assert!(markdown.contains("|  |  |"));
    }

    #[test]
    fn caret_selection_stays_normalized() {
        let mut document = document();
        document
            .insert_table_block(usize::MAX, 2, 2)
            .expect("insert succeeds");
        let selection = document.selection();
        assert!(selection.anchor <= document.editor_text().len());
        let _ = TextSelection::caret(0);
    }

    /// A paragraph, a 3x2 table and a paragraph after it.
    fn document_with_table_between_paragraphs() -> WriterDocument {
        let mut table = WriterTable::new("table-1", 3, 2);
        table.set(0, 0, "Region");
        table.set(0, 1, "Sales");
        table.set(1, 0, "North");
        table.set(1, 1, "10");
        table.set(2, 0, "South");
        table.set(2, 1, "7");
        let mut document = WriterDocument::new("doc", "Doc");
        document
            .blocks
            .push(RichBlock::new(1, "paragraph", "Intro"));
        document
            .blocks
            .push(RichBlock::new(2, TABLE_BLOCK_KIND, &table.to_markdown()));
        document
            .blocks
            .push(RichBlock::new(3, "paragraph", "Outro"));
        document
    }

    fn kinds(document: &WriterDocument) -> Vec<String> {
        document
            .blocks
            .iter()
            .map(|b| b.kind.as_str().to_string())
            .collect()
    }

    #[test]
    fn typing_in_a_table_cell_keeps_the_table_one_block() {
        let mut document = document_with_table_between_paragraphs();
        let text = document.editor_text();
        let at = text.find("North").expect("cell text") + "North".len();
        let mut edited = text.clone();
        edited.insert(at, 'X');
        document
            .replace_editor_text_at(&edited, Some(at + 1))
            .expect("cell edit");

        assert_eq!(
            kinds(&document),
            ["paragraph", TABLE_BLOCK_KIND, "paragraph"],
            "the table stays one table block"
        );
        assert_eq!(document.blocks[0].text.as_str(), "Intro");
        assert_eq!(document.blocks[2].text.as_str(), "Outro");
        let table = document.table_from_block(2).expect("table parses");
        assert_eq!(table.rows[1][0], "NorthX");
        assert_eq!(table.rows[1][1], "10", "other cells are untouched");

        let pdf = crate::export_pdf(&document);
        let content = loom_pdf::inspect::readable_content(&pdf).expect("readable PDF");
        assert!(!content.contains('|'), "no pipe characters are printed");
        assert!(
            content.contains("(NorthX) Tj"),
            "the edited cell is printed"
        );
    }

    #[test]
    fn enter_after_a_table_starts_a_paragraph_and_keeps_the_table() {
        let mut document = document_with_table_between_paragraphs();
        let table_text = document.blocks[1].text.as_str().to_string();
        let text = document.editor_text();
        let table_end = text.find("Outro").expect("next paragraph") - 1;
        let mut edited = text.clone();
        edited.insert(table_end, '\n');
        document
            .replace_editor_text_at(&edited, Some(table_end + 1))
            .expect("enter");

        assert_eq!(
            kinds(&document),
            ["paragraph", TABLE_BLOCK_KIND, "paragraph", "paragraph"]
        );
        assert_eq!(document.blocks[1].text.as_str(), table_text);
        assert_eq!(document.blocks[2].text.as_str(), "");
        assert_eq!(document.blocks[3].text.as_str(), "Outro");
    }

    #[test]
    fn table_cells_count_as_words_and_characters_not_as_markdown() {
        let mut document = WriterDocument::new("counts", "Counts");
        document
            .blocks
            .push(RichBlock::new(1, "paragraph", "Intro words here"));
        document.blocks.push(RichBlock::new(
            2,
            TABLE_BLOCK_KIND,
            "| Item | Qty |\n| --- | --- |\n| Apples | 3 |",
        ));

        // The paragraph has 3 words and 16 characters. The table's cells are
        // Item, Qty, Apples and 3: 4 words and 14 characters. Pipes, padding and
        // dashes are Markdown syntax and count for nothing.
        let stats = document.statistics();
        assert_eq!(stats.word_count, 7);
        assert_eq!(stats.char_count, 30);
        assert_eq!(stats.char_count_no_spaces, 28);
        // The status bar also counts the one character between the two blocks.
        assert_eq!(document.text_counts(), (7, 31));
        let pagination = document.estimate_pagination();
        assert_eq!((pagination.words, pagination.characters), (7, 31));
    }

    #[test]
    fn table_markdown_preserves_text_typed_before_first_pipe() {
        let text = "TypedBefore| Col1 | Col2 |\n| --- | --- |\n| Val1 | Val2 |";
        let parsed = parse_table_markdown(text);
        assert_eq!(parsed.rows.len(), 2);
        assert!(parsed.rows[0][0].contains("TypedBefore"));
    }

    #[test]
    fn insert_table_places_caret_inside_first_cell() {
        let mut document = document();
        let _ = document.insert_table_block(usize::MAX, 2, 2).unwrap();
        let block_offset = document
            .blocks
            .iter()
            .take(document.blocks.len() - 1)
            .map(|b| b.text.len_bytes() + 1)
            .sum::<usize>();
        assert_eq!(document.selection().anchor, block_offset + 2);
    }

    #[test]
    fn writer_table_converts_to_and_from_loom_objects_table() {
        let mut writer_table = WriterTable::new("tbl-1", 2, 3);
        writer_table.rows[0] = vec!["H1".into(), "H2".into(), "H3".into()];
        writer_table.rows[1] = vec!["A".into(), "B".into(), "C".into()];
        writer_table.header_row = true;

        let loom_table = writer_table.to_loom_table();
        assert_eq!(loom_table.row_count(), 2);
        assert_eq!(loom_table.col_count(), 3);
        assert_eq!(loom_table.cell_text(0, 0).as_deref(), Some("H1"));
        assert_eq!(loom_table.cell_text(1, 2).as_deref(), Some("C"));

        let converted_back = WriterTable::from_loom_table(&loom_table);
        assert_eq!(writer_table, converted_back);
    }

    #[test]
    fn table_tab_navigates_and_appends_row_at_end() {
        let mut document = document();
        let _ = document.insert_table_block(usize::MAX, 2, 2).unwrap();
        let block_idx = document.blocks.len() - 1;
        let block_id = document.blocks[block_idx].id;

        // Caret starts at (0, 0)
        let initial_caret = document.selection().anchor;
        let loc = document.table_cell_at_offset(initial_caret).unwrap();
        assert_eq!((loc.row, loc.col), (0, 0));

        // Tab to (0, 1)
        let c1 = document.tab_navigate_table(initial_caret, false).unwrap();
        let loc1 = document.table_cell_at_offset(c1).unwrap();
        assert_eq!((loc1.row, loc1.col), (0, 1));

        // Tab to (1, 0)
        let c2 = document.tab_navigate_table(c1, false).unwrap();
        let loc2 = document.table_cell_at_offset(c2).unwrap();
        assert_eq!((loc2.row, loc2.col), (1, 0));

        // Tab to (1, 1) - final cell
        let c3 = document.tab_navigate_table(c2, false).unwrap();
        let loc3 = document.table_cell_at_offset(c3).unwrap();
        assert_eq!((loc3.row, loc3.col), (1, 1));

        // Tab from final cell appends row and places caret at (2, 0)
        let c4 = document.tab_navigate_table(c3, false).unwrap();
        let loc4 = document.table_cell_at_offset(c4).unwrap();
        assert_eq!((loc4.row, loc4.col), (2, 0));

        let table = document.table_from_block(block_id).unwrap();
        assert_eq!(table.rows.len(), 3);
    }

    #[test]
    fn table_arrow_navigation_moves_between_cells() {
        let mut document = document();
        let _ = document.insert_table_block(usize::MAX, 3, 3).unwrap();
        let initial_caret = document.selection().anchor;

        // Down to (1, 0)
        let down1 = document
            .arrow_navigate_table(initial_caret, TableArrowDirection::Down)
            .unwrap();
        let loc_down1 = document.table_cell_at_offset(down1).unwrap();
        assert_eq!((loc_down1.row, loc_down1.col), (1, 0));

        // Right to (1, 1)
        let right1 = document
            .arrow_navigate_table(down1, TableArrowDirection::Right)
            .unwrap();
        let loc_r1 = document.table_cell_at_offset(right1).unwrap();
        assert_eq!((loc_r1.row, loc_r1.col), (1, 1));

        // Up to (0, 1)
        let up1 = document
            .arrow_navigate_table(right1, TableArrowDirection::Up)
            .unwrap();
        let loc_u1 = document.table_cell_at_offset(up1).unwrap();
        assert_eq!((loc_u1.row, loc_u1.col), (0, 1));
    }

    #[test]
    fn table_insert_and_delete_rows_and_columns() {
        let mut document = document();
        let _ = document.insert_table_block(usize::MAX, 2, 2).unwrap();
        let block_idx = document.blocks.len() - 1;
        let block_id = document.blocks[block_idx].id;
        let caret = document.selection().anchor;

        // Insert row below -> 3 rows
        let c_row = document.insert_table_row_at_offset(caret, true).unwrap();
        let table1 = document.table_from_block(block_id).unwrap();
        assert_eq!(table1.rows.len(), 3);

        // Delete row -> back to 2 rows
        let c_del_row = document.delete_table_row_at_offset(c_row).unwrap();
        let table2 = document.table_from_block(block_id).unwrap();
        assert_eq!(table2.rows.len(), 2);

        // Insert column right -> 3 cols
        let c_col = document
            .insert_table_column_at_offset(c_del_row, true)
            .unwrap();
        let table3 = document.table_from_block(block_id).unwrap();
        assert_eq!(table3.columns(), 3);

        // Delete column -> back to 2 cols
        let _ = document.delete_table_column_at_offset(c_col).unwrap();
        let table4 = document.table_from_block(block_id).unwrap();
        assert_eq!(table4.columns(), 2);
    }
}
