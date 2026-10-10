//! The divider row of a Markdown table is not a cell. The caret skips it, and an
//! edit that would change it is refused, so it stays a divider.

use loom_writer_core::{WriterDocument, TABLE_BLOCK_KIND};
use std::ops::Range;

/// Byte ranges, in editor text, of the divider rows of the document's tables.
/// A table block's divider is its second line; the first line is the header.
pub(crate) fn divider_ranges(document: &WriterDocument) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut block_start = 0;
    for block in &document.blocks {
        let text = block.text.as_str();
        if block.kind == TABLE_BLOCK_KIND {
            if let Some(header_end) = text.find('\n') {
                let start = header_end + 1;
                let end = text[start..]
                    .find('\n')
                    .map_or(text.len(), |offset| start + offset);
                ranges.push(block_start + start..block_start + end);
            }
        }
        // Blocks are joined by one newline in the editor text.
        block_start += text.len() + 1;
    }
    ranges
}

/// The caret to use after a move from `previous` to `caret`. A caret that lands
/// on a divider moves on past it in the direction of the move; a move up lands
/// at the end of the header row.
pub(crate) fn skip_divider(document: &WriterDocument, previous: usize, caret: usize) -> usize {
    let Some(divider) = divider_ranges(document)
        .into_iter()
        .find(|range| range.start <= caret && caret <= range.end)
    else {
        return caret;
    };
    let line_below_exists = divider.end < document.editor_text().len();
    if previous < caret && line_below_exists {
        divider.end + 1
    } else {
        divider.start.saturating_sub(1)
    }
}

/// The byte offsets, in editor text, at which the document's table blocks end.
pub(crate) fn table_ends(document: &WriterDocument) -> Vec<usize> {
    let mut ends = Vec::new();
    let mut block_start = 0;
    for block in &document.blocks {
        let text = block.text.as_str();
        if block.kind == TABLE_BLOCK_KIND {
            ends.push(block_start + text.len());
        }
        block_start += text.len() + 1;
    }
    ends
}

/// Text typed right after a table's last row starts a paragraph of its own: a
/// line break goes before it, so the table keeps its rows. Returns the edited
/// text and its caret, or `None` when the edit does not insert text at a table's
/// end. Typing a line break there already makes a new block, so it is left alone.
pub(crate) fn break_after_table(
    old: &str,
    new: &str,
    table_ends: &[usize],
    caret: usize,
) -> Option<(String, usize)> {
    let prefix = old
        .bytes()
        .zip(new.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    let longest_suffix = old.len().min(new.len()) - prefix;
    let suffix = old
        .bytes()
        .rev()
        .zip(new.bytes().rev())
        .take_while(|(a, b)| a == b)
        .count()
        .min(longest_suffix);
    let inserted_end = new.len() - suffix;
    if inserted_end <= prefix || !table_ends.contains(&prefix) {
        return None;
    }
    let (head, typed) = (new.get(..prefix)?, new.get(prefix..)?);
    if typed.starts_with('\n') {
        return None;
    }
    let mut edited = String::with_capacity(new.len() + 1);
    edited.push_str(head);
    edited.push('\n');
    edited.push_str(typed);
    Some((edited, caret + 1))
}

/// Whether replacing `old` with `new` would change a divider row, that is,
/// whether the changed span reaches a divider. Typing at the end of a header row
/// or anywhere in a data row leaves the divider alone.
pub(crate) fn edit_touches_divider(old: &str, new: &str, dividers: &[Range<usize>]) -> bool {
    if old == new {
        return false;
    }
    let prefix = old
        .bytes()
        .zip(new.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    let longest_suffix = old.len().min(new.len()) - prefix;
    let suffix = old
        .bytes()
        .rev()
        .zip(new.bytes().rev())
        .take_while(|(a, b)| a == b)
        .count()
        .min(longest_suffix);
    let changed_end = old.len() - suffix;
    dividers
        .iter()
        .any(|divider| prefix <= divider.end && changed_end >= divider.start)
}
