//! Loom Writer document engine — headless and fully testable.
//!
//! This crate implements the core document model, persistence to `.loomdoc`,
//! and export to Markdown / plain text. It deliberately has no UI dependency;
//! the Slint interface (a documented follow-on) consumes this engine.

use loom_document::{Mutation, Offset, Text, TextEdit};
use loom_package::manifest::{
    json as pkg_json, Checksum, Manifest, ManifestEntry, MimeType, PackageKind, SchemaVersion,
};
use loom_package::zip::{self, PackageArchive};
use loom_text::ParagraphStyle;

/// Stable document id.
pub type DocId = String;

/// A single rich-text block (paragraph or heading) in the document.
#[derive(Debug, Clone, PartialEq)]
pub struct RichBlock {
    /// Stable id.
    pub id: u64,
    /// Block kind: "paragraph", "heading1".."heading6".
    pub kind: String,
    /// Text content.
    pub text: Text,
    /// Paragraph style.
    pub style: loom_text::ParagraphStyle,
    /// Character style runs (byte ranges into text).
    pub runs: Vec<loom_text::StyleRun>,
}

impl RichBlock {
    /// Create a new block.
    pub fn new(id: u64, kind: &str, text: &str) -> Self {
        Self {
            id,
            kind: kind.to_string(),
            text: Text::from_str(text),
            style: ParagraphStyle::default(),
            runs: Vec::new(),
        }
    }
}

/// A Loom Writer document.
#[derive(Debug, Clone, PartialEq)]
pub struct WriterDocument {
    /// Stable document id.
    pub id: DocId,
    /// Title (used for display and export).
    pub title: String,
    /// Document blocks in order.
    pub blocks: Vec<RichBlock>,
}

impl WriterDocument {
    /// Create a new empty document.
    pub fn new(id: impl Into<DocId>, title: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            blocks: Vec::new(),
        }
    }

    /// Append a block generated from manually maintained next id.
    pub fn push(&mut self, b: RichBlock) {
        self.blocks.push(b);
    }

    /// Number of blocks.
    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    /// Whether empty.
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    /// Get a block by id.
    pub fn get(&self, id: u64) -> Option<&RichBlock> {
        self.blocks.iter().find(|b| b.id == id)
    }

    /// Next free block id.
    pub fn next_id(&self) -> u64 {
        self.blocks.iter().map(|b| b.id).max().unwrap_or(0) + 1
    }

    /// Compute a flattened text of all blocks joined by newlines.
    pub fn plain_text(&self) -> String {
        let mut out = String::new();
        for (i, b) in self.blocks.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            out.push_str(b.text.as_str());
        }
        out
    }

    /// Return the canonical text representation used by the editable Writer
    /// surface. Paragraphs are separated by one newline so ordinary Enter
    /// events create a new block. The empty string is the canonical encoding
    /// for an empty document; otherwise every newline is meaningful, including
    /// leading, trailing, and consecutive newlines for empty blocks.
    pub fn editor_text(&self) -> String {
        self.blocks
            .iter()
            .map(|block| block.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Replace the document's blocks from editable plain text paragraphs.
    ///
    /// Paragraphs are separated by single newlines. Empty fields are retained,
    /// so leading, trailing, and empty paragraphs are representable. Exact
    /// text matches retain the complete old block, including id, kind, paragraph
    /// style, and runs, even when an insertion or deletion moves them. The
    /// empty string represents an empty document rather than one empty block.
    pub fn replace_paragraphs(&mut self, plain_text: &str) {
        let normalized = plain_text.replace("\r\n", "\n").replace('\r', "\n");
        let paragraphs = if normalized.is_empty() {
            Vec::new()
        } else {
            normalized.split('\n').map(str::to_owned).collect()
        };

        let old_blocks = std::mem::take(&mut self.blocks);
        let exact_matches = exact_paragraph_matches(&old_blocks, &paragraphs);
        let metadata_matches = metadata_matches(&old_blocks, &paragraphs, &exact_matches);
        let mut next_id = old_blocks.iter().map(|block| block.id).max().unwrap_or(0) + 1;
        let blocks = paragraphs
            .into_iter()
            .enumerate()
            .map(|(index, text)| match metadata_matches[index] {
                Some(old_index) => {
                    let old = &old_blocks[old_index];
                    if old.text.as_str() == text {
                        old.clone()
                    } else {
                        let mut block = old.clone();
                        block.text = Text::from_str(&text);
                        block.runs = remap_style_runs(old.text.as_str(), &text, &old.runs);
                        block
                    }
                }
                None => {
                    let block = RichBlock::new(next_id, "paragraph", &text);
                    next_id += 1;
                    block
                }
            })
            .collect();
        self.blocks = blocks;
    }

    /// A many-block Mutation (for undo replay). Returns a single mutation
    /// that replaces entire content (used by CLI + tests as demonstration of
    /// transactional edit). For real incremental editing, blocks build
    /// per-block edits; this is a valid atomic model too.
    pub fn replace_all_mutation(&self, next: &Self) -> Mutation {
        let mut m = Mutation::new();
        let mut old = Text::from_str(&self.plain_text());
        let new_text = next.plain_text();
        // Simple diff: delete all, insert new.
        if old.len_bytes() > 0 {
            m.push(TextEdit::delete(
                Offset(0),
                loom_document::Length(old.len_bytes()),
                old.as_str(),
            ));
            old = Text::empty();
        }
        if !new_text.is_empty() {
            m.push(TextEdit::insert(Offset(0), &new_text));
        }
        let _ = old;
        m
    }

    /// Serialize the document content to the JSON content entry.
    pub fn to_content_json(&self) -> String {
        let mut s = String::with_capacity(256);
        s.push('{');
        s.push_str("\"id\":");
        s.push_str(&pkg_json::escape(&self.id));
        s.push_str(",\"title\":");
        s.push_str(&pkg_json::escape(&self.title));
        s.push_str(",\"blocks\":[");
        for (i, b) in self.blocks.iter().enumerate() {
            if i > 0 {
                s.push(',');
            }
            s.push('{');
            s.push_str("\"id\":");
            s.push_str(&b.id.to_string());
            s.push_str(",\"kind\":");
            s.push_str(&pkg_json::escape(&b.kind));
            s.push_str(",\"text\":");
            s.push_str(&pkg_json::escape(b.text.as_str()));
            s.push_str(",\"align\":");
            s.push_str(&pkg_json::escape(alignment_name(b.style.alignment)));
            // Keep the legacy alignment field above so older readers can open
            // documents written by this version. The nested style and runs
            // fields carry the complete rich-text model for newer readers.
            s.push_str(",\"style\":");
            s.push_str(&paragraph_style_json(&b.style));
            s.push_str(",\"runs\":");
            s.push_str(&runs_json(&b.runs));
            s.push('}');
        }
        s.push(']');
        s.push('}');
        s
    }

    /// Parse document content from JSON (the format written by `to_content_json`).
    pub fn from_content_json(s: &str) -> Result<Self, WriterError> {
        let mut id = String::new();
        let mut title = String::new();
        let mut blocks: Vec<RichBlock> = Vec::new();

        // Minimal safe parse: reuse loom_package's bounded JSON parser on the
        // top-level object, then iterate the entries array.
        let parser = ContentParser::new(s);
        let fields = parser.parse()?;
        for (k, v) in &fields {
            match k.as_str() {
                "id" => id = json_value_text(v),
                "title" => title = json_value_text(v),
                "blocks" => {
                    let raw = match v {
                        JsonValue::Raw(raw) => raw,
                        JsonValue::String(_) => {
                            return Err(WriterError::Json("expected blocks array".into()))
                        }
                    };
                    blocks = parse_blocks(raw)?;
                }
                _ => {}
            }
        }
        Ok(Self { id, title, blocks })
    }

    /// Render to Markdown.
    pub fn to_markdown(&self) -> String {
        let mut out = String::new();
        for b in &self.blocks {
            match b.kind.as_str() {
                "heading1" => out.push_str(&format!("# {}\n\n", b.text.as_str())),
                "heading2" => out.push_str(&format!("## {}\n\n", b.text.as_str())),
                "heading3" => out.push_str(&format!("### {}\n\n", b.text.as_str())),
                "heading4" => out.push_str(&format!("#### {}\n\n", b.text.as_str())),
                "heading5" => out.push_str(&format!("##### {}\n\n", b.text.as_str())),
                "heading6" => out.push_str(&format!("###### {}\n\n", b.text.as_str())),
                _ => out.push_str(&format!("{}\n\n", b.text.as_str())),
            }
        }
        out
    }

    /// Render document content to semantic HTML markup.
    pub fn to_html_string(&self) -> String {
        let mut out = String::new();
        out.push_str("<!DOCTYPE html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n<title>");
        out.push_str(&self.title);
        out.push_str("</title>\n</head>\n<body>\n");
        for b in &self.blocks {
            let tag = match b.kind.as_str() {
                "heading1" => "h1",
                "heading2" => "h2",
                "heading3" => "h3",
                "heading4" => "h4",
                "heading5" => "h5",
                "heading6" => "h6",
                "quote" => "blockquote",
                _ => "p",
            };
            out.push_str(&format!("<{tag}>"));
            out.push_str(b.text.as_str());
            out.push_str(&format!("</{tag}>\n"));
        }
        out.push_str("</body>\n</html>\n");
        out
    }

    /// Extracts a table of contents outline from all heading blocks.
    pub fn generate_toc(&self) -> Vec<TocEntry> {
        let mut entries = Vec::new();
        for b in &self.blocks {
            let level = match b.kind.as_str() {
                "heading1" => Some(1),
                "heading2" => Some(2),
                "heading3" => Some(3),
                "heading4" => Some(4),
                "heading5" => Some(5),
                "heading6" => Some(6),
                _ => None,
            };
            if let Some(lvl) = level {
                let trimmed = b.text.as_str().trim();
                if !trimmed.is_empty() {
                    entries.push(TocEntry {
                        block_id: b.id,
                        title: trimmed.to_string(),
                        level: lvl,
                    });
                }
            }
        }
        entries
    }

    /// Computes full document text statistics.
    pub fn statistics(&self) -> DocumentStats {
        let mut words = 0;
        let mut chars = 0;
        let mut chars_no_spaces = 0;
        let mut sentences = 0;
        for b in &self.blocks {
            let text = b.text.as_str();
            chars += text.chars().count();
            chars_no_spaces += text.chars().filter(|c| !c.is_whitespace()).count();
            words += text.split_whitespace().count();
            sentences += text
                .chars()
                .filter(|&c| c == '.' || c == '!' || c == '?')
                .count();
        }
        let reading_time = if words > 0 {
            (words as f32 / 200.0).max(0.1)
        } else {
            0.0
        };
        DocumentStats {
            word_count: words,
            char_count: chars,
            char_count_no_spaces: chars_no_spaces,
            block_count: self.blocks.len(),
            sentence_count: sentences.max(if words > 0 { 1 } else { 0 }),
            reading_time_minutes: reading_time,
        }
    }

    /// Locates word boundaries `(start_char, end_char)` for a character position in a block.
    pub fn find_word_boundaries(
        &self,
        block_index: usize,
        char_offset: usize,
    ) -> Option<(usize, usize)> {
        let block = self.blocks.get(block_index)?;
        let chars: Vec<char> = block.text.as_str().chars().collect();
        if chars.is_empty() {
            return Some((0, 0));
        }
        let pos = char_offset.min(chars.len().saturating_sub(1));

        let is_word_char = |c: char| c.is_alphanumeric() || c == '_';
        let target_is_word = is_word_char(chars[pos]);

        let mut start = pos;
        while start > 0 && is_word_char(chars[start - 1]) == target_is_word {
            start -= 1;
        }

        let mut end = pos;
        while end < chars.len() && is_word_char(chars[end]) == target_is_word {
            end += 1;
        }

        Some((start, end))
    }

    /// Estimate long-form document metrics including page count, word count, character count, and reading time.
    pub fn estimate_pagination(&self) -> PaginationMetrics {
        let plain = self.plain_text();
        let words = plain.split_whitespace().count();
        let characters = plain.chars().count();
        let pages_by_words = words.div_ceil(250);
        let pages_by_chars = characters.div_ceil(1500);
        let total_pages = pages_by_words.max(pages_by_chars).max(1);
        let reading_time_minutes = (words as f32 / 200.0).max(0.1);
        PaginationMetrics {
            total_pages,
            words,
            characters,
            reading_time_minutes,
        }
    }

    /// Split a block at byte offset, creating a new block with remaining text and preserving styles.
    pub fn split_block(&mut self, block_id: u64, byte_offset: usize) -> Result<u64, WriterError> {
        let index = self
            .blocks
            .iter()
            .position(|b| b.id == block_id)
            .ok_or_else(|| WriterError::Invalid(format!("block {block_id} not found")))?;
        let block = &self.blocks[index];
        let text_len = block.text.as_str().len();
        let offset = byte_offset.min(text_len);

        let first_text = &block.text.as_str()[..offset];
        let second_text = &block.text.as_str()[offset..];

        let new_id = self.next_id();
        let mut first_block = RichBlock::new(block.id, &block.kind, first_text);
        first_block.style = block.style.clone();
        for run in &block.runs {
            if run.start < offset {
                first_block.runs.push(loom_text::StyleRun {
                    start: run.start,
                    end: run.end.min(offset),
                    style: run.style.clone(),
                });
            }
        }

        let mut second_block = RichBlock::new(new_id, &block.kind, second_text);
        second_block.style = block.style.clone();
        for run in &block.runs {
            if run.end > offset {
                let start = run.start.saturating_sub(offset);
                let end = run.end - offset;
                second_block.runs.push(loom_text::StyleRun {
                    start,
                    end,
                    style: run.style.clone(),
                });
            }
        }

        self.blocks[index] = first_block;
        self.blocks.insert(index + 1, second_block);
        Ok(new_id)
    }

    /// Merge two adjacent blocks.
    pub fn merge_blocks(&mut self, first_id: u64, second_id: u64) -> Result<(), WriterError> {
        let first_index = self
            .blocks
            .iter()
            .position(|b| b.id == first_id)
            .ok_or_else(|| WriterError::Invalid(format!("block {first_id} not found")))?;
        let second_index = self
            .blocks
            .iter()
            .position(|b| b.id == second_id)
            .ok_or_else(|| WriterError::Invalid(format!("block {second_id} not found")))?;
        if second_index != first_index + 1 {
            return Err(WriterError::Invalid(
                "blocks must be adjacent to merge".into(),
            ));
        }

        let first = &self.blocks[first_index];
        let second = &self.blocks[second_index];
        let first_len = first.text.as_str().len();
        let combined_text = format!("{}{}", first.text.as_str(), second.text.as_str());

        let mut combined_block = RichBlock::new(first.id, &first.kind, &combined_text);
        combined_block.style = first.style.clone();
        combined_block.runs.extend(first.runs.clone());
        for run in &second.runs {
            combined_block.runs.push(loom_text::StyleRun {
                start: run.start + first_len,
                end: run.end + first_len,
                style: run.style.clone(),
            });
        }

        self.blocks[first_index] = combined_block;
        self.blocks.remove(second_index);
        Ok(())
    }

    /// Formats a sub-range within a block with a given character style.
    pub fn format_block_range(
        &mut self,
        block_id: u64,
        start_byte: usize,
        end_byte: usize,
        style: loom_text::CharacterStyle,
    ) -> Result<(), WriterError> {
        let block = self
            .blocks
            .iter_mut()
            .find(|b| b.id == block_id)
            .ok_or_else(|| WriterError::Invalid(format!("block {block_id} not found")))?;
        let text_len = block.text.as_str().len();
        let start = start_byte.min(text_len);
        let end = end_byte.min(text_len);
        if start >= end {
            return Ok(());
        }
        block
            .runs
            .retain(|run| run.end <= start || run.start >= end);
        block.runs.push(loom_text::StyleRun { start, end, style });
        block.runs.sort_by_key(|run| run.start);
        Ok(())
    }

    /// Sets paragraph alignment for a specific block.
    pub fn set_block_alignment(
        &mut self,
        block_id: u64,
        alignment: loom_text::Alignment,
    ) -> Result<(), WriterError> {
        let block = self
            .blocks
            .iter_mut()
            .find(|b| b.id == block_id)
            .ok_or_else(|| WriterError::Invalid(format!("block {block_id} not found")))?;
        block.style.alignment = alignment;
        Ok(())
    }

    /// Sets block kind (e.g. "heading1", "paragraph", "quote", "bullet").
    pub fn set_block_kind(
        &mut self,
        block_id: u64,
        kind: impl Into<String>,
    ) -> Result<(), WriterError> {
        let block = self
            .blocks
            .iter_mut()
            .find(|b| b.id == block_id)
            .ok_or_else(|| WriterError::Invalid(format!("block {block_id} not found")))?;
        block.kind = kind.into();
        Ok(())
    }

    /// Sets paragraph line spacing multiplier and space after in points for a block.
    pub fn set_block_spacing(
        &mut self,
        block_id: u64,
        line_spacing: f32,
        space_after: f32,
    ) -> Result<(), WriterError> {
        let block = self
            .blocks
            .iter_mut()
            .find(|b| b.id == block_id)
            .ok_or_else(|| WriterError::Invalid(format!("block {block_id} not found")))?;
        if line_spacing > 0.0 {
            block.style.line_spacing = line_spacing;
        }
        if space_after >= 0.0 {
            block.style.space_after = space_after;
        }
        Ok(())
    }
}

/// An entry in the document's table of contents outline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TocEntry {
    /// ID of the heading block.
    pub block_id: u64,
    /// Heading text content.
    pub title: String,
    /// Level (1 for h1, 2 for h2, ..., 6 for h6).
    pub level: u8,
}

/// Long-form document pagination and reading metrics.
#[derive(Debug, Clone, PartialEq)]
pub struct PaginationMetrics {
    /// Estimated total formatted pages.
    pub total_pages: usize,
    /// Total word count.
    pub words: usize,
    /// Total character count.
    pub characters: usize,
    /// Estimated reading time in minutes.
    pub reading_time_minutes: f32,
}

/// Comprehensive statistics for a written document.
#[derive(Debug, Clone, PartialEq)]
pub struct DocumentStats {
    /// Total words across all blocks.
    pub word_count: usize,
    /// Total characters (including whitespace).
    pub char_count: usize,
    /// Total characters (excluding whitespace).
    pub char_count_no_spaces: usize,
    /// Total paragraphs / blocks.
    pub block_count: usize,
    /// Total sentences across all blocks.
    pub sentence_count: usize,
    /// Estimated reading time in minutes (assuming 200 WPM).
    pub reading_time_minutes: f32,
}

