//! Layout checks, including a comparison with the original whole-document
//! implementation (kept here verbatim as `reference_*`) so the cached and
//! windowed paths are proven to place every line and caret identically.

use crate::{
    CaretAffinity, DocumentPage, LayoutFragment, PageFragment, PageLayout, PageRect, PageStyle,
    PageViewport, SelectionRect, TextSelection, VisibleRange, WriterDocument, PAGE_GAP_PT,
};

pub(super) fn reference_measure(
    block: &crate::RichBlock,
    font_size: f32,
) -> crate::text_metrics::LineMeasure {
    crate::text_metrics::LineMeasure::new(
        block.text.as_str(),
        0,
        &block.runs,
        font_size,
        crate::text_metrics::Style::new(crate::is_heading_kind(block.kind.as_str()), false),
        crate::text_metrics::Uncovered::Estimate,
    )
}

fn reference_wrap_by_width(
    block: &crate::RichBlock,
    font_size: f32,
    width: f32,
) -> Vec<(usize, usize)> {
    reference_measure(block, font_size).wrap(block.text.as_str(), width)
}

impl WriterDocument {
    /// Deterministically paginates text using conservative font metrics.
    ///
    /// This is a reference CPU layout used for previews and tests. The future
    /// shaping engine can replace it while preserving this page-fragment API.
    pub(super) fn reference_paginate(
        &self,
        style: &PageStyle,
    ) -> Result<Vec<DocumentPage>, String> {
        if !style.width_pt.is_finite()
            || !style.height_pt.is_finite()
            || !style.body_font_size_pt.is_finite()
            || !style.line_height.is_finite()
            || style.width_pt <= style.margin_left_pt + style.margin_right_pt
            || style.height_pt <= style.margin_top_pt + style.margin_bottom_pt
            || style.body_font_size_pt <= 0.0
            || style.line_height <= 0.0
        {
            return Err("page style has invalid geometry".into());
        }
        let usable_width = style.width_pt - style.margin_left_pt - style.margin_right_pt;
        let usable_height = style.height_pt - style.margin_top_pt - style.margin_bottom_pt;
        let mut pages = vec![DocumentPage {
            index: 0,
            fragments: Vec::new(),
        }];
        let mut height_used = 0.0_f32;
        let body_line = style.body_font_size_pt * style.line_height;
        let new_page = |pages: &mut Vec<DocumentPage>| {
            pages.push(DocumentPage {
                index: pages.len(),
                fragments: Vec::new(),
            });
        };
        for (block_index, block) in self.blocks.iter().enumerate() {
            let text = block.text.as_str();
            // Pagination must use the same block metrics as `layout`: a
            // heading has wider glyphs and a taller line box than body text.
            // Keeping this calculation here (rather than applying body
            // columns to every block) prevents long headings from producing
            // fragments that the renderer cannot fit on the page.
            let font_size = style.font_size_for_kind(block.kind.as_str());
            let line_height = block
                .object_display_size(usable_width, usable_height)
                .map_or(font_size * style.line_height, |(_, height)| height);
            let ranges = reference_wrap_by_width(block, font_size, usable_width);
            let page_has_content = |pages: &Vec<DocumentPage>| {
                pages.last().is_some_and(|page| !page.fragments.is_empty())
            };
            // Keep with next: a heading does not end a page without the first
            // lines of the block after it.
            if block.kind.starts_with("heading") && page_has_content(&pages) {
                let mut after = height_used + ranges.len() as f32 * line_height;
                if after + body_line <= usable_height + f32::EPSILON {
                    after += body_line;
                }
                if let Some(next) = self.blocks.get(block_index + 1) {
                    let next_size = style.font_size_for_kind(next.kind.as_str());
                    let next_line = next
                        .object_display_size(usable_width, usable_height)
                        .map_or(next_size * style.line_height, |(_, height)| height);
                    let next_lines = reference_wrap_by_width(next, next_size, usable_width).len();
                    let wanted = if next_lines >= 3 { 2 } else { 1 };
                    let mut probe = after;
                    let mut fitting = 0;
                    while fitting < wanted && probe + next_line <= usable_height + f32::EPSILON {
                        probe += next_line;
                        fitting += 1;
                    }
                    if fitting < wanted {
                        new_page(&mut pages);
                        height_used = 0.0;
                    }
                }
            }
            // Widow and orphan control: a paragraph of three or more lines never
            // leaves one line alone at the foot of a page or the head of the next.
            let total = ranges.len();
            let mut done = 0;
            while done < total {
                let remaining = total - done;
                let mut take = 0;
                let mut used = height_used;
                while take < remaining && used + line_height <= usable_height + f32::EPSILON {
                    used += line_height;
                    take += 1;
                }
                if take < remaining && total >= 3 {
                    if remaining - take == 1 {
                        take = take.saturating_sub(1);
                        if take < 2 && done == 0 {
                            take = 0;
                        }
                    }
                    if done == 0 && take == 1 {
                        take = 0;
                    }
                }
                if take == 0 {
                    if page_has_content(&pages) {
                        new_page(&mut pages);
                        height_used = 0.0;
                        continue;
                    }
                    take = 1;
                }
                for &(start, end) in &ranges[done..done + take] {
                    pages
                        .last_mut()
                        .expect("at least one page")
                        .fragments
                        .push(PageFragment {
                            block_id: block.id,
                            start,
                            end,
                            text: text[start..end].to_string(),
                        });
                    height_used += line_height;
                }
                done += take;
                if done < total {
                    new_page(&mut pages);
                    height_used = 0.0;
                }
            }
            // Keep paragraph spacing only when the next line also fits.
            if let Some(next) = self
                .blocks
                .get(block_index + 1)
                .filter(|_| height_used > 0.0)
            {
                let next_size = style.font_size_for_kind(next.kind.as_str());
                let next_line_height = next
                    .object_display_size(usable_width, usable_height)
                    .map_or(next_size * style.line_height, |(_, height)| height);
                if height_used + body_line + next_line_height <= usable_height + f32::EPSILON {
                    height_used += body_line;
                }
            }
        }
        Ok(pages)
    }

