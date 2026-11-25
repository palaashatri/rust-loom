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

    /// Returns estimated heap bytes retained by this table.
    pub fn retained_bytes(&self) -> usize {
        self.spans.capacity() * std::mem::size_of::<HyperlinkSpan>()
            + self
                .spans
                .iter()
                .map(|s| {
                    let t = match &s.link.target {
                        HyperlinkTarget::Url(u) | HyperlinkTarget::Bookmark(u) => u.capacity(),
                        _ => 0,
                    };
                    t + s.link.tooltip.as_ref().map_or(0, String::capacity)
                })
                .sum::<usize>()
    }

    /// Rebase or remove hyperlink spans after `replace_paragraphs` replaces `old_blocks` with `new_blocks`.
    pub fn rebase_after_block_replace(
        &mut self,
        old_blocks: &[crate::RichBlock],
        new_blocks: &[crate::RichBlock],
    ) {
        if self.spans.is_empty() {
            return;
        }
        use std::collections::HashMap;
        let old_text_by_id: HashMap<u64, &str> =
            old_blocks.iter().map(|b| (b.id, b.text.as_str())).collect();
        let new_by_id: HashMap<u64, &str> =
            new_blocks.iter().map(|b| (b.id, b.text.as_str())).collect();
        self.spans.retain(|s| {
            let Some(&old_t) = old_text_by_id.get(&s.block_id) else {
                return false;
            };
            let Some(&new_t) = new_by_id.get(&s.block_id) else {
                return false;
            };
            old_t == new_t && s.end <= new_t.len()
        });
    }

    /// Serializes `self` to a JSON string for `.loom` content persistence.
    pub fn write_content_json(&self) -> String {
        let mut out = String::from("{\"spans\":[");
        let mut first = true;
        for s in &self.spans {
            let target = match &s.link.target {
                HyperlinkTarget::Url(u) => format!("url:{u}"),
                HyperlinkTarget::Bookmark(b) => format!("bm:{b}"),
                _ => continue,
            };
            if !first {
                out.push(',');
            }
            first = false;
            out.push_str(&format!(
                "{{\"b\":{},\"s\":{},\"e\":{},\"t\":{}",
                s.block_id,
                s.start,
                s.end,
                crate::pkg_json::escape(&target)
            ));
            if let Some(tip) = &s.link.tooltip {
                out.push_str(&format!(",\"tip\":{}", crate::pkg_json::escape(tip)));
            }
            out.push('}');
        }
        out.push_str("]}");
        out
    }

    /// Deserializes a `HyperlinkTable` from a `.loom` JSON string.
    pub fn from_content_json(raw: &str) -> Result<Self, String> {
        let mut table = Self::default();
        let Some(spans_pos) = raw.find("\"spans\"") else {
            return Ok(table);
        };
        let Some(arr_rel) = raw[spans_pos..].find('[') else {
            return Ok(table);
        };
        for item in split_json_array_objects(&raw[spans_pos + arr_rel..]) {
            let fields = crate::ContentParser::new(item)
                .parse()
                .map_err(|e| e.to_string())?;
            let mut block_id: Option<u64> = None;
            let mut start: Option<usize> = None;
            let mut end: Option<usize> = None;
            let mut t_val: Option<String> = None;
            let mut tooltip: Option<String> = None;
            for (k, v) in fields {
                match (k.as_str(), v) {
                    ("b", crate::JsonValue::Raw(s)) => block_id = s.trim().parse().ok(),
                    ("s", crate::JsonValue::Raw(s)) => start = s.trim().parse().ok(),
                    ("e", crate::JsonValue::Raw(s)) => end = s.trim().parse().ok(),
                    ("t", crate::JsonValue::String(s)) => t_val = Some(s),
                    ("tip", crate::JsonValue::String(s)) => tooltip = Some(s),
                    _ => {}
                }
            }
            let (Some(block_id), Some(start), Some(end), Some(t_val)) =
                (block_id, start, end, t_val)
            else {
                continue;
            };
            if let Some(url) = t_val.strip_prefix("url:") {
                if let Ok(span) = HyperlinkSpan::url(block_id, start..end, url, tooltip) {
                    table.set(span);
                }
            } else if let Some(bm) = t_val.strip_prefix("bm:") {
                table.set(HyperlinkSpan::bookmark(block_id, start..end, bm, tooltip));
            }
        }
        Ok(table)
    }
}

fn split_json_array_objects(arr: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut in_string = false;
    let mut escaped = false;
    let mut depth = 0usize;
    let mut obj_start: Option<usize> = None;
    for (idx, ch) in arr.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '{' => {
                if depth == 0 {
                    obj_start = Some(idx);
                }
                depth += 1;
            }
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    if let Some(start) = obj_start.take() {
                        out.push(&arr[start..=idx]);
                    }
                }
            }
            _ => {}
        }
    }
    out
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

    #[test]
    fn content_json_round_trips_special_chars_and_braces() {
        let mut table = HyperlinkTable::default();
        table.set(
            HyperlinkSpan::url(
                1,
                0..5,
                "https://example.com/a?q={x}",
                Some("Tip with {braces} & \"quotes\"\\slash".into()),
            )
            .unwrap(),
        );
        table.set(HyperlinkSpan::bookmark(
            2,
            1..4,
            "sec\"one\\{two}",
            Some("Line1\nLine2".into()),
        ));
        let json = table.write_content_json();
        let decoded = HyperlinkTable::from_content_json(&json).expect("valid JSON");
        assert_eq!(decoded, table);
    }

    #[test]
    fn rebase_after_block_replace_retains_unchanged_and_drops_edited() {
        let mut table = HyperlinkTable::default();
        table.set(HyperlinkSpan::url(1, 0..4, "https://a.example", None).unwrap());
        table.set(HyperlinkSpan::url(2, 0..4, "https://b.example", None).unwrap());
        let old_blocks = vec![
            crate::RichBlock::new(1, "paragraph", "Same text"),
            crate::RichBlock::new(2, "paragraph", "Old text"),
        ];
        let new_blocks = vec![
            crate::RichBlock::new(1, "paragraph", "Same text"),
            crate::RichBlock::new(2, "paragraph", "Edited text"),
        ];
        table.rebase_after_block_replace(&old_blocks, &new_blocks);
        assert_eq!(table.spans.len(), 1);
        assert_eq!(table.spans[0].block_id, 1);
    }
}
