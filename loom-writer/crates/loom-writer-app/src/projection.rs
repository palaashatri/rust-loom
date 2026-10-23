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
    WriterSelectionRect, WriterTableCellRect,
};

/// Everything the page canvas shows for one document state.
pub(crate) struct Projection {
    pub rows: Vec<WriterRenderBlock>,
    pub selection_rects: Vec<WriterSelectionRect>,
    pub comment_rects: Vec<WriterCommentRect>,
    pub table_cell_rects: Vec<WriterTableCellRect>,
    /// Pictures on the pages shown, in page-stack coordinates.
    pub images: Vec<PageImagePart>,
    pub page_count: i32,
    pub stack_height: f32,
    /// Pages the rows, selection and comment rectangles cover.
    pub pages: Range<usize>,
}

/// One picture to draw: where it sits and what it shows.
#[derive(Clone)]
pub(crate) struct PageImagePart {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub label: String,
    pub image: loom_writer_core::InlineImage,
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
        content: slint::StyledText::from_markdown(
            &crate::table_actions::break_keys::without_breaks(writer_render_markup(block)),
        )
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
        has_color: false,
        color: slint::Color::from_argb_encoded(0),
        table_cell: false,
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
            has_color: false,
            color: slint::Color::from_argb_encoded(0),
        })
        .collect()
}