fn alignment_name(alignment: loom_text::Alignment) -> &'static str {
    match alignment {
        loom_text::Alignment::Left => "left",
        loom_text::Alignment::Center => "center",
        loom_text::Alignment::Right => "right",
        loom_text::Alignment::Justify => "justify",
    }
}

fn parse_alignment(value: &str) -> loom_text::Alignment {
    match value {
        "center" => loom_text::Alignment::Center,
        "right" => loom_text::Alignment::Right,
        "justify" => loom_text::Alignment::Justify,
        _ => loom_text::Alignment::Left,
    }
}

fn line_break_name(rule: loom_text::LineBreakRule) -> &'static str {
    match rule {
        loom_text::LineBreakRule::NoBreak => "no-break",
        loom_text::LineBreakRule::Word => "word",
        loom_text::LineBreakRule::Char => "char",
    }
}

fn parse_line_break(value: &str) -> loom_text::LineBreakRule {
    match value {
        "no-break" => loom_text::LineBreakRule::NoBreak,
        "char" => loom_text::LineBreakRule::Char,
        _ => loom_text::LineBreakRule::Word,
    }
}

fn font_weight_name(weight: loom_text::FontWeight) -> &'static str {
    match weight {
        loom_text::FontWeight::Thin => "thin",
        loom_text::FontWeight::Light => "light",
        loom_text::FontWeight::Regular => "regular",
        loom_text::FontWeight::Medium => "medium",
        loom_text::FontWeight::Semibold => "semibold",
        loom_text::FontWeight::Bold => "bold",
        loom_text::FontWeight::Black => "black",
    }
}

fn parse_font_weight(value: &str) -> loom_text::FontWeight {
    match value {
        "thin" => loom_text::FontWeight::Thin,
        "light" => loom_text::FontWeight::Light,
        "medium" => loom_text::FontWeight::Medium,
        "semibold" => loom_text::FontWeight::Semibold,
        "bold" => loom_text::FontWeight::Bold,
        "black" => loom_text::FontWeight::Black,
        _ => loom_text::FontWeight::Regular,
    }
}

fn json_f32(value: f32) -> String {
    if value.is_finite() {
        value.to_string()
    } else {
        // JSON has no NaN or infinity. Styles normally contain finite values,
        // but keeping the content writer valid is safer than emitting invalid
        // package data if a caller supplies a non-finite value.
        "0.0".into()
    }
}

fn json_bool(value: bool) -> &'static str {
    if value {
        "true"
    } else {
        "false"
    }
}

fn push_optional_json_string(out: &mut String, value: Option<&str>) {
    match value {
        Some(value) => out.push_str(&pkg_json::escape(value)),
        None => out.push_str("null"),
    }
}

fn paragraph_style_json(style: &loom_text::ParagraphStyle) -> String {
    let mut out = String::from("{\"alignment\":");
    out.push_str(&pkg_json::escape(alignment_name(style.alignment)));
    out.push_str(",\"space_before\":");
    out.push_str(&json_f32(style.space_before));
    out.push_str(",\"space_after\":");
    out.push_str(&json_f32(style.space_after));
    out.push_str(",\"line_spacing\":");
    out.push_str(&json_f32(style.line_spacing));
    out.push_str(",\"left_indent\":");
    out.push_str(&json_f32(style.left_indent));
    out.push_str(",\"right_indent\":");
    out.push_str(&json_f32(style.right_indent));
    out.push_str(",\"first_line_indent\":");
    out.push_str(&json_f32(style.first_line_indent));
    out.push_str(",\"break_rule\":");
    out.push_str(&pkg_json::escape(line_break_name(style.break_rule)));
    out.push('}');
    out
}

fn character_style_json(style: &loom_text::CharacterStyle) -> String {
    let mut out = String::from("{\"font_family\":");
    out.push_str(&pkg_json::escape(&style.font_family));
    out.push_str(",\"font_size\":");
    out.push_str(&json_f32(style.font_size));
    out.push_str(",\"weight\":");
    out.push_str(&pkg_json::escape(font_weight_name(style.weight)));
    out.push_str(",\"italic\":");
    out.push_str(json_bool(style.italic));
    out.push_str(",\"underline\":");
    out.push_str(json_bool(style.underline));
    out.push_str(",\"strikethrough\":");
    out.push_str(json_bool(style.strikethrough));
    out.push_str(",\"superscript\":");
    out.push_str(json_bool(style.superscript));
    out.push_str(",\"subscript\":");
    out.push_str(json_bool(style.subscript));
    out.push_str(",\"color\":");
    push_optional_json_string(&mut out, style.color.as_deref());
    out.push_str(",\"background\":");
    push_optional_json_string(&mut out, style.background.as_deref());
    out.push('}');
    out
}

fn runs_json(runs: &[loom_text::StyleRun]) -> String {
    let mut out = String::from("[");
    for (index, run) in runs.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str("{\"start\":");
        out.push_str(&run.start.to_string());
        out.push_str(",\"end\":");
        out.push_str(&run.end.to_string());
        out.push_str(",\"style\":");
        out.push_str(&character_style_json(&run.style));
        out.push('}');
    }
    out.push(']');
    out
}

/// Match exact paragraph text in order, retaining the stable metadata of the
/// old block. LCS gives insertions and deletions in the middle the same
/// behavior as edits at the beginning or end, instead of relying on position.
fn exact_paragraph_matches(old: &[RichBlock], new: &[String]) -> Vec<Option<usize>> {
    let mut lcs = vec![vec![0usize; new.len() + 1]; old.len() + 1];
    for old_index in (0..old.len()).rev() {
        for new_index in (0..new.len()).rev() {
            lcs[old_index][new_index] = if old[old_index].text.as_str() == new[new_index] {
                1 + lcs[old_index + 1][new_index + 1]
            } else {
                lcs[old_index + 1][new_index].max(lcs[old_index][new_index + 1])
            };
        }
    }

    let mut matches = vec![None; new.len()];
    let (mut old_index, mut new_index) = (0, 0);
    while old_index < old.len() && new_index < new.len() {
        if old[old_index].text.as_str() == new[new_index]
            && lcs[old_index][new_index] == 1 + lcs[old_index + 1][new_index + 1]
        {
            matches[new_index] = Some(old_index);
            old_index += 1;
            new_index += 1;
        } else if lcs[old_index + 1][new_index] >= lcs[old_index][new_index + 1] {
            old_index += 1;
        } else {
            new_index += 1;
        }
    }
    matches
}

/// Add conservative metadata matches for changed paragraphs in an unmatched
/// gap. Equal-sized gaps retain positional matching. For unequal gaps, a
/// small sequence alignment pairs only sufficiently similar text, leaving
/// unrelated insertions with fresh metadata instead of borrowing a deleted
/// block's id, kind, or style.
fn metadata_matches(
    old: &[RichBlock],
    new: &[String],
    exact_matches: &[Option<usize>],
) -> Vec<Option<usize>> {
    let anchors = exact_matches
        .iter()
        .enumerate()
        .filter_map(|(new_index, old_index)| old_index.map(|old_index| (new_index, old_index)));
    let mut matches = exact_matches.to_vec();
    let mut old_start = 0;
    let mut new_start = 0;

    for (new_end, old_end) in anchors.chain(std::iter::once((new.len(), old.len()))) {
        let old_gap_len = old_end - old_start;
        let new_gap_len = new_end - new_start;
        if old_gap_len == new_gap_len {
            for offset in 0..new_gap_len {
                if matches[new_start + offset].is_none() {
                    matches[new_start + offset] = Some(old_start + offset);
                }
            }
        } else if old_gap_len > 0 && new_gap_len > 0 {
            for (new_offset, old_offset) in
                align_metadata_gap(&old[old_start..old_end], &new[new_start..new_end])
            {
                matches[new_start + new_offset] = Some(old_start + old_offset);
            }
        }

        if new_end < new.len() {
            old_start = old_end + 1;
            new_start = new_end + 1;
        }
    }

    matches
}

fn align_metadata_gap(old: &[RichBlock], new: &[String]) -> Vec<(usize, usize)> {
    let mut scores = vec![vec![0usize; new.len() + 1]; old.len() + 1];
    for old_index in (0..old.len()).rev() {
        for new_index in (0..new.len()).rev() {
            let skip_old = scores[old_index + 1][new_index];
            let skip_new = scores[old_index][new_index + 1];
            let pair = paragraph_similarity(&old[old_index].text, &new[new_index])
                .map(|score| score + scores[old_index + 1][new_index + 1])
                .unwrap_or(0);
            scores[old_index][new_index] = skip_old.max(skip_new).max(pair);
        }
    }

    let mut pairs = Vec::new();
    let (mut old_index, mut new_index) = (0, 0);
    while old_index < old.len() && new_index < new.len() {
        let score = paragraph_similarity(&old[old_index].text, &new[new_index]);
        let pair = score
            .map(|score| score + scores[old_index + 1][new_index + 1])
            .unwrap_or(0);
        let skip_old = scores[old_index + 1][new_index];
        let skip_new = scores[old_index][new_index + 1];
        if score.is_some() && pair >= skip_old.max(skip_new) {
            pairs.push((new_index, old_index));
            old_index += 1;
            new_index += 1;
        } else if skip_old >= skip_new {
            old_index += 1;
        } else {
            new_index += 1;
        }
    }
    pairs
}

/// Return a normalized similarity score when the unchanged prefix/suffix is
/// substantial enough to identify an edited paragraph. The score is scaled so
/// sequence alignment can compare multiple candidate pairs without floats.
fn paragraph_similarity(old: &Text, new: &str) -> Option<usize> {
    let old_chars: Vec<char> = old.as_str().chars().collect();
    let new_chars: Vec<char> = new.chars().collect();
    let prefix = old_chars
        .iter()
        .zip(&new_chars)
        .take_while(|(old, new)| old == new)
        .count();
    let max_suffix = (old_chars.len() - prefix).min(new_chars.len() - prefix);
    let suffix = (0..max_suffix)
        .take_while(|offset| {
            old_chars[old_chars.len() - 1 - offset] == new_chars[new_chars.len() - 1 - offset]
        })
        .count();
    let shared = prefix + suffix;
    let shorter = old_chars.len().min(new_chars.len());
    if shared == 0 || shorter == 0 || shared * 2 < shorter {
        return None;
    }
    Some(shared * 1_000 / old_chars.len().max(new_chars.len()))
}

/// Remap byte-ranged character styles across the single contiguous edit that
/// a native text editor normally reports for one change. Text outside the
/// changed range keeps its exact run; an insertion inside or at the edge of a
/// run extends that run so typing does not discard the active style.
fn remap_style_runs(
    old_text: &str,
    new_text: &str,
    runs: &[loom_text::StyleRun],
) -> Vec<loom_text::StyleRun> {
    if runs.is_empty() || old_text == new_text {
        return runs.to_vec();
    }

    let old_chars: Vec<char> = old_text.chars().collect();
    let new_chars: Vec<char> = new_text.chars().collect();
    let prefix_chars = old_chars
        .iter()
        .zip(&new_chars)
        .take_while(|(old, new)| old == new)
        .count();
    let max_suffix_chars = (old_chars.len() - prefix_chars).min(new_chars.len() - prefix_chars);
    let suffix_chars = (0..max_suffix_chars)
        .take_while(|offset| {
            old_chars[old_chars.len() - 1 - offset] == new_chars[new_chars.len() - 1 - offset]
        })
        .count();

    let old_change_start = byte_offset(&old_chars, prefix_chars);
    let old_change_end = byte_offset(&old_chars, old_chars.len() - suffix_chars);
    let new_change_start = byte_offset(&new_chars, prefix_chars);
    let new_change_end = byte_offset(&new_chars, new_chars.len() - suffix_chars);
    let delta = new_change_end as isize
        - new_change_start as isize
        - (old_change_end as isize - old_change_start as isize);
    let insertion = old_change_start == old_change_end;

    runs.iter()
        .filter_map(|run| {
            let (start, end) = if insertion {
                if run.end < old_change_start {
                    (run.start, run.end)
                } else if run.start > old_change_start {
                    (shift_offset(run.start, delta), shift_offset(run.end, delta))
                } else {
                    (run.start, shift_offset(run.end, delta))
                }
            } else if run.end <= old_change_start {
                (run.start, run.end)
            } else if run.start >= old_change_end {
                (shift_offset(run.start, delta), shift_offset(run.end, delta))
            } else {
                let start = if run.start < old_change_start {
                    run.start
                } else {
                    new_change_start
                };
                let end = if run.end > old_change_end {
                    shift_offset(run.end, delta)
                } else {
                    new_change_end
                };
                (start, end)
            };
            let start = start.min(new_text.len());
            let end = end.min(new_text.len());
            (start < end).then(|| loom_text::StyleRun {
                start,
                end,
                style: run.style.clone(),
            })
        })
        .collect()
}

fn byte_offset(chars: &[char], count: usize) -> usize {
    chars
        .iter()
        .take(count)
        .map(|character| character.len_utf8())
        .sum()
}

fn shift_offset(offset: usize, delta: isize) -> usize {
    if delta >= 0 {
        offset.saturating_add(delta as usize)
    } else {
        offset.saturating_sub(delta.unsigned_abs())
    }
}

/// Parse error for Writer content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriterError {
    /// JSON parse issue.
    Json(String),
    /// Invalid document structure.
    Invalid(String),
}

impl std::fmt::Display for WriterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json(e) => write!(f, "json: {e}"),
            Self::Invalid(e) => write!(f, "invalid: {e}"),
        }
    }
}

impl std::error::Error for WriterError {}

fn parse_blocks(raw: &str) -> Result<Vec<RichBlock>, WriterError> {
    // raw is a JSON array string. Reuse a bounded parser.
    let p = ContentParser::new(raw);
    let items = p.parse_array()?;
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let fields = ContentParser::new(&item).parse()?;
        let mut id = 0u64;
        let mut kind = String::new();
        let mut text = String::new();
        let mut legacy_alignment = None;
        let mut style_raw = None;
        let mut runs_raw = None;
        for (k, v) in &fields {
            match k.as_str() {
                "id" => {
                    id = json_value_text(v)
                        .parse()
                        .map_err(|_| WriterError::Invalid("bad id".into()))?
                }
                "kind" => kind = json_value_text(v),
                "text" => text = json_value_text(v),
                "align" => legacy_alignment = Some(json_value_text(v)),
                "style" => {
                    if let JsonValue::Raw(raw) = v {
                        style_raw = Some(raw.clone());
                    }
                }
                "runs" => {
                    if let JsonValue::Raw(raw) = v {
                        runs_raw = Some(raw.clone());
                    }
                }
                _ => {}
            }
        }
        let mut b = RichBlock::new(id, &kind, &text);
        if let Some(alignment) = legacy_alignment {
            b.style.alignment = parse_alignment(&alignment);
        }
        if let Some(raw) = style_raw {
            b.style = parse_paragraph_style(&raw, b.style)?;
        }
        if let Some(raw) = runs_raw {
            b.runs = parse_runs(&raw)?;
        }
        out.push(b);
    }
    Ok(out)
}

fn parse_paragraph_style(
    raw: &str,
    mut style: loom_text::ParagraphStyle,
) -> Result<loom_text::ParagraphStyle, WriterError> {
    let fields = ContentParser::new(raw).parse()?;
    for (key, value) in &fields {
        match key.as_str() {
            "alignment" | "align" => {
                style.alignment = parse_alignment(&json_value_text(value));
            }
            "space_before" => parse_f32_value(value, &mut style.space_before),
            "space_after" => parse_f32_value(value, &mut style.space_after),
            "line_spacing" => parse_f32_value(value, &mut style.line_spacing),
            "left_indent" => parse_f32_value(value, &mut style.left_indent),
            "right_indent" => parse_f32_value(value, &mut style.right_indent),
            "first_line_indent" => parse_f32_value(value, &mut style.first_line_indent),
            "break_rule" => {
                style.break_rule = parse_line_break(&json_value_text(value));
            }
            _ => {}
        }
    }
    Ok(style)
}

fn parse_runs(raw: &str) -> Result<Vec<loom_text::StyleRun>, WriterError> {
    let items = ContentParser::new(raw).parse_array()?;
    let mut runs = Vec::with_capacity(items.len());
    for item in items {
        let fields = ContentParser::new(&item).parse()?;
        let mut start = 0usize;
        let mut end = 0usize;
        let mut style = loom_text::CharacterStyle::default();
        let mut style_raw = None;
        for (key, value) in &fields {
            match key.as_str() {
                "start" => {
                    start = json_value_text(value)
                        .parse()
                        .map_err(|_| WriterError::Invalid("bad run start".into()))?;
                }
                "end" => {
                    end = json_value_text(value)
                        .parse()
                        .map_err(|_| WriterError::Invalid("bad run end".into()))?;
                }
                "style" => {
                    if let JsonValue::Raw(raw) = value {
                        style_raw = Some(raw.clone());
                    }
                }
                _ => {}
            }
        }
        if let Some(raw) = style_raw {
            style = parse_character_style(&raw, style)?;
        }
        runs.push(loom_text::StyleRun { start, end, style });
    }
    Ok(runs)
}

fn parse_character_style(
    raw: &str,
    mut style: loom_text::CharacterStyle,
) -> Result<loom_text::CharacterStyle, WriterError> {
    let fields = ContentParser::new(raw).parse()?;
    for (key, value) in &fields {
        match key.as_str() {
            "font_family" => {
                if let JsonValue::String(value) = value {
                    style.font_family = value.clone();
                }
            }
            "font_size" => parse_f32_value(value, &mut style.font_size),
            "weight" => {
                style.weight = parse_font_weight(&json_value_text(value));
            }
            "italic" => parse_bool_value(value, &mut style.italic),
            "underline" => parse_bool_value(value, &mut style.underline),
            "strikethrough" => parse_bool_value(value, &mut style.strikethrough),
            "superscript" => parse_bool_value(value, &mut style.superscript),
            "subscript" => parse_bool_value(value, &mut style.subscript),
            "color" => {
                if let Some(value) = parse_optional_string(value) {
                    style.color = value;
                }
            }
            "background" => {
                if let Some(value) = parse_optional_string(value) {
                    style.background = value;
                }
            }
            _ => {}
        }
    }
    Ok(style)
}

fn parse_f32_value(value: &JsonValue, target: &mut f32) {
    if let Ok(parsed) = json_value_text(value).parse::<f32>() {
        if parsed.is_finite() {
            *target = parsed;
        }
    }
}

