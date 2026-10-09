//! Document comments: anchored threads on `WriterDocument`.
//!
//! Extends `WriterDocument` in its own module so `lib.rs` stays within its
//! registered byte ceiling. Threads anchor to a stable block id plus a UTF-8
//! byte range. Text edits rebase each range so it remains attached to the
//! selected words. If those words disappear completely, the thread remains
//! visible as an explicitly orphaned review item.

use crate::{
    apply_global_character_style, changed_text_ranges, character_style_at_global,
    normalize_editor_text, pkg_json, CommentThread, ContentParser, JsonValue, RichBlock,
    WriterDocument, WriterError,
};

/// One contiguous edit in editor-text byte offsets: the old range
/// `old_start..old_end` was replaced by the new range `new_start..new_end`.
/// Knowing the real edit lets comment anchors rebase correctly where a
/// before/after diff alone is ambiguous (for example typing a copy of a word
/// next to the word itself).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TextEdit {
    pub old_start: usize,
    pub old_end: usize,
    pub new_start: usize,
    pub new_end: usize,
}

impl TextEdit {
    /// Infer the edit from old and new text with no further knowledge.
    pub(crate) fn diff(old_text: &str, new_text: &str) -> Self {
        let (old_start, old_end, new_start, new_end) = changed_text_ranges(old_text, new_text);
        Self {
            old_start,
            old_end,
            new_start,
            new_end,
        }
    }

    /// Place a pure insertion or pure deletion using the caret the editor
    /// reports after the edit. Returns `None` when the caret does not explain
    /// the change, so the caller falls back to [`Self::diff`].
    fn from_caret(old_text: &str, new_text: &str, caret: usize) -> Option<Self> {
        if caret > new_text.len() || !new_text.is_char_boundary(caret) {
            return None;
        }
        if new_text.len() > old_text.len() {
            let inserted = new_text.len() - old_text.len();
            let start = caret.checked_sub(inserted)?;
            let explained = old_text.is_char_boundary(start)
                && new_text.get(..start)? == old_text.get(..start)?
                && new_text.get(caret..)? == old_text.get(start..)?;
            explained.then_some(Self {
                old_start: start,
                old_end: start,
                new_start: start,
                new_end: caret,
            })
        } else if new_text.len() < old_text.len() {
            let end = caret + (old_text.len() - new_text.len());
            let explained = old_text.is_char_boundary(end)
                && new_text.get(..caret)? == old_text.get(..caret)?
                && new_text.get(caret..)? == old_text.get(end..)?;
            explained.then_some(Self {
                old_start: caret,
                old_end: end,
                new_start: caret,
                new_end: caret,
            })
        } else {
            None
        }
    }
}

/// The display name used for comments created locally (Loom is local-first
/// and has no accounts).
pub const LOCAL_AUTHOR: &str = "You";

impl WriterDocument {
    /// Anchors a new comment thread to the given block and byte range.
    ///
    /// Returns the generated comment id. The next comment id is derived from
    /// the existing threads so ids stay unique without a clock.
    pub fn add_comment_thread(
        &mut self,
        block_id: u64,
        start: usize,
        end: usize,
        body: &str,
    ) -> Result<String, String> {
        let body = body.trim();
        if body.is_empty() {
            return Err("comment body must not be empty".into());
        }
        let block = self
            .blocks
            .iter()
            .find(|block| block.id == block_id)
            .ok_or_else(|| format!("unknown block {block_id}"))?;
        let end = end.min(block.text.len_bytes());
        if start > end {
            return Err("comment range is inverted".into());
        }
        if !block.text.as_str().is_char_boundary(start)
            || !block.text.as_str().is_char_boundary(end)
        {
            return Err("comment range is not a valid UTF-8 block range".into());
        }
        let mut next = 1u32;
        loop {
            let id = format!("comment-{next}");
            if self.comments.iter().any(|c| c.id == id) {
                next += 1;
                continue;
            }
            self.comments.push(CommentThread {
                id: id.clone(),
                author: LOCAL_AUTHOR.to_string(),
                block_id,
                start,
                end,
                body: body.to_string(),
                resolved: false,
                orphaned: false,
            });
            return Ok(id);
        }
    }

