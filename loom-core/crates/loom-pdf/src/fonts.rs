//! The Inter faces Loom embeds in every PDF, their metrics, and the hook that
//! lets a later release supply characters Inter does not cover.
//!
//! Faces come from `loom-ui`'s bundled font directory (SIL OFL 1.1), so the PDF,
//! the editors and the exported text all measure with the same glyph advances.
//! The bytes are the ones `loom-fonts` embeds ([`loom_fonts::bundled_faces`]):
//! one copy per binary serves the font catalogue and the PDF writer.

use std::fmt::Debug;
use std::sync::{Arc, OnceLock};

use ttf_parser::Face;

use crate::text::{compat_expansion, is_not_drawn};

/// The bundled file of each face, indexed like [`inter`] (regular, bold,
/// italic, bold italic).
const INTER_FILES: [&str; 4] = [
    "Inter-Regular.ttf",
    "Inter-Bold.ttf",
    "Inter-Italic.ttf",
    "Inter-BoldItalic.ttf",
];

/// The bytes of a bundled face. `loom-fonts` is a dependency with its
/// `bundled-inter` feature on, so a missing face is a build error in this
/// crate's manifest, not a runtime condition.
fn bundled_bytes(file: &str) -> &'static [u8] {
    loom_fonts::bundled_faces()
        .iter()
        .find(|(name, _)| *name == file)
        .map(|(_, bytes)| *bytes)
        .unwrap_or_else(|| panic!("loom-fonts does not bundle {file}"))
}

/// PostScript names of the bundled faces, indexed like [`inter`].
const INTER_NAMES: [&str; 4] = [
    "Inter-Regular",
    "Inter-Bold",
    "Inter-Italic",
    "Inter-BoldItalic",
];

/// A font program supplied for characters the bundled Inter faces lack.
///
/// `name` is a unique PostScript-style name (it keys the embedded copy);
/// `data` is a TrueType file that lives for the whole process
/// (`include_bytes!`, or one deliberate `Box::leak` at startup).
#[derive(Debug, Clone, Copy)]
pub struct FallbackFont {
    /// Unique face name, for example `NotoSansCJKjp-Regular`.
    pub name: &'static str,
    /// The font file.
    pub data: &'static [u8],
}

/// Chooses a font for a character that no bundled Inter face can show.
///
/// The PDF writer asks only after Inter's cmap has no glyph for `ch`, once per
/// distinct character and face style per document (the answer is cached).
/// Returning `None` keeps the notdef box. Loom bundles no fallback font today;
/// this is the seam where a CJK or symbol font plugs in later.
///
/// A returned face is accepted only if it is a single TrueType (`glyf`) font
/// program: font collections (`.ttc`) and CFF/PostScript outlines are refused,
/// as are names that collide with a bundled face or with another font program
/// already used by the document. Each refusal is recorded in
/// [`crate::PdfDocument::fallback_problems`] and the character shows `.notdef`.
pub trait FontFallback: Debug + Send + Sync {
    /// A face covering `ch` in the requested style, if one is available.
    fn font_for(&self, ch: char, bold: bool, italic: bool) -> Option<FallbackFont>;
}

/// Font bytes either remain borrowed from a binary or are shared by Arc.
#[derive(Clone)]
enum FontBytes {
    Static(&'static [u8]),
    Owned(Arc<[u8]>),
}

impl FontBytes {
    fn as_slice(&self) -> &[u8] {
        match self {
            Self::Static(bytes) => bytes,
            Self::Owned(bytes) => bytes,
        }
    }
}

/// A parsed font with cached metrics. Owned faces reparse a borrowed Face on
/// demand so the Arc owns the bytes without a self-referential lifetime.
#[derive(Clone)]
pub(crate) struct Font {
    pub(crate) name: String,
    data: FontBytes,
    index: u32,
    face_static: Option<Face<'static>>,
    units_per_em: f32,
    number_of_glyphs: u16,
    subsetting_allowed: bool,
    /// Glyph id for ASCII 0..128 (`0` when the face has none).
    ascii_gid: [u16; 128],
    /// Advance in font units for each of those glyphs.
    ascii_advance: [u16; 128],
    notdef_advance: u16,
}

impl Font {
    /// Parse a borrowed binary face; `None` when it is not usable.
    pub(crate) fn new(name: &'static str, data: &'static [u8]) -> Option<Self> {
        Self::from_static(name, data).ok()
    }

    fn from_static(name: &'static str, data: &'static [u8]) -> Result<Self, String> {
        if !valid_postscript_name(name) {
            return Err("font name is not a safe PostScript name".into());
        }
        let face =
            Face::parse(data, 0).map_err(|error| format!("font face does not parse: {error}"))?;
        let subsetting_allowed = validate_face(data, 0, &face)?;
        let units_per_em = f32::from(face.units_per_em());
        if units_per_em <= 0.0 {
            return Err("font face has zero units per em".into());
        }
        let (ascii_gid, ascii_advance, notdef_advance) = cached_metrics(&face);
        let number_of_glyphs = face.number_of_glyphs();
        Ok(Self {
            name: name.to_owned(),
            data: FontBytes::Static(data),
            index: 0,
            face_static: Some(face),
            units_per_em,
            number_of_glyphs,
            subsetting_allowed,
            ascii_gid,
            ascii_advance,
            notdef_advance,
        })
    }

