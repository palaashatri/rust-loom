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
use std::sync::Arc;

use loom_text::StyleRun;

mod metrics;
mod placement;
mod selection;
mod shaped_line;
pub(crate) use metrics::{block_bidi_runs, block_positions};
use placement::{Placer, Segment};
use selection::{floor_in_blocks, rects_for_lines};
pub use shaped_line::shape_wrapped_block_lines;
pub(crate) use shaped_line::{parse_rgba, shape_block_line, shape_block_lines};
pub use shaped_line::{
    shape_text_line, WriterCaretStop, WriterPaintedRun, WriterSelectionSpan, WriterShapedVisualLine,
};

use crate::text_metrics::bidi::BidiRuns;
use crate::text_metrics::justification::{line_geometry_with_measure, measure_block};
use crate::text_metrics::LineMeasure;
use crate::{
    DocumentPage, LayoutFragment, PageFragment, PageLayout, PageRect, PageStyle, PageViewport,
    RichBlock, SelectionRect, TextSelection, VisibleRange, WriterDocument, PAGE_GAP_PT,
};

#[derive(Debug, Clone, PartialEq)]
struct WrappedLine {
    start: usize,
    end: usize,
    advance_pt: f32,
    shaped_line: Option<Arc<WriterShapedVisualLine>>,
}

struct CachedBlock {
    graphemes: usize,
    /// The font catalogue the measurement was made against.
    epoch: u64,
    font_bits: u32,
    width_bits: u32,
    kind: String,
    text: String,
    runs: Vec<StyleRun>,
    direction: loom_text::ParagraphDirection,
    alignment: loom_text::Alignment,
    /// Where every grapheme of the text sits, independent of the wrap width.
    measure: Rc<LineMeasure>,
    /// Paragraph bidi levels, shared by every wrapped line of this block.
    bidi_runs: Option<Rc<BidiRuns>>,
    lines: Rc<[WrappedLine]>,
    words: usize,
    chars: usize,
    line_height_pt: f32,
    seen: u64,
}

