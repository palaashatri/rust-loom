//! Caret movement along the lines the page actually shows.
//!
//! The editor's hidden text input lays the whole document out at one size, left
//! aligned and regular weight, so its Up, Down, Home and End disagree with the
//! page once headings, bold runs, alignment or pictures are involved. These
//! moves use the document flow instead: the same wrapped lines, fonts and
//! alignment that carets, selections and pointer hits are computed from.

use crate::text_metrics::bidi::visible_end;
use crate::{
    text_advance, CaretAffinity, FlowLine, PageStyle, PageViewport, RichBlock, WriterDocument,
    TABLE_BLOCK_KIND,
};

/// A caret movement by visible line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisualMove {
    /// One visible line up.
    Up,
    /// One visible line down.
    Down,
    /// The start of the visible line.
    LineStart,
    /// The end of the visible line (before the space a soft wrap leaves).
    LineEnd,
    /// One step toward the visual left. Only a line with right-to-left text has
    /// visual steps; in any other line, or at its left edge, the move is `None`
    /// and the caller keeps its logical move.
    Left,
    /// One step toward the visual right; see [`VisualMove::Left`].
    Right,
}

/// Where a visual move lands.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CaretTarget {
    /// Byte offset in the editor text stream.
    pub offset: usize,
    /// Horizontal position, in points on the page, to keep as the goal column
    /// for consecutive vertical moves.
    pub x: f32,
    /// Which line a caret at a soft-wrap boundary belongs to.
    pub affinity: CaretAffinity,
}

/// Move the caret at `offset` by one visible line, or to the start or end of
/// its visible line. `x_hint` is the goal column carried over from a previous
/// vertical move. Tables are stepped over (their cells have their own
/// navigation); a picture line is entered at its start. Returns `None` when the
/// layout cannot be built or the caret has no visible line.
pub fn visual_caret_move(
    doc: &WriterDocument,
    offset: usize,
    affinity: CaretAffinity,
    kind: VisualMove,
    x_hint: Option<f32>,
) -> Option<CaretTarget> {
    // Left and Right only need the layout inside right-to-left text: anywhere
    // else they are the input's, and must not pay for a whole-document layout.
    if matches!(kind, VisualMove::Left | VisualMove::Right)
        && !block_may_be_right_to_left(doc, offset)
    {
        return None;
    }
    let style = doc.page.page_style();
    let flow = doc
        .flow(
            &style,
            PageViewport {
                width: style.width_pt,
                height: style.height_pt,
                zoom: 1.0,
                scroll_x: 0.0,
                scroll_y: 0.0,
            },
        )
        .ok()?;
    let lines = flow.lines();
    let current = line_at(lines, offset, affinity)?;
    let here = &lines[current];
    let here_block = &doc.blocks[here.block_index];
    let own_x = || caret_x(&style, here_block, here, offset, affinity);
    match kind {
        VisualMove::LineStart => Some(CaretTarget {
            offset: here.global_start,
            x: caret_x(
                &style,
                here_block,
                here,
                here.global_start,
                CaretAffinity::Downstream,
            ),
            affinity: CaretAffinity::Downstream,
        }),
        VisualMove::LineEnd => Some(end_of(doc, &style, lines, current)),
        VisualMove::Left | VisualMove::Right => visual_step(
            doc,
            &style,
            lines,
            current,
            offset,
            affinity,
            kind == VisualMove::Right,
        ),
        VisualMove::Up | VisualMove::Down => {
            let x = x_hint.unwrap_or_else(own_x);
            let target = if kind == VisualMove::Up {
                (0..current).rev().find(|&i| walkable(doc, &lines[i]))
            } else {
                (current + 1..lines.len()).find(|&i| walkable(doc, &lines[i]))
            };
            match target {
                Some(index) => Some(land(doc, &style, lines, index, x)),
                // No line beyond: Up goes to the line start, Down to its end.
                None if kind == VisualMove::Up => Some(CaretTarget {
                    offset: here.global_start,
                    x,
                    affinity: CaretAffinity::Downstream,
                }),
                None => Some(CaretTarget {
                    x,
                    ..end_of(doc, &style, lines, current)
                }),
            }
        }
    }
}