fn parse_bool_value(value: &JsonValue, target: &mut bool) {
    if let JsonValue::Raw(value) = value {
        match value.as_str() {
            "true" => *target = true,
            "false" => *target = false,
            _ => {}
        }
    }
}

fn parse_optional_string(value: &JsonValue) -> Option<Option<String>> {
    match value {
        JsonValue::String(value) => Some(Some(value.clone())),
        JsonValue::Raw(value) if value == "null" => Some(None),
        _ => None,
    }
}

fn json_value_text(value: &JsonValue) -> String {
    match value {
        JsonValue::String(value) | JsonValue::Raw(value) => value.clone(),
    }
}

/// A tiny bounded JSON parser for Writer content (reuses the same structure
/// as loom_package's manifest parser but simpler; keeps the workspace
/// dependency-free).
#[derive(Debug, Clone, PartialEq, Eq)]
enum JsonValue {
    /// A decoded JSON string, retaining its type so a string "null" is not
    /// confused with the JSON null value used by optional character colors.
    String(String),
    /// An unquoted primitive or a balanced object/array.
    Raw(String),
}

struct ContentParser {
    bytes: Vec<u8>,
    pos: usize,
}

impl ContentParser {
    fn new(s: &str) -> Self {
        Self {
            bytes: s.as_bytes().to_vec(),
            pos: 0,
        }
    }

    fn skip_ws(&mut self) {
        while self.pos < self.bytes.len()
            && matches!(self.bytes[self.pos], b' ' | b'\t' | b'\n' | b'\r')
        {
            self.pos += 1;
        }
    }

    fn parse(&self) -> Result<Vec<(String, JsonValue)>, WriterError> {
        // Parse an object while retaining whether a value was a JSON string.
        // Optional style fields need this distinction because `"null"` is a
        // valid color value and must not be confused with JSON null.
        let mut p = self.clone();
        p.skip_ws();
        if p.next() != Some(b'{') {
            return Err(WriterError::Json("expected object".into()));
        }
        let mut fields = Vec::new();
        p.skip_ws();
        if p.peek() == Some(b'}') {
            return Ok(fields);
        }
        loop {
            p.skip_ws();
            let key = p.parse_string()?;
            p.skip_ws();
            if p.next() != Some(b':') {
                return Err(WriterError::Json("expected ':'".into()));
            }
            let value = p.capture_typed_value()?;
            fields.push((key, value));
            p.skip_ws();
            match p.peek() {
                Some(b',') => {
                    p.pos += 1;
                }
                Some(b'}') => {
                    p.pos += 1;
                    break;
                }
                _ => return Err(WriterError::Json("expected ',' or '}'".into())),
            }
        }
        Ok(fields)
    }

    fn capture_typed_value(&mut self) -> Result<JsonValue, WriterError> {
        self.skip_ws();
        if self.peek() == Some(b'"') {
            Ok(JsonValue::String(self.parse_string()?))
        } else {
            Ok(JsonValue::Raw(self.capture_value()?))
        }
    }

    fn parse_array(&self) -> Result<Vec<String>, WriterError> {
        let mut p = self.clone();
        p.skip_ws();
        if p.next() != Some(b'[') {
            return Err(WriterError::Json("expected array".into()));
        }
        let mut items = Vec::new();
        p.skip_ws();
        if p.peek() == Some(b']') {
            return Ok(items);
        }
        loop {
            p.skip_ws();
            let raw = p.capture_value()?;
            items.push(raw);
            p.skip_ws();
            match p.peek() {
                Some(b',') => {
                    p.pos += 1;
                }
                Some(b']') => {
                    p.pos += 1;
                    break;
                }
                _ => return Err(WriterError::Json("expected ',' or ']'".into())),
            }
        }
        Ok(items)
    }

    fn capture_value(&mut self) -> Result<String, WriterError> {
        self.skip_ws();
        let start = self.pos;
        // For objects/arrays, do a balanced capture; for strings use parse_string.
        match self.peek() {
            Some(b'"') => self.parse_string(),
            Some(b'{') => {
                let mut depth = 0;
                loop {
                    let b = self
                        .next()
                        .ok_or_else(|| WriterError::Json("unterminated object".into()))?;
                    if b == b'{' {
                        depth += 1;
                    } else if b == b'}' {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    } else if b == b'"' {
                        // Rewind to the opening quote, then skip the string so
                        // braces inside it are not counted.
                        self.pos -= 1;
                        let _ = self.parse_string();
                    }
                }
                Ok(std::str::from_utf8(&self.bytes[start..self.pos])
                    .map_err(|_| WriterError::Json("invalid utf8".into()))?
                    .to_string())
            }
            Some(b'[') => {
                let mut depth = 0;
                loop {
                    let b = self
                        .next()
                        .ok_or_else(|| WriterError::Json("unterminated array".into()))?;
                    if b == b'[' {
                        depth += 1;
                    } else if b == b']' {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    } else if b == b'"' {
                        self.pos -= 1;
                        let _ = self.parse_string();
                    }
                }
                Ok(std::str::from_utf8(&self.bytes[start..self.pos])
                    .map_err(|_| WriterError::Json("invalid utf8".into()))?
                    .to_string())
            }
            Some(_) => {
                // Number/true/false/null.
                while self.pos < self.bytes.len()
                    && !matches!(
                        self.bytes[self.pos],
                        b',' | b'}' | b']' | b' ' | b'\t' | b'\n' | b'\r'
                    )
                {
                    self.pos += 1;
                }
                let raw = std::str::from_utf8(&self.bytes[start..self.pos])
                    .map_err(|_| WriterError::Json("invalid value".into()))?
                    .to_string();
                if raw.is_empty() {
                    Err(WriterError::Json("empty value".into()))
                } else {
                    Ok(raw)
                }
            }
            None => Err(WriterError::Json("unexpected end".into())),
        }
    }

    fn parse_string(&mut self) -> Result<String, WriterError> {
        if self.next() != Some(b'"') {
            return Err(WriterError::Json("expected string".into()));
        }
        let mut out = String::new();
        loop {
            let b = self
                .next()
                .ok_or_else(|| WriterError::Json("unterminated string".into()))?;
            match b {
                b'"' => break,
                b'\\' => {
                    let esc = self
                        .next()
                        .ok_or_else(|| WriterError::Json("bad escape".into()))?;
                    match esc {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let mut cp = 0u32;
                            for _ in 0..4 {
                                let h = self
                                    .next()
                                    .ok_or_else(|| WriterError::Json("bad unicode".into()))?;
                                let v = match h {
                                    b'0'..=b'9' => h - b'0',
                                    b'a'..=b'f' => h - b'a' + 10,
                                    b'A'..=b'F' => h - b'A' + 10,
                                    _ => return Err(WriterError::Json("bad unicode".into())),
                                };
                                cp = (cp << 4) | v as u32;
                            }
                            if let Some(c) = char::from_u32(cp) {
                                out.push(c);
                            }
                        }
                        _ => return Err(WriterError::Json("bad escape".into())),
                    }
                }
                b if b < 0x20 => return Err(WriterError::Json("control char in string".into())),
                _ => {
                    // Assemble UTF-8 sequence.
                    let mut seq = vec![b];
                    let needed = if b >= 0xF0 {
                        3
                    } else if b >= 0xE0 {
                        2
                    } else if b >= 0xC0 {
                        1
                    } else {
                        0
                    };
                    for _ in 0..needed {
                        let nb = self
                            .next()
                            .ok_or_else(|| WriterError::Json("truncated utf8".into()))?;
                        seq.push(nb);
                    }
                    let s = std::str::from_utf8(&seq)
                        .map_err(|_| WriterError::Json("invalid utf8".into()))?;
                    out.push_str(s);
                }
            }
        }
        Ok(out)
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn next(&mut self) -> Option<u8> {
        let b = self.peek();
        if b.is_some() {
            self.pos += 1;
        }
        b
    }
}

impl Clone for ContentParser {
    fn clone(&self) -> Self {
        Self {
            bytes: self.bytes.clone(),
            pos: self.pos,
        }
    }
}

/// Save a document to a `.loomdoc` byte buffer (ZIP + manifest).
pub fn save_document(doc: &WriterDocument) -> Result<Vec<u8>, loom_package::zip::ArchiveError> {
    let mut arch = PackageArchive::new();
    let content = doc.to_content_json();
    arch.add("content/document.json", content.into_bytes())?;
    let manifest = Manifest {
        schema: SchemaVersion::CURRENT,
        kind: PackageKind::Writer,
        id: doc.id.clone(),
        title: doc.title.clone(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        entries: vec![ManifestEntry {
            path: "content/document.json".into(),
            mime: MimeType::parse("application/vnd.loom.document-content")?,
            size: doc.to_content_json().len() as u64,
            sha256: Checksum::from_bytes(zip::sha256(doc.to_content_json().as_bytes())),
        }],
    };
    let manifest_str = pkg_json::write(&manifest);
    arch.add("manifest.json", manifest_str.into_bytes())?;
    arch.to_bytes()
}

/// Load a document from a `.loomdoc` byte buffer, validating the manifest.
pub fn load_document(bytes: &[u8]) -> Result<WriterDocument, WriterError> {
    let arch = PackageArchive::from_bytes(bytes)
        .map_err(|e| WriterError::Invalid(format!("archive: {e}")))?;
    let manifest_bytes = arch
        .get("manifest.json")
        .ok_or_else(|| WriterError::Invalid("missing manifest.json".into()))?;
    let manifest_str = std::str::from_utf8(manifest_bytes)
        .map_err(|_| WriterError::Invalid("manifest not utf8".into()))?;
    let manifest: Manifest = pkg_json::parse_manifest(manifest_str)
        .map_err(|e| WriterError::Invalid(format!("manifest: {e}")))?;
    if manifest.kind != PackageKind::Writer {
        return Err(WriterError::Invalid("not a Writer document".into()));
    }
    arch.validate_manifest(&manifest)
        .map_err(|e| WriterError::Invalid(format!("validation: {e}")))?;
    let content = arch
        .get("content/document.json")
        .ok_or_else(|| WriterError::Invalid("missing document.json".into()))?;
    let s = std::str::from_utf8(content)
        .map_err(|_| WriterError::Invalid("content not utf8".into()))?;
    WriterDocument::from_content_json(s)
}

/// Render the document to a single A4 PDF page using the shared
/// deterministic PDF writer. Output is byte-for-byte deterministic for the
/// same document.
pub fn export_pdf(doc: &WriterDocument) -> Vec<u8> {
    use loom_pdf::{PdfDocument, TextStyle};
    let mut pdf = PdfDocument::new();
    let page = pdf.add_page(595.0, 842.0);
    pdf.draw_text(
        page,
        56.0,
        790.0,
        &doc.title,
        &TextStyle {
            size_pt: 20.0,
            bold: true,
            ..Default::default()
        },
    );
    let body = TextStyle {
        size_pt: 11.0,
        fill_rgb: (0.15, 0.13, 0.11),
        ..Default::default()
    };
    let mut y = 760.0;
    for b in &doc.blocks {
        let style = match b.kind.as_str() {
            "heading1" => TextStyle {
                size_pt: 15.0,
                bold: true,
                ..Default::default()
            },
            "heading2" => TextStyle {
                size_pt: 13.0,
                bold: true,
                ..Default::default()
            },
            _ => body.clone(),
        };
        pdf.draw_text(page, 56.0, y, b.text.as_str(), &style);
        y -= 22.0;
    }
    pdf.serialize()
}

/// One text-search hit in a document block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchMatch {
    /// Stable block id.
    pub block_id: u64,
    /// UTF-8 byte offset of the match start.
    pub start: usize,
    /// UTF-8 byte offset immediately after the match.
    pub end: usize,
}

/// One generated table-of-contents entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableOfContentsEntry {
    /// Heading block id.
    pub block_id: u64,
    /// Heading depth from 1 to 6.
    pub level: u8,
    /// Heading text.
    pub title: String,
}

/// Approximate page geometry used by the deterministic reference paginator.
#[derive(Debug, Clone, PartialEq)]
pub struct PageStyle {
    /// Page width in points.
    pub width_pt: f32,
    /// Page height in points.
    pub height_pt: f32,
    /// Top margin in points.
    pub margin_top_pt: f32,
    /// Bottom margin in points.
    pub margin_bottom_pt: f32,
    /// Left margin in points.
    pub margin_left_pt: f32,
    /// Right margin in points.
    pub margin_right_pt: f32,
    /// Body font size in points.
    pub body_font_size_pt: f32,
    /// Line-height multiplier.
    pub line_height: f32,
}

impl Default for PageStyle {
    fn default() -> Self {
        Self {
            width_pt: 595.0,
            height_pt: 842.0,
            margin_top_pt: 72.0,
            margin_bottom_pt: 72.0,
            margin_left_pt: 72.0,
            margin_right_pt: 72.0,
            body_font_size_pt: 11.0,
            line_height: 1.35,
        }
    }
}

/// Standard physical paper sizes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PaperSize {
    #[default]
    A4,
    Letter,
    Legal,
    Executive,
    A3,
    A5,
}

impl PaperSize {
    /// Returns the (width, height) dimensions in points for portrait orientation.
    pub fn dimensions_pt(&self) -> (f32, f32) {
        match self {
            Self::A4 => (595.0, 842.0),
            Self::Letter => (612.0, 792.0),
            Self::Legal => (612.0, 1008.0),
            Self::Executive => (522.0, 756.0),
            Self::A3 => (842.0, 1191.0),
            Self::A5 => (420.0, 595.0),
        }
    }
}

/// Page orientation for layout and printing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PageOrientation {
    #[default]
    Portrait,
    Landscape,
}

/// Standard page margin presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PageMarginsPreset {
    #[default]
    Normal, // 1 inch (72pt)
    Narrow,   // 0.5 inch (36pt)
    Moderate, // 1 inch top/bottom, 0.75 inch left/right (54pt)
    Wide,     // 1 inch top/bottom, 2 inch left/right (144pt)
}

impl PageMarginsPreset {
    /// Returns (top, bottom, left, right) margins in points.
    pub fn margins_pt(&self) -> (f32, f32, f32, f32) {
        match self {
            Self::Normal => (72.0, 72.0, 72.0, 72.0),
            Self::Narrow => (36.0, 36.0, 36.0, 36.0),
            Self::Moderate => (72.0, 72.0, 54.0, 54.0),
            Self::Wide => (72.0, 72.0, 144.0, 144.0),
        }
    }
}

/// Calculates estimated silent reading time in minutes for a given word count.
pub fn calculate_reading_time_minutes(word_count: usize, words_per_minute: u32) -> f32 {
    let wpm = if words_per_minute == 0 {
        200
    } else {
        words_per_minute
    };
    word_count as f32 / wpm as f32
}

/// Calculates estimated spoken presentation time in minutes for a given word count.
pub fn calculate_speaking_time_minutes(word_count: usize, words_per_minute: u32) -> f32 {
    let wpm = if words_per_minute == 0 {
        130
    } else {
        words_per_minute
    };
    word_count as f32 / wpm as f32
}

/// Page number numbering format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PageNumberFormat {
    #[default]
    Arabic,
    RomanUpper,
    RomanLower,
    Alphabetical,
}

impl PageNumberFormat {
    /// Formats a 1-based page number into a string according to this format.
    pub fn format(&self, page_num: usize) -> String {
        match self {
            Self::Arabic => format!("{}", page_num),
            Self::RomanUpper => match page_num {
                1 => "I".into(),
                2 => "II".into(),
                3 => "III".into(),
                4 => "IV".into(),
                5 => "V".into(),
                6 => "VI".into(),
                7 => "VII".into(),
                8 => "VIII".into(),
                9 => "IX".into(),
                10 => "X".into(),
                _ => format!("{}", page_num),
            },
            Self::RomanLower => match page_num {
                1 => "i".into(),
                2 => "ii".into(),
                3 => "iii".into(),
                4 => "iv".into(),
                5 => "v".into(),
                6 => "vi".into(),
                7 => "vii".into(),
                8 => "viii".into(),
                9 => "ix".into(),
                10 => "x".into(),
                _ => format!("{}", page_num),
            },
            Self::Alphabetical => {
                if (1..=26).contains(&page_num) {
                    let ch = (b'A' + (page_num as u8 - 1)) as char;
                    ch.to_string()
                } else {
                    format!("{}", page_num)
                }
            }
        }
    }
}

/// Header and footer layout configuration for paginated documents.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct HeaderFooterConfig {
    /// Header text (supports `{page}` and `{total}` placeholders).
    pub header_text: String,
    /// Footer text (supports `{page}` and `{total}` placeholders).
    pub footer_text: String,
    /// Header/footer text alignment.
    pub alignment: String,
    /// Page number formatting style.
    pub page_number_format: PageNumberFormat,
    /// Whether the first page omits headers and footers (e.g. cover page).
    pub different_first_page: bool,
}

impl HeaderFooterConfig {
    /// Formats header text for a given page index (1-based) and total page count.
    pub fn format_header(&self, page_num: usize, total_pages: usize) -> Option<String> {
        if self.different_first_page && page_num == 1 {
            return None;
        }
        if self.header_text.is_empty() {
            return None;
        }
        let formatted_page = self.page_number_format.format(page_num);
        let res = self
            .header_text
            .replace("{page}", &formatted_page)
            .replace("{total}", &total_pages.to_string());
        Some(res)
    }

    /// Formats footer text for a given page index (1-based) and total page count.
    pub fn format_footer(&self, page_num: usize, total_pages: usize) -> Option<String> {
        if self.different_first_page && page_num == 1 {
            return None;
        }
        if self.footer_text.is_empty() {
            return None;
        }
        let formatted_page = self.page_number_format.format(page_num);
        let res = self
            .footer_text
            .replace("{page}", &formatted_page)
            .replace("{total}", &total_pages.to_string());
        Some(res)
    }
}

/// Footnote or endnote citation entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FootnoteEntry {
    pub id: u64,
    pub marker: String,
    pub text: String,
    pub is_endnote: bool,
}

/// Multi-column page layout configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColumnCount {
    #[default]
    Single,
    TwoColumns,
    ThreeColumns,
}

impl ColumnCount {
    pub fn count(&self) -> usize {
        match self {
            Self::Single => 1,
            Self::TwoColumns => 2,
            Self::ThreeColumns => 3,
        }
    }
}

