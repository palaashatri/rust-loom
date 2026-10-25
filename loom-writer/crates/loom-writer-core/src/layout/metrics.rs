//! Cached whole-block measurements and paragraph bidi levels.

use std::rc::Rc;

use crate::text_metrics::bidi::BidiRuns;
use crate::text_metrics::justification::measure_block;
use crate::text_metrics::LineMeasure;
use crate::RichBlock;

use super::WRAP_CACHE;

/// Where every grapheme of `block` sits at `font_size`: from the cache when
/// the block is unchanged since it was wrapped, else measured on demand.
pub(crate) fn block_positions(block: &RichBlock, font_size: f32) -> Rc<LineMeasure> {
    let text = block.text.as_str();
    let epoch = crate::fonts::font_epoch();
    let cached = WRAP_CACHE.with(|cache| {
        cache
            .borrow()
            .blocks
            .get(&block.id)
            .filter(|hit| {
                hit.epoch == epoch
                    && hit.font_bits == font_size.to_bits()
                    && hit.text == text
                    && hit.runs == block.runs
                    && hit.kind == block.kind
            })
            .map(|hit| hit.measure.clone())
    });
    cached.unwrap_or_else(|| Rc::new(measure_block(block, font_size)))
}

/// Paragraph bidi levels from the cached wrap, or computed on demand for an
/// unwrapped block. Bidi depends on the paragraph text and its block identity.
pub(crate) fn block_bidi_runs(block: &RichBlock) -> Option<Rc<BidiRuns>> {
    let text = block.text.as_str();
    let cached = WRAP_CACHE.with(|cache| {
        cache
            .borrow()
            .blocks
            .get(&block.id)
            .filter(|hit| hit.text == text && hit.kind == block.kind)
            .map(|hit| hit.bidi_runs.clone())
    });
    cached.unwrap_or_else(|| BidiRuns::of(text).map(Rc::new))
}
