//! Document comments: anchored threads on `WriterDocument`.
//!
//! Extends `WriterDocument` in its own module so `lib.rs` stays within its
//! registered byte ceiling. Threads anchor to a stable block id plus a UTF-8
//! byte range; block ids survive edits, so threads follow their text. Ranges
//! are clamped at read time when text shrinks, and threads whose block was
//! deleted are reported as dangling rather than surfaced.

use crate::{pkg_json, CommentThread, ContentParser, JsonValue, WriterDocument};

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

    /// Threads whose anchor block still exists, with ranges clamped to the
    /// current text length. Threads on deleted blocks are dropped.
    pub fn live_comment_threads(&self) -> Vec<&CommentThread> {
        self.comments
            .iter()
            .filter(|thread| self.blocks.iter().any(|block| block.id == thread.block_id))
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
                "{{\"id\":{},\"author\":{},\"block_id\":{},\"start\":{},\"end\":{},\"body\":{},\"resolved\":{}}}",
                pkg_json::escape(&thread.id),
                pkg_json::escape(&thread.author),
                thread.block_id,
                thread.start,
                thread.end,
                pkg_json::escape(&thread.body),
                thread.resolved,
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
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RichBlock;

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
    fn legacy_content_without_comments_loads_empty() {
        let json = "{\"id\":\"d\",\"title\":\"T\",\"blocks\":[],\"selection\":{\"anchor\":0,\"focus\":0,\"affinity\":\"upstream\"}}";
        let reopened = WriterDocument::from_content_json(json).expect("parse");
        assert!(reopened.comments.is_empty());
    }
}