    /// Build a page-and-viewport projection from the same fragments returned
    /// by [`Self::paginate`]. Geometry is deterministic and does not depend on
    /// a window or renderer, which keeps selection and scroll tests faithful
    /// to the editor's page model.
    pub(super) fn reference_layout(
        &self,
        style: &PageStyle,
        viewport: PageViewport,
    ) -> Result<PageLayout, String> {
        if !viewport.width.is_finite()
            || !viewport.height.is_finite()
            || !viewport.zoom.is_finite()
            || !viewport.scroll_x.is_finite()
            || !viewport.scroll_y.is_finite()
            || viewport.width <= 0.0
            || viewport.height <= 0.0
            || viewport.zoom <= 0.0
        {
            return Err("page viewport has invalid geometry".into());
        }

        let pages = self.reference_paginate(style)?;
        let usable_width = style.width_pt - style.margin_left_pt - style.margin_right_pt;
        let usable_height = style.height_pt - style.margin_top_pt - style.margin_bottom_pt;
        // Keep reference wrapping independent; share only glyph advances for its own ranges.
        let mut shaped_widths = std::collections::HashMap::new();
        for block in &self.blocks {
            if block.kind == crate::TABLE_BLOCK_KIND
                || block
                    .object_display_size(usable_width, usable_height)
                    .is_some()
            {
                continue;
            }
            let ranges: Vec<_> = pages
                .iter()
                .flat_map(|page| page.fragments.iter())
                .filter(|fragment| fragment.block_id == block.id)
                .map(|fragment| fragment.start..fragment.end)
                .collect();
            let font_size = style.font_size_for_kind(block.kind.as_str());
            let shaped_lines = super::shaped_line::shape_block_lines(
                block,
                &ranges,
                font_size,
                block.text_column_width(style),
            );
            for (range, shaped) in ranges.into_iter().zip(shaped_lines) {
                if let Some(shaped) = shaped {
                    shaped_widths.insert((block.id, range.start, range.end), shaped.width_pt());
                }
            }
        }
        let zoom = viewport.zoom;
        let scroll_x = viewport.scroll_x.max(0.0);
        let scroll_y = viewport.scroll_y.max(0.0);
        let page_width = style.width_pt * zoom;
        let page_height = style.height_pt * zoom;
        let page_gap = PAGE_GAP_PT * zoom;
        let body_line_height = style.body_font_size_pt * style.line_height * zoom;
        let viewport_rect = PageRect {
            x: 0.0,
            y: 0.0,
            width: viewport.width,
            height: viewport.height,
        };

        let mut page_bounds = Vec::with_capacity(pages.len());
        let mut fragments = Vec::new();
        for page in &pages {
            let page_x = ((viewport.width - page_width) / 2.0).max(0.0) - scroll_x;
            let page_y = page.index as f32 * (page_height + page_gap) - scroll_y;
            let bounds = PageRect {
                x: page_x,
                y: page_y,
                width: page_width,
                height: page_height,
            };
            page_bounds.push(bounds);

            let content_x = page_x + style.margin_left_pt * zoom;
            let content_y = page_y + style.margin_top_pt * zoom;
            let mut line_y = content_y;
            for (fragment_index, source) in page.fragments.iter().enumerate() {
                let block = self.blocks.iter().find(|block| block.id == source.block_id);
                let font_size = block
                    .map(|block| style.font_size_for_kind(block.kind.as_str()))
                    .unwrap_or(style.body_font_size_pt);
                let object_size =
                    block.and_then(|block| block.object_display_size(usable_width, usable_height));
                let line_height =
                    object_size.map_or(font_size * style.line_height, |(_, height)| height) * zoom;
                let runs = block.map_or(&[][..], |block| block.runs.as_slice());
                // The line's width is its place in the whole block's text.
                let line_width = match block {
                    Some(block) => object_size.map_or_else(
                        || {
                            shaped_widths
                                .get(&(block.id, source.start, source.end))
                                .copied()
                                .unwrap_or_else(|| {
                                    reference_measure(block, font_size)
                                        .advance(source.start, source.end)
                                })
                        },
                        |(width, _)| width,
                    ),
                    None => crate::text_metrics::text_advance(
                        &source.text,
                        source.start,
                        runs,
                        font_size,
                    ),
                } * zoom;
                let fragment_bounds = PageRect {
                    x: content_x,
                    y: line_y,
                    width: line_width,
                    height: line_height,
                };
                fragments.push(LayoutFragment {
                    block_id: source.block_id,
                    start: source.start,
                    end: source.end,
                    text: source.text.clone(),
                    bounds: fragment_bounds,
                });
                line_y += line_height;
                if let Some(next) = page
                    .fragments
                    .get(fragment_index + 1)
                    .filter(|fragment| fragment.block_id != source.block_id)
                {
                    let next_height = self
                        .blocks
                        .iter()
                        .find(|block| block.id == next.block_id)
                        .map(|block| {
                            block
                                .object_display_size(usable_width, usable_height)
                                .map_or_else(
                                    || {
                                        style.font_size_for_kind(block.kind.as_str())
                                            * style.line_height
                                            * zoom
                                    },
                                    |(_, height)| height * zoom,
                                )
                        })
                        .unwrap_or(body_line_height);
                    let body_bottom = page_y + (style.height_pt - style.margin_bottom_pt) * zoom;
                    if line_y + body_line_height + next_height <= body_bottom + f32::EPSILON {
                        line_y += body_line_height;
                    }
                }
            }
        }

        let visible_ranges = fragments
            .iter()
            .filter_map(|fragment| {
                if !fragment.bounds.intersects(viewport_rect) {
                    return None;
                }
                let page_index = page_bounds
                    .iter()
                    .position(|page| {
                        fragment.bounds.y >= page.y && fragment.bounds.y < page.y + page.height
                    })
                    .unwrap_or(0);
                Some(VisibleRange {
                    page_index,
                    block_id: fragment.block_id,
                    start: fragment.start,
                    end: fragment.end,
                })
            })
            .collect();

        let selection_rects =
            self.reference_selection_rectangles(&fragments, &pages, style, zoom, &self.selection);

        Ok(PageLayout {
            pages,
            page_bounds,
            fragments,
            visible_ranges,
            selection_rects,
            zoom,
            scroll_x,
            scroll_y,
            viewport_width: viewport.width,
            viewport_height: viewport.height,
        })
    }

