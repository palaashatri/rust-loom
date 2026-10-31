//! Bounded placement of anchored footnotes in the shared page flow.

use std::ops::Range;

use super::{shape_block_line, shape_wrapped_block_lines, FlowLine, InlineAdvance};
use crate::loom_objects::table::Table;
use crate::text_metrics::justification::measure_block;
use crate::{FootnoteEntry, PageRect, PageStyle, RichBlock, WriterDocument};

const FOOTNOTE_FONT_SCALE: f32 = 0.8;
const FOOTNOTE_LABEL_WIDTH_PT: f32 = 18.0;
const FOOTNOTE_SEPARATOR_GAP_PT: f32 = 6.0;

/// An inline reference in document coordinates and its projected page.
#[derive(Debug, Clone, PartialEq)]
pub struct FootnoteReferencePlacement {
    /// Stable note ID shared by the reference and its body.
    pub note_id: u64,
    /// Current document-order number (one-based).
    pub number: usize,
    /// Stable block containing the reference.
    pub block_id: u64,
    /// Index of the containing block in the document.
    pub block_index: usize,
    /// UTF-8 byte offset within the block.
    pub offset: usize,
    /// UTF-8 byte offset in the editor text stream.
    pub global_offset: usize,
    /// Zero-based page containing the reference line.
    pub page: usize,
    /// Shaped ordinal width reserved in the source line, in page points.
    pub marker_width_pt: f32,
}

/// One wrapped line of a footnote body in page coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct FootnoteLinePlacement {
    /// Stable note ID shared by the reference and body.
    pub note_id: u64,
    /// Current document-order number (one-based).
    pub number: usize,
    /// UTF-8 byte range in the canonical note body.
    pub body_range: Range<usize>,
    /// Zero-based page on which this note line is drawn.
    pub page: usize,
    /// Page containing the inline reference.
    pub reference_page: usize,
    /// Whether this line starts the note body.
    pub starts_note: bool,
    /// Whether this line continues from the reference page.
    pub continued: bool,
    /// Whether the PDF/editor should draw the ordinal beside this line.
    pub show_number: bool,
    /// Note font size in points.
    pub font_size_pt: f32,
    /// Number label bounds in viewport coordinates.
    pub number_bounds: PageRect,
    /// Text bounds in viewport coordinates.
    pub text_bounds: PageRect,
}

struct NoteText<'a> {
    number: usize,
    entry: &'a FootnoteEntry,
    ranges: Vec<Range<usize>>,
    heights: Vec<f32>,
}

#[derive(Clone, Copy)]
struct NoteLine {
    note: usize,
    line: usize,
}

pub(crate) fn inline_advances_for_block(
    doc: &WriterDocument,
    block_id: u64,
    font_size_pt: f32,
) -> Vec<InlineAdvance> {
    doc.numbered_footnotes()
        .into_iter()
        .filter(|(_, note)| note.block_id == block_id)
        .map(|(number, note)| InlineAdvance {
            offset: note.offset,
            width_pt: reference_marker_advance(number, font_size_pt),
        })
        .collect()
}

/// Inline marker advances for references inside one typed table cell.
pub(crate) fn inline_advances_for_table_cell(
    doc: &WriterDocument,
    block_id: u64,
    table: &Table,
    row: usize,
    column: usize,
    font_size_pt: f32,
) -> Vec<InlineAdvance> {
    let Some(cell_range) = crate::TableTextMap::new(table).cell_range(row, column) else {
        return Vec::new();
    };
    doc.numbered_footnotes()
        .into_iter()
        .filter(|(_, note)| {
            note.block_id == block_id
                && cell_range.start <= note.offset
                && note.offset <= cell_range.end
        })
        .map(|(number, note)| InlineAdvance {
            offset: note.offset - cell_range.start,
            width_pt: reference_marker_advance(number, font_size_pt),
        })
        .collect()
}

pub(crate) fn reference_marker_advance(number: usize, body_size_pt: f32) -> f32 {
    let label = number.to_string();
    let size_pt = (body_size_pt * 0.65).max(5.0);
    let marker = RichBlock::new(0, "paragraph", label.as_str());
    shape_block_line(&marker, 0..label.len(), size_pt, size_pt * 4.0)
        .map(|line| line.width_pt())
        .filter(|width| width.is_finite() && *width > 0.0)
        .unwrap_or_else(|| measure_block(&marker, size_pt).advance(0, label.len()))
}

type FootnotePagination = (
    Vec<FlowLine>,
    Vec<Range<usize>>,
    Vec<FootnoteReferencePlacement>,
    Vec<FootnoteLinePlacement>,
);

