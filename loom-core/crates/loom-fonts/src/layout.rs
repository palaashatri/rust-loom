//! A laid out line: shaped runs in visual order plus the caret index built
//! from them, so hit-testing, caret placement and selection rectangles are
//! answered from tables computed once.

use crate::face::{Decoration, Variation};
use crate::info::FaceId;
use crate::shaped::{CaretStop, Shaped};
use crate::work::{self, lower_bound};
use std::ops::Range;

/// Which character a caret at a logical offset belongs to. At most places
/// the two choices are the same point on screen; at a bidi boundary they are
/// two different points, and an editor must remember which one the user is at.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Affinity {
    /// The leading edge of the character that starts at the offset (the
    /// caret belongs to the character after it). Also what a hit test reports
    /// when both choices are the same point.
    Leading,
    /// The trailing edge of the character that ends at the offset (the caret
    /// belongs to the character before it).
    Trailing,
}

/// One place a caret can sit on a line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CaretPosition {
    /// Logical byte offset.
    pub offset: usize,
    /// Which side of the boundary the caret belongs to.
    pub affinity: Affinity,
    /// Distance from the line's left edge.
    pub x: f32,
}

/// A horizontal extent of a selection on one line. The caller supplies the
/// vertical extent from the line's baseline, `ascent` and `descent`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SelectionRect {
    /// Distance from the line's left edge.
    pub x: f32,
    /// Extent to the right of `x`.
    pub width: f32,
}

/// A shaped piece of a line drawn with one face, size and style.
#[derive(Clone, Debug, PartialEq)]
pub struct LaidOutRun {
    /// Byte range of the paragraph text this run covers (of the line text for
    /// [`crate::FontCatalog::layout_line`]).
    pub range: Range<usize>,
    /// The face the run was shaped with.
    pub face: FaceId,
    /// True when the run is right to left.
    pub rtl: bool,
    /// Glyphs; `cluster` values are relative to `range.start`.
    pub shaped: Shaped,
    /// Distance from the line's left edge to the run's left edge.
    pub x: f32,
    /// Index of the [`crate::StyledRun`] / [`crate::StyledSpan`] the text
    /// came from.
    pub style: usize,
    /// Font size the run was shaped at.
    pub size: f32,
    /// This face is lighter than the requested bold with no named instance to
    /// supply it; a renderer may embolden. Judged per face, so a fallback face
    /// that has a real bold is not emboldened.
    pub synthetic_bold: bool,
    /// This face has no italic; a renderer may slant.
    pub synthetic_italic: bool,
    /// Variable-font coordinates the run was shaped with; draw the glyphs at
    /// the same coordinates.
    pub variations: Vec<Variation>,
    /// Ascent of this run's face at its size.
    pub ascent: f32,
    /// Descent of this run's face at its size.
    pub descent: f32,
    /// Line gap the run's face recommends at its size.
    pub line_gap: f32,
    /// Underline placement for this face and size, when it records one.
    pub underline: Option<Decoration>,
    /// Strikeout placement for this face and size, when it records one.
    pub strikeout: Option<Decoration>,
    /// Caret stops of this run: absolute byte offsets, x from the line's left
    /// edge, ascending by offset.
    pub caret_stops: Vec<CaretStop>,
}

#[derive(Clone, Debug, PartialEq)]
struct Boundary {
    offset: usize,
    leading: f32,
    trailing: f32,
}

/// Caret lookup tables for one line.
#[derive(Clone, Debug, PartialEq, Default)]
struct LineIndex {
    /// One entry per caret boundary, ascending by offset.
    boundaries: Vec<Boundary>,
    /// Every distinct caret position, ascending by x (ties by offset).
    visual: Vec<CaretPosition>,
}

impl LineIndex {
    fn from_boundaries(boundaries: Vec<Boundary>) -> Self {
        let mut visual = Vec::with_capacity(boundaries.len() * 2);
        for boundary in &boundaries {
            visual.push(CaretPosition {
                offset: boundary.offset,
                affinity: Affinity::Leading,
                x: boundary.leading,
            });
            if boundary.trailing != boundary.leading {
                visual.push(CaretPosition {
                    offset: boundary.offset,
                    affinity: Affinity::Trailing,
                    x: boundary.trailing,
                });
            }
        }
        visual.sort_by(|a, b| {
            work::add_steps(1);
            a.x.total_cmp(&b.x).then(a.offset.cmp(&b.offset))
        });
        Self { boundaries, visual }
    }