/// Multi-column layout settings with column gaps and separator line options.
#[derive(Debug, Clone, PartialEq)]
pub struct MultiColumnConfig {
    pub columns: ColumnCount,
    pub column_gap_pt: f32,
    pub show_separator_line: bool,
}

impl Default for MultiColumnConfig {
    fn default() -> Self {
        Self {
            columns: ColumnCount::Single,
            column_gap_pt: 18.0, // 0.25 inch default gap
            show_separator_line: false,
        }
    }
}

impl MultiColumnConfig {
    /// Calculates width of each column given total printable width in points.
    pub fn calculate_column_width(&self, printable_width_pt: f32) -> f32 {
        let count = self.columns.count();
        if count <= 1 {
            return printable_width_pt;
        }
        let total_gap = self.column_gap_pt * (count - 1) as f32;
        let available = (printable_width_pt - total_gap).max(10.0);
        available / count as f32
    }
}

/// Initial drop cap configuration for styled paragraph opening letters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DropCapConfig {
    pub lines: u32,
    pub characters: u32,
    pub enabled: bool,
}

impl Default for DropCapConfig {
    fn default() -> Self {
        Self {
            lines: 3,
            characters: 1,
            enabled: false,
        }
    }
}

/// Diagonal or horizontal document watermark configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct WatermarkConfig {
    pub text: String,
    pub font_size_pt: f32,
    pub color_rgba: [u8; 4],
    pub rotation_deg: f32,
    pub opacity: f32,
    pub enabled: bool,
}

impl Default for WatermarkConfig {
    fn default() -> Self {
        Self {
            text: "DRAFT".into(),
            font_size_pt: 72.0,
            color_rgba: [128, 128, 128, 128],
            rotation_deg: -45.0,
            opacity: 0.25,
            enabled: false,
        }
    }
}

/// Margin line numbering configuration for legal and academic manuscripts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineNumberingConfig {
    pub start_number: u32,
    pub count_by: u32,
    pub restart_each_page: bool,
    pub enabled: bool,
}

impl Default for LineNumberingConfig {
    fn default() -> Self {
        Self {
            start_number: 1,
            count_by: 1,
            restart_each_page: false,
            enabled: false,
        }
    }
}

/// Explicit page and section break kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakKind {
    PageBreak,
    SectionBreakNextPage,
    SectionBreakContinuous,
    ColumnBreak,
}

/// Section break configuration with layout overrides.
#[derive(Debug, Clone, PartialEq)]
pub struct BreakConfig {
    pub kind: BreakKind,
    pub orientation_override: Option<PageOrientation>,
    pub restart_page_numbering: bool,
}

impl BreakConfig {
    pub fn new(kind: BreakKind) -> Self {
        Self {
            kind,
            orientation_override: None,
            restart_page_numbering: false,
        }
    }
}

/// Text hyphenation configuration for justified long-form typesetting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HyphenationConfig {
    /// Minimum length of word eligible for hyphenation.
    pub min_word_length: usize,
    /// Minimum characters before hyphen break.
    pub min_leading_chars: usize,
    /// Minimum characters after hyphen break.
    pub min_trailing_chars: usize,
    /// Hyphenation character (defaults to soft hyphen U+00AD).
    pub hyphen_char: char,
}

impl Default for HyphenationConfig {
    fn default() -> Self {
        Self {
            min_word_length: 6,
            min_leading_chars: 3,
            min_trailing_chars: 3,
            hyphen_char: '\u{00AD}',
        }
    }
}

/// Identifies candidate hyphenation break positions in an English word based on syllable patterns.
pub fn find_hyphenation_points(word: &str, config: &HyphenationConfig) -> Vec<usize> {
    if word.chars().count() < config.min_word_length {
        return Vec::new();
    }

    let is_vowel = |c: char| matches!(c.to_ascii_lowercase(), 'a' | 'e' | 'i' | 'o' | 'u' | 'y');
    let chars: Vec<(usize, char)> = word.char_indices().collect();
    let char_count = chars.len();
    let mut breaks = Vec::new();

    for i in config.min_leading_chars..char_count.saturating_sub(config.min_trailing_chars) {
        let prev_c = chars[i - 1].1;
        let curr_c = chars[i].1;

        // Vowel-Consonant / Consonant-Vowel transitions (basic syllable heuristic)
        if (!is_vowel(prev_c) && is_vowel(curr_c))
            || (is_vowel(prev_c)
                && !is_vowel(curr_c)
                && i + 1 < char_count
                && !is_vowel(chars[i + 1].1))
        {
            breaks.push(chars[i].0);
        }
    }

    breaks
}

/// Inserts soft hyphens into long words across a block of text.
pub fn insert_soft_hyphens(text: &str, config: &HyphenationConfig) -> String {
    let mut result = String::with_capacity(text.len() + 16);
    for word in text.split_inclusive(|c: char| !c.is_alphabetic()) {
        let alpha_part: String = word.chars().take_while(|c| c.is_alphabetic()).collect();
        let trailing_part: String = word.chars().skip(alpha_part.chars().count()).collect();

        let break_points = find_hyphenation_points(&alpha_part, config);
        if break_points.is_empty() {
            result.push_str(word);
        } else {
            let mut last = 0;
            for pt in break_points {
                result.push_str(&alpha_part[last..pt]);
                result.push(config.hyphen_char);
                last = pt;
            }
            result.push_str(&alpha_part[last..]);
            result.push_str(&trailing_part);
        }
    }
    result
}

/// Knuth-Plass-style line-breaking penalty configuration for justified paragraph typesetting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineBreakPenaltyConfig {
    /// Penalty for breaking a line on a hyphen.
    pub hyphen_penalty: u32,
    /// Penalty for breaking two consecutive lines on a hyphen.
    pub consecutive_hyphen_penalty: u32,
    /// Penalty for creating a single-line paragraph widow or orphan.
    pub widow_orphan_penalty: u32,
}

impl Default for LineBreakPenaltyConfig {
    fn default() -> Self {
        Self {
            hyphen_penalty: 50,
            consecutive_hyphen_penalty: 120,
            widow_orphan_penalty: 150,
        }
    }
}

/// Calculates line break aesthetic penalty for paragraph justification optimization.
pub fn calculate_line_break_penalty(
    is_hyphenated: bool,
    previous_was_hyphenated: bool,
    is_widow_or_orphan: bool,
    config: &LineBreakPenaltyConfig,
) -> u32 {
    let mut penalty = 0;
    if is_hyphenated {
        penalty += config.hyphen_penalty;
        if previous_was_hyphenated {
            penalty += config.consecutive_hyphen_penalty;
        }
    }
    if is_widow_or_orphan {
        penalty += config.widow_orphan_penalty;
    }
    penalty
}

/// Numbering formats for footnote and endnote reference markers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FootnoteNumberingStyle {
    /// Standard Arabic numerals: 1, 2, 3...
    Numeric,
    /// Lowercase Roman numerals: i, ii, iii, iv...
    RomanLower,
    /// Lowercase alphabetical letters: a, b, c...
    AlphabeticalLower,
    /// Traditional academic symbol sequence: *, †, ‡, §...
    Symbols,
}

/// Generates the citation reference marker string for a given footnote index (1-based).
pub fn calculate_footnote_marker(index: usize, style: FootnoteNumberingStyle) -> String {
    let idx = index.max(1);
    match style {
        FootnoteNumberingStyle::Numeric => idx.to_string(),
        FootnoteNumberingStyle::RomanLower => {
            let roman_numerals = [(10, "x"), (9, "ix"), (5, "v"), (4, "iv"), (1, "i")];
            let mut num = idx;
            let mut res = String::new();
            for &(val, sym) in &roman_numerals {
                while num >= val {
                    res.push_str(sym);
                    num -= val;
                }
            }
            if res.is_empty() {
                "i".into()
            } else {
                res
            }
        }
        FootnoteNumberingStyle::AlphabeticalLower => {
            let char_code = ((idx - 1) % 26) as u8 + b'a';
            (char_code as char).to_string()
        }
        FootnoteNumberingStyle::Symbols => {
            let symbols = ["*", "†", "‡", "§", "‖", "¶"];
            let sym = symbols[(idx - 1) % symbols.len()];
            let repeat = (idx - 1) / symbols.len() + 1;
            sym.repeat(repeat)
        }
    }
}

/// Advanced text search and replace matching configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SearchOptions {
    /// Case-sensitive matching if true.
    pub case_sensitive: bool,
    /// Match whole words only (delimited by word boundaries).
    pub match_whole_word: bool,
}

/// Locates all occurrence byte ranges `(start, end)` satisfying `SearchOptions`.
pub fn find_matches_with_options(
    text: &str,
    query: &str,
    options: &SearchOptions,
) -> Vec<(usize, usize)> {
    if text.is_empty() || query.is_empty() {
        return Vec::new();
    }

    let mut matches = Vec::new();
    let text_norm = if options.case_sensitive {
        text.to_string()
    } else {
        text.to_lowercase()
    };
    let query_norm = if options.case_sensitive {
        query.to_string()
    } else {
        query.to_lowercase()
    };

    let mut offset = 0;
    while let Some(found_idx) = text_norm[offset..].find(&query_norm) {
        let abs_start = offset + found_idx;
        let abs_end = abs_start + query_norm.len();

        let passes_whole_word = if options.match_whole_word {
            let is_start_boundary = abs_start == 0
                || text[..abs_start]
                    .chars()
                    .last()
                    .map(|c| !c.is_alphanumeric() && c != '_')
                    .unwrap_or(true);
            let is_end_boundary = abs_end == text.len()
                || text[abs_end..]
                    .chars()
                    .next()
                    .map(|c| !c.is_alphanumeric() && c != '_')
                    .unwrap_or(true);
            is_start_boundary && is_end_boundary
        } else {
            true
        };

        if passes_whole_word {
            matches.push((abs_start, abs_end));
        }

        // Advance past current match
        offset = abs_start + query_norm.len().max(1);
    }

    matches
}

/// Replaces all query occurrences satisfying `SearchOptions` with `replacement`.
pub fn replace_matches(
    text: &str,
    query: &str,
    replacement: &str,
    options: &SearchOptions,
) -> String {
    let matches = find_matches_with_options(text, query, options);
    if matches.is_empty() {
        return text.to_string();
    }

    let mut result = String::with_capacity(text.len());
    let mut last_idx = 0;
    for (start, end) in matches {
        result.push_str(&text[last_idx..start]);
        result.push_str(replacement);
        last_idx = end;
    }
    result.push_str(&text[last_idx..]);
    result
}

/// One block fragment assigned to a page by the reference paginator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageFragment {
    /// Source block id.
    pub block_id: u64,
    /// First UTF-8 byte in the source block.
    pub start: usize,
    /// Exclusive last UTF-8 byte.
    pub end: usize,
    /// Laid-out text.
    pub text: String,
}

/// One deterministic page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentPage {
    /// Zero-based page index.
    pub index: usize,
    /// Ordered block fragments.
    pub fragments: Vec<PageFragment>,
}

/// Anchored comment thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommentThread {
    /// Stable comment id.
    pub id: String,
    /// Author display name.
    pub author: String,
    /// Source block id.
    pub block_id: u64,
    /// UTF-8 byte range start.
    pub start: usize,
    /// UTF-8 byte range end.
    pub end: usize,
    /// Comment body.
    pub body: String,
    /// Whether the thread is resolved.
    pub resolved: bool,
}

/// Tracked text replacement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Revision {
    /// Stable revision id.
    pub id: String,
    /// Author display name.
    pub author: String,
    /// Affected block.
    pub block_id: u64,
    /// Previous text.
    pub before: String,
    /// Current text.
    pub after: String,
}

/// A simple table model used by Writer tables and mail-merge previews.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriterTable {
    /// Stable table id.
    pub id: String,
    /// Row-major cells.
    pub rows: Vec<Vec<String>>,
    /// Whether the first row is a header.
    pub header_row: bool,
}

impl WriterTable {
    /// Creates a rectangular empty table.
    pub fn new(id: impl Into<String>, rows: usize, columns: usize) -> Self {
        Self {
            id: id.into(),
            rows: vec![vec![String::new(); columns]; rows],
            header_row: rows > 0,
        }
    }

    /// Column count.
    pub fn columns(&self) -> usize {
        self.rows.iter().map(Vec::len).max().unwrap_or(0)
    }

    /// Updates one cell.
    pub fn set(&mut self, row: usize, column: usize, value: impl Into<String>) -> bool {
        let Some(row) = self.rows.get_mut(row) else {
            return false;
        };
        let Some(cell) = row.get_mut(column) else {
            return false;
        };
        *cell = value.into();
        true
    }

    /// Exports a Markdown table.
    pub fn to_markdown(&self) -> String {
        let columns = self.columns();
        if self.rows.is_empty() || columns == 0 {
            return String::new();
        }
        let mut output = String::new();
        for (row_index, row) in self.rows.iter().enumerate() {
            output.push('|');
            for column in 0..columns {
                output.push(' ');
                output.push_str(
                    &row.get(column)
                        .map(String::as_str)
                        .unwrap_or("")
                        .replace('|', "\\|"),
                );
                output.push_str(" |");
            }
            output.push('\n');
            if row_index == 0 {
                output.push('|');
                for _ in 0..columns {
                    output.push_str(" --- |");
                }
                output.push('\n');
            }
        }
        output
    }
}

/// Higher-level authoring state that remains separate from the stable `.loomdoc`
/// block payload until the extended package schema is finalized.
#[derive(Debug, Clone)]
pub struct WriterWorkspace {
    /// Current document.
    pub document: WriterDocument,
    /// Comment threads.
    pub comments: Vec<CommentThread>,
    /// Pending tracked changes.
    pub revisions: Vec<Revision>,
    /// Named bookmarks to block ids.
    pub bookmarks: std::collections::BTreeMap<String, u64>,
    /// Embedded table models keyed by id.
    pub tables: std::collections::BTreeMap<String, WriterTable>,
}

impl WriterWorkspace {
    /// Creates an authoring workspace.
    pub fn new(document: WriterDocument) -> Self {
        Self {
            document,
            comments: Vec::new(),
            revisions: Vec::new(),
            bookmarks: std::collections::BTreeMap::new(),
            tables: std::collections::BTreeMap::new(),
        }
    }

    /// Adds a validated anchored comment.
    pub fn add_comment(&mut self, comment: CommentThread) -> Result<(), String> {
        if self
            .comments
            .iter()
            .any(|existing| existing.id == comment.id)
        {
            return Err(format!("duplicate comment id {}", comment.id));
        }
        let block = self
            .document
            .get(comment.block_id)
            .ok_or_else(|| format!("unknown block {}", comment.block_id))?;
        if comment.start > comment.end
            || comment.end > block.text.len_bytes()
            || !block.text.as_str().is_char_boundary(comment.start)
            || !block.text.as_str().is_char_boundary(comment.end)
        {
            return Err("comment range is not a valid UTF-8 block range".into());
        }
        self.comments.push(comment);
        Ok(())
    }

    /// Resolves or reopens a comment.
    pub fn set_comment_resolved(&mut self, id: &str, resolved: bool) -> bool {
        let Some(comment) = self.comments.iter_mut().find(|comment| comment.id == id) else {
            return false;
        };
        comment.resolved = resolved;
        true
    }

    /// Applies a text edit and records it as a pending revision.
    pub fn revise_block(
        &mut self,
        revision_id: impl Into<String>,
        author: impl Into<String>,
        block_id: u64,
        next_text: &str,
    ) -> Result<(), String> {
        let revision_id = revision_id.into();
        if self
            .revisions
            .iter()
            .any(|revision| revision.id == revision_id)
        {
            return Err(format!("duplicate revision id {revision_id}"));
        }
        let block = self
            .document
            .blocks
            .iter_mut()
            .find(|block| block.id == block_id)
            .ok_or_else(|| format!("unknown block {block_id}"))?;
        let before = block.text.as_str().to_string();
        if before == next_text {
            return Ok(());
        }
        block.runs = remap_style_runs(&before, next_text, &block.runs);
        block.text = Text::from_str(next_text);
        self.revisions.push(Revision {
            id: revision_id,
            author: author.into(),
            block_id,
            before,
            after: next_text.to_string(),
        });
        Ok(())
    }

    /// Accepts a revision, retaining the edited text.
    pub fn accept_revision(&mut self, id: &str) -> bool {
        let before = self.revisions.len();
        self.revisions.retain(|revision| revision.id != id);
        before != self.revisions.len()
    }

    /// Rejects a revision and restores its previous text.
    pub fn reject_revision(&mut self, id: &str) -> bool {
        let Some(index) = self.revisions.iter().position(|revision| revision.id == id) else {
            return false;
        };
        let revision = self.revisions.remove(index);
        let Some(block) = self
            .document
            .blocks
            .iter_mut()
            .find(|block| block.id == revision.block_id)
        else {
            return false;
        };
        block.runs = remap_style_runs(block.text.as_str(), &revision.before, &block.runs);
        block.text = Text::from_str(&revision.before);
        true
    }

    /// Inserts a table placeholder block and stores its model.
    pub fn insert_table(&mut self, table: WriterTable) -> Result<u64, String> {
        if table.id.trim().is_empty() || self.tables.contains_key(&table.id) {
            return Err("table id must be non-empty and unique".into());
        }
        let block_id = self.document.next_id();
        self.document.push(RichBlock::new(
            block_id,
            &format!("table:{}", table.id),
            &table.to_markdown(),
        ));
        self.tables.insert(table.id.clone(), table);
        Ok(block_id)
    }

    /// Adds or updates a bookmark.
    pub fn set_bookmark(&mut self, name: &str, block_id: u64) -> Result<(), String> {
        if name.trim().is_empty() || self.document.get(block_id).is_none() {
            return Err("bookmark name or target is invalid".into());
        }
        self.bookmarks.insert(name.to_string(), block_id);
        Ok(())
    }

    /// Reports dangling anchors and duplicate ids.
    pub fn validate(&self) -> Vec<String> {
        let mut issues = Vec::new();
        let mut block_ids = std::collections::HashSet::new();
        for block in &self.document.blocks {
            if !block_ids.insert(block.id) {
                issues.push(format!("duplicate block id {}", block.id));
            }
            for run in &block.runs {
                if run.start > run.end
                    || run.end > block.text.len_bytes()
                    || !block.text.as_str().is_char_boundary(run.start)
                    || !block.text.as_str().is_char_boundary(run.end)
                {
                    issues.push(format!("block {} has an invalid style run", block.id));
                }
            }
        }
        for comment in &self.comments {
            if self.document.get(comment.block_id).is_none() {
                issues.push(format!("comment {} targets a missing block", comment.id));
            }
        }
        for (name, block_id) in &self.bookmarks {
            if self.document.get(*block_id).is_none() {
                issues.push(format!("bookmark {name} targets a missing block"));
            }
        }
        issues
    }
}