/// Add reference-page note reservations and deterministic note-only
/// continuation pages to the body pagination.
pub(super) fn paginate(
    doc: &WriterDocument,
    style: &PageStyle,
    lines: Vec<FlowLine>,
    page_lines: Vec<Range<usize>>,
    block_starts: &[usize],
) -> Result<FootnotePagination, String> {
    if doc.footnotes.is_empty() {
        return Ok((lines, page_lines, Vec::new(), Vec::new()));
    }
    doc.validate_footnotes()?;

    let usable_width = style.width_pt - style.margin_left_pt - style.margin_right_pt;
    let usable_height = style.height_pt - style.margin_top_pt - style.margin_bottom_pt;
    let font_size = (style.body_font_size_pt * FOOTNOTE_FONT_SCALE).max(6.0);
    let text_width = usable_width - FOOTNOTE_LABEL_WIDTH_PT;
    if !text_width.is_finite() || text_width <= 0.0 {
        return Err("page is too narrow for footnote labels and text".into());
    }

    let mut notes = Vec::with_capacity(doc.footnotes.len());
    for (number, entry) in doc.numbered_footnotes() {
        let block = RichBlock::new(entry.id, "paragraph", entry.body.as_str());
        let shaped = shape_wrapped_block_lines(&block, font_size, text_width);
        let (ranges, shaped_heights): (Vec<Range<usize>>, Vec<f32>) = if let Some(lines) = shaped {
            lines
                .into_iter()
                .map(|(range, shaped)| {
                    let height = shaped.as_ref().map_or(0.0, |line| {
                        line.ascent_pt() + line.descent_pt() + line.line_gap_pt()
                    });
                    (range, height)
                })
                .unzip()
        } else {
            (
                measure_block(&block, font_size)
                    .wrap(entry.body.as_str(), text_width)
                    .into_iter()
                    .map(|(start, end)| start..end)
                    .collect(),
                Vec::new(),
            )
        };
        let ranges = if ranges.is_empty() {
            std::iter::once(0..entry.body.len()).collect()
        } else {
            ranges
        };
        let base_height = font_size * style.line_height;
        let heights = ranges
            .iter()
            .enumerate()
            .map(|(index, _)| {
                shaped_heights
                    .get(index)
                    .copied()
                    .unwrap_or(0.0)
                    .max(base_height)
            })
            .collect::<Vec<_>>();
        if heights
            .iter()
            .any(|height| !height.is_finite() || *height <= 0.0)
        {
            return Err(format!("footnote {} has invalid line geometry", entry.id));
        }
        notes.push(NoteText {
            number,
            entry,
            ranges,
            heights,
        });
    }

    let mut body_pages: Vec<Vec<FlowLine>> = page_lines
        .iter()
        .map(|range| lines[range.clone()].to_vec())
        .collect();
    let mut explicit_empty: Vec<bool> = body_pages.iter().map(Vec::is_empty).collect();
    if body_pages.is_empty() {
        body_pages.push(Vec::new());
        explicit_empty.push(false);
    }

    // Keep every reference with its body line. If note space cannot fit below
    // the current body, move the body suffix after the last reference first,
    // then move that reference line as needed. This avoids floating anchors.
    let body_line_height = style.body_font_size_pt * style.line_height;
    let mut page = 0usize;
    while page < body_pages.len() {
        let anchors = anchor_locations(doc, &notes, &body_pages)?;
        let page_anchors = anchors
            .iter()
            .enumerate()
            .filter_map(|(note, (anchor_page, anchor_line))| {
                (*anchor_page == page).then_some((note, *anchor_line))
            })
            .collect::<Vec<_>>();
        if page_anchors.is_empty() {
            page += 1;
            continue;
        }

        let required_opening_height = page_anchors
            .iter()
            .map(|(note, _)| notes[*note].heights[0])
            .sum::<f32>();
        let used = body_height(&body_pages[page], body_line_height);
        let available = usable_height - used - FOOTNOTE_SEPARATOR_GAP_PT;
        if available + f32::EPSILON >= required_opening_height {
            page += 1;
            continue;
        }

        let Some((_, anchor_line)) = page_anchors.iter().max_by_key(|(_, line)| *line) else {
            page += 1;
            continue;
        };
        let split = if anchor_line + 1 < body_pages[page].len() {
            anchor_line + 1
        } else {
            *anchor_line
        };
        if split == 0
            && body_pages[page].len() == 1
            && body_pages[page][0].bounds.height
                + FOOTNOTE_SEPARATOR_GAP_PT
                + required_opening_height
                > usable_height + f32::EPSILON
        {
            return Err(
                "footnote reference and body line do not fit within the page margins".into(),
            );
        }

        let moved = body_pages[page].split_off(split);
        if moved.is_empty() {
            return Err("footnote pagination could not move body text to make note space".into());
        }
        if page + 1 == body_pages.len() {
            body_pages.push(Vec::new());
            explicit_empty.push(false);
        }
        body_pages[page + 1].splice(0..0, moved);
        reflow_overfull_pages(
            &mut body_pages,
            &mut explicit_empty,
            page + 1,
            usable_height,
            body_line_height,
        )?;
        if body_pages[page].is_empty() && !explicit_empty[page] {
            body_pages.remove(page);
            explicit_empty.remove(page);
        }
    }

    let anchors = anchor_locations(doc, &notes, &body_pages)?;
    let mut notes_by_page = vec![Vec::new(); body_pages.len()];
    for (note, (page, _)) in anchors.iter().enumerate() {
        notes_by_page[*page].push(note);
    }

    let mut body_output = Vec::new();
    let mut references = Vec::with_capacity(notes.len());
    let mut note_lines = Vec::new();
    let mut reference_pages = vec![0usize; notes.len()];
    let mut continuations: Vec<(usize, NoteLine)> = Vec::new();
    for (body_page, mut body) in body_pages.into_iter().enumerate() {
        let reference_page = body_output.len();
        for line in &mut body {
            line.page = reference_page;
        }
        for note_index in &notes_by_page[body_page] {
            let note = &notes[*note_index];
            let (anchor_page, _) = anchors[*note_index];
            reference_pages[*note_index] = reference_page;
            debug_assert_eq!(anchor_page, body_page);
            let block_index = doc
                .blocks
                .iter()
                .position(|block| block.id == note.entry.block_id)
                .ok_or_else(|| format!("footnote {} has no source block", note.entry.id))?;
            references.push(FootnoteReferencePlacement {
                note_id: note.entry.id,
                number: note.number,
                block_id: note.entry.block_id,
                block_index,
                offset: note.entry.offset,
                global_offset: block_starts[block_index] + note.entry.offset,
                page: reference_page,
                marker_width_pt: reference_marker_advance(
                    note.number,
                    if doc.blocks[block_index].kind == crate::TABLE_BLOCK_KIND {
                        crate::TABLE_CELL_BASE_FONT_SIZE_PT
                    } else {
                        style.font_size_for_kind(doc.blocks[block_index].kind.as_str())
                    },
                ),
            });
        }

        let body_used = body_height(&body, body_line_height);
        let mut note_room = (usable_height - body_used - FOOTNOTE_SEPARATOR_GAP_PT).max(0.0);
        let mut next_line = vec![0usize; notes.len()];
        let mut reference_lines = Vec::new();
        let mut opening_reservation = notes_by_page[body_page]
            .iter()
            .map(|note| notes[*note].heights[0])
            .sum::<f32>();
        for note_index in &notes_by_page[body_page] {
            let first_height = notes[*note_index].heights[0];
            if first_height > note_room + f32::EPSILON {
                return Err("footnote opening lines do not fit on their reference page".into());
            }
            reference_lines.push(NoteLine {
                note: *note_index,
                line: 0,
            });
            note_room -= first_height;
            opening_reservation -= first_height;
            next_line[*note_index] = 1;
            while let Some(height) = notes[*note_index].heights.get(next_line[*note_index]) {
                if *height + opening_reservation > note_room + f32::EPSILON {
                    break;
                }
                reference_lines.push(NoteLine {
                    note: *note_index,
                    line: next_line[*note_index],
                });
                note_room -= *height;
                next_line[*note_index] += 1;
            }
        }
        let reference_height = reference_lines
            .iter()
            .map(|line| notes[line.note].heights[line.line])
            .sum::<f32>();
        let note_top = style.margin_top_pt + usable_height - reference_height;
        let mut note_y = note_top;
        for (position, line) in reference_lines.iter().enumerate() {
            let note = &notes[line.note];
            let height = note.heights[line.line];
            note_lines.push(make_placement(
                note,
                line.line,
                PlacementPages {
                    page: reference_page,
                    reference_page,
                },
                PlacementGeometry {
                    y: note_y,
                    height,
                    left: style.margin_left_pt,
                    width: usable_width,
                    font_size_pt: font_size,
                },
                line.line == 0 || position > 0 && reference_lines[position - 1].note != line.note,
            ));
            note_y += height;
        }
        body_output.push(body);

        for note_index in &notes_by_page[body_page] {
            while next_line[*note_index] < notes[*note_index].ranges.len() {
                continuations.push((
                    reference_page,
                    NoteLine {
                        note: *note_index,
                        line: next_line[*note_index],
                    },
                ));
                next_line[*note_index] += 1;
            }
        }
    }
    append_continuation_pages(
        &continuations,
        &notes,
        &reference_pages,
        &mut ContinuationContext {
            style,
            usable_height,
            usable_width,
            font_size_pt: font_size,
            pages: &mut body_output,
            placements: &mut note_lines,
        },
    )?;

    let mut lines = Vec::new();
    let mut page_lines = Vec::with_capacity(body_output.len());
    for (page, body) in body_output.into_iter().enumerate() {
        let start = lines.len();
        for mut line in body {
            line.page = page;
            lines.push(line);
        }
        page_lines.push(start..lines.len());
    }
    Ok((lines, page_lines, references, note_lines))
}