    /// Resolves or reopens a thread. Returns whether the id was found.
    pub fn set_comment_thread_resolved(&mut self, id: &str, resolved: bool) -> bool {
        match self.comments.iter_mut().find(|c| c.id == id) {
            Some(thread) => {
                thread.resolved = resolved;
                true
            }
            None => false,
        }
    }

    /// Deletes a thread. Returns whether the id was found.
    pub fn remove_comment_thread(&mut self, id: &str) -> bool {
        let before = self.comments.len();
        self.comments.retain(|c| c.id != id);
        self.comments.len() != before
    }

    /// Rebase comment ranges through the one contiguous text change reported by
    /// the editor. If a paragraph split makes a range cross blocks, keep the
    /// surviving part in the block where the comment begins. If all of the
    /// selected text disappears, keep the thread visible and mark it orphaned
    /// instead of attaching it to unrelated words.
    pub(crate) fn rebase_comment_anchors(
        comments: &mut [CommentThread],
        old_blocks: &[RichBlock],
        new_blocks: &[RichBlock],
    ) {
        if comments.is_empty() {
            return;
        }
        let old_text = blocks_to_text(old_blocks);
        let new_text = blocks_to_text(new_blocks);
        if old_text == new_text {
            return;
        }
        let edit = TextEdit::diff(&old_text, &new_text);
        Self::rebase_comment_anchors_through(comments, old_blocks, new_blocks, edit);
    }

    /// Rebase comment ranges through an explicit edit.
    pub(crate) fn rebase_comment_anchors_through(
        comments: &mut [CommentThread],
        old_blocks: &[RichBlock],
        new_blocks: &[RichBlock],
        edit: TextEdit,
    ) {
        let TextEdit {
            old_start: old_change_start,
            old_end: old_change_end,
            new_start: new_change_start,
            new_end: new_change_end,
        } = edit;

        for comment in comments {
            if comment.orphaned {
                continue;
            }
            let Some((old_block_start, old_block)) = block_start(old_blocks, comment.block_id)
            else {
                comment.orphaned = true;
                comment.start = 0;
                comment.end = 0;
                continue;
            };
            if comment.start > comment.end
                || comment.end > old_block.text.len_bytes()
                || !old_block.text.as_str().is_char_boundary(comment.start)
                || !old_block.text.as_str().is_char_boundary(comment.end)
            {
                comment.orphaned = true;
                comment.start = 0;
                comment.end = 0;
                continue;
            }

            let old_start = old_block_start + comment.start;
            let old_end = old_block_start + comment.end;
            let new_start = map_comment_endpoint(
                old_start,
                old_change_start,
                old_change_end,
                new_change_start,
                new_change_end,
                true,
            );
            let new_end = map_comment_endpoint(
                old_end,
                old_change_start,
                old_change_end,
                new_change_start,
                new_change_end,
                false,
            )
            .max(new_start);

            let Some((new_block, local_start)) = block_at_offset(new_blocks, new_start) else {
                comment.orphaned = true;
                comment.start = 0;
                comment.end = 0;
                continue;
            };
            comment.block_id = new_block.id;
            comment.start = local_start.min(new_block.text.len_bytes());
            comment.end = match block_at_offset(new_blocks, new_end) {
                Some((end_block, local_end)) if end_block.id == new_block.id => local_end,
                Some(_) => new_block.text.len_bytes(),
                None => new_block.text.len_bytes(),
            }
            .max(comment.start)
            .min(new_block.text.len_bytes());

            if old_start < old_end && comment.start == comment.end {
                comment.orphaned = true;
            }
        }
    }

