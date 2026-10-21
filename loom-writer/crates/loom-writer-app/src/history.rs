//! Undo and redo history for the Writer editor.
//!
//! Each entry keeps the document before and after one edit. Unchanged text and
//! picture or chart payloads are shared between the two documents (`Text` and
//! the payload `Arc`s are reference-counted), so an entry is charged for what
//! it keeps alive, not for two full copies. The budget is enforced on those
//! charges. The newest edit is never dropped to meet it, so the last action can
//! always be undone, even if it alone is larger than the budget.
//!
//! Typing groups into one step per word: a space or punctuation mark typed
//! straight after a word starts a new step, and so do Enter, a caret jump and a
//! pause longer than [`TYPING_COALESCE_WINDOW_MS`].

use std::mem::size_of_val;
use std::sync::Arc;

use loom_writer_core::{RichBlock, WriterDocument};

/// Most undo and redo entries kept.
pub(crate) const HISTORY_MAX_ENTRIES: usize = 128;
/// Budget for undo and redo, measured by [`retained_bytes`]. A 400-page document
/// keeps well over twenty whole-document steps inside it.
pub(crate) const HISTORY_MAX_BYTES: usize = 256 * 1024 * 1024;
/// Typing this soon after the previous keystroke may share its undo step.
pub(crate) const TYPING_COALESCE_WINDOW_MS: u64 = 750;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HistoryKind {
    Typing,
    DocumentAction,
}

#[derive(Debug, Clone)]
pub(crate) struct HistoryEntry {
    pub(crate) before: WriterDocument,
    pub(crate) after: WriterDocument,
    pub(crate) kind: HistoryKind,
    pub(crate) last_edit_ms: u64,
    /// Where the last typing edit left the caret: block id and byte offset.
    caret: Option<(u64, usize)>,
    /// Bytes this entry keeps alive, measured when it is recorded.
    pub(crate) bytes: usize,
}

/// The one block a typing edit changed, and the bytes it replaced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BlockEdit {
    block: u64,
    start: usize,
    old_len: usize,
    new_len: usize,
    /// The character just before the edit.
    before_char: Option<char>,
    /// The first character the edit inserted.
    inserted_first: Option<char>,
}

impl BlockEdit {
    fn caret_after(&self) -> (u64, usize) {
        (self.block, self.start + self.new_len)
    }
}

/// The single block that differs between two documents, with its changed text
/// range. `None` for structural changes (Enter, pasted blocks), changes to
/// several blocks, and style-only changes.
fn single_block_edit(before: &WriterDocument, after: &WriterDocument) -> Option<BlockEdit> {
    if before.blocks.is_empty() && after.blocks.len() == 1 {
        // The first character typed into a blank document creates its paragraph.
        // That is still typing, so it is an insertion at the start of the new block.
        let block = &after.blocks[0];
        let text = block.text.as_str();
        let first = text.chars().next()?;
        return Some(BlockEdit {
            block: block.id,
            start: 0,
            old_len: 0,
            new_len: text.len(),
            before_char: None,
            inserted_first: Some(first),
        });
    }
    if before.blocks.len() != after.blocks.len() {
        return None;
    }
    let mut changed = None;
    for (old, new) in before.blocks.iter().zip(&after.blocks) {
        if old == new {
            continue;
        }
        if changed.is_some() || old.id != new.id {
            return None;
        }
        changed = Some((old, new));
    }
    let (old, new) = changed?;
    let (old_text, new_text) = (old.text.as_str(), new.text.as_str());
    if old_text == new_text {
        return None;
    }
    let prefix: usize = old_text
        .chars()
        .zip(new_text.chars())
        .take_while(|(a, b)| a == b)
        .map(|(a, _)| a.len_utf8())
        .sum();
    let suffix: usize = old_text[prefix..]
        .chars()
        .rev()
        .zip(new_text[prefix..].chars().rev())
        .take_while(|(a, b)| a == b)
        .map(|(a, _)| a.len_utf8())
        .sum();
    let start = prefix;
    let new_len = new_text.len() - prefix - suffix;
    Some(BlockEdit {
        block: new.id,
        start,
        old_len: old_text.len() - prefix - suffix,
        new_len,
        before_char: old_text[..start].chars().next_back(),
        inserted_first: new_text[start..start + new_len].chars().next(),
    })
}

/// Whether `edit` continues typing where the previous typing edit left the caret:
/// forward typing, or deleting backwards from there.
fn continues_typing(caret: Option<(u64, usize)>, edit: &BlockEdit) -> bool {
    matches!(
        caret,
        Some((block, offset))
            if block == edit.block && (edit.start == offset || edit.start + edit.old_len == offset)
    )
}

/// A space or punctuation mark typed straight after a word starts a new step, so
/// each step holds a word together with the separator that follows it.
fn begins_a_word_group(edit: &BlockEdit) -> bool {
    matches!(
        (edit.before_char, edit.inserted_first),
        (Some(previous), Some(next)) if previous.is_alphanumeric() && !next.is_alphanumeric()
    )
}

