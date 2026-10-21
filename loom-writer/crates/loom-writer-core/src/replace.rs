//! Find and replace over document blocks.
//!
//! A search reports non-overlapping matches read left to right, and Replace All
//! replaces exactly those, so the count the user sees is the count replaced.
//! A table block is Markdown: its divider row is not text, and an unescaped
//! `|` is structure, not text. Neither can be matched or changed. A replacement
//! written into a table has its pipes escaped and its line breaks removed, so the
//! table keeps its rows and columns. Comment anchors rebase through every change,
//! so each thread keeps pointing at the same words.

use std::ops::Range;

use crate::{search, RichBlock, SearchMatch, Text, WriterDocument, TABLE_BLOCK_KIND};

/// Byte ranges of the matches of `query` in `block`, non-overlapping and left to
/// right. Matches inside a table block exclude its divider row and any match that
/// would cover a structural pipe.
pub(crate) fn block_matches(
    block: &RichBlock,
    query: &str,
    case_sensitive: bool,
) -> Vec<(usize, usize)> {
    let text = block.text.as_str();
    let candidates = candidate_matches(text, query, case_sensitive);
    let accepted = if block.kind == TABLE_BLOCK_KIND {
        let divider = divider_range(text);
        let bytes = text.as_bytes();
        candidates
            .into_iter()
            .filter(|&(start, end)| {
                let on_divider = divider
                    .as_ref()
                    .is_some_and(|divider| start < divider.end && end > divider.start);
                !on_divider && !has_structural_pipe(bytes, start, end)
            })
            .collect()
    } else {
        candidates
    };
    non_overlapping(accepted)
}

/// Every occurrence of `query` in `text`, sorted by start, overlaps included.
fn candidate_matches(text: &str, query: &str, case_sensitive: bool) -> Vec<(usize, usize)> {
    if query.is_empty() {
        return Vec::new();
    }
    if case_sensitive {
        return text
            .char_indices()
            .filter(|&(start, _)| text[start..].starts_with(query))
            .map(|(start, _)| (start, start + query.len()))
            .collect();
    }
    let needle = query.to_lowercase();
    if needle.is_ascii() && text.is_ascii() {
        // The common case: plain ASCII text and query, matched byte by byte.
        return search::ascii_case_insensitive(text, needle.as_bytes()).collect();
    }
    // Unicode lowercase can change byte length, so search by character windows
    // and map matches back to original UTF-8 byte offsets. Each start character
    // contributes its shortest matching window.
    let boundaries: Vec<usize> = text
        .char_indices()
        .map(|(index, _)| index)
        .chain(std::iter::once(text.len()))
        .collect();
    let mut found = Vec::new();
    for start_char in 0..boundaries.len().saturating_sub(1) {
        for end_char in start_char + 1..boundaries.len() {
            let candidate = &text[boundaries[start_char]..boundaries[end_char]];
            let lowered = candidate.to_lowercase();
            if lowered == needle {
                found.push((boundaries[start_char], boundaries[end_char]));
                break;
            }
            if lowered.len() > needle.len().saturating_mul(4).max(needle.len() + 8) {
                break;
            }
        }
    }
    found
}

/// Greedy left-to-right selection from matches sorted by start: a match is kept
/// only when it begins at or after the end of the last kept one.
fn non_overlapping(sorted: Vec<(usize, usize)>) -> Vec<(usize, usize)> {
    let mut kept: Vec<(usize, usize)> = Vec::with_capacity(sorted.len());
    for (start, end) in sorted {
        let after_last = match kept.last() {
            Some(&(_, last_end)) => start >= last_end,
            None => true,
        };
        if after_last {
            kept.push((start, end));
        }
    }
    kept
}

/// Byte range of a table block's divider row: its second line, when it has one.
fn divider_range(text: &str) -> Option<Range<usize>> {
    let start = text.find('\n')? + 1;
    let end = text[start..]
        .find('\n')
        .map_or(text.len(), |offset| start + offset);
    Some(start..end)
}

/// Whether `start..end` covers an unescaped `|`, which separates table cells.
fn has_structural_pipe(bytes: &[u8], start: usize, end: usize) -> bool {
    (start..end).any(|index| bytes[index] == b'|' && (index == 0 || bytes[index - 1] != b'\\'))
}

