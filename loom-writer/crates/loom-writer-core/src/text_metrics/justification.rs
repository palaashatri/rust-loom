//! Shared byte-offset geometry for justified lines.
//!
//! Only automatic wraps with internal ASCII-space gaps are expanded. The
//! legacy advance-only path defers bidi expansion; the shaped path distributes
//! gaps over actual visual glyph positions. Source ranges stay in logical UTF-8
//! offsets so caret, selection, pointer and drawing code share the expansion.

use std::ops::Range;
use std::rc::Rc;

use loom_text::Alignment;
use unicode_segmentation::UnicodeSegmentation;

use super::{bidi::visible_end, LineMeasure, Style, Uncovered};
use crate::{layout::block_positions, RichBlock};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LineGeometry {
    pub(crate) range: Range<usize>,
    pub(crate) visible_end: usize,
    pub(crate) natural_width: f32,
    pub(crate) width: f32,
    pub(crate) gaps: Vec<(Range<usize>, f32)>,
    pub(crate) deferred_bidi: bool,
}

impl LineGeometry {
    /// Endpoint for caret, hit and selection geometry. A natural-width line
    /// keeps its legacy caret stop after a soft-wrap space; explicit breaks
    /// and lines with expanded gaps stop at the visible text end.
    pub(crate) fn interaction_end(&self, text: &str) -> usize {
        let has_explicit_break = text[self.range.clone()].ends_with(['\n', '\u{b}', '\u{c}']);
        if self.gaps.is_empty() && !has_explicit_break {
            self.range.end
        } else {
            self.visible_end
        }
    }

    pub(crate) fn extra_before(&self, offset: usize) -> f32 {
        self.gaps
            .iter()
            .map(|(range, extra)| {
                if offset <= range.start {
                    0.0
                } else if offset >= range.end {
                    *extra
                } else {
                    *extra * (offset - range.start) as f32 / (range.end - range.start) as f32
                }
            })
            .sum()
    }

    pub(crate) fn extra_between(&self, start: usize, end: usize) -> f32 {
        (self.extra_before(end) - self.extra_before(start)).max(0.0)
    }

    pub(crate) fn offset_at_x(&self, text: &str, measure: &LineMeasure, x: f32) -> usize {
        const SLACK: f32 = 1e-3;
        for (relative, grapheme) in text[self.range.start..self.visible_end].grapheme_indices(true)
        {
            let start = self.range.start + relative;
            let end = start + grapheme.len();
            let left = measure.advance(self.range.start, start) + self.extra_before(start);
            let right = measure.advance(self.range.start, end) + self.extra_before(end);
            if x < (left + right) / 2.0 + SLACK {
                return start;
            }
        }
        self.visible_end
    }
}

/// The common expansion calculation. `natural_width` excludes line-ending
/// bytes and `requested` comes from the original paragraph style.
pub(crate) fn line_geometry(
    text: &str,
    range: Range<usize>,
    natural_width: f32,
    available_width: f32,
    requested: bool,
    bidi_visual: bool,
) -> LineGeometry {
    let visible_end = visible_end(text, range.clone());
    let mut result = LineGeometry {
        range: range.clone(),
        visible_end,
        natural_width,
        width: natural_width,
        gaps: Vec::new(),
        deferred_bidi: false,
    };
    if !requested || !is_automatic_line(text, &range) {
        return result;
    }

    let candidates = eligible_spaces(text, range.start..visible_end);
    if candidates.is_empty() {
        return result;
    }
    if bidi_visual {
        result.deferred_bidi = true;
        return result;
    }
    if !available_width.is_finite()
        || !natural_width.is_finite()
        || available_width <= natural_width
    {
        return result;
    }
    let extra = (available_width - natural_width) / candidates.len() as f32;
    if !extra.is_finite() || extra <= 0.0 {
        return result;
    }
    result.gaps = candidates.into_iter().map(|range| (range, extra)).collect();
    result.width = natural_width + extra * result.gaps.len() as f32;
    result
}

fn is_automatic_line(text: &str, range: &Range<usize>) -> bool {
    range.end < text.len()
        && !text[range.clone()].ends_with(['\n', '\u{b}', '\u{c}'])
        && !text[range.end..].starts_with(['\n', '\u{b}', '\u{c}'])
}

fn eligible_spaces(text: &str, range: Range<usize>) -> Vec<Range<usize>> {
    let chars: Vec<(usize, char)> = text[range.clone()]
        .char_indices()
        .map(|(at, ch)| (range.start + at, ch))
        .collect();
    let mut gaps = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        if chars[index].1 != ' ' {
            index += 1;
            continue;
        }
        let first = index;
        while index < chars.len() && chars[index].1 == ' ' {
            index += 1;
        }
        if first > 0
            && index < chars.len()
            && !chars[first - 1].1.is_whitespace()
            && !chars[index].1.is_whitespace()
        {
            let start = chars[first].0;
            let end = if index < chars.len() {
                chars[index].0
            } else {
                range.end
            };
            gaps.push(start..end);
        }
    }
    gaps
}