impl WriterDocument {
    /// Finds literal text in every block.
    pub fn find_all(&self, query: &str, case_sensitive: bool) -> Vec<SearchMatch> {
        if query.is_empty() {
            return Vec::new();
        }
        let needle = if case_sensitive {
            query.to_string()
        } else {
            query.to_lowercase()
        };
        let mut matches = Vec::new();
        for block in &self.blocks {
            if case_sensitive {
                matches.extend(
                    block
                        .text
                        .as_str()
                        .match_indices(query)
                        .map(|(start, value)| SearchMatch {
                            block_id: block.id,
                            start,
                            end: start + value.len(),
                        }),
                );
            } else {
                // Unicode lowercase can change byte length, so search by character
                // windows and map matches back to original UTF-8 byte offsets.
                let original = block.text.as_str();
                let boundaries: Vec<usize> = original
                    .char_indices()
                    .map(|(index, _)| index)
                    .chain(std::iter::once(original.len()))
                    .collect();
                for start_char in 0..boundaries.len().saturating_sub(1) {
                    for end_char in start_char + 1..boundaries.len() {
                        let candidate = &original[boundaries[start_char]..boundaries[end_char]];
                        let lowered = candidate.to_lowercase();
                        if lowered == needle {
                            matches.push(SearchMatch {
                                block_id: block.id,
                                start: boundaries[start_char],
                                end: boundaries[end_char],
                            });
                            break;
                        }
                        if lowered.len() > needle.len().saturating_mul(4).max(needle.len() + 8) {
                            break;
                        }
                    }
                }
            }
        }
        matches
    }

    /// Counts total occurrences of a query string across all blocks.
    pub fn count_matches(&self, query: &str, case_sensitive: bool) -> usize {
        self.find_all(query, case_sensitive).len()
    }

    /// Replaces literal text in every block while remapping style runs.
    pub fn replace_all(&mut self, query: &str, replacement: &str, case_sensitive: bool) -> usize {
        if query.is_empty() {
            return 0;
        }
        let mut replacements = 0;
        for block in &mut self.blocks {
            let before = block.text.as_str().to_string();
            let after = if case_sensitive {
                replacements += before.matches(query).count();
                before.replace(query, replacement)
            } else {
                let hits: Vec<SearchMatch> = WriterDocument {
                    id: String::new(),
                    title: String::new(),
                    blocks: vec![block.clone()],
                }
                .find_all(query, false);
                if hits.is_empty() {
                    continue;
                }
                replacements += hits.len();
                let mut output = before.clone();
                for hit in hits.iter().rev() {
                    output.replace_range(hit.start..hit.end, replacement);
                }
                output
            };
            if after != before {
                block.runs = remap_style_runs(&before, &after, &block.runs);
                block.text = Text::from_str(&after);
            }
        }
        replacements
    }

    /// Generates a table of contents from heading blocks.
    pub fn table_of_contents(&self) -> Vec<TableOfContentsEntry> {
        self.blocks
            .iter()
            .filter_map(|block| {
                let level = block.kind.strip_prefix("heading")?.parse::<u8>().ok()?;
                if !(1..=6).contains(&level) {
                    return None;
                }
                Some(TableOfContentsEntry {
                    block_id: block.id,
                    level,
                    title: block.text.as_str().to_string(),
                })
            })
            .collect()
    }

    /// Deterministically paginates text using conservative font metrics.
    ///
    /// This is a reference CPU layout used for previews and tests. The future
    /// shaping engine can replace it while preserving this page-fragment API.
    pub fn paginate(&self, style: &PageStyle) -> Result<Vec<DocumentPage>, String> {
        if !style.width_pt.is_finite()
            || !style.height_pt.is_finite()
            || !style.body_font_size_pt.is_finite()
            || !style.line_height.is_finite()
            || style.width_pt <= style.margin_left_pt + style.margin_right_pt
            || style.height_pt <= style.margin_top_pt + style.margin_bottom_pt
            || style.body_font_size_pt <= 0.0
            || style.line_height <= 0.0
        {
            return Err("page style has invalid geometry".into());
        }
        let usable_width = style.width_pt - style.margin_left_pt - style.margin_right_pt;
        let usable_height = style.height_pt - style.margin_top_pt - style.margin_bottom_pt;
        let average_glyph_width = style.body_font_size_pt * 0.52;
        let columns = (usable_width / average_glyph_width).floor().max(1.0) as usize;
        let line_height = style.body_font_size_pt * style.line_height;
        let lines_per_page = (usable_height / line_height).floor().max(1.0) as usize;
        let mut pages = vec![DocumentPage {
            index: 0,
            fragments: Vec::new(),
        }];
        let mut lines_used = 0usize;
        for block in &self.blocks {
            let text = block.text.as_str();
            let ranges = wrap_utf8_ranges(text, columns);
            for (start, end) in ranges {
                if lines_used == lines_per_page {
                    pages.push(DocumentPage {
                        index: pages.len(),
                        fragments: Vec::new(),
                    });
                    lines_used = 0;
                }
                pages
                    .last_mut()
                    .expect("at least one page")
                    .fragments
                    .push(PageFragment {
                        block_id: block.id,
                        start,
                        end,
                        text: text[start..end].to_string(),
                    });
                lines_used += 1;
            }
            // Paragraph spacing consumes one reference line unless already at a page break.
            if lines_used > 0 && lines_used < lines_per_page {
                lines_used += 1;
            }
        }
        Ok(pages)
    }
}

fn wrap_utf8_ranges(text: &str, columns: usize) -> Vec<(usize, usize)> {
    if text.is_empty() {
        return vec![(0, 0)];
    }
    let mut ranges = Vec::new();
    let mut line_start = 0usize;
    let mut last_break = None;
    let mut chars = 0usize;
    for (index, character) in text.char_indices() {
        chars += 1;
        if character.is_whitespace() {
            last_break = Some(index + character.len_utf8());
        }
        if chars >= columns {
            let end = last_break
                .filter(|break_at| *break_at > line_start)
                .unwrap_or(index + character.len_utf8());
            ranges.push((line_start, end));
            line_start = end;
            chars = text[line_start..index + character.len_utf8()]
                .chars()
                .count();
            last_break = None;
        }
    }
    if line_start < text.len() {
        ranges.push((line_start, text.len()));
    }
    ranges
}

/// A mail merge template containing `{{field}}` merge placeholders.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MergeTemplate {
    /// Template body text with embedded `{{field}}` placeholders.
    pub body_template: String,
}

impl MergeTemplate {
    /// Create a new merge template from body text.
    pub fn new(body_template: &str) -> Self {
        Self {
            body_template: body_template.to_string(),
        }
    }

    /// Extracts unique placeholder field names in order of first appearance.
    pub fn extract_fields(&self) -> Vec<String> {
        let mut fields = Vec::new();
        let mut rest = self.body_template.as_str();
        while let Some(open) = rest.find("{{") {
            let inner = &rest[open + "{{".len()..];
            let Some(close) = inner.find("}}") else {
                break;
            };
            let name = &inner[..close];
            if !name.is_empty() && !fields.iter().any(|existing| existing == name) {
                fields.push(name.to_string());
            }
            rest = &inner[close + "}}".len()..];
        }
        fields
    }

    /// Renders one merged document by substituting `{{field}}` placeholders with
    /// record values. Unknown placeholders are left intact.
    pub fn render_record(&self, fields: &std::collections::BTreeMap<String, String>) -> String {
        let mut out = String::with_capacity(self.body_template.len());
        let mut rest = self.body_template.as_str();
        while let Some(open) = rest.find("{{") {
            out.push_str(&rest[..open]);
            let inner = &rest[open + "{{".len()..];
            let Some(close) = inner.find("}}") else {
                out.push_str(rest[open..].as_ref());
                return out;
            };
            let name = &inner[..close];
            match fields.get(name) {
                Some(value) => out.push_str(value),
                None => out.push_str(&rest[open..open + "{{".len() + close + "}}".len()]),
            }
            rest = &inner[close + "}}".len()..];
        }
        out.push_str(rest);
        out
    }
}

/// One heading entry in the extracted document outline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineEntry {
    /// Heading level 1..=6.
    pub level: u8,
    /// Heading text content.
    pub title: String,
    /// Index of the block in the document's block list.
    pub block_index: usize,
}

impl OutlineEntry {
    /// Renders the outline entry indented two spaces per level below 1.
    pub fn render_indented(&self) -> String {
        let indent = "  ".repeat(self.level.saturating_sub(1) as usize);
        format!("{indent}{}", self.title)
    }
}

/// Walks an outline slice producing hierarchical dotted numbering ("1.", "1.1.", "1.1.1.")
/// followed by each entry's title. A level jump deeper than +1 nests as a single +1 step;
/// counters at deeper levels reset whenever an outer level increments. Level 0 entries are
/// skipped. Returns one "<prefix> <title>" string per entry, in order.
pub fn numbered_outline(entries: &[OutlineEntry]) -> Vec<String> {
    // Active ancestor chain: (effective level, occurrence count at that depth).
    let mut active: Vec<(u8, u32)> = Vec::new();
    let mut rendered = Vec::with_capacity(entries.len());

    for entry in entries {
        if entry.level == 0 {
            continue;
        }
        let level = entry.level;

        // Close out branches at the same level or deeper.
        while let Some(last) = active.last() {
            if last.0 > level {
                active.pop();
            } else {
                break;
            }
        }

        match active.last() {
            // Sibling of an existing entry at this level.
            Some(last) if last.0 == level => {
                let count = last.1 + 1;
                *active.last_mut().expect("last checked above") = (level, count);
            }
            // Child entry; deep jumps clamp to one level past the parent.
            parent => {
                let effective = match parent {
                    Some((parent_level, _)) => level.min(parent_level + 1),
                    None => level,
                };
                active.push((effective, 1));
            }
        }

        let prefix = active
            .iter()
            .map(|(_, count)| count.to_string())
            .collect::<Vec<_>>()
            .join(".");
        rendered.push(format!("{prefix}. {}", entry.title));
    }

    rendered
}

/// Extracts a navigable outline from document blocks, keeping only heading
/// blocks (level derived from the block kind). Skips empty titles.
pub fn extract_outline(blocks: &[RichBlock]) -> Vec<OutlineEntry> {
    blocks
        .iter()
        .enumerate()
        .filter_map(|(block_index, block)| {
            let level = block.kind.strip_prefix("heading")?.parse::<u8>().ok()?;
            if !(1..=6).contains(&level) {
                return None;
            }
            let trimmed = block.text.as_str().trim();
            if trimmed.is_empty() {
                return None;
            }
            Some(OutlineEntry {
                level,
                title: trimmed.to_string(),
                block_index,
            })
        })
        .collect()
}

/// Supported bibliography formatting styles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CitationStyle {
    /// American Psychological Association author-date form.
    #[default]
    Apa,
    /// Modern Language Association form.
    Mla,
    /// Chicago author-date-style simplified form.
    Chicago,
    /// IEEE numeric bracketed form.
    Ieee,
}

/// A bibliographic source entry.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CitationEntry {
    /// Author display name, e.g. "Ada Lovelace".
    pub author: String,
    /// Work title.
    pub title: String,
    /// Publication year.
    pub year: u32,
    /// Publisher or journal name; empty when none.
    pub publisher: String,
}

impl CitationEntry {
    /// Formats the entry per style using these exact deterministic forms
    /// (when `publisher` is non-empty):
    ///
    /// - Apa: `{author} ({year}). {title}. {publisher}.`
    /// - Mla: `{author}. "{title}." {publisher}, {year}.`
    /// - Chicago: `{author}. "{title}." {publisher}, {year}.`
    /// - Ieee: `[{n}] {author}, "{title}," {publisher}, {year}.`
    ///
    /// An empty `publisher` omits its whole segment (including the
    /// surrounding separator) so output never contains doubled spaces or a
    /// trailing space:
    ///
    /// - Apa: `{author} ({year}). {title}.`
    /// - Mla/Chicago: `{author}. "{title}." {year}.`
    /// - Ieee: `[{n}] {author}, "{title}," {year}.`
    pub fn format(&self, style: CitationStyle, number: usize) -> String {
        let body = match style {
            CitationStyle::Apa => {
                format!("{} ({}). {}.", self.author, self.year, self.title)
            }
            CitationStyle::Mla | CitationStyle::Chicago => {
                format!("{}. \"{}.\"", self.author, self.title)
            }
            CitationStyle::Ieee => {
                format!("[{}] {}, \"{},\"", number, self.author, self.title)
            }
        };
        let tail = match style {
            CitationStyle::Apa => format!("{}.", self.publisher),
            CitationStyle::Mla | CitationStyle::Chicago | CitationStyle::Ieee => {
                format!("{}, {}.", self.publisher, self.year)
            }
        };
        if self.publisher.is_empty() && style == CitationStyle::Ieee {
            // IEEE places the year directly after the quoted title when there
            // is no venue, instead of after an omitted publisher segment.
            return format!("{} {}.", body, self.year);
        }
        if self.publisher.is_empty() {
            return match style {
                CitationStyle::Apa => body,
                CitationStyle::Mla | CitationStyle::Chicago => {
                    format!("{} {}.", body, self.year)
                }
                CitationStyle::Ieee => unreachable!("handled above"),
            };
        }
        format!("{body} {tail}")
    }

    /// Derives the "Surname, Initial." reference form from a display name.
    /// The last whitespace-separated word is treated as the surname and the
    /// first word contributes the initial; a single-word author passes
    /// through unchanged.
    pub fn surname_initial(&self) -> String {
        let mut parts = self.author.split_whitespace();
        let Some(first) = parts.next() else {
            return String::new();
        };
        let words: Vec<&str> = std::iter::once(first).chain(parts).collect();
        if words.len() == 1 {
            return self.author.clone();
        }
        let surname = words[words.len() - 1];
        let initial = first.chars().next().unwrap_or_default();
        format!("{surname}, {initial}.")
    }
}

/// Approximate English syllable count for a word using standard heuristic rules.
///
/// Exact deterministic procedure (input is defensively lowercased with
/// `to_ascii_lowercase`, so lowercase input is expected but other case also
/// behaves sanely):
///
/// 1. Vowels are `a`, `e`, `i`, `o`, `u`, `y`. Every non-alphabetic character
///    acts as a separator that breaks vowel runs.
/// 2. Each maximal run of vowels counts as one syllable.
/// 3. Silent trailing `e`: when the word has more than two characters, ends in
///    `e`, and the second-to-last character is not a vowel, one syllable is
///    subtracted (saturating at zero).
/// 4. Consonant + `le` ending: when the word has at least three characters,
///    ends in `le`, and the character before `le` is not a vowel, one syllable
///    is added back (that `e` is pronounced, as in "table").
/// 5. An empty word yields `0`; any non-empty word yields at least `1`.
pub fn estimate_syllables(word: &str) -> usize {
    let lowered = word.to_ascii_lowercase();
    let is_vowel = |c: char| matches!(c, 'a' | 'e' | 'i' | 'o' | 'u' | 'y');

    let mut groups = 0usize;
    let mut prev_vowel = false;
    for c in lowered.chars() {
        if is_vowel(c) && !prev_vowel {
            groups += 1;
        }
        prev_vowel = is_vowel(c);
    }

    let chars: Vec<char> = lowered.chars().collect();
    let mut syllables = groups;

    if chars.len() > 2 && chars[chars.len() - 1] == 'e' && !is_vowel(chars[chars.len() - 2]) {
        syllables = syllables.saturating_sub(1);
    }

    if chars.len() >= 3
        && chars[chars.len() - 1] == 'e'
        && chars[chars.len() - 2] == 'l'
        && !is_vowel(chars[chars.len() - 3])
    {
        syllables += 1;
    }

    if chars.is_empty() {
        0
    } else {
        syllables.max(1)
    }
}

/// Computes readability scores over plain text using the Flesch formulas.
///
/// Returns `(flesch_reading_ease, fkgl_grade_level)` where:
///
/// - `words` are whitespace-separated tokens containing at least one
///   alphanumeric character;
/// - `sentences` are the segments produced by splitting on the sentence
///   terminators `.`, `!`, `?` that contain at least one such word
///   (unterminated trailing text forms a final segment);
/// - `syllables` is `estimate_syllables` summed over all words.
///
/// `flesch_reading_ease = 206.835 - 1.015*(words/sentences) -
/// 84.6*(syllables/words)` and `fkgl_grade_level = 0.39*(words/sentences) +
/// 11.8*(syllables/words)`.
///
/// Higher reading-ease means easier text; higher grade level means harder
/// text. Abbreviations like "e.g." are tokenized naively by this heuristic
/// and inflate the counts. Empty or degenerate input (no sentence segment
/// containing a word) returns `Err("no sentences")`.
pub fn readability_scores(text: &str) -> Result<(f64, f64), String> {
    let is_word = |w: &str| w.chars().any(|c| c.is_alphanumeric());
    let words: Vec<&str> = text.split_whitespace().filter(|w| is_word(w)).collect();
    let sentences = text
        .split(['.', '!', '?'])
        .filter(|segment| segment.split_whitespace().any(is_word))
        .count();

    if sentences == 0 {
        return Err("no sentences".to_string());
    }

    let syllables: usize = words.iter().map(|w| estimate_syllables(w)).sum();
    let words_f = words.len() as f64;
    let sentences_f = sentences as f64;
    let syllables_f = syllables as f64;

    let words_per_sentence = words_f / sentences_f;
    let syllables_per_word = syllables_f / words_f;

    let flesch = 206.835 - 1.015 * words_per_sentence - 84.6 * syllables_per_word;
    let fkgl = 0.39 * words_per_sentence + 11.8 * syllables_per_word;

    Ok((flesch, fkgl))
}

/// One word-level difference operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WordDiffOp {
    /// Unchanged run of words.
    Equal(Vec<String>),
    /// Words removed from the old document.
    Deleted(Vec<String>),
    /// Words added in the new document.
    Inserted(Vec<String>),
}

