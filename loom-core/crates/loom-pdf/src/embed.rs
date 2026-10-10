//! Embedded, subset TrueType fonts as PDF `Type0` / `CIDFontType2` objects.
//!
//! Text is shown with the *new* glyph ids the subsetter assigns, used directly
//! as CIDs under `Identity-H`, so no `CIDToGIDMap` is needed. A `ToUnicode` CMap
//! lets viewers copy and search the text, and `/W` carries the exact advances.

use std::fmt::Write as _;
use std::io::Write as _;

use subsetter::GlyphRemapper;

use crate::fonts::Font;

/// One font program used by a document, with the glyphs drawn from it.
#[derive(Clone)]
pub(crate) struct FaceSlot {
    /// Page resource name, for example `F1`.
    pub(crate) resource: String,
    font: Font,
    remapper: GlyphRemapper,
    /// The character each new glyph id was first drawn for (`None` for
    /// `.notdef`, id 0). Index = new glyph id.
    chars: Vec<Option<char>>,
}

impl FaceSlot {
    pub(crate) fn new(resource: String, font: Font) -> Self {
        Self {
            resource,
            font,
            remapper: GlyphRemapper::new(),
            chars: vec![None],
        }
    }

    pub(crate) fn font_name(&self) -> &str {
        self.font.name
    }

    /// The glyph id to write for `ch`, registering it for the subset. A
    /// character the face lacks shows its `.notdef` glyph (id 0).
    pub(crate) fn encode(&mut self, ch: char) -> u16 {
        let original = self.font.glyph(ch).unwrap_or(0);
        let new = self.remapper.remap(original);
        if usize::from(new) == self.chars.len() {
            self.chars.push(Some(ch));
        }
        new
    }

    /// The subset font program, or `None` if the subsetter rejects the font.
    pub(crate) fn subset(&self) -> Option<Vec<u8>> {
        subsetter::subset(self.font.data, 0, &self.remapper).ok()
    }

    /// The objects of this face: `Type0`, `CIDFontType2`, `FontDescriptor`,
    /// `FontFile2`, `ToUnicode`, plus a `CIDToGIDMap` stream when `subset` is
    /// `None` (the whole font is embedded and the subset ids map back to its
    /// own). `first` is the object number of the first object.
    pub(crate) fn objects_with(&self, first: usize, subset: Option<Vec<u8>>) -> Vec<Vec<u8>> {
        let tag = self.subset_tag();
        let base_font = format!("{tag}+{}", self.font.name);
        let originals: Vec<u16> = self.remapper.remapped_gids().collect();
        let widths: Vec<String> = originals
            .iter()
            .map(|gid| trim_number(self.font.to_1000(f32::from(self.font.advance_units(*gid)))))
            .collect();
        let whole_font = subset.is_none();
        let program = subset.unwrap_or_else(|| self.font.data.to_vec());
        let packed = deflate(&program);

        let (descendant, descriptor, file, to_unicode) =
            (first + 1, first + 2, first + 3, first + 4);
        let mut objects = Vec::with_capacity(5);
        objects.push(
            format!(
                "<< /Type /Font /Subtype /Type0 /BaseFont /{base_font} /Encoding /Identity-H \
                 /DescendantFonts [{descendant} 0 R] /ToUnicode {to_unicode} 0 R >>"
            )
            .into_bytes(),
        );
        let map_ref = to_unicode + 1;
        objects.push(
            format!(
                "<< /Type /Font /Subtype /CIDFontType2 /BaseFont /{base_font} \
                 /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> \
                 /FontDescriptor {descriptor} 0 R /DW 1000 /W [0 [{}]] /CIDToGIDMap {} >>",
                widths.join(" "),
                if whole_font {
                    format!("{map_ref} 0 R")
                } else {
                    "/Identity".to_string()
                }
            )
            .into_bytes(),
        );
        objects.push(self.descriptor(&base_font, file).into_bytes());
        objects.push(stream_object(
            &format!("/Filter /FlateDecode /Length1 {}", program.len()),
            &packed,
        ));
        objects.push(stream_object("", self.to_unicode_cmap().as_bytes()));
        if whole_font {
            let map: Vec<u8> = originals.iter().flat_map(|g| g.to_be_bytes()).collect();
            objects.push(stream_object("", &map));
        }
        objects
    }

