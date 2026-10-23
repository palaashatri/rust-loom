//! Pointer-to-document hit testing for Writer's page canvas.

use loom_writer_core::{PageViewport, WriterDocument};
use slint::{Model, ModelRc};

use crate::WriterTableCellRect;

/// Convert editor-local, zoomed pointer coordinates to a canonical document
/// byte offset, using a fresh projection for callers without the app model.
#[cfg(test)]
pub(super) fn writer_pointer_offset(
    doc: &WriterDocument,
    viewport: PageViewport,
    x: f32,
    y: f32,
) -> Option<usize> {
    if doc
        .blocks
        .iter()
        .any(|block| block.kind == loom_writer_core::TABLE_BLOCK_KIND)
    {
        let style = doc.page.page_style();
        let zoom = crate::normalize_page_zoom(viewport.zoom, 1.0);
        let projection = crate::projection::project(doc, viewport, Some(style.height_pt * zoom));
        return writer_pointer_offset_with_rect_lookup(
            doc,
            viewport,
            x,
            y,
            projection.table_cell_rects.len(),
            |index| projection.table_cell_rects.get(index).cloned(),
        );
    }

    writer_pointer_offset_with_rect_lookup(doc, viewport, x, y, 0, |_| None)
}

/// Use the app's already-published rectangles during pointer motion so drag
/// selection does not rebuild the full document projection for every event.
pub(super) fn writer_pointer_offset_with_model(
    doc: &WriterDocument,
    viewport: PageViewport,
    x: f32,
    y: f32,
    table_rects: &ModelRc<WriterTableCellRect>,
) -> Option<usize> {
    writer_pointer_offset_with_rect_lookup(doc, viewport, x, y, table_rects.row_count(), |index| {
        table_rects.row_data(index)
    })
}

fn writer_pointer_offset_with_rect_lookup(
    doc: &WriterDocument,
    viewport: PageViewport,
    x: f32,
    y: f32,
    rect_count: usize,
    mut rect_at: impl FnMut(usize) -> Option<WriterTableCellRect>,
) -> Option<usize> {
    if doc.blocks.is_empty() {
        return Some(0);
    }

    let zoom = crate::normalize_page_zoom(viewport.zoom, 1.0);
    let local_x = x / zoom;
    let local_y = y / zoom;
    for index in 0..rect_count {
        let Some(rect) = rect_at(index) else {
            continue;
        };
        if rect.caret_offset >= 0
            && local_x >= rect.x
            && local_x < rect.x + rect.width
            && local_y >= rect.y
            && local_y < rect.y + rect.height
        {
            let cell_offset = usize::try_from(rect.caret_offset).ok()?;
            return Some(
                table_cell_pointer_offset(doc, cell_offset, &rect, local_x).unwrap_or(cell_offset),
            );
        }
    }

    writer_text_metric_offset(doc, viewport, x, y)
}

fn table_cell_pointer_offset(
    doc: &WriterDocument,
    cell_offset: usize,
    rect: &WriterTableCellRect,
    local_x: f32,
) -> Option<usize> {
    let location = doc.table_cell_at_offset(cell_offset)?;
    let (display_text, source_offsets) =
        doc.table_cell_display_map(location.block_id, location.row, location.col)?;
    let table = doc.table_from_block(location.block_id)?;
    let catalog_style = loom_objects::TableStyleCatalog::get(&table.style_id)
        .unwrap_or_else(loom_objects::TableStyleCatalog::default_style);
    let options = loom_objects::TableStyleOptions {
        header_row: table.header_row,
        banded_rows: table.banded_rows,
        first_column: table.first_column,
        ..Default::default()
    };
    let bold = catalog_style
        .resolve_cell(
            location.row,
            location.col,
            table.rows.len(),
            table.columns(),
            &options,
            None,
        )
        .bold;
    let runs = if bold {
        vec![loom_text::StyleRun {
            start: 0,
            end: display_text.len(),
            style: loom_text::CharacterStyle {
                weight: loom_text::FontWeight::Bold,
                ..Default::default()
            },
        }]
    } else {
        Vec::new()
    };
    let font_size = loom_writer_core::PageStyle::default().font_size_for_kind("paragraph");
    let display_byte =
        loom_writer_core::offset_at_x(&display_text, 0, &runs, font_size, local_x - rect.x - 6.0);
    let source_byte = source_offsets.get(display_byte).copied()?;
    let block_start = doc
        .blocks
        .iter()
        .take(location.block_index)
        .map(|block| block.text.as_str().len() + 1)
        .sum::<usize>();
    Some(block_start + source_byte)
}

