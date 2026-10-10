//! One line of text laid out with a resolved font: bidi and script
//! segmentation, per-character fallback and shaping, placed left to right.

use crate::catalog::FontCatalog;
use crate::face::{LoadedFace, ShapeSettings, TextDirection};
use crate::info::FaceId;
use crate::resolve::FontRef;
use crate::segment::{segment, visual_order};
use crate::shaped::Shaped;
use std::cell::RefCell;
use std::ops::Range;
use std::sync::Arc;

/// A shaped piece of a line drawn with one face.
#[derive(Clone, Debug, PartialEq)]
pub struct LaidOutRun {
    /// Byte range of the line's text this run covers.
    pub range: Range<usize>,
    /// The face the run was shaped with.
    pub face: FaceId,
    /// True when the run is right to left.
    pub rtl: bool,
    /// Glyphs; `cluster` values are relative to `range.start`.
    pub shaped: Shaped,
    /// Distance from the line's left edge to the run's left edge.
    pub x: f32,
}

/// A laid out line. Runs are in visual (left to right) order.
#[derive(Clone, Debug, PartialEq)]
pub struct LineLayout {
    /// The runs, left to right.
    pub runs: Vec<LaidOutRun>,
    /// Total advance width.
    pub width: f32,
    /// True when no face could be loaded and `width` is a rough estimate
    /// (half an em per character); the line has no runs.
    pub estimated: bool,
}

impl LineLayout {
    /// The caret x for byte `offset` of the laid out text.
    pub fn x_at_offset(&self, text: &str, offset: usize) -> f32 {
        let offset = offset.min(text.len());
        let Some(run) = self
            .runs
            .iter()
            .find(|r| r.range.start <= offset && offset < r.range.end)
            .or_else(|| self.runs.iter().max_by_key(|r| r.range.end))
        else {
            return if self.estimated { self.width } else { 0.0 };
        };
        let relative = offset.saturating_sub(run.range.start);
        run.x + run.shaped.x_at_offset(&text[run.range.clone()], relative)
    }

    /// The byte offset of the boundary nearest to horizontal position `x`.
    pub fn offset_at_x(&self, text: &str, x: f32) -> usize {
        if self.runs.is_empty() {
            return if x <= 0.0 { 0 } else { text.len() };
        }
        let run = self
            .runs
            .iter()
            .find(|r| x < r.x + r.shaped.width)
            .unwrap_or_else(|| &self.runs[self.runs.len() - 1]);
        let local = run.shaped.offset_at_x(&text[run.range.clone()], x - run.x);
        run.range.start + local
    }
}

/// Lazily loaded chain faces, so a fallback CJK font is read only when a
/// character needs it.
struct Chain<'a> {
    catalog: &'a FontCatalog,
    ids: &'a [FaceId],
    slots: RefCell<Vec<Option<Option<Arc<LoadedFace>>>>>,
}

impl<'a> Chain<'a> {
    fn new(catalog: &'a FontCatalog, ids: &'a [FaceId]) -> Self {
        Self {
            catalog,
            ids,
            slots: RefCell::new(vec![None; ids.len()]),
        }
    }

    fn get(&self, index: usize) -> Option<Arc<LoadedFace>> {
        let mut slots = self.slots.borrow_mut();
        slots[index]
            .get_or_insert_with(|| self.catalog.load(self.ids[index]).ok())
            .clone()
    }

    fn covers(&self, index: usize, ch: char) -> bool {
        self.get(index).is_some_and(|face| face.covers(ch))
    }
}

/// Characters that should stay with the face of the text before them rather
/// than start a fallback run of their own.
fn follows_previous(ch: char) -> bool {
    ch.is_whitespace()
        || ch.is_ascii_punctuation()
        || ch.is_ascii_digit()
        || matches!(
            ch,
            '\u{0300}'..='\u{036F}'
                | '\u{1AB0}'..='\u{1AFF}'
                | '\u{1DC0}'..='\u{1DFF}'
                | '\u{200C}'..='\u{200D}'
                | '\u{20D0}'..='\u{20FF}'
                | '\u{FE00}'..='\u{FE0F}'
                | '\u{FE20}'..='\u{FE2F}'
        )
}

/// Splits `range` of `text` into stretches that share one chain index.
///
/// Each character takes the first chain face that covers it. Spaces, digits,
/// ASCII punctuation and combining marks keep the previous character's face
/// when it covers them, so a fallback run is not fragmented by a space. A
/// character no face covers stays with the previous face (it draws as that
/// face's missing-glyph box).
pub(crate) fn split_by_coverage(
    text: &str,
    range: Range<usize>,
    chain_len: usize,
    covers: &dyn Fn(usize, char) -> bool,
) -> Vec<(Range<usize>, usize)> {
    let mut parts: Vec<(Range<usize>, usize)> = Vec::new();
    for (offset, ch) in text[range.clone()].char_indices() {
        let start = range.start + offset;
        let end = start + ch.len_utf8();
        let previous = parts.last().map(|(_, face)| *face);
        let face = match previous {
            Some(prev) if follows_previous(ch) && covers(prev, ch) => prev,
            _ => (0..chain_len)
                .find(|&i| covers(i, ch))
                .or(previous)
                .unwrap_or(0),
        };
        match parts.last_mut() {
            Some((span, last)) if *last == face => span.end = end,
            _ => parts.push((start..end, face)),
        }
    }
    parts
}

