//! The visual order of a line that holds right-to-left text.
//!
//! Everything else in this crate measures a line in logical order, which is the
//! drawn order for left-to-right text. A line with right-to-left characters is
//! drawn in visual order (Unicode bidi rule L2), so its carets, selection
//! rectangles and pointer hits are read from [`LineOrder`]. Text with no
//! character that can be right to left, and lines whose bidi levels are all
//! even, skip this module: their layout is the logical one, as before.

use std::ops::Range;

use loom_fonts::{segment, visual_order, TextDirection, TextRun};
use unicode_segmentation::UnicodeSegmentation;

use super::measure::LineMeasure;
use crate::layout::{block_bidi_runs, block_positions};
use crate::{CaretAffinity, RichBlock};

#[cfg(test)]
thread_local! {
    static PARAGRAPH_SCANS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn paragraph_scan_count() -> usize {
    PARAGRAPH_SCANS.with(std::cell::Cell::get)
}

/// The bidi runs of a paragraph's text, or `None` from [`BidiRuns::of`] when no
/// character can be right to left.
pub(crate) struct BidiRuns {
    runs: Vec<TextRun>,
}

impl BidiRuns {
    pub(crate) fn of(text: &str) -> Option<Self> {
        #[cfg(test)]
        PARAGRAPH_SCANS.with(|count| count.set(count.get() + 1));
        text.chars().any(may_be_right_to_left).then(|| Self {
            runs: segment(text, TextDirection::Auto),
        })
    }

    /// The bidi embedding level of the character at byte `at`.
    fn level_at(&self, at: usize) -> u8 {
        let index = self.runs.partition_point(|run| run.range.end <= at);
        self.runs.get(index).map_or(0, |run| run.level)
    }
}

/// Whether a character can take a right-to-left bidi level: the right-to-left
/// scripts and their presentation forms, the right-to-left mark, and the marks
/// that open a right-to-left embedding or isolate.
fn may_be_right_to_left(ch: char) -> bool {
    matches!(
        ch as u32,
        0x0590..=0x08FF
            | 0x200F
            | 0x202B
            | 0x202E
            | 0x2067
            | 0x2068
            | 0xFB1D..=0xFDFF
            | 0xFE70..=0xFEFF
            | 0x10800..=0x10FFF
            | 0x1E800..=0x1EFFF
    )
}

/// A caret position on a line: the byte offset it belongs to, the side of that
/// offset it sits on, and its x in points from the line's visible text.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Stop {
    pub offset: usize,
    pub affinity: CaretAffinity,
    pub x: f32,
}

/// A run of selected graphemes drawn next to each other: one highlight.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Span {
    pub start: usize,
    pub end: usize,
    pub left: f32,
    pub right: f32,
}

/// One visible grapheme of a line, placed in visual order.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Glyph {
    start: usize,
    end: usize,
    /// Points from the left edge of the visible text.
    left: f32,
    right: f32,
    rtl: bool,
}

impl Glyph {
    /// The x where a caret belonging to the grapheme's first character sits.
    fn leading(&self) -> f32 {
        if self.rtl {
            self.right
        } else {
            self.left
        }
    }

    /// The x where a caret belonging to the grapheme's last character sits.
    fn trailing(&self) -> f32 {
        if self.rtl {
            self.left
        } else {
            self.right
        }
    }

    fn leading_stop(&self) -> Stop {
        Stop {
            offset: self.start,
            affinity: CaretAffinity::Downstream,
            x: self.leading(),
        }
    }

    fn trailing_stop(&self) -> Stop {
        Stop {
            offset: self.end,
            affinity: CaretAffinity::Upstream,
            x: self.trailing(),
        }
    }

    /// The caret stop on this grapheme's visual left edge.
    fn left_stop(&self) -> Stop {
        if self.rtl {
            self.trailing_stop()
        } else {
            self.leading_stop()
        }
    }

