//! Deterministic pagination and page geometry.
//!
//! Wrapping a block into lines is the expensive part of laying out a
//! document, and a keystroke changes one block. Wrapped lines are therefore
//! cached per block and revalidated by comparing the block's text, runs, font
//! size and wrap width, so an edit re-wraps only the paragraph it touched while
//! pagination and geometry stay cheap arithmetic over the cached lines.
//!
//! [`DocumentFlow`] is the whole-document pagination in viewport coordinates.
//! It can answer for just the pages near the viewport, which is what the editor
//! projects, instead of materialising every page of a long document.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;

use loom_text::StyleRun;

use crate::text_metrics::{block_measure, LineMeasure};
use crate::{
    floor_grapheme_boundary, DocumentPage, LayoutFragment, PageFragment, PageLayout, PageRect,
    PageStyle, PageViewport, RichBlock, SelectionRect, TextSelection, VisibleRange, WriterDocument,
    PAGE_GAP_PT,
};

#[derive(Debug, Clone, Copy, PartialEq)]
struct WrappedLine {
    start: usize,
    end: usize,
    advance_pt: f32,
}

struct CachedBlock {
    graphemes: usize,
    font_bits: u32,
    width_bits: u32,
    kind: String,
    text: String,
    runs: Vec<StyleRun>,
    /// Where every grapheme of the text sits, independent of the wrap width.
    measure: Rc<LineMeasure>,
    lines: Rc<[WrappedLine]>,
    words: usize,
    chars: usize,
    seen: u64,
}

/// What the cache knows about one block.
#[derive(Clone)]
struct BlockInfo {
    lines: Rc<[WrappedLine]>,
    words: usize,
    chars: usize,
    graphemes: usize,
}

#[derive(Default)]
struct WrapCache {
    blocks: HashMap<u64, CachedBlock>,
    generation: u64,
}

thread_local! {
    static WRAP_CACHE: RefCell<WrapCache> = RefCell::new(WrapCache::default());
}

/// Wrapped lines and text counts for one block, from the cache when the block
/// has not changed since it was last wrapped at this size and width.
fn wrapped(block: &RichBlock, font_size: f32, width: f32) -> BlockInfo {
    WRAP_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let generation = cache.generation;
        let text = block.text.as_str();
        let (font_bits, width_bits) = (font_size.to_bits(), width.to_bits());
        // The grapheme positions depend on the text, runs and size only, so a
        // block that is re-wrapped at a new width keeps them.
        let mut reusable: Option<Rc<LineMeasure>> = None;
        if let Some(hit) = cache.blocks.get_mut(&block.id) {
            if hit.font_bits == font_bits && hit.text == text && hit.runs == block.runs {
                if hit.width_bits == width_bits && hit.kind == block.kind {
                    hit.seen = generation;
                    return BlockInfo {
                        lines: hit.lines.clone(),
                        words: hit.words,
                        chars: hit.chars,
                        graphemes: hit.graphemes,
                    };
                }
                reusable = Some(hit.measure.clone());
            }
        }
        #[cfg(test)]
        WRAP_MISSES.with(|misses| misses.set(misses.get() + 1));
        let measure =
            reusable.unwrap_or_else(|| Rc::new(block_measure(text, &block.runs, font_size)));
        let lines: Rc<[WrappedLine]> = measure
            .wrap(text, width)
            .into_iter()
            .map(|(start, end)| WrappedLine {
                start,
                end,
                advance_pt: measure.advance(start, end),
            })
            .collect();
        let (mut words, mut chars) = (0, 0);
        crate::tables::for_each_counted_text(&block.kind, text, |part| {
            words += part.split_whitespace().count();
            chars += part.chars().count();
        });
        let graphemes = crate::grapheme_count(text);
        cache.blocks.insert(
            block.id,
            CachedBlock {
                font_bits,
                width_bits,
                kind: block.kind.clone(),
                text: text.to_string(),
                runs: block.runs.clone(),
                measure,
                lines: lines.clone(),
                words,
                chars,
                graphemes,
                seen: generation,
            },
        );
        BlockInfo {
            lines,
            words,
            chars,
            graphemes,
        }
    })
}

