//! Document comments: anchored threads on `WriterDocument`.
//!
//! Extends `WriterDocument` in its own module so `lib.rs` stays within its
//! registered byte ceiling. Threads anchor to a stable block id plus a UTF-8
//! byte range. Text edits rebase each range so it remains attached to the
//! selected words. If those words disappear completely, the thread remains
//! visible as an explicitly orphaned review item.

use crate::{
    changed_text_ranges, pkg_json, CommentThread, ContentParser, JsonValue, RichBlock,
    WriterDocument,
};

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
        let (old_change_start, old_change_end, new_change_start, new_change_end) =
            changed_text_ranges(&old_text, &new_text);

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

fn blocks_to_text(blocks: &[RichBlock]) -> String {
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

    #[test]
    fn legacy_content_without_comments_loads_empty() {
        let json = "{\"id\":\"d\",\"title\":\"T\",\"blocks\":[],\"selection\":{\"anchor\":0,\"focus\":0,\"affinity\":\"upstream\"}}";
        let reopened = WriterDocument::from_content_json(json).expect("parse");
        assert!(reopened.comments.is_empty());
    }
}
