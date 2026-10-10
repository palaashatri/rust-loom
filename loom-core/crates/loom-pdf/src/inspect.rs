//! Read-back helpers for PDFs written by this crate.
//!
//! Page text is stored as glyph ids, so a test (or an exporter's regression
//! check) cannot look for the words in the raw bytes any more. These helpers
//! read the cross-reference table, inflate content streams and use each font's
//! `ToUnicode` CMap to turn glyph ids back into characters.
//!
//! This is *not* a general PDF reader: it understands exactly the object
//! shapes [`crate::PdfDocument::serialize`] produces (classic xref table, no
//! object streams, direct `/Length`).

use std::collections::{BTreeMap, HashMap};
use std::io::Read as _;

/// One embedded font program, as found in a written PDF.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddedFont {
    /// Page resource name, for example `F2`.
    pub resource: String,
    /// `BaseFont`, including the six-letter subset prefix.
    pub base_font: String,
    /// Whether the name carries a subset prefix (`ABCDEF+Name`).
    pub subset: bool,
    /// Size of the compressed `FontFile2` stream.
    pub program_bytes: usize,
    /// Size of the font program after inflating it.
    pub inflated_program_bytes: usize,
    /// The font program, inflated (a subset TrueType file).
    pub program: Vec<u8>,
    /// Glyphs the `ToUnicode` CMap names.
    pub mapped_glyphs: usize,
}

/// The characters shown on each page; text runs from different `BT`..`ET`
/// blocks are separated by `\n`.
pub fn page_text(pdf: &[u8]) -> Result<Vec<String>, String> {
    let document = Document::parse(pdf)?;
    document
        .pages()?
        .iter()
        .map(|page| Ok(document.render(page)?.1))
        .collect()
}

/// Every page's content stream, decompressed, with each hex glyph string
/// replaced by the characters it shows as `(literal)` (parentheses and
/// backslashes escaped). Pages are joined by `\n`.
pub fn readable_content(pdf: &[u8]) -> Result<String, String> {
    let document = Document::parse(pdf)?;
    let mut out = Vec::new();
    for page in document.pages()? {
        out.push(document.render(&page)?.0);
    }
    Ok(out.join("\n"))
}

/// The embedded fonts, in resource-name order.
pub fn embedded_fonts(pdf: &[u8]) -> Result<Vec<EmbeddedFont>, String> {
    let document = Document::parse(pdf)?;
    let mut fonts: BTreeMap<String, EmbeddedFont> = BTreeMap::new();
    for page in document.pages()? {
        for (resource, number) in &page.fonts {
            if fonts.contains_key(resource) {
                continue;
            }
            fonts.insert(resource.clone(), document.font_info(resource, *number)?);
        }
    }
    Ok(fonts.into_values().collect())
}

struct Page {
    contents: usize,
    fonts: BTreeMap<String, usize>,
}

struct Document<'a> {
    bytes: &'a [u8],
    /// Byte offset of each object, index = object number.
    offsets: Vec<usize>,
    /// End of each object's bytes (start of the next object or the xref).
    ends: Vec<usize>,
}