/// Start a pass over a document's blocks and, after a pass, drop entries for
/// blocks that no longer exist.
fn begin_pass() {
    WRAP_CACHE.with(|cache| cache.borrow_mut().generation += 1);
}

fn end_pass(block_count: usize) {
    WRAP_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.blocks.len() > block_count * 2 + 256 {
            let generation = cache.generation;
            cache.blocks.retain(|_, block| block.seen == generation);
        }
    });
}

/// Where every grapheme of `block` sits at `font_size`: from the cache when
/// the block is unchanged since it was wrapped, else measured now. Carets and
/// selection rectangles are read from this, so they land where the glyphs the
/// page draws are, kerning included.
fn block_positions(block: &RichBlock, font_size: f32) -> Rc<LineMeasure> {
    let text = block.text.as_str();
    let cached = WRAP_CACHE.with(|cache| {
        cache
            .borrow()
            .blocks
            .get(&block.id)
            .filter(|hit| {
                hit.font_bits == font_size.to_bits() && hit.text == text && hit.runs == block.runs
            })
            .map(|hit| hit.measure.clone())
    });
    cached.unwrap_or_else(|| Rc::new(block_measure(text, &block.runs, font_size)))
}

/// One wrapped line placed on a page.
#[derive(Debug, Clone, PartialEq)]
pub struct FlowLine {
    /// Index of the source block in the document.
    pub block_index: usize,
    /// First UTF-8 byte of the line in its block.
    pub start: usize,
    /// Exclusive last UTF-8 byte of the line in its block.
    pub end: usize,
    /// Offset of the line's first byte in the editor text stream.
    pub global_start: usize,
    /// Zero-based page the line sits on.
    pub page: usize,
    /// Line bounds in viewport coordinates.
    pub bounds: PageRect,
}

impl FlowLine {
    fn global_end(&self) -> usize {
        self.global_start + (self.end - self.start)
    }
}

struct Pagination {
    lines: Vec<FlowLine>,
    page_lines: Vec<Range<usize>>,
    block_starts: Vec<usize>,
}

fn invalid_style(style: &PageStyle) -> bool {
    !style.width_pt.is_finite()
        || !style.height_pt.is_finite()
        || !style.body_font_size_pt.is_finite()
        || !style.line_height.is_finite()
        || style.width_pt <= style.margin_left_pt + style.margin_right_pt
        || style.height_pt <= style.margin_top_pt + style.margin_bottom_pt
        || style.body_font_size_pt <= 0.0
        || style.line_height <= 0.0
}

fn invalid_viewport(viewport: &PageViewport) -> bool {
    !viewport.width.is_finite()
        || !viewport.height.is_finite()
        || !viewport.zoom.is_finite()
        || !viewport.scroll_x.is_finite()
        || !viewport.scroll_y.is_finite()
        || viewport.width <= 0.0
        || viewport.height <= 0.0
        || viewport.zoom <= 0.0
}

impl WriterDocument {
    fn pagination(&self, style: &PageStyle) -> Result<Pagination, String> {
        if invalid_style(style) {
            return Err("page style has invalid geometry".into());
        }
        let usable_width = style.width_pt - style.margin_left_pt - style.margin_right_pt;
        let usable_height = style.height_pt - style.margin_top_pt - style.margin_bottom_pt;
        let body_line = style.body_font_size_pt * style.line_height;
        begin_pass();
        let mut lines: Vec<FlowLine> = Vec::new();
        let mut page_lines = Vec::new();
        let mut page_first_line = 0usize;
        let mut page_index = 0usize;
        let mut height_used = 0.0_f32;
        let mut block_starts = Vec::with_capacity(self.blocks.len());
        let mut block_start = 0usize;
        for (block_index, block) in self.blocks.iter().enumerate() {
            block_starts.push(block_start);
            // Pagination must use the same block metrics as the geometry: a
            // heading has wider glyphs and a taller line box than body text.
            let font_size = style.font_size_for_kind(block.kind.as_str());
            let line_height = font_size * style.line_height;
            let info = wrapped(block, font_size, usable_width);
            for line in info.lines.iter() {
                if lines.len() > page_first_line
                    && height_used + line_height > usable_height + f32::EPSILON
                {
                    page_lines.push(page_first_line..lines.len());
                    page_first_line = lines.len();
                    page_index += 1;
                    height_used = 0.0;
                }
                lines.push(FlowLine {
                    block_index,
                    start: line.start,
                    end: line.end,
                    global_start: block_start + line.start,
                    page: page_index,
                    bounds: PageRect {
                        x: 0.0,
                        y: 0.0,
                        width: line.advance_pt,
                        height: 0.0,
                    },
                });
                height_used += line_height;
            }
            // Paragraph spacing consumes one body line unless the next block
            // starts on a fresh page.
            if block_index + 1 < self.blocks.len()
                && height_used > 0.0
                && height_used + body_line <= usable_height + f32::EPSILON
            {
                height_used += body_line;
            }
            block_start += block.text.len_bytes() + 1;
        }
        page_lines.push(page_first_line..lines.len());
        end_pass(self.blocks.len());
        Ok(Pagination {
            lines,
            page_lines,
            block_starts,
        })
    }

