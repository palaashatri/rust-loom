//! The text the page draws, as typed segments.
//!
//! A row of the page is the words of one laid-out line (or one table cell) and
//! the character style they carry. They reach the Slint page as segments, each
//! a plain string with its weight, slant and rules, so no markup language sits
//! between the document and the screen: a `*` or `<` or `1.` the user typed is
//! just that character, and nothing the user typed can change how the rest of
//! the line is drawn.
//!
//! Most rows are one look from end to end. Those stay a single segment that the
//! text item aligns and wraps itself, exactly as before. A row with several
//! looks, or with an underline or strikethrough, is positioned here instead:
//! each segment gets the horizontal offset the page layout measures for it
//! (`loom-fonts` through [`text_advance`]), so glyphs, caret and selection
//! highlight agree. Line ends (a soft break, a page break) are not text; they
//! end the line, and the layout already put what follows on the next row.
//!
//! Weight (bold or not), slant, size, underline and strikethrough are drawn per
//! run, and the page layout measures each run at the size it is drawn at, so
//! the caret and the glyphs agree. Colour and superscript are not drawn yet.
//! Glyphs of different sizes on one line share a baseline.

use std::collections::BTreeSet;

use loom_text::{Alignment, CharacterStyle, FontWeight, StyleRun};
use loom_writer_core::{text_advance, RichBlock};

use crate::{WriterTextRule, WriterTextSegment};

/// Weight of a bold run. Slint's font weight is a number; the page measures
/// weights below Bold as Regular, so a segment is only ever these two.
const BOLD: i32 = 700;
const REGULAR: i32 = 400;

/// Line pitch, in em, of text this module wraps itself. The page layout owns
/// the pitch of ordinary lines; this is only for table cells and the fallback.
const WRAPPED_LINE_EM: f32 = 1.25;
/// Where the rules sit, in em from the top of a line of Inter, whose baseline
/// is about 0.97 em down. The thickness has a floor so a small size still draws
/// a visible line.
const UNDERLINE_TOP_EM: f32 = 1.07;
/// Where the baseline of a line of Inter sits, in em from the top of the line.
/// Glyphs of different sizes on one line are placed so their baselines match.
const BASELINE_EM: f32 = 0.97;
const STRIKETHROUGH_TOP_EM: f32 = 0.62;
const RULE_THICKNESS_EM: f32 = 0.07;
const RULE_THICKNESS_MIN_PT: f32 = 0.6;
/// A line this much wider than its row, in points, is wrapped; narrower
/// differences are rounding between two measurements of the same text.
const FIT_SLACK_PT: f32 = 0.5;

/// How the text of a row sits across the row.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Align {
    /// Placed here, by the block's own alignment against the row width, and
    /// wrapped to it: table cells, chart labels and the fallback rows.
    Block,
    /// One line the page layout already broke, measured `width` points wide;
    /// aligned with exactly the shift the layout uses for its highlights.
    Line(f32),
    /// Left at the row's own edge: a stretch of a line, which the layout
    /// placed by measuring it.
    Start,
}

/// What a row draws.
pub(crate) struct RowText {
    pub segments: Vec<WriterTextSegment>,
    pub rules: Vec<WriterTextRule>,
    /// False: at most one segment, aligned and wrapped by the text item.
    pub positioned: bool,
}

/// The attributes the page draws per run, and the size it draws them at.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Look {
    bold: bool,
    italic: bool,
    underline: bool,
    strikethrough: bool,
    size: f32,
}

impl Look {
    /// The look of a run inside a block whose own size is `block_size`; text
    /// outside every run has the block's look. A heading block is bold, as the
    /// PDF draws it, and a run can add emphasis but not take it away.
    fn of(style: Option<&CharacterStyle>, block_size: f32, heading: bool) -> Self {
        match style {
            Some(style) => Self {
                bold: heading || style.weight.numeric() >= FontWeight::Bold.numeric(),
                italic: style.italic,
                underline: style.underline,
                strikethrough: style.strikethrough,
                size: loom_writer_core::run_size(style, block_size),
            },
            None => Self {
                bold: heading,
                italic: false,
                underline: false,
                strikethrough: false,
                size: block_size,
            },
        }
    }
}

