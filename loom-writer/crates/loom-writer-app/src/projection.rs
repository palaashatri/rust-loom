//! Projection of the page stack into the Slint models.
//!
//! The editor draws a long document one screen at a time, so only the pages
//! near the viewport are turned into rows, selection rectangles and comment
//! rectangles. The cost of a keystroke or a scroll step then follows what is
//! on screen, not how long the document is. Geometry still comes from the core
//! layout of the whole document, so positions are identical either way.

use std::collections::HashMap;
use std::ops::Range;

use loom_writer_core::fonts;
use loom_writer_core::{
    DocumentFlow, PageRect, PageStyle, PageViewport, RichBlock, SelectionRect, TextSelection,
    WriterDocument,
};
use slint::SharedString;

/// Screens of height kept when the view height is not yet known.
const PAGE_VIEW_FALLBACK_SCREENS: f32 = 2.0;

use crate::{
    normalize_page_scroll, normalize_page_zoom, writer_render_alignment,
    writer_render_height_for_width, writer_render_markup, WriterCommentRect, WriterRenderBlock,
    WriterSelectionRect,
};

/// Everything the page canvas shows for one document state.
pub(crate) struct Projection {
    pub rows: Vec<WriterRenderBlock>,
    pub selection_rects: Vec<WriterSelectionRect>,
    pub comment_rects: Vec<WriterCommentRect>,
    pub page_count: i32,
    pub stack_height: f32,
    /// Pages the rows, selection and comment rectangles cover.
    pub pages: Range<usize>,
}

/// Layout viewport in page points: width and height stay the page's so the
/// projection is independent of shell width; zoom and scroll are controller
/// state.
pub(crate) fn layout_viewport(style: &PageStyle, viewport: PageViewport) -> PageViewport {
    PageViewport {
        width: style.width_pt,
        height: style.height_pt,
        zoom: normalize_page_zoom(viewport.zoom, 1.0),
        scroll_x: normalize_page_scroll(viewport.scroll_x),
        scroll_y: normalize_page_scroll(viewport.scroll_y),
    }
}

/// The pages worth materialising for a view `view_height` pixels tall: those
/// within one screen above and below it. An unknown height keeps a few pages.
pub(crate) fn window_pages(
    flow: &DocumentFlow,
    style: &PageStyle,
    view_height: f32,
) -> Range<usize> {
    let span = if view_height > 1.0 {
        view_height
    } else {
        style.height_pt * flow.zoom() * PAGE_VIEW_FALLBACK_SCREENS
    };
    flow.visible_pages(-span, 2.0 * span)
}

/// The number shown beside each numbered list item, `0` elsewhere. Numbers
/// run across consecutive numbered blocks and restart after any other kind.
fn list_numbers(doc: &WriterDocument) -> Vec<usize> {
    let mut counter = 0usize;
    doc.blocks
        .iter()
        .map(|block| match block.kind.as_str() {
            "list-numbered" => {
                counter += 1;
                counter
            }
            _ => {
                counter = 0;
                0
            }
        })
        .collect()
}

fn list_marker(kind: &str, number: usize) -> String {
    match kind {
        "list-bulleted" => "\u{2022}".to_string(),
        "list-numbered" => format!("{number}."),
        _ => String::new(),
    }
}