    /// Deterministically paginates text using conservative font metrics.
    ///
    /// This is a reference CPU layout used for previews and tests. The future
    /// shaping engine can replace it while preserving this page-fragment API.
    pub fn paginate(&self, style: &PageStyle) -> Result<Vec<DocumentPage>, String> {
        let pagination = self.pagination(style)?;
        Ok(pagination
            .page_lines
            .iter()
            .enumerate()
            .map(|(index, range)| DocumentPage {
                index,
                fragments: pagination.lines[range.clone()]
                    .iter()
                    .map(|line| {
                        let block = &self.blocks[line.block_index];
                        PageFragment {
                            block_id: block.id,
                            start: line.start,
                            end: line.end,
                            text: block.text.as_str()[line.start..line.end].to_string(),
                        }
                    })
                    .collect(),
            })
            .collect())
    }

    /// Paginate and place every line in viewport coordinates without copying
    /// any text. Cheap enough to run per edit: see the module notes.
    pub fn flow(&self, style: &PageStyle, viewport: PageViewport) -> Result<DocumentFlow, String> {
        if invalid_viewport(&viewport) {
            return Err("page viewport has invalid geometry".into());
        }
        let Pagination {
            mut lines,
            page_lines,
            block_starts,
        } = self.pagination(style)?;
        let zoom = viewport.zoom;
        let scroll_x = viewport.scroll_x.max(0.0);
        let scroll_y = viewport.scroll_y.max(0.0);
        let page_width = style.width_pt * zoom;
        let page_height = style.height_pt * zoom;
        let page_gap = PAGE_GAP_PT * zoom;
        let body_line_height = style.body_font_size_pt * style.line_height * zoom;

        let mut page_bounds = Vec::with_capacity(page_lines.len());
        for (index, range) in page_lines.iter().enumerate() {
            let page_x = ((viewport.width - page_width) / 2.0).max(0.0) - scroll_x;
            let page_y = index as f32 * (page_height + page_gap) - scroll_y;
            page_bounds.push(PageRect {
                x: page_x,
                y: page_y,
                width: page_width,
                height: page_height,
            });
            let content_x = page_x + style.margin_left_pt * zoom;
            let mut line_y = page_y + style.margin_top_pt * zoom;
            for at in range.clone() {
                let block = &self.blocks[lines[at].block_index];
                let line_height =
                    style.font_size_for_kind(block.kind.as_str()) * style.line_height * zoom;
                let width = lines[at].bounds.width * zoom;
                lines[at].bounds = PageRect {
                    x: content_x,
                    y: line_y,
                    width,
                    height: line_height,
                };
                line_y += line_height;
                if at + 1 < range.end && lines[at + 1].block_index != lines[at].block_index {
                    // Match the paginator's conservative paragraph break while
                    // letting larger heading metrics occupy their real box.
                    line_y += body_line_height;
                }
            }
        }
        Ok(DocumentFlow {
            style: style.clone(),
            lines,
            page_lines,
            page_bounds,
            block_starts,
            zoom,
            scroll_x,
            scroll_y,
            viewport_width: viewport.width,
            viewport_height: viewport.height,
        })
    }