/// A stretch of a block's bytes drawn in one look.
struct Piece {
    start: usize,
    end: usize,
    look: Look,
}

/// Characters that end a line instead of being drawn.
fn is_line_end(ch: char) -> bool {
    matches!(ch, '\n' | '\u{b}' | '\u{c}')
}

fn drawn(text: &str) -> String {
    text.chars().filter(|ch| !is_line_end(*ch)).collect()
}

fn has_drawn(text: &str) -> bool {
    text.chars().any(|ch| !is_line_end(ch))
}

fn floor_boundary(text: &str, offset: usize) -> usize {
    let mut at = offset.min(text.len());
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

/// A block's text with its runs, measured at one size the way the page layout
/// measures it, so what is placed here is where the layout puts its highlight.
struct Measure<'a> {
    text: &'a str,
    runs: &'a [StyleRun],
    size: f32,
}

impl Measure<'_> {
    /// Width in points of bytes `from..to` of the block, shaped on their own.
    fn width(&self, from: usize, to: usize) -> f32 {
        text_advance(&self.text[from..to], from, self.runs, self.size)
    }

    /// The same, without the white space that hangs off the end of a line.
    fn ink_width(&self, from: usize, to: usize) -> f32 {
        self.width(from, from + self.text[from..to].trim_end().len())
    }
}

/// The block's text cut wherever its look changes. Runs that differ only in
/// colour or family share one piece.
fn pieces(block: &RichBlock, block_size: f32) -> Vec<Piece> {
    let text = block.text.as_str();
    let mut cuts = BTreeSet::from([0, text.len()]);
    for run in &block.runs {
        cuts.insert(floor_boundary(text, run.start));
        cuts.insert(floor_boundary(text, run.end));
    }
    let cuts: Vec<usize> = cuts.into_iter().collect();
    let heading = loom_writer_core::is_heading_kind(block.kind.as_str());
    let mut pieces: Vec<Piece> = Vec::new();
    for pair in cuts.windows(2) {
        let (start, end) = (pair[0], pair[1]);
        let look = Look::of(
            block
                .runs
                .iter()
                .find(|run| run.start <= start && start < run.end)
                .map(|run| &run.style),
            block_size,
            heading,
        );
        match pieces.last_mut() {
            Some(last) if last.look == look => last.end = end,
            _ => pieces.push(Piece { start, end, look }),
        }
    }
    pieces
}

fn segment(text: String, look: Look, x: f32, y: f32) -> WriterTextSegment {
    WriterTextSegment {
        text: text.into(),
        x,
        y,
        size: look.size,
        weight: if look.bold { BOLD } else { REGULAR },
        italic: look.italic,
        underline: look.underline,
        strikethrough: look.strikethrough,
    }
}