impl<'a> Document<'a> {
    fn parse(bytes: &'a [u8]) -> Result<Self, String> {
        let marker = b"startxref\n";
        let at = bytes
            .windows(marker.len())
            .rposition(|w| w == marker)
            .ok_or("no startxref")?;
        let xref_pos: usize = text(&bytes[at + marker.len()..])
            .split_whitespace()
            .next()
            .and_then(|n| n.parse().ok())
            .ok_or("bad startxref")?;
        let table = text(&bytes[xref_pos..]);
        let mut lines = table.lines();
        if lines.next() != Some("xref") {
            return Err("xref table missing".into());
        }
        let count: usize = lines
            .next()
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|n| n.parse().ok())
            .ok_or("bad xref count")?;
        let mut offsets = vec![0usize];
        for line in lines.skip(1).take(count.saturating_sub(1)) {
            offsets.push(
                line.split_whitespace()
                    .next()
                    .and_then(|n| n.parse().ok())
                    .ok_or("bad xref entry")?,
            );
        }
        let mut sorted: Vec<usize> = offsets[1..].to_vec();
        sorted.push(xref_pos);
        sorted.sort_unstable();
        let ends = offsets
            .iter()
            .map(|offset| {
                sorted
                    .iter()
                    .copied()
                    .find(|candidate| candidate > offset)
                    .unwrap_or(xref_pos)
            })
            .collect();
        Ok(Self {
            bytes,
            offsets,
            ends,
        })
    }

    /// The dictionary text and (for a stream) the raw stream data of object `n`.
    fn object(&self, n: usize) -> Result<(String, Option<&'a [u8]>), String> {
        let start = *self
            .offsets
            .get(n)
            .ok_or_else(|| format!("no object {n}"))?;
        let mut body = &self.bytes[start..self.ends[n]];
        // "N 0 obj\n" header.
        let header_end = body
            .iter()
            .position(|b| *b == b'\n')
            .ok_or("object header")?;
        body = &body[header_end + 1..];
        let trailer = b"\nendobj\n";
        if body.ends_with(trailer) {
            body = &body[..body.len() - trailer.len()];
        }
        let marker = b"stream\n";
        match body.windows(marker.len()).position(|w| w == marker) {
            Some(at) if body[..at].starts_with(b"<<") => {
                let dict = text(&body[..at]);
                let data = &body[at + marker.len()..];
                let data = data.strip_suffix(b"\nendstream").unwrap_or(data);
                Ok((dict, Some(data)))
            }
            _ => Ok((text(body), None)),
        }
    }

    fn catalog_pages(&self) -> Result<Vec<usize>, String> {
        let (catalog, _) = self.object(1)?;
        let tree = reference_after(&catalog, "/Pages").ok_or("no /Pages")?;
        let (dict, _) = self.object(tree)?;
        let kids_at = dict.find("/Kids").ok_or("no /Kids")?;
        let list = &dict[kids_at..];
        let list = &list[list.find('[').ok_or("kids")? + 1..list.find(']').ok_or("kids")?];
        Ok(references(list))
    }

    fn pages(&self) -> Result<Vec<Page>, String> {
        self.catalog_pages()?
            .into_iter()
            .map(|number| {
                let (dict, _) = self.object(number)?;
                let contents = reference_after(&dict, "/Contents").ok_or("no /Contents")?;
                let mut fonts = BTreeMap::new();
                if let Some(at) = dict.find("/Font <<") {
                    let inner = &dict[at + "/Font <<".len()..];
                    let inner = &inner[..inner.find(">>").unwrap_or(inner.len())];
                    let mut tokens = inner.split_whitespace();
                    while let Some(name) = tokens.next() {
                        let number = tokens.next().and_then(|n| n.parse().ok());
                        let _generation = tokens.next();
                        let _r = tokens.next();
                        if let (Some(name), Some(number)) = (name.strip_prefix('/'), number) {
                            fonts.insert(name.to_string(), number);
                        }
                    }
                }
                Ok(Page { contents, fonts })
            })
            .collect()
    }

    /// Decompressed bytes of a stream object.
    fn stream(&self, n: usize) -> Result<Vec<u8>, String> {
        let (dict, data) = self.object(n)?;
        let data = data.ok_or_else(|| format!("object {n} is not a stream"))?;
        if dict.contains("/FlateDecode") {
            let mut inflated = Vec::new();
            flate2::read::ZlibDecoder::new(data)
                .read_to_end(&mut inflated)
                .map_err(|e| format!("object {n}: {e}"))?;
            Ok(inflated)
        } else {
            Ok(data.to_vec())
        }
    }

    /// `ToUnicode`: glyph id to text, for the `Type0` font object `n`.
    fn cmap(&self, n: usize) -> Result<HashMap<u16, String>, String> {
        let (dict, _) = self.object(n)?;
        let to_unicode = reference_after(&dict, "/ToUnicode").ok_or("no /ToUnicode")?;
        let stream = self.stream(to_unicode)?;
        let mut map = HashMap::new();
        let mut in_block = false;
        for line in text(&stream).lines() {
            let line = line.trim();
            if line.ends_with("beginbfchar") {
                in_block = true;
            } else if line == "endbfchar" {
                in_block = false;
            } else if in_block {
                let mut parts = line.split_whitespace();
                if let (Some(code), Some(value)) = (parts.next(), parts.next()) {
                    let code = u16::from_str_radix(code.trim_matches(['<', '>']), 16)
                        .map_err(|e| e.to_string())?;
                    map.insert(code, utf16_hex(value.trim_matches(['<', '>']))?);
                }
            }
        }
        Ok(map)
    }

    fn font_info(&self, resource: &str, n: usize) -> Result<EmbeddedFont, String> {
        let (type0, _) = self.object(n)?;
        let descendant = reference_after(&type0, "/DescendantFonts").ok_or("no descendant")?;
        let (cid, _) = self.object(descendant)?;
        let base_font = name_after(&cid, "/BaseFont").ok_or("no BaseFont")?;
        let descriptor = reference_after(&cid, "/FontDescriptor").ok_or("no descriptor")?;
        let (descriptor, _) = self.object(descriptor)?;
        let file = reference_after(&descriptor, "/FontFile2").ok_or("no FontFile2")?;
        let (_, data) = self.object(file)?;
        let program = self.stream(file)?;
        Ok(EmbeddedFont {
            resource: resource.to_string(),
            subset: base_font.as_bytes().get(6) == Some(&b'+'),
            base_font,
            program_bytes: data.map_or(0, <[u8]>::len),
            inflated_program_bytes: program.len(),
            program,
            mapped_glyphs: self.cmap(n)?.len(),
        })
    }

    /// `(readable content, extracted text)` of one page.
    fn render(&self, page: &Page) -> Result<(String, String), String> {
        let mut cmaps: HashMap<&str, HashMap<u16, String>> = HashMap::new();
        for (name, number) in &page.fonts {
            cmaps.insert(name, self.cmap(*number)?);
        }
        let content = String::from_utf8_lossy(&self.stream(page.contents)?).into_owned();
        let mut readable = Vec::new();
        let mut lines = Vec::new();
        let mut font = String::new();
        let mut last_name = String::new();
        let mut run = String::new();
        for line in content.lines() {
            let mut tokens = Vec::new();
            for token in line.split(' ') {
                if let Some(name) = token.strip_prefix('/') {
                    last_name = name.to_string();
                } else if token == "Tf" {
                    font.clone_from(&last_name);
                } else if token == "ET" && !run.is_empty() {
                    lines.push(std::mem::take(&mut run));
                }
                if let Some(hex) = token
                    .strip_prefix('<')
                    .and_then(|t| t.strip_suffix('>'))
                    .filter(|t| !t.starts_with('<'))
                {
                    let shown = decode_glyphs(hex, cmaps.get(font.as_str()))?;
                    run.push_str(&shown);
                    tokens.push(format!("({})", escape_literal(&shown)));
                } else {
                    tokens.push(token.to_string());
                }
            }
            readable.push(tokens.join(" "));
        }
        if !run.is_empty() {
            lines.push(run);
        }
        Ok((readable.join("\n"), lines.join("\n")))
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// The object number in `/Key N G R` (the first match).
fn reference_after(dict: &str, key: &str) -> Option<usize> {
    let rest = dict[dict.find(key)? + key.len()..].trim_start();
    let rest = rest.strip_prefix('[').unwrap_or(rest);
    rest.split_whitespace().next()?.parse().ok()
}

/// Every `N G R` object number in `list`.
fn references(list: &str) -> Vec<usize> {
    let tokens: Vec<&str> = list.split_whitespace().collect();
    tokens
        .windows(3)
        .filter(|w| w[2] == "R")
        .filter_map(|w| w[0].parse().ok())
        .collect()
}

fn name_after(dict: &str, key: &str) -> Option<String> {
    let rest = dict[dict.find(key)? + key.len()..].trim_start();
    let rest = rest.strip_prefix('/')?;
    Some(rest.split_whitespace().next()?.to_string())
}

fn utf16_hex(hex: &str) -> Result<String, String> {
    let units: Result<Vec<u16>, _> = hex
        .as_bytes()
        .chunks(4)
        .map(|chunk| u16::from_str_radix(&text(chunk), 16))
        .collect();
    let units = units.map_err(|e| e.to_string())?;
    Ok(String::from_utf16_lossy(&units))
}

/// Characters for a hex string of two-byte glyph ids; an unmapped id (such as
/// `.notdef`) shows as U+FFFD.
fn decode_glyphs(hex: &str, cmap: Option<&HashMap<u16, String>>) -> Result<String, String> {
    let mut out = String::new();
    for chunk in hex.as_bytes().chunks(4) {
        let gid = u16::from_str_radix(&text(chunk), 16).map_err(|e| e.to_string())?;
        match cmap.and_then(|m| m.get(&gid)) {
            Some(shown) => out.push_str(shown),
            None => out.push('\u{FFFD}'),
        }
    }
    Ok(out)
}

fn escape_literal(s: &str) -> String {
    let mut out = String::new();
    for ch in s.chars() {
        match ch {
            '(' => out.push_str("\\("),
            ')' => out.push_str("\\)"),
            '\\' => out.push_str("\\\\"),
            c => out.push(c),
        }
    }
    out
}