    /// Build a page-and-viewport projection from the same fragments returned
    /// by [`Self::paginate`]. Geometry is deterministic and does not depend on
    /// a window or renderer, which keeps selection and scroll tests faithful
    /// to the editor's page model.
    pub fn layout(&self, style: &PageStyle, viewport: PageViewport) -> Result<PageLayout, String> {
        let flow = self.flow(style, viewport)?;
        let viewport_rect = PageRect {
            x: 0.0,
            y: 0.0,
            width: viewport.width,
            height: viewport.height,
        };
        let mut pages = Vec::with_capacity(flow.page_lines.len());
        let mut fragments = Vec::with_capacity(flow.lines.len());
        let mut visible_ranges = Vec::new();
        for (index, range) in flow.page_lines.iter().enumerate() {
            let mut page_fragments = Vec::with_capacity(range.len());
            for line in &flow.lines[range.clone()] {
                let block = &self.blocks[line.block_index];
                let text = block.text.as_str()[line.start..line.end].to_string();
                page_fragments.push(PageFragment {
                    block_id: block.id,
                    start: line.start,
                    end: line.end,
                    text: text.clone(),
                });
                if line.bounds.intersects(viewport_rect) {
                    visible_ranges.push(VisibleRange {
                        page_index: line.page,
                        block_id: block.id,
                        start: line.start,
                        end: line.end,
                    });
                }
                fragments.push(LayoutFragment {
                    block_id: block.id,
                    start: line.start,
                    end: line.end,
                    text,
                    bounds: line.bounds,
                });
            }
            pages.push(DocumentPage {
                index,
                fragments: page_fragments,
            });
        }
        let selection_rects = flow.selection_rects(self, &self.selection, 0..flow.page_lines.len());
        Ok(PageLayout {
            pages,
            page_bounds: flow.page_bounds,
            fragments,
            visible_ranges,
            selection_rects,
            zoom: flow.zoom,
            scroll_x: flow.scroll_x,
            scroll_y: flow.scroll_y,
            viewport_width: flow.viewport_width,
            viewport_height: flow.viewport_height,
        })
    }

    /// Alias emphasizing that this projection extends the existing paginator.
    pub fn paginate_with_viewport(
        &self,
        style: &PageStyle,
        viewport: PageViewport,
    ) -> Result<PageLayout, String> {
        self.layout(style, viewport)
    }

    /// Project any text range onto an already-computed page layout.
    ///
    /// Reusing the layout keeps comment anchors aligned with the same wrapped
    /// fragments as the editor without paginating the document once per
    /// comment.
    pub fn selection_rectangles_for(
        &self,
        layout: &PageLayout,
        style: &PageStyle,
        selection: &TextSelection,
    ) -> Vec<SelectionRect> {
        let block_starts = self.block_starts();
        let index_of: HashMap<u64, usize> = self
            .blocks
            .iter()
            .enumerate()
            .rev()
            .map(|(index, block)| (block.id, index))
            .collect();
        let mut lines = Vec::with_capacity(layout.fragments.len());
        let mut fragments = layout.fragments.iter();
        for page in &layout.pages {
            for source in &page.fragments {
                let Some(fragment) = fragments.next() else {
                    break;
                };
                let Some(&block_index) = index_of.get(&source.block_id) else {
                    continue;
                };
                lines.push(FlowLine {
                    block_index,
                    start: source.start,
                    end: source.end,
                    global_start: block_starts[block_index] + source.start,
                    page: page.index,
                    bounds: fragment.bounds,
                });
            }
        }
        rects_for_lines(
            self,
            style,
            layout.zoom,
            &block_starts,
            &lines,
            selection,
            0..lines.len(),
        )
    }

    /// Floor an editor-stream offset to a grapheme boundary and clamp it to
    /// the editor text, without building that text.
    pub(crate) fn floor_offset(&self, offset: usize) -> usize {
        floor_in_blocks(self, &self.block_starts(), offset)
    }

