//! A loaded face: its bytes plus the shaping and metric queries on them.

use crate::shaped::{Shaped, ShapedGlyph};
use harfrust::{Direction, Feature, Language, Script, ShaperData, Tag, UnicodeBuffer};
use skrifa::instance::{LocationRef, Size};
use skrifa::{FontRef, MetadataProvider};
use std::fmt;
use std::sync::Arc;

/// Why a face could not be loaded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FontError {
    /// The catalogue contains no faces at all (bundled faces disabled and
    /// nothing found on disk).
    EmptyCatalog,
    /// The file could not be read.
    Io(String),
    /// The bytes are not a usable font.
    Parse(String),
    /// The file is larger than [`MAX_FONT_FILE_BYTES`].
    TooLarge,
}

/// Largest font file [`crate::FontCatalog::load`] reads (128 MiB). Real CJK
/// collections are well under this; anything bigger is refused rather than
/// read into memory.
pub const MAX_FONT_FILE_BYTES: u64 = 128 << 20;

impl fmt::Display for FontError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyCatalog => f.write_str("no fonts are available"),
            Self::Io(message) => write!(f, "font file could not be read: {message}"),
            Self::Parse(message) => write!(f, "font data is not valid: {message}"),
            Self::TooLarge => f.write_str("font file is larger than the supported limit"),
        }
    }
}

impl std::error::Error for FontError {}

/// Paragraph direction handed to the shaper.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextDirection {
    /// Guess from the first strong character.
    #[default]
    Auto,
    /// Left to right.
    LeftToRight,
    /// Right to left.
    RightToLeft,
}

/// One coordinate of a variable font's design space, in user units
/// (`wght` 700, `wdth` 87.5).
///
/// Catalogue-produced values are never NaN, which is why `Eq` is implemented.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Variation {
    /// Axis tag such as `*b"wght"`.
    pub tag: [u8; 4],
    /// The coordinate; clamped to the axis range when applied.
    pub value: f32,
}

impl Eq for Variation {}

/// Knobs for one shaping call.
#[derive(Clone, Debug, PartialEq)]
pub struct ShapeSettings {
    /// Variable-font coordinates to apply; empty means the default instance.
    /// [`crate::FontRef::chain_setup`] says which to use for a resolved font.
    pub variations: Vec<Variation>,
    /// Run direction. Callers that segment with [`crate::segment`] pass the
    /// run's direction; `Auto` suits a single-direction string.
    pub direction: TextDirection,
    /// ISO 15924 script tag such as `*b"Latn"`; `None` guesses from the text.
    pub script: Option<[u8; 4]>,
    /// BCP 47 language such as `"tr"`; affects locale-specific forms.
    pub language: Option<String>,
    /// Apply pair kerning (`kern`). On by default.
    pub kerning: bool,
    /// Apply standard ligatures (`liga`, `clig`). On by default.
    pub ligatures: bool,
}

impl Default for ShapeSettings {
    fn default() -> Self {
        Self {
            variations: Vec::new(),
            direction: TextDirection::Auto,
            script: None,
            language: None,
            kerning: true,
            ligatures: true,
        }
    }
}

/// Where a text decoration (underline, strikeout) sits and how thick it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Decoration {
    /// Distance from the baseline to the top of the line; positive is above
    /// the baseline, so an underline's offset is usually negative.
    pub offset: f32,
    /// Stroke thickness.
    pub thickness: f32,
}

/// Typographic line metrics at a size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LineMetrics {
    /// Distance from the baseline up to the top of a line (positive).
    pub ascent: f32,
    /// Distance from the baseline down to the bottom of a line (positive).
    pub descent: f32,
    /// Extra space between lines recommended by the font.
    pub line_gap: f32,
    /// `ascent + descent + line_gap`, the font's natural line pitch.
    pub line_height: f32,
    /// Height of capital letters, when the font records it.
    pub cap_height: Option<f32>,
    /// Height of lower-case x, when the font records it.
    pub x_height: Option<f32>,
    /// Underline position and thickness from `post`, when recorded.
    pub underline: Option<Decoration>,
    /// Strikeout position and thickness from `OS/2`, when recorded.
    pub strikeout: Option<Decoration>,
}

