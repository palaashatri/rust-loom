//! A measured line of text: where every grapheme starts, in points.
//!
//! The text is cut into pieces that the page shapes separately (a change of
//! bold, italic, underline or strikethrough starts a new piece, and so does
//! white space), each piece is shaped in its Inter face with kerning, and the
//! pieces' advances are laid end to end. Positions are therefore the ones the
//! page draws a glyph at, which is what carets, selection rectangles,
//! pointer hit-testing and line breaking need. Characters Inter does not cover
//! are drawn by whatever font the window falls back to, so their widths are
//! estimates ([`Uncovered::Estimate`]); the PDF export measures them at the
//! advance it writes them with ([`Uncovered::Drawn`]).

use loom_pdf::TextStyle;
use loom_text::StyleRun;
use unicode_segmentation::UnicodeSegmentation;

use super::faces::{self, Style};

/// What a grapheme Inter cannot draw is measured as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Uncovered {
    /// The window's fallback font is unknown: full-width scripts and emoji
    /// take one em, letters and the rest a typical advance, except the
    /// characters the PDF draws as plain letters, which measure as them.
    Estimate,
    /// The width the PDF writes the character with (its `.notdef` box, an
    /// expansion such as `fi`, or a fallback font's glyph).
    Drawn,
}

/// The attributes that make the page shape two stretches of text separately.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Piece {
    style: Style,
    underline: bool,
    strikethrough: bool,
}

fn piece_at(runs: &[StyleRun], position: usize, base: Style) -> Piece {
    let mut piece = Piece {
        style: base,
        underline: false,
        strikethrough: false,
    };
    if let Some(run) = runs
        .iter()
        .find(|run| run.start <= position && position < run.end)
    {
        piece.style.bold |= super::is_bold_weight(run.style.weight);
        piece.style.italic |= run.style.italic;
        piece.underline = run.style.underline;
        piece.strikethrough = run.style.strikethrough;
    }
    piece
}

/// Advance in 1/1000 em of a character no face of ours covers, as an editor
/// estimates it: full-width scripts and emoji one em, letters and the rest a
/// typical advance.
fn estimated_units(ch: char, bold: bool) -> f32 {
    match ch {
        c if (c as u32) >= 0x2E80 => 1000.0,
        c if c.is_alphabetic() => {
            if bold {
                610.0
            } else {
                590.0
            }
        }
        _ => {
            if bold {
                600.0
            } else {
                570.0
            }
        }
    }
}

/// The advance in em of a grapheme Inter cannot draw.
fn uncovered_em(grapheme: &str, style: Style, how: Uncovered) -> f32 {
    let drawn = |grapheme: &str| {
        loom_pdf::text_width_pt(
            grapheme,
            &TextStyle {
                size_pt: 1.0,
                bold: style.bold,
                italic: style.italic,
                ..TextStyle::default()
            },
        )
    };
    match how {
        Uncovered::Drawn => drawn(grapheme),
        Uncovered::Estimate if grapheme.chars().all(loom_pdf::is_measured_exactly) => {
            drawn(grapheme)
        }
        Uncovered::Estimate => grapheme
            .chars()
            .next()
            .map_or(0.0, |ch| estimated_units(ch, style.bold) / 1000.0),
    }
}

/// A grapheme that is one white-space character.
fn lone_space(grapheme: &str) -> Option<char> {
    let mut chars = grapheme.chars();
    let ch = chars.next()?;
    (chars.next().is_none() && ch.is_whitespace()).then_some(ch)
}

/// Per-grapheme advances of a text, before they are scaled and accumulated.
pub(super) struct Advances {
    /// Byte offset of each grapheme.
    pub starts: Vec<u32>,
    /// Byte offset just past each grapheme.
    pub ends: Vec<u32>,
    /// Advance of each grapheme in em, kerning included.
    pub em: Vec<f32>,
    /// Whether the advance came from shaping (as opposed to an estimate or
    /// the PDF's own advance).
    pub shaped: Vec<bool>,
}

struct Builder<'a> {
    text: &'a str,
    uncovered: Uncovered,
    out: Advances,
}

/// The unfinished word: graphemes `first..` of the output.
struct Word {
    style: Style,
    first: usize,
}

