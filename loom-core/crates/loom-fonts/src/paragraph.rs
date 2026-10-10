//! Paragraph and line layout: styled runs, paragraph-level bidi, per-cluster
//! fallback and shaping.
//!
//! Bidi is resolved once for the whole paragraph (UAX 9 needs the context of
//! the text around a line, so a wrapped line of "123 abc" in a Hebrew
//! paragraph is not laid out like the same words alone) and each line is
//! reordered from that result. Scripts are resolved over the paragraph too.
//! Text is then cut into pieces that share a style, a face, a script and a
//! direction, and each piece is shaped once.

use crate::catalog::FontCatalog;
use crate::face::{LoadedFace, ShapeSettings, TextDirection};
use crate::layout::{LaidOutRun, LineLayout};
use crate::resolve::{FaceSetup, FontRef};
use crate::segment::{base_level, is_script_neutral, resolve_scripts};
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::ops::Range;
use std::sync::Arc;
use unicode_bidi::BidiInfo;
use unicode_segmentation::UnicodeSegmentation;

/// A stretch of text with one font, size and shaping options, for
/// [`FontCatalog::layout_line`]. A line is the concatenation of its runs.
#[derive(Clone, Copy, Debug)]
pub struct StyledRun<'a> {
    /// The text of the run.
    pub text: &'a str,
    /// The resolved font (see [`FontCatalog::resolve`]).
    pub font: &'a FontRef,
    /// Font size; the unit of the result is the unit of the size.
    pub size: f32,
    /// BCP 47 language such as `"tr"` for locale-specific forms.
    pub language: Option<&'a str>,
    /// Apply pair kerning (on by default).
    pub kerning: bool,
    /// Apply standard ligatures (on by default).
    pub ligatures: bool,
}

impl<'a> StyledRun<'a> {
    /// A run with default shaping options.
    pub fn new(text: &'a str, font: &'a FontRef, size: f32) -> Self {
        Self {
            text,
            font,
            size,
            language: None,
            kerning: true,
            ligatures: true,
        }
    }
}

/// A byte range of a paragraph's text with its font, size and shaping
/// options, for [`Paragraph::new`].
#[derive(Clone, Debug)]
pub struct StyledSpan<'a> {
    /// The bytes of the paragraph text this span styles.
    pub range: Range<usize>,
    /// The resolved font (see [`FontCatalog::resolve`]).
    pub font: &'a FontRef,
    /// Font size.
    pub size: f32,
    /// BCP 47 language such as `"tr"`.
    pub language: Option<&'a str>,
    /// Apply pair kerning (on by default).
    pub kerning: bool,
    /// Apply standard ligatures (on by default).
    pub ligatures: bool,
}

impl<'a> StyledSpan<'a> {
    /// A span with default shaping options.
    pub fn new(range: Range<usize>, font: &'a FontRef, size: f32) -> Self {
        Self {
            range,
            font,
            size,
            language: None,
            kerning: true,
            ligatures: true,
        }
    }
}

/// A paragraph prepared for line layout: bidi levels and scripts resolved
/// over the whole text, plus its styles.
///
/// Spans should tile the text in order. A byte outside every span takes the
/// style of the nearest span starting before it (or the first span). Build
/// one per paragraph, then lay out each wrapped line with
/// [`FontCatalog::layout_paragraph_line`]; re-resolving per line would give
/// wrong levels at line starts.
pub struct Paragraph<'a> {
    text: &'a str,
    spans: Vec<StyledSpan<'a>>,
    bidi: BidiInfo<'a>,
    /// `(end byte, script)` runs, ascending; scripts are fully resolved.
    scripts: Vec<(usize, Option<[u8; 4]>)>,
}

impl fmt::Debug for Paragraph<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Paragraph")
            .field("len", &self.text.len())
            .field("spans", &self.spans.len())
            .finish()
    }
}