fn writer_text_metric_offset(
    doc: &WriterDocument,
    viewport: PageViewport,
    x: f32,
    y: f32,
) -> Option<usize> {
    if doc.blocks.is_empty() {
        return Some(0);
    }
    let style = doc.page.page_style();
    let zoom = crate::normalize_page_zoom(viewport.zoom, 1.0);
    let local_x = x / zoom;
    let local_y = y / zoom;
    let flow = doc
        .flow(
            &style,
            PageViewport {
                width: style.width_pt,
                height: style.height_pt,
                zoom: 1.0,
                scroll_x: 0.0,
                scroll_y: 0.0,
            },
        )
        .ok()?;
    let base_page = flow.page_bounds().first().copied()?;
    // Only the page under the pointer and its neighbours can hold the nearest
    // line, so the rest of a long document is never visited.
    let pointer_y = local_y + base_page.y + style.margin_top_pt;
    let reach = style.height_pt + loom_writer_core::PAGE_GAP_PT;
    let pages = flow.visible_pages(pointer_y - reach, pointer_y + reach);

    let mut nearest: Option<(f32, usize)> = None;
    for line in flow.lines_on(pages) {
        let block = &doc.blocks[line.block_index];
        let font_size = style.font_size_for_kind(block.kind.as_str());
        let line_y = line.bounds.y - base_page.y - style.margin_top_pt;
        let line_height = line.bounds.height;
        let distance = if local_y < line_y {
            line_y - local_y
        } else if local_y > line_y + line_height {
            local_y - (line_y + line_height)
        } else {
            0.0
        };
        // A line no nearer than the best so far is never chosen; skip shaping it.
        if nearest.is_some_and(|(best_distance, _)| distance >= best_distance) {
            continue;
        }
        // The column the line is aligned in: the text column less the indent.
        let column_width = block.text_column_width(&style);
        let alignment_offset = match block.style.alignment {
            loom_text::Alignment::Center => ((column_width - line.bounds.width) / 2.0).max(0.0),
            loom_text::Alignment::Right => (column_width - line.bounds.width).max(0.0),
            loom_text::Alignment::Left | loom_text::Alignment::Justify => 0.0,
        };
        let line_x = line.bounds.x - base_page.x - style.margin_left_pt + alignment_offset;
        let local_byte = loom_writer_core::offset_at_x(
            &block.text.as_str()[line.start..line.end],
            line.start,
            &block.runs,
            font_size,
            local_x - line_x,
        );
        let offset = line.global_start + local_byte;
        if distance <= f32::EPSILON {
            return Some(offset);
        }
        if nearest
            .map(|(best_distance, _)| distance < best_distance)
            .unwrap_or(true)
        {
            nearest = Some((distance, offset));
        }
    }
    nearest.map(|(_, offset)| offset)
}

#[cfg(test)]
#[path = "table_pointer_geometry_tests.rs"]
mod table_pointer_geometry_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointer_in_populated_cell_after_empty_cell_uses_painted_cell() {
        let markdown = "|  | B |";
        let mut document = WriterDocument::new("empty-leading-cell", "Empty leading cell");
        document.push(loom_writer_core::markdown_table_block(1, markdown));
        assert_eq!(
            document.table_cell_ranges(1),
            Some(vec![vec![(0, 0), (1, 2)]]),
            "the first cell is empty and B follows its separator"
        );
        let viewport = PageViewport::default();
        let projection = crate::projection::project(
            &document,
            viewport,
            Some(loom_writer_core::PageStyle::default().height_pt),
        );
        assert_eq!(projection.table_cell_rects.len(), 2);

        let rects = projection.table_cell_rects;
        let populated_cell = &rects[1];
        assert_eq!(populated_cell.caret_offset, 1, "B starts after the empty cell and its separator");
        let x = populated_cell.x + 6.0;
        let y = populated_cell.y + populated_cell.height / 2.0;
        let hit = writer_pointer_offset_with_rect_lookup(
            &document,
            viewport,
            x,
            y,
            rects.len(),
            |index| rects.get(index).cloned(),
        )
        .expect("pointer inside the projected B cell resolves to a document offset");

        assert_eq!(
            hit, 1,
            "clicking at B's painted text origin places the caret before B"
        );
        let location = document
            .table_cell_at_offset(hit)
            .expect("the pointer result remains inside the table");
        assert_eq!((location.row, location.col), (0, 1));
    }
}
