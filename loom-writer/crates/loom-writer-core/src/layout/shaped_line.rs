//! Writer's transient, shared geometry for a shaped visual line.
//!
//! LineLayout is built once from the full paragraph direction and styled
//! spans. This wrapper keeps the positioned paint runs and the caret/selection
//! index together so screen rendering, pointer input and PDF export can consume
//! the same glyph positions.

use std::collections::BTreeMap;
use std::ops::Range;
use std::sync::Arc;

use loom_fonts::{
    line_break_opportunities, Affinity, Decoration, FontCatalog, FontRef, LaidOutRun, LineLayout,
    LoadedFace, Paragraph, Shaped, StyledSpan, TextDirection, Variation,
};
use loom_text::{Alignment, CharacterStyle, ParagraphDirection, StyleRun};
use unicode_segmentation::UnicodeSegmentation;

use crate::text_metrics::justification::shaped_line_gaps;
use crate::{fonts, is_heading_kind, run_size, RichBlock};

/// A shaped run with the exact font program, glyph positions and paint needed
/// by Writer's screen and PDF consumers. `x_pt` includes paragraph alignment
/// and any justification expansion. `rgba` and `background_rgba` are straight
/// alpha; raster tiles are premultiplied at the paint boundary.
#[derive(Clone, Debug)]
pub struct WriterPaintedRun {
    /// Absolute UTF-8 byte range in the source paragraph (or cell text).
    pub source_range: Range<usize>,
    /// The parsed face used to shape and rasterize these glyphs.
    pub face: Arc<LoadedFace>,
    /// Glyph IDs in visual order, including expanded advances when justified.
    pub shaped: Shaped,
    /// Visual-left x from the text-column origin, in page points.
    pub x_pt: f32,
    /// Effective font size used by shaping and paint, in points.
    pub size_pt: f32,
    /// The stored/inherited size before super/subscript scaling.
    pub requested_size_pt: f32,
    /// Face ascent at the effective size.
    pub ascent_pt: f32,
    /// Face descent at the effective size.
    pub descent_pt: f32,
    /// Font variations used by shaping and rasterization.
    pub variations: Vec<Variation>,
    /// Whether paint applies synthetic bold without changing glyph advances.
    pub synthetic_bold: bool,
    /// Whether paint applies synthetic italic without changing glyph advances.
    pub synthetic_italic: bool,
    /// Straight-alpha foreground color.
    pub rgba: [u8; 4],
    /// True when a valid foreground color came from the character style.
    /// When false, `rgba` contains the canonical black document default.
    pub foreground_color_explicit: bool,
    /// Optional straight-alpha background color for the run.
    pub background_rgba: Option<[u8; 4]>,
    /// Positive values move the run baseline up from the shared line baseline.
    pub baseline_shift_pt: f32,
    /// Draw underline using the face metric when available.
    pub underline: bool,
    /// Draw strikeout using the face metric when available.
    pub strikethrough: bool,
    /// Face underline placement, when present.
    pub underline_metric: Option<Decoration>,
    /// Face strikeout placement, when present.
    pub strikeout_metric: Option<Decoration>,
    /// Absolute source offsets and expanded x positions for this run.
    pub caret_stops: Vec<WriterCaretStop>,
}

impl PartialEq for WriterPaintedRun {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.face, &other.face)
            && self.source_range == other.source_range
            && self.shaped == other.shaped
            && self.x_pt == other.x_pt
            && self.size_pt == other.size_pt
            && self.requested_size_pt == other.requested_size_pt
            && self.ascent_pt == other.ascent_pt
            && self.descent_pt == other.descent_pt
            && self.variations == other.variations
            && self.synthetic_bold == other.synthetic_bold
            && self.synthetic_italic == other.synthetic_italic
            && self.rgba == other.rgba
            && self.foreground_color_explicit == other.foreground_color_explicit
            && self.background_rgba == other.background_rgba
            && self.baseline_shift_pt == other.baseline_shift_pt
            && self.underline == other.underline
            && self.strikethrough == other.strikethrough
            && self.underline_metric == other.underline_metric
            && self.strikeout_metric == other.strikeout_metric
            && self.caret_stops == other.caret_stops
    }
}

/// One visual caret stop, ordered by source offset and physical x as needed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WriterCaretStop {
    /// Absolute UTF-8 byte offset in the source text.
    pub offset: usize,
    /// Which side of a bidi boundary this stop represents.
    pub affinity: Affinity,
    /// X from the line's text-column origin, including justification.
    pub x_pt: f32,
}