/// Rows for every block when the page layout is unavailable: each block gets
/// a conservative height from the content width, stacked in order.
fn fallback_rows(doc: &WriterDocument, style: &PageStyle) -> Vec<WriterRenderBlock> {
    let content_width = (style.width_pt - style.margin_left_pt - style.margin_right_pt).max(1.0);
    let numbers = doc.list_item_numbers();
    let mut y = 0.0_f32;
    doc.blocks
        .iter()
        .enumerate()
        .map(|(index, block)| {
            let font_size = style.font_size_for_kind(block.kind.as_str());
            let height = writer_render_height_for_width(block, font_size, content_width)
                .max(font_size * style.line_height);
            let marker = block.list_marker(numbers[index]);
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

fn mark_selected_cells(
    rects: &mut [WriterTableCellRect],
    block_id: u64,
    columns: usize,
    range: Option<crate::table_actions::TableCellRange>,
) {
    for (index, rect) in rects.iter_mut().enumerate() {
        rect.selected = range.is_some_and(|range| {
            range.contains_cell(block_id, index / columns.max(1), index % columns.max(1))
        });
    }
}

fn display_byte_at_source_offset(
    text: &str,
    source_offsets: &[usize],
    source_byte: usize,
) -> usize {
    text.char_indices()
        .map(|(display_byte, _)| display_byte)
        .chain(std::iter::once(text.len()))
        .filter_map(|display_byte| {
            source_offsets
                .get(display_byte)
                .map(|source| (display_byte, source.abs_diff(source_byte)))
        })
        .min_by_key(|(display_byte, distance)| (*distance, *display_byte))
        .map(|(display_byte, _)| display_byte)
        .unwrap_or(0)
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
                has_color: false,
                color: slint::Color::from_argb_encoded(0),
            });
        }
        return Projection {
            rows: fallback_rows(doc, &style),
            selection_rects,
            comment_rects: Vec::new(),
            table_cell_rects: Vec::new(),
            images: Vec::new(),
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
    let numbers = doc.list_item_numbers();
    let selection = doc.selection();
    let selected_table_range =
        crate::table_actions::get_table_cell_selection_range().filter(|range| {
            selection.is_collapsed()
                && doc
                    .table_cell_at_offset(selection.anchor)
                    .is_some_and(|location| location.block_id == range.block_id)
        });
    let mut families: HashMap<String, String> = HashMap::new();
    let mut rows: Vec<WriterRenderBlock> = Vec::new();
    let mut table_cell_rects: Vec<WriterTableCellRect> = Vec::new();
    let mut images: Vec<PageImagePart> = Vec::new();
    let mut seen_table_blocks: std::collections::HashSet<u64> = std::collections::HashSet::new();
    let mut seen_chart_blocks: std::collections::HashSet<u64> = std::collections::HashSet::new();
    let mut table_block_positions: HashMap<u64, (f32, usize, usize, f32, f32)> = HashMap::new();
    for line in flow.lines_on(pages.clone()) {
        let block = &doc.blocks[line.block_index];
        let font_size = style.font_size_for_kind(block.kind.as_str());
        let marker = block.list_marker(numbers[line.block_index]);
        // The column the block's lines are aligned in: the text column less its indent.
        let column_width = block.text_column_width(&style);
        let x = ((line.bounds.x - base_page.x - style.margin_left_pt * zoom) / zoom).max(0.0);
        let y = ((line.bounds.y - base_page.y - style.margin_top_pt * zoom) / zoom).max(0.0);
        let height = (line.bounds.height / zoom).max(font_size * style.line_height);

        if let Some(picture) = block.image.as_ref().filter(|_| block.is_image()) {
            let width = line.bounds.width / zoom;
            let shift = match block.style.alignment {
                loom_text::Alignment::Center => (column_width - width) / 2.0,
                loom_text::Alignment::Right => column_width - width,
                _ => 0.0,
            };
            images.push(PageImagePart {
                x: x + shift.max(0.0),
                y,
                width,
                height: line.bounds.height / zoom,
                label: format!("Picture: {}", picture.name),
                image: picture.clone(),
            });
            continue;
        }

        #[derive(Clone, PartialEq, Eq)]
        struct TableLayoutKey {
            block_id: u64,
            style_id: String,
            header_row: bool,
            banded_rows: bool,
            first_column: bool,
            num_rows: usize,
            cols: usize,
            cell_offsets: Vec<i32>,
            col_width_bits: u32,
            row_height_bits: u32,
            table_y_bits: u32,
        }

        thread_local! {
            static TABLE_CELL_LAYOUT_CACHE: std::cell::RefCell<Option<(TableLayoutKey, Vec<WriterTableCellRect>)>> =
                const { std::cell::RefCell::new(None) };
        }

        if block.kind == loom_writer_core::TABLE_BLOCK_KIND {
            if seen_table_blocks.insert(block.id) {
                if let Some(table) = doc.table_from_block(block.id) {
                    let catalog_style = loom_objects::TableStyleCatalog::get(&table.style_id)
                        .unwrap_or_else(loom_objects::TableStyleCatalog::default_style);
                    let options = loom_objects::TableStyleOptions {
                        header_row: table.header_row,
                        banded_rows: table.banded_rows,
                        first_column: table.first_column,
                        ..Default::default()
                    };
                    let cols = table.columns().max(1);
                    let num_rows = table.rows.len();
                    let col_width = column_width / cols as f32;
                    let row_height = table_row_height(&style);
                    let table_x = 0.0_f32;
                    let table_y =
                        block_top_y(&flow, line.block_index, &style, base_page).unwrap_or(y);
                    let block_start = doc.blocks[..line.block_index]
                        .iter()
                        .map(|previous| previous.text.as_str().len() + 1)
                        .sum::<usize>();
                    let mut cell_offsets = vec![-1; num_rows * cols];
                    for (row, ranges) in loom_writer_core::compute_cell_ranges(block.text.as_str())
                        .into_iter()
                        .enumerate()
                        .take(num_rows)
                    {
                        for (col, (start, _)) in ranges.into_iter().enumerate().take(cols) {
                            let global_offset = block_start.saturating_add(start);
                            cell_offsets[row * cols + col] =
                                i32::try_from(global_offset).unwrap_or(-1);
                        }
                    }
                    table_block_positions
                        .insert(block.id, (table_y, cols, num_rows, col_width, row_height));

                    let cache_key = TableLayoutKey {
                        block_id: block.id,
                        style_id: table.style_id.clone(),
                        header_row: table.header_row,
                        banded_rows: table.banded_rows,
                        first_column: table.first_column,
                        num_rows,
                        cols,
                        cell_offsets: cell_offsets.clone(),
                        col_width_bits: col_width.to_bits(),
                        row_height_bits: row_height.to_bits(),
                        table_y_bits: table_y.to_bits(),
                    };

                    let hit = TABLE_CELL_LAYOUT_CACHE.with(|cache| {
                        if let Some((k, rects)) = cache.borrow().as_ref() {
                            if *k == cache_key {
                                return Some(rects.clone());
                            }
                        }
                        None
                    });

                    let rect_start = table_cell_rects.len();
                    if let Some(cached_rects) = hit {
                        table_cell_rects.extend(cached_rects);
                    } else {
                        let mut new_rects = Vec::with_capacity(num_rows * cols);
                        for r in 0..num_rows {
                            let cur_y = table_y + r as f32 * row_height;
                            for c in 0..cols {
                                let cur_x = table_x + c as f32 * col_width;
                                let resolved = catalog_style
                                    .resolve_cell(r, c, num_rows, cols, &options, None);
                                let has_bg = resolved.background_color.is_some();
                                let bg_color = if let Some(hex) = &resolved.background_color {
                                    slint::Color::from_argb_encoded(parse_hex_color(hex))
                                } else {
                                    slint::Color::from_argb_encoded(0)
                                };
                                let border_color = slint::Color::from_argb_encoded(0xFFD0D0D0);
                                let selection_border_color = high_contrast_selection_border_color();
                                new_rects.push(WriterTableCellRect {
                                    x: cur_x,
                                    y: cur_y,
                                    width: col_width,
                                    height: row_height,
                                    bg_color,
                                    has_bg,
                                    border_color,
                                    selection_border_color,
                                    border_width: 1.0,
                                    selected: false,
                                    table_cell: true,
                                    caret_offset: cell_offsets[r * cols + c],
                                });
                            }
                        }
                        table_cell_rects.extend(new_rects.iter().cloned());
                        TABLE_CELL_LAYOUT_CACHE.with(|cache| {
                            *cache.borrow_mut() = Some((cache_key, new_rects));
                        });
                    }
                    mark_selected_cells(
                        &mut table_cell_rects[rect_start..],
                        block.id,
                        cols,
                        selected_table_range,
                    );

                    for (r, row) in table.rows.iter().enumerate() {
                        let cur_y = table_y + r as f32 * row_height;
                        for (c, cell_text) in row.iter().enumerate() {
                            let text_trimmed = cell_text.trim();
                            if !text_trimmed.is_empty() {
                                let cur_x = table_x + c as f32 * col_width;
                                let resolved = catalog_style
                                    .resolve_cell(r, c, num_rows, cols, &options, None);
                                let mut cell_block =
                                    RichBlock::new(block.id, "paragraph", text_trimmed);
                                if resolved.bold {
                                    cell_block.runs.push(loom_text::StyleRun {
                                        start: 0,
                                        end: text_trimmed.len(),
                                        style: loom_text::CharacterStyle {
                                            weight: loom_text::FontWeight::Bold,
                                            ..Default::default()
                                        },
                                    });
                                }
                                let mut row = render_row(
                                    &cell_block,
                                    cur_x + 6.0,
                                    cur_y + 4.0,
                                    col_width - 12.0,
                                    row_height - 8.0,
                                    "",
                                    "",
                                );
                                row.table_cell = true;
                                // The style's own text colour (white on a dark
                                // header fill), not the page ink.
                                if let Some(hex) = &resolved.text_color {
                                    row.has_color = true;
                                    row.color =
                                        slint::Color::from_argb_encoded(parse_hex_color(hex));
                                }
                                rows.push(row);
                            }
                        }
                    }
                }
            }
            continue;
        }

        if block.kind == loom_writer_core::CHART_BLOCK_KIND {
            if seen_chart_blocks.insert(block.id) {
                if let Some(chart) = loom_writer_core::chart_from_block(block) {
                    let chart_title = chart.title.as_deref().unwrap_or("Chart");
                    let chart_w = column_width.min(460.0);
                    let chart_h = 200.0f32;
                    table_cell_rects.push(WriterTableCellRect {
                        x: 0.0,
                        y,
                        width: chart_w,
                        height: chart_h,
                        bg_color: slint::Color::from_argb_encoded(0xFFF8F9FA),
                        has_bg: true,
                        border_color: slint::Color::from_argb_encoded(0xFFD9D9D9),
                        selection_border_color: slint::Color::from_argb_encoded(0xFF000000),
                        border_width: 1.0,
                        selected: false,
                        table_cell: false,
                        caret_offset: -1,
                    });
                    let mut title_block = RichBlock::new(block.id, "heading2", chart_title);
                    title_block.runs.push(loom_text::StyleRun {
                        start: 0,
                        end: chart_title.len(),
                        style: loom_text::CharacterStyle {
                            weight: loom_text::FontWeight::Bold,
                            ..Default::default()
                        },
                    });
                    rows.push(render_row(
                        &title_block,
                        12.0,
                        y + 8.0,
                        chart_w - 24.0,
                        24.0,
                        "",
                        "",
                    ));
                    let summary_text = format!(
                        "[{:?} chart: {} categories, {} series]",
                        chart.chart_type,
                        chart.categories.len(),
                        chart.series.len()
                    );
                    let summary_block = RichBlock::new(block.id, "paragraph", &summary_text);
                    rows.push(render_row(
                        &summary_block,
                        12.0,
                        y + 36.0,
                        chart_w - 24.0,
                        20.0,
                        "",
                        "",
                    ));
                }
            }
            continue;
        }

        let segments = line_segments(block, line.start, line.end);
        if let [only] = segments.as_slice() {
            let family = drawn_family(&only.family, &mut families);
            rows.push(render_row(
                &fragment_block(block, line.start, line.end),
                x,
                y,
                column_width,
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
    if doc.selection().is_collapsed() {
        if let Some(loc) = doc.table_cell_at_offset(doc.selection().focus) {
            if let Some(&(table_y, cols, num_rows, col_width, row_height)) =
                table_block_positions.get(&loc.block_id)
            {
                if loc.row < num_rows && loc.col < cols {
                    if let Some((display_text, source_offsets)) =
                        doc.table_cell_display_map(loc.block_id, loc.row, loc.col)
                    {
                        // The caret follows the same trimmed, unescaped text the
                        // table renderer paints inside the cell.
                        let block_start = doc
                            .blocks
                            .iter()
                            .take(loc.block_index)
                            .map(|block| block.text.as_str().len() + 1)
                            .sum::<usize>();
                        let source_byte = doc.selection().focus.saturating_sub(block_start);
                        let display_byte = display_byte_at_source_offset(
                            &display_text,
                            &source_offsets,
                            source_byte,
                        );
                        let before = display_text.get(..display_byte).unwrap_or("");
                        let resolved = doc.table_from_block(loc.block_id).map(|t| {
                            let style_def = loom_objects::TableStyleCatalog::get(&t.style_id)
                                .unwrap_or_else(loom_objects::TableStyleCatalog::default_style);
                            let options = loom_objects::TableStyleOptions {
                                header_row: t.header_row,
                                banded_rows: t.banded_rows,
                                first_column: t.first_column,
                                ..Default::default()
                            };
                            style_def.resolve_cell(loc.row, loc.col, num_rows, cols, &options, None)
                        });
                        let bold_runs = if resolved.as_ref().is_some_and(|r| r.bold) {
                            vec![loom_text::StyleRun {
                                start: 0,
                                end: before.len(),
                                style: loom_text::CharacterStyle {
                                    weight: loom_text::FontWeight::Bold,
                                    ..Default::default()
                                },
                            }]
                        } else {
                            Vec::new()
                        };
                        let advance = loom_writer_core::text_advance(
                            before,
                            0,
                            &bold_runs,
                            PageStyle::default().font_size_for_kind("paragraph"),
                        );
                        let caret_x = loc.col as f32 * col_width + 6.0 + advance;
                        let caret_y = table_y + loc.row as f32 * row_height + 4.0;
                        let caret_color = resolved.and_then(|r| r.text_color);
                        selection_rects = vec![WriterSelectionRect {
                            page_index: 0,
                            x: caret_x,
                            y: caret_y,
                            width: 1.0,
                            height: row_height - 8.0,
                            caret: true,
                            has_color: caret_color.is_some(),
                            color: slint::Color::from_argb_encoded(
                                caret_color.as_deref().map_or(0, parse_hex_color),
                            ),
                        }];
                    }
                }
            }
        }
    }
    if selection_rects.is_empty() && doc.blocks.is_empty() {
        selection_rects.push(WriterSelectionRect {
            page_index: 0,
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: style.body_font_size_pt * style.line_height,
            caret: true,
            has_color: false,
            color: slint::Color::from_argb_encoded(0),
        });
    }
    let comment_rects = comment_rects(doc, &style, &flow, base_page, &pages);
    Projection {
        rows,
        selection_rects,
        comment_rects,
        table_cell_rects,
        images,
        page_count: flow.page_count().max(1).min(i32::MAX as usize) as i32,
        stack_height,
        pages,
    }
}

/// The height of one table row as the page paints it. The core layout reserves
/// a taller band per table row for its Markdown text; the page draws rows at
/// this pitch, so the caret and the reveal follow this height.
pub(crate) fn table_row_height(style: &PageStyle) -> f32 {
    (style.body_font_size_pt * style.line_height + 8.0).max(24.0)
}

/// The canvas y that a table block's rows are drawn from: the top of the
/// block's first line in the whole flow. It does not depend on which pages are
/// drawn, so the rows keep their place as the page scrolls past a page break.
fn block_top_y(
    flow: &DocumentFlow,
    block_index: usize,
    style: &PageStyle,
    base_page: PageRect,
) -> Option<f32> {
    let lines = flow.lines();
    let first = lines.partition_point(|line| line.block_index < block_index);
    let line = lines
        .get(first)
        .filter(|line| line.block_index == block_index)?;
    let zoom = flow.zoom().max(f32::EPSILON);
    Some(((line.bounds.y - base_page.y - style.margin_top_pt * zoom) / zoom).max(0.0))
}

/// The top and bottom, in viewport coordinates (the frame `DocumentFlow`
/// reports for lines), of the painted table row that holds `offset`. `None`
/// when `offset` is not in a table, so the caller measures the core line.
pub(crate) fn table_row_view_span(
    doc: &WriterDocument,
    style: &PageStyle,
    flow: &DocumentFlow,
    offset: usize,
) -> Option<(f32, f32)> {
    let location = doc.table_cell_at_offset(offset)?;
    let base = flow.page_bounds().first().copied()?;
    let top = block_top_y(flow, location.block_index, style, base)?;
    let zoom = flow.zoom().max(f32::EPSILON);
    let row = table_row_height(style);
    let view_top = (top + location.row as f32 * row) * zoom + style.margin_top_pt * zoom + base.y;
    Some((view_top, view_top + row * zoom))
}

fn parse_hex_color(hex: &str) -> u32 {
    let clean = hex.trim_start_matches('#');
    if clean.len() == 6 {
        if let Ok(rgb) = u32::from_str_radix(clean, 16) {
            return 0xFF000000 | rgb;
        }
    }
    0xFF000000
}

fn high_contrast_selection_border_color() -> slint::Color {
    // High Contrast renders table cells on the white paper surface, so a
    // black boundary provides 21:1 contrast even for source styles with
    // mid-luminance fills that cannot support a 7:1 monochrome boundary.
    slint::Color::from_argb_encoded(0xFF000000)
}
