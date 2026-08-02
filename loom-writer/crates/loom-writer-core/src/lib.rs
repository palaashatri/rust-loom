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
            mime: MimeType::parse("application/vnd.loom.document-content").unwrap(),
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