/// What the page draws for `block` in a row `width` points wide, set at
/// `font_size`.
pub(crate) fn row_text(block: &RichBlock, font_size: f32, width: f32, align: Align) -> RowText {
    let text = block.text.as_str();
    let all = pieces(block, font_size);
    let shown: Vec<&Piece> = all
        .iter()
        .filter(|piece| has_drawn(&text[piece.start..piece.end]))
        .collect();
    let decorated = shown
        .iter()
        .any(|piece| piece.look.underline || piece.look.strikethrough);
    let broken_inside = text.trim_end_matches(is_line_end).contains(is_line_end);
    if shown.len() <= 1 && !decorated && !broken_inside {
        let segments = shown
            .first()
            .map(|piece| vec![segment(drawn(text), piece.look, 0.0, 0.0)])
            .unwrap_or_default();
        return RowText {
            segments,
            rules: Vec::new(),
            positioned: false,
        };
    }

    let measure = Measure {
        text,
        runs: &block.runs,
        size: font_size,
    };
    let mut segments = Vec::new();
    let mut rules = Vec::new();
    let lines = line_ranges(&measure, width, align);
    for (index, &(line_start, line_end)) in lines.iter().enumerate() {
        let ink_end = line_start + text[line_start..line_end].trim_end().len();
        let line_width = match align {
            Align::Line(measured) => measured,
            _ => measure.width(line_start, ink_end),
        };
        let shift = match (align, block.style.alignment.resolve(text)) {
            (Align::Start, _) => 0.0,
            (_, Alignment::Center) => ((width - line_width) / 2.0).max(0.0),
            (_, Alignment::Right) => (width - line_width).max(0.0),
            _ => 0.0,
        };
        // The tallest glyph on the line sets its height; smaller glyphs sit on
        // the same baseline, so each one's top is lowered by its size gap.
        let line_size = all
            .iter()
            .filter(|piece| piece.start.max(line_start) < piece.end.min(line_end))
            .map(|piece| piece.look.size)
            .fold(font_size, f32::max);
        let line_top =
            index as f32 * loom_writer_core::line_pitch(&block.runs, font_size) * WRAPPED_LINE_EM;
        for piece in &all {
            let (from, to) = (piece.start.max(line_start), piece.end.min(line_end));
            if from >= to {
                continue;
            }
            let words = drawn(&text[from..to]);
            if words.is_empty() {
                continue;
            }
            let x = shift + measure.width(line_start, from);
            let y = line_top + (line_size - piece.look.size) * BASELINE_EM;
            segments.push(segment(words, piece.look, x, y));
            let inked = to.min(ink_end);
            if inked > from && (piece.look.underline || piece.look.strikethrough) {
                let run_width = measure.width(from, inked);
                let thickness = (piece.look.size * RULE_THICKNESS_EM).max(RULE_THICKNESS_MIN_PT);
                for (wanted, top_em) in [
                    (piece.look.underline, UNDERLINE_TOP_EM),
                    (piece.look.strikethrough, STRIKETHROUGH_TOP_EM),
                ] {
                    if wanted {
                        rules.push(WriterTextRule {
                            x,
                            y: y + piece.look.size * top_em,
                            width: run_width,
                            height: thickness,
                        });
                    }
                }
            }
        }
    }
    RowText {
        segments,
        rules,
        positioned: true,
    }
}

/// The byte ranges of the lines of `text`: a line ends after each line-end
/// character, and for [`Align::Block`] a longer line is wrapped to `width`.
/// Ranges cover the text; the line-end characters stay at the end of theirs.
fn line_ranges(measure: &Measure, width: f32, align: Align) -> Vec<(usize, usize)> {
    let text = measure.text;
    let mut hard = Vec::new();
    let mut start = 0;
    for (at, ch) in text.char_indices() {
        if is_line_end(ch) {
            hard.push((start, at + ch.len_utf8()));
            start = at + ch.len_utf8();
        }
    }
    if start < text.len() || hard.is_empty() {
        hard.push((start, text.len()));
    }
    if align != Align::Block {
        return hard;
    }
    hard.into_iter()
        .flat_map(|(start, end)| wrap(measure, width, start, end))
        .collect()
}

/// Greedy word wrap of `text[start..end]` to `width`: a line breaks before the
/// first word that no longer fits, and a word wider than the row stays whole on
/// a line of its own.
fn wrap(measure: &Measure, width: f32, start: usize, end: usize) -> Vec<(usize, usize)> {
    let text = measure.text;
    let fits = |from: usize, to: usize| measure.ink_width(from, to) <= width + FIT_SLACK_PT;
    let mut lines = Vec::new();
    let mut from = start;
    while from < end {
        if fits(from, end) {
            lines.push((from, end));
            break;
        }
        let mut previous = None;
        let mut cut = None;
        for (at, ch) in text[from..end].char_indices() {
            let at = from + at;
            let starts_word = at > from
                && !ch.is_whitespace()
                && previous.is_some_and(|before: char| before.is_whitespace());
            previous = Some(ch);
            if !starts_word {
                continue;
            }
            if fits(from, at) {
                cut = Some(at);
            } else {
                // The first break after `from` is taken even when it does not
                // fit, so a long word advances instead of looping.
                cut = cut.or(Some(at));
                break;
            }
        }
        let cut = cut.unwrap_or(end);
        lines.push((from, cut));
        from = cut;
    }
    if lines.is_empty() {
        lines.push((start, end));
    }
    lines
}