impl Builder<'_> {
    fn grapheme_at(&self, index: usize) -> &str {
        &self.text[self.out.starts[index] as usize..self.out.ends[index] as usize]
    }

    /// The text of graphemes `first..last`.
    fn text_of(&self, first: usize, last: usize) -> &str {
        &self.text[self.out.starts[first] as usize..self.out.ends[last - 1] as usize]
    }

    /// Fills the advances of graphemes `first..last` from `ends`, the end of
    /// each in em from the start of `ends`' text.
    fn fill(&mut self, first: usize, ends: &[f32]) {
        let mut previous = 0.0_f32;
        for (offset, end) in ends.iter().enumerate() {
            self.out.em[first + offset] = end - previous;
            self.out.shaped[first + offset] = true;
            previous = *end;
        }
    }

    fn all_covered(&self, style: Style, first: usize, last: usize) -> bool {
        (first..last).all(|index| {
            self.grapheme_at(index)
                .chars()
                .all(|ch| faces::char_metrics(style, ch).covered)
        })
    }

    /// Shapes graphemes `first..last` (all covered) as one text.
    fn shape(&mut self, style: Style, first: usize, last: usize) {
        let piece = self.text_of(first, last);
        let ends = faces::cached_word(style, piece).or_else(|| faces::shape_word(style, piece));
        match ends {
            Some(ends) if ends.len() == last - first => self.fill(first, &ends),
            _ => {
                for index in first..last {
                    self.estimate(style, index);
                }
            }
        }
    }

    fn estimate(&mut self, style: Style, index: usize) {
        let grapheme = self.grapheme_at(index).to_string();
        self.out.em[index] = uncovered_em(&grapheme, style, self.uncovered);
        self.out.shaped[index] = false;
    }

    /// Resolves the advances of the graphemes `word.first..last` of a
    /// finished word.
    fn finish_word(&mut self, word: Word, last: usize) {
        let Word { style, first } = word;
        if first >= last {
            return;
        }
        // Whole word, covered and known: one lookup.
        if let Some(ends) = faces::cached_word(style, self.text_of(first, last)) {
            if ends.len() == last - first {
                self.fill(first, &ends);
                return;
            }
        }
        if self.all_covered(style, first, last) {
            self.shape(style, first, last);
            return;
        }
        // A mix: shape each stretch Inter covers, estimate the rest.
        let mut run_start = first;
        for index in first..last {
            let covered = self
                .grapheme_at(index)
                .chars()
                .all(|ch| faces::char_metrics(style, ch).covered);
            if covered {
                continue;
            }
            if run_start < index {
                self.shape(style, run_start, index);
            }
            self.estimate(style, index);
            run_start = index + 1;
        }
        if run_start < last {
            self.shape(style, run_start, last);
        }
    }
}

/// Advances of `text`, which begins at byte `text_start` of its block (so the
/// character `runs`, which are in block offsets, line up). `base` is the
/// face every character starts from; a run adds bold or italic to it.
pub(super) fn advances(
    text: &str,
    text_start: usize,
    runs: &[StyleRun],
    base: Style,
    uncovered: Uncovered,
) -> Advances {
    let mut builder = Builder {
        text,
        uncovered,
        out: Advances {
            starts: Vec::with_capacity(text.len()),
            ends: Vec::with_capacity(text.len()),
            em: Vec::with_capacity(text.len()),
            shaped: Vec::with_capacity(text.len()),
        },
    };
    let mut word: Option<(Word, Piece)> = None;
    for (offset, grapheme) in text.grapheme_indices(true) {
        let piece = piece_at(runs, text_start + offset, base);
        let space = lone_space(grapheme);
        let index = builder.out.starts.len();
        builder.out.starts.push(offset as u32);
        builder.out.ends.push((offset + grapheme.len()) as u32);
        builder.out.em.push(0.0);
        builder.out.shaped.push(false);
        if let Some((_, current)) = &word {
            if *current != piece {
                // A new style starts a new shaping run.
                if let Some((done, _)) = word.take() {
                    builder.finish_word(done, index);
                }
            } else if space.is_some() {
                // The first space after a word belongs to it: Inter Bold
                // kerns a comma, a full stop and an ellipsis against the
                // space that follows them. No character kerns against a
                // space that precedes it, nor two spaces together (the
                // tests check every character the face covers).
                if let Some((done, _)) = word.take() {
                    builder.finish_word(done, index + 1);
                }
                continue;
            }
        }
        match space {
            Some(ch) => {
                let metrics = faces::char_metrics(piece.style, ch);
                if metrics.covered {
                    builder.out.em[index] = metrics.advance;
                    builder.out.shaped[index] = true;
                } else {
                    builder.estimate(piece.style, index);
                }
            }
            None => {
                if word.is_none() {
                    word = Some((
                        Word {
                            style: piece.style,
                            first: index,
                        },
                        piece,
                    ));
                }
            }
        }
    }
    if let Some((done, _)) = word.take() {
        let last = builder.out.starts.len();
        builder.finish_word(done, last);
    }
    builder.out
}

