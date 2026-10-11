//! The faces text is measured in, as `loom-fonts` loads them, and the caches
//! that keep shaping cheap enough to run on every keystroke.
//!
//! The document font is the bundled Inter, measured from the four embedded
//! faces whatever the system has installed (the page and the PDF draw those
//! same bytes). A run that names another family is measured in the face the
//! process font catalogue resolves for it ([`crate::fonts`]): the family's own
//! face when installed, its metric-compatible or fallback stand-in when not,
//! which is also the family the page is told to draw with. Faces are loaded
//! once per process. A word's advances depend on its neighbours only through
//! the space that follows it (Inter Bold kerns a comma, a full stop and an
//! ellipsis against it; nothing kerns against a space that precedes it, which
//! the tests check for every character the faces cover, and against whole-run
//! shaping), so each distinct word with its trailing space is shaped once per
//! face and its per-grapheme advances are reused wherever it appears. Caches
//! are per thread: no locking, and a worker that exports a PDF warms its own.
//! They are dropped when the font catalogue is replaced.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use loom_fonts::{FontCatalog, LoadedFace, ShapeSettings, TextDirection};
use unicode_segmentation::UnicodeSegmentation;

use crate::fonts::{family_key, font_catalog, font_epoch};

/// Distinct non-document families measured in one process. A document that
/// names more is measured in the document font beyond the limit.
const MAX_FAMILIES: usize = 512;

/// A font family as measurement knows it: a small number standing for a
/// normalised family name. Zero is the document font.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct Family(u16);

impl Family {
    /// The bundled document font.
    pub(crate) const DOCUMENT: Family = Family(0);

    fn is_document(self) -> bool {
        self.0 == 0
    }
}

fn family_names() -> &'static Mutex<Vec<String>> {
    static NAMES: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
    NAMES.get_or_init(|| Mutex::new(Vec::new()))
}

/// The measurement family for a family name stored in a character style.
pub(crate) fn family_for(name: &str) -> Family {
    let Some(key) = family_key(name) else {
        return Family::DOCUMENT;
    };
    let mut names = family_names()
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    if let Some(position) = names.iter().position(|known| *known == key) {
        return Family(position as u16 + 1);
    }
    if names.len() >= MAX_FAMILIES {
        return Family::DOCUMENT;
    }
    names.push(key);
    Family(names.len() as u16)
}

fn name_of(family: Family) -> Option<String> {
    let index = usize::from(family.0).checked_sub(1)?;
    family_names()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(index)
        .cloned()
}

/// Which face a run is set in: a family, and the bold and italic the page
/// markup expresses (weights from Bold up draw as Bold).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct Style {
    pub bold: bool,
    pub italic: bool,
    pub family: Family,
}

impl Style {
    /// A face of the document font.
    pub(crate) const fn new(bold: bool, italic: bool) -> Self {
        Self {
            bold,
            italic,
            family: Family::DOCUMENT,
        }
    }

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

/// The four document faces, indexed by [`Style::index`]. `None` only if the
/// bundled faces were compiled out, in which case everything is estimated.
fn document_faces() -> &'static [Option<Arc<LoadedFace>>; 4] {
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

/// The face the catalogue gives `family` at the weight and slant of `style`.
fn catalog_face(family: Family, bold: bool, italic: bool) -> Option<Arc<LoadedFace>> {
    let name = name_of(family)?;
    let catalog = font_catalog();
    let font = catalog
        .resolve(&name, if bold { 700 } else { 400 }, italic)
        .ok()?;
    catalog.load(font.primary()).ok()
}

/// Shaping for text measured one direction at a time: left to right, with
/// kerning and the standard ligatures on as the page draws them.
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

/// What one face has measured so far.
struct Slot {
    face: Option<Arc<LoadedFace>>,
    ascii: [Option<CharMetrics>; 128],
    chars: HashMap<char, CharMetrics>,
    words: HashMap<Box<str>, Rc<[f32]>>,
    word_bytes: usize,
}

impl Slot {
    fn new(face: Option<Arc<LoadedFace>>) -> Self {
        Self {
            face,
            ascii: [None; 128],
            chars: HashMap::new(),
            words: HashMap::new(),
            word_bytes: 0,
        }
    }
}

/// The four document slots first, then one slot per other face used.
struct Caches {
    epoch: u64,
    slots: Vec<Slot>,
    other: HashMap<Style, usize>,
}

impl Default for Caches {
    fn default() -> Self {
        Self {
            epoch: font_epoch(),
            slots: document_faces()
                .iter()
                .map(|face| Slot::new(face.clone()))
                .collect(),
            other: HashMap::new(),
        }
    }
}

impl Caches {
    /// Forgets every face that came from the catalogue once it was replaced.
    fn sync_epoch(&mut self) {
        let now = font_epoch();
        if now != self.epoch {
            self.slots.truncate(4);
            self.other.clear();
            self.epoch = now;
        }
    }