impl FontCatalog {
    /// Lays out `text` (one line, no line breaks) at `size` with `font`.
    ///
    /// `base` is the paragraph direction; pass `Auto` unless the paragraph
    /// has an explicit one.
    pub fn layout_line(
        &self,
        text: &str,
        font: &FontRef,
        size: f32,
        base: TextDirection,
    ) -> LineLayout {
        let estimate = |text: &str| LineLayout {
            runs: Vec::new(),
            width: text.chars().filter(|c| !c.is_control()).count() as f32 * size * 0.5,
            estimated: true,
        };
        if text.is_empty() {
            return LineLayout {
                runs: Vec::new(),
                width: 0.0,
                estimated: false,
            };
        }
        let chain = Chain::new(self, &font.chain);
        if (0..font.chain.len()).all(|i| chain.get(i).is_none()) {
            return estimate(text);
        }
        // A face that fails to load covers nothing, so its characters move on
        // to the next face; the primary slot still anchors uncovered ones.
        let covers = |index: usize, ch: char| chain.covers(index, ch);

        struct Piece {
            range: Range<usize>,
            chain_index: usize,
            level: u8,
            script: Option<[u8; 4]>,
        }
        let mut pieces: Vec<Piece> = Vec::new();
        for run in segment(text, base) {
            for (range, chain_index) in
                split_by_coverage(text, run.range.clone(), font.chain.len(), &covers)
            {
                pieces.push(Piece {
                    range,
                    chain_index,
                    level: run.level,
                    script: run.script,
                });
            }
        }

        let levels: Vec<u8> = pieces.iter().map(|p| p.level).collect();
        let order = visual_order(&levels);
        let mut runs = Vec::with_capacity(pieces.len());
        let mut x = 0.0_f32;
        for logical in order {
            let piece = &pieces[logical];
            // The primary may have failed to load: use the first face that did.
            let used = std::iter::once(piece.chain_index)
                .chain(0..font.chain.len())
                .find(|&i| chain.get(i).is_some());
            let Some((used, face)) = used.and_then(|i| chain.get(i).map(|f| (i, f))) else {
                continue;
            };
            let rtl = piece.level % 2 == 1;
            let settings = ShapeSettings {
                direction: if rtl {
                    TextDirection::RightToLeft
                } else {
                    TextDirection::LeftToRight
                },
                script: piece.script,
                ..ShapeSettings::default()
            };
            let shaped = face.shape(&text[piece.range.clone()], size, &settings);
            let width = shaped.width;
            runs.push(LaidOutRun {
                range: piece.range.clone(),
                face: font.chain[used],
                rtl,
                shaped,
                x,
            });
            x += width;
        }
        LineLayout {
            runs,
            width: x,
            estimated: false,
        }
    }

    /// Width of `text` at `size` with `font`, including fallback faces.
    pub fn text_width(&self, text: &str, font: &FontRef, size: f32) -> f32 {
        self.layout_line(text, font, size, TextDirection::Auto)
            .width
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn covers_only<'a>(tables: &'a [&'a str]) -> impl Fn(usize, char) -> bool + 'a {
        move |index, ch| tables[index].contains(ch)
    }

    #[test]
    fn everything_covered_by_the_primary_stays_one_part() {
        let covers = covers_only(&["abc def", ""]);
        let parts = split_by_coverage("abc def", 0..7, 2, &covers);
        assert_eq!(parts, vec![(0..7, 0)]);
    }

    #[test]
    fn an_uncovered_character_moves_to_the_first_face_that_has_it() {
        let covers = covers_only(&["ab ", "XY"]);
        let parts = split_by_coverage("abXYab", 0..6, 2, &covers);
        assert_eq!(parts, vec![(0..2, 0), (2..4, 1), (4..6, 0)]);
    }

    #[test]
    fn spaces_and_digits_do_not_fragment_a_fallback_run() {
        let covers = covers_only(&["ab 1", "XY 1"]);
        // The space after Y is covered by the previous (fallback) face.
        let parts = split_by_coverage("aXY Yb", 0..6, 2, &covers);
        assert_eq!(parts, vec![(0..1, 0), (1..5, 1), (5..6, 0)]);
    }

    #[test]
    fn a_character_nobody_covers_stays_with_the_previous_face() {
        let covers = covers_only(&["ab", "X"]);
        let parts = split_by_coverage("aXqb", 0..4, 2, &covers);
        assert_eq!(parts, vec![(0..1, 0), (1..3, 1), (3..4, 0)]);
    }

    #[test]
    fn multibyte_characters_split_on_boundaries() {
        let text = "a\u{4E2D}b";
        let covers = covers_only(&["ab", "\u{4E2D}"]);
        let parts = split_by_coverage(text, 0..text.len(), 2, &covers);
        assert_eq!(parts, vec![(0..1, 0), (1..4, 1), (4..5, 0)]);
    }
}
