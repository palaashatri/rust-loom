//! Table blocks: markdown-native tables on `WriterDocument`.
//!
//! A table is a block whose kind is [`TABLE_BLOCK_KIND`] and whose text is a
//! Markdown table written by [`WriterTable::to_markdown`]. The block text is
//! the single source of truth: editing it edits the table, so no separate
//! model can desync. This module lives outside `lib.rs` to keep the crate
//! root within its registered byte ceiling.

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
            .sum::<usize>();
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
}

/// Index in `document.blocks` after which a table inserted at `offset` lands.
fn block_insert_index(document: &WriterDocument, offset: usize) -> usize {
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

/// Parses a Markdown table written by [`WriterTable::to_markdown`]. The
/// header separator row is skipped; escaped pipes restore to literal pipes.
pub fn parse_table_markdown(markdown: &str) -> WriterTable {
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut header_row = false;
    for line in markdown.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with('|') || !trimmed.ends_with('|') || trimmed.len() < 2 {
            continue;
        }
        let inner = &trimmed[1..trimmed.len() - 1];
        let cells: Vec<String> = inner
            .split('|')
            .map(|cell| cell.trim().replace("\\|", "|"))
            .collect();
        if cells.iter().all(|cell| {
            let compact = cell.replace(['-', ':', ' '], "");
            compact.is_empty() && !cell.is_empty()
        }) {
            header_row = true;
            continue;
        }
        rows.push(cells);
    }
    WriterTable {
        id: String::new(),
        rows,
        header_row,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TextSelection;

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
}
