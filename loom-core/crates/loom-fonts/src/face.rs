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
}

impl fmt::Display for FontError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyCatalog => f.write_str("no fonts are available"),
            Self::Io(message) => write!(f, "font file could not be read: {message}"),
            Self::Parse(message) => write!(f, "font data is not valid: {message}"),
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

/// Knobs for one shaping call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShapeSettings {
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
            direction: TextDirection::Auto,
            script: None,
            language: None,
            kerning: true,
            ligatures: true,
        }
    }
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

    /// Ascent, descent and line gap at `size`.
    pub fn line_metrics(&self, size: f32) -> LineMetrics {
        let Some(font) = self.font() else {
            return LineMetrics {
                ascent: 0.0,
                descent: 0.0,
                line_gap: 0.0,
                line_height: 0.0,
                cap_height: None,
                x_height: None,
            };
        };
        let metrics = font.metrics(Size::new(size), LocationRef::default());
        let ascent = metrics.ascent.max(0.0);
        let descent = (-metrics.descent).max(0.0);
        let line_gap = metrics.leading.max(0.0);
        LineMetrics {
            ascent,
            descent,
            line_gap,
            line_height: ascent + descent + line_gap,
            cap_height: metrics.cap_height,
            x_height: metrics.x_height,
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
        let shaper = self.shaper_data.shaper(&font).build();

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