/// One visual selection span that retains its logical source bytes.
#[derive(Clone, Debug, PartialEq)]
pub struct WriterSelectionSpan {
    /// Absolute UTF-8 byte range in the source paragraph or cell text.
    pub source_range: Range<usize>,
    /// Left edge from the text-column origin, including alignment.
    pub x_pt: f32,
    /// Painted width in points.
    pub width_pt: f32,
}

/// One authoritative shaped visual line used by Writer geometry consumers.
/// Natural widths are retained for diagnostics; public paint/caret geometry
/// uses the expanded and aligned positions.
#[derive(Clone, Debug, PartialEq)]
pub struct WriterShapedVisualLine {
    source_range: Range<usize>,
    visible_end: usize,
    interaction_end: usize,
    natural_width_pt: f32,
    width_pt: f32,
    alignment_shift_pt: f32,
    ascent_pt: f32,
    descent_pt: f32,
    line_gap_pt: f32,
    estimated: bool,
    contains_rtl: bool,
    painted_runs: Vec<WriterPaintedRun>,
    carets_by_offset: Vec<(usize, f32, f32)>,
    visual_carets: Vec<WriterCaretStop>,
}

#[derive(Clone)]
struct SpanStyle {
    range: Range<usize>,
    style: CharacterStyle,
    requested_size_pt: f32,
    effective_size_pt: f32,
    is_superscript: bool,
    is_subscript: bool,
}

impl WriterShapedVisualLine {
    /// The source bytes covered by this wrapped line.
    pub fn source_range(&self) -> Range<usize> {
        self.source_range.clone()
    }

    /// Natural line width before justification expansion.
    pub fn natural_width_pt(&self) -> f32 {
        self.natural_width_pt
    }

    /// Painted line width, including justification expansion.
    pub fn width_pt(&self) -> f32 {
        self.width_pt
    }

    /// X shift from the text-column origin for center/right/logical alignment.
    pub fn alignment_shift_pt(&self) -> f32 {
        self.alignment_shift_pt
    }

    /// Top extent above the shared line baseline, including superscripts.
    pub fn ascent_pt(&self) -> f32 {
        self.ascent_pt
    }

    /// Bottom extent below the shared line baseline, including subscripts.
    pub fn descent_pt(&self) -> f32 {
        self.descent_pt
    }

    /// Largest line gap among shaped runs.
    pub fn line_gap_pt(&self) -> f32 {
        self.line_gap_pt
    }

    /// True when the font catalog could not provide shaped glyph runs.
    pub fn estimated(&self) -> bool {
        self.estimated
    }

    /// The glyph runs in visual-left-to-right paint order.
    pub fn painted_runs(&self) -> &[WriterPaintedRun] {
        &self.painted_runs
    }

    /// Caret x from the text-column origin for a source offset and affinity.
    pub fn caret_x(&self, offset: usize, affinity: Affinity) -> f32 {
        let offset = offset.clamp(self.source_range.start, self.interaction_end);
        let at = self
            .carets_by_offset
            .partition_point(|(known, _, _)| *known < offset);
        let Some(&(known, leading, trailing)) = self
            .carets_by_offset
            .get(at)
            .or_else(|| self.carets_by_offset.last())
        else {
            return self.alignment_shift_pt;
        };
        let x = if known == offset {
            match affinity {
                Affinity::Leading => leading,
                Affinity::Trailing => trailing,
            }
        } else {
            leading
        };
        x
    }

    /// The closest shaped caret stop to an x coordinate from the text-column
    /// origin. Ties retain the lower logical byte offset, matching LineLayout.
    pub fn hit_test(&self, x_pt: f32) -> Option<WriterCaretStop> {
        let x = x_pt;
        let at = self.visual_carets.partition_point(|stop| stop.x_pt < x);
        let below = at.checked_sub(1).and_then(|i| self.visual_carets.get(i));
        let above = self.visual_carets.get(at);
        match (below, above) {
            (Some(left), Some(right)) => {
                if (x - left.x_pt).abs() <= (right.x_pt - x).abs() {
                    Some(*left)
                } else {
                    Some(*right)
                }
            }
            (Some(only), None) | (None, Some(only)) => Some(*only),
            (None, None) => None,
        }
    }