#[derive(Clone)]
enum Bytes {
    Static(&'static [u8]),
    Shared(Arc<[u8]>),
}

impl Bytes {
    fn as_slice(&self) -> &[u8] {
        match self {
            Self::Static(bytes) => bytes,
            Self::Shared(bytes) => bytes,
        }
    }
}

/// A parsed face ready to shape and measure. Cheap to share; the catalogue
/// hands these out behind an `Arc`.
pub struct LoadedFace {
    bytes: Bytes,
    index: u32,
    units_per_em: u16,
    shaper_data: ShaperData,
}

impl fmt::Debug for LoadedFace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LoadedFace")
            .field("bytes", &self.bytes.as_slice().len())
            .field("index", &self.index)
            .field("units_per_em", &self.units_per_em)
            .finish()
    }
}

impl LoadedFace {
    pub(crate) fn from_static(bytes: &'static [u8], index: u32) -> Result<Self, FontError> {
        Self::new(Bytes::Static(bytes), index)
    }

    pub(crate) fn from_vec(bytes: Vec<u8>, index: u32) -> Result<Self, FontError> {
        Self::new(Bytes::Shared(bytes.into()), index)
    }

    fn new(bytes: Bytes, index: u32) -> Result<Self, FontError> {
        let font = FontRef::from_index(bytes.as_slice(), index)
            .map_err(|error| FontError::Parse(error.to_string()))?;
        let units_per_em = font
            .metrics(Size::unscaled(), LocationRef::default())
            .units_per_em;
        if units_per_em == 0 {
            return Err(FontError::Parse("units per em is zero".into()));
        }
        let shaper_data = ShaperData::new(&font);
        Ok(Self {
            bytes,
            index,
            units_per_em,
            shaper_data,
        })
    }

