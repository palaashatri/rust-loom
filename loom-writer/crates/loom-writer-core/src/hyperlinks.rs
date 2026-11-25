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

    /// Serializes `self` to a JSON string for `.loom` content persistence.
    pub fn write_content_json(&self) -> String {
        let mut out = String::from("{\"spans\":[");
        for (i, s) in self.spans.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            let target = match &s.link.target {
                HyperlinkTarget::Url(u) => format!("url:{}", u.replace('"', "\\\"")),
                HyperlinkTarget::Bookmark(b) => format!("bm:{}", b.replace('"', "\\\"")),
                _ => String::new(),
            };
            out.push_str(&format!(
                "{{\"b\":{},\"s\":{},\"e\":{},\"t\":\"{}\"}}",
                s.block_id, s.start, s.end, target
            ));
        }
        out.push_str("]}");
        out
    }

    /// Deserializes a `HyperlinkTable` from a `.loom` JSON string.
    pub fn from_content_json(raw: &str) -> Result<Self, String> {
        let mut table = Self::default();
        for chunk in raw.split("{\"b\":").skip(1) {
            let Some(b_end) = chunk.find(',') else {
                continue;
            };
            let Ok(block_id) = chunk[..b_end].trim().parse::<u64>() else {
                continue;
            };
            let rest = &chunk[b_end + 1..];
            let Some(s_pos) = rest.find("\"s\":") else {
                continue;
            };
            let after_s = &rest[s_pos + 4..];
            let Some(s_end) = after_s.find(',') else {
                continue;
            };
            let Ok(start) = after_s[..s_end].trim().parse::<usize>() else {
                continue;
            };
            let after_s_comma = &after_s[s_end + 1..];
            let Some(e_pos) = after_s_comma.find("\"e\":") else {
                continue;
            };
            let after_e = &after_s_comma[e_pos + 4..];
            let Some(e_end) = after_e.find(',') else {
                continue;
            };
            let Ok(end) = after_e[..e_end].trim().parse::<usize>() else {
                continue;
            };
            let Some(t_pos) = after_e.find("\"t\":\"") else {
                continue;
            };
            let after_t = &after_e[t_pos + 5..];
            let Some(t_end) = after_t.find('"') else {
                continue;
            };
            let t_val = &after_t[..t_end];
            if let Some(url) = t_val.strip_prefix("url:") {
                if let Ok(span) = HyperlinkSpan::url(block_id, start..end, url, None) {
                    table.set(span);
                }
            } else if let Some(bm) = t_val.strip_prefix("bm:") {
                table.set(HyperlinkSpan::bookmark(block_id, start..end, bm, None));
            }
        }
        Ok(table)
    }
}

impl crate::WriterDocument {
    /// Inserts or replaces a URL hyperlink on `block_id` at `range`.
    pub fn set_hyperlink(
        &mut self,
        block_id: u64,
        range: Range<usize>,
        raw_url: &str,
        tooltip: Option<String>,
    ) -> Result<(), HyperlinkError> {
        let span = HyperlinkSpan::url(block_id, range, raw_url, tooltip)?;
        self.hyperlinks.set(span);
        Ok(())
    }

    /// Removes any hyperlink overlapping `range` in `block_id`. Returns `true` if removed.
    pub fn remove_hyperlink(&mut self, block_id: u64, range: Range<usize>) -> bool {
        let before = self.hyperlinks.spans.len();
        self.hyperlinks.remove(block_id, range);
        self.hyperlinks.spans.len() < before
    }

    /// Returns the hyperlink at `offset` in `block_id`, if any.
    pub fn hyperlink_at(&self, block_id: u64, offset: usize) -> Option<&HyperlinkSpan> {
        self.hyperlinks.at(block_id, offset)
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
