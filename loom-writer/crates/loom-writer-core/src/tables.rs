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

/// Splits a row body on pipes that are not escaped with a backslash, so a
/// cell holding a literal `|` (written `\|`) stays in one cell.
fn split_cells(inner: &str) -> Vec<&str> {
    let mut cells = Vec::new();
    let mut start = 0;
    let mut escaped = false;
    for (index, c) in inner.char_indices() {
        match c {
            '\\' => escaped = !escaped,
            '|' if !escaped => {
                cells.push(&inner[start..index]);
                start = index + 1;
            }
            _ => escaped = false,
        }
    }
    cells.push(&inner[start..]);
    cells
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
        let cells: Vec<String> = split_cells(inner)
            .into_iter()
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

/// Calls `visit` with each piece of a block's text that counts toward the word
/// and character totals. A paragraph counts whole. A table counts each cell's
/// text, so its pipes, padding and header separator are not words.
pub(crate) fn for_each_counted_text(kind: &str, text: &str, mut visit: impl FnMut(&str)) {
    if kind == TABLE_BLOCK_KIND {
        for cell in parse_table_markdown(text).rows.iter().flatten() {
            visit(cell.as_str());
        }
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
        let content: String = pdf.iter().map(|&byte| char::from(byte)).collect();
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
}