struct PlacementPages {
    page: usize,
    reference_page: usize,
}

struct PlacementGeometry {
    y: f32,
    height: f32,
    left: f32,
    width: f32,
    font_size_pt: f32,
}

fn make_placement(
    note: &NoteText<'_>,
    line: usize,
    pages: PlacementPages,
    geometry: PlacementGeometry,
    show_number: bool,
) -> FootnoteLinePlacement {
    let PlacementGeometry {
        y,
        height,
        left,
        width,
        font_size_pt,
    } = geometry;
    let number_width = FOOTNOTE_LABEL_WIDTH_PT.min(width);
    FootnoteLinePlacement {
        note_id: note.entry.id,
        number: note.number,
        body_range: note.ranges[line].clone(),
        page: pages.page,
        reference_page: pages.reference_page,
        starts_note: line == 0,
        continued: pages.page != pages.reference_page && line > 0,
        show_number,
        font_size_pt,
        number_bounds: PageRect {
            x: left,
            y,
            width: number_width,
            height,
        },
        text_bounds: PageRect {
            x: left + number_width,
            y,
            width: (width - number_width).max(0.0),
            height,
        },
    }
}

struct ContinuationContext<'a> {
    style: &'a PageStyle,
    usable_height: f32,
    usable_width: f32,
    font_size_pt: f32,
    pages: &'a mut Vec<Vec<FlowLine>>,
    placements: &'a mut Vec<FootnoteLinePlacement>,
}