    /// Builds the index from runs given in visual order.
    ///
    /// Taken in logical order, each run contributes the stops of its own
    /// characters; where two runs meet the earlier one's last stop is the
    /// boundary's trailing position and the later one's first stop its
    /// leading position.
    fn from_runs(runs: &[LaidOutRun], line_start: usize) -> Self {
        let mut order: Vec<usize> = (0..runs.len()).collect();
        order.sort_by_key(|&i| runs[i].range.start);
        let mut boundaries: Vec<Boundary> = Vec::new();
        for &i in &order {
            for stop in &runs[i].caret_stops {
                work::add_steps(1);
                match boundaries.last_mut() {
                    Some(last) if last.offset == stop.offset => last.leading = stop.x,
                    _ => boundaries.push(Boundary {
                        offset: stop.offset,
                        leading: stop.x,
                        trailing: stop.x,
                    }),
                }
            }
        }
        if boundaries.is_empty() {
            return Self::single(line_start, 0.0);
        }
        Self::from_boundaries(boundaries)
    }

    fn single(offset: usize, x: f32) -> Self {
        Self::from_boundaries(vec![Boundary {
            offset,
            leading: x,
            trailing: x,
        }])
    }

    /// An index for text nothing could be measured for: half an em per
    /// character, so a caret can still be placed and clicked.
    fn estimated(text: &str, line: &Range<usize>, widths: &dyn Fn(usize, char) -> f32) -> Self {
        let mut boundaries = Vec::new();
        let mut x = 0.0_f32;
        for (i, ch) in text[line.clone()].char_indices() {
            let offset = line.start + i;
            boundaries.push(Boundary {
                offset,
                leading: x,
                trailing: x,
            });
            x += widths(offset, ch);
        }
        boundaries.push(Boundary {
            offset: line.end,
            leading: x,
            trailing: x,
        });
        Self::from_boundaries(boundaries)
    }
}

/// A laid out line. Runs are in visual (left to right) order.
#[derive(Clone, Debug, PartialEq)]
pub struct LineLayout {
    /// The part of the paragraph text this line covers.
    pub range: Range<usize>,
    /// The runs, left to right.
    pub runs: Vec<LaidOutRun>,
    /// Total advance width.
    pub width: f32,
    /// Largest run ascent on the line (positive, above the baseline). For an
    /// empty line, the ascent of the style at its position, so an empty
    /// paragraph still has a caret height.
    pub ascent: f32,
    /// Largest run descent on the line (positive, below the baseline).
    pub descent: f32,
    /// Largest line gap recommended by the faces on the line.
    pub line_gap: f32,
    /// True when no face could be loaded and `width` is a rough estimate
    /// (half an em per character); the line has no runs.
    pub estimated: bool,
    index: LineIndex,
}

impl LineLayout {
    /// A line with no glyphs: `range` is the (possibly empty) text it covers.
    pub(crate) fn blank(range: Range<usize>, ascent: f32, descent: f32, line_gap: f32) -> Self {
        let at = range.start;
        Self {
            range,
            runs: Vec::new(),
            width: 0.0,
            ascent,
            descent,
            line_gap,
            estimated: false,
            index: LineIndex::single(at, 0.0),
        }
    }

    /// An estimate for a line nothing could measure.
    pub(crate) fn estimated(
        text: &str,
        range: Range<usize>,
        widths: &dyn Fn(usize, char) -> f32,
        ascent: f32,
        descent: f32,
    ) -> Self {
        let index = LineIndex::estimated(text, &range, widths);
        let width = index.boundaries.last().map_or(0.0, |b| b.leading);
        Self {
            range,
            runs: Vec::new(),
            width,
            ascent,
            descent,
            line_gap: 0.0,
            estimated: true,
            index,
        }
    }