    /// Parse and own an indexed font face without leaking or copying its bytes.
    pub(crate) fn from_owned(data: Arc<[u8]>, index: u32) -> Result<Self, String> {
        validate_font_file_size(data.len())?;
        let face = Face::parse(&data, index)
            .map_err(|error| format!("font face {index} does not parse: {error}"))?;
        let subsetting_allowed = validate_face(&data, index, &face)?;
        let name = postscript_name(&face)
            .ok_or_else(|| "font face has no valid PostScript name".to_string())?;
        let units_per_em = f32::from(face.units_per_em());
        if units_per_em <= 0.0 {
            return Err("font face has zero units per em".into());
        }
        let (ascii_gid, ascii_advance, notdef_advance) = cached_metrics(&face);
        let number_of_glyphs = face.number_of_glyphs();
        Ok(Self {
            name,
            data: FontBytes::Owned(data),
            index,
            face_static: None,
            units_per_em,
            number_of_glyphs,
            subsetting_allowed,
            ascii_gid,
            ascii_advance,
            notdef_advance,
        })
    }

    pub(crate) fn data(&self) -> &[u8] {
        self.data.as_slice()
    }

    pub(crate) fn index(&self) -> u32 {
        self.index
    }

    pub(crate) fn subsetting_allowed(&self) -> bool {
        self.subsetting_allowed
    }

    pub(crate) fn number_of_glyphs(&self) -> u16 {
        self.number_of_glyphs
    }

    pub(crate) fn same_program(&self, other: &Self) -> bool {
        self.index == other.index && self.data() == other.data()
    }

    pub(crate) fn same_program_bytes(&self, data: &[u8], index: u32) -> bool {
        self.index == index && self.data() == data
    }

    fn face(&self) -> Face<'_> {
        match &self.face_static {
            Some(face) => face.clone(),
            None => Face::parse(self.data(), self.index).expect("validated owned font face"),
        }
    }

    /// The glyph this face draws for `ch`; `None` when its cmap has no entry.
    pub(crate) fn glyph(&self, ch: char) -> Option<u16> {
        if (ch as u32) < 128 {
            let gid = self.ascii_gid[ch as usize];
            return (gid != 0).then_some(gid);
        }
        self.face().glyph_index(ch).map(|g| g.0).filter(|g| *g != 0)
    }

    /// Horizontal advance of glyph `gid` in font units.
    pub(crate) fn advance_units(&self, gid: u16) -> u16 {
        if gid == 0 {
            return self.notdef_advance;
        }
        self.face()
            .glyph_hor_advance(ttf_parser::GlyphId(gid))
            .unwrap_or(self.notdef_advance)
    }

    /// Advance of `ch` in font units, notdef when the face lacks the glyph.
    pub(crate) fn char_advance_units(&self, ch: char) -> u32 {
        if is_not_drawn(ch) {
            return 0;
        }
        if (ch as u32) < 128 {
            return u32::from(self.ascii_advance[ch as usize]);
        }
        if let Some(gid) = self.glyph(ch) {
            return u32::from(self.advance_units(gid));
        }
        match self.expansion(ch) {
            Some((chars, len)) => chars[..len]
                .iter()
                .map(|part| u32::from(self.advance_units(self.glyph(*part).unwrap_or(0))))
                .sum(),
            None => u32::from(self.notdef_advance),
        }
    }

    pub(crate) fn expansion(&self, ch: char) -> Option<([char; 3], usize)> {
        let (chars, len) = compat_expansion(ch)?;
        chars[..len]
            .iter()
            .all(|part| self.glyph(*part).is_some())
            .then_some((chars, len))
    }

    pub(crate) fn measures_exactly(&self, ch: char) -> bool {
        is_not_drawn(ch) || self.glyph(ch).is_some() || self.expansion(ch).is_some()
    }

    pub(crate) fn units_per_em(&self) -> f32 {
        self.units_per_em
    }

    pub(crate) fn to_1000(&self, units: f32) -> f32 {
        units * 1000.0 / self.units_per_em
    }

    pub(crate) fn face_for_embedding(&self) -> Face<'_> {
        self.face()
    }
}

/// Refuse an oversized font before callers copy borrowed bytes into owned storage.
pub(crate) fn validate_font_file_size(byte_len: usize) -> Result<(), String> {
    if u64::try_from(byte_len).unwrap_or(u64::MAX) > loom_fonts::MAX_FONT_FILE_BYTES {
        return Err(format!(
            "font file is {byte_len} bytes; limit is {} bytes",
            loom_fonts::MAX_FONT_FILE_BYTES
        ));
    }
    Ok(())
}