/// Computes a word-level LCS diff between two texts.
///
/// Tokenization splits on whitespace; punctuation attaches to tokens. The
/// classic dynamic-programming longest common subsequence is computed over
/// the word vectors (`O(n*m)` time and space, fine for documents) and walked
/// forward to emit grouped operations in document order. Deterministic: on
/// ties prefer deletion before insertion and earlier positions.
///
/// Empty inputs behave sensibly: two empty texts produce an empty diff, and
/// one empty text produces a single `Deleted` or `Inserted` operation
/// covering every word of the other.
pub fn word_diff(old_text: &str, new_text: &str) -> Vec<WordDiffOp> {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Kind {
        Equal,
        Deleted,
        Inserted,
    }

    fn push_word(runs: &mut Vec<(Kind, Vec<String>)>, kind: Kind, word: &str) {
        match runs.last_mut() {
            Some((run_kind, words)) if *run_kind == kind => words.push(word.to_string()),
            _ => runs.push((kind, vec![word.to_string()])),
        }
    }

    let old: Vec<&str> = old_text.split_whitespace().collect();
    let new: Vec<&str> = new_text.split_whitespace().collect();
    let (n, m) = (old.len(), new.len());

    // `table[i][j]` is the LCS length of `old[i..]` and `new[j..]`.
    let mut table = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            table[i][j] = if old[i] == new[j] {
                table[i + 1][j + 1] + 1
            } else {
                table[i + 1][j].max(table[i][j + 1])
            };
        }
    }

    let mut runs: Vec<(Kind, Vec<String>)> = Vec::new();
    let (mut i, mut j) = (0usize, 0usize);
    while i < n && j < m {
        if old[i] == new[j] {
            push_word(&mut runs, Kind::Equal, old[i]);
            i += 1;
            j += 1;
        } else if table[i + 1][j] >= table[i][j + 1] {
            // Ties prefer deletion before insertion.
            push_word(&mut runs, Kind::Deleted, old[i]);
            i += 1;
        } else {
            push_word(&mut runs, Kind::Inserted, new[j]);
            j += 1;
        }
    }
    // Whichever side still has words left forms a single trailing run.
    for word in old[i..].iter().copied() {
        push_word(&mut runs, Kind::Deleted, word);
    }
    for word in new[j..].iter().copied() {
        push_word(&mut runs, Kind::Inserted, word);
    }

    runs.into_iter()
        .map(|(kind, words)| match kind {
            Kind::Equal => WordDiffOp::Equal(words),
            Kind::Deleted => WordDiffOp::Deleted(words),
            Kind::Inserted => WordDiffOp::Inserted(words),
        })
        .collect()
}

/// Imports plain text as document blocks: paragraphs are separated by one or more blank
/// lines; single newlines within a group are treated as soft line breaks inside one
/// paragraph. Leading and trailing blank lines are ignored. Returns Err when the input has
/// no paragraph content.
pub fn import_text_paragraphs(text: &str) -> Result<Vec<String>, String> {
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut paragraphs = Vec::new();
    let mut current = String::new();
    for line in normalized.split('\n') {
        if line.trim().is_empty() && current.trim().is_empty() {
            continue;
        }
        if line.trim().is_empty() {
            paragraphs.push(current.trim_end().to_string());
            current.clear();
        } else {
            if !current.is_empty() {
                current.push('\n');
            }
            current.push_str(line);
        }
    }
    if !current.trim().is_empty() {
        paragraphs.push(current);
    }
    if paragraphs.is_empty() {
        return Err("no paragraph content found".into());
    }
    Ok(paragraphs)
}

/// Provenance of tabular data pasted into a Writer document.
///
/// When a Sheets range is pasted into Writer (Stage E cross-application
/// workflow 5), the document records where the data came from so the paste can
/// remain traceable and, optionally, refreshable. The pasted cell texts are
/// snapshotted here rather than referenced live: the document stays
/// local-first and renders identically without the source workbook present.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkedTableRegion {
    /// Stable identifier for this region within the document.
    pub region_id: String,
    /// Source workbook path on disk.
    pub source_workbook: String,
    /// Source sheet name within the workbook.
    pub source_sheet: String,
    /// A1-style source range, e.g. "A1:C9".
    pub source_range: String,
    /// Snapshot of the pasted cell texts (row-major).
    pub snapshot_rows: Vec<Vec<String>>,
    /// Whether the host may refresh this region from the live workbook.
    pub refreshable: bool,
}

impl LinkedTableRegion {
    /// Validates the region's invariants.
    ///
    /// Rules, checked in order; the error names the violated rule:
    ///
    /// - `region_id`, `source_sheet`, and `source_range` are non-empty;
    /// - `source_range` contains exactly one `':'` and both halves are
    ///   non-empty (e.g. "A1:C9");
    /// - every snapshot row has the same number of cells.
    pub fn validate(&self) -> Result<(), String> {
        if self.region_id.is_empty() {
            return Err("region id must not be empty".to_string());
        }
        if self.source_sheet.is_empty() {
            return Err("source sheet must not be empty".to_string());
        }
        if self.source_range.is_empty() {
            return Err("source range must not be empty".to_string());
        }
        let colons = self.source_range.matches(':').count();
        if colons != 1 {
            return Err(format!(
                "source range must contain exactly one ':', found {}",
                colons
            ));
        }
        let mut halves = self.source_range.split(':');
        let start = halves.next().unwrap_or_default();
        let end = halves.next().unwrap_or_default();
        if start.is_empty() || end.is_empty() {
            return Err("both ends of the source range must be non-empty".to_string());
        }
        if let Some(width) = self.snapshot_rows.first().map(Vec::len) {
            for (i, row) in self.snapshot_rows.iter().enumerate() {
                if row.len() != width {
                    return Err(format!(
                        "snapshot row {} has {} cells but the table width is {}",
                        i,
                        row.len(),
                        width
                    ));
                }
            }
        }
        Ok(())
    }

    /// Renders the snapshot as plain text: cells within a row joined by
    /// `" | "` and rows joined by `"\n"` (the documented paste format).
    pub fn render_plain(&self) -> String {
        self.snapshot_rows
            .iter()
            .map(|row| row.join(" | "))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Replaces the snapshot with `rows`.
    ///
    /// Every new row must have the same number of cells as the existing
    /// table width (the length of the first existing row); otherwise the
    /// snapshot is left unchanged and `Err` names the mismatch. When the
    /// current snapshot is empty, the first new row defines the new width
    /// and the remaining rows must match it.
    pub fn update_snapshot(&mut self, rows: Vec<Vec<String>>) -> Result<(), String> {
        let expected = match self.snapshot_rows.first() {
            Some(first) => Some(first.len()),
            None => rows.first().map(Vec::len),
        };
        if let Some(expected) = expected {
            for (i, row) in rows.iter().enumerate() {
                if row.len() != expected {
                    return Err(format!(
                        "new snapshot row {} has {} cells but the table width is {}",
                        i,
                        row.len(),
                        expected
                    ));
                }
            }
        }
        self.snapshot_rows = rows;
        Ok(())
    }
}

/// One OCR-derived text block with source-region provenance.
#[derive(Debug, Clone, PartialEq)]
pub struct OcrTextBlock {
    pub text: String,
    /// Source region in pixels of the scanned page.
    pub region: (u32, u32, u32, u32), // x, y, w, h
    pub confidence: f32,
}

/// Converts OCR blocks into editable paragraphs with provenance: blocks are ordered
/// top-to-bottom by region y (then x); consecutive blocks whose vertical gap is less than
/// 1.5x the smaller block height merge into one paragraph joined by a space. Whitespace is
/// collapsed. Returns Err when any block has empty text or confidence outside [0,1].
pub fn paragraphs_from_ocr_blocks(
    blocks: &[OcrTextBlock],
) -> Result<Vec<(String, Vec<usize>)>, String> {
    for (i, block) in blocks.iter().enumerate() {
        if block.text.trim().is_empty() {
            return Err(format!("OCR block {i} has empty text"));
        }
        if !(0.0..=1.0).contains(&block.confidence) {
            return Err(format!(
                "OCR block {i} has confidence {} outside [0,1]",
                block.confidence
            ));
        }
    }
    let mut order: Vec<usize> = (0..blocks.len()).collect();
    order.sort_by(|&a, &b| {
        blocks[a]
            .region
            .1
            .cmp(&blocks[b].region.1)
            .then_with(|| blocks[a].region.0.cmp(&blocks[b].region.0))
    });
    let mut paragraphs: Vec<(String, Vec<usize>)> = Vec::new();
    for &idx in &order {
        let block = &blocks[idx];
        let (_, y, _, h) = block.region;
        let prev = paragraphs
            .last()
            .and_then(|(_, group)| group.last())
            .map(|&prev_idx| &blocks[prev_idx]);
        let merges = match prev {
            Some(prev) => {
                let gap = i64::from(y) - i64::from(prev.region.1) - i64::from(prev.region.3);
                2 * gap < 3 * i64::from(h.min(prev.region.3))
            }
            None => false,
        };
        if merges {
            let paragraph = paragraphs.last_mut().expect("merge requires a paragraph");
            let collapsed = block.text.split_whitespace().collect::<Vec<_>>().join(" ");
            paragraph.0.push(' ');
            paragraph.0.push_str(&collapsed);
            paragraph.1.push(idx);
        } else {
            paragraphs.push((
                block.text.split_whitespace().collect::<Vec<_>>().join(" "),
                vec![idx],
            ));
        }
    }
    Ok(paragraphs)
}

/// FNV-1a 64-bit hash over bytes.
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Stable integrity digest of a document's user-visible content: hashes each block's kind
/// and text content in order, prefixed by the block count. Two documents that would render
/// identically produce equal digests. Uses [`fnv1a64`].
pub fn writer_document_digest(blocks: &[RichBlock]) -> u64 {
    let mut input = format!("blocks:{}\n", blocks.len());
    for block in blocks {
        input.push_str(&format!("{}:{}\n", block.kind, block.text.as_str()));
    }
    fnv1a64(input.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writer_document_integrity_digest() {
        let make_blocks = || {
            vec![
                RichBlock::new(1, "heading1", "Report"),
                RichBlock::new(2, "paragraph", "Hello world"),
                RichBlock::new(3, "paragraph", "Second paragraph"),
            ]
        };
        let blocks = make_blocks();
        let digest = writer_document_digest(&blocks);
        assert_eq!(digest, writer_document_digest(&make_blocks()));

        let mut changed_text = make_blocks();
        changed_text[1].text = Text::from_str("Hello brave world");
        assert_ne!(
            digest,
            writer_document_digest(&changed_text),
            "changing a block's text must change the digest"
        );

        let mut reordered = make_blocks();
        reordered.reverse();
        assert_ne!(
            digest,
            writer_document_digest(&reordered),
            "reordering blocks must change the digest"
        );

        assert_eq!(
            writer_document_digest(&[]),
            fnv1a64(b"blocks:0\n"),
            "empty document digest must equal the digest of the bare prefix"
        );
    }

    #[test]
    fn ocr_blocks_to_editable_paragraphs() {
        let blocks = vec![
            OcrTextBlock {
                text: "Second paragraph".to_string(),
                region: (0, 200, 300, 20),
                confidence: 0.7,
            },
            OcrTextBlock {
                text: "Hello   world ".to_string(),
                region: (0, 10, 300, 20),
                confidence: 0.9,
            },
            OcrTextBlock {
                text: "\tmore  text".to_string(),
                region: (4, 35, 290, 20),
                confidence: 0.8,
            },
        ];
        let paragraphs = paragraphs_from_ocr_blocks(&blocks).unwrap();
        assert_eq!(paragraphs.len(), 2);
        assert_eq!(
            paragraphs[0],
            ("Hello world more text".to_string(), vec![1, 2])
        );
        assert_eq!(paragraphs[1], ("Second paragraph".to_string(), vec![0]));

        let empty = vec![OcrTextBlock {
            text: "   ".to_string(),
            region: (0, 0, 10, 10),
            confidence: 0.5,
        }];
        assert!(paragraphs_from_ocr_blocks(&empty)
            .unwrap_err()
            .contains("empty text"));

        let overconfident = vec![OcrTextBlock {
            text: "ok".to_string(),
            region: (0, 0, 10, 10),
            confidence: 1.5,
        }];
        assert!(paragraphs_from_ocr_blocks(&overconfident)
            .unwrap_err()
            .contains("[0,1]"));
    }

    fn demo_doc() -> WriterDocument {
        let mut d = WriterDocument::new("doc-1", "My Report");
        let h = RichBlock::new(d.next_id(), "heading1", "My Report");
        d.push(h);
        let p = RichBlock::new(
            d.next_id(),
            "paragraph",
            "This is an original Loom document.",
        );
        d.push(p);
        d
    }

    #[test]
    fn plain_text_and_markdown() {
        let d = demo_doc();
        assert_eq!(
            d.plain_text(),
            "My Report\nThis is an original Loom document."
        );
        let md = d.to_markdown();
        assert!(md.starts_with("# My Report"));
        assert!(md.contains("This is an original Loom document."));
    }

    #[test]
    fn save_load_roundtrip() {
        let d = demo_doc();
        let bytes = save_document(&d).unwrap();
        let loaded = load_document(&bytes).unwrap();
        assert_eq!(loaded.id, "doc-1");
        assert_eq!(loaded.title, "My Report");
        assert_eq!(loaded.len(), 2);
        assert_eq!(
            loaded.blocks[1].text.as_str(),
            "This is an original Loom document."
        );
    }

    #[test]
    fn corrupted_content_rejected() {
        let d = demo_doc();
        let mut bytes = save_document(&d).unwrap();
        // Flip bits in the content area.
        let idx = bytes.iter().position(|&b| b == b'T').unwrap();
        bytes[idx] ^= 0xFF;
        assert!(load_document(&bytes).is_err());
    }

    #[test]
    fn json_content_roundtrip() {
        let d = demo_doc();
        let json = d.to_content_json();
        let back = WriterDocument::from_content_json(&json).unwrap();
        assert_eq!(back, d);
    }

    #[test]
    fn json_content_roundtrip_preserves_full_styles_and_runs() {
        let mut d = WriterDocument::new("styled-doc", "Styled document");
        let mut block = RichBlock::new(7, "heading2", "Café 🌿");
        block.style = loom_text::ParagraphStyle {
            alignment: loom_text::Alignment::Justify,
            space_before: 2.5,
            space_after: 11.25,
            line_spacing: 1.4,
            left_indent: 8.0,
            right_indent: 3.5,
            first_line_indent: -2.0,
            break_rule: loom_text::LineBreakRule::Char,
        };
        block.runs = vec![
            loom_text::StyleRun {
                start: 0,
                end: "Café".len(),
                style: loom_text::CharacterStyle {
                    font_family: "Iosevka Null".into(),
                    font_size: 17.5,
                    weight: loom_text::FontWeight::Black,
                    italic: true,
                    underline: true,
                    strikethrough: true,
                    superscript: true,
                    subscript: false,
                    color: Some("null".into()),
                    background: Some("#12345678".into()),
                },
            },
            loom_text::StyleRun {
                start: "Café".len(),
                end: block.text.len_bytes(),
                style: loom_text::CharacterStyle {
                    font_family: String::new(),
                    font_size: 9.25,
                    weight: loom_text::FontWeight::Light,
                    italic: false,
                    underline: false,
                    strikethrough: false,
                    superscript: false,
                    subscript: true,
                    color: None,
                    background: None,
                },
            },
        ];
        d.push(block);

        let json = d.to_content_json();
        assert!(json.contains("\"style\""));
        assert!(json.contains("\"runs\""));
        let back = WriterDocument::from_content_json(&json).unwrap();
        assert_eq!(back, d);
    }

    #[test]
    fn legacy_alignment_only_json_still_loads_with_default_style_and_runs() {
        let json = r#"{
            "id":"legacy-doc",
            "title":"Legacy document",
            "blocks":[
                {"id":4,"kind":"heading1","text":"Legacy heading","align":"center"}
            ]
        }"#;

        let doc = WriterDocument::from_content_json(json).unwrap();
        assert_eq!(doc.id, "legacy-doc");
        assert_eq!(doc.title, "Legacy document");
        assert_eq!(doc.blocks[0].style.alignment, loom_text::Alignment::Center);
        assert_eq!(doc.blocks[0].style.space_after, 8.0);
        assert!(doc.blocks[0].runs.is_empty());
    }

    #[test]
    fn next_id_increments() {
        let d = demo_doc();
        assert_eq!(d.next_id(), 3);
        let empty = WriterDocument::new("x", "y");
        assert_eq!(empty.next_id(), 1);
    }

    #[test]
    fn replace_all_mutation() {
        let d = demo_doc();
        let mut next = d.clone();
        next.push(RichBlock::new(next.next_id(), "paragraph", "More text."));
        let m = d.replace_all_mutation(&next);
        let applied = m.apply(&Text::from_str(&d.plain_text()));
        assert!(applied.as_str().contains("More text."));
    }

    #[test]
    fn replace_paragraphs_matches_metadata_across_middle_insert_delete() {
        let mut d = WriterDocument::new("doc-1", "My Report");
        let mut title = RichBlock::new(10, "heading1", "Title");
        title.style.alignment = loom_text::Alignment::Center;
        title.runs = vec![loom_text::StyleRun {
            start: 0,
            end: title.text.len_bytes(),
            style: loom_text::CharacterStyle {
                weight: loom_text::FontWeight::Bold,
                ..Default::default()
            },
        }];
        let mut body = RichBlock::new(20, "paragraph", "Body");
        body.style.alignment = loom_text::Alignment::Right;
        body.runs = vec![loom_text::StyleRun {
            start: 0,
            end: body.text.len_bytes(),
            style: loom_text::CharacterStyle {
                italic: true,
                ..Default::default()
            },
        }];
        let mut tail = RichBlock::new(30, "heading2", "Tail");
        tail.style.alignment = loom_text::Alignment::Justify;
        d.push(title.clone());
        d.push(body.clone());
        d.push(tail.clone());

        d.replace_paragraphs("Title\nInserted\nBody\nTail");

        assert_eq!(d.blocks[0], title);
        assert_eq!(d.blocks[1].id, 31);
        assert_eq!(d.blocks[1].kind, "paragraph");
        assert!(d.blocks[1].runs.is_empty());
        assert_eq!(d.blocks[2], body);
        assert_eq!(d.blocks[3], tail);

        d.replace_paragraphs("Title\nBody\nTail");

        assert_eq!(d.len(), 3);
        assert_eq!(d.blocks[0], title);
        assert_eq!(d.blocks[1], body);
        assert_eq!(d.blocks[2], tail);
    }

    #[test]
    fn replace_paragraphs_keeps_changed_metadata_across_a_nearby_insertion() {
        let mut d = WriterDocument::new("doc-1", "My Report");
        let title = RichBlock::new(10, "heading1", "Title");
        let mut original = RichBlock::new(20, "heading2", "Original paragraph");
        original.style.alignment = loom_text::Alignment::Right;
        let tail = RichBlock::new(30, "paragraph", "Tail");
        d.push(title);
        d.push(original.clone());
        d.push(tail);

        d.replace_paragraphs("Title\nEdited paragraph\nInserted\nTail");

        assert_eq!(d.blocks[1].id, original.id);
        assert_eq!(d.blocks[1].kind, original.kind);
        assert_eq!(d.blocks[1].style, original.style);
        assert_eq!(d.blocks[1].text.as_str(), "Edited paragraph");
        assert_eq!(d.blocks[2].kind, "paragraph");
        assert_ne!(d.blocks[2].id, original.id);
    }

    #[test]
    fn replace_paragraphs_preserves_runs_for_unchanged_and_remaps_them_on_change() {
        let mut d = WriterDocument::new("doc-1", "My Report");
        let mut unchanged = RichBlock::new(10, "heading1", "Keep this");
        unchanged.style.alignment = loom_text::Alignment::Center;
        unchanged.runs = vec![loom_text::StyleRun {
            start: 0,
            end: unchanged.text.len_bytes(),
            style: loom_text::CharacterStyle {
                underline: true,
                ..Default::default()
            },
        }];
        let mut changed = RichBlock::new(20, "heading2", "Change this");
        changed.style.alignment = loom_text::Alignment::Right;
        changed.runs = vec![loom_text::StyleRun {
            start: 0,
            end: changed.text.len_bytes(),
            style: loom_text::CharacterStyle {
                strikethrough: true,
                ..Default::default()
            },
        }];
        d.push(unchanged.clone());
        d.push(changed.clone());

        d.replace_paragraphs("Keep this\nChanged text");

        assert_eq!(d.blocks[0], unchanged);
        assert_eq!(d.blocks[1].id, changed.id);
        assert_eq!(d.blocks[1].kind, changed.kind);
        assert_eq!(d.blocks[1].style, changed.style);
        assert_eq!(d.blocks[1].text.as_str(), "Changed text");
        assert_eq!(d.blocks[1].runs.len(), 1);
        assert_eq!(d.blocks[1].runs[0].start, 0);
        assert_eq!(d.blocks[1].runs[0].end, "Changed text".len());
        assert_eq!(d.blocks[1].runs[0].style, changed.runs[0].style);
    }

    #[test]
    fn replace_paragraphs_shifts_a_style_run_when_text_is_inserted_inside_it() {
        let mut d = WriterDocument::new("doc-1", "My Report");
        let mut block = RichBlock::new(10, "paragraph", "plain styled tail");
        block.runs = vec![loom_text::StyleRun {
            start: "plain ".len(),
            end: "plain styled".len(),
            style: loom_text::CharacterStyle {
                weight: loom_text::FontWeight::Bold,
                ..Default::default()
            },
        }];
        d.push(block.clone());

        d.replace_paragraphs("plain sXtyled tail");

        assert_eq!(d.blocks[0].text.as_str(), "plain sXtyled tail");
        assert_eq!(d.blocks[0].runs.len(), 1);
        assert_eq!(d.blocks[0].runs[0].start, "plain ".len());
        assert_eq!(d.blocks[0].runs[0].end, "plain sXtyled".len());
        assert_eq!(d.blocks[0].runs[0].style, block.runs[0].style);
    }

    #[test]
    fn replace_paragraphs_preserves_single_newlines_and_empty_paragraphs() {
        let mut d = WriterDocument::new("doc-1", "My Report");

        d.replace_paragraphs("\r\nfirst\r\n\r\nthird\r\n");

        assert_eq!(
            d.blocks
                .iter()
                .map(|block| block.text.as_str())
                .collect::<Vec<_>>(),
            vec!["", "first", "", "third", ""]
        );
        assert_eq!(d.editor_text(), "\nfirst\n\nthird\n");
    }

    #[test]
    fn replace_paragraphs_removes_all_blocks_for_empty_input() {
        let mut d = demo_doc();

        d.replace_paragraphs("");

        assert!(d.is_empty());
        assert_eq!(d.editor_text(), "");
    }

    #[test]
    fn search_replace_toc_and_pagination_are_functional() {
        let mut doc = demo_doc();
        doc.blocks[0].kind = "heading1".into();
        assert_eq!(doc.find_all("loom", false).len(), 1);
        assert_eq!(doc.replace_all("loom", "Loom Suite", false), 1);
        assert!(doc.plain_text().contains("Loom Suite"));
        assert_eq!(doc.table_of_contents()[0].level, 1);
        let pages = doc
            .paginate(&PageStyle {
                width_pt: 180.0,
                height_pt: 120.0,
                margin_top_pt: 10.0,
                margin_bottom_pt: 10.0,
                margin_left_pt: 10.0,
                margin_right_pt: 10.0,
                body_font_size_pt: 10.0,
                line_height: 1.0,
            })
            .unwrap();
        assert!(!pages.is_empty());
        assert!(pages.iter().all(|page| !page.fragments.is_empty()));
    }

    #[test]
    fn comments_revisions_bookmarks_and_tables_are_editable() {
        let doc = demo_doc();
        let block_id = doc.blocks[0].id;
        let mut workspace = WriterWorkspace::new(doc);
        workspace
            .add_comment(CommentThread {
                id: "comment-1".into(),
                author: "Author".into(),
                block_id,
                start: 0,
                end: 4,
                body: "Clarify".into(),
                resolved: false,
            })
            .unwrap();
        assert!(workspace.set_comment_resolved("comment-1", true));
        workspace
            .revise_block("revision-1", "Editor", block_id, "Edited title")
            .unwrap();
        assert_eq!(
            workspace.document.get(block_id).unwrap().text.as_str(),
            "Edited title"
        );
        assert!(workspace.reject_revision("revision-1"));
        workspace.set_bookmark("intro", block_id).unwrap();
        let mut table = WriterTable::new("table-1", 2, 2);
        table.set(0, 0, "Name");
        table.set(1, 0, "Loom");
        let table_block = workspace.insert_table(table).unwrap();
        assert!(workspace
            .document
            .get(table_block)
            .unwrap()
            .text
            .as_str()
            .contains("Name"));
        assert!(workspace.validate().is_empty());
    }

    #[test]
    fn split_and_merge_blocks_preserves_content_and_runs() {
        let mut doc = WriterDocument::new("doc-split", "Split Test");
        let mut block = RichBlock::new(1, "paragraph", "Hello Beautiful World");
        block.runs.push(loom_text::StyleRun {
            start: 6,
            end: 15,
            style: loom_text::CharacterStyle {
                weight: loom_text::FontWeight::Bold,
                ..Default::default()
            },
        });
        doc.push(block);

        let new_id = doc.split_block(1, 6).unwrap();
        assert_eq!(doc.len(), 2);
        assert_eq!(doc.blocks[0].text.as_str(), "Hello ");
        assert_eq!(doc.blocks[1].id, new_id);
        assert_eq!(doc.blocks[1].text.as_str(), "Beautiful World");
        assert_eq!(doc.blocks[1].runs.len(), 1);
        assert_eq!(doc.blocks[1].runs[0].start, 0);
        assert_eq!(doc.blocks[1].runs[0].end, 9);
        assert_eq!(
            doc.blocks[1].runs[0].style.weight,
            loom_text::FontWeight::Bold
        );

        doc.merge_blocks(1, new_id).unwrap();
        assert_eq!(doc.len(), 1);
        assert_eq!(doc.blocks[0].text.as_str(), "Hello Beautiful World");
        assert_eq!(doc.blocks[0].runs.len(), 1);
        assert_eq!(doc.blocks[0].runs[0].start, 6);
        assert_eq!(doc.blocks[0].runs[0].end, 15);
    }

    #[test]
    fn estimate_pagination_calculates_pages_and_metrics() {
        let mut doc = WriterDocument::new("doc-page", "Page Metric Test");
        let paragraph =
            "Loom Writer provides calm, focused, and professional typographic layout. ".repeat(40);
        doc.push(RichBlock::new(1, "paragraph", &paragraph));
        let metrics = doc.estimate_pagination();
        assert!(metrics.words > 300);
        assert!(metrics.total_pages >= 2);
        assert!(metrics.reading_time_minutes > 1.0);
    }

    #[test]
    fn block_formatting_and_alignment_operations() {
        let mut doc = WriterDocument::new("doc-fmt", "Formatting Test");
        let block = RichBlock::new(
            1,
            "paragraph",
            "The quick brown fox jumps over the lazy dog",
        );
        doc.push(block);

        // Format "brown fox" as italic
        doc.format_block_range(
            1,
            10,
            19,
            loom_text::CharacterStyle {
                italic: true,
                ..Default::default()
            },
        )
        .unwrap();

        let b = &doc.blocks[0];
        assert_eq!(b.runs.len(), 1);
        assert_eq!(b.runs[0].start, 10);
        assert_eq!(b.runs[0].end, 19);
        assert!(b.runs[0].style.italic);

        // Set alignment to Center
        doc.set_block_alignment(1, loom_text::Alignment::Center)
            .unwrap();
        assert_eq!(doc.blocks[0].style.alignment, loom_text::Alignment::Center);

        // Set block kind to heading1
        doc.set_block_kind(1, "heading1").unwrap();
        assert_eq!(doc.blocks[0].kind, "heading1");
    }

    #[test]
    fn to_html_string_generates_semantic_markup() {
        let mut doc = WriterDocument::new("doc-html", "HTML Document");
        doc.push(RichBlock::new(1, "heading1", "Document Title"));
        doc.push(RichBlock::new(2, "paragraph", "First paragraph body text."));
        doc.push(RichBlock::new(3, "quote", "A quoted passage."));

        let html = doc.to_html_string();
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("<title>HTML Document</title>"));
        assert!(html.contains("<h1>Document Title</h1>"));
        assert!(html.contains("<p>First paragraph body text.</p>"));
        assert!(html.contains("<blockquote>A quoted passage.</blockquote>"));
    }