    /// `replace_paragraphs` for a caller that knows the exact edit: comment
    /// anchors rebase through `edit` instead of through an inferred diff.
    pub(crate) fn replace_paragraphs_with_edit(&mut self, plain_text: &str, edit: TextEdit) {
        let old_blocks = self.rebuild_blocks(plain_text, Some(edit));
        if self.comments.is_empty() {
            return;
        }
        Self::rebase_comment_anchors_through(&mut self.comments, &old_blocks, &self.blocks, edit);
    }

    /// Replace the editor's canonical text after a native text-buffer edit.
    /// The single changed range is inferred from old/new text, using the
    /// caret reported by the editor to place pure insertions and deletions
    /// exactly. Inserted content inherits the style at its old caret.
    pub fn replace_editor_text_at(
        &mut self,
        new_text: &str,
        caret: Option<usize>,
    ) -> Result<bool, WriterError> {
        let new_text = normalize_editor_text(new_text);
        let old_text = self.editor_text();
        if old_text == new_text {
            self.set_selection(self.selection.clone());
            return Ok(false);
        }
        let edit = caret
            .and_then(|caret| TextEdit::from_caret(&old_text, &new_text, caret))
            .unwrap_or_else(|| TextEdit::diff(&old_text, &new_text));
        let inherited = character_style_at_global(self, edit.old_start, self.selection.affinity);
        self.replace_paragraphs_with_edit(&new_text, edit);
        apply_global_character_style(self, edit.new_start, edit.new_end, inherited);
        self.set_selection(self.selection.clone());
        Ok(true)
    }

    /// Replace the editor's canonical text with only the old/new diff to go on.
    pub fn replace_editor_text(&mut self, new_text: &str) -> Result<bool, WriterError> {
        self.replace_editor_text_at(new_text, None)
    }

    /// Comment threads whose text anchor still exists in the document.
    /// Orphaned threads remain available in `comments` for review.
    pub fn live_comment_threads(&self) -> Vec<&CommentThread> {
        self.comments
            .iter()
            .filter(|thread| {
                !thread.orphaned && self.blocks.iter().any(|block| block.id == thread.block_id)
            })
            .collect()
    }

    /// Serializes the threads to the JSON array embedded in document content.
    pub(crate) fn comments_to_content_json(&self) -> String {
        let mut out = String::from("[");
        for (index, thread) in self.comments.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push_str(&format!(
                "{{\"id\":{},\"author\":{},\"block_id\":{},\"start\":{},\"end\":{},\"body\":{},\"resolved\":{},\"orphaned\":{}}}",
                pkg_json::escape(&thread.id),
                pkg_json::escape(&thread.author),
                thread.block_id,
                thread.start,
                thread.end,
                pkg_json::escape(&thread.body),
                thread.resolved,
                thread.orphaned,
            ));
        }
        out.push(']');
        out
    }

    /// Parses the JSON array written by [`Self::comments_to_content_json`].
    /// Malformed entries are skipped; unknown fields are ignored.
    pub(crate) fn comments_from_content_json(value: &str) -> Vec<CommentThread> {
        ContentParser::new(value)
            .parse_array()
            .unwrap_or_default()
            .iter()
            .filter_map(|item| comment_thread_from_raw(item))
            .collect()
    }
}

/// Parses one thread object out of a raw JSON object value.
pub(crate) fn comment_thread_from_raw(raw: &str) -> Option<CommentThread> {
    let fields = ContentParser::new(raw).parse().ok()?;
    let mut id = String::new();
    let mut author = String::new();
    let mut block_id = 0u64;
    let mut start = 0usize;
    let mut end = 0usize;
    let mut body = String::new();
    let mut resolved = false;
    let mut orphaned = false;
    for (k, v) in &fields {
        let text = match v {
            JsonValue::String(s) => s.clone(),
            JsonValue::Raw(raw) => raw.trim_matches('"').to_string(),
        };
        match k.as_str() {
            "id" => id = text,
            "author" => author = text,
            "block_id" => block_id = text.parse().ok()?,
            "start" => start = text.parse().ok()?,
            "end" => end = text.parse().ok()?,
            "body" => body = text,
            "resolved" => resolved = text == "true",
            "orphaned" => orphaned = text == "true",
            _ => {}
        }
    }
    Some(CommentThread {
        id,
        author,
        block_id,
        start,
        end,
        body,
        resolved,
        orphaned,
    })
}