/// Expand eligible spaces in a shaped line, including mixed-direction lines.
///
/// The caller supplies the authoritative shaped width. Unlike the legacy
/// estimate path above, this helper does not defer bidi paragraphs: the
/// returned byte ranges are applied to shaped glyph clusters in visual order.
pub(crate) fn shaped_line_gaps(
    text: &str,
    range: Range<usize>,
    natural_width: f32,
    available_width: f32,
    requested: bool,
) -> Vec<(Range<usize>, f32)> {
    if !requested || !is_automatic_line(text, &range) {
        return Vec::new();
    }
    let visible_end = visible_end(text, range.clone());
    let candidates = eligible_spaces(text, range.start..visible_end);
    if candidates.is_empty()
        || !natural_width.is_finite()
        || !available_width.is_finite()
        || available_width <= natural_width
    {
        return Vec::new();
    }
    let extra = (available_width - natural_width) / candidates.len() as f32;
    if !extra.is_finite() || extra <= 0.0 {
        return Vec::new();
    }
    candidates.into_iter().map(|range| (range, extra)).collect()
}

/// Legacy metric-path justification spaces for an original block line,
/// including whether the current line needs the deferred-bidi notice. App
/// fragments call this on the unsplit block so a copied fragment cannot turn
/// a wrapped line into a final line. Shaped rendering consumes gaps from its
/// visual glyph geometry instead.
pub fn paragraph_line_spaces(
    block: &RichBlock,
    line: Range<usize>,
    font_size: f32,
    available_width: f32,
) -> (Vec<(Range<usize>, f32)>, bool) {
    let text = block.text.as_str();
    let requested = block.style.alignment.resolve(text) == Alignment::Justify;
    let measure = block_positions(block, font_size);
    let geometry =
        line_geometry_with_measure(block, line, measure.as_ref(), available_width, requested);
    (geometry.gaps, geometry.deferred_bidi)
}

impl RichBlock {
    /// Whether justification is deferred in the legacy unshaped metric path
    /// because the block contains bidi text. The canonical shaped path handles
    /// bidi glyph positions and does not use this result as a limitation.
    /// The query shares the same paragraph bidi cache used by line layout.
    pub fn justification_deferred_for_bidi(&self) -> bool {
        crate::layout::block_bidi_runs(self).is_some()
    }

    /// Legacy expansion ranges for a source line, in this block's byte
    /// offsets, plus whether bidi expansion is deferred by the estimate path.
    /// The input range must refer to this original block, not a rendered line
    /// fragment, so final-line eligibility remains correct. Shaped consumers
    /// use the gaps attached to the canonical visual line.
    pub fn justification_spaces_for_line(
        &self,
        line: Range<usize>,
        font_size: f32,
        available_width: f32,
    ) -> (Vec<(Range<usize>, f32)>, bool) {
        paragraph_line_spaces(self, line, font_size, available_width)
    }

    /// The caret byte nearest `x` in a line, using shaped geometry when
    /// available. The legacy estimate fallback keeps its existing bidi visual
    /// order without distributing RTL justification gaps.
    pub fn line_offset_at_x_with_justification(
        &self,
        line: Range<usize>,
        font_size: f32,
        available_width: f32,
        x: f32,
    ) -> usize {
        paragraph_line_offset_at_x(self, line, font_size, available_width, x)
    }

    /// Shaped width for a block range using the face the page draws for the
    /// block and its inline runs, including run-specific sizes.
    pub fn measured_advance(&self, start: usize, end: usize, font_size: f32) -> f32 {
        crate::layout::shape_text_line(
            self.text.as_str(),
            &self.runs,
            font_size,
            self.kind.as_str(),
            self.style.direction,
            Alignment::Left,
            start..end,
            f32::MAX / 4.0,
        )
        .map_or_else(
            || block_positions(self, font_size).advance(start, end),
            |line| line.natural_width_pt(),
        )
    }
}

/// The nearest caret byte for an original block line, using shaped visual
/// geometry when available. The fallback estimate path does not distribute
/// bidi gaps and retains its visual-order mapping.
pub fn paragraph_line_offset_at_x(
    block: &RichBlock,
    line: Range<usize>,
    font_size: f32,
    available_width: f32,
    x: f32,
) -> usize {
    if let Some(shaped) =
        crate::layout::shape_block_line(block, line.clone(), font_size, available_width)
    {
        return shaped
            .hit_test(x + shaped.alignment_shift_pt())
            .map_or(0, |stop| stop.offset.saturating_sub(line.start));
    }
    if let Some(stop) = block
        .visual_line(font_size, line.clone())
        .and_then(|order| order.stop_at(x))
    {
        return stop.offset - line.start;
    }
    let text = block.text.as_str();
    let measure = block_positions(block, font_size);
    let geometry = line_geometry_with_measure(
        block,
        line.clone(),
        measure.as_ref(),
        available_width,
        block.style.alignment.resolve(text) == Alignment::Justify,
    );
    if geometry.gaps.is_empty() {
        let hit_end = geometry.interaction_end(text);
        measure
            .offset_at_x(x + measure.advance(0, line.start), hit_end)
            .clamp(line.start, hit_end)
            - line.start
    } else {
        geometry.offset_at_x(text, &measure, x) - line.start
    }
}

