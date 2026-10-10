//! Plain descriptions of installed font faces.

use std::path::PathBuf;

/// Where a face's bytes come from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FaceSource {
    /// A face compiled into the binary (the Inter faces `loom-ui` ships).
    Bundled(&'static str),
    /// A font file on disk.
    File(PathBuf),
}

/// What the font's `OS/2.fsType` field allows when embedding it in a document
/// such as a PDF. Subsetting has its own bit, see [`FaceInfo::subsetting_allowed`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmbeddingPermission {
    /// No restriction (fsType 0).
    Installable,
    /// May be embedded for editing (bit 3).
    Editable,
    /// May be embedded for preview and printing only (bit 2).
    PreviewAndPrint,
    /// Must not be embedded (bit 1).
    Restricted,
}

impl EmbeddingPermission {
    pub(crate) fn from_fs_type(fs_type: u16) -> Self {
        if fs_type & 0x0002 != 0 {
            Self::Restricted
        } else if fs_type & 0x0004 != 0 {
            Self::PreviewAndPrint
        } else if fs_type & 0x0008 != 0 {
            Self::Editable
        } else {
            Self::Installable
        }
    }

    /// True when a producer such as the PDF exporter may embed the font.
    pub fn allows_embedding(self) -> bool {
        self != Self::Restricted
    }

    pub(crate) fn code(self) -> u8 {
        match self {
            Self::Installable => 0,
            Self::Editable => 1,
            Self::PreviewAndPrint => 2,
            Self::Restricted => 3,
        }
    }

    pub(crate) fn from_code(code: u8) -> Self {
        match code {
            1 => Self::Editable,
            2 => Self::PreviewAndPrint,
            3 => Self::Restricted,
            _ => Self::Installable,
        }
    }
}

/// Stable-per-catalogue handle of one face. It indexes the catalogue that
/// produced it and means nothing to another catalogue or a later scan; persist
/// family, weight and italic instead (see [`crate::FontRef`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FaceId(pub(crate) u32);

impl FaceId {
    /// The position of the face in [`crate::FontCatalog::faces_in_order`].
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// One variation axis of a variable font (an `fvar` axis record). Values are
/// user-space coordinates such as 100..900 for `wght`.
///
/// Values come from 16.16 fixed-point fields, so they are never NaN and the
/// `Eq` implementation is sound.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VariationAxis {
    /// Axis tag such as `*b"wght"`.
    pub tag: [u8; 4],
    /// Lowest coordinate.
    pub min: f32,
    /// Coordinate of the default instance.
    pub default: f32,
    /// Highest coordinate.
    pub max: f32,
}

impl Eq for VariationAxis {}

/// A named instance of a variable font ("Bold", "Light Italic").
///
/// Like [`VariationAxis`], the coordinates are never NaN.
#[derive(Clone, Debug, PartialEq)]
pub struct NamedInstance {
    /// The instance's style name, empty when the name record is missing.
    pub style: String,
    /// One coordinate per axis, in the order of [`FaceInfo::axes`].
    pub coords: Vec<f32>,
}

impl Eq for NamedInstance {}

/// Everything the catalogue knows about one face without loading its glyphs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FaceInfo {
    /// Typographic family ("Inter"), falling back to the legacy family name.
    pub family: String,
    /// The legacy (name ID 1) family, the style-linked group name such as
    /// "Inter Medium". Equal to `family` for fonts without name ID 16.
    pub legacy_family: String,
    /// Further family names (name IDs 1 and 16 in other languages and on other
    /// platforms) that also find this face. At most [`MAX_ALIASES`] are kept.
    pub aliases: Vec<String>,
    /// Style name, for example "Bold Italic".
    pub style: String,
    /// Full font name.
    pub full_name: String,
    /// PostScript name, the identity used to remove duplicates.
    pub postscript_name: String,
    /// CSS-style weight, 1 to 1000 (400 regular, 700 bold). For a variable
    /// font this is the weight of the default instance (`OS/2.usWeightClass`);
    /// the weights of its named instances are in [`FaceInfo::instances`].
    pub weight: u16,
    /// `OS/2.usWidthClass`, 1 (ultra-condensed) to 9 (ultra-expanded), 5 normal.
    pub width: u16,
    /// True for italic and oblique faces.
    pub italic: bool,
    /// True when the font declares itself fixed pitch.
    pub monospace: bool,
    /// True for fonts with an `fvar` table.
    pub variable: bool,
    /// Variation axes in `fvar` order; empty for a static font.
    pub axes: Vec<VariationAxis>,
    /// Named instances in `fvar` order; empty for a static font.
    pub instances: Vec<NamedInstance>,
    /// Embedding permission from `OS/2.fsType`.
    pub embedding: EmbeddingPermission,
    /// False when `fsType` forbids subsetting (bit 8).
    pub subsetting_allowed: bool,
    /// Where the bytes live.
    pub source: FaceSource,
    /// Font index inside a `.ttc` / `.otc`; 0 for single fonts.
    pub index: u32,
}

/// Most alias names kept for one face, so a hostile `name` table cannot make a
/// catalogue entry unbounded.
pub const MAX_ALIASES: usize = 32;

impl FaceInfo {
    /// Key used to remove the same face found twice.
    pub(crate) fn dedupe_key(&self) -> (String, String, u16, bool, u16) {
        let identity = if self.postscript_name.is_empty() {
            &self.full_name
        } else {
            &self.postscript_name
        };
        (
            identity.to_lowercase(),
            self.family.to_lowercase(),
            self.weight,
            self.italic,
            self.width,
        )
    }

    /// The axis with `tag`, if the face is a variable font that has one.
    pub fn axis(&self, tag: [u8; 4]) -> Option<&VariationAxis> {
        self.axes.iter().find(|axis| axis.tag == tag)
    }
}

/// Lower-cases and collapses whitespace so "  Segoe  UI " matches "segoe ui".
pub(crate) fn normalize_family(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for word in name.split_whitespace() {
        if !out.is_empty() {
            out.push(' ');
        }
        out.extend(word.chars().flat_map(char::to_lowercase));
    }
    out
}
