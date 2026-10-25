//! Table blocks on `WriterDocument`.
//!
//! A table is a block whose kind is [`TABLE_BLOCK_KIND`] and whose cells are a
//! typed `loom_objects::Table` held by the block. The block's text is derived
//! from those cells (see `store`), so the editor shows a table as one line of
//! text with control separators between cells and rows. Documents written
//! before typed storage hold Markdown instead; `legacy_markdown` migrates them
//! once on load. This module lives outside `lib.rs` to keep the crate root
//! within its registered byte ceiling.

use crate::comments::TextEdit;
use crate::{RichBlock, WriterDocument};

mod editing;
mod legacy_markdown;
mod markdown_export;
mod markup;
mod ops;
mod store;

pub(crate) use markdown_export::{table_html, table_markdown};

/// A table block built from Markdown table text, migrated exactly as a document
/// written before typed storage is migrated on load.
pub fn markdown_table_block(id: u64, markdown: &str) -> RichBlock {
    RichBlock::table_block(id, legacy_markdown::migrate(markdown, id).table)
}

/// A block of any kind for a test fixture: a table kind is migrated from its
/// Markdown text, every other kind is a plain block.
#[cfg(test)]
pub(crate) fn test_block(id: u64, kind: &str, text: &str) -> RichBlock {
    if kind == TABLE_BLOCK_KIND {
        markdown_table_block(id, text)
    } else {
        RichBlock::new(id, kind, text)
    }
}
#[cfg(test)]
mod ops_tests;
#[cfg(test)]
mod row_height_tests;
#[cfg(test)]
mod typed_tests;

pub use editing::apply_flat_text;
pub(crate) use editing::flat_after_typing;
pub(crate) use legacy_markdown::{migrate, remap_legacy_offsets, LegacyBlock};
pub(crate) use store::table_row_spans;
pub use store::{
    flat_table_text, is_table_separator, TableTextMap, TABLE_CELL_SEPARATOR, TABLE_ROW_SEPARATOR,
};

/// Block kind for table blocks.
pub const TABLE_BLOCK_KIND: &str = "table";

/// The number of lines the tallest cell of a table block takes: one, plus the
/// most soft line breaks in any single cell. Every row of the table shares this
/// height, so a cell with a line break makes the whole table's rows taller.
pub fn table_line_count(text: &str) -> usize {
    text.split(is_table_separator)
        .map(|cell| cell.matches('\u{b}').count())
        .max()
        .unwrap_or(0)
        + 1
}

/// The height of one table row that holds `lines` lines in its tallest cell: a
/// one-line row is the page's body line with cell padding (at least 24 pt), and
/// each further line adds one body line.
pub fn table_row_height(style: &crate::PageStyle, lines: usize) -> f32 {
    let body_line = style.body_font_size_pt * style.line_height;
    let one_line = (body_line + 8.0).max(24.0);
    one_line + lines.saturating_sub(1) as f32 * body_line
}

/// Default geometry for a table inserted from the UI.
pub const INSERT_ROWS: usize = 3;
pub const INSERT_COLUMNS: usize = 3;

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

/// Calls `visit` with each piece of a block's text that counts toward the word
/// and character totals. A paragraph counts whole. A table counts each cell's
/// text, so its separators and grid are not words.
pub(crate) fn for_each_counted_text(block: &RichBlock, mut visit: impl FnMut(&str)) {
    if block.kind == TABLE_BLOCK_KIND {
        for row in block
            .typed_table()
            .into_iter()
            .flat_map(|table| table.rows.iter())
        {
            for cell in &row.cells {
                visit(&cell.content.text());
            }
        }
    } else if block.kind == crate::IMAGE_BLOCK_KIND || block.kind == crate::CHART_BLOCK_KIND {
        // A picture is one object, not words.
    } else {
        visit(block.text.as_str());
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