    /// The caret stop on this grapheme's visual right edge.
    fn right_stop(&self) -> Stop {
        if self.rtl {
            self.leading_stop()
        } else {
            self.trailing_stop()
        }
    }
}

/// The visible graphemes of a line that holds right-to-left text, in visual
/// order from left to right.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LineOrder {
    glyphs: Vec<Glyph>,
}

impl LineOrder {
    /// The visual order of the visible bytes `visible` of `text`, measured by
    /// `measure` (the same text). `None` when every grapheme is at an even
    /// level, so the line is drawn in logical order.
    pub(crate) fn build(
        bidi: &BidiRuns,
        text: &str,
        measure: &LineMeasure,
        visible: Range<usize>,
    ) -> Option<Self> {
        let logical: Vec<(usize, usize, u8)> = text[visible.clone()]
            .grapheme_indices(true)
            .map(|(offset, grapheme)| {
                let start = visible.start + offset;
                (start, start + grapheme.len(), bidi.level_at(start))
            })
            .collect();
        if logical.iter().all(|&(_, _, level)| level % 2 == 0) {
            return None;
        }
        let levels: Vec<u8> = logical.iter().map(|&(_, _, level)| level).collect();
        let mut left = 0.0_f32;
        let glyphs = visual_order(&levels)
            .into_iter()
            .map(|index| {
                let (start, end, level) = logical[index];
                let right = left + measure.advance(start, end);
                let glyph = Glyph {
                    start,
                    end,
                    left,
                    right,
                    rtl: level % 2 == 1,
                };
                left = right;
                glyph
            })
            .collect();
        Some(Self { glyphs })
    }

    /// The x of the caret at block byte `offset`, from the visible text's left
    /// edge. Offsets outside the visible text sit at its nearest end.
    pub(crate) fn x_of(&self, offset: usize, affinity: CaretAffinity) -> f32 {
        let leading = self.glyphs.iter().find(|glyph| glyph.start == offset);
        let trailing = self.glyphs.iter().find(|glyph| glyph.end == offset);
        let found = match affinity {
            CaretAffinity::Downstream => leading
                .map(|glyph| glyph.leading())
                .or_else(|| trailing.map(|glyph| glyph.trailing())),
            CaretAffinity::Upstream => trailing
                .map(|glyph| glyph.trailing())
                .or_else(|| leading.map(|glyph| glyph.leading())),
        };
        if let Some(x) = found {
            return x;
        }
        let first = self.glyphs.iter().min_by_key(|glyph| glyph.start);
        let last = self.glyphs.iter().max_by_key(|glyph| glyph.end);
        match (first, last) {
            (Some(first), _) if offset <= first.start => first.leading(),
            (_, Some(last)) => last.trailing(),
            _ => 0.0,
        }
    }

    /// The caret nearest a point `x` points from the visible text's left edge.
    /// A point left of a grapheme's middle belongs on its visual left edge, as
    /// in logical order. `None` only for a line with no glyphs.
    pub(crate) fn stop_at(&self, x: f32) -> Option<Stop> {
        // Half a micro-point of slack keeps a stop's own x on that stop.
        const SLACK: f32 = 1e-3;
        for glyph in &self.glyphs {
            if x < (glyph.left + glyph.right) / 2.0 + SLACK {
                return Some(glyph.left_stop());
            }
        }
        self.glyphs.last().map(Glyph::right_stop)
    }

    /// Every caret stop, left to right. Where two candidate stops share an
    /// offset and an x they are one caret, which keeps the leading affinity a
    /// plain caret has.
    fn stops(&self) -> Vec<Stop> {
        let mut candidates = Vec::with_capacity(self.glyphs.len() * 2 + 1);
        if let Some(first) = self.glyphs.first() {
            candidates.push(first.left_stop());
        }
        for (index, glyph) in self.glyphs.iter().enumerate() {
            candidates.push(glyph.right_stop());
            if let Some(next) = self.glyphs.get(index + 1) {
                candidates.push(next.left_stop());
            }
        }
        let mut stops: Vec<Stop> = Vec::with_capacity(candidates.len());
        for stop in candidates {
            match stops.last_mut() {
                Some(last) if last.offset == stop.offset && (last.x - stop.x).abs() < 1e-3 => {
                    if stop.affinity == CaretAffinity::Downstream {
                        last.affinity = CaretAffinity::Downstream;
                    }
                }
                _ => stops.push(stop),
            }
        }
        stops
    }