/// Whether the block that holds editor `offset` can hold right-to-left text.
/// Only the block's own text is scanned, so this costs no layout.
fn block_may_be_right_to_left(doc: &WriterDocument, offset: usize) -> bool {
    let mut start = 0;
    for block in &doc.blocks {
        let end = start + block.text.as_str().len();
        if offset <= end {
            return crate::text_metrics::bidi::BidiRuns::of(block.text.as_str()).is_some();
        }
        start = end + 1;
    }
    false
}

fn walkable(doc: &WriterDocument, line: &FlowLine) -> bool {
    doc.blocks[line.block_index].kind != TABLE_BLOCK_KIND
}

fn is_object(doc: &WriterDocument, style: &PageStyle, line: &FlowLine) -> bool {
    let usable_width = style.width_pt - style.margin_left_pt - style.margin_right_pt;
    let usable_height = style.height_pt - style.margin_top_pt - style.margin_bottom_pt;
    doc.blocks[line.block_index]
        .object_display_size(usable_width, usable_height)
        .is_some()
}

/// The line holding `offset`. At a soft-wrap boundary both lines qualify and
/// the caret affinity decides.
fn line_at(lines: &[FlowLine], offset: usize, affinity: CaretAffinity) -> Option<usize> {
    let mut found: Option<usize> = None;
    for (index, line) in lines.iter().enumerate() {
        let end = line.global_start + (line.end - line.start);
        if line.global_start <= offset && offset <= end {
            found = Some(index);
            if affinity == CaretAffinity::Upstream {
                break;
            }
        } else if line.global_start > offset {
            break;
        }
    }
    found
}

/// Points from the page's left edge to where the line's text begins.
fn line_origin(style: &PageStyle, block: &RichBlock, line: &FlowLine) -> f32 {
    let column = block.text_column_width(style);
    let shift = match block.style.alignment.resolve(block.text.as_str()) {
        loom_text::Alignment::Center => ((column - line.bounds.width) / 2.0).max(0.0),
        loom_text::Alignment::Right => (column - line.bounds.width).max(0.0),
        loom_text::Alignment::Left
        | loom_text::Alignment::Justify
        | loom_text::Alignment::Start => 0.0,
    };
    line.bounds.x + shift
}

/// The x of the caret at editor `offset` on `line`. A right-to-left line places
/// it in the order its glyphs are drawn in; any other line by logical advance.
fn caret_x(
    style: &PageStyle,
    block: &RichBlock,
    line: &FlowLine,
    offset: usize,
    affinity: CaretAffinity,
) -> f32 {
    let font_size = style.font_size_for_kind(block.kind.as_str());
    let local = (offset - line.global_start).min(line.end - line.start);
    if let Some(order) = block.visual_line(font_size, line.start..line.end) {
        return line_origin(style, block, line) + order.x_of(line.start + local, affinity);
    }
    let text = &block.text.as_str()[line.start..line.start + local];
    line_origin(style, block, line) + text_advance(text, line.start, &block.runs, font_size)
}

/// Bytes of the line a caret may sit after: a soft wrap leaves its space
/// hanging past the visible text, and a hard break is never a caret position.
fn visible_len(block: &RichBlock, line: &FlowLine) -> usize {
    visible_end(block.text.as_str(), line.start..line.end) - line.start
}

/// Affinity for a caret at `offset` on line `index`: a caret at the very end
/// of a line belongs to that line (so one that wraps straight into the next
/// stays on the upper line), as a caret placed there by any other means does.
fn affinity_on(lines: &[FlowLine], index: usize, offset: usize) -> CaretAffinity {
    let line = &lines[index];
    if offset == line.global_start + (line.end - line.start) {
        CaretAffinity::Upstream
    } else {
        CaretAffinity::Downstream
    }
}

fn end_of(
    doc: &WriterDocument,
    style: &PageStyle,
    lines: &[FlowLine],
    index: usize,
) -> CaretTarget {
    let line = &lines[index];
    let block = &doc.blocks[line.block_index];
    let offset = line.global_start + visible_len(block, line);
    let affinity = affinity_on(lines, index, offset);
    CaretTarget {
        offset,
        x: caret_x(style, block, line, offset, affinity),
        affinity,
    }
}