impl<'a> Paragraph<'a> {
    /// Resolves bidi levels (`base` is the paragraph direction; `Auto` takes
    /// it from the first strong character) and scripts for `text`.
    pub fn new(text: &'a str, spans: &[StyledSpan<'a>], base: TextDirection) -> Self {
        let mut spans: Vec<StyledSpan<'a>> = spans
            .iter()
            .map(|span| {
                let mut span = span.clone();
                span.range.end = span.range.end.min(text.len());
                span.range.start = span.range.start.min(span.range.end);
                span
            })
            .collect();
        spans.sort_by_key(|span| span.range.start);

        let per_char = resolve_scripts(text);
        let mut scripts: Vec<(usize, Option<[u8; 4]>)> = Vec::new();
        for ((start, ch), script) in text.char_indices().zip(per_char) {
            let end = start + ch.len_utf8();
            match scripts.last_mut() {
                Some(last) if last.1 == script => last.0 = end,
                _ => scripts.push((end, script)),
            }
        }
        Self {
            text,
            spans,
            bidi: BidiInfo::new(text, base_level(base)),
            scripts,
        }
    }

    /// The paragraph text.
    pub fn text(&self) -> &'a str {
        self.text
    }

    /// Length of the text in bytes.
    pub fn len(&self) -> usize {
        self.text.len()
    }

    /// True for an empty paragraph.
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    fn span_index_at(&self, byte: usize) -> usize {
        self.spans
            .partition_point(|span| span.range.start <= byte)
            .saturating_sub(1)
    }

    fn script_at(&self, byte: usize) -> Option<[u8; 4]> {
        let at = self.scripts.partition_point(|(end, _)| *end <= byte);
        self.scripts.get(at).and_then(|(_, script)| *script)
    }

    /// `range` clamped into the text and moved onto character boundaries.
    fn clamp(&self, range: Range<usize>) -> Range<usize> {
        let snap = |mut at: usize| {
            at = at.min(self.text.len());
            while !self.text.is_char_boundary(at) {
                at += 1;
            }
            at
        };
        let start = snap(range.start);
        start..snap(range.end).max(start)
    }
}

/// The chain of one span's font: faces loaded on first use, and a cache of
/// which characters each face covers.
struct Chain<'a> {
    catalog: &'a FontCatalog,
    font: &'a FontRef,
    slots: RefCell<Vec<Option<Option<Arc<LoadedFace>>>>>,
    coverage: RefCell<HashMap<(usize, char), bool>>,
}

impl<'a> Chain<'a> {
    fn new(catalog: &'a FontCatalog, font: &'a FontRef) -> Self {
        Self {
            catalog,
            font,
            slots: RefCell::new(vec![None; font.chain.len()]),
            coverage: RefCell::new(HashMap::new()),
        }
    }

    fn len(&self) -> usize {
        self.font.chain.len()
    }

    fn get(&self, index: usize) -> Option<Arc<LoadedFace>> {
        let mut slots = self.slots.borrow_mut();
        slots
            .get_mut(index)?
            .get_or_insert_with(|| self.catalog.load(self.font.chain[index]).ok())
            .clone()
    }

    /// The first face that loaded, which anchors characters nobody covers.
    fn first_loaded(&self) -> Option<usize> {
        (0..self.len()).find(|&i| self.get(i).is_some())
    }

    fn covers(&self, index: usize, ch: char) -> bool {
        if let Some(hit) = self.coverage.borrow().get(&(index, ch)) {
            return *hit;
        }
        let covered = self.get(index).is_some_and(|face| face.covers(ch));
        self.coverage.borrow_mut().insert((index, ch), covered);
        covered
    }

    fn setup(&self, index: usize) -> FaceSetup {
        self.font
            .chain_setup
            .get(index)
            .cloned()
            .unwrap_or_else(|| FaceSetup {
                variations: Vec::new(),
                synthetic_bold: index == 0 && self.font.synthetic_bold,
                synthetic_italic: index == 0 && self.font.synthetic_italic,
            })
    }
}

/// One grapheme cluster as face assignment sees it.
pub(crate) struct ClusterInfo<'t> {
    pub text: &'t str,
    /// The style span the cluster belongs to.
    pub span: usize,
    /// True when the cluster's base takes its face from its neighbours
    /// (spaces, digits, punctuation, marks).
    pub neutral: bool,
}