/// Where each grapheme of one text starts, in points from its start.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LineMeasure {
    /// Byte offset of every grapheme boundary, ending with the text length.
    bounds: Vec<u32>,
    /// Distance from the start of the text to each boundary.
    x: Vec<f32>,
}

impl LineMeasure {
    /// Measures `text` at `size` points; `text_start` is its byte offset in
    /// the block whose `runs` apply.
    pub(crate) fn new(
        text: &str,
        text_start: usize,
        runs: &[StyleRun],
        size: f32,
        base: Style,
        uncovered: Uncovered,
    ) -> Self {
        let Advances { mut starts, em, .. } = advances(text, text_start, runs, base, uncovered);
        let mut x = Vec::with_capacity(em.len() + 1);
        let mut sum = 0.0_f32;
        x.push(0.0);
        for advance in em {
            sum += advance;
            x.push(sum * size);
        }
        starts.push(text.len() as u32);
        Self { bounds: starts, x }
    }

    /// Width of the whole text.
    pub(crate) fn width(&self) -> f32 {
        self.x.last().copied().unwrap_or(0.0)
    }

    /// Number of graphemes.
    pub(crate) fn len(&self) -> usize {
        self.bounds.len() - 1
    }

    /// Distance from the start of the text to byte `offset`, which should be
    /// a grapheme boundary (one inside a grapheme counts as the next).
    pub(crate) fn x_at(&self, offset: usize) -> f32 {
        let at = self
            .bounds
            .partition_point(|bound| (*bound as usize) < offset);
        self.x[at.min(self.x.len() - 1)]
    }

    /// Width of bytes `start..end`.
    pub(crate) fn advance(&self, start: usize, end: usize) -> f32 {
        self.x_at(end) - self.x_at(start)
    }

    /// The graphemes of `text` (the text this was measured from) with their
    /// advances: `(start byte, grapheme, advance)`.
    pub(crate) fn graphemes<'t>(&self, text: &'t str) -> Vec<(usize, &'t str, f32)> {
        (0..self.len())
            .map(|index| {
                let (start, end) = (self.bounds[index] as usize, self.bounds[index + 1] as usize);
                (start, &text[start..end], self.x[index + 1] - self.x[index])
            })
            .collect()
    }

    /// The byte offset of the boundary nearest to `x` among the graphemes of
    /// the first `visible` bytes (the text without its trailing hard line
    /// breaks): a point left of a grapheme's middle belongs before it.
    pub(crate) fn offset_at_x(&self, x: f32, visible: usize) -> usize {
        // Half a micro-point of slack keeps a point computed from a boundary
        // on that boundary, even in front of zero-width graphemes.
        const SLACK: f32 = 1e-3;
        for index in 0..self.len() {
            if self.bounds[index + 1] as usize > visible {
                break;
            }
            let (left, right) = (self.x[index], self.x[index + 1]);
            if x < left + (right - left) / 2.0 + SLACK {
                return self.bounds[index] as usize;
            }
        }
        visible
    }

    /// Splits the text this was measured from into the byte ranges of its
    /// lines so that each fits `max_width`, breaking after white space where
    /// possible and inside a word only when one word is wider than a line. A
    /// hard line break ends its line and is part of it. Spaces may hang past
    /// the edge, as in any editor.
    pub(crate) fn wrap(&self, text: &str, max_width: f32) -> Vec<(usize, usize)> {
        if text.is_empty() {
            return vec![(0, 0)];
        }
        let mut ranges = Vec::new();
        let mut line_start = 0usize;
        let mut last_break: Option<usize> = None;
        for index in 0..self.len() {
            let (start, end) = (self.bounds[index] as usize, self.bounds[index + 1] as usize);
            let grapheme = &text[start..end];
            if grapheme == "\n" {
                ranges.push((self.bounds[line_start] as usize, end));
                line_start = index + 1;
                last_break = None;
                continue;
            }
            let glyph = self.x[index + 1] - self.x[index];
            let width = self.x[index] - self.x[line_start];
            let is_space = grapheme.chars().any(char::is_whitespace);
            if !is_space && index > line_start && width + glyph > max_width {
                let cut = last_break
                    .filter(|break_at| *break_at > line_start)
                    .unwrap_or(index);
                ranges.push((self.bounds[line_start] as usize, self.bounds[cut] as usize));
                line_start = cut;
                last_break = None;
            }
            if is_space {
                last_break = Some(index + 1);
            }
        }
        if (self.bounds[line_start] as usize) < text.len() {
            ranges.push((self.bounds[line_start] as usize, text.len()));
        }
        ranges
    }
}
