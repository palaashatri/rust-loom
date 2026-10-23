//! Find and replace over document blocks.
//!
//! A search reports non-overlapping matches read left to right, and Replace All
//! replaces exactly those, so the count the user sees is the count replaced.
//! A table block is Markdown: its divider row is not text, and an unescaped
//! `|` is structure, not text. Neither can be matched or changed. A replacement
//! written into a table has its pipes escaped and its line breaks removed, so the
//! table keeps its rows and columns. Comment anchors rebase through every change,
//! so each thread keeps pointing at the same words.

use crate::tables::{is_table_separator, TABLE_BLOCK_KIND};
use crate::{search, RichBlock, SearchMatch, Text, WriterDocument};

/// Byte ranges of the matches of `query` in `block`, non-overlapping and left to
/// right. A table block is searched as its flat text, whose cell and row
/// separators never match a query, so a match stays inside one cell.
pub(crate) fn block_matches(
    block: &RichBlock,
    query: &str,
    options: FindOptions,
) -> Vec<(usize, usize)> {
    let text = block.text.as_str();
    let mut candidates = candidate_matches(text, query, options.case_sensitive);
    if options.whole_word {
        candidates.retain(|&(start, end)| is_whole_word(text, query, start, end));
    }
    non_overlapping(candidates)
}

/// How a search matches: both default to off (case-insensitive, any substring).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FindOptions {
    pub case_sensitive: bool,
    /// A match must not touch a letter, digit or underscore on a side where the
    /// query itself begins or ends with one, so `cat` skips `concatenate`
    /// while a query ending in a space still matches before any letter.
    pub whole_word: bool,
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Whether `start..end` of `text` sits on word boundaries for `query`.
fn is_whole_word(text: &str, query: &str, start: usize, end: usize) -> bool {
    let starts_word = query.chars().next().is_some_and(is_word_char);
    let ends_word = query.chars().next_back().is_some_and(is_word_char);
    let before_ok = !starts_word || !text[..start].chars().next_back().is_some_and(is_word_char);
    let after_ok = !ends_word || !text[end..].chars().next().is_some_and(is_word_char);
    before_ok && after_ok
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

/// Every match in the document, in block order.
pub(crate) fn find_all(
    document: &WriterDocument,
    query: &str,
    case_sensitive: bool,
) -> Vec<SearchMatch> {
    find_all_with(
        document,
        query,
        FindOptions {
            case_sensitive,
            whole_word: false,
        },
    )
}

/// Every match under `options`, in block order.
pub(crate) fn find_all_with(
    document: &WriterDocument,
    query: &str,
    options: FindOptions,
) -> Vec<SearchMatch> {
    document
        .blocks
        .iter()
        .flat_map(|block| {
            block_matches(block, query, options)
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
    let options = FindOptions {
        case_sensitive,
        whole_word: false,
    };
    replace_all_with(document, query, replacement, options)
}

/// Replaces every match under `options` and returns how many were replaced.
pub(crate) fn replace_all_with(
    document: &mut WriterDocument,
    query: &str,
    replacement: &str,
    options: FindOptions,
) -> usize {
    let mut replacements = 0;
    for index in 0..document.blocks.len() {
        let spans = block_matches(&document.blocks[index], query, options);
        if spans.is_empty() {
            continue;
        }
        replacements += spans.len();
        let insert = if document.blocks[index].kind == TABLE_BLOCK_KIND {
            cell_replacement(replacement)
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
            if document.blocks[index].kind == TABLE_BLOCK_KIND {
                // The replacement keeps the grid (no separators, no line breaks),
                // so the cells take the change and keep their own runs.
                let _ = document.edit_table_text(block_id, &current);
            } else {
                let block = &mut document.blocks[index];
                block.runs = runs;
                block.text = Text::from_str(&current);
            }
        }
    }
    replacements
}

/// Replacement text as a table cell holds it: separators are dropped and line
/// breaks become spaces.
fn cell_replacement(replacement: &str) -> String {
    replacement
        .chars()
        .filter(|c| !is_table_separator(*c))
        .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
        .collect()
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

impl WriterDocument {
    /// Finds literal text in every block under `options`.
    pub fn find_all_with(&self, query: &str, options: FindOptions) -> Vec<SearchMatch> {
        find_all_with(self, query, options)
    }

    /// Replaces every match under `options` and returns how many were replaced.
    pub fn replace_all_with(
        &mut self,
        query: &str,
        replacement: &str,
        options: FindOptions,
    ) -> usize {
        let replacements = replace_all_with(self, query, replacement, options);
        self.selection = self.clamp_selection(self.selection.clone());
        replacements
    }
}

#[cfg(test)]
#[path = "replace_tests.rs"]
mod tests;