    #[test]
    fn generate_toc_extracts_hierarchical_headings() {
        let mut doc = WriterDocument::new("doc-toc", "TOC Test");
        doc.push(RichBlock::new(1, "heading1", "Introduction"));
        doc.push(RichBlock::new(2, "paragraph", "Introductory content."));
        doc.push(RichBlock::new(3, "heading2", "Background"));
        doc.push(RichBlock::new(4, "heading3", "Prior Work"));
        doc.push(RichBlock::new(5, "heading1", "Conclusion"));

        let toc = doc.generate_toc();
        assert_eq!(toc.len(), 4);
        assert_eq!(toc[0].title, "Introduction");
        assert_eq!(toc[0].level, 1);
        assert_eq!(toc[1].title, "Background");
        assert_eq!(toc[1].level, 2);
        assert_eq!(toc[2].title, "Prior Work");
        assert_eq!(toc[2].level, 3);
        assert_eq!(toc[3].title, "Conclusion");
        assert_eq!(toc[3].level, 1);
    }

    #[test]
    fn count_matches_counts_occurrences() {
        let mut doc = WriterDocument::new("doc-search", "Search Test");
        doc.push(RichBlock::new(
            1,
            "p",
            "The quick brown fox jumps over the lazy dog.",
        ));
        doc.push(RichBlock::new(2, "p", "The dog was very lazy and slow."));

        assert_eq!(doc.count_matches("dog", true), 2);
        assert_eq!(doc.count_matches("the", true), 1);
        assert_eq!(doc.count_matches("the", false), 3);
        assert_eq!(doc.count_matches("cat", false), 0);
    }

    #[test]
    fn document_statistics_calculation() {
        let mut doc = WriterDocument::new("doc-stats", "Stats Document");
        doc.push(RichBlock::new(1, "p", "Hello world this is Loom Writer."));
        doc.push(RichBlock::new(2, "p", "Second paragraph with more words."));

        let stats = doc.statistics();
        assert_eq!(stats.block_count, 2);
        assert_eq!(stats.word_count, 11);
        assert_eq!(stats.sentence_count, 2);
        assert!(stats.char_count > 40);
        assert!(stats.char_count_no_spaces < stats.char_count);
        assert!(stats.reading_time_minutes > 0.0);
    }

    #[test]
    fn readability_scoring_and_syllables() {
        // Syllable heuristic spot checks (rules documented on estimate_syllables).
        assert_eq!(estimate_syllables("cat"), 1); // one vowel group "a"
        assert_eq!(estimate_syllables("reading"), 2); // groups "ea", "i"
                                                      // "table": groups a,e = 2; silent-e -> 1; consonant+"le" adds 1 back -> 2.
        assert_eq!(estimate_syllables("table"), 2);
        assert_eq!(estimate_syllables(""), 0);

        // Fixture A: six monosyllables, one sentence.
        // words=6, syllables=6, sentences=1
        // Flesch = 206.835 - 1.015*(6/1) - 84.6*(6/6) = 206.835 - 6.09 - 84.6 = 116.145
        // FKGL   = 0.39*(6/1) + 11.8*(6/6) = 2.34 + 11.8 = 14.14
        let (easy_flesch, easy_fkgl) = readability_scores("The cat sat on the mat.").unwrap();
        assert!((easy_flesch - 116.145).abs() < 1e-9);
        assert!((easy_fkgl - 14.14).abs() < 1e-9);

        // Fixture B: seven polysyllables, one long sentence.
        // incredible=4 (groups i,e,i,e=4; silent-e -1; consonant+"le" +1)
        // organizations=5 (o,a,i,a,io)  systematically=6 (y,e,a,i,a,y)
        // documented=4 (o,u,e,e)  extraordinary=5 (e,ao,i,a,y)
        // educational=5 (e,u,a,io,a)  examinations=5 (e,a,i,a,io)
        // words=7, syllables=34, sentences=1
        // Flesch = 206.835 - 1.015*7 - 84.6*(34/7) = 199.73 - 410.91428571... = -211.18428571428564
        // FKGL   = 0.39*7 + 11.8*(34/7) = 2.73 + 57.31428571... = 60.044285714285714
        let (hard_flesch, hard_fkgl) = readability_scores(
            "Incredible organizations systematically documented extraordinary educational examinations.",
        )
        .unwrap();
        assert!((hard_flesch - -211.18428571428564).abs() < 1e-9);
        assert!((hard_fkgl - 60.044285714285714).abs() < 1e-9);

        assert!(easy_flesch > hard_flesch);
        assert!(easy_fkgl < hard_fkgl);

        // Mixed terminators '.', '!', '?' split into three sentences.
        // words=9 monosyllables, syllables=9, sentences=3
        // Flesch = 206.835 - 1.015*(9/3) - 84.6*(9/9) = 206.835 - 3.045 - 84.6 = 119.19
        // FKGL   = 0.39*(9/3) + 11.8*(9/9) = 1.17 + 11.8 = 12.97
        let (mixed_flesch, mixed_fkgl) =
            readability_scores("The cat ran! The dog hid? The bird sang.").unwrap();
        assert!((mixed_flesch - 119.19).abs() < 1e-9);
        assert!((mixed_fkgl - 12.97).abs() < 1e-9);

        assert_eq!(readability_scores("").unwrap_err(), "no sentences");
        assert_eq!(readability_scores("?!.").unwrap_err(), "no sentences");

        // Unterminated trailing text still forms one sentence.
        assert!(readability_scores("hello world").is_ok());
    }

    #[test]
    fn find_word_boundaries_expansion() {
        let mut doc = WriterDocument::new("doc-wb", "Word Boundaries");
        doc.push(RichBlock::new(1, "p", "The quick brown fox."));

        // Position on 'u' in "quick" (char index 5)
        let bounds = doc.find_word_boundaries(0, 5).unwrap();
        assert_eq!(bounds, (4, 9)); // "quick" spans [4..9]

        // Position on space between "quick" and "brown" (char index 9)
        let space_bounds = doc.find_word_boundaries(0, 9).unwrap();
        assert_eq!(space_bounds, (9, 10)); // single space
    }

    #[test]
    fn block_spacing_formatting() {
        let mut doc = WriterDocument::new("doc-space", "Spacing Document");
        doc.push(RichBlock::new(1, "p", "First paragraph."));

        doc.set_block_spacing(1, 1.5, 12.0).unwrap();
        assert_eq!(doc.blocks[0].style.line_spacing, 1.5);
        assert_eq!(doc.blocks[0].style.space_after, 12.0);
    }

    #[test]
    fn paper_sizes_and_reading_metrics() {
        let a4 = PaperSize::A4.dimensions_pt();
        assert_eq!(a4, (595.0, 842.0));

        let letter = PaperSize::Letter.dimensions_pt();
        assert_eq!(letter, (612.0, 792.0));

        let normal_margins = PageMarginsPreset::Normal.margins_pt();
        assert_eq!(normal_margins, (72.0, 72.0, 72.0, 72.0));

        let narrow_margins = PageMarginsPreset::Narrow.margins_pt();
        assert_eq!(narrow_margins, (36.0, 36.0, 36.0, 36.0));

        let read_time = calculate_reading_time_minutes(500, 250);
        assert_eq!(read_time, 2.0);

        let speak_time = calculate_speaking_time_minutes(260, 130);
        assert_eq!(speak_time, 2.0);
    }

    #[test]
    fn header_footer_formatting_and_page_numbers() {
        assert_eq!(PageNumberFormat::Arabic.format(3), "3");
        assert_eq!(PageNumberFormat::RomanUpper.format(4), "IV");
        assert_eq!(PageNumberFormat::RomanLower.format(5), "v");
        assert_eq!(PageNumberFormat::Alphabetical.format(2), "B");

        let config = HeaderFooterConfig {
            header_text: "Document Title - Page {page} of {total}".into(),
            footer_text: "Confidential - {page}".into(),
            alignment: "center".into(),
            page_number_format: PageNumberFormat::Arabic,
            different_first_page: true,
        };

        // First page should be omitted
        assert_eq!(config.format_header(1, 10), None);
        assert_eq!(config.format_footer(1, 10), None);

        // Second page
        assert_eq!(
            config.format_header(2, 10),
            Some("Document Title - Page 2 of 10".into())
        );
        assert_eq!(config.format_footer(2, 10), Some("Confidential - 2".into()));
    }