    fn font(&self) -> Option<FontRef<'_>> {
        FontRef::from_index(self.bytes.as_slice(), self.index).ok()
    }

    /// The whole font file (the entire collection for a `.ttc`), for
    /// embedding in a PDF.
    pub fn bytes(&self) -> &[u8] {
        self.bytes.as_slice()
    }

    /// Font index inside the file; 0 unless the file is a collection.
    pub fn collection_index(&self) -> u32 {
        self.index
    }

    /// Design units per em.
    pub fn units_per_em(&self) -> u16 {
        self.units_per_em
    }

    /// The glyph for `ch`, if the face maps it.
    pub fn glyph_id(&self, ch: char) -> Option<u16> {
        let glyph = self.font()?.charmap().map(ch)?;
        u16::try_from(glyph.to_u32()).ok().filter(|g| *g != 0)
    }

    /// True when the face has a glyph for `ch`. Characters that never need a
    /// glyph (controls, zero-width joiners and variation selectors) count as
    /// covered, so they never force a fallback run.
    pub fn covers(&self, ch: char) -> bool {
        is_invisible(ch) || self.glyph_id(ch).is_some()
    }

    /// The first character of `text` this face cannot draw.
    pub fn first_uncovered(&self, text: &str) -> Option<char> {
        text.chars().find(|c| !self.covers(*c))
    }

    /// Nominal horizontal advance of `glyph` at `size`, without kerning.
    pub fn glyph_advance(&self, glyph: u16, size: f32) -> f32 {
        let Some(font) = self.font() else { return 0.0 };
        font.glyph_metrics(Size::unscaled(), LocationRef::default())
            .advance_width(skrifa::GlyphId::new(u32::from(glyph)))
            .unwrap_or(0.0)
            * size
            / f32::from(self.units_per_em)
    }

    /// Ascent, descent and line gap at `size` for the default instance.
    pub fn line_metrics(&self, size: f32) -> LineMetrics {
        self.line_metrics_at(size, &[])
    }

    /// Like [`LoadedFace::line_metrics`] at the variable-font coordinates
    /// `variations` (ignored by a static font).
    pub fn line_metrics_at(&self, size: f32, variations: &[Variation]) -> LineMetrics {
        let Some(font) = self.font() else {
            return LineMetrics {
                ascent: 0.0,
                descent: 0.0,
                line_gap: 0.0,
                line_height: 0.0,
                cap_height: None,
                x_height: None,
                underline: None,
                strikeout: None,
            };
        };
        let metrics = if variations.is_empty() {
            font.metrics(Size::new(size), LocationRef::default())
        } else {
            let location = font.axes().location(
                variations
                    .iter()
                    .map(|v| (skrifa::Tag::new(&v.tag), v.value)),
            );
            font.metrics(Size::new(size), &location)
        };
        let ascent = metrics.ascent.max(0.0);
        let descent = (-metrics.descent).max(0.0);
        let line_gap = metrics.leading.max(0.0);
        let decoration = |d: skrifa::metrics::Decoration| Decoration {
            offset: d.offset,
            thickness: d.thickness,
        };
        LineMetrics {
            ascent,
            descent,
            line_gap,
            line_height: ascent + descent + line_gap,
            cap_height: metrics.cap_height,
            x_height: metrics.x_height,
            underline: metrics.underline.map(decoration),
            strikeout: metrics.strikeout.map(decoration),
        }
    }

    /// Shapes `text` as a single run at `size`.
    ///
    /// `text` should hold one direction and one script (see
    /// [`crate::segment`]); line breaks and other control characters get zero
    /// advance. Glyphs come back in visual order.
    pub fn shape(&self, text: &str, size: f32, settings: &ShapeSettings) -> Shaped {
        let rtl_requested = settings.direction == TextDirection::RightToLeft;
        let Some(font) = self.font().filter(|_| !text.is_empty()) else {
            return Shaped::empty(rtl_requested);
        };
        let instance = (!settings.variations.is_empty()).then(|| {
            harfrust::ShaperInstance::from_variations(
                &font,
                settings.variations.iter().map(|v| harfrust::Variation {
                    tag: Tag::new(&v.tag),
                    value: v.value,
                }),
            )
        });
        let shaper = self
            .shaper_data
            .shaper(&font)
            .instance(instance.as_ref())
            .build();

        let mut buffer = UnicodeBuffer::new();
        buffer.push_str(text);
        match settings.direction {
            TextDirection::Auto => {}
            TextDirection::LeftToRight => buffer.set_direction(Direction::LeftToRight),
            TextDirection::RightToLeft => buffer.set_direction(Direction::RightToLeft),
        }
        if let Some(script) = settings
            .script
            .and_then(|tag| std::str::from_utf8(&tag).ok().map(str::to_owned))
            .and_then(|tag| tag.parse::<Script>().ok())
        {
            buffer.set_script(script);
        }
        if let Some(language) = settings
            .language
            .as_deref()
            .and_then(|tag| tag.parse::<Language>().ok())
        {
            buffer.set_language(language);
        }
        buffer.guess_segment_properties();
        let rtl = buffer.direction() == Direction::RightToLeft;

        let mut features = Vec::new();
        if !settings.kerning {
            features.push(Feature::new(Tag::new(b"kern"), 0, ..));
        }
        if !settings.ligatures {
            features.push(Feature::new(Tag::new(b"liga"), 0, ..));
            features.push(Feature::new(Tag::new(b"clig"), 0, ..));
        }

        let output = shaper.shape(buffer, harfrust::ShapeOptions::new().features(&features));
        let scale = size / f32::from(self.units_per_em);
        let mut width = 0.0_f32;
        let glyphs: Vec<ShapedGlyph> = output
            .glyph_infos()
            .iter()
            .zip(output.glyph_positions())
            .map(|(info, position)| {
                let cluster = info.cluster as usize;
                let control = text
                    .get(cluster..)
                    .and_then(|rest| rest.chars().next())
                    .is_some_and(char::is_control);
                let advance = if control {
                    0.0
                } else {
                    position.x_advance as f32 * scale
                };
                width += advance;
                ShapedGlyph {
                    glyph_id: u16::try_from(info.glyph_id).unwrap_or(0),
                    cluster,
                    advance,
                    x_offset: position.x_offset as f32 * scale,
                    y_offset: position.y_offset as f32 * scale,
                }
            })
            .collect();
        Shaped { glyphs, width, rtl }
    }

    /// Width of `text` at `size` with default shaping (kerning on).
    pub fn text_width(&self, text: &str, size: f32) -> f32 {
        self.shape(text, size, &ShapeSettings::default()).width
    }
}

/// Characters a face is never expected to draw.
fn is_invisible(ch: char) -> bool {
    ch.is_control()
        || matches!(
            ch,
            '\u{200B}'..='\u{200F}'
                | '\u{2028}'..='\u{202E}'
                | '\u{2060}'..='\u{2064}'
                | '\u{FE00}'..='\u{FE0F}'
                | '\u{FEFF}'
                | '\u{E0100}'..='\u{E01EF}'
        )
}
