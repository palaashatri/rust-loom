//! PDF drawing for the shared footnote placement plan.

use std::collections::HashMap;
use std::ops::Range;

use loom_fonts::Affinity;
use loom_pdf::{PageIndex, PdfDocument};

use crate::export::shaped_export::{self, FontRegistry};
use crate::{
    shape_wrapped_block_lines, DocumentFlow, PageStyle, RichBlock, WriterDocument, WriterError,
    WriterShapedVisualLine,
};

#[cfg(test)]
#[path = "export_footnotes_tests.rs"]
mod tests;

/// Draw a small raised ordinal at an already-resolved source-text caret.
pub(crate) fn draw_reference_marker(
    pdf: &mut PdfDocument,
    fonts: &mut FontRegistry,
    page: PageIndex,
    number: usize,
    x: f32,
    line_top: f32,
    body_size_pt: f32,
    context: &str,
) -> Result<(), WriterError> {
    let text = format!("{number}");
    let size_pt = (body_size_pt * 0.65).max(5.0);
    let block = RichBlock::new(0, "paragraph", text.as_str());
    let block = shaped_export::with_automatic_ink(&block);
    let shaped = shape_wrapped_block_lines(&block, size_pt, size_pt * 4.0)
        .and_then(|mut lines| {
            (lines.len() == 1)
                .then(|| lines.pop().and_then(|(_, line)| line))
                .flatten()
        })
        .ok_or_else(|| WriterError::Invalid(format!("PDF export could not shape {context}")))?;
    shaped_export::draw_line(
        pdf,
        fonts,
        page,
        context,
        shaped_export::PdfLine {
            text: &text,
            line_top_pdf: line_top + body_size_pt * 0.22,
            text_origin_x: x,
            shaped_line: &shaped,
        },
    )
}

/// Draw the note-body lines at the same page coordinates used by `DocumentFlow`.
pub(crate) fn draw_body_lines(
    pdf: &mut PdfDocument,
    fonts: &mut FontRegistry,
    pages: &[PageIndex],
    flow: &DocumentFlow,
    doc: &WriterDocument,
    page_style: &PageStyle,
) -> Result<(), WriterError> {
    let mut shaped_by_note: HashMap<u64, Vec<(Range<usize>, Option<WriterShapedVisualLine>)>> =
        HashMap::with_capacity(doc.footnotes.len());
    for note in &doc.footnotes {
        let first = flow
            .footnote_lines()
            .iter()
            .find(|line| line.note_id == note.id)
            .ok_or_else(|| {
                WriterError::Invalid(format!(
                    "PDF export has no placement for footnote {}",
                    note.id
                ))
            })?;
        let block = RichBlock::new(note.id, "paragraph", note.body.as_str());
        let block = shaped_export::with_automatic_ink(&block);
        let lines = shape_wrapped_block_lines(
            &block,
            first.font_size_pt,
            first.text_bounds.width / flow.zoom(),
        )
        .ok_or_else(|| {
            WriterError::Invalid(format!("PDF export could not shape footnote {}", note.id))
        })?;
        shaped_by_note.insert(note.id, lines);
    }

    for placement in flow.footnote_lines() {
        let page = *pages
            .get(placement.page)
            .ok_or_else(|| WriterError::Invalid("PDF export footnote page is missing".into()))?;
        let note = doc
            .footnotes
            .iter()
            .find(|note| note.id == placement.note_id)
            .ok_or_else(|| {
                WriterError::Invalid(format!(
                    "PDF export footnote {} is missing",
                    placement.note_id
                ))
            })?;
        let shaped = shaped_by_note
            .get(&placement.note_id)
            .and_then(|lines| {
                lines
                    .iter()
                    .find(|(range, _)| *range == placement.body_range)
            })
            .and_then(|(_, shaped)| shaped.as_ref());
        let page_bounds = flow
            .page_bounds()
            .get(placement.page)
            .ok_or_else(|| WriterError::Invalid("PDF export footnote bounds are missing".into()))?;
        let line_top = page_style.height_pt - (placement.text_bounds.y - page_bounds.y);
        let text_origin_x = placement.text_bounds.x - page_bounds.x;
        if placement.show_number {
            draw_reference_marker(
                pdf,
                fonts,
                page,
                placement.number,
                placement.number_bounds.x - page_bounds.x,
                line_top,
                placement.font_size_pt,
                &format!("footnote {} label", placement.note_id),
            )?;
        }
        if let Some(shaped) = shaped {
            let context = format!("footnote {} body", note.id);
            shaped_export::draw_line(
                pdf,
                fonts,
                page,
                &context,
                shaped_export::PdfLine {
                    text: note.body.as_str(),
                    line_top_pdf: line_top,
                    text_origin_x,
                    shaped_line: shaped,
                },
            )?;
        }
    }
    Ok(())
}

/// Find the physical caret x for a byte offset in a shaped source line.
pub(crate) fn caret_x(line: &WriterShapedVisualLine, offset: usize) -> f32 {
    line.inline_marker_x(offset)
        .unwrap_or_else(|| line.caret_x(offset, Affinity::Leading))
}