    /// The neighboring caret stop in visual order, if one exists.
    pub fn visual_neighbor(
        &self,
        offset: usize,
        affinity: Affinity,
        forward: bool,
    ) -> Option<WriterCaretStop> {
        let at = self
            .visual_carets
            .iter()
            .position(|stop| stop.offset == offset && stop.affinity == affinity)
            .or_else(|| {
                self.visual_carets
                    .iter()
                    .position(|stop| stop.offset == offset)
            })?;
        if forward {
            self.visual_carets.get(at + 1).copied()
        } else {
            at.checked_sub(1)
                .and_then(|previous| self.visual_carets.get(previous).copied())
        }
    }

    /// Selection extents in visual order. The source range remains logical.
    pub fn selection_rects(&self, range: Range<usize>) -> Vec<loom_fonts::SelectionRect> {
        let mut spans: Vec<_> = self
            .selection_spans(range)
            .into_iter()
            .map(|span| (span.x_pt, span.x_pt + span.width_pt))
            .collect();
        spans.sort_by(|left, right| left.0.total_cmp(&right.0));
        let mut merged: Vec<(f32, f32)> = Vec::new();
        for (left, right) in spans {
            match merged.last_mut() {
                Some(last) if left <= last.1 + 0.01 => last.1 = last.1.max(right),
                _ => merged.push((left, right)),
            }
        }
        merged
            .into_iter()
            .map(|(left, right)| loom_fonts::SelectionRect {
                x: left,
                width: right - left,
            })
            .collect()
    }

    /// Selection spans in visual order, each retaining its exact logical
    /// source range for downstream overlays and comment geometry.
    pub fn selection_spans(&self, range: Range<usize>) -> Vec<WriterSelectionSpan> {
        let start = range.start.max(self.source_range.start);
        let selection_end = if self.contains_rtl {
            self.visible_end
        } else {
            self.interaction_end
        };
        let end = range.end.min(selection_end);
        if start >= end {
            return Vec::new();
        }
        let mut spans = Vec::new();
        for run in &self.painted_runs {
            let from = start.max(run.source_range.start);
            let to = end.min(run.source_range.end);
            if from >= to {
                continue;
            }
            let x_at = |offset: usize| {
                let at = run.caret_stops.partition_point(|stop| stop.offset < offset);
                run.caret_stops
                    .get(at)
                    .or_else(|| run.caret_stops.last())
                    .map_or(run.x_pt, |stop| stop.x_pt)
            };
            let (left, right) = (x_at(from), x_at(to));
            if (left - right).abs() > f32::EPSILON {
                spans.push(WriterSelectionSpan {
                    source_range: from..to,
                    x_pt: left.min(right),
                    width_pt: (right - left).abs(),
                });
            }
        }
        spans.sort_by(|left, right| left.x_pt.total_cmp(&right.x_pt));
        spans
    }
}

/// Shape one block line using paragraph direction, run styles and the current
/// Writer font catalog. `available_width_pt` is the text-column width before
/// alignment; justification is applied only to automatic non-final lines.
pub(crate) fn shape_block_line(
    block: &RichBlock,
    range: Range<usize>,
    base_size_pt: f32,
    available_width_pt: f32,
) -> Option<WriterShapedVisualLine> {
    shape_text_line(
        block.text.as_str(),
        &block.runs,
        base_size_pt,
        block.kind.as_str(),
        block.style.direction,
        block.style.alignment,
        range,
        available_width_pt,
    )
}

/// Shape all the wrapped ranges of a block from one paragraph context. The
/// bidi levels, script items and fallback spans therefore come from one
/// source paragraph even when its lines are laid out separately.
pub(crate) fn shape_block_lines(
    block: &RichBlock,
    ranges: &[Range<usize>],
    base_size_pt: f32,
    available_width_pt: f32,
) -> Vec<Option<WriterShapedVisualLine>> {
    shape_text_ranges(
        block.text.as_str(),
        &block.runs,
        base_size_pt,
        block.kind.as_str(),
        block.style.direction,
        block.style.alignment,
        ranges,
        available_width_pt,
    )
}