fn floor_char_boundary(text: &str, mut index: usize) -> usize {
    index = index.min(text.len());
    while !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn fragment_block(block: &RichBlock, start: usize, end: usize) -> RichBlock {
    let text = block.text.as_str();
    let start = floor_char_boundary(text, start);
    let end = floor_char_boundary(text, end).max(start);
    let mut fragment = RichBlock::new(block.id, block.kind.as_str(), &text[start..end]);
    fragment.style = block.style.clone();
    fragment.runs = block
        .runs
        .iter()
        .filter_map(|run| {
            let run_start = run.start.max(start);
            let run_end = run.end.min(end);
            (run_start < run_end).then(|| loom_text::StyleRun {
                start: run_start - start,
                end: run_end - start,
                style: run.style.clone(),
            })
        })
        .collect();
    fragment
}

/// A stretch of one line set in one font family.
struct Segment {
    start: usize,
    end: usize,
    /// The family as the document stores it.
    family: String,
}

/// The stretches of bytes `start..end` of `block` that are set in one family.
/// Names that mean the same family (case, "Sans", the unnamed family) are one
/// stretch, which is also where measurement shapes a new piece.
fn line_segments(block: &RichBlock, start: usize, end: usize) -> Vec<Segment> {
    let text = block.text.as_str();
    let mut cuts = vec![start, end];
    for run in &block.runs {
        for edge in [run.start, run.end] {
            if edge > start && edge < end && text.is_char_boundary(edge) {
                cuts.push(edge);
            }
        }
    }
    cuts.sort_unstable();
    cuts.dedup();
    let mut segments: Vec<Segment> = Vec::new();
    for pair in cuts.windows(2) {
        let (from, to) = (pair[0], pair[1]);
        let family = block
            .runs
            .iter()
            .find(|run| run.start <= from && from < run.end)
            .map_or("", |run| run.style.font_family.as_str());
        match segments.last_mut() {
            Some(last) if fonts::same_family(&last.family, family) => last.end = to,
            _ => segments.push(Segment {
                start: from,
                end: to,
                family: family.to_owned(),
            }),
        }
    }
    if segments.is_empty() {
        segments.push(Segment {
            start,
            end,
            family: String::new(),
        });
    }
    segments
}

/// The family the page draws `stored` in: empty for the document font, which
/// the markup already uses, else the installed family that stands for it.
fn drawn_family(stored: &str, cache: &mut HashMap<String, String>) -> String {
    if fonts::is_document_font(stored) {
        return String::new();
    }
    cache
        .entry(stored.to_owned())
        .or_insert_with(|| {
            let drawn = fonts::resolve_family(stored).drawn;
            if drawn == fonts::DEFAULT_FAMILY {
                String::new()
            } else {
                drawn
            }
        })
        .clone()
}

fn render_row(
    block: &RichBlock,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    marker: &str,
    family: &str,
) -> WriterRenderBlock {
    let unsupported = matches!(block.style.alignment, loom_text::Alignment::Justify);
    WriterRenderBlock {
        content: slint::StyledText::from_markdown(&writer_render_markup(block))
            .unwrap_or_else(|_| slint::StyledText::from_plain_text(block.text.as_str())),
        x,
        y,
        width,
        height,
        font_size: PageStyle::default().font_size_for_kind(block.kind.as_str()),
        font_family: SharedString::from(family),
        alignment: writer_render_alignment(block.style.alignment),
        marker: SharedString::from(marker),
        unsupported,
        unsupported_label: if unsupported {
            SharedString::from("Justify unavailable")
        } else {
            SharedString::default()
        },
    }
}

/// Selection rectangles in the editor's content coordinates: relative to the
/// first page's content box, unzoomed.
pub(crate) fn ui_selection_rects(
    style: &PageStyle,
    zoom: f32,
    base_page: PageRect,
    ranges: &[SelectionRect],
) -> Vec<WriterSelectionRect> {
    ranges
        .iter()
        .map(|selection| WriterSelectionRect {
            page_index: selection.page_index as i32,
            x: ((selection.rect.x - base_page.x - style.margin_left_pt * zoom) / zoom).max(0.0),
            y: ((selection.rect.y - base_page.y - style.margin_top_pt * zoom) / zoom).max(0.0),
            width: (selection.rect.width / zoom).max(1.0),
            height: (selection.rect.height / zoom).max(1.0),
            caret: selection.start == selection.end,
        })
        .collect()
}

/// Rows for every block when the page layout is unavailable: each block gets
/// a conservative height from the content width, stacked in order.
fn fallback_rows(doc: &WriterDocument, style: &PageStyle) -> Vec<WriterRenderBlock> {
    let content_width = (style.width_pt - style.margin_left_pt - style.margin_right_pt).max(1.0);
    let numbers = list_numbers(doc);
    let mut y = 0.0_f32;
    doc.blocks
        .iter()
        .enumerate()
        .map(|(index, block)| {
            let font_size = style.font_size_for_kind(block.kind.as_str());
            let height = writer_render_height_for_width(block, font_size, content_width)
                .max(font_size * style.line_height);
            let marker = list_marker(block.kind.as_str(), numbers[index]);
            let row = render_row(block, 0.0, y, content_width, height, &marker, "");
            y += height;
            row
        })
        .collect()
}

/// Rectangles for each live comment, on the window's pages. The marker flag
/// stays on a thread's first rectangle in the whole document, so a thread that
/// starts above the window never grows a second marker inside it.
fn comment_rects(
    doc: &WriterDocument,
    style: &PageStyle,
    flow: &DocumentFlow,
    base_page: PageRect,
    pages: &Range<usize>,
) -> Vec<WriterCommentRect> {
    if doc.comments.is_empty() {
        return Vec::new();
    }
    let zoom = flow.zoom().max(f32::EPSILON);
    let mut block_starts = HashMap::with_capacity(doc.blocks.len());
    let mut cursor = 0usize;
    for (index, block) in doc.blocks.iter().enumerate() {
        block_starts.entry(block.id).or_insert((index, cursor));
        cursor += block.text.as_str().len() + 1;
    }
    let mut projected = Vec::new();
    for thread in &doc.comments {
        if thread.orphaned {
            continue;
        }
        let Some(&(index, block_start)) = block_starts.get(&thread.block_id) else {
            continue;
        };
        let text = doc.blocks[index].text.as_str();
        if thread.start >= thread.end
            || thread.end > text.len()
            || !text.is_char_boundary(thread.start)
            || !text.is_char_boundary(thread.end)
        {
            continue;
        }
        let range = TextSelection::range(block_start + thread.start, block_start + thread.end);
        let rectangles = flow.selection_rects(doc, &range, 0..flow.page_count());
        let label = SharedString::from(format!("Show comment by {} on page", thread.author));
        let ui_rectangles = ui_selection_rects(style, zoom, base_page, &rectangles);
        for (position, rect) in ui_rectangles.into_iter().enumerate() {
            if !pages.contains(&(rect.page_index as usize)) {
                continue;
            }
            projected.push(WriterCommentRect {
                comment_id: SharedString::from(thread.id.as_str()),
                label: label.clone(),
                page_index: rect.page_index,
                x: rect.x,
                y: rect.y,
                width: rect.width,
                height: rect.height,
                marker: position == 0,
            });
        }
    }
    projected
}

/// Project the document for a view `view_height` pixels tall, or for every
/// page when `view_height` is `None`.
pub(crate) fn project(
    doc: &WriterDocument,
    viewport: PageViewport,
    view_height: Option<f32>,
) -> Projection {
    let style = doc.page.page_style();
    let Ok(flow) = doc.flow(&style, layout_viewport(&style, viewport)) else {
        let mut selection_rects = Vec::new();
        if doc.blocks.is_empty() {
            selection_rects.push(WriterSelectionRect {
                page_index: 0,
                x: 0.0,
                y: 0.0,
                width: 1.0,
                height: style.body_font_size_pt * style.line_height,
                caret: true,
            });
        }
        return Projection {
            rows: fallback_rows(doc, &style),
            selection_rects,
            comment_rects: Vec::new(),
            page_count: 1,
            stack_height: style.height_pt,
            pages: 0..1,
        };
    };
    let zoom = flow.zoom().max(f32::EPSILON);
    let bounds = flow.page_bounds();
    let base_page = bounds[0];
    let stack_height = match (bounds.first(), bounds.last()) {
        (Some(first), Some(last)) => ((last.y + last.height - first.y) / zoom).max(style.height_pt),
        _ => style.height_pt,
    };
    let pages = match view_height {
        Some(height) => window_pages(&flow, &style, height),
        None => 0..flow.page_count(),
    };

    let content_width = (style.width_pt - style.margin_left_pt - style.margin_right_pt).max(1.0);
    let numbers = list_numbers(doc);
    let mut families: HashMap<String, String> = HashMap::new();
    let mut rows: Vec<WriterRenderBlock> = Vec::new();
    for line in flow.lines_on(pages.clone()) {
        let block = &doc.blocks[line.block_index];
        let font_size = style.font_size_for_kind(block.kind.as_str());
        let marker = list_marker(block.kind.as_str(), numbers[line.block_index]);
        let x = ((line.bounds.x - base_page.x - style.margin_left_pt * zoom) / zoom).max(0.0);
        let y = ((line.bounds.y - base_page.y - style.margin_top_pt * zoom) / zoom).max(0.0);
        let height = (line.bounds.height / zoom).max(font_size * style.line_height);
        let segments = line_segments(block, line.start, line.end);
        if let [only] = segments.as_slice() {
            let family = drawn_family(&only.family, &mut families);
            rows.push(render_row(
                &fragment_block(block, line.start, line.end),
                x,
                y,
                content_width,
                height,
                &marker,
                &family,
            ));
            continue;
        }
        // Several families on one line: the page draws each stretch as its
        // own row, placed where the selection highlight for that stretch is,
        // so glyphs, caret and highlight come from one measurement.
        let text = block.text.as_str();
        let mut drawn_any = false;
        for segment in &segments {
            // The page markup strips white space at the start of a row, so a
            // stretch is drawn from its first visible character and placed by
            // the measurement of that character.
            let stretch = &text[segment.start..segment.end];
            let skipped = stretch.len() - stretch.trim_start().len();
            let start = segment.start + skipped;
            if start >= segment.end {
                continue;
            }
            let from = line.global_start + (start - line.start);
            let to = line.global_start + (segment.end - line.start);
            let rects = flow.selection_rects(
                doc,
                &TextSelection::range(from, to),
                line.page..line.page + 1,
            );
            let Some(found) = rects.first() else { continue };
            let segment_x =
                ((found.rect.x - base_page.x - style.margin_left_pt * zoom) / zoom).max(0.0);
            let segment_width = found.rect.width / zoom;
            // Left aligned at its measured position; the width runs on to the
            // content edge so a rounding difference never wraps the stretch.
            let width = (content_width - segment_x).max(segment_width) + 4.0;
            let family = drawn_family(&segment.family, &mut families);
            let mut row = render_row(
                &fragment_block(block, start, segment.end),
                segment_x,
                y,
                width,
                height,
                if drawn_any { "" } else { &marker },
                &family,
            );
            row.alignment = writer_render_alignment(loom_text::Alignment::Left);
            if drawn_any {
                row.unsupported = false;
                row.unsupported_label = SharedString::default();
            }
            drawn_any = true;
            rows.push(row);
        }
    }

    let ranges = flow.selection_rects(doc, &doc.selection(), pages.clone());
    let mut selection_rects = ui_selection_rects(&style, zoom, base_page, &ranges);
    if selection_rects.is_empty() && doc.blocks.is_empty() {
        selection_rects.push(WriterSelectionRect {
            page_index: 0,
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: style.body_font_size_pt * style.line_height,
            caret: true,
        });
    }
    let comment_rects = comment_rects(doc, &style, &flow, base_page, &pages);
    Projection {
        rows,
        selection_rects,
        comment_rects,
        page_count: flow.page_count().max(1).min(i32::MAX as usize) as i32,
        stack_height,
        pages,
    }
}
