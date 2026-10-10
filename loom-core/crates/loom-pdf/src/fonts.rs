//! The Inter faces Loom embeds in every PDF, their metrics, and the hook that
//! lets a later release supply characters Inter does not cover.
//!
//! Faces come from `loom-ui`'s bundled font directory (SIL OFL 1.1), so the PDF,
//! the editors and the exported text all measure with the same glyph advances.
//! The bytes are the ones `loom-fonts` embeds ([`loom_fonts::bundled_faces`]):
//! one copy per binary serves the font catalogue and the PDF writer.

use std::fmt::Debug;
use std::sync::OnceLock;

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

/// A parsed font with its metrics cached for fast per-character lookups.
#[derive(Clone)]
pub(crate) struct Font {
    pub(crate) name: &'static str,
    pub(crate) data: &'static [u8],
    face: Face<'static>,
    units_per_em: f32,
    /// Glyph id for ASCII 0..128 (`0` when the face has none).
    ascii_gid: [u16; 128],
    /// Advance in font units for each of those glyphs.
    ascii_advance: [u16; 128],
    notdef_advance: u16,
}

impl Font {
    /// Parse `data`; `None` when it is not a usable font.
    pub(crate) fn new(name: &'static str, data: &'static [u8]) -> Option<Self> {
        let face = Face::parse(data, 0).ok()?;
        let units_per_em = f32::from(face.units_per_em());
        if units_per_em <= 0.0 {
            return None;
        }
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
        let notdef_advance = advance(0);
        Some(Self {
            name,
            data,
            face,
            units_per_em,
            ascii_gid,
            ascii_advance,
            notdef_advance,
        })
    }

    /// The glyph this face draws for `ch`; `None` when its cmap has no entry.
    pub(crate) fn glyph(&self, ch: char) -> Option<u16> {
        if (ch as u32) < 128 {
            let gid = self.ascii_gid[ch as usize];
            return (gid != 0).then_some(gid);
        }
        self.face.glyph_index(ch).map(|g| g.0).filter(|g| *g != 0)
    }

    /// Horizontal advance of glyph `gid` in font units.
    pub(crate) fn advance_units(&self, gid: u16) -> u16 {
        if gid == 0 {
            return self.notdef_advance;
        }
        self.face
            .glyph_hor_advance(ttf_parser::GlyphId(gid))
            .unwrap_or(self.notdef_advance)
    }

    /// Advance of `ch` in font units, notdef when the face lacks the glyph.
    /// Characters that are not drawn (see [`crate::text::is_not_drawn`]) take
    /// no space, and a character drawn as its plain expansion (a ligature code
    /// point shown as `fi`, say) takes the expansion's width.
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

    /// The plain characters `ch` is drawn as when this face has no glyph for
    /// it, provided the face covers all of them.
    pub(crate) fn expansion(&self, ch: char) -> Option<([char; 3], usize)> {
        let (chars, len) = compat_expansion(ch)?;
        chars[..len]
            .iter()
            .all(|part| self.glyph(*part).is_some())
            .then_some((chars, len))
    }

    /// Whether this face draws `ch` exactly, so a width measured from it is
    /// the width that is drawn: a glyph, an expansion, or no drawing at all.
    pub(crate) fn measures_exactly(&self, ch: char) -> bool {
        is_not_drawn(ch) || self.glyph(ch).is_some() || self.expansion(ch).is_some()
    }

    pub(crate) fn units_per_em(&self) -> f32 {
        self.units_per_em
    }

    /// Scale a value in font units to the 1000-unit glyph space PDF uses.
    pub(crate) fn to_1000(&self, units: f32) -> f32 {
        units * 1000.0 / self.units_per_em
    }

    pub(crate) fn face(&self) -> &Face<'static> {
        &self.face
    }
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
    let font = Font::new(name, face.data)
        .ok_or_else(|| format!("fallback font {name:?} is not a usable TrueType font"))?;
    let tables = font.face().tables();
    if face.data.starts_with(b"OTTO") || tables.glyf.is_none() || tables.cff.is_some() {
        return Err(format!(
            "fallback font {name:?} has CFF/PostScript outlines; only TrueType (glyf) \
             faces can be embedded"
        ));
    }
    Ok(font)
}

/// Whether [`crate::text_width_pt`] measures `ch` exactly as it is drawn: the
/// bundled Inter faces have its glyph or a plain expansion, or it is not drawn.
pub(crate) fn inter_measures_exactly(ch: char) -> bool {
    inter(0).measures_exactly(ch)
}
