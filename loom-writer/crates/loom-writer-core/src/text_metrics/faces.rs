//! The bundled Inter faces as `loom-fonts` loads them, and the caches that
//! keep shaping cheap enough to run on every keystroke.
//!
//! The page and the PDF both draw in Inter, so these are the faces measurement
//! must use. Faces are loaded once per process. A word's advances depend on
//! its neighbours only through the space that follows it (Inter Bold kerns a
//! comma, a full stop and an ellipsis against it; nothing kerns against a
//! space that precedes it, which the tests check for every character the faces
//! cover, and against whole-run shaping), so each distinct word with its
//! trailing space is shaped once per face and its per-grapheme advances are
//! reused wherever it appears. Caches are per thread: no locking, and a worker
//! that exports a PDF warms its own.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, OnceLock};

use loom_fonts::{FontCatalog, LoadedFace, ShapeSettings, TextDirection};
use unicode_segmentation::UnicodeSegmentation;

/// Which of the four Inter faces a run is set in: the bold and italic the
/// page markup expresses (weights from Bold up draw as Bold).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct Style {
    pub bold: bool,
    pub italic: bool,
}

impl Style {
    fn index(self) -> usize {
        usize::from(self.bold) + 2 * usize::from(self.italic)
    }
}

/// One character as a face sets it on its own.
#[derive(Clone, Copy, Debug)]
pub(crate) struct CharMetrics {
    /// Whether the face has a glyph for the character, or never draws one.
    pub covered: bool,
    /// Advance in em.
    pub advance: f32,
}

const UNAVAILABLE: CharMetrics = CharMetrics {
    covered: false,
    advance: 0.0,
};

/// Words longer than this are measured but not remembered: URLs and
/// unbroken runs would fill the cache with entries that are never seen again.
const MAX_CACHED_WORD_BYTES: usize = 160;
/// Bytes of word text the caches of one thread may hold before they restart.
const MAX_CACHED_BYTES: usize = 4 << 20;

/// The four faces, indexed by [`Style::index`]. `None` only if the bundled
/// faces were compiled out, in which case everything is estimated.
fn faces() -> &'static [Option<Arc<LoadedFace>>; 4] {
    static FACES: OnceLock<[Option<Arc<LoadedFace>>; 4]> = OnceLock::new();
    FACES.get_or_init(|| {
        let catalog = FontCatalog::bundled_only();
        let load = |bold: bool, italic: bool| {
            let font = catalog
                .resolve("Inter", if bold { 700 } else { 400 }, italic)
                .ok()?;
            catalog.load(font.primary()).ok()
        };
        [
            load(false, false),
            load(true, false),
            load(false, true),
            load(true, true),
        ]
    })
}

fn face(style: Style) -> Option<&'static Arc<LoadedFace>> {
    faces()[style.index()].as_ref()
}

/// Shaping for text measured one direction at a time: left to right, which is
/// all Inter covers, with kerning and the standard ligatures on as the page
/// draws them.
fn settings() -> ShapeSettings {
    ShapeSettings {
        direction: TextDirection::LeftToRight,
        ..ShapeSettings::default()
    }
}

/// Characters that take no room and draw nothing, however a font maps them.
fn never_drawn(ch: char) -> bool {
    matches!(
        ch as u32,
        0x00..=0x1F
            | 0x7F..=0x9F
            | 0xAD
            | 0x200B..=0x200F
            | 0x2028..=0x202E
            | 0x2060..=0x206F
            | 0xFE00..=0xFE0F
            | 0xFEFF
            | 0xE0000..=0xE0FFF
    )
}

struct Caches {
    ascii: [[Option<CharMetrics>; 128]; 4],
    chars: [HashMap<char, CharMetrics>; 4],
    words: [HashMap<Box<str>, Rc<[f32]>>; 4],
    word_bytes: usize,
}

impl Default for Caches {
    fn default() -> Self {
        Self {
            ascii: [[None; 128]; 4],
            chars: Default::default(),
            words: Default::default(),
            word_bytes: 0,
        }
    }
}

thread_local! {
    static CACHES: RefCell<Caches> = RefCell::new(Caches::default());
}

fn measure_char(style: Style, ch: char) -> CharMetrics {
    if never_drawn(ch) {
        return CharMetrics {
            covered: true,
            advance: 0.0,
        };
    }
    let Some(face) = face(style) else {
        return UNAVAILABLE;
    };
    if !face.covers(ch) {
        return UNAVAILABLE;
    }
    let advance = face.shape(&ch.to_string(), 1.0, &settings()).width;
    CharMetrics {
        covered: true,
        advance,
    }
}

/// How `style`'s face sets `ch` alone.
pub(crate) fn char_metrics(style: Style, ch: char) -> CharMetrics {
    CACHES.with(|caches| {
        let mut caches = caches.borrow_mut();
        let slot = style.index();
        if (ch as u32) < 128 {
            let entry = &mut caches.ascii[slot][ch as usize];
            return *entry.get_or_insert_with(|| measure_char(style, ch));
        }
        *caches.chars[slot]
            .entry(ch)
            .or_insert_with(|| measure_char(style, ch))
    })
}

/// The remembered advances of `word` (a word with the space that follows it):
/// the distance in em from the start of the word to the end of each of its
/// graphemes.
pub(crate) fn cached_word(style: Style, word: &str) -> Option<Rc<[f32]>> {
    CACHES.with(|caches| caches.borrow().words[style.index()].get(word).cloned())
}

/// Shapes `word` (text the face covers entirely) and returns the distance in
/// em from its start to the end of each grapheme, kerning applied, or `None`
/// when no face is available. A word that is short enough is remembered.
pub(crate) fn shape_word(style: Style, word: &str) -> Option<Rc<[f32]>> {
    let face = face(style)?;
    let shaped = face.shape(word, 1.0, &settings());
    let stops = shaped.caret_stops(word);
    let mut ends: Vec<f32> = Vec::with_capacity(word.len().min(64));
    let mut at = 0usize;
    let mut previous = 0.0_f32;
    for (offset, grapheme) in word.grapheme_indices(true) {
        let end = offset + grapheme.len();
        while at < stops.len() && stops[at].offset < end {
            at += 1;
        }
        let x = stops
            .get(at)
            .map_or(shaped.width, |stop| stop.x)
            .max(previous);
        ends.push(x);
        previous = x;
    }
    let ends: Rc<[f32]> = ends.into();
    if word.len() <= MAX_CACHED_WORD_BYTES {
        CACHES.with(|caches| {
            let mut caches = caches.borrow_mut();
            if caches.word_bytes + word.len() > MAX_CACHED_BYTES {
                caches.words.iter_mut().for_each(HashMap::clear);
                caches.word_bytes = 0;
            }
            if caches.words[style.index()]
                .insert(word.into(), Rc::clone(&ends))
                .is_none()
            {
                caches.word_bytes += word.len();
            }
        });
    }
    Some(ends)
}

#[cfg(test)]
pub(crate) fn clear_caches() {
    CACHES.with(|caches| *caches.borrow_mut() = Caches::default());
}

#[cfg(test)]
pub(crate) fn cached_word_count() -> usize {
    CACHES.with(|caches| caches.borrow().words.iter().map(HashMap::len).sum())
}