    /// Assembles a layout from shaped runs (visual order).
    pub(crate) fn from_runs(range: Range<usize>, runs: Vec<LaidOutRun>) -> Self {
        let width = runs.last().map_or(0.0, |r| r.x + r.shaped.width);
        let ascent = runs.iter().map(|r| r.ascent).fold(0.0, f32::max);
        let descent = runs.iter().map(|r| r.descent).fold(0.0, f32::max);
        let line_gap = runs.iter().map(|r| r.line_gap).fold(0.0, f32::max);
        let index = LineIndex::from_runs(&runs, range.start);
        Self {
            range,
            runs,
            width,
            ascent,
            descent,
            line_gap,
            estimated: false,
            index,
        }
    }

    /// `ascent + descent + line_gap`: the line's natural height at 100% line
    /// spacing.
    pub fn line_height(&self) -> f32 {
        self.ascent + self.descent + self.line_gap
    }

    /// The caret x for byte `offset` of the paragraph text.
    ///
    /// An offset inside a grapheme cluster (or a ligature that cannot be
    /// split) maps to the next boundary; offsets outside the line clamp to
    /// its ends. At a bidi boundary `affinity` picks which of the two carets
    /// is meant; elsewhere both return the same x. `O(log n)`.
    pub fn x_at_offset(&self, offset: usize, affinity: Affinity) -> f32 {
        let boundaries = &self.index.boundaries;
        let Some(last) = boundaries.last() else {
            return 0.0;
        };
        let at = lower_bound(boundaries, |b| b.offset < offset);
        let boundary = boundaries.get(at).unwrap_or(last);
        match affinity {
            Affinity::Leading => boundary.leading,
            Affinity::Trailing => boundary.trailing,
        }
    }

    /// The caret nearest to horizontal position `x` (distance from the
    /// line's left edge): its byte offset and which side of the boundary it
    /// sits on. Positions beyond either end clamp to the first or last
    /// caret. Where two logical positions share one point on screen the lower
    /// offset is reported, and a point that is the same for both affinities
    /// reports [`Affinity::Leading`]. `O(log n)`.
    pub fn offset_at_x(&self, x: f32) -> (usize, Affinity) {
        let visual = &self.index.visual;
        let at = lower_bound(visual, |p| p.x < x);
        let below = at.checked_sub(1).map(|i| visual[i]);
        let above = visual.get(at).copied();
        let best = match (below, above) {
            (Some(b), Some(a)) => {
                if (x - b.x).abs() <= (a.x - x).abs() {
                    b
                } else {
                    a
                }
            }
            (Some(only), None) | (None, Some(only)) => only,
            (None, None) => return (self.range.start, Affinity::Leading),
        };
        (best.offset, best.affinity)
    }

    /// Every caret position on the line, left to right. Arrow-key movement in
    /// visual order steps through this list.
    pub fn caret_positions(&self) -> &[CaretPosition] {
        &self.index.visual
    }

    /// The horizontal extents covered by the logical byte `range`, left to
    /// right, touching extents merged. A range crossing a bidi boundary can
    /// give several. Parts of the range outside the line are ignored.
    pub fn selection_rects(&self, range: Range<usize>) -> Vec<SelectionRect> {
        let start = range.start.max(self.range.start);
        let end = range.end.min(self.range.end);
        let mut spans: Vec<(f32, f32)> = Vec::new();
        if start >= end {
            return Vec::new();
        }
        for run in &self.runs {
            let from = start.max(run.range.start);
            let to = end.min(run.range.end);
            if from >= to {
                continue;
            }
            let stop_x = |offset: usize| {
                let at = lower_bound(&run.caret_stops, |s| s.offset < offset);
                run.caret_stops
                    .get(at)
                    .or(run.caret_stops.last())
                    .map_or(run.x, |s| s.x)
            };
            let (a, b) = (stop_x(from), stop_x(to));
            if a != b {
                spans.push((a.min(b), a.max(b)));
            }
        }
        spans.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut merged: Vec<(f32, f32)> = Vec::new();
        for (left, right) in spans {
            match merged.last_mut() {
                Some(last) if left <= last.1 + 0.01 => last.1 = last.1.max(right),
                _ => merged.push((left, right)),
            }
        }
        merged
            .into_iter()
            .map(|(left, right)| SelectionRect {
                x: left,
                width: right - left,
            })
            .collect()
    }
}