    #[test]
    fn multi_column_layout_and_drop_caps() {
        let single = MultiColumnConfig::default();
        assert_eq!(single.calculate_column_width(500.0), 500.0);

        let two_col = MultiColumnConfig {
            columns: ColumnCount::TwoColumns,
            column_gap_pt: 20.0,
            show_separator_line: true,
        };
        // (500 - 20) / 2 = 240.0
        assert_eq!(two_col.calculate_column_width(500.0), 240.0);

        let three_col = MultiColumnConfig {
            columns: ColumnCount::ThreeColumns,
            column_gap_pt: 10.0,
            show_separator_line: false,
        };
        // (500 - 20) / 3 = 160.0
        assert_eq!(three_col.calculate_column_width(500.0), 160.0);

        let drop_cap = DropCapConfig::default();
        assert_eq!(drop_cap.lines, 3);
        assert_eq!(drop_cap.characters, 1);
        assert!(!drop_cap.enabled);
    }

    #[test]
    fn watermark_and_line_numbering() {
        let watermark = WatermarkConfig::default();
        assert_eq!(watermark.text, "DRAFT");
        assert_eq!(watermark.rotation_deg, -45.0);
        assert!(!watermark.enabled);

        let line_num = LineNumberingConfig::default();
        assert_eq!(line_num.start_number, 1);
        assert_eq!(line_num.count_by, 1);
        assert!(!line_num.restart_each_page);
    }

    #[test]
    fn break_config_and_kinds() {
        let page_break = BreakConfig::new(BreakKind::PageBreak);
        assert_eq!(page_break.kind, BreakKind::PageBreak);
        assert_eq!(page_break.orientation_override, None);

        let mut section = BreakConfig::new(BreakKind::SectionBreakNextPage);
        section.orientation_override = Some(PageOrientation::Landscape);
        section.restart_page_numbering = true;
        assert_eq!(
            section.orientation_override,
            Some(PageOrientation::Landscape)
        );
        assert!(section.restart_page_numbering);
    }

    #[test]
    fn text_hyphenation_and_soft_hyphens() {
        let config = HyphenationConfig::default();
        let pts = find_hyphenation_points("international", &config);
        assert!(!pts.is_empty());

        let text = "The international conference was outstanding.";
        let hyphenated = insert_soft_hyphens(text, &config);
        assert!(hyphenated.contains('\u{00AD}'));
    }

    #[test]
    fn line_break_penalty_optimization() {
        let config = LineBreakPenaltyConfig::default();

        // Clean break with no hyphen or widow
        let clean = calculate_line_break_penalty(false, false, false, &config);
        assert_eq!(clean, 0);

        // Single hyphen break
        let single_hyphen = calculate_line_break_penalty(true, false, false, &config);
        assert_eq!(single_hyphen, 50);

        // Two consecutive hyphens
        let consecutive_hyphen = calculate_line_break_penalty(true, true, false, &config);
        assert_eq!(consecutive_hyphen, 170); // 50 + 120

        // Widow/orphan line
        let widow = calculate_line_break_penalty(false, false, true, &config);
        assert_eq!(widow, 150);
    }

    #[test]
    fn footnote_numbering_markers() {
        assert_eq!(
            calculate_footnote_marker(1, FootnoteNumberingStyle::Numeric),
            "1"
        );
        assert_eq!(
            calculate_footnote_marker(5, FootnoteNumberingStyle::Numeric),
            "5"
        );

        assert_eq!(
            calculate_footnote_marker(1, FootnoteNumberingStyle::RomanLower),
            "i"
        );
        assert_eq!(
            calculate_footnote_marker(4, FootnoteNumberingStyle::RomanLower),
            "iv"
        );
        assert_eq!(
            calculate_footnote_marker(9, FootnoteNumberingStyle::RomanLower),
            "ix"
        );

        assert_eq!(
            calculate_footnote_marker(1, FootnoteNumberingStyle::AlphabeticalLower),
            "a"
        );
        assert_eq!(
            calculate_footnote_marker(3, FootnoteNumberingStyle::AlphabeticalLower),
            "c"
        );

        assert_eq!(
            calculate_footnote_marker(1, FootnoteNumberingStyle::Symbols),
            "*"
        );
        assert_eq!(
            calculate_footnote_marker(2, FootnoteNumberingStyle::Symbols),
            "†"
        );
        assert_eq!(
            calculate_footnote_marker(7, FootnoteNumberingStyle::Symbols),
            "**"
        ); // Wrapped second pass
    }

    #[test]
    fn advanced_search_and_replace_with_options() {
        let sample = "The quick brown fox jumps over the lazy Fox.";

        // Case-sensitive search for "Fox"
        let case_sens = SearchOptions {
            case_sensitive: true,
            match_whole_word: false,
        };
        let m_case = find_matches_with_options(sample, "Fox", &case_sens);
        assert_eq!(m_case.len(), 1);
        assert_eq!(&sample[m_case[0].0..m_case[0].1], "Fox");

        // Case-insensitive search for "fox"
        let case_insens = SearchOptions {
            case_sensitive: false,
            match_whole_word: false,
        };
        let m_all = find_matches_with_options(sample, "fox", &case_insens);
        assert_eq!(m_all.len(), 2);

        // Whole word search for "the"
        let whole_word = SearchOptions {
            case_sensitive: false,
            match_whole_word: true,
        };
        let m_the = find_matches_with_options(sample, "the", &whole_word);
        assert_eq!(m_the.len(), 2);

        // Replace all "fox" with "dog"
        let replaced = replace_matches(sample, "fox", "dog", &case_insens);
        assert_eq!(replaced, "The quick brown dog jumps over the lazy dog.");
    }

    #[test]
    fn mail_merge_template_rendering() {
        // Field extraction preserves first-appearance order and deduplicates.
        let template = MergeTemplate::new(
            "Dear {{name}}, your {{plan}} plan renews soon. Thank you, {{name}}.",
        );
        assert_eq!(template.extract_fields(), vec!["name", "plan"]);

        // Rendering substitutes every occurrence of known fields.
        let mut record = std::collections::BTreeMap::new();
        record.insert("name".to_string(), "Ada".to_string());
        record.insert("plan".to_string(), "Studio".to_string());
        assert_eq!(
            template.render_record(&record),
            "Dear Ada, your Studio plan renews soon. Thank you, Ada."
        );

        // Unknown placeholders are left intact.
        let mut partial = std::collections::BTreeMap::new();
        partial.insert("plan".to_string(), "Pro".to_string());
        assert_eq!(
            template.render_record(&partial),
            "Dear {{name}}, your Pro plan renews soon. Thank you, {{name}}."
        );

        // A template without placeholders renders unchanged even with an empty map.
        let plain = MergeTemplate::new("No placeholders in this body.");
        let empty: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
        assert_eq!(plain.render_record(&empty), "No placeholders in this body.");
    }

    #[test]
    fn document_outline_extraction() {
        let blocks = vec![
            RichBlock::new(1, "paragraph", "Preamble text."),
            RichBlock::new(2, "heading1", "Introduction"),
            RichBlock::new(3, "paragraph", "Intro body."),
            RichBlock::new(4, "heading2", "Background"),
            RichBlock::new(5, "heading3", "Prior Work"),
            RichBlock::new(6, "heading2", "   "),
            RichBlock::new(7, "quote", "A quoted passage."),
            RichBlock::new(8, "heading1", "Conclusion"),
        ];

        let outline = extract_outline(&blocks);
        assert_eq!(outline.len(), 4);
        assert_eq!(outline[0].level, 1);
        assert_eq!(outline[0].title, "Introduction");
        assert_eq!(outline[0].block_index, 1);
        assert_eq!(outline[1].level, 2);
        assert_eq!(outline[1].title, "Background");
        assert_eq!(outline[1].block_index, 3);
        assert_eq!(outline[2].level, 3);
        assert_eq!(outline[2].title, "Prior Work");
        assert_eq!(outline[2].block_index, 4);
        assert_eq!(outline[3].level, 1);
        assert_eq!(outline[3].title, "Conclusion");
        assert_eq!(outline[3].block_index, 7);

        // Whitespace-only headings and non-heading kinds never appear.
        assert!(outline.iter().all(|entry| !entry.title.trim().is_empty()));

        // Indentation is two spaces per level below one.
        assert_eq!(outline[0].render_indented(), "Introduction");
        assert_eq!(outline[1].render_indented(), "  Background");
        assert_eq!(outline[2].render_indented(), "    Prior Work");

        assert!(extract_outline(&[]).is_empty());
    }

    #[test]
    fn citation_formatting_styles() {
        let entry = CitationEntry {
            author: "Ada Lovelace".to_string(),
            title: "Notes on the Analytical Engine".to_string(),
            year: 1843,
            publisher: "Taylor's Scientific Memoirs".to_string(),
        };

        // One fixture entry rendered in all four styles with exact forms.
        assert_eq!(
            entry.format(CitationStyle::Apa, 1),
            "Ada Lovelace (1843). Notes on the Analytical Engine. Taylor's Scientific Memoirs."
        );
        assert_eq!(
            entry.format(CitationStyle::Mla, 1),
            "Ada Lovelace. \"Notes on the Analytical Engine.\" Taylor's Scientific Memoirs, 1843."
        );
        assert_eq!(
            entry.format(CitationStyle::Chicago, 1),
            "Ada Lovelace. \"Notes on the Analytical Engine.\" Taylor's Scientific Memoirs, 1843."
        );
        assert_eq!(
            entry.format(CitationStyle::Ieee, 1),
            "[1] Ada Lovelace, \"Notes on the Analytical Engine,\" Taylor's Scientific Memoirs, 1843."
        );

        // Empty publishers omit their segment without double spaces.
        let bare = CitationEntry {
            author: "Ada Lovelace".to_string(),
            title: "Notes on the Analytical Engine".to_string(),
            year: 1843,
            publisher: String::new(),
        };
        assert_eq!(
            bare.format(CitationStyle::Apa, 2),
            "Ada Lovelace (1843). Notes on the Analytical Engine."
        );
        assert_eq!(
            bare.format(CitationStyle::Mla, 2),
            "Ada Lovelace. \"Notes on the Analytical Engine.\" 1843."
        );
        assert_eq!(
            bare.format(CitationStyle::Chicago, 2),
            "Ada Lovelace. \"Notes on the Analytical Engine.\" 1843."
        );
        assert_eq!(
            bare.format(CitationStyle::Ieee, 2),
            "[2] Ada Lovelace, \"Notes on the Analytical Engine,\" 1843."
        );

        // Multi-word authors derive "Surname, Initial." from the display name.
        let multi = CitationEntry {
            author: "Grace Brewster Hopper".to_string(),
            ..CitationEntry::default()
        };
        assert_eq!(multi.surname_initial(), "Hopper, G.");

        // Single-word authors pass through unchanged.
        let single = CitationEntry {
            author: "Plato".to_string(),
            ..CitationEntry::default()
        };
        assert_eq!(single.surname_initial(), "Plato");
    }

    #[test]
    fn word_diff_lcs_operations() {
        // Identical texts collapse into a single equal run.
        assert_eq!(
            word_diff("the quick brown fox", "the quick brown fox"),
            vec![WordDiffOp::Equal(vec![
                "the".to_string(),
                "quick".to_string(),
                "brown".to_string(),
                "fox".to_string()
            ])]
        );

        // One substituted word plus one inserted word; ties prefer deletion
        // first, so the deleted run precedes the inserted run.
        assert_eq!(
            word_diff("the quick fox", "the slow brown fox"),
            vec![
                WordDiffOp::Equal(vec!["the".to_string()]),
                WordDiffOp::Deleted(vec!["quick".to_string()]),
                WordDiffOp::Inserted(vec!["slow".to_string(), "brown".to_string()]),
                WordDiffOp::Equal(vec!["fox".to_string()]),
            ]
        );

        // Empty old text: every new word is one inserted run.
        assert_eq!(
            word_diff("", "alpha beta"),
            vec![WordDiffOp::Inserted(vec![
                "alpha".to_string(),
                "beta".to_string()
            ])]
        );

        // Empty new text: every old word is one deleted run.
        assert_eq!(
            word_diff("alpha beta", ""),
            vec![WordDiffOp::Deleted(vec![
                "alpha".to_string(),
                "beta".to_string()
            ])]
        );

        // Both empty: no operations at all.
        assert_eq!(word_diff("", ""), Vec::new());

        // Reconstruction property: keeping equal words and applying the
        // deletions/insertions to the old word stream must reproduce the new
        // word stream exactly.
        let words = |t: &str| t.split_whitespace().map(str::to_string).collect::<Vec<_>>();
        let reconstruct = |diff: &[WordDiffOp]| {
            let mut old_side = Vec::new();
            let mut new_side = Vec::new();
            for op in diff {
                match op {
                    WordDiffOp::Equal(ws) => {
                        old_side.extend(ws.iter().cloned());
                        new_side.extend(ws.iter().cloned());
                    }
                    WordDiffOp::Deleted(ws) => old_side.extend(ws.iter().cloned()),
                    WordDiffOp::Inserted(ws) => new_side.extend(ws.iter().cloned()),
                }
            }
            (old_side, new_side)
        };

        let old_text = "loom writer tracks every edit with great care";
        let new_text = "loom editor will track all edits with care";
        let diff = word_diff(old_text, new_text);
        let (old_side, new_side) = reconstruct(&diff);
        assert_eq!(old_side, words(old_text));
        assert_eq!(new_side, words(new_text));

        // The property also holds for the empty-input edge cases.
        let (_, new_side) = reconstruct(&word_diff("", "a b c"));
        assert_eq!(new_side, words("a b c"));
        let (old_side, _) = reconstruct(&word_diff("x y", ""));
        assert_eq!(old_side, words("x y"));
    }

    #[test]
    fn text_import_paragraph_detection() {
        // Blank-line separated paragraphs; soft single newlines stay inside a paragraph
        let imported = import_text_paragraphs("First para.\nStill first.\n\nSecond para.").unwrap();
        assert_eq!(
            imported,
            vec![
                "First para.\nStill first.".to_string(),
                "Second para.".to_string()
            ]
        );

        // Leading/trailing blank lines and CRLF are normalized away
        let padded = import_text_paragraphs("\r\n\r\nAlpha\r\nBeta\r\n\r\n").unwrap();
        assert_eq!(padded, vec!["Alpha\nBeta".to_string()]);

        // Multiple consecutive blank lines collapse into one break
        let gapped = import_text_paragraphs("One\n\n\n\nTwo").unwrap();
        assert_eq!(gapped, vec!["One".to_string(), "Two".to_string()]);

        // Whitespace-only lines count as blank
        let spaced = import_text_paragraphs("A\n   \nB").unwrap();
        assert_eq!(spaced, vec!["A".to_string(), "B".to_string()]);

        // Empty or blank-only input is an error
        assert!(import_text_paragraphs("").is_err());
        assert!(import_text_paragraphs("\n\n  \n").is_err());
    }

    #[test]
    fn linked_table_paste_validation() {
        let mut region = LinkedTableRegion {
            region_id: "lt-1".into(),
            source_workbook: "/tmp/budget.loomsheet".into(),
            source_sheet: "Summary".into(),
            source_range: "A1:B2".into(),
            snapshot_rows: vec![
                vec!["Item".to_string(), "Cost".to_string()],
                vec!["Desk".to_string(), "120".to_string()],
            ],
            refreshable: true,
        };

        // A well-formed region validates.
        assert_eq!(region.validate(), Ok(()));

        // Exact plain rendering of a 2x2 snapshot.
        assert_eq!(region.render_plain(), "Item | Cost\nDesk | 120");

        // update_snapshot replaces the snapshot and re-renders from it.
        region
            .update_snapshot(vec![
                vec!["Item".to_string(), "Cost".to_string()],
                vec!["Chair".to_string(), "85".to_string()],
            ])
            .unwrap();
        assert_eq!(region.render_plain(), "Item | Cost\nChair | 85");
        assert_eq!(region.validate(), Ok(()));

        // Ragged snapshot rows are rejected by validate.
        let ragged = LinkedTableRegion {
            region_id: "lt-2".into(),
            source_workbook: "/tmp/wb.loomsheet".into(),
            source_sheet: "Data".into(),
            source_range: "A1:C2".into(),
            snapshot_rows: vec![
                vec!["a".to_string(), "b".to_string(), "c".to_string()],
                vec!["d".to_string()],
            ],
            refreshable: false,
        };
        assert_eq!(
            ragged.validate().unwrap_err(),
            "snapshot row 1 has 1 cells but the table width is 3"
        );

        // update_snapshot rejects rows whose width differs and leaves the
        // snapshot untouched on failure.
        assert_eq!(
            region
                .update_snapshot(vec![
                    vec!["x".to_string(), "y".to_string()],
                    vec!["z".to_string()],
                ])
                .unwrap_err(),
            "new snapshot row 1 has 1 cells but the table width is 2"
        );
        assert_eq!(region.render_plain(), "Item | Cost\nChair | 85");

        // An empty source range is invalid; so is one without exactly one ':'.
        for bad in ["", "A1B2", "A1:", ":C9", "A1:B2:C9"] {
            let mut broken = LinkedTableRegion {
                region_id: "lt-3".into(),
                source_workbook: "/tmp/wb.loomsheet".into(),
                source_sheet: "Data".into(),
                source_range: bad.to_string(),
                snapshot_rows: vec![vec!["a".to_string()]],
                refreshable: false,
            };
            assert!(broken.validate().is_err());
            // Sanity: fixing only the range makes it valid again.
            broken.source_range = "A1:A1".into();
            assert_eq!(broken.validate(), Ok(()));
        }
    }

    #[test]
    fn outline_numbering_hierarchies() {
        let entry = |level: u8, title: &str, index: usize| OutlineEntry {
            level,
            title: title.to_string(),
            block_index: index,
        };

        let entries = vec![
            entry(1, "Intro", 0),
            entry(2, "Background", 2),
            entry(2, "Prior Work", 4),
            entry(3, "Survey", 5),
            entry(1, "Method", 8),
            entry(2, "Setup", 10),
        ];
        assert_eq!(
            numbered_outline(&entries),
            vec![
                "1. Intro",
                "1.1. Background",
                "1.2. Prior Work",
                "1.2.1. Survey",
                "2. Method",
                "2.1. Setup",
            ]
        );

        // A deep jump nests as a single +1 step
        let jumped = vec![entry(1, "One", 0), entry(5, "Deep", 1)];
        assert_eq!(numbered_outline(&jumped), vec!["1. One", "1.1. Deep"]);

        // Level resets clear deeper counters
        let reset = vec![entry(2, "A", 0), entry(3, "B", 1), entry(2, "C", 2)];
        assert_eq!(numbered_outline(&reset), vec!["1. A", "1.1. B", "2. C"]);

        assert!(numbered_outline(&[]).is_empty());
    }
}