/// Chooses a chain index for every cluster (`None` when no face is usable).
///
/// A cluster is never split between faces: it takes the first chain face
/// that covers all of it, else the first that covers its base character, so
/// a base and its combining marks are drawn and shaped together. A neutral
/// cluster stays in the previous cluster's face when that face covers it, so
/// a space inside a fallback run does not interrupt the run and kerning is
/// not broken. A cluster nothing covers stays with the previous face, which
/// draws its missing-glyph box.
pub(crate) fn assign_faces(
    clusters: &[ClusterInfo<'_>],
    chain_len: &dyn Fn(usize) -> usize,
    covers: &dyn Fn(usize, usize, char) -> bool,
    anchor: &dyn Fn(usize) -> Option<usize>,
) -> Vec<Option<usize>> {
    let mut out = Vec::with_capacity(clusters.len());
    let mut previous: Option<(usize, usize)> = None;
    for cluster in clusters {
        let all = |face: usize| {
            cluster
                .text
                .chars()
                .all(|ch| covers(cluster.span, face, ch))
        };
        let same_span_previous = previous.filter(|(span, _)| *span == cluster.span);
        let adopted = same_span_previous
            .filter(|_| cluster.neutral)
            .map(|(_, face)| face)
            .filter(|&face| all(face));
        let len = chain_len(cluster.span);
        let face = adopted
            .or_else(|| (0..len).find(|&face| all(face)))
            .or_else(|| {
                let base = cluster.text.chars().next()?;
                (0..len).find(|&face| covers(cluster.span, face, base))
            })
            .or_else(|| same_span_previous.map(|(_, face)| face))
            .or_else(|| anchor(cluster.span));
        if let Some(face) = face {
            previous = Some((cluster.span, face));
        }
        out.push(face);
    }
    out
}

/// A run of text shaped as one unit.
struct Piece {
    range: Range<usize>,
    span: usize,
    chain_index: usize,
    script: Option<[u8; 4]>,
}

/// Splits the logical `range` (one bidi level) into pieces, or `None` when a
/// style's font has no usable face at all.
fn pieces_in(
    paragraph: &Paragraph<'_>,
    chains: &[Chain<'_>],
    range: Range<usize>,
) -> Option<Vec<Piece>> {
    let slice = &paragraph.text[range.clone()];
    let mut clusters = Vec::new();
    let mut bounds = Vec::new();
    for (offset, grapheme) in slice.grapheme_indices(true) {
        let start = range.start + offset;
        let span = paragraph.span_index_at(start);
        let neutral = grapheme.chars().next().is_some_and(is_script_neutral);
        bounds.push((start, start + grapheme.len()));
        clusters.push(ClusterInfo {
            text: grapheme,
            span,
            neutral,
        });
    }
    let faces = assign_faces(
        &clusters,
        &|span| chains[span].len(),
        &|span, face, ch| chains[span].covers(face, ch),
        &|span| chains[span].first_loaded(),
    );
    let mut pieces: Vec<Piece> = Vec::new();
    for ((cluster, (start, end)), face) in clusters.iter().zip(bounds).zip(faces) {
        let chain_index = face?;
        let script = paragraph.script_at(start);
        match pieces.last_mut() {
            Some(last)
                if last.span == cluster.span
                    && last.chain_index == chain_index
                    && last.script == script =>
            {
                last.range.end = end;
            }
            _ => pieces.push(Piece {
                range: start..end,
                span: cluster.span,
                chain_index,
                script,
            }),
        }
    }
    Some(pieces)
}

impl FontCatalog {
    /// Lays out one line made of `runs` (one line, no line breaks) with bidi
    /// resolved over the line itself. `base` is the paragraph direction;
    /// pass `Auto` unless the paragraph has an explicit one. For a wrapped
    /// paragraph use [`Paragraph`] and
    /// [`FontCatalog::layout_paragraph_line`] instead.
    pub fn layout_line(&self, runs: &[StyledRun<'_>], base: TextDirection) -> LineLayout {
        let text: String = runs.iter().map(|run| run.text).collect();
        let mut spans = Vec::with_capacity(runs.len());
        let mut at = 0;
        for run in runs {
            spans.push(StyledSpan {
                range: at..at + run.text.len(),
                font: run.font,
                size: run.size,
                language: run.language,
                kerning: run.kerning,
                ligatures: run.ligatures,
            });
            at += run.text.len();
        }
        let paragraph = Paragraph::new(&text, &spans, base);
        self.layout_paragraph_line(&paragraph, 0..text.len())
    }

    /// Lays out `text` as one single-style line.
    pub fn layout_text(
        &self,
        text: &str,
        font: &FontRef,
        size: f32,
        base: TextDirection,
    ) -> LineLayout {
        self.layout_line(&[StyledRun::new(text, font, size)], base)
    }

    /// Width of `text` at `size` with `font`, including fallback faces.
    pub fn text_width(&self, text: &str, font: &FontRef, size: f32) -> f32 {
        self.layout_text(text, font, size, TextDirection::Auto)
            .width
    }

    /// Lays out the bytes `line` of `paragraph`.
    ///
    /// Run ranges and the offsets taken by [`LineLayout`] queries are
    /// paragraph offsets. A line crossing a hard paragraph break (a newline
    /// inside the range) is laid out as consecutive bidi paragraphs, left to
    /// right.
    pub fn layout_paragraph_line(
        &self,
        paragraph: &Paragraph<'_>,
        line: Range<usize>,
    ) -> LineLayout {
        let line = paragraph.clamp(line);
        if paragraph.spans.is_empty() {
            return LineLayout::blank(line, 0.0, 0.0, 0.0);
        }
        let chains: Vec<Chain<'_>> = paragraph
            .spans
            .iter()
            .map(|span| Chain::new(self, span.font))
            .collect();

        if line.is_empty() {
            return self.blank_line(paragraph, &chains, line);
        }

        // Cut every bidi paragraph's part of the line into level runs in
        // visual order, then each level run into shapeable pieces.
        let mut ordered: Vec<(Piece, bool)> = Vec::new();
        for info in &paragraph.bidi.paragraphs {
            let from = line.start.max(info.range.start);
            let to = line.end.min(info.range.end);
            if from >= to {
                continue;
            }
            let (levels, level_runs) = paragraph.bidi.visual_runs(info, from..to);
            for level_run in level_runs {
                let rtl = levels[level_run.start].is_rtl();
                let Some(mut pieces) = pieces_in(paragraph, &chains, level_run) else {
                    return self.estimate(paragraph, line);
                };
                if rtl {
                    pieces.reverse();
                }
                ordered.extend(pieces.into_iter().map(|piece| (piece, rtl)));
            }
        }

        let mut runs = Vec::with_capacity(ordered.len());
        let mut x = 0.0_f32;
        for (piece, rtl) in ordered {
            let span = &paragraph.spans[piece.span];
            let chain = &chains[piece.span];
            let Some(face) = chain.get(piece.chain_index) else {
                return self.estimate(paragraph, line);
            };
            let setup = chain.setup(piece.chain_index);
            let settings = ShapeSettings {
                variations: setup.variations.clone(),
                direction: if rtl {
                    TextDirection::RightToLeft
                } else {
                    TextDirection::LeftToRight
                },
                script: piece.script,
                language: span.language.map(str::to_owned),
                kerning: span.kerning,
                ligatures: span.ligatures,
            };
            let text = &paragraph.text[piece.range.clone()];
            let shaped = face.shape(text, span.size, &settings);
            let metrics = face.line_metrics_at(span.size, &setup.variations);
            let caret_stops = shaped
                .caret_stops(text)
                .into_iter()
                .map(|stop| crate::shaped::CaretStop {
                    offset: piece.range.start + stop.offset,
                    x: x + stop.x,
                })
                .collect();
            let width = shaped.width;
            runs.push(LaidOutRun {
                range: piece.range,
                face: span.font.chain[piece.chain_index],
                rtl,
                shaped,
                x,
                style: piece.span,
                size: span.size,
                synthetic_bold: setup.synthetic_bold,
                synthetic_italic: setup.synthetic_italic,
                variations: setup.variations,
                ascent: metrics.ascent,
                descent: metrics.descent,
                line_gap: metrics.line_gap,
                underline: metrics.underline,
                strikeout: metrics.strikeout,
                caret_stops,
            });
            x += width;
        }
        LineLayout::from_runs(line, runs)
    }

    /// An empty line: no glyphs, but the metrics of the style at its
    /// position so a caret in an empty paragraph has a height.
    fn blank_line(
        &self,
        paragraph: &Paragraph<'_>,
        chains: &[Chain<'_>],
        line: Range<usize>,
    ) -> LineLayout {
        let index = paragraph.span_index_at(line.start);
        let span = &paragraph.spans[index];
        let chain = &chains[index];
        let metrics = chain.first_loaded().and_then(|face| {
            let setup = chain.setup(face);
            chain
                .get(face)
                .map(|f| f.line_metrics_at(span.size, &setup.variations))
        });
        match metrics {
            Some(m) => LineLayout::blank(line, m.ascent, m.descent, m.line_gap),
            None => LineLayout::blank(line, span.size * 0.8, span.size * 0.2, 0.0),
        }
    }

    /// Half an em per character, in each character's own style size.
    fn estimate(&self, paragraph: &Paragraph<'_>, line: Range<usize>) -> LineLayout {
        let widths = |offset: usize, ch: char| {
            if ch.is_control() {
                0.0
            } else {
                paragraph.spans[paragraph.span_index_at(offset)].size * 0.5
            }
        };
        let size = paragraph
            .spans
            .iter()
            .map(|span| span.size)
            .fold(0.0, f32::max);
        LineLayout::estimated(paragraph.text, line, &widths, size * 0.8, size * 0.2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Chain 0 and 1 of span 0 with the given coverage tables.
    fn assign(text: &[(&str, bool)], tables: &[&str]) -> Vec<Option<usize>> {
        let clusters: Vec<ClusterInfo<'_>> = text
            .iter()
            .map(|(text, neutral)| ClusterInfo {
                text,
                span: 0,
                neutral: *neutral,
            })
            .collect();
        assign_faces(
            &clusters,
            &|_| tables.len(),
            &|_, face, ch| tables[face].contains(ch),
            &|_| Some(0),
        )
    }

    #[test]
    fn a_character_goes_to_the_first_face_that_covers_it() {
        let faces = assign(&[("a", false), ("X", false), ("b", false)], &["ab", "XY"]);
        assert_eq!(faces, [Some(0), Some(1), Some(0)]);
    }

    #[test]
    fn a_base_and_its_mark_are_never_split() {
        // Face 0 has the base letter "x" but not the mark; face 1 has only
        // the mark. The cluster stays whole in the face with its base.
        let faces = assign(&[("x\u{483}", false)], &["x", "\u{483}"]);
        assert_eq!(faces, [Some(0)]);
        // The reverse: the base only in the fallback, the mark only in the
        // primary. Still one face, the one with the base.
        let faces = assign(&[("\u{4e2d}\u{301}", false)], &["\u{301}", "\u{4e2d}"]);
        assert_eq!(faces, [Some(1)]);
        // When one face has both, that face is preferred over the base-only one.
        let faces = assign(&[("x\u{301}", false)], &["x", "x\u{301}"]);
        assert_eq!(faces, [Some(1)]);
    }

    #[test]
    fn neutral_clusters_follow_the_previous_face_when_it_covers_them() {
        let tables = ["ab 1", "XY 1"];
        // The space and digit after Y stay in the fallback run.
        let faces = assign(
            &[
                ("a", false),
                ("X", false),
                ("Y", false),
                (" ", true),
                ("1", true),
                ("b", false),
            ],
            &tables,
        );
        assert_eq!(
            faces,
            [Some(0), Some(1), Some(1), Some(1), Some(1), Some(0)]
        );
        // When the previous face lacks the neutral, the first covering face
        // takes it.
        let faces = assign(&[("X", false), (" ", true)], &["ab ", "XY"]);
        assert_eq!(faces, [Some(1), Some(0)]);
    }

    #[test]
    fn a_non_neutral_cluster_never_adopts_the_previous_face() {
        // "b" is covered by both faces; it takes the first (primary), not the
        // fallback run it follows.
        let faces = assign(&[("X", false), ("b", false)], &["ab", "XYb"]);
        assert_eq!(faces, [Some(1), Some(0)]);
    }

    #[test]
    fn a_character_nobody_covers_stays_with_the_previous_face() {
        let faces = assign(
            &[("a", false), ("X", false), ("q", false), ("b", false)],
            &["ab", "X"],
        );
        assert_eq!(faces, [Some(0), Some(1), Some(1), Some(0)]);
        // At the very start it falls back to the anchor face.
        let faces = assign(&[("q", false)], &["ab", "X"]);
        assert_eq!(faces, [Some(0)]);
    }

    #[test]
    fn spans_do_not_share_previous_faces() {
        let clusters = [
            ClusterInfo {
                text: "X",
                span: 0,
                neutral: false,
            },
            ClusterInfo {
                text: " ",
                span: 1,
                neutral: true,
            },
        ];
        let covers = |span: usize, face: usize, ch: char| match (span, face) {
            (0, 1) => "X ".contains(ch),
            (0, 0) => false,
            (1, 0) => " ".contains(ch),
            _ => false,
        };
        let faces = assign_faces(&clusters, &|_| 2, &covers, &|_| Some(0));
        // The space is in span 1, whose own primary covers it.
        assert_eq!(faces, [Some(1), Some(0)]);
    }

    #[test]
    fn no_usable_face_is_reported() {
        let clusters = [ClusterInfo {
            text: "a",
            span: 0,
            neutral: false,
        }];
        let faces = assign_faces(&clusters, &|_| 0, &|_, _, _| false, &|_| None);
        assert_eq!(faces, [None]);
    }
}