/// Replacement text as it must be stored in a table cell: every unescaped pipe
/// is escaped and every line break becomes a space.
fn table_cell_text(replacement: &str) -> String {
    let mut out = String::with_capacity(replacement.len());
    let mut escaped = false;
    for c in replacement.chars() {
        match c {
            '\n' => out.push(' '),
            '|' if !escaped => out.push_str("\\|"),
            other => out.push(other),
        }
        escaped = c == '\\' && !escaped;
    }
    out
}

/// Every match in the document, in block order.
pub(crate) fn find_all(
    document: &WriterDocument,
    query: &str,
    case_sensitive: bool,
) -> Vec<SearchMatch> {
    document
        .blocks
        .iter()
        .flat_map(|block| {
            block_matches(block, query, case_sensitive)
                .into_iter()
                .map(move |(start, end)| SearchMatch {
                    block_id: block.id,
                    start,
                    end,
                })
        })
        .collect()
}

/// Replaces every match of `query` and returns how many were replaced.
pub(crate) fn replace_all(
    document: &mut WriterDocument,
    query: &str,
    replacement: &str,
    case_sensitive: bool,
) -> usize {
    let mut replacements = 0;
    for index in 0..document.blocks.len() {
        let spans = block_matches(&document.blocks[index], query, case_sensitive);
        if spans.is_empty() {
            continue;
        }
        replacements += spans.len();
        let insert = if document.blocks[index].kind == TABLE_BLOCK_KIND {
            table_cell_text(replacement)
        } else {
            replacement.to_string()
        };
        let before = document.blocks[index].text.as_str().to_string();
        let changes: Vec<(usize, usize, usize)> = spans
            .iter()
            .map(|&(start, end)| (start, end, insert.len()))
            .collect();
        let block_id = document.blocks[index].id;
        rebase_block_comments(document, block_id, &before, &changes);
        // One replacement at a time, last first, so earlier offsets stay valid.
        // The style runs are remapped around each change, so text between two
        // matches keeps its style.
        let mut current = before.clone();
        let mut runs = document.blocks[index].runs.clone();
        for &(start, end) in spans.iter().rev() {
            let mut next = current.clone();
            next.replace_range(start..end, &insert);
            runs = crate::remap_style_runs(&current, &next, &runs);
            current = next;
        }
        if current != before {
            let block = &mut document.blocks[index];
            block.runs = runs;
            block.text = Text::from_str(&current);
        }
    }
    replacements
}

/// Maps each comment thread in `block_id` through the changes made to that block.
/// `changes` are `(start, end, inserted_len)` in ascending order and are measured
/// in the block's text before the edit (`old_text`).
fn rebase_block_comments(
    document: &mut WriterDocument,
    block_id: u64,
    old_text: &str,
    changes: &[(usize, usize, usize)],
) {
    for thread in document
        .comments
        .iter_mut()
        .filter(|thread| thread.block_id == block_id && !thread.orphaned)
    {
        let valid = thread.start <= thread.end
            && thread.end <= old_text.len()
            && old_text.is_char_boundary(thread.start)
            && old_text.is_char_boundary(thread.end);
        if !valid {
            thread.orphaned = true;
            thread.start = 0;
            thread.end = 0;
            continue;
        }
        let start = map_offset(thread.start, changes, true);
        let end = map_offset(thread.end, changes, false).max(start);
        if thread.start < thread.end && start == end {
            // Every word the thread was attached to is gone.
            thread.orphaned = true;
            thread.start = 0;
            thread.end = 0;
            continue;
        }
        thread.start = start;
        thread.end = end;
    }
}

/// The new offset of an old one. An offset before a change stays put, one after
/// it moves by the change's growth, and one strictly inside a replaced span goes
/// to the start (for a comment's start) or the end (for its end) of the
/// replacement.
fn map_offset(offset: usize, changes: &[(usize, usize, usize)], is_start: bool) -> usize {
    let mut shift: isize = 0;
    for &(start, end, inserted) in changes {
        if offset <= start {
            break;
        }
        if offset >= end {
            shift += inserted as isize - (end - start) as isize;
            continue;
        }
        let base = start as isize + shift;
        return if is_start {
            base as usize
        } else {
            (base + inserted as isize) as usize
        };
    }
    (offset as isize + shift) as usize
}

#[cfg(test)]
#[path = "replace_tests.rs"]
mod tests;