/// Land on line `index` as near horizontal position `x` as its text allows.
fn land(
    doc: &WriterDocument,
    style: &PageStyle,
    lines: &[FlowLine],
    index: usize,
    x: f32,
) -> CaretTarget {
    let line = &lines[index];
    let block = &doc.blocks[line.block_index];
    if is_object(doc, style, line) {
        return CaretTarget {
            offset: line.global_start,
            x,
            affinity: CaretAffinity::Downstream,
        };
    }
    let font_size = style.font_size_for_kind(block.kind.as_str());
    if let Some(order) = block.visual_line(font_size, line.start..line.end) {
        if let Some(stop) = order.stop_at(x - line_origin(style, block, line)) {
            return CaretTarget {
                offset: line.global_start + (stop.offset - line.start),
                x,
                affinity: stop.affinity,
            };
        }
    }
    let local = crate::offset_at_x(
        &block.text.as_str()[line.start..line.end],
        line.start,
        &block.runs,
        font_size,
        x - line_origin(style, block, line),
    )
    .min(visible_len(block, line));
    let offset = line.global_start + local;
    CaretTarget {
        offset,
        x,
        affinity: affinity_on(lines, index, offset),
    }
}

/// One step toward the visual left (or right) in a line that holds
/// right-to-left text, from the caret at `offset`. `None` in a left-to-right
/// line and at the line's edge, where the caller's logical move applies.
fn visual_step(
    doc: &WriterDocument,
    style: &PageStyle,
    lines: &[FlowLine],
    index: usize,
    offset: usize,
    affinity: CaretAffinity,
    forward: bool,
) -> Option<CaretTarget> {
    let line = &lines[index];
    let block = &doc.blocks[line.block_index];
    let font_size = style.font_size_for_kind(block.kind.as_str());
    let order = block.visual_line(font_size, line.start..line.end)?;
    let at = line.start + offset.checked_sub(line.global_start)?;
    let target = order.neighbour(at, affinity, forward)?;
    Some(CaretTarget {
        offset: line.global_start + (target.offset - line.start),
        x: line_origin(style, block, line) + target.x,
        affinity: target.affinity,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const LONG: &str = "The quick brown fox jumps over the lazy dog while the sun sets slowly \
                        behind distant hills and the evening grows quiet";

    fn doc(blocks: Vec<RichBlock>) -> WriterDocument {
        let mut doc = WriterDocument::new("caret", "Caret");
        doc.blocks = blocks;
        doc
    }

    fn lines_of(doc: &WriterDocument) -> Vec<FlowLine> {
        let style = doc.page.page_style();
        doc.flow(
            &style,
            PageViewport {
                width: style.width_pt,
                height: style.height_pt,
                zoom: 1.0,
                scroll_x: 0.0,
                scroll_y: 0.0,
            },
        )
        .unwrap()
        .lines()
        .to_vec()
    }

    fn mv(doc: &WriterDocument, at: usize, kind: VisualMove, hint: Option<f32>) -> CaretTarget {
        visual_caret_move(doc, at, CaretAffinity::Downstream, kind, hint).expect("a target")
    }

    fn heading() -> WriterDocument {
        let mut block = RichBlock::new(1, "heading1", LONG);
        block.style.alignment = loom_text::Alignment::Center;
        doc(vec![block])
    }

    #[test]
    fn down_from_a_wrapped_heading_lands_on_its_second_visible_line() {
        let doc = heading();
        let lines = lines_of(&doc);
        assert!(lines.len() >= 2, "the 24pt heading wraps: {}", lines.len());
        let first = &lines[0];
        let at = first.global_start + 6;
        let style = doc.page.page_style();
        let start_x = caret_x(&style, &doc.blocks[0], first, at, CaretAffinity::Downstream);
        let target = mv(&doc, at, VisualMove::Down, None);
        let second = &lines[1];
        assert!(
            target.offset >= second.global_start
                && target.offset <= second.global_start + (second.end - second.start),
            "offset {} is on the second line {}..{}",
            target.offset,
            second.global_start,
            second.end
        );
        assert!(
            (caret_x(
                &style,
                &doc.blocks[0],
                second,
                target.offset,
                target.affinity
            ) - start_x)
                .abs()
                < 20.0,
            "kept near x {start_x}"
        );
    }

    #[test]
    fn end_on_a_wrapped_line_stops_at_that_lines_end_not_the_paragraphs() {
        let doc = heading();
        let lines = lines_of(&doc);
        let first = &lines[0];
        let target = mv(&doc, first.global_start + 3, VisualMove::LineEnd, None);
        let shown = &LONG[first.start..first.end];
        assert_eq!(target.offset, first.global_start + shown.trim_end().len());
        assert!(target.offset < LONG.len());
        // A last line ends at the paragraph end.
        let last = lines.last().unwrap();
        let end = mv(&doc, last.global_start, VisualMove::LineEnd, None);
        assert_eq!(end.offset, LONG.len());
    }

    #[test]
    fn home_goes_to_the_start_of_the_visible_line() {
        let doc = heading();
        let lines = lines_of(&doc);
        let second = &lines[1];
        let target = mv(&doc, second.global_start + 4, VisualMove::LineStart, None);
        assert_eq!(target.offset, second.global_start);
        assert_eq!(target.affinity, CaretAffinity::Downstream);
    }

    #[test]
    fn the_goal_column_persists_across_lines_of_different_length() {
        let doc = doc(vec![
            RichBlock::new(1, "paragraph", "0123456789 0123456789 0123456789"),
            RichBlock::new(2, "paragraph", "ab"),
            RichBlock::new(3, "paragraph", "0123456789 0123456789 0123456789"),
        ]);
        let start = 20;
        let first = mv(&doc, start, VisualMove::Down, None);
        assert!(
            first.offset <= 35,
            "clamped to the short line: {}",
            first.offset
        );
        let second = mv(&doc, first.offset, VisualMove::Down, Some(first.x));
        let block_three = 33 + 1 + 2 + 1;
        let col = second.offset - block_three;
        assert!(
            (col as i32 - start as i32).abs() <= 1,
            "back at about column {start}, got {col}"
        );
        // Without the remembered column it would stay near the short line's end.
        let forgotten = mv(&doc, first.offset, VisualMove::Down, None);
        assert!(forgotten.offset - block_three <= 2);
        // And Up returns the same way.
        let up = mv(&doc, second.offset, VisualMove::Up, Some(second.x));
        assert!(up.offset >= 34 && up.offset <= 36);
    }

    #[test]
    fn vertical_moves_follow_the_alignment_of_the_target_line() {
        let mut centred = RichBlock::new(2, "paragraph", "centre");
        centred.style.alignment = loom_text::Alignment::Center;
        let doc = doc(vec![
            RichBlock::new(1, "paragraph", "left aligned text"),
            centred,
        ]);
        let target = mv(&doc, 0, VisualMove::Down, None);
        // The left edge of the page is far left of the centred word, so the
        // caret lands before it rather than at some column of its own text.
        assert_eq!(target.offset, 18);
    }

    #[test]
    fn up_on_the_first_line_and_down_on_the_last_go_to_the_ends() {
        let doc = doc(vec![RichBlock::new(1, "paragraph", "one two")]);
        assert_eq!(mv(&doc, 4, VisualMove::Up, None).offset, 0);
        assert_eq!(mv(&doc, 2, VisualMove::Down, None).offset, 7);
    }

    #[test]
    fn tables_are_stepped_over() {
        let doc = doc(vec![
            RichBlock::new(1, "paragraph", "above"),
            crate::tables::markdown_table_block(2, "| a | b |\n| - | - |\n| c | d |"),
            RichBlock::new(3, "paragraph", "below"),
        ]);
        let below_start: usize = 6 + doc.blocks[1].text.as_str().len() + 1;
        assert_eq!(mv(&doc, 2, VisualMove::Down, None).offset - below_start, 2);
        let up = mv(&doc, below_start + 2, VisualMove::Up, None);
        assert_eq!(up.offset, 2);
    }
}

#[cfg(test)]
#[path = "rtl_caret_tests.rs"]
mod rtl_caret_tests;
