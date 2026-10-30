use std::collections::HashSet;
use std::ops::Range;

use loom_writer_core::{DocumentFlow, PageRect, TextSelection, WriterDocument};
use slint::SharedString;

use crate::{WriterFootnoteLine, WriterFootnoteReference};

pub(super) fn project_footnotes(
    doc: &WriterDocument,
    flow: &DocumentFlow,
    base_page: PageRect,
    zoom: f32,
    pages: &Range<usize>,
) -> (Vec<WriterFootnoteLine>, Vec<WriterFootnoteReference>) {
    if !zoom.is_finite() || zoom <= 0.0 {
        return (Vec::new(), Vec::new());
    }
    let mut continued_labels = HashSet::new();
    let mut lines = Vec::new();
    for placement in flow
        .footnote_lines()
        .iter()
        .filter(|placement| pages.contains(&placement.page))
    {
        let Some(note) = doc
            .footnotes
            .iter()
            .find(|note| note.id == placement.note_id)
        else {
            continue;
        };
        let Some(body) = note.body.get(placement.body_range.clone()) else {
            continue;
        };
        let continuation_prefix =
            if placement.continued && continued_labels.insert((note.id, placement.page)) {
                "(continued). "
            } else {
                ""
            };
        let number = if placement.show_number {
            format!("{}.", placement.number)
        } else {
            String::new()
        };
        lines.push(WriterFootnoteLine {
            number: SharedString::from(number.as_str()),
            text: SharedString::from(format!("{continuation_prefix}{body}")),
            accessible_label: SharedString::from(format!("Footnote {}: {body}", placement.number)),
            number_x: ((placement.number_bounds.x - base_page.x) / zoom).max(0.0),
            text_x: ((placement.text_bounds.x - base_page.x) / zoom).max(0.0),
            y: ((placement.number_bounds.y - base_page.y) / zoom).max(0.0),
            number_width: (placement.number_bounds.width / zoom).max(0.0),
            text_width: (placement.text_bounds.width / zoom).max(1.0),
            font_size: placement.font_size_pt,
            line_height: (placement.number_bounds.height / zoom).max(1.0),
        });
    }

    let style = doc.page.page_style();
    let mut references = Vec::new();
    for placement in flow
        .footnote_references()
        .iter()
        .filter(|placement| pages.contains(&placement.page))
    {
        let Some(note) = doc
            .footnotes
            .iter()
            .find(|note| note.id == placement.note_id)
        else {
            continue;
        };
        let marker_rect = flow.footnote_reference_rect(doc, placement);
        let fallback_rect = || {
            flow.selection_rects(
                doc,
                &TextSelection::caret(placement.global_offset),
                placement.page..placement.page + 1,
            )
            .into_iter()
            .next()
            .map(|rect| rect.rect)
        };
        let Some(rect) = marker_rect.or_else(fallback_rect) else {
            continue;
        };
        let reference_body_size =
            if doc.blocks[placement.block_index].kind == loom_writer_core::TABLE_BLOCK_KIND {
                loom_writer_core::TABLE_CELL_BASE_FONT_SIZE_PT
            } else {
                style.font_size_for_kind(doc.blocks[placement.block_index].kind.as_str())
            };
        let reference_font_size = (reference_body_size * 0.65).max(5.0);
        let label = placement.number.to_string();
        references.push(WriterFootnoteReference {
            id: SharedString::from(note.id.to_string()),
            label: SharedString::from(label.as_str()),
            accessible_label: SharedString::from(format!(
                "Footnote {} reference, activate to edit",
                placement.number
            )),
            x: ((rect.x - base_page.x) / zoom).max(0.0),
            y: ((rect.y - base_page.y) / zoom - reference_font_size * 0.35).max(0.0),
            font_size: reference_font_size,
            width: placement.marker_width_pt,
        });
    }
    (lines, references)
}
