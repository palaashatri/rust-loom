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
            s.push_str(&pkg_json::escape(match b.style.alignment {
                loom_text::Alignment::Left => "left",
                loom_text::Alignment::Center => "center",
                loom_text::Alignment::Right => "right",
                loom_text::Alignment::Justify => "justify",
            }));
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
                "id" => id = v.clone(),
                "title" => title = v.clone(),
                "blocks" => blocks = parse_blocks(v)?,
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
        let mut align = String::from("left");
        for (k, v) in &fields {
            match k.as_str() {
                "id" => {
                    id = v
                        .parse()
                        .map_err(|_| WriterError::Invalid("bad id".into()))?
                }
                "kind" => kind = v.clone(),
                "text" => text = v.clone(),
                "align" => align = v.clone(),
                _ => {}
            }
        }
        let mut b = RichBlock::new(id, &kind, &text);
        b.style.alignment = match align.as_str() {
            "center" => loom_text::Alignment::Center,
            "right" => loom_text::Alignment::Right,
            "justify" => loom_text::Alignment::Justify,
            _ => loom_text::Alignment::Left,
        };
        out.push(b);
    }
    Ok(out)
}

/// A tiny bounded JSON parser for Writer content (reuses the same structure
/// as loom_package's manifest parser but simpler; keeps the workspace
/// dependency-free).
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

    fn parse(&self) -> Result<Vec<(String, String)>, WriterError> {
        // Parse an object, collecting string-valued fields only.
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
            let key = p.parse_string()?;
            p.skip_ws();
            if p.next() != Some(b':') {
                return Err(WriterError::Json("expected ':'".into()));
            }
            // Capture the value as raw text.
            let val_raw = p.capture_value()?;
            fields.push((key, val_raw));
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
}