/// Wrap and shape one block from its styled paragraph geometry. Candidate line
/// ends come from UAX #14; an overlong unbreakable word falls back to grapheme
/// boundaries. The returned ranges and glyph geometry are both authoritative.
pub fn shape_wrapped_block_lines(
    block: &RichBlock,
    base_size_pt: f32,
    available_width_pt: f32,
) -> Option<Vec<(Range<usize>, Option<WriterShapedVisualLine>)>> {
    let text = block.text.as_str();
    if text.is_empty() {
        return Some(vec![(0..0, None)]);
    }
    if !base_size_pt.is_finite() || base_size_pt <= 0.0 || !available_width_pt.is_finite() {
        return None;
    }

    let catalog = fonts::font_catalog();
    let span_styles = span_styles(text, &block.runs, base_size_pt);
    if span_styles.is_empty() {
        return None;
    }
    let mut font_refs: Vec<FontRef> = Vec::with_capacity(span_styles.len());
    for span in &span_styles {
        let heading = is_heading_kind(block.kind.as_str());
        let bold = heading || span.style.weight.numeric() >= loom_text::FontWeight::Bold.numeric();
        let family = if span.style.font_family.trim().is_empty() {
            fonts::DEFAULT_FAMILY
        } else {
            span.style.font_family.as_str()
        };
        font_refs.push(
            catalog
                .resolve(family, if bold { 700 } else { 400 }, span.style.italic)
                .ok()?,
        );
    }
    let styled_spans: Vec<StyledSpan<'_>> = span_styles
        .iter()
        .zip(&font_refs)
        .map(|(style, font)| StyledSpan::new(style.range.clone(), font, style.effective_size_pt))
        .collect();
    let base = match block.style.direction {
        ParagraphDirection::Auto => TextDirection::Auto,
        ParagraphDirection::LeftToRight => TextDirection::LeftToRight,
        ParagraphDirection::RightToLeft => TextDirection::RightToLeft,
    };
    let paragraph = Paragraph::new(text, &styled_spans, base);
    let ranges = shaped_wrap_ranges(text, available_width_pt, &catalog, &paragraph)?;
    let mut lines = Vec::with_capacity(ranges.len());
    for range in ranges {
        if range.is_empty() {
            lines.push((range, None));
            continue;
        }
        let shaped = shape_line_from_paragraph(
            text,
            &span_styles,
            &catalog,
            &paragraph,
            block.style.direction,
            block.style.alignment,
            range.clone(),
            available_width_pt,
        )?;
        lines.push((range, Some(shaped)));
    }
    Some(lines)
}

fn shaped_wrap_ranges(
    text: &str,
    available_width_pt: f32,
    catalog: &FontCatalog,
    paragraph: &Paragraph<'_>,
) -> Option<Vec<Range<usize>>> {
    let mut ranges = Vec::new();
    let mut segment_start = 0;
    for (at, grapheme) in text.grapheme_indices(true) {
        if !is_hard_break_grapheme(grapheme) {
            continue;
        }
        let break_end = at + grapheme.len();
        let segment_line_start = ranges.len();
        wrap_segment(
            text,
            segment_start..at,
            available_width_pt,
            catalog,
            paragraph,
            &mut ranges,
        )?;
        if ranges.len() > segment_line_start {
            let last = ranges.last_mut()?;
            last.end = break_end;
        } else {
            ranges.push(segment_start..break_end);
        }
        segment_start = break_end;
    }
    wrap_segment(
        text,
        segment_start..text.len(),
        available_width_pt,
        catalog,
        paragraph,
        &mut ranges,
    )?;
    if segment_start == text.len() {
        // A final explicit break leaves a real empty visual line after it.
        ranges.push(text.len()..text.len());
    }
    if ranges.is_empty() {
        ranges.push(0..0);
    }
    Some(ranges)
}