    fn descriptor(&self, base_font: &str, file: usize) -> String {
        let face = self.font.face();
        let scale = |v: i16| trim_number(self.font.to_1000(f32::from(v)));
        let bbox = face.global_bounding_box();
        let ascent = face.ascender();
        let cap = face.capital_height().unwrap_or(ascent);
        let angle = face.italic_angle();
        let flags = 32 | if angle != 0.0 { 64 } else { 0 };
        let stem = if face.is_bold() { 130 } else { 80 };
        format!(
            "<< /Type /FontDescriptor /FontName /{base_font} /Flags {flags} \
             /FontBBox [{} {} {} {}] /ItalicAngle {} /Ascent {} /Descent {} \
             /CapHeight {} /StemV {stem} /FontFile2 {file} 0 R >>",
            scale(bbox.x_min),
            scale(bbox.y_min),
            scale(bbox.x_max),
            scale(bbox.y_max),
            trim_number(angle),
            scale(ascent),
            scale(face.descender()),
            scale(cap),
        )
    }

    /// The `ToUnicode` CMap: new glyph id to the character it was drawn for.
    fn to_unicode_cmap(&self) -> String {
        let pairs: Vec<(usize, char)> = self
            .chars
            .iter()
            .enumerate()
            .filter_map(|(gid, ch)| ch.map(|c| (gid, c)))
            .collect();
        let mut cmap = String::from(
            "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
             /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
             /CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n\
             1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
        );
        for block in pairs.chunks(100) {
            let _ = writeln!(cmap, "{} beginbfchar", block.len());
            for (gid, ch) in block {
                let mut units = [0u16; 2];
                let hex: String = ch
                    .encode_utf16(&mut units)
                    .iter()
                    .map(|unit| format!("{unit:04X}"))
                    .collect();
                let _ = writeln!(cmap, "<{gid:04X}> <{hex}>");
            }
            cmap.push_str("endbfchar\n");
        }
        cmap.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
        cmap
    }

    /// A deterministic six-letter subset prefix derived from the face and glyphs.
    fn subset_tag(&self) -> String {
        let mut hash: u32 = 0x811c_9dc5;
        let mut feed = |byte: u8| {
            hash ^= u32::from(byte);
            hash = hash.wrapping_mul(0x0100_0193);
        };
        self.font.name.bytes().for_each(&mut feed);
        for gid in self.remapper.remapped_gids() {
            gid.to_be_bytes().into_iter().for_each(&mut feed);
        }
        (0..6)
            .map(|i| char::from(b'A' + ((hash >> (i * 5)) % 26) as u8))
            .collect()
    }
}

/// A stream object with `extra` dictionary entries and a correct `/Length`.
pub(crate) fn stream_object(extra: &str, data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + 64);
    out.extend_from_slice(format!("<< {extra} /Length {} >>\nstream\n", data.len()).as_bytes());
    out.extend_from_slice(data);
    out.extend_from_slice(b"\nendstream");
    out
}

/// zlib-compress `data` (the `FlateDecode` filter). Deterministic.
pub(crate) fn deflate(data: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::new(6));
    // Writing to a Vec cannot fail.
    let _ = encoder.write_all(data);
    encoder.finish().unwrap_or_default()
}

/// A PDF number with at most three decimals and no trailing zeros.
pub(crate) fn trim_number(value: f32) -> String {
    let text = format!("{value:.3}");
    let trimmed = text.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() || trimmed == "-0" {
        "0".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fonts;

    fn slot_with(text: &str) -> FaceSlot {
        let mut slot = FaceSlot::new("F1".into(), fonts::inter(0).clone());
        for ch in text.chars() {
            slot.encode(ch);
        }
        slot
    }

    #[test]
    fn a_failed_subset_embeds_the_whole_font_with_an_explicit_gid_map() {
        let slot = slot_with("Ab");
        let objects = slot.objects_with(10, None);
        assert_eq!(objects.len(), 6, "extra CIDToGIDMap stream");
        let cid = String::from_utf8_lossy(&objects[1]).into_owned();
        assert!(cid.contains("/CIDToGIDMap 15 0 R"), "{cid}");
        // Glyph 0, then 'A', then 'b' in the original font.
        let map = &objects[5];
        let body = &map[map.windows(7).position(|w| w == b"stream\n").unwrap() + 7..];
        assert_eq!(body[..2], [0, 0]);
        let a = fonts::inter(0).glyph('A').unwrap().to_be_bytes();
        assert_eq!(body[2..4], a);
    }

    #[test]
    fn numbers_are_trimmed() {
        assert_eq!(trim_number(281.25), "281.25");
        assert_eq!(trim_number(1000.0), "1000");
        assert_eq!(trim_number(-0.0004), "0");
        assert_eq!(trim_number(-9.4), "-9.4");
    }

    #[test]
    fn subset_tag_is_stable_and_alphabetic() {
        let tag = slot_with("Hello").subset_tag();
        assert_eq!(tag, slot_with("Hello").subset_tag());
        assert_eq!(tag.len(), 6);
        assert!(tag.bytes().all(|b| b.is_ascii_uppercase()));
        assert_ne!(tag, slot_with("Hellp").subset_tag());
    }
}
