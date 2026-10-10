//! The Inter faces Loom embeds in every PDF, their metrics, and the hook that
//! lets a later release supply characters Inter does not cover.
//!
//! Faces come from `loom-ui`'s bundled font directory (SIL OFL 1.1), so the PDF,
//! the editors and the exported text all measure with the same glyph advances.

use std::fmt::Debug;
use std::sync::OnceLock;

use ttf_parser::Face;

const REGULAR: &[u8] = include_bytes!("../../loom-ui/ui/fonts/Inter-Regular.ttf");
const BOLD: &[u8] = include_bytes!("../../loom-ui/ui/fonts/Inter-Bold.ttf");
const ITALIC: &[u8] = include_bytes!("../../loom-ui/ui/fonts/Inter-Italic.ttf");
const BOLD_ITALIC: &[u8] = include_bytes!("../../loom-ui/ui/fonts/Inter-BoldItalic.ttf");

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
/// `data` is a TrueType or OpenType file that lives for the whole process
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
/// distinct character and face style per document. Returning `None` keeps the
/// notdef box. Loom bundles no fallback font today; this is the seam where a
/// CJK or symbol font plugs in later.
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
    /// Line breaks and other control characters take no space.
    pub(crate) fn char_advance_units(&self, ch: char) -> u16 {
        if is_invisible_control(ch) {
            return 0;
        }
        if (ch as u32) < 128 {
            return self.ascii_advance[ch as usize];
        }
        self.glyph(ch)
            .map_or(self.notdef_advance, |gid| self.advance_units(gid))
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

/// Characters that must never reach the page: they would draw as notdef boxes.
pub(crate) fn is_invisible_control(ch: char) -> bool {
    matches!(ch, '\u{0}'..='\u{1F}' | '\u{7F}'..='\u{9F}')
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
        let load = |i: usize, data: &'static [u8]| {
            Font::new(INTER_NAMES[i], data).expect("bundled Inter face must parse")
        };
        [
            load(0, REGULAR),
            load(1, BOLD),
            load(2, ITALIC),
            load(3, BOLD_ITALIC),
        ]
    })[index]
}

/// Whether `name` is one of the bundled Inter faces.
pub(crate) fn is_bundled(name: &str) -> bool {
    INTER_NAMES.contains(&name)
}