    fn slot_index(&mut self, style: Style) -> usize {
        if style.family.is_document() {
            return style.index();
        }
        if let Some(index) = self.other.get(&style) {
            return *index;
        }
        let face = catalog_face(style.family, style.bold, style.italic);
        self.slots.push(Slot::new(face));
        let index = self.slots.len() - 1;
        self.other.insert(style, index);
        index
    }

    fn word_bytes(&self) -> usize {
        self.slots.iter().map(|slot| slot.word_bytes).sum()
    }
}

thread_local! {
    static CACHES: RefCell<Caches> = RefCell::new(Caches::default());
}

fn with_caches<R>(work: impl FnOnce(&mut Caches) -> R) -> R {
    CACHES.with(|caches| {
        let mut caches = caches.borrow_mut();
        caches.sync_epoch();
        work(&mut caches)
    })
}

fn measure_char(face: Option<&Arc<LoadedFace>>, ch: char) -> CharMetrics {
    if never_drawn(ch) {
        return CharMetrics {
            covered: true,
            advance: 0.0,
        };
    }
    let Some(face) = face else {
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
    with_caches(|caches| {
        let index = caches.slot_index(style);
        let slot = &mut caches.slots[index];
        if (ch as u32) < 128 {
            if let Some(found) = slot.ascii[ch as usize] {
                return found;
            }
            let measured = measure_char(slot.face.as_ref(), ch);
            slot.ascii[ch as usize] = Some(measured);
            return measured;
        }
        if let Some(found) = slot.chars.get(&ch) {
            return *found;
        }
        let measured = measure_char(slot.face.as_ref(), ch);
        slot.chars.insert(ch, measured);
        measured
    })
}

/// The remembered advances of `word` (a word with the space that follows it):
/// the distance in em from the start of the word to the end of each of its
/// graphemes.
pub(crate) fn cached_word(style: Style, word: &str) -> Option<Rc<[f32]>> {
    with_caches(|caches| {
        let index = caches.slot_index(style);
        caches.slots[index].words.get(word).cloned()
    })
}

/// Shapes `word` (text the face covers entirely) and returns the distance in
/// em from its start to the end of each grapheme, kerning applied, or `None`
/// when no face is available. A word that is short enough is remembered.
pub(crate) fn shape_word(style: Style, word: &str) -> Option<Rc<[f32]>> {
    let face = with_caches(|caches| {
        let index = caches.slot_index(style);
        caches.slots[index].face.clone()
    })?;
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
        with_caches(|caches| {
            if caches.word_bytes() + word.len() > MAX_CACHED_BYTES {
                for slot in &mut caches.slots {
                    slot.words.clear();
                    slot.word_bytes = 0;
                }
            }
            let index = caches.slot_index(style);
            let slot = &mut caches.slots[index];
            if slot.words.insert(word.into(), Rc::clone(&ends)).is_none() {
                slot.word_bytes += word.len();
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
    with_caches(|caches| caches.slots.iter().map(|slot| slot.words.len()).sum())
}