/// Make a line's measurement and geometry using the page's current metric
/// model. Kept here so layout, caret and selection cannot invent separate
/// space calculations.
pub(crate) fn block_line_geometry(
    block: &RichBlock,
    line: Range<usize>,
    font_size: f32,
    available_width: f32,
) -> (LineGeometry, Rc<LineMeasure>) {
    let text = block.text.as_str();
    let measure = block_positions(block, font_size);
    let geometry = line_geometry_with_measure(
        block,
        line,
        measure.as_ref(),
        available_width,
        block.style.alignment.resolve(text) == Alignment::Justify,
    );
    (geometry, measure)
}

pub(crate) fn measure_block(block: &RichBlock, font_size: f32) -> LineMeasure {
    LineMeasure::new(
        block.text.as_str(),
        0,
        &block.runs,
        font_size,
        Style::new(crate::is_heading_kind(block.kind.as_str()), false),
        Uncovered::Estimate,
    )
}

pub(crate) fn line_geometry_with_measure(
    block: &RichBlock,
    line: Range<usize>,
    measure: &LineMeasure,
    available_width: f32,
    requested: bool,
) -> LineGeometry {
    let text = block.text.as_str();
    let visible_end = visible_end(text, line.clone());
    let paragraph_bidi = requested
        && is_automatic_line(text, &line)
        && !eligible_spaces(text, line.start..visible_end).is_empty()
        && crate::layout::block_bidi_runs(block).is_some();
    line_geometry(
        text,
        line.clone(),
        measure.advance(line.start, visible_end),
        available_width,
        requested,
        paragraph_bidi,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_internal_ascii_space_runs_are_expanded() {
        let text = "  one  two three\u{a0}four five\tsix  ";
        let line = 0..text.len();
        let geometry = line_geometry(text, line, 20.0, 100.0, true, false);
        assert!(
            geometry.gaps.is_empty(),
            "paragraph-final line stays natural"
        );

        let text = "prefix:  one  two three\u{a0}four five\tsix  next";
        let start = "prefix: ".len();
        let range = start..text.len() - 5;
        let geometry = line_geometry(text, range.clone(), 20.0, 100.0, true, false);
        assert_eq!(
            geometry
                .gaps
                .iter()
                .map(|(gap, _)| &text[gap.clone()])
                .collect::<Vec<_>>(),
            ["  ", " ", " "]
        );
        assert!(geometry
            .gaps
            .iter()
            .all(|(range, _)| { range.start >= start && range.end < text.len() }));
        assert_eq!(
            geometry.visible_end,
            "prefix:  one  two three\u{a0}four five\tsix".len()
        );
    }

    #[test]
    fn final_explicit_break_and_single_word_lines_remain_natural() {
        for (text, range) in [
            ("one two", 0..7),
            ("one two\nnext", 0..8),
            ("one\nnext", 0..4),
            ("one\nnext", 0..3),
        ] {
            let geometry = line_geometry(text, range, 24.0, 100.0, true, false);
            assert!(geometry.gaps.is_empty(), "{text:?}: {geometry:?}");
            assert_eq!(geometry.width, geometry.natural_width);
        }
        let text = "prefix: one two\nnext";
        let range = "prefix: ".len()..text.find('\n').unwrap();
        let geometry = line_geometry(text, range, 24.0, 100.0, true, false);
        assert!(
            geometry.gaps.is_empty(),
            "a range ending before an explicit break is natural"
        );

        let text = "prefix: word next";
        let range = "prefix: ".len()..text.find(" next").unwrap() + 1;
        let geometry = line_geometry(text, range, 24.0, 100.0, true, false);
        assert!(
            geometry.gaps.is_empty(),
            "single-word auto lines stay natural"
        );
    }

    #[test]
    fn rtl_justification_is_deferred_without_changing_line_geometry() {
        let text = "one two אבג next";
        let natural = 42.0;
        let geometry = line_geometry(text, 0..8, natural, 100.0, true, true);
        assert!(geometry.gaps.is_empty());
        assert!(geometry.deferred_bidi);
        assert_eq!(geometry.width, natural);
    }

    #[test]
    fn line_local_hits_clamp_to_a_nonzero_start_for_left_and_right_points() {
        let text = "prefix: one two next";
        let start = "prefix: ".len();
        let end = text.find(" next").unwrap() + 1;
        let mut block = RichBlock::new(1, "paragraph", text);
        let font_size = 12.0;
        block.style.alignment = Alignment::Left;
        assert_eq!(
            block.line_offset_at_x_with_justification(start..end, font_size, 80.0, -500.0),
            0,
            "a point left of a continuation line belongs to that line's start"
        );
        assert_eq!(
            block.line_offset_at_x_with_justification(start..end, font_size, 80.0, 10_000.0),
            "one two ".len(),
            "a natural line preserves its trailing soft-wrap space caret stop"
        );
    }
}