pub(crate) fn blocks_to_text(blocks: &[RichBlock]) -> String {
    blocks
        .iter()
        .map(|block| block.text.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

fn block_start(blocks: &[RichBlock], block_id: u64) -> Option<(usize, &RichBlock)> {
    let mut start = 0;
    for (index, block) in blocks.iter().enumerate() {
        if index > 0 {
            start += 1;
        }
        if block.id == block_id {
            return Some((start, block));
        }
        start += block.text.len_bytes();
    }
    None
}

fn block_at_offset(blocks: &[RichBlock], offset: usize) -> Option<(&RichBlock, usize)> {
    let mut start = 0;
    for (index, block) in blocks.iter().enumerate() {
        if index > 0 {
            start += 1;
        }
        let end = start + block.text.len_bytes();
        if offset <= end {
            return Some((block, offset.saturating_sub(start)));
        }
        start = end;
    }
    None
}

fn map_comment_endpoint(
    offset: usize,
    old_start: usize,
    old_end: usize,
    new_start: usize,
    new_end: usize,
    start_endpoint: bool,
) -> usize {
    if old_start == old_end {
        if offset < old_start || (offset == old_start && !start_endpoint) {
            offset
        } else {
            offset.saturating_add(new_end.saturating_sub(new_start))
        }
    } else if offset < old_start {
        offset
    } else if offset == old_start {
        new_start
    } else if offset < old_end {
        if start_endpoint {
            new_start
        } else {
            new_end
        }
    } else {
        offset
            .saturating_sub(old_end - old_start)
            .saturating_add(new_end - new_start)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RichBlock, TextSelection};

    fn document_with_block() -> WriterDocument {
        let mut document = WriterDocument::new("doc", "Doc");
        document
            .blocks
            .push(RichBlock::new(7, "paragraph", "Hello anchored world"));
        document
    }

    #[test]
    fn add_thread_anchors_and_generates_unique_ids() {
        let mut document = document_with_block();
        let first = document
            .add_comment_thread(7, 6, 14, "Nice phrase")
            .expect("add succeeds");
        let second = document
            .add_comment_thread(7, 0, 5, "Another")
            .expect("add succeeds");
        assert_ne!(first, second);
        let threads = document.live_comment_threads();
        assert_eq!(threads.len(), 2);
        assert_eq!(threads[0].author, LOCAL_AUTHOR);
        assert_eq!((threads[0].start, threads[0].end), (6, 14));
    }

    #[test]
    fn add_thread_rejects_empty_body_unknown_block_bad_range() {
        let mut document = document_with_block();
        assert!(document.add_comment_thread(7, 0, 5, "   ").is_err());
        assert!(document.add_comment_thread(99, 0, 5, "hi").is_err());
        assert!(document.add_comment_thread(7, 3, 1, "hi").is_err());
    }

    #[test]
    fn resolve_and_delete_find_by_id() {
        let mut document = document_with_block();
        let id = document.add_comment_thread(7, 0, 5, "hi").unwrap();
        assert!(document.set_comment_thread_resolved(&id, true));
        assert!(document.live_comment_threads()[0].resolved);
        assert!(!document.set_comment_thread_resolved("missing", true));
        assert!(document.remove_comment_thread(&id));
        assert!(!document.remove_comment_thread(&id));
    }

    #[test]
    fn threads_on_deleted_blocks_are_not_live() {
        let mut document = document_with_block();
        document
            .add_comment_thread(7, 0, 5, "hi")
            .expect("add succeeds");
        document.blocks.clear();
        assert!(document.live_comment_threads().is_empty());
    }

    #[test]
    fn comments_round_trip_through_content_json() {
        let mut document = document_with_block();
        document
            .add_comment_thread(7, 6, 14, "Nice phrase")
            .expect("add succeeds");
        document.set_comment_thread_resolved(&document.comments[0].id.clone(), true);
        let json = document.to_content_json();
        let reopened = WriterDocument::from_content_json(&json).expect("parse");
        assert_eq!(reopened.comments, document.comments);
        assert!(json.contains("\"comments\":["));
    }

    #[test]
    fn insertion_before_comment_keeps_the_comment_on_the_same_words() {
        let mut document = WriterDocument::new("doc", "Doc");
        document
            .blocks
            .push(RichBlock::new(7, "paragraph", "Hello world"));
        document
            .add_comment_thread(7, 6, 11, "Keep this phrase")
            .expect("add comment");

        document.set_selection(TextSelection::caret(0));
        document
            .replace_selection_text("New ")
            .expect("insert before comment");

        let comment = &document.comments[0];
        let block = document.get(comment.block_id).expect("comment block");
        assert_eq!((comment.start, comment.end), (10, 15));
        assert_eq!(&block.text.as_str()[comment.start..comment.end], "world");

        let bytes = crate::save_document(&document).expect("save document");
        let reopened = crate::load_document(&bytes).expect("reopen document");
        let comment = &reopened.comments[0];
        let block = reopened
            .get(comment.block_id)
            .expect("reopened comment block");
        assert_eq!(&block.text.as_str()[comment.start..comment.end], "world");
    }

    #[test]
    fn comment_anchor_uses_utf8_bytes_and_survives_paragraph_split_and_merge() {
        let mut document = WriterDocument::new("doc", "Doc");
        document
            .blocks
            .push(RichBlock::new(7, "paragraph", "Hello 🌍 world"));
        document
            .add_comment_thread(7, 6, 10, "Keep the symbol")
            .expect("add comment");
        document.set_selection(TextSelection::caret(0));
        document
            .replace_selection_text("New ")
            .expect("insert before emoji");

        let comment = &document.comments[0];
        let block = document.get(comment.block_id).expect("emoji block");
        assert_eq!(&block.text.as_str()[comment.start..comment.end], "🌍");

        document.replace_paragraphs("New Hello\n🌍 world");
        let comment = &document.comments[0];
        let block = document.get(comment.block_id).expect("split comment block");
        assert_eq!(&block.text.as_str()[comment.start..comment.end], "🌍");

        document.replace_paragraphs("New Hello 🌍 world");
        let comment = &document.comments[0];
        let block = document
            .get(comment.block_id)
            .expect("merged comment block");
        assert_eq!(&block.text.as_str()[comment.start..comment.end], "🌍");
        assert!(!comment.orphaned);
    }

    #[test]
    fn partial_anchor_deletion_shrinks_and_complete_deletion_is_orphaned() {
        let mut document = WriterDocument::new("doc", "Doc");
        document
            .blocks
            .push(RichBlock::new(7, "paragraph", "Hello world"));
        document
            .add_comment_thread(7, 6, 11, "Review this word")
            .expect("add comment");

        document.replace_paragraphs("Hello wrld");
        let comment = &document.comments[0];
        let block = document.get(comment.block_id).expect("shrunk anchor block");
        assert_eq!(&block.text.as_str()[comment.start..comment.end], "wrld");
        assert!(!comment.orphaned);

        document.replace_paragraphs("Hello ");
        assert!(document.comments[0].orphaned);
        assert!(document.live_comment_threads().is_empty());
        assert_eq!(document.comments[0].body, "Review this word");

        let bytes = crate::save_document(&document).expect("save document");
        let reopened = crate::load_document(&bytes).expect("reopen document");
        assert!(reopened.comments[0].orphaned);
        assert!(reopened.live_comment_threads().is_empty());
    }

    fn anchored_text(document: &WriterDocument) -> String {
        let comment = &document.comments[0];
        let block = document.get(comment.block_id).expect("comment block");
        block.text.as_str()[comment.start..comment.end].to_string()
    }

    fn one_block(text: &str, start: usize, end: usize) -> WriterDocument {
        let mut document = WriterDocument::new("doc", "Doc");
        document.blocks.push(RichBlock::new(7, "paragraph", text));
        document.add_comment_thread(7, start, end, "c").unwrap();
        document
    }

    #[test]
    fn inserting_a_repeat_of_the_commented_word_before_it_moves_the_anchor() {
        let mut document = one_block("Hello world", 6, 11);
        document.set_selection(TextSelection::caret(6));
        document.replace_selection_text("world ").unwrap();
        let comment = &document.comments[0];
        assert_eq!((comment.start, comment.end), (12, 17));
    }

    #[test]
    fn deleting_a_repeat_before_the_commented_word_keeps_the_anchor_on_its_word() {
        let mut document = one_block("Hello world world", 12, 17);
        document.set_selection(TextSelection::range(6, 12));
        document.replace_selection_text("").unwrap();
        let comment = &document.comments[0];
        assert!(!comment.orphaned);
        assert_eq!((comment.start, comment.end), (6, 11));
        assert_eq!(anchored_text(&document), "world");
    }

    #[test]
    fn native_editor_insertion_uses_the_caret_to_place_the_change() {
        let mut document = one_block("Hello world", 6, 11);
        document
            .replace_editor_text_at("Hello world world", Some(12))
            .unwrap();
        let comment = &document.comments[0];
        assert_eq!((comment.start, comment.end), (12, 17));
    }

    #[test]
    fn native_editor_deletion_uses_the_caret_to_place_the_change() {
        let mut document = one_block("Hello world world", 12, 17);
        document
            .replace_editor_text_at("Hello world", Some(6))
            .unwrap();
        let comment = &document.comments[0];
        assert!(!comment.orphaned);
        assert_eq!((comment.start, comment.end), (6, 11));
    }

    #[test]
    fn insertion_at_the_end_does_not_extend_and_inside_does() {
        let mut document = one_block("Hello world", 6, 11);
        document.set_selection(TextSelection::caret(11));
        document.replace_selection_text("!").unwrap();
        assert_eq!(anchored_text(&document), "world");
        document.set_selection(TextSelection::caret(8));
        document.replace_selection_text("XY").unwrap();
        assert_eq!(anchored_text(&document), "woXYrld");
    }

    #[test]
    fn typing_before_emoji_and_combining_marks_keeps_valid_boundaries() {
        let mut document = one_block("e\u{301}x 🌍 y", 5, 9);
        assert_eq!(anchored_text(&document), "🌍");
        document.set_selection(TextSelection::caret(0));
        document.replace_selection_text("🎉").unwrap();
        assert_eq!(anchored_text(&document), "🌍");
    }

    #[test]
    fn splitting_inside_the_comment_keeps_its_start_block_part() {
        let mut document = one_block("Hello world", 6, 11);
        document.set_selection(TextSelection::caret(8));
        document.replace_selection_text("\n").unwrap();
        assert!(!document.comments[0].orphaned);
        assert_eq!(anchored_text(&document), "wo");
    }

    #[test]
    fn deleting_the_whole_commented_text_orphans_but_keeps_the_thread_manageable() {
        let mut document = one_block("Hello world!", 6, 11);
        document.set_selection(TextSelection::range(5, 11));
        document.replace_selection_text("").unwrap();
        assert!(document.comments[0].orphaned);
        assert_eq!(document.comments.len(), 1);
        let id = document.comments[0].id.clone();
        assert!(document.set_comment_thread_resolved(&id, true));
        assert!(document.remove_comment_thread(&id));
    }

    #[test]
    fn legacy_content_without_comments_loads_empty() {
        let json = "{\"id\":\"d\",\"title\":\"T\",\"blocks\":[],\"selection\":{\"anchor\":0,\"focus\":0,\"affinity\":\"upstream\"}}";
        let reopened = WriterDocument::from_content_json(json).expect("parse");
        assert!(reopened.comments.is_empty());
    }
}