fn wrap_segment(
    text: &str,
    segment: Range<usize>,
    available_width_pt: f32,
    catalog: &FontCatalog,
    paragraph: &Paragraph<'_>,
    ranges: &mut Vec<Range<usize>>,
) -> Option<()> {
    if segment.is_empty() {
        return Some(());
    }
    let segment_text = &text[segment.clone()];
    let legal_ends: Vec<usize> = line_break_opportunities(segment_text)
        .into_iter()
        .map(|opportunity| segment.start + opportunity.offset)
        .collect();
    let grapheme_ends: Vec<usize> = segment_text
        .grapheme_indices(true)
        .map(|(at, grapheme)| segment.start + at + grapheme.len())
        .collect();
    let mut line_start = segment.start;
    while line_start < segment.end {
        let mut last_fitting_legal_end = None;
        for candidate_end in legal_ends.iter().copied().filter(|end| *end > line_start) {
            let width = shaped_ink_width(text, line_start..candidate_end, catalog, paragraph)?;
            if width <= available_width_pt {
                last_fitting_legal_end = Some(candidate_end);
            } else {
                break;
            }
        }
        let line_end = if let Some(end) = last_fitting_legal_end {
            end
        } else {
            let mut last_fitting_grapheme_end = None;
            for candidate_end in grapheme_ends
                .iter()
                .copied()
                .filter(|end| *end > line_start)
            {
                let width = shaped_ink_width(text, line_start..candidate_end, catalog, paragraph)?;
                if width <= available_width_pt {
                    last_fitting_grapheme_end = Some(candidate_end);
                } else {
                    break;
                }
            }
            last_fitting_grapheme_end.unwrap_or_else(|| {
                grapheme_ends
                    .iter()
                    .copied()
                    .find(|end| *end > line_start)
                    .unwrap_or(segment.end)
            })
        };
        if line_end <= line_start {
            return None;
        }
        ranges.push(line_start..line_end);
        line_start = line_end;
    }
    Some(())
}

fn shaped_ink_width(
    text: &str,
    range: Range<usize>,
    catalog: &FontCatalog,
    paragraph: &Paragraph<'_>,
) -> Option<f32> {
    let line_text = text.get(range.clone())?;
    let ink_end = range.start + line_text.trim_end_matches(char::is_whitespace).len();
    if ink_end <= range.start {
        return Some(0.0);
    }
    let layout = catalog.layout_paragraph_line(paragraph, range.start..ink_end);
    (!layout.estimated && layout.width.is_finite()).then_some(layout.width)
}

fn is_hard_break_grapheme(grapheme: &str) -> bool {
    matches!(grapheme, "\r\n" | "\n" | "\u{b}" | "\u{c}")
}

/// Shape text and style runs for paragraphs and table-cell lines using the
/// same geometry implementation. Run ranges and the result are source bytes.
#[allow(clippy::too_many_arguments)]
pub fn shape_text_line(
    text: &str,
    style_runs: &[StyleRun],
    base_size_pt: f32,
    kind: &str,
    direction: ParagraphDirection,
    alignment: Alignment,
    range: Range<usize>,
    available_width_pt: f32,
) -> Option<WriterShapedVisualLine> {
    shape_text_ranges(
        text,
        style_runs,
        base_size_pt,
        kind,
        direction,
        alignment,
        std::slice::from_ref(&range),
        available_width_pt,
    )
    .into_iter()
    .next()
    .flatten()
}