fn cached_metrics(face: &Face<'_>) -> ([u16; 128], [u16; 128], u16) {
    let advance = |gid: u16| {
        face.glyph_hor_advance(ttf_parser::GlyphId(gid))
            .unwrap_or(0)
    };
    let mut ascii_gid = [0u16; 128];
    let mut ascii_advance = [0u16; 128];
    for code in 0..128u8 {
        let gid = face.glyph_index(char::from(code)).map_or(0, |g| g.0);
        ascii_gid[usize::from(code)] = gid;
        ascii_advance[usize::from(code)] = advance(gid);
    }
    (ascii_gid, ascii_advance, advance(0))
}

fn validate_face(data: &[u8], index: u32, face: &Face<'_>) -> Result<bool, String> {
    let tables = face.tables();
    if data.starts_with(b"OTTO") || tables.glyf.is_none() || tables.cff.is_some() {
        return Err(
            "CFF/PostScript font outlines are unsupported; TrueType glyf outlines are required"
                .into(),
        );
    }
    if face
        .raw_face()
        .table(ttf_parser::Tag::from_bytes(b"CFF2"))
        .is_some()
    {
        return Err("CFF2 font outlines are unsupported".into());
    }
    if face
        .raw_face()
        .table(ttf_parser::Tag::from_bytes(b"fvar"))
        .is_some()
    {
        return Err(
            "variable font instances are unsupported; select a static instance first".into(),
        );
    }
    let raw_os2 = face.raw_face().table(ttf_parser::Tag::from_bytes(b"OS/2"));
    let subsetting_allowed = match (raw_os2, tables.os2) {
        (None, None) => true,
        (Some(_), None) => {
            return Err("font has a malformed OS/2 embedding-permission table".into())
        }
        (Some(_), Some(os2)) => {
            if os2.permissions() == Some(ttf_parser::os2::Permissions::Restricted) {
                return Err("font embedding is restricted by OS/2 fsType".into());
            }
            if os2.permissions().is_none() {
                return Err("font has invalid OS/2 embedding permissions".into());
            }
            if !os2.is_outline_embedding_allowed() {
                return Err("font permits bitmap embedding only, not TrueType outlines".into());
            }
            os2.is_subsetting_allowed()
        }
        (None, Some(_)) => return Err("font OS/2 table was not present in the source bytes".into()),
    };
    if data.starts_with(b"ttcf") && !subsetting_allowed {
        return Err(format!(
            "font collection face {index} forbids subsetting and cannot be embedded as a standalone FontFile2"
        ));
    }
    Ok(subsetting_allowed)
}

fn postscript_name(face: &Face<'_>) -> Option<String> {
    face.names()
        .into_iter()
        .find(|name| name.name_id == ttf_parser::name_id::POST_SCRIPT_NAME && name.is_unicode())
        .and_then(|name| name.to_string())
        .filter(|name| valid_postscript_name(name))
}

fn valid_postscript_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 127
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

/// Index of the Inter face a style selects (0 regular, 1 bold, 2 italic,
/// 3 bold italic); also the face's `/F1`..`/F4` resource number minus one.
pub(crate) fn inter_index(bold: bool, italic: bool) -> usize {
    usize::from(bold) + 2 * usize::from(italic)
}

/// The bundled face at `index` (see [`inter_index`]).
pub(crate) fn inter(index: usize) -> &'static Font {
    static FACES: OnceLock<[Font; 4]> = OnceLock::new();
    &FACES.get_or_init(|| {
        let load = |i: usize| {
            Font::new(INTER_NAMES[i], bundled_bytes(INTER_FILES[i]))
                .expect("bundled Inter face must parse")
        };
        [load(0), load(1), load(2), load(3)]
    })[index]
}

/// Whether `name` is one of the bundled Inter faces.
pub(crate) fn is_bundled(name: &str) -> bool {
    INTER_NAMES.contains(&name)
}

/// Parses a fallback face, refusing what the embedder cannot write correctly:
/// bundled names, font collections and CFF/PostScript outlines.
pub(crate) fn validate_fallback(face: FallbackFont) -> Result<Font, String> {
    let name = face.name;
    if name.is_empty() || name.contains(char::is_whitespace) || name.contains('/') {
        return Err(format!(
            "fallback font name {name:?} is not a PostScript name"
        ));
    }
    if is_bundled(name) {
        return Err(format!(
            "fallback font {name:?} has the name of a bundled Inter face"
        ));
    }
    if face.data.starts_with(b"ttcf") {
        return Err(format!(
            "fallback font {name:?} is a font collection (.ttc); supply one TrueType face"
        ));
    }
    Font::from_static(name, face.data)
        .map_err(|reason| format!("fallback font {name:?} is not a usable TrueType font: {reason}"))
}

/// Whether [`crate::text_width_pt`] measures `ch` exactly as it is drawn: the
/// bundled Inter faces have its glyph or a plain expansion, or it is not drawn.
pub(crate) fn inter_measures_exactly(ch: char) -> bool {
    inter(0).measures_exactly(ch)
}
