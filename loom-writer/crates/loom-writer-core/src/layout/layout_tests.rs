//! Layout checks, including a comparison with the original whole-document
//! implementation (kept here verbatim as `reference_*`) so the cached and
//! windowed paths are proven to place every line and caret identically.

use crate::{
    CaretAffinity, DocumentPage, LayoutFragment, PageFragment, PageLayout, PageRect, PageStyle,
    PageViewport, SelectionRect, TextSelection, VisibleRange, WriterDocument, PAGE_GAP_PT,
};

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
        for (block_index, block) in self.blocks.iter().enumerate() {
            let text = block.text.as_str();
            // Pagination must use the same block metrics as `layout`: a
            // heading has wider glyphs and a taller line box than body text.
            // Keeping this calculation here (rather than applying body
            // columns to every block) prevents long headings from producing
            // fragments that the renderer cannot fit on the page.
            let font_size = style.font_size_for_kind(block.kind.as_str());
            let line_height = font_size * style.line_height;
            let ranges =
                crate::text_metrics::wrap_by_width(text, &block.runs, font_size, usable_width);
            for (start, end) in ranges {
                if !pages.last().is_some_and(|page| page.fragments.is_empty())
                    && height_used + line_height > usable_height + f32::EPSILON
                {
                    pages.push(DocumentPage {
                        index: pages.len(),
                        fragments: Vec::new(),
                    });
                    height_used = 0.0;
                }
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
            // Paragraph spacing consumes one body line unless the next block
            // starts on a fresh page. This mirrors `layout`, which adds the
            // same gap only between fragments that share a page.
            if block_index + 1 < self.blocks.len()
                && height_used > 0.0
                && height_used + style.body_font_size_pt * style.line_height
                    <= usable_height + f32::EPSILON
            {
                height_used += style.body_font_size_pt * style.line_height;
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
                let line_height = font_size * style.line_height * zoom;
                let runs = block.map_or(&[][..], |block| block.runs.as_slice());
                let line_width =
                    crate::text_metrics::text_advance(&source.text, source.start, runs, font_size)
                        * zoom;
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
                let next_block_id = page
                    .fragments
                    .get(fragment_index + 1)
                    .map(|fragment| fragment.block_id);
                if next_block_id.is_some() && next_block_id != Some(source.block_id) {
                    // Match the paginator's conservative paragraph break while
                    // allowing larger heading metrics to occupy their actual
                    // visual line box.
                    line_y += body_line_height;
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
                let advance = |text: &str, start: usize| {
                    crate::text_metrics::text_advance(text, start, &block.runs, font_size) * zoom
                };
                let available_width =
                    (style.width_pt - style.margin_left_pt - style.margin_right_pt).max(1.0) * zoom;
                let alignment_offset = match block.style.alignment {
                    loom_text::Alignment::Center => {
                        ((available_width - fragment.bounds.width) / 2.0).max(0.0)
                    }
                    loom_text::Alignment::Right => {
                        (available_width - fragment.bounds.width).max(0.0)
                    }
                    loom_text::Alignment::Left | loom_text::Alignment::Justify => 0.0,
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
