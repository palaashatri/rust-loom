//! Read-back helpers for PDFs written by this crate.
//!
//! Page text is stored as glyph ids, so a test (or an exporter's regression
//! check) cannot look for the words in the raw bytes any more. These helpers
//! read the cross-reference table, inflate content streams and use each font's
//! `ToUnicode` CMap (and `/ActualText` spans) to turn glyph ids back into
//! characters.
//!
//! This is *not* a general PDF reader: it understands exactly the object
//! shapes [`crate::PdfDocument::serialize`] produces (classic xref table, no
//! object streams, direct `/Length`). It does check the structure it reads and
//! returns an error for a file that is internally inconsistent: xref offsets,
//! the trailer size, every stream's declared `/Length`, the page count, each
//! font's `/W` widths against its embedded program, `ToUnicode` entry counts,
//! and that every shown glyph id is `.notdef` or named by its font's map.
//! [`verify`] runs all of those over a whole file.
//!
//! Compiled for this crate's own tests and for crates that enable the
//! `test-support` feature; it is not part of the production API.

use std::collections::{BTreeMap, HashMap};
use std::io::Read as _;

/// One embedded font program, as found in a written PDF.
#[derive(Debug, Clone, PartialEq)]
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
    /// The `/W` widths, in glyph-id order, in 1/1000 em.
    pub widths: Vec<f32>,
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

/// The spacing numbers of every `TJ` array in every page's content stream, in
/// drawing order: thousandths of an em to move back (positive) or forward
/// (negative) after the glyph before the number. Empty when no run was spaced
/// by a [`crate::RunShaper`].
pub fn spacing_adjustments(pdf: &[u8]) -> Result<Vec<f32>, String> {
    let document = Document::parse(pdf)?;
    let mut numbers = Vec::new();
    for page in document.pages()? {
        let content = String::from_utf8_lossy(&document.stream(page.contents)?).into_owned();
        let mut in_array = false;
        for token in content.split_whitespace() {
            match token {
                "[" => in_array = true,
                "]" => in_array = false,
                _ if in_array && !token.starts_with('<') => {
                    numbers.push(
                        token
                            .parse::<f32>()
                            .map_err(|e| format!("bad TJ number {token:?}: {e}"))?,
                    );
                }
                _ => {}
            }
        }
    }
    Ok(numbers)
}