/// What the cache knows about one block.
#[derive(Clone)]
struct BlockInfo {
    lines: Rc<[WrappedLine]>,
    words: usize,
    chars: usize,
    graphemes: usize,
    line_height_pt: f32,
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
        let epoch = crate::fonts::font_epoch();
        // The grapheme positions depend on the text, runs and size only, so a
        // block that is re-wrapped at a new width keeps them.
        let mut reusable: Option<Rc<LineMeasure>> = None;
        let mut reusable_bidi: Option<Option<Rc<BidiRuns>>> = None;
        if let Some(hit) = cache.blocks.get_mut(&block.id) {
            if hit.epoch == epoch
                && hit.font_bits == font_bits
                && hit.text == text
                && hit.runs == block.runs
                && hit.kind == block.kind
                && hit.direction == block.style.direction
                && hit.alignment == block.style.alignment
            {
                if hit.width_bits == width_bits {
                    hit.seen = generation;
                    return BlockInfo {
                        lines: hit.lines.clone(),
                        words: hit.words,
                        chars: hit.chars,
                        graphemes: hit.graphemes,
                        line_height_pt: hit.line_height_pt,
                    };
                }
                reusable = Some(hit.measure.clone());
                reusable_bidi = Some(hit.bidi_runs.clone());
            }
        }
        #[cfg(test)]
        WRAP_MISSES.with(|misses| misses.set(misses.get() + 1));
        let measure = reusable.unwrap_or_else(|| Rc::new(measure_block(block, font_size)));
        let bidi_runs = reusable_bidi
            .unwrap_or_else(|| BidiRuns::with_direction(text, block.style.direction).map(Rc::new));
        // A table is one line per row: its cells are drawn in columns, not wrapped.
        let measured_lines: Vec<((usize, usize), Option<WriterShapedVisualLine>)> =
            if block.kind == crate::TABLE_BLOCK_KIND {
                crate::tables::table_row_spans(text)
                    .into_iter()
                    .map(|span| (span, None))
                    .collect()
            } else if let Some(shaped) = shape_wrapped_block_lines(block, font_size, width) {
                shaped
                    .into_iter()
                    .map(|(range, line)| ((range.start, range.end), line))
                    .collect()
            } else {
                let spans: Vec<(usize, usize)> = measure.wrap(text, width).into_iter().collect();
                let line_ranges: Vec<_> = spans.iter().map(|(start, end)| *start..*end).collect();
                shape_block_lines(block, &line_ranges, font_size, width)
                    .into_iter()
                    .zip(spans)
                    .map(|(line, span)| (span, line))
                    .collect()
            };
        let lines: Vec<WrappedLine> = measured_lines
            .into_iter()
            .map(|((start, end), shaped_line)| {
                let shaped_line = shaped_line.map(Arc::new);
                WrappedLine {
                    start,
                    end,
                    advance_pt: shaped_line
                        .as_ref()
                        .map_or_else(|| measure.advance(start, end), |line| line.width_pt()),
                    shaped_line,
                }
            })
            .collect();
        let line_height_pt = lines
            .iter()
            .filter_map(|line| line.shaped_line.as_deref())
            .map(|line| line.ascent_pt() + line.descent_pt() + line.line_gap_pt())
            .fold(0.0, f32::max);
        let lines: Rc<[WrappedLine]> = lines.into();
        let (mut words, mut chars) = (0, 0);
        crate::tables::for_each_counted_text(block, |part| {
            words += part.split_whitespace().count();
            chars += part.chars().count();
        });
        let graphemes = crate::grapheme_count(text);
        cache.blocks.insert(
            block.id,
            CachedBlock {
                epoch,
                font_bits,
                width_bits,
                kind: block.kind.clone(),
                text: text.to_string(),
                runs: block.runs.clone(),
                direction: block.style.direction,
                alignment: block.style.alignment,
                measure,
                bidi_runs,
                lines: lines.clone(),
                words,
                chars,
                graphemes,
                line_height_pt,
                seen: generation,
            },
        );
        BlockInfo {
            lines,
            words,
            chars,
            graphemes,
            line_height_pt,
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
    /// Shared shaped line geometry and paint runs, if all faces were resolved.
    pub shaped_line: Option<Arc<WriterShapedVisualLine>>,
}

impl FlowLine {
    fn global_end(&self) -> usize {
        self.global_start + (self.end - self.start)
    }
}

/// The height of one text line of `block`. A table's rows are drawn taller than
/// a line of body text (cell padding), so its lines reserve the row height the
/// page draws and whatever follows the table starts below it.
pub(crate) fn text_line_height(style: &PageStyle, block: &RichBlock) -> f32 {
    let base = if block.kind == crate::TABLE_BLOCK_KIND {
        crate::table_row_height(style, crate::table_line_count(block.text.as_str()))
    } else {
        crate::line_pitch(&block.runs, style.font_size_for_kind(block.kind.as_str()))
            * style.line_height
    };
    base * line_spacing_factor(block)
}

/// The block's line spacing relative to the default paragraph spacing, so a
/// paragraph at the default spacing keeps the page's own line height.
pub(crate) fn line_spacing_factor(block: &RichBlock) -> f32 {
    let factor = block.style.line_spacing / loom_text::ParagraphStyle::default().line_spacing;
    if factor.is_finite() && factor > 0.0 {
        factor
    } else {
        1.0
    }
}

/// Room a list item keeps to the left of its text for its marker, in ems of the
/// item's font. Three digits and the gap after them fit in it.
const LIST_MARKER_ROOM_EMS: f32 = 2.5;

impl RichBlock {
    /// Points the block's lines are indented from the left margin: its left
    /// indent plus its list level's indent, never negative and never so large
    /// that the column closes. The PDF export indents from this; the editor
    /// also reserves [`Self::list_marker_room_pt`] for a list item's marker.
    pub fn left_indent_pt(&self, style: &PageStyle) -> f32 {
        let usable = style.width_pt - style.margin_left_pt - style.margin_right_pt;
        let indent = if self.style.left_indent.is_finite() {
            self.style.left_indent
        } else {
            0.0
        };
        (indent + self.list_indent_pt()).clamp(0.0, (usable - 1.0).max(0.0))
    }

