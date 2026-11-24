//! Inline hyperlink spans for [`WriterDocument`].

use loom_objects::{Hyperlink, HyperlinkError, HyperlinkTarget};
use std::ops::Range;

/// A hyperlink attached to a byte range `[start, end)` within a block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HyperlinkSpan {
    pub block_id: u64,
    pub start: usize,
    pub end: usize,
    pub link: Hyperlink,
}

impl HyperlinkSpan {
    /// Creates a validated URL hyperlink span.
    pub fn url(
        block_id: u64,
        range: Range<usize>,
        raw_url: &str,
        tooltip: Option<String>,
    ) -> Result<Self, HyperlinkError> {
        let link = Hyperlink::url(raw_url, tooltip)?;
        Ok(Self {
            block_id,
            start: range.start,
            end: range.end,
            link,
        })
    }

    /// Creates a bookmark hyperlink span.
    pub fn bookmark(
        block_id: u64,
        range: Range<usize>,
        bookmark: impl Into<String>,
        tooltip: Option<String>,
    ) -> Self {
        Self {
            block_id,
            start: range.start,
            end: range.end,
            link: Hyperlink {
                target: HyperlinkTarget::Bookmark(bookmark.into()),
                tooltip,
            },
        }
    }

    /// Returns `true` if `offset` (block-local UTF-8 byte) falls inside `[start, end)`.
    pub fn contains(&self, offset: usize) -> bool {
        offset >= self.start && offset < self.end
    }
}

/// Collection of inline hyperlinks in a document, keyed by `(block_id, start..end)`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HyperlinkTable {
    pub spans: Vec<HyperlinkSpan>,
}

impl HyperlinkTable {
    /// Inserts or replaces any overlapping hyperlink in the same block.
    pub fn set(&mut self, span: HyperlinkSpan) {
        if span.start >= span.end {
            return;
        }
        self.spans.retain(|existing| {
            existing.block_id != span.block_id
                || existing.end <= span.start
                || existing.start >= span.end
        });
        self.spans.push(span);
        self.spans.sort_by_key(|s| (s.block_id, s.start, s.end));
    }

    /// Removes any hyperlink overlapping `range` in `block_id`.
    pub fn remove(&mut self, block_id: u64, range: Range<usize>) -> bool {
        let before = self.spans.len();
        self.spans.retain(|existing| {
            existing.block_id != block_id
                || existing.end <= range.start
                || existing.start >= range.end
        });
        self.spans.len() != before
    }

    /// Finds the hyperlink at `offset` in `block_id`.
    pub fn at(&self, block_id: u64, offset: usize) -> Option<&HyperlinkSpan> {
        self.spans
            .iter()
            .find(|s| s.block_id == block_id && s.contains(offset))
    }

    /// Returns all hyperlinks in `block_id`.
    pub fn for_block(&self, block_id: u64) -> Vec<&HyperlinkSpan> {
        self.spans
            .iter()
            .filter(|s| s.block_id == block_id)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hyperlink_table_set_lookup_remove() {
        let mut table = HyperlinkTable::default();
        let span = HyperlinkSpan::url(1, 0..5, "example.com", Some("Example".into())).unwrap();
        table.set(span);
        assert_eq!(
            table.at(1, 2).map(|s| &s.link.target),
            Some(&HyperlinkTarget::Url("https://example.com".into()))
        );
        assert!(table.at(1, 5).is_none());
        assert!(table.remove(1, 0..5));
        assert!(table.at(1, 2).is_none());
    }

    #[test]
    fn rejects_javascript_scheme() {
        let err = HyperlinkSpan::url(1, 0..4, "javascript:alert(1)", None).unwrap_err();
        assert_eq!(err, HyperlinkError::UnsupportedScheme("javascript".into()));
    }
}
