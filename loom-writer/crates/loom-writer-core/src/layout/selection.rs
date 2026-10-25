//! Selection and caret rectangles on the canonical page line geometry.

use std::ops::Range;
use std::rc::Rc;

use crate::text_metrics::bidi::{visible_end, BidiRuns, LineOrder};
use crate::text_metrics::justification::line_geometry_with_measure;
use crate::text_metrics::LineMeasure;
use crate::{
    floor_grapheme_boundary, PageRect, PageStyle, SelectionRect, TextSelection, WriterDocument,
};

use super::{block_bidi_runs, block_positions, FlowLine};

pub(super) fn floor_in_blocks(
    doc: &WriterDocument,
    block_starts: &[usize],
    offset: usize,
) -> usize {
    let Some(last) = doc.blocks.len().checked_sub(1) else {
        return 0;
    };
    let total = block_starts[last] + doc.blocks[last].text.len_bytes();
    let offset = offset.min(total);
    let index = block_starts
        .partition_point(|start| *start <= offset)
        .saturating_sub(1);
    let start = block_starts[index];
    start + floor_grapheme_boundary(doc.blocks[index].text.as_str(), offset - start)
}

/// Selection highlights or caret for `selection` on `lines`, limited to the
/// selected page window.
pub(super) fn rects_for_lines(
    doc: &WriterDocument,
    style: &PageStyle,
    zoom: f32,
    block_starts: &[usize],
    lines: &[FlowLine],
    selection: &TextSelection,
    window: Range<usize>,
) -> Vec<SelectionRect> {
    let anchor = floor_in_blocks(doc, block_starts, selection.anchor);
    let focus = floor_in_blocks(doc, block_starts, selection.focus);
    let (selection_start, selection_end) = (anchor.min(focus), anchor.max(focus));
    let (lo, hi) = if selection_start < selection_end {
        (
            lines.partition_point(|line| line.global_end() <= selection_start),
            lines.partition_point(|line| line.global_start < selection_end),
        )
    } else {
        (
            lines.partition_point(|line| line.global_end() < selection_start),
            lines.partition_point(|line| line.global_start <= selection_start),
        )
    };
    let (lo, hi) = (lo.max(window.start), hi.min(window.end).min(lines.len()));
    let mut result = Vec::new();
    let mut measured: Option<(usize, Rc<LineMeasure>)> = None;
    let mut bidi_runs: Option<(usize, Option<Rc<BidiRuns>>)> = None;
    for at in lo..hi.max(lo) {
        let line = &lines[at];
        let block = &doc.blocks[line.block_index];
        let font_size = style.font_size_for_kind(block.kind.as_str());
        let source = &block.text.as_str()[line.start..line.end];
        let positions = match &measured {
            Some((index, positions)) if *index == line.block_index => positions.clone(),
            _ => {
                let positions = block_positions(block, font_size);
                measured = Some((line.block_index, positions.clone()));
                positions
            }
        };
        let bidi = match &bidi_runs {
            Some((index, bidi)) if *index == line.block_index => bidi.clone(),
            _ => {
                let bidi = block_bidi_runs(block);
                bidi_runs = Some((line.block_index, bidi.clone()));
                bidi
            }
        };
        let order = bidi.as_deref().and_then(|bidi| {
            let visible = visible_end(block.text.as_str(), line.start..line.end);
            LineOrder::build(bidi, block.text.as_str(), &positions, line.start..visible)
        });
        let geometry = line_geometry_with_measure(
            block,
            line.start..line.end,
            &positions,
            block.text_column_width(style),
            block.style.alignment.resolve(block.text.as_str()) == loom_text::Alignment::Justify,
        );
        let interaction_end = if order.is_some() {
            geometry.visible_end
        } else {
            geometry.interaction_end(block.text.as_str())
        };
        let advance = |from: usize, to: usize| {
            (positions.advance(from, to) + geometry.extra_between(from, to)) * zoom
        };
        let available_width = block.text_column_width(style) * zoom;
        let alignment_offset = match block.style.alignment.resolve(block.text.as_str()) {
            loom_text::Alignment::Center => ((available_width - line.bounds.width) / 2.0).max(0.0),
            loom_text::Alignment::Right => (available_width - line.bounds.width).max(0.0),
            loom_text::Alignment::Left
            | loom_text::Alignment::Justify
            | loom_text::Alignment::Start => 0.0,
        };
        let line_x = line.bounds.x + alignment_offset;
        let (global_start, global_end) = (line.global_start, line.global_end());
        let overlap_start = selection_start.max(global_start);
        let overlap_end = selection_end.min(global_end);
        if overlap_start < overlap_end {
            let local_start = overlap_start - global_start;
            let local_end = overlap_end - global_start;
            let from = (line.start + local_start.min(source.len())).min(interaction_end);
            let to = (line.start + local_end.min(source.len())).min(interaction_end);
            if let Some(order) = &order {
                for span in order.spans(from..to) {
                    result.push(SelectionRect {
                        page_index: line.page,
                        block_id: block.id,
                        start: span.start,
                        end: span.end,
                        rect: PageRect {
                            x: line_x + span.left,
                            y: line.bounds.y,
                            width: span.right - span.left,
                            height: line.bounds.height,
                        },
                    });
                }
            } else {
                result.push(SelectionRect {
                    page_index: line.page,
                    block_id: block.id,
                    start: line.start + local_start,
                    end: line.start + local_end,
                    rect: PageRect {
                        x: line_x + advance(line.start, from),
                        y: line.bounds.y,
                        width: advance(from, to),
                        height: line.bounds.height,
                    },
                });
            }
        } else if selection_start == selection_end {
            let at_start = selection_start == global_start;
            let at_end = selection_start == global_end;
            let strictly_inside = selection_start > global_start && selection_start < global_end;
            let has_previous_boundary = at > 0 && lines[at - 1].global_end() == global_start;
            let is_last_line = at + 1 == lines.len();
            let use_line = strictly_inside
                || (at_start
                    && (!has_previous_boundary
                        || selection.affinity == crate::CaretAffinity::Downstream))
                || (at_end
                    && (selection.affinity == crate::CaretAffinity::Upstream || is_last_line));
            if use_line {
                let local = selection_start
                    .saturating_sub(global_start)
                    .min(source.len());
                let offset = line.start + local;
                let geometry_offset = offset.min(interaction_end);
                let caret_x = match &order {
                    Some(order) => order.x_of(geometry_offset, selection.affinity),
                    None => {
                        positions.advance(line.start, geometry_offset)
                            + geometry.extra_before(geometry_offset)
                    }
                } * zoom;
                result.push(SelectionRect {
                    page_index: line.page,
                    block_id: block.id,
                    start: offset,
                    end: offset,
                    rect: PageRect {
                        x: line_x + caret_x,
                        y: line.bounds.y,
                        width: 1.0,
                        height: line.bounds.height,
                    },
                });
            }
        }
    }
    result
}