    /// The room a list item keeps to the left of its text in the editor, for
    /// its marker: the marker is drawn in that room, so the item's text starts
    /// right of the body column. Other blocks keep no room.
    pub fn list_marker_room_pt(&self, style: &PageStyle) -> f32 {
        if crate::list_levels::is_list_kind(&self.kind) {
            style.font_size_for_kind(self.kind.as_str()) * LIST_MARKER_ROOM_EMS
        } else {
            0.0
        }
    }

    /// Points from the left margin to where the block's text starts in the
    /// editor: its [`Self::left_indent_pt`] plus its marker room, never so large
    /// that the column closes. Lines, the caret and the wrap all start here.
    pub fn text_start_pt(&self, style: &PageStyle) -> f32 {
        let usable = style.width_pt - style.margin_left_pt - style.margin_right_pt;
        (self.left_indent_pt(style) + self.list_marker_room_pt(style))
            .clamp(0.0, (usable - 1.0).max(0.0))
    }

    /// The width the block's lines wrap and align to: the text column less
    /// its text start.
    pub fn text_column_width(&self, style: &PageStyle) -> f32 {
        let usable = style.width_pt - style.margin_left_pt - style.margin_right_pt;
        (usable - self.text_start_pt(style)).max(1.0)
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
        let mut placer = Placer::new(usable_height);
        let mut block_starts = Vec::with_capacity(self.blocks.len());
        let mut block_start = 0usize;
        for (block_index, block) in self.blocks.iter().enumerate() {
            block_starts.push(block_start);
            // Pagination must use the same block metrics as the geometry: a
            // heading has wider glyphs and a taller line box than body text.
            let font_size = style.font_size_for_kind(block.kind.as_str());
            let picture = block.object_display_size(usable_width, usable_height);
            let info = wrapped(block, font_size, block.text_column_width(style));
            // Typed table cells may wrap and carry larger run metrics than the
            // table's flat editor text can express. Keep one reserved height
            // per typed row, falling back only when shaping itself is refused;
            // the projection reports that refusal visibly.
            let table_row_heights = if block.kind == crate::TABLE_BLOCK_KIND {
                block.typed_table().and_then(|table| {
                    crate::measure_table_row_geometry(
                        table,
                        block.text_column_width(style),
                        crate::TABLE_CELL_BASE_FONT_SIZE_PT,
                    )
                    .ok()
                    .filter(|geometry| geometry.row_heights_pt().len() == info.lines.len())
                    .map(|geometry| geometry.row_heights_pt().to_vec())
                })
            } else {
                None
            };
            let line_height = picture.map_or_else(
                || text_line_height(style, block).max(info.line_height_pt),
                |(_, h)| h,
            );
            if block.kind.starts_with("heading") && placer.has_content() {
                // A heading never ends a page without the start of its text.
                let own = info.lines.len() as f32 * line_height;
                let mut used = placer.used + own;
                if used + body_line <= usable_height + f32::EPSILON {
                    used += body_line;
                }
                if let Some(next) = self.blocks.get(block_index + 1) {
                    let next_size = style.font_size_for_kind(next.kind.as_str());
                    let next_lines = wrapped(next, next_size, next.text_column_width(style));
                    let next_height = next
                        .object_display_size(usable_width, usable_height)
                        .map_or(text_line_height(style, next), |(_, h)| h);
                    let wanted = if next_lines.lines.len() >= 3 { 2 } else { 1 };
                    let mut probe = Placer::new(usable_height);
                    probe.used = used;
                    if !probe.fits(wanted, next_height) {
                        placer.next_page();
                    }
                }
            }
            let text = block.text.as_str();
            let controlled = picture.is_none() && block.kind != crate::TABLE_BLOCK_KIND;
            let mut segment_start = 0usize;
            for (index, line) in info.lines.iter().enumerate() {
                let breaks = text[line.start..line.end].ends_with('\u{c}');
                if breaks || index + 1 == info.lines.len() {
                    let line_heights = table_row_heights
                        .as_ref()
                        .map(|heights| &heights[segment_start..=index]);
                    placer.place(&Segment {
                        block_index,
                        block_start,
                        lines: &info.lines[segment_start..=index],
                        line_height,
                        line_heights,
                        fixed_width: picture.map(|(w, _)| w),
                        controlled,
                        break_after: breaks,
                    });
                    segment_start = index + 1;
                }
            }
            // Paragraph spacing consumes one body line unless the next block
            // starts on a fresh page.
            if block_index + 1 < self.blocks.len()
                && placer.used > 0.0
                && placer.used + body_line <= usable_height + f32::EPSILON
            {
                placer.used += body_line;
            }
            block_start += block.text.len_bytes() + 1;
        }
        let (lines, page_lines) = placer.finish();
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
        let usable_width = style.width_pt - style.margin_left_pt - style.margin_right_pt;
        let usable_height = style.height_pt - style.margin_top_pt - style.margin_bottom_pt;

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
                let line = &lines[at];
                let shaped_height = line.shaped_line.as_ref().map_or(0.0, |shaped| {
                    shaped.ascent_pt() + shaped.descent_pt() + shaped.line_gap_pt()
                });
                let reserved_height_pt = line.bounds.height;
                let line_height = block
                    .object_display_size(usable_width, usable_height)
                    .map_or_else(
                        || {
                            let base_height = if block.kind == crate::TABLE_BLOCK_KIND
                                && block.typed_table().is_some()
                            {
                                reserved_height_pt
                            } else {
                                reserved_height_pt.max(text_line_height(style, block))
                            };
                            base_height.max(shaped_height)
                        },
                        |(_, h)| h,
                    )
                    * zoom;
                let geometry = line_geometry_with_measure(
                    block,
                    line.start..line.end,
                    &block_positions(block, style.font_size_for_kind(block.kind.as_str())),
                    block.text_column_width(style),
                    block.style.alignment.resolve(block.text.as_str())
                        == loom_text::Alignment::Justify,
                );
                let width = line.shaped_line.as_ref().map_or_else(
                    || {
                        if geometry.gaps.is_empty() {
                            line.bounds.width * zoom
                        } else {
                            geometry.width * zoom
                        }
                    },
                    |shaped| shaped.width_pt() * zoom,
                );
                lines[at].bounds = PageRect {
                    x: content_x + block.text_start_pt(style) * zoom,
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
                    shaped_line: shape_block_line(
                        &self.blocks[block_index],
                        source.start..source.end,
                        style.font_size_for_kind(self.blocks[block_index].kind.as_str()),
                        self.blocks[block_index].text_column_width(style),
                    )
                    .map(Arc::new),
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

    /// Floor both selection ends to grapheme boundaries and clamp them to the
    /// editor text, without building that text.
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

    /// Number of user-perceived characters before `offset`; whole preceding
    /// blocks come from the per-block layout cache.
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
        begin_pass();
        let before: usize = self.blocks[..index]
            .iter()
            .map(|block| {
                let font_size = style.font_size_for_kind(block.kind.as_str());
                wrapped(block, font_size, block.text_column_width(&style)).graphemes + 1
            })
            .sum();
        end_pass(self.blocks.len());
        let text = self.blocks[index].text.as_str();
        before + crate::grapheme_count(&text[..offset - starts[index]])
    }

    /// Words and characters in the editor text, from the per-block cache.
    pub fn text_counts(&self) -> (usize, usize) {
        let style = self.page.page_style();
        begin_pass();
        let (mut words, mut chars) = (0usize, 0usize);
        for block in &self.blocks {
            let font_size = style.font_size_for_kind(block.kind.as_str());
            let info = wrapped(block, font_size, block.text_column_width(&style));
            words += info.words;
            chars += info.chars;
        }
        end_pass(self.blocks.len());
        (words, chars + self.blocks.len().saturating_sub(1))
    }
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

#[cfg(test)]
mod indent_tests;

#[cfg(test)]
mod list_indent_tests;

#[cfg(test)]
mod break_tests;

#[cfg(test)]
#[path = "rtl_paragraph_tests.rs"]
mod rtl_paragraph_tests;
