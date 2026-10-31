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
/// Join adjacent selection spans that touch on the same visual line, so a
/// shaped run boundary does not leave visible seams or duplicate rectangles.
pub(super) fn merge_selection_rects(rects: Vec<SelectionRect>) -> Vec<SelectionRect> {
    let mut merged: Vec<SelectionRect> = Vec::with_capacity(rects.len());
    for rect in rects {
        if let Some(previous) = merged.last_mut() {
            let previous_right = previous.rect.x + previous.rect.width;
            let rect_right = rect.rect.x + rect.rect.width;
            let touches = (previous_right - rect.rect.x).abs() <= 0.01
                || (rect_right - previous.rect.x).abs() <= 0.01;
            if previous.page_index == rect.page_index
                && previous.block_id == rect.block_id
                && (previous.end == rect.start || rect.end == previous.start)
                && (previous.rect.y - rect.rect.y).abs() <= 0.01
                && (previous.rect.height - rect.rect.height).abs() <= 0.01
                && touches
            {
                previous.start = previous.start.min(rect.start);
                previous.end = previous.end.max(rect.end);
                previous.rect.x = previous.rect.x.min(rect.rect.x);
                previous.rect.width = previous_right.max(rect_right) - previous.rect.x;
                continue;
            }
        }
        merged.push(rect);
    }
    merged
}

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
        if block.kind == crate::TABLE_BLOCK_KIND {
            result.extend(table_cell_selection_rects(
                doc,
                style,
                zoom,
                block_starts[line.block_index],
                line,
                selection,
                selection_start..selection_end,
            ));
            continue;
        }
        if let Some(shaped) = &line.shaped_line {
            let (global_start, global_end) = (line.global_start, line.global_end());
            let overlap_start = selection_start.max(global_start);
            let overlap_end = selection_end.min(global_end);
            if overlap_start < overlap_end {
                let from = line.start + (overlap_start - global_start).min(source.len());
                let to = line.start + (overlap_end - global_start).min(source.len());
                for span in shaped.selection_spans(from..to) {
                    result.push(SelectionRect {
                        page_index: line.page,
                        block_id: block.id,
                        start: span.source_range.start,
                        end: span.source_range.end,
                        rect: PageRect {
                            x: line.bounds.x + span.x_pt * zoom,
                            y: line.bounds.y,
                            width: span.width_pt * zoom,
                            height: line.bounds.height,
                        },
                    });
                }
            } else if selection_start == selection_end {
                let at_start = selection_start == global_start;
                let at_end = selection_start == global_end;
                let strictly_inside =
                    selection_start > global_start && selection_start < global_end;
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
                    let affinity = match selection.affinity {
                        crate::CaretAffinity::Downstream => loom_fonts::Affinity::Leading,
                        crate::CaretAffinity::Upstream => loom_fonts::Affinity::Trailing,
                    };
                    result.push(SelectionRect {
                        page_index: line.page,
                        block_id: block.id,
                        start: offset,
                        end: offset,
                        rect: PageRect {
                            x: line.bounds.x + shaped.caret_x(offset, affinity) * zoom,
                            y: line.bounds.y,
                            width: 1.0,
                            height: line.bounds.height,
                        },
                    });
                }
            }
            continue;
        }
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
            block
                .style
                .alignment
                .resolve_with_direction(block.text.as_str(), block.style.direction)
                == loom_text::Alignment::Justify,
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
        let resolved_alignment = block
            .style
            .alignment
            .resolve_with_direction(block.text.as_str(), block.style.direction);
        let paragraph_start = loom_text::Alignment::Start
            .resolve_with_direction(block.text.as_str(), block.style.direction);
        let alignment_offset = match resolved_alignment {
            loom_text::Alignment::Center => ((available_width - line.bounds.width) / 2.0).max(0.0),
            loom_text::Alignment::End | loom_text::Alignment::Right => {
                (available_width - line.bounds.width).max(0.0)
            }
            loom_text::Alignment::Justify if paragraph_start == loom_text::Alignment::Right => {
                (available_width - line.bounds.width).max(0.0)
            }
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

fn table_cell_selection_rects(
    doc: &WriterDocument,
    style: &PageStyle,
    zoom: f32,
    block_start: usize,
    line: &FlowLine,
    selection: &TextSelection,
    overlap: std::ops::Range<usize>,
) -> Vec<SelectionRect> {
    let (selection_start, selection_end) = (overlap.start, overlap.end);
    let block = &doc.blocks[line.block_index];
    let Some(table) = block.typed_table() else {
        return Vec::new();
    };
    let map = crate::TableTextMap::new(table);
    let widths = crate::editor_column_widths(table, block.text_column_width(style));
    let options = loom_objects::TableStyleOptions {
        header_row: table.header_row,
        total_row: table.total_row,
        first_column: table.first_column,
        last_column: table.last_column,
        banded_rows: table.banded_rows,
        banded_columns: table.banded_columns,
    };
    let catalog = loom_objects::TableStyleCatalog::get(&table.style_id)
        .unwrap_or_else(loom_objects::TableStyleCatalog::default_style);
    let mut result = Vec::new();
    let collapsed = selection_start == selection_end;
    let caret_cell = collapsed
        .then(|| map.cell_at(selection_start.saturating_sub(block_start)))
        .flatten();
    for row in 0..table.row_count() {
        for column in 0..table.col_count() {
            let Some(cell_range) = map.cell_range(row, column) else {
                continue;
            };
            if cell_range.start > line.end || cell_range.end < line.start {
                continue;
            }
            if !collapsed {
                let global_start = block_start + cell_range.start;
                let global_end = block_start + cell_range.end;
                if selection_start.max(global_start) >= selection_end.min(global_end) {
                    continue;
                }
            } else if caret_cell != Some((row, column)) {
                continue;
            }
            let resolved = catalog.resolve_cell(
                row,
                column,
                table.row_count(),
                table.col_count(),
                &options,
                table.cell(row, column),
            );
            let cell_block = crate::table_cell_rich_block(
                block.id,
                table,
                row,
                column,
                crate::TABLE_CELL_BASE_FONT_SIZE_PT,
                resolved.bold,
                resolved.text_color.as_deref(),
            )
            .unwrap_or_else(|| crate::RichBlock::new(block.id, "paragraph", ""));
            let Some(cell_geometry) = crate::shape_table_cell_geometry_with_footnotes(
                doc,
                block.id,
                table,
                (row, column),
                &cell_block,
                crate::TABLE_CELL_BASE_FONT_SIZE_PT,
                (widths[column] - 2.0 * crate::TABLE_CELL_HORIZONTAL_INSET_PT).max(1.0),
            )
            .ok() else {
                continue;
            };
            let column_x = widths.iter().take(column).sum::<f32>();
            let cell_offset = selection_start
                .saturating_sub(block_start + cell_range.start)
                .min(cell_block.text.as_str().len());
            if collapsed {
                let caret_line = cell_geometry
                    .lines()
                    .iter()
                    .enumerate()
                    .find(|(_, cell_line)| {
                        cell_line
                            .shaped()
                            .is_some_and(|shaped| shaped.inline_marker_x(cell_offset).is_some())
                    })
                    .or_else(|| {
                        cell_geometry
                            .lines()
                            .iter()
                            .enumerate()
                            .find(|(_, cell_line)| {
                                let range = cell_line.source_range();
                                range.start <= cell_offset && cell_offset < range.end
                            })
                    })
                    .or_else(|| {
                        (cell_offset == cell_block.text.as_str().len())
                            .then(|| cell_geometry.lines().len().checked_sub(1))
                            .flatten()
                            .map(|index| (index, &cell_geometry.lines()[index]))
                    });
                let Some((line_index, cell_line)) = caret_line else {
                    continue;
                };
                let line_top = cell_geometry.lines()[..line_index]
                    .iter()
                    .map(|cell_line| cell_line.pitch_pt())
                    .sum::<f32>();
                let x = cell_line.shaped().map_or(0.0, |shaped| {
                    shaped.caret_x(
                        cell_offset,
                        match selection.affinity {
                            crate::CaretAffinity::Downstream => loom_fonts::Affinity::Leading,
                            crate::CaretAffinity::Upstream => loom_fonts::Affinity::Trailing,
                        },
                    )
                });
                result.push(SelectionRect {
                    page_index: line.page,
                    block_id: block.id,
                    start: cell_range.start + cell_offset,
                    end: cell_range.start + cell_offset,
                    rect: PageRect {
                        x: line.bounds.x
                            + (column_x + crate::TABLE_CELL_HORIZONTAL_INSET_PT + x) * zoom,
                        y: line.bounds.y + (crate::TABLE_CELL_VERTICAL_INSET_PT + line_top) * zoom,
                        width: 1.0,
                        height: cell_line.pitch_pt() * zoom,
                    },
                });
                continue;
            }
            let selected_start = selection_start
                .saturating_sub(block_start)
                .max(cell_range.start);
            let selected_end = selection_end
                .saturating_sub(block_start)
                .min(cell_range.end);
            if selected_start >= selected_end {
                continue;
            }
            let from = selected_start - cell_range.start;
            let to = selected_end - cell_range.start;
            let mut line_top = 0.0;
            for cell_line in cell_geometry.lines() {
                if let Some(shaped) = cell_line.shaped() {
                    for span in shaped.selection_spans(from..to) {
                        result.push(SelectionRect {
                            page_index: line.page,
                            block_id: block.id,
                            start: cell_range.start + span.source_range.start,
                            end: cell_range.start + span.source_range.end,
                            rect: PageRect {
                                x: line.bounds.x
                                    + (column_x
                                        + crate::TABLE_CELL_HORIZONTAL_INSET_PT
                                        + span.x_pt)
                                        * zoom,
                                y: line.bounds.y
                                    + (crate::TABLE_CELL_VERTICAL_INSET_PT + line_top) * zoom,
                                width: span.width_pt * zoom,
                                height: cell_line.pitch_pt() * zoom,
                            },
                        });
                    }
                }
                line_top += cell_line.pitch_pt();
            }
        }
    }
    result
}
