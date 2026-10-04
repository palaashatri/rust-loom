//! Projection of the page stack into the Slint models.
//!
//! The editor draws a long document one screen at a time, so only the pages
//! near the viewport are turned into rows, selection rectangles and comment
//! rectangles. The cost of a keystroke or a scroll step then follows what is
//! on screen, not how long the document is. Geometry still comes from the core
//! layout of the whole document, so positions are identical either way.

use std::collections::HashMap;
use std::ops::Range;

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

fn render_row(
    block: &RichBlock,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    marker: &str,
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
            let row = render_row(block, 0.0, y, content_width, height, &marker);
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
    let rows = flow
        .lines_on(pages.clone())
        .iter()
        .map(|line| {
            let block = &doc.blocks[line.block_index];
            let font_size = style.font_size_for_kind(block.kind.as_str());
            let marker = list_marker(block.kind.as_str(), numbers[line.block_index]);
            render_row(
                &fragment_block(block, line.start, line.end),
                ((line.bounds.x - base_page.x - style.margin_left_pt * zoom) / zoom).max(0.0),
                ((line.bounds.y - base_page.y - style.margin_top_pt * zoom) / zoom).max(0.0),
                content_width,
                (line.bounds.height / zoom).max(font_size * style.line_height),
                &marker,
            )
        })
        .collect();

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