#[allow(clippy::too_many_arguments)]
fn shape_text_ranges(
    text: &str,
    style_runs: &[StyleRun],
    base_size_pt: f32,
    kind: &str,
    direction: ParagraphDirection,
    alignment: Alignment,
    ranges: &[Range<usize>],
    available_width_pt: f32,
) -> Vec<Option<WriterShapedVisualLine>> {
    if !base_size_pt.is_finite() || base_size_pt <= 0.0 || !available_width_pt.is_finite() {
        return vec![None; ranges.len()];
    }
    let ranges: Vec<_> = ranges
        .iter()
        .cloned()
        .map(|range| clamp_range(text, range))
        .collect();
    let catalog = fonts::font_catalog();
    let span_styles = span_styles(text, style_runs, base_size_pt);
    if span_styles.is_empty() {
        return vec![None; ranges.len()];
    }
    let mut font_refs: Vec<FontRef> = Vec::with_capacity(span_styles.len());
    for span in &span_styles {
        let heading = is_heading_kind(kind);
        let bold = heading || span.style.weight.numeric() >= loom_text::FontWeight::Bold.numeric();
        let family = if span.style.font_family.trim().is_empty() {
            fonts::DEFAULT_FAMILY
        } else {
            span.style.font_family.as_str()
        };
        let Ok(font) = catalog.resolve(family, if bold { 700 } else { 400 }, span.style.italic)
        else {
            return vec![None; ranges.len()];
        };
        font_refs.push(font);
    }
    let styled_spans: Vec<StyledSpan<'_>> = span_styles
        .iter()
        .zip(&font_refs)
        .map(|(style, font)| StyledSpan::new(style.range.clone(), font, style.effective_size_pt))
        .collect();
    let base = match direction {
        ParagraphDirection::Auto => TextDirection::Auto,
        ParagraphDirection::LeftToRight => TextDirection::LeftToRight,
        ParagraphDirection::RightToLeft => TextDirection::RightToLeft,
    };
    let paragraph = Paragraph::new(text, &styled_spans, base);
    ranges
        .into_iter()
        .map(|range| {
            shape_line_from_paragraph(
                text,
                &span_styles,
                &catalog,
                &paragraph,
                direction,
                alignment,
                range,
                available_width_pt,
            )
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn shape_line_from_paragraph(
    text: &str,
    span_styles: &[SpanStyle],
    catalog: &FontCatalog,
    paragraph: &Paragraph<'_>,
    direction: ParagraphDirection,
    alignment: Alignment,
    range: Range<usize>,
    available_width_pt: f32,
) -> Option<WriterShapedVisualLine> {
    // A mandatory break belongs to the source line, but it is not a visible
    // glyph. Shape only the content bytes while keeping `range` as the line's
    // authoritative editing/source extent.
    let paint_range = visible_paint_range(text, range.clone());
    let natural = catalog.layout_paragraph_line(paragraph, paint_range.clone());
    if natural.estimated {
        return None;
    }
    let requested_justify = alignment.resolve_with_direction(text, direction) == Alignment::Justify;
    let gaps = shaped_line_gaps(
        text,
        range.clone(),
        natural.width,
        available_width_pt,
        requested_justify,
    );
    let mut laid_out = natural.runs.clone();
    let glyph_expansions = expand_run_advances(&mut laid_out, &gaps);
    let mut paint_runs = Vec::with_capacity(laid_out.len());
    let mut current_x = 0.0_f32;
    let mut ascent_pt = 0.0_f32;
    let mut descent_pt = 0.0_f32;
    let mut line_gap_pt = 0.0_f32;
    for run in &mut laid_out {
        let style = span_styles.get(run.style)?;
        let face = catalog.load(run.face).ok()?;
        run.x = current_x;
        run.shaped.width = run.shaped.glyphs.iter().map(|glyph| glyph.advance).sum();
        current_x += run.shaped.width;
        let run_text = text.get(run.range.clone())?;
        let caret_stops = run
            .shaped
            .caret_stops(run_text)
            .into_iter()
            .map(|stop| WriterCaretStop {
                offset: run.range.start + stop.offset,
                affinity: Affinity::Leading,
                x_pt: run.x + stop.x,
            })
            .collect();
        let shift = baseline_shift(style);
        let foreground = parse_rgba(style.style.color.as_deref());
        ascent_pt = ascent_pt.max(run.ascent + shift);
        descent_pt = descent_pt.max(run.descent - shift);
        line_gap_pt = line_gap_pt.max(run.line_gap);
        paint_runs.push(WriterPaintedRun {
            source_range: run.range.clone(),
            face,
            shaped: run.shaped.clone(),
            x_pt: run.x,
            size_pt: run.size,
            requested_size_pt: style.requested_size_pt,
            ascent_pt: run.ascent,
            descent_pt: run.descent,
            variations: run.variations.clone(),
            synthetic_bold: run.synthetic_bold,
            synthetic_italic: run.synthetic_italic,
            rgba: foreground.unwrap_or([0, 0, 0, 255]),
            foreground_color_explicit: foreground.is_some(),
            background_rgba: parse_rgba(style.style.background.as_deref()),
            baseline_shift_pt: shift,
            underline: style.style.underline,
            strikethrough: style.style.strikethrough,
            underline_metric: run.underline,
            strikeout_metric: run.strikeout,
            caret_stops,
        });
    }
    let width_pt = current_x;
    let physical_alignment = alignment.resolve_with_direction(text, direction);
    let paragraph_start = Alignment::Start.resolve_with_direction(text, direction);
    let alignment_shift_pt = match physical_alignment {
        Alignment::Center => ((available_width_pt - width_pt) / 2.0).max(0.0),
        Alignment::Right => (available_width_pt - width_pt).max(0.0),
        Alignment::Justify if gaps.is_empty() && paragraph_start == Alignment::Right => {
            (available_width_pt - width_pt).max(0.0)
        }
        Alignment::Left | Alignment::Justify => 0.0,
        Alignment::Start | Alignment::End => unreachable!("logical alignment was resolved"),
    };
    for run in &mut paint_runs {
        run.x_pt += alignment_shift_pt;
        for stop in &mut run.caret_stops {
            stop.x_pt += alignment_shift_pt;
        }
    }
    let source_end = range.end;
    let visible_end = visible_end(text, range.clone());
    let hard_break = text[range.clone()].ends_with(['\n', '\u{b}', '\u{c}']);
    let interaction_end = if gaps.is_empty() && !hard_break {
        source_end
    } else {
        visible_end
    };
    let (mut carets_by_offset, mut visual_carets) =
        caret_index(&natural, &glyph_expansions, paint_range);
    for (_, leading, trailing) in &mut carets_by_offset {
        *leading += alignment_shift_pt;
        *trailing += alignment_shift_pt;
    }
    for stop in &mut visual_carets {
        stop.x_pt += alignment_shift_pt;
    }
    carets_by_offset.retain(|(offset, _, _)| *offset <= interaction_end);
    visual_carets.retain(|stop| stop.offset <= interaction_end);
    let contains_rtl = laid_out.iter().any(|run| run.rtl);
    Some(WriterShapedVisualLine {
        source_range: range,
        visible_end,
        interaction_end,
        natural_width_pt: natural.width,
        width_pt,
        alignment_shift_pt,
        ascent_pt: ascent_pt.max(natural.ascent),
        descent_pt: descent_pt.max(natural.descent),
        line_gap_pt: line_gap_pt.max(natural.line_gap),
        estimated: natural.estimated,
        contains_rtl,
        painted_runs: paint_runs,
        carets_by_offset,
        visual_carets,
    })
}

fn visible_paint_range(text: &str, range: Range<usize>) -> Range<usize> {
    let Some(line) = text.get(range.clone()) else {
        return range;
    };
    let marker_len = if line.ends_with("\r\n") {
        "\r\n".len()
    } else {
        line.chars()
            .next_back()
            .filter(|marker| matches!(marker, '\n' | '\u{b}' | '\u{c}'))
            .map(char::len_utf8)
            .unwrap_or(0)
    };
    range.start..range.end - marker_len
}

fn span_styles(text: &str, runs: &[StyleRun], base_size_pt: f32) -> Vec<SpanStyle> {
    if text.is_empty() {
        return Vec::new();
    }
    let mut cuts = vec![0, text.len()];
    for run in runs {
        let start = floor_char_boundary(text, run.start);
        let end = floor_char_boundary(text, run.end.min(text.len()));
        cuts.extend([start, end]);
    }
    cuts.sort_unstable();
    cuts.dedup();
    cuts.windows(2)
        .filter_map(|pair| {
            let range = pair[0]..pair[1];
            if range.is_empty() {
                return None;
            }
            let style = runs
                .iter()
                .find(|run| run.start <= range.start && range.start < run.end)
                .map(|run| run.style.clone())
                .unwrap_or_default();
            let requested_size_pt = run_size(&style, base_size_pt);
            let superscript = style.superscript && !style.subscript;
            let subscript = style.subscript && !style.superscript;
            let effective_size_pt = if superscript || subscript {
                requested_size_pt * 0.7
            } else {
                requested_size_pt
            };
            Some(SpanStyle {
                range,
                style,
                requested_size_pt,
                effective_size_pt,
                is_superscript: superscript,
                is_subscript: subscript,
            })
        })
        .collect()
}

fn baseline_shift(style: &SpanStyle) -> f32 {
    if style.is_superscript {
        style.requested_size_pt * 0.35
    } else if style.is_subscript {
        -(style.requested_size_pt * 0.35)
    } else {
        0.0
    }
}

#[derive(Clone, Copy)]
struct GlyphExpansion {
    left_pt: f32,
    right_pt: f32,
    extra_pt: f32,
}

/// Expands the glyph advances in visual order and retains the exact locations
/// of each added gap so bidi caret affinities can be shifted from the same
/// shaped line index.
fn expand_run_advances(
    runs: &mut [LaidOutRun],
    gaps: &[(Range<usize>, f32)],
) -> Vec<GlyphExpansion> {
    let mut extras: Vec<Vec<f32>> = runs
        .iter()
        .map(|run| vec![0.0; run.shaped.glyphs.len()])
        .collect();
    for (gap, extra) in gaps {
        let glyphs: Vec<_> = runs
            .iter()
            .enumerate()
            .flat_map(|(run_index, run)| {
                run.shaped
                    .glyphs
                    .iter()
                    .enumerate()
                    .filter_map(move |(glyph_index, glyph)| {
                        let cluster = run.range.start.saturating_add(glyph.cluster);
                        gap.contains(&cluster).then_some((run_index, glyph_index))
                    })
            })
            .collect();
        if glyphs.is_empty() {
            continue;
        }
        let per_glyph = *extra / glyphs.len() as f32;
        for (run_index, glyph_index) in glyphs {
            extras[run_index][glyph_index] += per_glyph;
        }
    }

    let mut geometry = Vec::new();
    for (run_index, run) in runs.iter_mut().enumerate() {
        let mut pen_x = run.x;
        for (glyph_index, glyph) in run.shaped.glyphs.iter_mut().enumerate() {
            let left_pt = pen_x;
            let natural_advance = glyph.advance;
            let extra_pt = extras[run_index][glyph_index];
            pen_x += natural_advance;
            geometry.push(GlyphExpansion {
                left_pt,
                right_pt: pen_x,
                extra_pt,
            });
            glyph.advance += extra_pt;
        }
        run.shaped.width = run.shaped.glyphs.iter().map(|glyph| glyph.advance).sum();
    }
    geometry
}

fn caret_index(
    line: &LineLayout,
    expansions: &[GlyphExpansion],
    range: Range<usize>,
) -> (Vec<(usize, f32, f32)>, Vec<WriterCaretStop>) {
    let mut boundaries: BTreeMap<usize, (f32, f32)> = BTreeMap::new();
    let mut visual_carets = Vec::with_capacity(line.caret_positions().len());
    for stop in line.caret_positions() {
        let x_pt = stop.x + expansion_at(stop.x, expansions);
        let boundary = boundaries.entry(stop.offset).or_insert((x_pt, x_pt));
        match stop.affinity {
            Affinity::Leading => boundary.0 = x_pt,
            Affinity::Trailing => boundary.1 = x_pt,
        }
        visual_carets.push(WriterCaretStop {
            offset: stop.offset,
            affinity: stop.affinity,
            x_pt,
        });
    }
    if boundaries.is_empty() {
        boundaries.insert(range.start, (0.0, 0.0));
        visual_carets.push(WriterCaretStop {
            offset: range.start,
            affinity: Affinity::Leading,
            x_pt: 0.0,
        });
    }
    let carets_by_offset = boundaries
        .into_iter()
        .map(|(offset, (leading, trailing))| (offset, leading, trailing))
        .collect();
    visual_carets.sort_by(|left, right| {
        left.x_pt
            .total_cmp(&right.x_pt)
            .then(left.offset.cmp(&right.offset))
            .then_with(|| match (left.affinity, right.affinity) {
                (Affinity::Leading, Affinity::Trailing) => std::cmp::Ordering::Less,
                (Affinity::Trailing, Affinity::Leading) => std::cmp::Ordering::Greater,
                _ => std::cmp::Ordering::Equal,
            })
    });
    (carets_by_offset, visual_carets)
}

fn expansion_at(x_pt: f32, expansions: &[GlyphExpansion]) -> f32 {
    expansions
        .iter()
        .map(|glyph| {
            if glyph.extra_pt == 0.0 || x_pt <= glyph.left_pt {
                0.0
            } else if x_pt >= glyph.right_pt {
                glyph.extra_pt
            } else {
                let width = glyph.right_pt - glyph.left_pt;
                if width <= f32::EPSILON {
                    glyph.extra_pt
                } else {
                    glyph.extra_pt * ((x_pt - glyph.left_pt) / width).clamp(0.0, 1.0)
                }
            }
        })
        .sum()
}

pub(crate) fn parse_rgba(value: Option<&str>) -> Option<[u8; 4]> {
    let hex = value?.strip_prefix('#')?;
    if hex.len() != 6 && hex.len() != 8 {
        return None;
    }
    let channel = |at| u8::from_str_radix(hex.get(at..at + 2)?, 16).ok();
    Some([
        channel(0)?,
        channel(2)?,
        channel(4)?,
        if hex.len() == 8 { channel(6)? } else { 255 },
    ])
}

fn floor_char_boundary(text: &str, mut offset: usize) -> usize {
    offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

fn clamp_range(text: &str, range: Range<usize>) -> Range<usize> {
    let start = floor_char_boundary(text, range.start);
    let end = floor_char_boundary(text, range.end).max(start);
    start..end
}

fn visible_end(text: &str, range: Range<usize>) -> usize {
    range.start
        + text[range]
            .trim_end_matches(['\r', '\n', '\u{b}', '\u{c}'])
            .len()
}