    /// The caret stop beside the caret at block byte `offset`, in visual order:
    /// the one toward the visual right when `forward`, else toward the left.
    /// `None` at the end of the line.
    pub(crate) fn neighbour(
        &self,
        offset: usize,
        affinity: CaretAffinity,
        forward: bool,
    ) -> Option<Stop> {
        let stops = self.stops();
        let index = stops
            .iter()
            .position(|stop| stop.offset == offset && stop.affinity == affinity)
            .or_else(|| stops.iter().position(|stop| stop.offset == offset))?;
        if forward {
            stops.get(index + 1).copied()
        } else {
            index
                .checked_sub(1)
                .and_then(|previous| stops.get(previous).copied())
        }
    }

    /// The highlights for the block bytes `selection`: each run of selected
    /// graphemes that is next to the others in visual order is one span.
    pub(crate) fn spans(&self, selection: Range<usize>) -> Vec<Span> {
        let mut spans: Vec<Span> = Vec::new();
        let mut open: Option<Span> = None;
        for glyph in &self.glyphs {
            if glyph.start >= selection.start && glyph.end <= selection.end {
                if let Some(span) = &mut open {
                    span.start = span.start.min(glyph.start);
                    span.end = span.end.max(glyph.end);
                    span.right = glyph.right;
                } else {
                    open = Some(Span {
                        start: glyph.start,
                        end: glyph.end,
                        left: glyph.left,
                        right: glyph.right,
                    });
                }
            } else if let Some(span) = open.take() {
                spans.push(span);
            }
        }
        spans.extend(open);
        spans
    }
}

/// The end of the visible part of a line (block bytes): its hard break and the
/// spaces a soft wrap leaves hanging past the edge are not drawn.
pub(crate) fn visible_end(text: &str, line: Range<usize>) -> usize {
    let slice = &text[line.clone()];
    if let Some(last) = slice.chars().next_back() {
        if matches!(last, '\n' | '\u{b}' | '\u{c}') {
            return line.end - last.len_utf8();
        }
    }
    if line.end >= text.len() {
        line.end
    } else {
        line.start + slice.trim_end().len()
    }
}

/// The visual stop nearest `x` on the line of `text` whose visible bytes are
/// `visible`, or `None` when the line is laid out left to right.
pub(crate) fn visual_stop_at(
    text: &str,
    measure: &LineMeasure,
    visible: Range<usize>,
    x: f32,
) -> Option<Stop> {
    BidiRuns::of(text)
        .and_then(|bidi| LineOrder::build(&bidi, text, measure, visible))
        .and_then(|order| order.stop_at(x))
}

impl RichBlock {
    /// The visual order of the visible part of `line` (block bytes), or `None`
    /// when the line is laid out left to right.
    pub(crate) fn visual_line(&self, font_size: f32, line: Range<usize>) -> Option<LineOrder> {
        let text = self.text.as_str();
        let bidi = block_bidi_runs(self)?;
        let measure = block_positions(self, font_size);
        let visible = visible_end(text, line.clone());
        LineOrder::build(&bidi, text, measure.as_ref(), line.start..visible)
    }

    /// The offset, relative to `line.start`, of the caret nearest `x` points
    /// from the line's text origin. A right-to-left line is read in the order it
    /// is drawn in; any other line is measured in logical order, as before.
    pub fn line_offset_at_x(&self, font_size: f32, line: Range<usize>, x: f32) -> usize {
        self.line_offset_at_x_with_justification(line, font_size, 0.0, x)
    }
}
