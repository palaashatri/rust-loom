//! Migration of table blocks written before typed storage (document format 1).
//!
//! Those blocks hold Markdown: pipes, a divider row that carries each column's
//! alignment, emphasis markers, and a `<!-- loom-table: ... -->` metadata line.
//! [`migrate`] reads them with the rules the previous Writer used, so each cell
//! keeps its text, emphasis and alignment, and returns the typed table with a map
//! from old source offsets to the flat editor offsets.

use loom_objects::table::{CellStyleOverride, HorizontalAlign, Table};

use super::markup;
use super::store::TableTextMap;
use crate::{CommentThread, RichBlock, TextSelection};

/// One cell of a legacy row: its text with escapes decoded, and for every byte
/// boundary of that text the byte offset it came from in the source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParsedTableCellText {
    pub(crate) text: String,
    pub(crate) source_offsets: Vec<usize>,
}

struct ParsedTableLine {
    cells: Vec<ParsedTableCellText>,
    is_delimiter: bool,
}

/// Maps a legacy source offset to an offset in the migrated editor text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourceMap {
    /// `(source offset, flat offset)` for every cell byte boundary, in order.
    pairs: Vec<(usize, usize)>,
    flat_len: usize,
}

impl SourceMap {
    /// The flat offset for a source offset. A position between cells (a pipe,
    /// the divider or the metadata) moves to the start of the next cell, and one
    /// past the last cell to the end of the table.
    pub(crate) fn flat_offset(&self, source: usize) -> usize {
        let index = self.pairs.partition_point(|&(at, _)| at < source);
        self.pairs
            .get(index)
            .map_or(self.flat_len, |&(_, flat)| flat)
    }
}