/// Bytes that a pair of documents keeps alive. Each document's strings, comments
/// and block records are copied, so they are counted in full. A block's text and
/// picture or chart payload is counted once, unless the other document holds the
/// same allocation for the block at the same position, which is how an edit that
/// leaves a block alone shares it. A block moved to another position is counted
/// again, so the figure errs high rather than low.
pub(crate) fn retained_bytes(before: &WriterDocument, after: &WriterDocument) -> usize {
    let overhead = |document: &WriterDocument| {
        let comments: usize = document
            .comments
            .iter()
            .map(|thread| {
                thread.id.len() + thread.author.len() + thread.body.len() + size_of_val(thread)
            })
            .sum();
        document.id.len() + document.title.len() + comments
    };
    overhead(before)
        + overhead(after)
        + before.blocks.iter().map(block_record_bytes).sum::<usize>()
        + after.blocks.iter().map(block_record_bytes).sum::<usize>()
        + before
            .blocks
            .iter()
            .map(|block| payload_bytes(block, None))
            .sum::<usize>()
        + after
            .blocks
            .iter()
            .enumerate()
            .map(|(index, block)| payload_bytes(block, before.blocks.get(index)))
            .sum::<usize>()
}

/// A block record: the struct itself, its kind string and its style runs.
fn block_record_bytes(block: &RichBlock) -> usize {
    size_of_val(block) + block.kind.capacity() + size_of_val(block.runs.as_slice())
}

/// The text, picture and chart bytes of `block` that `prior` (the same block
/// before the edit, if any) does not already hold.
fn payload_bytes(block: &RichBlock, prior: Option<&RichBlock>) -> usize {
    let prior = prior.filter(|prior| prior.id == block.id);
    let mut total = 0;
    let same_text =
        prior.is_some_and(|prior| prior.text.as_str().as_ptr() == block.text.as_str().as_ptr());
    if !same_text {
        total += block.text.len_bytes();
    }
    if let Some(image) = &block.image {
        let shared = prior
            .and_then(|prior| prior.image.as_ref())
            .is_some_and(|old| Arc::ptr_eq(&old.bytes, &image.bytes));
        if !shared {
            total += image.bytes.len();
        }
    }
    if let Some(chart) = &block.chart {
        let shared = prior
            .and_then(|prior| prior.chart.as_ref())
            .is_some_and(|old| Arc::ptr_eq(old, chart));
        if !shared {
            total += chart.len();
        }
    }
    total
}

#[derive(Debug)]
pub(crate) struct EditorHistory {
    pub(crate) undo: Vec<HistoryEntry>,
    pub(crate) redo: Vec<HistoryEntry>,
    max_entries: usize,
    max_bytes: usize,
    pub(crate) total_bytes: usize,
}

impl EditorHistory {
    pub(crate) fn new() -> Self {
        Self::with_budget(HISTORY_MAX_ENTRIES, HISTORY_MAX_BYTES)
    }

    pub(crate) fn with_budget(max_entries: usize, max_bytes: usize) -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            max_entries,
            max_bytes,
            total_bytes: 0,
        }
    }

    pub(crate) fn record(
        &mut self,
        before: WriterDocument,
        after: WriterDocument,
        kind: HistoryKind,
        now_ms: u64,
    ) {
        if before == after {
            return;
        }
        self.redo.clear();
        let edit = single_block_edit(&before, &after);
        if let (HistoryKind::Typing, Some(edit), Some(last)) = (kind, edit, self.undo.last_mut()) {
            if last.kind == HistoryKind::Typing
                && last.after == before
                && now_ms.saturating_sub(last.last_edit_ms) <= TYPING_COALESCE_WINDOW_MS
                && continues_typing(last.caret, &edit)
                && !begins_a_word_group(&edit)
            {
                last.bytes = retained_bytes(&last.before, &after);
                last.caret = Some(edit.caret_after());
                last.last_edit_ms = now_ms;
                last.after = after;
                self.recalculate_and_trim();
                return;
            }
        }
        let caret = match (kind, edit) {
            (HistoryKind::Typing, Some(edit)) => Some(edit.caret_after()),
            _ => None,
        };
        self.undo.push(HistoryEntry {
            bytes: retained_bytes(&before, &after),
            before,
            after,
            kind,
            last_edit_ms: now_ms,
            caret,
        });
        self.recalculate_and_trim();
    }

    pub(crate) fn undo(&mut self) -> Option<WriterDocument> {
        let entry = self.undo.pop()?;
        let before = entry.before.clone();
        self.redo.push(entry);
        Some(before)
    }

    pub(crate) fn redo(&mut self) -> Option<WriterDocument> {
        let entry = self.redo.pop()?;
        let after = entry.after.clone();
        self.undo.push(entry);
        Some(after)
    }

    #[cfg(test)]
    pub(crate) fn undo_len(&self) -> usize {
        self.undo.len()
    }

    #[cfg(test)]
    pub(crate) fn redo_len(&self) -> usize {
        self.redo.len()
    }

    /// Whether undo is currently available — authoritative for `edit.undo` enablement.
    pub(crate) fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// Whether redo is currently available — authoritative for `edit.redo` enablement.
    pub(crate) fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    fn entry_count(&self) -> usize {
        self.undo.len() + self.redo.len()
    }

    #[cfg(test)]
    pub(crate) fn total_bytes(&self) -> usize {
        self.total_bytes
    }

    /// Drops the oldest undo entries, then the oldest redo entries, until the
    /// budget holds. The newest undo entry is kept even above the budget.
    fn recalculate_and_trim(&mut self) {
        self.total_bytes = self.undo.iter().chain(&self.redo).map(|e| e.bytes).sum();
        while self.entry_count() > self.max_entries || self.total_bytes > self.max_bytes {
            if self.undo.len() > 1 {
                self.undo.remove(0);
            } else if !self.redo.is_empty() {
                self.redo.remove(0);
            } else {
                break;
            }
            self.total_bytes = self.undo.iter().chain(&self.redo).map(|e| e.bytes).sum();
        }
    }
}