fn append_continuation_pages(
    remaining: &[(usize, NoteLine)],
    notes: &[NoteText<'_>],
    reference_pages: &[usize],
    context: &mut ContinuationContext<'_>,
) -> Result<(), String> {
    if remaining.is_empty() {
        return Ok(());
    }
    let mut page_lines = Vec::new();
    let mut used = 0.0;
    for (reference_page, line) in remaining {
        let note = &notes[line.note];
        let height = note.heights[line.line];
        if height > context.usable_height + f32::EPSILON {
            return Err(format!(
                "footnote {} line exceeds page height",
                note.entry.id
            ));
        }
        if used + height > context.usable_height + f32::EPSILON && !page_lines.is_empty() {
            place_continuation_page(&page_lines, notes, reference_pages, context);
            page_lines.clear();
            used = 0.0;
        }
        page_lines.push((*reference_page, *line));
        used += height;
    }
    if !page_lines.is_empty() {
        place_continuation_page(&page_lines, notes, reference_pages, context);
    }
    Ok(())
}

fn place_continuation_page(
    lines: &[(usize, NoteLine)],
    notes: &[NoteText<'_>],
    reference_pages: &[usize],
    context: &mut ContinuationContext<'_>,
) {
    let page = context.pages.len();
    let mut y = context.style.margin_top_pt;
    let mut previous_note = None;
    for (_, line) in lines {
        let note = &notes[line.note];
        let reference_page = reference_pages[line.note];
        let height = note.heights[line.line];
        let show_number = line.line == 0 || previous_note != Some(line.note);
        context.placements.push(make_placement(
            note,
            line.line,
            PlacementPages {
                page,
                reference_page,
            },
            PlacementGeometry {
                y,
                height,
                left: context.style.margin_left_pt,
                width: context.usable_width,
                font_size_pt: context.font_size_pt,
            },
            show_number,
        ));
        y += height;
        previous_note = Some(line.note);
    }
    context.pages.push(Vec::new());
}

fn anchor_locations(
    doc: &WriterDocument,
    notes: &[NoteText<'_>],
    pages: &[Vec<FlowLine>],
) -> Result<Vec<(usize, usize)>, String> {
    notes
        .iter()
        .map(|note| {
            let block_index = doc
                .blocks
                .iter()
                .position(|block| block.id == note.entry.block_id)
                .ok_or_else(|| format!("footnote {} has no source block", note.entry.id))?;
            let marker_line = pages.iter().enumerate().find_map(|(page, lines)| {
                lines
                    .iter()
                    .position(|line| {
                        line.block_index == block_index
                            && line.shaped_line.as_ref().is_some_and(|shaped| {
                                shaped.inline_marker_x(note.entry.offset).is_some()
                            })
                    })
                    .map(|line| (page, line))
            });
            let anchor_line = marker_line.or_else(|| {
                pages.iter().enumerate().find_map(|(page, lines)| {
                    lines
                        .iter()
                        .position(|line| {
                            line.block_index == block_index
                                && (contains_anchor(line, note.entry.offset)
                                    || doc.blocks[block_index].kind == crate::TABLE_BLOCK_KIND
                                        && line.start == note.entry.offset)
                        })
                        .map(|line| (page, line))
                })
            });
            anchor_line
                .ok_or_else(|| format!("footnote {} has no wrapped reference line", note.entry.id))
        })
        .collect()
}

fn contains_anchor(line: &FlowLine, offset: usize) -> bool {
    if line.start == line.end {
        offset == line.start
    } else {
        offset > line.start && offset <= line.end || offset == line.start && line.start == 0
    }
}

fn body_height(lines: &[FlowLine], paragraph_gap: f32) -> f32 {
    let mut used = 0.0;
    let mut previous_block = None;
    for line in lines {
        if previous_block.is_some_and(|block| block != line.block_index) {
            used += paragraph_gap;
        }
        let shaped_height = line.shaped_line.as_ref().map_or(0.0, |shaped| {
            shaped.ascent_pt() + shaped.descent_pt() + shaped.line_gap_pt()
        });
        used += line.bounds.height.max(shaped_height);
        previous_block = Some(line.block_index);
    }
    used
}

/// Repack the pages after a note reservation moves body lines forward. A
/// moved suffix can make the destination page too tall, so spill its tail
/// forward until every affected page fits the ordinary body area.
fn reflow_overfull_pages(
    pages: &mut Vec<Vec<FlowLine>>,
    explicit_empty: &mut Vec<bool>,
    start_page: usize,
    usable_height: f32,
    paragraph_gap: f32,
) -> Result<(), String> {
    let mut page = start_page;
    while page < pages.len() {
        if body_height(&pages[page], paragraph_gap) <= usable_height + f32::EPSILON {
            page += 1;
            continue;
        }

        let mut used = 0.0;
        let mut previous_block = None;
        let mut split = 0;
        for (index, line) in pages[page].iter().enumerate() {
            if previous_block.is_some_and(|block| block != line.block_index) {
                used += paragraph_gap;
            }
            let shaped_height = line.shaped_line.as_ref().map_or(0.0, |shaped| {
                shaped.ascent_pt() + shaped.descent_pt() + shaped.line_gap_pt()
            });
            used += line.bounds.height.max(shaped_height);
            if used > usable_height + f32::EPSILON {
                break;
            }
            split = index + 1;
            previous_block = Some(line.block_index);
        }
        if split == 0 {
            return Err("body line exceeds page height during footnote reflow".into());
        }

        let moved = pages[page].split_off(split);
        if page + 1 == pages.len() {
            pages.push(Vec::new());
            explicit_empty.push(false);
        }
        explicit_empty[page + 1] = false;
        pages[page + 1].splice(0..0, moved);
        page += 1;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PageViewport, RichBlock};

    fn tiny_style() -> PageStyle {
        PageStyle {
            width_pt: 400.0,
            height_pt: 50.0,
            margin_top_pt: 0.0,
            margin_bottom_pt: 0.0,
            margin_left_pt: 0.0,
            margin_right_pt: 0.0,
            body_font_size_pt: 10.0,
            line_height: 1.0,
        }
    }

    fn flow(doc: &WriterDocument, style: &PageStyle) -> crate::DocumentFlow {
        doc.flow(
            style,
            PageViewport {
                width: style.width_pt,
                height: style.height_pt,
                ..PageViewport::default()
            },
        )
        .expect("valid footnote layout")
    }

    fn paragraph_doc(text: &str) -> WriterDocument {
        let mut doc = WriterDocument::new("footnote-layout", "Footnote layout");
        doc.blocks.push(RichBlock::new(7, "paragraph", text));
        doc
    }

    fn add_note(doc: &mut WriterDocument, id: u64, block_id: u64, offset: usize, body: &str) {
        doc.footnotes.push(FootnoteEntry {
            id,
            block_id,
            offset,
            body: body.into(),
        });
    }

    #[test]
    fn a_reference_reserves_note_space_and_moves_following_body_lines() {
        let mut doc = paragraph_doc("first\nsecond\nthird\nfourth\nfifth");
        add_note(&mut doc, 41, 7, 2, "a short note");
        let flow = flow(&doc, &tiny_style());

        let reference = &flow.footnote_references()[0];
        assert_eq!(reference.page, 0);
        assert!(flow.lines().iter().any(|line| line.page > reference.page));
        assert!(
            flow.lines()
                .iter()
                .filter(|line| line.page == reference.page)
                .count()
                < 5,
            "the note reservation moves body lines off its reference page"
        );
        assert!(flow
            .footnote_lines()
            .iter()
            .any(|line| line.note_id == reference.note_id && line.page == reference.page));
    }

    #[test]
    fn overflowing_note_continues_on_deterministic_note_only_pages() {
        let mut doc = paragraph_doc("source\nbody one\nbody two\nbody three");
        let body = (0..12)
            .map(|n| format!("note {n}"))
            .collect::<Vec<_>>()
            .join("\n");
        add_note(&mut doc, 41, 7, 0, &body);
        let flow = flow(&doc, &tiny_style());

        let reference = &flow.footnote_references()[0];
        let continuation_pages = flow
            .footnote_lines()
            .iter()
            .filter(|line| line.note_id == reference.note_id && line.continued)
            .map(|line| line.page)
            .collect::<std::collections::BTreeSet<_>>();
        assert!(!continuation_pages.is_empty());
        assert!(continuation_pages.iter().any(|page| {
            flow.lines_on(*page..*page + 1).is_empty()
                && flow
                    .footnote_lines()
                    .iter()
                    .any(|line| line.page == *page && line.note_id == reference.note_id)
        }));
        let continuation = *continuation_pages.first().expect("continuation page");
        assert_eq!(flow.editor_span(continuation..continuation + 1), None);
    }

    #[test]
    fn reference_numbers_follow_document_order_not_storage_order() {
        let mut doc = WriterDocument::new("footnote-order", "Footnote order");
        doc.blocks.push(RichBlock::new(7, "paragraph", "first"));
        doc.blocks.push(RichBlock::new(2, "paragraph", "second"));
        add_note(&mut doc, 20, 2, 0, "second note");
        add_note(&mut doc, 10, 7, 0, "first note");
        let flow = flow(&doc, &PageStyle::default());

        assert_eq!(
            flow.footnote_references()
                .iter()
                .map(|reference| (reference.note_id, reference.number))
                .collect::<Vec<_>>(),
            vec![(10, 1), (20, 2)]
        );
    }

    #[test]
    fn inline_markers_reserve_ordinal_advance_through_word_wraps_and_carets() {
        let text = "antidisestablishmentarianism antidisestablishmentarianism";
        let mut doc = paragraph_doc(text);
        let mut style = tiny_style();
        style.width_pt = 120.0;
        style.height_pt = 160.0;
        style.margin_left_pt = 10.0;
        style.margin_right_pt = 10.0;
        let first_wrap = flow(&doc, &style)
            .lines()
            .iter()
            .find(|line| line.start > 0)
            .map(|line| line.start)
            .expect("long word wraps inside the text column");
        assert!(first_wrap > 9 && text.as_bytes()[first_wrap] != b' ');
        for offset in 1..=9 {
            add_note(&mut doc, offset as u64, 7, offset, "note");
        }
        add_note(&mut doc, 10, 7, first_wrap, "note");

        let flow = flow(&doc, &style);
        let references = flow.footnote_references();
        assert_eq!(references.len(), 10);
        let first = references
            .iter()
            .find(|reference| reference.number == 1)
            .expect("first ordinal is laid out");
        let tenth = references
            .iter()
            .find(|reference| reference.number == 10)
            .expect("two-digit ordinal is laid out");
        assert!(tenth.marker_width_pt > first.marker_width_pt);

        for reference in references {
            let line = flow
                .lines()
                .iter()
                .find(|line| {
                    line.page == reference.page
                        && line.block_index == reference.block_index
                        && line.shaped_line.as_ref().is_some_and(|shaped| {
                            shaped.inline_marker_x(reference.offset).is_some()
                        })
                })
                .expect("each marker belongs to a shaped source line");
            let shaped = line.shaped_line.as_ref().expect("shaped line");
            let before = shaped.caret_x(reference.offset, loom_fonts::Affinity::Leading);
            let after = shaped.caret_x(reference.offset, loom_fonts::Affinity::Trailing);
            let marker_x = shaped
                .inline_marker_x(reference.offset)
                .expect("marker x shares the source line geometry");
            assert!((after - before - reference.marker_width_pt).abs() < 0.05);
            assert!((marker_x - before).abs() < 0.05);
            assert!(shaped.width_pt() <= 100.0 + 0.1);
        }
    }

    #[test]
    fn competing_notes_keep_openings_on_reference_page_and_continuations_in_order() {
        let text = "Alpha anchor.\nOmega anchor.\nBody keeps height.";
        let mut doc = paragraph_doc(text);
        let second_anchor = "Alpha anchor.\nOmega anchor.\n".len() + "Body".len();
        add_note(
            &mut doc,
            10,
            7,
            "Alpha".len(),
            &"First note continuation. ".repeat(80),
        );
        add_note(
            &mut doc,
            20,
            7,
            second_anchor,
            &"Second note continuation. ".repeat(80),
        );
        let mut style = tiny_style();
        style.height_pt = 50.0;

        let flow = flow(&doc, &style);
        let references = flow.footnote_references();
        assert_eq!(
            references
                .iter()
                .map(|reference| (reference.note_id, reference.page))
                .collect::<Vec<_>>(),
            vec![(10, 0), (20, 1)],
            "the later anchor moves when one page cannot open both notes",
        );
        for reference in references {
            assert!(
                flow.footnote_lines().iter().any(|line| {
                    line.note_id == reference.note_id
                        && line.page == reference.page
                        && line.starts_note
                }),
                "note {} has no opening line on reference page {}",
                reference.note_id,
                reference.page,
            );
        }
        for page in 1..flow.page_count() {
            let note_ids = flow
                .footnote_lines()
                .iter()
                .filter(|line| line.page == page)
                .map(|line| line.note_id)
                .collect::<Vec<_>>();
            assert!(
                note_ids.windows(2).all(|pair| pair[0] <= pair[1]),
                "continuation page {page} changed note order: {note_ids:?}"
            );
        }
    }

    #[test]
    fn table_cell_reference_uses_the_table_rows_utf8_offsets() {
        let mut table = crate::loom_objects::table::Table::new(1, 1);
        table.set_cell_text(0, 0, "Cell text");
        let mut doc = WriterDocument::new("footnote-table", "Footnote table");
        doc.blocks.push(RichBlock::table_block(7, table));
        let range = crate::TableTextMap::new(doc.blocks[0].typed_table().unwrap())
            .cell_range(0, 0)
            .unwrap();
        let offset = range.start + "Cell".len();
        add_note(&mut doc, 41, 7, offset, "cell note");
        let flow = flow(&doc, &tiny_style());

        let reference = &flow.footnote_references()[0];
        assert_eq!(reference.block_index, 0);
        assert_eq!(reference.offset, offset);
        assert_eq!(flow.lines()[0].page, reference.page);
    }

    #[test]
    fn table_cell_reference_at_a_later_row_start_keeps_its_anchor() {
        let mut table = crate::loom_objects::table::Table::new(2, 1);
        table.set_cell_text(0, 0, "First row");
        table.set_cell_text(1, 0, "Second row");
        let mut doc = WriterDocument::new("footnote-table-row-start", "Table row start");
        doc.blocks.push(RichBlock::table_block(7, table));
        let table = doc.blocks[0].typed_table().unwrap();
        let offset = crate::TableTextMap::new(table)
            .cell_range(1, 0)
            .unwrap()
            .start;
        add_note(&mut doc, 43, 7, offset, "row-start note");

        let mut style = tiny_style();
        style.height_pt = 120.0;
        let flow = flow(&doc, &style);
        let reference = flow.footnote_references().first().unwrap();
        assert_eq!(reference.offset, offset);
        assert_eq!(reference.page, 0);
        assert!(flow.footnote_reference_rect(&doc, reference).is_some());
    }

    #[test]
    fn table_cell_reference_reserves_advance_for_wrap_caret_and_selection() {
        let text = "antidisestablishmentarianism antidisestablishmentarianism";
        let mut table = crate::loom_objects::table::Table::new(1, 1);
        table.set_cell_text(0, 0, text);
        let mut doc = WriterDocument::new("footnote-table-geometry", "Footnote table geometry");
        doc.blocks.push(RichBlock::table_block(7, table));
        let cell_range = crate::TableTextMap::new(doc.blocks[0].typed_table().unwrap())
            .cell_range(0, 0)
            .unwrap();
        let offset = cell_range.start + 20;
        doc.footnotes.push(crate::FootnoteEntry {
            id: 42,
            block_id: 7,
            offset,
            body: "table cell note".into(),
        });

        let mut style = tiny_style();
        style.width_pt = 160.0;
        style.height_pt = 320.0;
        style.margin_left_pt = 10.0;
        style.margin_right_pt = 10.0;
        let block = &doc.blocks[0];
        let table = block.typed_table().unwrap();
        let inner_width = (block.text_column_width(&style)
            - 2.0 * crate::TABLE_CELL_HORIZONTAL_INSET_PT)
            .max(1.0);
        let cell_block = crate::table_cell_rich_block(
            block.id,
            table,
            0,
            0,
            crate::TABLE_CELL_BASE_FONT_SIZE_PT,
            true,
            None,
        )
        .unwrap();
        let geometry = crate::shape_table_cell_geometry_with_footnotes(
            &doc,
            block.id,
            table,
            (0, 0),
            &cell_block,
            crate::TABLE_CELL_BASE_FONT_SIZE_PT,
            inner_width,
        )
        .unwrap();
        assert!(geometry.lines().len() > 1, "the cell fixture must wrap");
        let local_offset = offset - cell_range.start;
        let (marker_line_index, marker_line) = geometry
            .lines()
            .iter()
            .enumerate()
            .find(|(_, line)| {
                line.shaped()
                    .is_some_and(|shaped| shaped.inline_marker_x(local_offset).is_some())
            })
            .expect("the table cell owns a shaped inline marker");
        let shaped = marker_line.shaped().unwrap();
        let marker_x = shaped.inline_marker_x(local_offset).unwrap();
        let before = shaped.caret_x(local_offset, loom_fonts::Affinity::Leading);
        let after = shaped.caret_x(local_offset, loom_fonts::Affinity::Trailing);
        let flow = flow(&doc, &style);
        let reference = flow.footnote_references().first().unwrap();
        assert!((after - before - reference.marker_width_pt).abs() < 0.05);
        assert!(shaped.width_pt() <= inner_width + 0.1);

        let marker_rect = flow
            .footnote_reference_rect(&doc, reference)
            .expect("the page projection uses the cell-local marker placement");
        let flow_line = flow
            .lines()
            .iter()
            .find(|line| line.block_index == 0 && line.start <= offset && offset <= line.end)
            .unwrap();
        let expected_x =
            flow_line.bounds.x + (crate::TABLE_CELL_HORIZONTAL_INSET_PT + marker_x) * flow.zoom();
        assert!((marker_rect.x - expected_x).abs() < 0.05);
        let expected_y = flow_line.bounds.y
            + (crate::TABLE_CELL_VERTICAL_INSET_PT
                + geometry.lines()[..marker_line_index]
                    .iter()
                    .map(|line| line.pitch_pt())
                    .sum::<f32>())
                * flow.zoom();
        assert!((marker_rect.y - expected_y).abs() < 0.05);
        let mut caret_selection = crate::TextSelection::caret(reference.global_offset);
        caret_selection.affinity = crate::CaretAffinity::Downstream;
        let caret = flow
            .caret_rect(&doc, &caret_selection)
            .expect("table caret shares the cell's shaped geometry");
        assert!((caret.rect.x - marker_rect.x).abs() < 0.05);

        let selection = flow.selection_rects(
            &doc,
            &crate::TextSelection::range(reference.global_offset - 1, reference.global_offset + 1),
            0..flow.page_count(),
        );
        let actual_width = selection
            .iter()
            .filter(|rect| rect.block_id == block.id)
            .map(|rect| rect.rect.width / flow.zoom())
            .sum::<f32>();
        let expected_width = geometry
            .lines()
            .iter()
            .filter_map(|line| line.shaped())
            .flat_map(|line| line.selection_spans(local_offset - 1..local_offset + 1))
            .map(|span| span.width_pt)
            .sum::<f32>();
        assert!((actual_width - expected_width).abs() < 0.1);
    }

    #[test]
    fn body_and_note_lines_stay_inside_margins_without_overlap() {
        let mut doc = paragraph_doc("a\nb\nc\nd\ne\nf");
        add_note(
            &mut doc,
            41,
            7,
            0,
            "note one\nnote two\nnote three\nnote four\nnote five",
        );
        let style = PageStyle {
            margin_top_pt: 4.0,
            margin_bottom_pt: 4.0,
            ..tiny_style()
        };
        let flow = flow(&doc, &style);
        let mut body_bottom = std::collections::HashMap::<usize, f32>::new();
        for line in flow.lines() {
            let page = flow.page_bounds()[line.page];
            let top = line.bounds.y - page.y;
            let bottom = top + line.bounds.height;
            assert!(top >= style.margin_top_pt - f32::EPSILON);
            assert!(bottom <= style.height_pt - style.margin_bottom_pt + f32::EPSILON);
            body_bottom
                .entry(line.page)
                .and_modify(|used| *used = used.max(bottom))
                .or_insert(bottom);
        }
        let mut note_top = std::collections::HashMap::<usize, f32>::new();
        for line in flow.footnote_lines() {
            let page = flow.page_bounds()[line.page];
            let top = line.text_bounds.y - page.y;
            let bottom = top + line.text_bounds.height;
            assert!(top >= style.margin_top_pt - f32::EPSILON);
            assert!(bottom <= style.height_pt - style.margin_bottom_pt + f32::EPSILON);
            note_top
                .entry(line.page)
                .and_modify(|used| *used = used.min(top))
                .or_insert(top);
        }
        for (page, bottom) in body_bottom {
            if let Some(top) = note_top.get(&page) {
                assert!(
                    bottom <= *top + f32::EPSILON,
                    "body overlaps notes on page {page}"
                );
            }
        }
    }

    #[test]
    fn moving_body_suffix_spills_repeatedly_in_document_order() {
        let sizing_text = (0..60)
            .map(|line| format!("line {line}"))
            .collect::<Vec<_>>()
            .join("\n");
        let sizing_doc = paragraph_doc(&sizing_text);
        let sizing_flow = flow(&sizing_doc, &tiny_style());
        let page_capacity = sizing_flow
            .lines()
            .iter()
            .filter(|line| line.page == 0)
            .count();
        assert!(page_capacity > 1, "the fixture needs a multi-line page");

        let text = (0..page_capacity * 3)
            .map(|line| format!("line {line}"))
            .collect::<Vec<_>>()
            .join("\n");
        let mut doc = paragraph_doc(&text);
        add_note(&mut doc, 41, 7, 0, "a short note");
        let flow = flow(&doc, &tiny_style());

        let expected_starts = text
            .split_inclusive('\n')
            .scan(0usize, |offset, line| {
                let start = *offset;
                *offset += line.len();
                Some(start)
            })
            .collect::<Vec<_>>();
        let actual_starts = flow
            .lines()
            .iter()
            .map(|line| line.start)
            .collect::<Vec<_>>();
        assert_eq!(actual_starts, expected_starts);
        assert_eq!(flow.page_count(), 4);
        assert_eq!(
            (0..flow.page_count())
                .map(|page| flow.lines().iter().filter(|line| line.page == page).count())
                .collect::<Vec<_>>(),
            vec![1, page_capacity, page_capacity, page_capacity - 1]
        );
    }
}