/// A migrated table block: the typed table and the offset map.
pub(crate) struct Migrated {
    pub(crate) table: Table,
    pub(crate) map: SourceMap,
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

/// Applies the parser's trim and `\|` decoding while keeping the source offset
/// of every byte boundary in the decoded text.
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

/// Parses one legacy table line. Text before the first pipe joins the first
/// cell and text after the last pipe joins the last cell, as the old reader did.
fn parse_table_line_raw(line: &str, line_offset: usize) -> Option<ParsedTableLine> {
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
    let last_pipe = trimmed.rfind('|')?;
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
        let prefix = mapped_cell_text(prefix, trimmed_offset);
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

/// Table options carried by the `<!-- loom-table: ... -->` line.
struct LegacyOptions {
    style_id: String,
    banded_rows: bool,
    first_column: bool,
    merges: Vec<(usize, usize, usize, usize)>,
}

fn read_options(trimmed: &str, options: &mut LegacyOptions) {
    let inner = trimmed
        .trim_start_matches("<!-- loom-table:")
        .trim_end_matches("-->")
        .trim();
    for part in inner.split_whitespace() {
        let Some((key, value)) = part.split_once('=') else {
            continue;
        };
        match key {
            "style" => options.style_id = value.to_string(),
            "banded" => options.banded_rows = value == "1",
            "first_col" => options.first_column = value == "1",
            "merges" => {
                for merge in value.split(';') {
                    let numbers: Vec<usize> = merge
                        .split(',')
                        .filter_map(|number| number.parse().ok())
                        .collect();
                    if let [row, column, rows, columns] = numbers[..] {
                        options.merges.push((row, column, rows, columns));
                    }
                }
            }
            _ => {}
        }
    }
}

/// Migrates the Markdown text of a format-1 table block. Each cell's emphasis
/// becomes runs; each column's divider alignment becomes the horizontal
/// alignment of its cells; the stored merges are applied as the old reader did.
pub(crate) fn migrate(markdown: &str, block_id: u64) -> Migrated {
    let mut rows: Vec<Vec<ParsedTableCellText>> = Vec::new();
    let mut alignments: Option<Vec<HorizontalAlign>> = None;
    let mut options = LegacyOptions {
        style_id: "table-style-medium-1".to_string(),
        banded_rows: true,
        first_column: false,
        merges: Vec::new(),
    };
    // The divider is the second table line, as the editor always drew it. A row of
    // dashes anywhere else (a header such as `| - | - |`) is cell text.
    let mut table_line = 0usize;
    for_each_markdown_line(markdown, |line, line_offset| {
        let trimmed = line.trim();
        if is_table_metadata_line(trimmed) {
            read_options(trimmed, &mut options);
            return;
        }
        let Some(parsed) = parse_table_line_raw(line, line_offset) else {
            return;
        };
        table_line += 1;
        if parsed.is_delimiter && table_line == 2 {
            alignments = Some(
                parsed
                    .cells
                    .iter()
                    .map(|cell| markup::divider_alignment(&cell.text))
                    .collect(),
            );
        } else {
            rows.push(parsed.cells);
        }
    });

    let columns = rows.iter().map(Vec::len).max().unwrap_or(0).max(1);
    let mut table = Table::new(rows.len().max(1), columns);
    table.id = format!("table-{block_id}");
    table.header_row = alignments.is_some();
    table.style_id = options.style_id;
    table.banded_rows = options.banded_rows;
    table.first_column = options.first_column;
    for (row, cells) in rows.iter().enumerate() {
        for column in 0..columns {
            let text = cells
                .get(column)
                .map_or_else(Vec::new, |cell| markup::runs(&cell.text));
            table.set_cell_rich_text(row, column, text);
            if let Some(align) = alignments.as_ref().and_then(|a| a.get(column)) {
                if let Some(cell) = table.cell_mut(row, column) {
                    cell.style_override = Some(CellStyleOverride {
                        horizontal_align: Some(*align),
                        ..Default::default()
                    });
                }
            }
        }
    }
    for &(row, column, row_span, column_span) in &options.merges {
        let _ = table.merge_cells(row, column, row_span, column_span);
    }

    let positions = TableTextMap::new(&table);
    let mut pairs = Vec::new();
    for (row, cells) in rows.iter().enumerate() {
        for (column, cell) in cells.iter().enumerate().take(columns) {
            if table.is_covered(row, column) {
                continue;
            }
            let Some(range) = positions.cell_range(row, column) else {
                continue;
            };
            let display = markup::display(cell.clone());
            for (index, &source) in display.source_offsets.iter().enumerate() {
                pairs.push((source, range.start + index));
            }
        }
    }
    Migrated {
        table,
        map: SourceMap {
            pairs,
            flat_len: positions.len(),
        },
    }
}

/// A block as it was before typed storage: the length of its legacy text and,
/// for a migrated table, the offset map from that text.
#[derive(Debug, Clone)]
pub(crate) struct LegacyBlock {
    pub(crate) old_len: usize,
    pub(crate) map: Option<SourceMap>,
}

/// Moves comment anchors and the selection from the legacy editor text onto the
/// migrated blocks. Blocks without a migrated table keep their offsets.
pub(crate) fn remap_legacy_offsets(
    blocks: &[RichBlock],
    legacy: &[LegacyBlock],
    comments: &mut [CommentThread],
    selection: &mut TextSelection,
) {
    if legacy.iter().all(|block| block.map.is_none()) {
        return;
    }
    let mut old_starts = Vec::with_capacity(blocks.len());
    let mut new_starts = Vec::with_capacity(blocks.len());
    let (mut old_start, mut new_start) = (0usize, 0usize);
    for (block, legacy_block) in blocks.iter().zip(legacy) {
        old_starts.push(old_start);
        new_starts.push(new_start);
        old_start += legacy_block.old_len + 1;
        new_start += block.text.len_bytes() + 1;
    }
    let map_global = |offset: usize| -> usize {
        let index = old_starts
            .partition_point(|&start| start <= offset)
            .saturating_sub(1);
        if index >= blocks.len() {
            return new_start;
        }
        let local = offset - old_starts[index];
        let local = match &legacy[index].map {
            Some(map) => map.flat_offset(local),
            None => local.min(blocks[index].text.len_bytes()),
        };
        new_starts[index] + local
    };
    selection.anchor = map_global(selection.anchor);
    selection.focus = map_global(selection.focus);
    for comment in comments.iter_mut() {
        let Some(index) = blocks.iter().position(|block| block.id == comment.block_id) else {
            continue;
        };
        if let Some(map) = &legacy[index].map {
            comment.start = map.flat_offset(comment.start);
            comment.end = map.flat_offset(comment.end).max(comment.start);
        }
    }
}