    /// Floor both ends of `selection` to grapheme boundaries and clamp them to
    /// the editor text, without building that text.
    pub fn clamp_selection(&self, mut selection: TextSelection) -> TextSelection {
        let starts = self.block_starts();
        selection.anchor = floor_in_blocks(self, &starts, selection.anchor);
        selection.focus = floor_in_blocks(self, &starts, selection.focus);
        selection
    }

    /// Offset of each block's first byte in the editor text stream.
    fn block_starts(&self) -> Vec<usize> {
        let mut start = 0usize;
        self.blocks
            .iter()
            .map(|block| {
                let at = start;
                start += block.text.len_bytes() + 1;
                at
            })
            .collect()
    }

    /// Number of user-perceived characters before `offset` in the editor text,
    /// each newline between blocks counting as one. Whole blocks before the
    /// offset come from the per-block cache.
    pub fn grapheme_position(&self, offset: usize) -> usize {
        if self.blocks.is_empty() {
            return 0;
        }
        let starts = self.block_starts();
        let offset = floor_in_blocks(self, &starts, offset);
        let index = starts
            .partition_point(|start| *start <= offset)
            .saturating_sub(1);
        let style = self.page.page_style();
        let width = (style.width_pt - style.margin_left_pt - style.margin_right_pt).max(1.0);
        begin_pass();
        let before: usize = self.blocks[..index]
            .iter()
            .map(|block| {
                let font_size = style.font_size_for_kind(block.kind.as_str());
                wrapped(block, font_size, width).graphemes + 1
            })
            .sum();
        end_pass(self.blocks.len());
        let text = self.blocks[index].text.as_str();
        before + crate::grapheme_count(&text[..offset - starts[index]])
    }

    /// Words and characters in the editor text, from the per-block cache so a
    /// keystroke recounts only the paragraph it changed.
    pub fn text_counts(&self) -> (usize, usize) {
        let style = self.page.page_style();
        let width = (style.width_pt - style.margin_left_pt - style.margin_right_pt).max(1.0);
        begin_pass();
        let (mut words, mut chars) = (0usize, 0usize);
        for block in &self.blocks {
            let font_size = style.font_size_for_kind(block.kind.as_str());
            let info = wrapped(block, font_size, width);
            words += info.words;
            chars += info.chars;
        }
        end_pass(self.blocks.len());
        (words, chars + self.blocks.len().saturating_sub(1))
    }
}