/// Checks the whole file's structure: header and trailer, every xref entry,
/// every object (so every stream's `/Length`), the page tree, each embedded
/// font against its program, and every page's text. `Ok` means the file is
/// internally consistent; it does not prove a third-party reader accepts it.
pub fn verify(pdf: &[u8]) -> Result<(), String> {
    let document = Document::parse(pdf)?;
    for number in 1..document.offsets.len() {
        document.object(number)?;
    }
    embedded_fonts(pdf)?;
    readable_content(pdf)?;
    Ok(())
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
        if !bytes.starts_with(b"%PDF-1.") {
            return Err("missing %PDF-1.x header".into());
        }
        if !bytes.ends_with(b"%%EOF\n") {
            return Err("file does not end with %%EOF".into());
        }
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
        if xref_pos >= bytes.len() || !bytes[xref_pos..].starts_with(b"xref\n") {
            return Err(format!(
                "startxref {xref_pos} does not point at the xref table"
            ));
        }
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
        if offsets.len() != count {
            return Err(format!(
                "xref declares {count} entries but lists {}",
                offsets.len()
            ));
        }
        let size: usize = table
            .find("/Size ")
            .and_then(|i| table[i + "/Size ".len()..].split_whitespace().next())
            .and_then(|n| n.parse().ok())
            .ok_or("trailer has no /Size")?;
        if size != count {
            return Err(format!(
                "trailer /Size {size} but the xref has {count} entries"
            ));
        }
        for (number, offset) in offsets.iter().enumerate().skip(1) {
            let header = format!("{number} 0 obj\n");
            if !bytes
                .get(*offset..)
                .is_some_and(|rest| rest.starts_with(header.as_bytes()))
            {
                return Err(format!(
                    "xref offset {offset} of object {number} does not point at \"{number} 0 obj\""
                ));
            }
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

    /// The dictionary text and (for a stream) the raw stream data of object
    /// `n`. A stream's data must be exactly its declared `/Length`.
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
                let data = data
                    .strip_suffix(b"\nendstream")
                    .ok_or_else(|| format!("object {n}: stream is not closed by endstream"))?;
                let declared = number_after(&dict, "/Length")
                    .ok_or_else(|| format!("object {n}: stream has no /Length"))?;
                if declared != data.len() {
                    return Err(format!(
                        "object {n}: /Length {declared} but the stream holds {} bytes",
                        data.len()
                    ));
                }
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
        let kids = references(list);
        let count = number_after(&dict, "/Count").ok_or("page tree has no /Count")?;
        if kids.is_empty() || kids.len() != count {
            return Err(format!(
                "page tree lists {} pages but /Count is {count}",
                kids.len()
            ));
        }
        Ok(kids)
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
                            if fonts.insert(name.to_string(), number).is_some() {
                                return Err(format!("font resource /{name} is listed twice"));
                            }
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

    /// `ToUnicode`: glyph id to text, for the `Type0` font object `n`. Every
    /// `beginbfchar` block must hold exactly the number of entries it declares.
    fn cmap(&self, n: usize) -> Result<HashMap<u16, String>, String> {
        let (dict, _) = self.object(n)?;
        let to_unicode = reference_after(&dict, "/ToUnicode").ok_or("no /ToUnicode")?;
        let stream = self.stream(to_unicode)?;
        let mut map = HashMap::new();
        let mut declared: Option<usize> = None;
        let mut seen = 0usize;
        for line in text(&stream).lines() {
            let line = line.trim();
            if let Some(count) = line.strip_suffix("beginbfchar") {
                if declared.is_some() {
                    return Err(format!("font {n}: beginbfchar inside a bfchar block"));
                }
                let count: usize = count
                    .trim()
                    .parse()
                    .map_err(|_| format!("font {n}: bad bfchar count {line:?}"))?;
                if count > 100 {
                    return Err(format!("font {n}: bfchar block of {count} exceeds 100"));
                }
                declared = Some(count);
                seen = 0;
            } else if line == "endbfchar" {
                match declared.take() {
                    Some(count) if count == seen => {}
                    Some(count) => {
                        return Err(format!(
                            "font {n}: bfchar block declares {count} entries but holds {seen}"
                        ))
                    }
                    None => return Err(format!("font {n}: endbfchar without beginbfchar")),
                }
            } else if declared.is_some() {
                let mut parts = line.split_whitespace();
                if let (Some(code), Some(value)) = (parts.next(), parts.next()) {
                    let code = u16::from_str_radix(code.trim_matches(['<', '>']), 16)
                        .map_err(|e| e.to_string())?;
                    map.insert(code, utf16_hex(value.trim_matches(['<', '>']))?);
                    seen += 1;
                }
            }
        }
        if declared.is_some() {
            return Err(format!("font {n}: bfchar block is not closed"));
        }
        Ok(map)
    }

    /// The `/W` widths of the descendant font object `cid`.
    fn widths(&self, cid: &str) -> Result<Vec<f32>, String> {
        let at = cid.find("/W [0 [").ok_or("no /W [0 [...]] array")?;
        let rest = &cid[at + "/W [0 [".len()..];
        let list = &rest[..rest.find("]]").ok_or("unterminated /W array")?];
        list.split_whitespace()
            .map(|w| {
                w.parse::<f32>()
                    .map_err(|_| format!("bad width {w:?} in /W"))
            })
            .collect()
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
        let widths = self.widths(&cid)?;
        let cmap = self.cmap(n)?;
        self.check_widths(resource, &cid, &program, &widths)?;
        if let Some(code) = cmap.keys().find(|code| usize::from(**code) >= widths.len()) {
            return Err(format!(
                "font {resource}: ToUnicode names glyph {code} but /W has {} widths",
                widths.len()
            ));
        }
        Ok(EmbeddedFont {
            resource: resource.to_string(),
            subset: base_font.as_bytes().get(6) == Some(&b'+'),
            base_font,
            program_bytes: data.map_or(0, <[u8]>::len),
            inflated_program_bytes: program.len(),
            program,
            mapped_glyphs: cmap.len(),
            widths,
        })
    }

    /// `/W` lists one width per glyph id in id order, each equal to the
    /// advance of that glyph in the embedded program (through the explicit
    /// `CIDToGIDMap` when the whole font was embedded).
    fn check_widths(
        &self,
        resource: &str,
        cid: &str,
        program: &[u8],
        widths: &[f32],
    ) -> Result<(), String> {
        let face = ttf_parser::Face::parse(program, 0)
            .map_err(|e| format!("font {resource}: embedded program does not parse: {e}"))?;
        // With the identity map a glyph id is a CID; the subsetter may keep
        // component glyphs after the ones /W lists, which no text refers to.
        let original: Vec<u16> = if cid.contains("/CIDToGIDMap /Identity") {
            (0..face.number_of_glyphs().min(widths.len() as u16)).collect()
        } else {
            let map = reference_after(cid, "/CIDToGIDMap")
                .ok_or_else(|| format!("font {resource}: no /CIDToGIDMap"))?;
            self.stream(map)?
                .chunks_exact(2)
                .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
                .collect()
        };
        if original.len() != widths.len() || widths.len() > usize::from(face.number_of_glyphs()) {
            return Err(format!(
                "font {resource}: /W lists {} widths for {} glyphs",
                widths.len(),
                original.len()
            ));
        }
        let per_em = f32::from(face.units_per_em());
        for (cid, (gid, width)) in original.iter().zip(widths).enumerate() {
            let advance = face
                .glyph_hor_advance(ttf_parser::GlyphId(*gid))
                .map_or(0.0, f32::from)
                * 1000.0
                / per_em;
            if (advance - width).abs() > 0.01 {
                return Err(format!(
                    "font {resource}: /W gives glyph {cid} width {width} but the program \
                     advances {advance}"
                ));
            }
        }
        Ok(())
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
        // `/ActualText <FEFF..>` seen, waiting for its `BDC`; then the text
        // that replaces the glyphs shown until `EMC`.
        let mut announced: Option<String> = None;
        let mut replacement: Option<String> = None;
        let mut expect_actual = false;
        // Inside a `[ ... ] TJ` array: the text its glyph strings show. The
        // spacing numbers are dropped and the array reads as the one literal
        // and `Tj` a run without spacing adjustments would have been.
        let mut array: Option<String> = None;
        for line in content.lines() {
            let mut tokens = Vec::new();
            for token in line.split(' ') {
                if token == "[" {
                    array = Some(String::new());
                    continue;
                }
                if let Some(shown_in_array) = array.as_mut() {
                    if token == "]" {
                        let shown = array.take().unwrap_or_default();
                        tokens.push(format!("({})", escape_literal(&shown)));
                        continue;
                    }
                    if let Some(hex) = token
                        .strip_prefix('<')
                        .and_then(|t| t.strip_suffix('>'))
                        .filter(|t| !t.starts_with('<'))
                    {
                        let shown = decode_glyphs(hex, cmaps.get(font.as_str()), &font)?;
                        let shown = replacement.take().unwrap_or(shown);
                        run.push_str(&shown);
                        shown_in_array.push_str(&shown);
                    }
                    continue;
                }
                if token == "TJ" {
                    tokens.push("Tj".to_string());
                    continue;
                }
                if token == "/ActualText" {
                    expect_actual = true;
                } else if let Some(name) = token.strip_prefix('/') {
                    last_name = name.to_string();
                } else if token == "Tf" {
                    font.clone_from(&last_name);
                } else if token == "ET" && !run.is_empty() {
                    lines.push(std::mem::take(&mut run));
                } else if token == "BDC" {
                    replacement = announced.take();
                } else if token == "EMC" {
                    replacement = None;
                }
                let hex = token
                    .strip_prefix('<')
                    .and_then(|t| t.strip_suffix('>'))
                    .filter(|t| !t.starts_with('<'));
                match hex {
                    Some(hex) if expect_actual => {
                        expect_actual = false;
                        let actual = utf16_hex(hex.strip_prefix("FEFF").unwrap_or(hex))?;
                        tokens.push(format!("({})", escape_literal(&actual)));
                        announced = Some(actual);
                    }
                    Some(hex) => {
                        let shown = decode_glyphs(hex, cmaps.get(font.as_str()), &font)?;
                        let shown = replacement.take().unwrap_or(shown);
                        run.push_str(&shown);
                        tokens.push(format!("({})", escape_literal(&shown)));
                    }
                    None => tokens.push(token.to_string()),
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

/// The unsigned integer after `key` (the first match; `/Length` does not
/// match `/Length1`).
fn number_after(dict: &str, key: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(found) = dict[from..].find(key) {
        let start = from + found + key.len();
        let rest = &dict[start..];
        if rest.starts_with(' ') {
            return rest.split_whitespace().next()?.parse().ok();
        }
        from = start;
    }
    None
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

/// Characters for a hex string of two-byte glyph ids. `.notdef` (id 0) shows
/// as U+FFFD; any other id the font's `ToUnicode` map does not name is an
/// error, because text extraction would silently lose it.
fn decode_glyphs(
    hex: &str,
    cmap: Option<&HashMap<u16, String>>,
    font: &str,
) -> Result<String, String> {
    let mut out = String::new();
    for chunk in hex.as_bytes().chunks(4) {
        let gid = u16::from_str_radix(&text(chunk), 16).map_err(|e| e.to_string())?;
        match cmap.and_then(|m| m.get(&gid)) {
            Some(shown) => out.push_str(shown),
            None if gid == 0 => out.push('\u{FFFD}'),
            None => {
                return Err(format!(
                    "font /{font}: glyph {gid} is shown but ToUnicode does not name it"
                ))
            }
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