    /// Project any text range onto an already-computed page layout.
    ///
    /// Reusing the layout keeps comment anchors aligned with the same wrapped
    /// fragments as the editor without paginating the document once per
    /// comment.
    pub(super) fn reference_rects_for(
        &self,
        layout: &PageLayout,
        style: &PageStyle,
        selection: &TextSelection,
    ) -> Vec<SelectionRect> {
        self.reference_selection_rectangles(
            &layout.fragments,
            &layout.pages,
            style,
            layout.zoom,
            selection,
        )
    }

    pub(super) fn reference_selection_rectangles(
        &self,
        fragments: &[LayoutFragment],
        pages: &[DocumentPage],
        style: &PageStyle,
        zoom: f32,
        selection: &TextSelection,
    ) -> Vec<SelectionRect> {
        let text = self.editor_text();
        let (selection_start, selection_end) = crate::normalized_grapheme_range(&text, selection);
        let mut block_starts = Vec::with_capacity(self.blocks.len());
        let mut cursor = 0usize;
        for (index, block) in self.blocks.iter().enumerate() {
            block_starts.push(cursor);
            cursor += block.text.as_str().len() + usize::from(index + 1 < self.blocks.len());
        }

        let mut result = Vec::new();
        let mut fragment_index = 0usize;
        let mut previous_fragment_end = None;
        for page in pages {
            for source in &page.fragments {
                let Some(fragment) = fragments.get(fragment_index) else {
                    continue;
                };
                let current_fragment_index = fragment_index;
                fragment_index += 1;
                let Some(block_index) = self
                    .blocks
                    .iter()
                    .position(|block| block.id == source.block_id)
                else {
                    continue;
                };
                let block = &self.blocks[block_index];
                let font_size = style.font_size_for_kind(block.kind.as_str());
                // A substring's width is its place in the whole block's text:
                // kerning acts across the substring's edges.
                let positions = reference_measure(block, font_size);
                let advance =
                    |text: &str, start: usize| positions.advance(start, start + text.len()) * zoom;
                let available_width =
                    (style.width_pt - style.margin_left_pt - style.margin_right_pt).max(1.0) * zoom;
                let alignment_offset = match block.style.alignment.resolve(block.text.as_str()) {
                    loom_text::Alignment::Center => {
                        ((available_width - fragment.bounds.width) / 2.0).max(0.0)
                    }
                    loom_text::Alignment::End | loom_text::Alignment::Right => {
                        (available_width - fragment.bounds.width).max(0.0)
                    }
                    loom_text::Alignment::Left
                    | loom_text::Alignment::Justify
                    | loom_text::Alignment::Start => 0.0,
                };
                let fragment_x = fragment.bounds.x + alignment_offset;
                let block_start = block_starts[block_index];
                let fragment_global_start = block_start + source.start;
                let fragment_global_end = block_start + source.end;
                let overlap_start = selection_start.max(fragment_global_start);
                let overlap_end = selection_end.min(fragment_global_end);
                if overlap_start < overlap_end {
                    let local_start = overlap_start - fragment_global_start;
                    let local_end = overlap_end - fragment_global_start;
                    let prefix = &source.text[..local_start.min(source.text.len())];
                    let selected = &source.text
                        [local_start.min(source.text.len())..local_end.min(source.text.len())];
                    let x = fragment_x + advance(prefix, source.start);
                    let width = advance(selected, source.start + local_start);
                    result.push(SelectionRect {
                        page_index: page.index,
                        block_id: source.block_id,
                        start: source.start + local_start,
                        end: source.start + local_end,
                        rect: PageRect {
                            x,
                            y: fragment.bounds.y,
                            width,
                            height: fragment.bounds.height,
                        },
                    });
                } else if selection_start == selection_end {
                    let local = selection_start.saturating_sub(fragment_global_start);
                    let at_start = selection_start == fragment_global_start;
                    let at_end = selection_start == fragment_global_end;
                    let strictly_inside = selection_start > fragment_global_start
                        && selection_start < fragment_global_end;
                    let has_previous_boundary =
                        previous_fragment_end == Some(fragment_global_start);
                    let is_last_fragment = current_fragment_index + 1 == fragments.len();
                    let use_fragment = strictly_inside
                        || (at_start
                            && (!has_previous_boundary
                                || selection.affinity == CaretAffinity::Downstream))
                        || (at_end
                            && (selection.affinity == CaretAffinity::Upstream || is_last_fragment));
                    if use_fragment {
                        let local = local.clamp(0, source.text.len());
                        let prefix = &source.text[..local];
                        result.push(SelectionRect {
                            page_index: page.index,
                            block_id: source.block_id,
                            start: source.start + local,
                            end: source.start + local,
                            rect: PageRect {
                                x: fragment_x + advance(prefix, source.start),
                                y: fragment.bounds.y,
                                width: 1.0,
                                height: fragment.bounds.height,
                            },
                        });
                    }
                }
                previous_fragment_end = Some(fragment_global_end);
            }
        }
        result
    }
}