/// Floor an editor-stream offset to a grapheme boundary using only the block
/// it falls in. Graphemes never span the newline between blocks, so this
/// equals flooring in the joined text without building it.
fn floor_in_blocks(doc: &WriterDocument, block_starts: &[usize], offset: usize) -> usize {
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

/// Selection highlights or caret for `selection` on the lines in `window`
/// (indexes into `lines`). Lines are in document order, so the lines a
/// selection touches are found by binary search rather than a scan.
fn rects_for_lines(
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
    let available_width =
        (style.width_pt - style.margin_left_pt - style.margin_right_pt).max(1.0) * zoom;
    let mut result = Vec::new();
    // The last block measured: a selection covers many lines of few blocks.
    let mut measured: Option<(usize, Rc<LineMeasure>)> = None;
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
        // Block byte offsets in, points out.
        let advance = |from: usize, to: usize| positions.advance(from, to) * zoom;
        let alignment_offset = match block.style.alignment {
            loom_text::Alignment::Center => ((available_width - line.bounds.width) / 2.0).max(0.0),
            loom_text::Alignment::Right => (available_width - line.bounds.width).max(0.0),
            loom_text::Alignment::Left | loom_text::Alignment::Justify => 0.0,
        };
        let line_x = line.bounds.x + alignment_offset;
        let (global_start, global_end) = (line.global_start, line.global_end());
        let overlap_start = selection_start.max(global_start);
        let overlap_end = selection_end.min(global_end);
        if overlap_start < overlap_end {
            let local_start = overlap_start - global_start;
            let local_end = overlap_end - global_start;
            let from = line.start + local_start.min(source.len());
            let to = line.start + local_end.min(source.len());
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
                result.push(SelectionRect {
                    page_index: line.page,
                    block_id: block.id,
                    start: line.start + local,
                    end: line.start + local,
                    rect: PageRect {
                        x: line_x + advance(line.start, line.start + local),
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

/// Whole-document pagination placed in viewport coordinates.
///
/// Built from the wrapped-line cache, it holds one entry per line and no text.
/// Callers that draw only part of a long document ask it for the pages near
/// the viewport ([`Self::visible_pages`]) and for rectangles on those pages.
#[derive(Debug, Clone, PartialEq)]
pub struct DocumentFlow {
    style: PageStyle,
    lines: Vec<FlowLine>,
    page_lines: Vec<Range<usize>>,
    page_bounds: Vec<PageRect>,
    block_starts: Vec<usize>,
    zoom: f32,
    scroll_x: f32,
    scroll_y: f32,
    viewport_width: f32,
    viewport_height: f32,
}

impl DocumentFlow {
    /// Number of pages (at least one).
    pub fn page_count(&self) -> usize {
        self.page_lines.len()
    }

    /// Page rectangles in viewport coordinates.
    pub fn page_bounds(&self) -> &[PageRect] {
        &self.page_bounds
    }

    /// The span of the editor text stream covered by the lines of `pages`, or
    /// `None` when those pages hold no lines.
    pub fn editor_span(&self, pages: Range<usize>) -> Option<(usize, usize)> {
        let end = pages.end.min(self.page_lines.len());
        let start = pages.start.min(end);
        if start >= end {
            return None;
        }
        let first = self.page_lines.get(start)?.start;
        let last = self.page_lines.get(end - 1)?.end;
        let (first, last) = (
            self.lines.get(first)?,
            self.lines.get(last.checked_sub(1)?)?,
        );
        Some((first.global_start, last.global_end()))
    }

    /// Effective page scale.
    pub fn zoom(&self) -> f32 {
        self.zoom
    }

    /// Every line, in document order.
    pub fn lines(&self) -> &[FlowLine] {
        &self.lines
    }

    /// The lines on the given pages.
    pub fn lines_on(&self, pages: Range<usize>) -> &[FlowLine] {
        let end = pages.end.min(self.page_lines.len());
        let start = pages.start.min(end);
        match (
            self.page_lines.get(start),
            self.page_lines.get(end.wrapping_sub(1)),
        ) {
            (Some(first), Some(last)) if start < end => &self.lines[first.start..last.end],
            _ => &[],
        }
    }

    /// The pages that intersect the vertical span `top..bottom` in viewport
    /// coordinates, never empty so the nearest page is always represented.
    pub fn visible_pages(&self, top: f32, bottom: f32) -> Range<usize> {
        let count = self.page_bounds.len();
        let first = self
            .page_bounds
            .partition_point(|page| page.y + page.height < top)
            .min(count - 1);
        let end = self
            .page_bounds
            .partition_point(|page| page.y <= bottom)
            .clamp(first + 1, count);
        first..end
    }

    /// Selection highlights or the caret for `selection`, limited to `pages`.
    pub fn selection_rects(
        &self,
        doc: &WriterDocument,
        selection: &TextSelection,
        pages: Range<usize>,
    ) -> Vec<SelectionRect> {
        let end = pages.end.min(self.page_lines.len());
        let start = pages.start.min(end);
        let window = match (
            self.page_lines.get(start),
            self.page_lines.get(end.wrapping_sub(1)),
        ) {
            (Some(first), Some(last)) if start < end => first.start..last.end,
            _ => 0..0,
        };
        rects_for_lines(
            doc,
            &self.style,
            self.zoom,
            &self.block_starts,
            &self.lines,
            selection,
            window,
        )
    }

    /// The rectangle of a collapsed caret at `offset`, wherever it falls in
    /// the document.
    pub fn caret_rect(
        &self,
        doc: &WriterDocument,
        selection: &TextSelection,
    ) -> Option<SelectionRect> {
        rects_for_lines(
            doc,
            &self.style,
            self.zoom,
            &self.block_starts,
            &self.lines,
            selection,
            0..self.lines.len(),
        )
        .into_iter()
        .next()
    }
}

#[cfg(test)]
thread_local! {
    /// Blocks wrapped from scratch on this thread, for cache tests.
    static WRAP_MISSES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
mod layout_tests;

#[cfg(test)]
mod layout_checks;
