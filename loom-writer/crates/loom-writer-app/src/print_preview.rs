use std::borrow::Cow;
use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use loom_desktop::NativeMenuBar;
use loom_objects::{TableStyleCatalog, TableStyleOptions};
use loom_text::{Alignment, FontWeight};
use loom_writer_core::pdf_export_view::PdfRevisionView;
use loom_writer_core::{
    editor_column_widths, HeaderFooterAlignment, HeaderFooterLineKind, PageMarginsPreset,
    PageOrientation, PageRect, PageSetup, PageStyle, PageViewport, PaperSize, RevisionKind,
    RevisionRecord, RichBlock, WriterDocument, TABLE_BLOCK_KIND, TABLE_CELL_BASE_FONT_SIZE_PT,
};
use slint::{Color, ComponentHandle, ModelRc, SharedString, VecModel};

use crate::{
    GuiState, HistoryKind, PrintPreviewBoxRect, PrintPreviewState, PrintPreviewTextLine, WriterApp,
};

struct PreviewSnapshot {
    total_pages: i32,
    current_page: i32,
    is_empty: bool,
    lines: Vec<PrintPreviewTextLine>,
    boxes: Vec<PrintPreviewBoxRect>,
}

fn paper_from_index(idx: i32) -> PaperSize {
    match idx {
        1 => PaperSize::Letter,
        _ => PaperSize::A4,
    }
}

fn paper_to_index(paper: PaperSize) -> i32 {
    match paper {
        PaperSize::Letter => 1,
        _ => 0,
    }
}

fn orientation_from_index(idx: i32) -> PageOrientation {
    match idx {
        1 => PageOrientation::Landscape,
        _ => PageOrientation::Portrait,
    }
}

fn orientation_to_index(orientation: PageOrientation) -> i32 {
    match orientation {
        PageOrientation::Landscape => 1,
        PageOrientation::Portrait => 0,
    }
}

fn margins_from_index(idx: i32) -> PageMarginsPreset {
    match idx {
        1 => PageMarginsPreset::Narrow,
        2 => PageMarginsPreset::Wide,
        _ => PageMarginsPreset::Normal,
    }
}

fn margins_to_index(margins: PageMarginsPreset) -> i32 {
    match margins {
        PageMarginsPreset::Narrow => 1,
        PageMarginsPreset::Wide => 2,
        _ => 0,
    }
}

fn revision_view_from_index(idx: i32) -> PdfRevisionView {
    match idx {
        1 => PdfRevisionView::Review,
        _ => PdfRevisionView::Final,
    }
}

fn build_preview_page_setup(
    base: &PageSetup,
    paper_idx: i32,
    orientation_idx: i32,
    margins_idx: i32,
) -> PageSetup {
    let mut setup = base.clone();
    setup.select_paper(paper_from_index(paper_idx));
    setup.select_orientation(orientation_from_index(orientation_idx));
    setup.select_margins(margins_from_index(margins_idx));
    setup
}

fn paper_ink() -> Color {
    Color::from_rgb_u8(0x1d, 0x1d, 0x1f)
}

fn muted_ink() -> Color {
    Color::from_rgb_u8(0x52, 0x52, 0x5b)
}

fn box_border_color() -> Color {
    Color::from_rgb_u8(0xcb, 0xd5, 0xe1)
}

fn media_fill_color() -> Color {
    Color::from_rgb_u8(0xf1, 0xf5, 0xf9)
}

fn media_border_color() -> Color {
    Color::from_rgb_u8(0x94, 0xa3, 0xb8)
}

fn parse_css_color(hex: &str) -> Option<Color> {
    let clean = hex.trim().trim_start_matches('#');
    match clean.len() {
        6 => {
            let r = u8::from_str_radix(&clean[0..2], 16).ok()?;
            let g = u8::from_str_radix(&clean[2..4], 16).ok()?;
            let b = u8::from_str_radix(&clean[4..6], 16).ok()?;
            Some(Color::from_rgb_u8(r, g, b))
        }
        8 => {
            let r = u8::from_str_radix(&clean[0..2], 16).ok()?;
            let g = u8::from_str_radix(&clean[2..4], 16).ok()?;
            let b = u8::from_str_radix(&clean[4..6], 16).ok()?;
            let a = u8::from_str_radix(&clean[6..8], 16).ok()?;
            Some(Color::from_argb_u8(a, r, g, b))
        }
        _ => None,
    }
}

fn alignment_to_int(align: Alignment) -> i32 {
    match align {
        Alignment::Center => 1,
        Alignment::Right => 2,
        _ => 0,
    }
}

fn header_footer_alignment_to_int(align: HeaderFooterAlignment) -> i32 {
    match align {
        HeaderFooterAlignment::Center => 1,
        HeaderFooterAlignment::Right => 2,
        HeaderFooterAlignment::Left => 0,
    }
}

fn doc_is_visually_empty(doc: &WriterDocument, revision_view: PdfRevisionView) -> bool {
    let blocks_empty = doc.blocks.iter().all(|block| {
        block.text.as_str().trim().is_empty()
            && !block.is_image()
            && !block.is_chart()
            && block.kind != TABLE_BLOCK_KIND
    });
    let no_hf = doc.header_footer.header_text.trim().is_empty()
        && doc.header_footer.footer_text.trim().is_empty();
    let no_revisions =
        revision_view == PdfRevisionView::Final || doc.tracked_changes.revisions().is_empty();
    blocks_empty && no_hf && doc.footnotes.is_empty() && no_revisions
}

fn build_working_document<'a>(
    doc: &'a WriterDocument,
    revision_view: PdfRevisionView,
) -> (Cow<'a, WriterDocument>, usize) {
    let original_blocks = doc.blocks.len();
    if revision_view == PdfRevisionView::Final {
        return (Cow::Borrowed(doc), original_blocks);
    }

    let mut review_doc = doc.clone();
    let view_id = review_doc.next_id();
    review_doc.push(RichBlock::new(
        view_id,
        "paragraph",
        &format!("PDF view: {}", revision_view.name()),
    ));

    let revisions = doc.tracked_changes.revisions();
    let heading_id = review_doc.next_id();
    review_doc.push(RichBlock::new(
        heading_id,
        "heading2",
        &format!("Unresolved tracked changes ({})", revisions.len()),
    ));
    if revisions.is_empty() {
        let empty_id = review_doc.next_id();
        review_doc.push(RichBlock::new(
            empty_id,
            "paragraph",
            "No unresolved tracked changes.",
        ));
    } else {
        for rev in revisions {
            let kind = match rev.kind {
                RevisionKind::Insertion => "Insertion",
                RevisionKind::Deletion => "Deletion",
                RevisionKind::Formatting => "Formatting",
            };
            let title_id = review_doc.next_id();
            review_doc.push(RichBlock::new(
                title_id,
                "heading3",
                &format!("Change #{} — {} — {}", rev.id, kind, rev.author),
            ));
            let snippet = rev.payload.text.replace('\n', " ");
            let summary = if snippet.trim().is_empty() {
                format!(
                    "Block {}, offsets {}..{}",
                    rev.anchor.block_id, rev.anchor.start, rev.anchor.end
                )
            } else {
                format!(
                    "Block {}, offsets {}..{}: \"{}\"",
                    rev.anchor.block_id,
                    rev.anchor.start,
                    rev.anchor.end,
                    snippet.chars().take(80).collect::<String>()
                )
            };
            let body_id = review_doc.next_id();
            review_doc.push(RichBlock::new(body_id, "paragraph", &summary));
        }
    }
    (Cow::Owned(review_doc), original_blocks)
}

fn revision_overlap_style(
    revisions: &[RevisionRecord],
    block_id: u64,
    start: usize,
    end: usize,
) -> Option<(Color, Color)> {
    for rev in revisions {
        if rev.anchor.block_id == block_id && rev.anchor.start < end && rev.anchor.end > start {
            return Some(match rev.kind {
                RevisionKind::Insertion => (
                    Color::from_rgb_u8(0xdc, 0xfc, 0xe7),
                    Color::from_rgb_u8(0x15, 0x80, 0x3d),
                ),
                RevisionKind::Deletion => (
                    Color::from_rgb_u8(0xfe, 0xe2, 0xe2),
                    Color::from_rgb_u8(0xb9, 0x1c, 0x1c),
                ),
                RevisionKind::Formatting => (
                    Color::from_rgb_u8(0xdb, 0xea, 0xfe),
                    Color::from_rgb_u8(0x1d, 0x4e, 0xd8),
                ),
            });
        }
    }
    None
}

fn project_header_footer(
    doc: &WriterDocument,
    style: &PageStyle,
    page_1based: usize,
    total_pages: usize,
    lines: &mut Vec<PrintPreviewTextLine>,
) {
    if doc.header_footer.validate_for_page_style(style).is_err() {
        return;
    }
    let slots = [
        (
            doc.header_footer.format_header(page_1based, total_pages),
            HeaderFooterLineKind::Header,
        ),
        (
            doc.header_footer.format_footer(page_1based, total_pages),
            HeaderFooterLineKind::Footer,
        ),
    ];
    for (maybe_text, kind) in slots {
        let Some(text) = maybe_text else {
            continue;
        };
        if text.is_empty() {
            continue;
        }
        if let Ok(geom) = doc.header_footer.line_geometry(&text, style, kind) {
            lines.push(PrintPreviewTextLine {
                x_pt: geom.x_pt,
                y_pt: geom.top_pt,
                w_pt: geom.width_pt.max(20.0),
                h_pt: geom.height_pt.max(geom.font_size_pt * 1.3),
                text: SharedString::from(text),
                font_pt: geom.font_size_pt,
                bold: false,
                italic: false,
                color: muted_ink(),
                align: header_footer_alignment_to_int(geom.alignment),
            });
        }
    }
}

fn project_table_on_page(
    block: &RichBlock,
    block_index: usize,
    flow_lines: &[loom_writer_core::FlowLine],
    style: &PageStyle,
    page_idx: usize,
    page_origin: PageRect,
    boxes: &mut Vec<PrintPreviewBoxRect>,
    lines: &mut Vec<PrintPreviewTextLine>,
) {
    let Some(table) = block.typed_table() else {
        return;
    };
    let num_rows = table.row_count();
    let cols = table.col_count().max(1);
    if num_rows == 0 {
        return;
    }
    let row_lines: Vec<_> = flow_lines
        .iter()
        .filter(|line| line.block_index == block_index)
        .collect();
    let column_width = block.text_column_width(style).max(40.0);
    let col_widths = editor_column_widths(table, column_width);
    let mut col_starts = Vec::with_capacity(cols);
    let mut running_x = style.margin_left_pt + block.left_indent_pt(style);
    for &w in &col_widths {
        col_starts.push(running_x);
        running_x += w;
    }

    let catalog_style =
        TableStyleCatalog::get(&table.style_id).unwrap_or_else(TableStyleCatalog::default_style);
    let options = TableStyleOptions {
        header_row: table.header_row,
        total_row: table.total_row,
        last_column: table.last_column,
        banded_rows: table.banded_rows,
        banded_columns: table.banded_columns,
        first_column: table.first_column,
    };

    for r in 0..num_rows {
        let Some(row_line) = row_lines.get(r) else {
            continue;
        };
        if row_line.page != page_idx {
            continue;
        }
        let row_y = row_line.bounds.y - page_origin.y;
        let base_row_h = row_line.bounds.height.max(12.0);
        for c in 0..cols {
            let cell = table.cell(r, c);
            if cell.is_some_and(|item| item.is_covered) {
                continue;
            }
            let col_span = cell.map_or(1, |item| item.col_span.max(1));
            let row_span = cell.map_or(1, |item| item.row_span.max(1));
            let end_col = (c + col_span).min(cols);
            let cell_w: f32 = col_widths[c..end_col].iter().sum::<f32>().max(8.0);
            let cell_h: f32 = if row_span <= 1 {
                base_row_h
            } else {
                (r..(r + row_span).min(num_rows))
                    .filter_map(|span_r| {
                        row_lines
                            .get(span_r)
                            .filter(|rl| rl.page == page_idx)
                            .map(|rl| rl.bounds.height.max(12.0))
                    })
                    .sum::<f32>()
                    .max(base_row_h)
            };

            let resolved = catalog_style.resolve_cell(r, c, num_rows, cols, &options, cell);
            let fill_color = resolved
                .background_color
                .as_deref()
                .and_then(parse_css_color);
            boxes.push(PrintPreviewBoxRect {
                x_pt: col_starts[c],
                y_pt: row_y,
                w_pt: cell_w,
                h_pt: cell_h,
                fill_color: fill_color.unwrap_or_else(|| Color::from_argb_u8(0, 0, 0, 0)),
                border_color: box_border_color(),
                has_fill: fill_color.is_some(),
                has_border: true,
            });

            let text = table.cell_text(r, c).unwrap_or_default();
            if !text.trim().is_empty() {
                lines.push(PrintPreviewTextLine {
                    x_pt: col_starts[c] + 4.0,
                    y_pt: row_y + 2.0,
                    w_pt: (cell_w - 8.0).max(4.0),
                    h_pt: (cell_h - 4.0).max(10.0),
                    text: SharedString::from(text),
                    font_pt: TABLE_CELL_BASE_FONT_SIZE_PT.min(style.body_font_size_pt),
                    bold: table.header_row && r == 0,
                    italic: false,
                    color: paper_ink(),
                    align: 0,
                });
            }
        }
    }
}

fn project_footnotes_on_page(
    doc: &WriterDocument,
    style: &PageStyle,
    flow: &loom_writer_core::DocumentFlow,
    page_idx: usize,
    page_origin: PageRect,
    boxes: &mut Vec<PrintPreviewBoxRect>,
    lines: &mut Vec<PrintPreviewTextLine>,
) {
    let page_notes: Vec<_> = flow
        .footnote_lines()
        .iter()
        .filter(|note| note.page == page_idx)
        .collect();
    if page_notes.is_empty() {
        return;
    }
    let min_y = page_notes
        .iter()
        .map(|n| n.text_bounds.y.min(n.number_bounds.y) - page_origin.y)
        .fold(f32::INFINITY, f32::min);
    if min_y.is_finite() {
        let usable_w = (style.width_pt - style.margin_left_pt - style.margin_right_pt).max(72.0);
        boxes.push(PrintPreviewBoxRect {
            x_pt: style.margin_left_pt,
            y_pt: (min_y - 5.0).max(style.margin_top_pt),
            w_pt: (usable_w * 0.32).max(72.0),
            h_pt: 0.75,
            fill_color: media_border_color(),
            border_color: media_border_color(),
            has_fill: true,
            has_border: false,
        });
    }

    for placement in page_notes {
        if placement.show_number {
            lines.push(PrintPreviewTextLine {
                x_pt: placement.number_bounds.x - page_origin.x,
                y_pt: placement.number_bounds.y - page_origin.y,
                w_pt: placement.number_bounds.width.max(12.0),
                h_pt: placement.number_bounds.height.max(10.0),
                text: SharedString::from(format!("{}.", placement.number)),
                font_pt: placement.font_size_pt.max(7.5),
                bold: true,
                italic: false,
                color: muted_ink(),
                align: 0,
            });
        }
        let Some(entry) = doc.footnotes.iter().find(|f| f.id == placement.note_id) else {
            continue;
        };
        let slice = entry
            .body
            .get(placement.body_range.clone())
            .unwrap_or(entry.body.as_str());
        let text = if placement.continued && !placement.show_number {
            format!("(cont.) {slice}")
        } else {
            slice.to_string()
        };
        lines.push(PrintPreviewTextLine {
            x_pt: placement.text_bounds.x - page_origin.x,
            y_pt: placement.text_bounds.y - page_origin.y,
            w_pt: placement.text_bounds.width.max(24.0),
            h_pt: placement.text_bounds.height.max(10.0),
            text: SharedString::from(text),
            font_pt: placement.font_size_pt.max(7.5),
            bold: false,
            italic: false,
            color: muted_ink(),
            align: 0,
        });
    }
}

fn push_placeholder_box(
    local_x: f32,
    local_y: f32,
    line_width: f32,
    line_height: f32,
    col_w: f32,
    style: &PageStyle,
    label: String,
    boxes: &mut Vec<PrintPreviewBoxRect>,
    lines: &mut Vec<PrintPreviewTextLine>,
) {
    let rect_w = line_width.max(48.0).min(col_w);
    let rect_h = line_height.max(24.0);
    boxes.push(PrintPreviewBoxRect {
        x_pt: local_x,
        y_pt: local_y,
        w_pt: rect_w,
        h_pt: rect_h,
        fill_color: media_fill_color(),
        border_color: media_border_color(),
        has_fill: true,
        has_border: true,
    });
    lines.push(PrintPreviewTextLine {
        x_pt: local_x + 4.0,
        y_pt: local_y,
        w_pt: (rect_w - 8.0).max(16.0),
        h_pt: rect_h,
        text: SharedString::from(label),
        font_pt: style.body_font_size_pt.min(10.0),
        bold: false,
        italic: true,
        color: muted_ink(),
        align: 1,
    });
}

fn project_preview_snapshot(
    doc: &WriterDocument,
    style: &PageStyle,
    revision_view: PdfRevisionView,
    requested_page: i32,
) -> Result<PreviewSnapshot, String> {
    doc.header_footer
        .validate_for_page_style(style)
        .map_err(|err| err.to_string())?;
    let (working_doc, original_block_count) = build_working_document(doc, revision_view);
    let pages = working_doc.paginate(style)?;
    let viewport = PageViewport {
        width: style.width_pt,
        height: style.height_pt,
        zoom: 1.0,
        scroll_x: 0.0,
        scroll_y: 0.0,
    };
    let flow = working_doc.flow(style, viewport)?;
    let total_pages = pages.len().max(flow.page_count()).max(1);
    let total_i32 = i32::try_from(total_pages).unwrap_or(i32::MAX);
    let current_page = requested_page.clamp(1, total_i32);
    let page_idx = (current_page - 1) as usize;
    let page_origin = flow
        .page_bounds()
        .get(page_idx)
        .copied()
        .unwrap_or(PageRect {
            x: 0.0,
            y: 0.0,
            width: style.width_pt,
            height: style.height_pt,
        });

    let mut lines = Vec::new();
    let mut boxes = Vec::new();
    project_header_footer(
        &working_doc,
        style,
        current_page as usize,
        total_pages,
        &mut lines,
    );

    let list_numbers = working_doc.list_item_numbers();
    let mut seen_tables = HashSet::new();
    let revisions = doc.tracked_changes.revisions();

    for line in flow.lines() {
        let Some(block) = working_doc.blocks.get(line.block_index) else {
            continue;
        };
        if block.kind == TABLE_BLOCK_KIND {
            if line.page == page_idx && seen_tables.insert(line.block_index) {
                project_table_on_page(
                    block,
                    line.block_index,
                    flow.lines(),
                    style,
                    page_idx,
                    page_origin,
                    &mut boxes,
                    &mut lines,
                );
            }
            continue;
        }
        if line.page != page_idx {
            continue;
        }

        let local_x = line.bounds.x - page_origin.x;
        let local_y = line.bounds.y - page_origin.y;
        let col_w = block.text_column_width(style).max(24.0);
        let box_w = line.bounds.width.max(col_w);

        if block.is_image() {
            let label = block
                .image
                .as_ref()
                .map(|img| format!("[Picture: {}]", img.name))
                .unwrap_or_else(|| "[Picture]".to_string());
            push_placeholder_box(
                local_x,
                local_y,
                line.bounds.width,
                line.bounds.height,
                col_w,
                style,
                label,
                &mut boxes,
                &mut lines,
            );
            continue;
        }

        if block.is_chart() {
            push_placeholder_box(
                local_x,
                local_y,
                line.bounds.width,
                line.bounds.height,
                col_w,
                style,
                "[Embedded Chart]".to_string(),
                &mut boxes,
                &mut lines,
            );
            continue;
        }

        if line.start == 0 {
            let marker =
                block.list_marker(list_numbers.get(line.block_index).copied().unwrap_or(0));
            if !marker.is_empty() {
                let marker_x = style.margin_left_pt + block.left_indent_pt(style);
                let marker_w = (local_x - marker_x).max(14.0);
                lines.push(PrintPreviewTextLine {
                    x_pt: marker_x,
                    y_pt: local_y,
                    w_pt: marker_w,
                    h_pt: line.bounds.height.max(style.body_font_size_pt * 1.25),
                    text: SharedString::from(marker),
                    font_pt: style.font_size_for_kind(&block.kind),
                    bold: false,
                    italic: false,
                    color: paper_ink(),
                    align: 0,
                });
            }
        }

        let mut text_color = if line.block_index >= original_block_count {
            muted_ink()
        } else {
            paper_ink()
        };
        if revision_view == PdfRevisionView::Review && line.block_index < original_block_count {
            if let Some((fill, accent)) =
                revision_overlap_style(revisions, block.id, line.start, line.end)
            {
                boxes.push(PrintPreviewBoxRect {
                    x_pt: local_x - 2.0,
                    y_pt: local_y,
                    w_pt: box_w + 4.0,
                    h_pt: line.bounds.height.max(12.0),
                    fill_color: fill,
                    border_color: accent,
                    has_fill: true,
                    has_border: true,
                });
                text_color = accent;
            }
        }

        let segment = block.text.as_str().get(line.start..line.end).unwrap_or("");
        if segment.is_empty() {
            continue;
        }
        let mut font_pt = style.font_size_for_kind(&block.kind);
        let mut bold = block.kind.starts_with("heading");
        let mut italic = false;
        for run in &block.runs {
            if run.start < line.end && run.end > line.start {
                if run.style.font_size > 0.0 {
                    font_pt = run.style.font_size;
                }
                if matches!(run.style.weight, FontWeight::Semibold | FontWeight::Bold) {
                    bold = true;
                }
                if run.style.italic {
                    italic = true;
                }
                if let Some(parsed) = run.style.color.as_deref().and_then(parse_css_color) {
                    text_color = parsed;
                }
            }
        }

        let resolved_align = block
            .style
            .alignment
            .resolve_with_direction(block.text.as_str(), block.style.direction);
        lines.push(PrintPreviewTextLine {
            x_pt: local_x,
            y_pt: local_y,
            w_pt: box_w,
            h_pt: line.bounds.height.max(font_pt * 1.25),
            text: SharedString::from(segment),
            font_pt,
            bold,
            italic,
            color: text_color,
            align: alignment_to_int(resolved_align),
        });
    }

    project_footnotes_on_page(
        &working_doc,
        style,
        &flow,
        page_idx,
        page_origin,
        &mut boxes,
        &mut lines,
    );

    Ok(PreviewSnapshot {
        total_pages: total_i32,
        current_page,
        is_empty: doc_is_visually_empty(doc, revision_view),
        lines,
        boxes,
    })
}

/// Refreshes the Print Preview geometry and page content using the currently selected setup controls.
pub(crate) fn refresh_preview(app: &WriterApp, state: &GuiState) {
    let preview = app.global::<PrintPreviewState>();
    let doc = state.current.borrow();
    let setup = build_preview_page_setup(
        &doc.page,
        preview.get_paper_size(),
        preview.get_orientation(),
        preview.get_margin_preset(),
    );
    let style = setup.page_style();
    preview.set_page_width_pt(style.width_pt);
    preview.set_page_height_pt(style.height_pt);
    preview.set_margin_top_pt(style.margin_top_pt);
    preview.set_margin_bottom_pt(style.margin_bottom_pt);
    preview.set_margin_left_pt(style.margin_left_pt);
    preview.set_margin_right_pt(style.margin_right_pt);
    preview.set_doc_title(SharedString::from(doc.title.as_str()));

    let revision_view = revision_view_from_index(preview.get_revision_view());
    match project_preview_snapshot(&doc, &style, revision_view, preview.get_current_page()) {
        Ok(snapshot) => {
            preview.set_total_pages(snapshot.total_pages);
            preview.set_current_page(snapshot.current_page);
            preview.set_is_empty(snapshot.is_empty);
            preview.set_lines(ModelRc::from(Rc::new(VecModel::from(snapshot.lines))));
            preview.set_boxes(ModelRc::from(Rc::new(VecModel::from(snapshot.boxes))));
            if setup == doc.page {
                preview.set_status_message("".into());
            } else {
                preview.set_status_message(
                    "Previewing unsaved page setup — click Apply Setup to save to document.".into(),
                );
            }
        }
        Err(error) => {
            preview.set_total_pages(1);
            preview.set_current_page(1);
            preview.set_is_empty(true);
            preview.set_lines(ModelRc::from(Rc::new(
                VecModel::<PrintPreviewTextLine>::default(),
            )));
            preview.set_boxes(ModelRc::from(Rc::new(
                VecModel::<PrintPreviewBoxRect>::default(),
            )));
            preview.set_status_message(SharedString::from(format!(
                "Cannot paginate setup: {error}"
            )));
        }
    }
}

/// Opens the interactive Print Preview & Page Setup modal synced to the active document.
pub(crate) fn open(app: &WriterApp, state: &GuiState) {
    let preview = app.global::<PrintPreviewState>();
    {
        let doc = state.current.borrow();
        preview.set_paper_size(paper_to_index(doc.page.paper));
        preview.set_orientation(orientation_to_index(doc.page.orientation));
        preview.set_margin_preset(margins_to_index(doc.page.margins));
    }
    preview.set_revision_view(0);
    preview.set_current_page(1);
    preview.set_status_message("".into());
    refresh_preview(app, state);
    preview.set_open(true);
    app.set_status_left("Opened Print Preview".into());
}

/// Applies the selected Paper Size, Orientation, and Margins back to the document as one undo step.
pub(crate) fn apply_page_setup(
    app: &WriterApp,
    state: &GuiState,
    menu_service: Option<&NativeMenuBar>,
) -> bool {
    let preview = app.global::<PrintPreviewState>();
    let mut next = state.current.borrow().clone();
    let candidate_page = build_preview_page_setup(
        &next.page,
        preview.get_paper_size(),
        preview.get_orientation(),
        preview.get_margin_preset(),
    );
    if next.page == candidate_page {
        preview.set_status_message("Page setup already matches the document.".into());
        return false;
    }
    let style = candidate_page.page_style();
    if let Err(error) = candidate_page
        .validate()
        .and_then(|()| {
            next.header_footer
                .validate_for_page_style(&style)
                .map_err(str::to_string)
        })
        .and_then(|()| next.paginate(&style).map(|_| ()))
    {
        preview.set_status_message(SharedString::from(format!("Setup rejected: {error}")));
        app.set_status_left(SharedString::from(format!("Page setup error: {error}")));
        return false;
    }

    next.page = candidate_page;
    let applied = crate::apply_with_history(app, state, next, HistoryKind::DocumentAction);
    if applied {
        if let Some(menu) = menu_service {
            crate::sync_menu_state(menu, app, state);
        }
        refresh_preview(app, state);
        preview.set_status_message("Applied page setup to document.".into());
        app.set_status_left("Applied page setup from Print Preview".into());
    }
    applied
}

/// Exports the previewed document configuration to PDF using the shared Writer save flow.
pub(crate) fn export_pdf(app: &WriterApp, state: &GuiState) -> Result<Option<PathBuf>, String> {
    let preview = app.global::<PrintPreviewState>();
    let view = revision_view_from_index(preview.get_revision_view());
    let export_doc = {
        let mut doc = state.current.borrow().clone();
        doc.page = build_preview_page_setup(
            &doc.page,
            preview.get_paper_size(),
            preview.get_orientation(),
            preview.get_margin_preset(),
        );
        doc
    };

    match state
        .dialogs
        .save_file(&crate::writer_export_request(state))
    {
        Ok(Some(path)) => {
            crate::export_pdf_file(&path, &export_doc, view)?;
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("document.pdf");
            let msg = format!("Exported {name} ({})", view.name());
            preview.set_status_message(SharedString::from(msg.as_str()));
            app.set_status_left(SharedString::from(msg));
            Ok(Some(path))
        }
        Ok(None) => {
            preview.set_status_message("PDF export cancelled.".into());
            app.set_status_left("Export cancelled".into());
            Ok(None)
        }
        Err(error) => {
            let msg = format!("Export dialog failed: {error}");
            preview.set_status_message(SharedString::from(msg.as_str()));
            app.set_status_left(SharedString::from(msg.clone()));
            Err(msg)
        }
    }
}

/// Connects Print Preview & Page Setup modal callbacks to Writer state and document actions.
pub(crate) fn wire(
    app: &WriterApp,
    state: &Rc<GuiState>,
    menu_service: &Option<Arc<NativeMenuBar>>,
) {
    let preview = app.global::<PrintPreviewState>();
    {
        let app_ref = app.as_weak();
        let state = state.clone();
        preview.on_open_preview(move || {
            let Some(app) = app_ref.upgrade() else {
                return;
            };
            open(&app, &state);
        });
    }
    {
        let app_ref = app.as_weak();
        preview.on_close_preview(move || {
            let Some(app) = app_ref.upgrade() else {
                return;
            };
            app.global::<PrintPreviewState>().set_open(false);
            app.invoke_focus_page();
        });
    }
    {
        let app_ref = app.as_weak();
        let state = state.clone();
        preview.on_settings_changed(move || {
            let Some(app) = app_ref.upgrade() else {
                return;
            };
            refresh_preview(&app, &state);
        });
    }
    {
        let app_ref = app.as_weak();
        let state = state.clone();
        let menu_service = menu_service.clone();
        preview.on_apply_page_setup(move || {
            let Some(app) = app_ref.upgrade() else {
                return;
            };
            let _ = apply_page_setup(&app, &state, menu_service.as_deref());
        });
    }
    {
        let app_ref = app.as_weak();
        let state = state.clone();
        preview.on_prev_page(move || {
            let Some(app) = app_ref.upgrade() else {
                return;
            };
            let preview = app.global::<PrintPreviewState>();
            let cur = preview.get_current_page();
            if cur > 1 {
                preview.set_current_page(cur - 1);
                refresh_preview(&app, &state);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        let state = state.clone();
        preview.on_next_page(move || {
            let Some(app) = app_ref.upgrade() else {
                return;
            };
            let preview = app.global::<PrintPreviewState>();
            let cur = preview.get_current_page();
            let total = preview.get_total_pages();
            if cur < total {
                preview.set_current_page(cur + 1);
                refresh_preview(&app, &state);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        let state = state.clone();
        preview.on_export_pdf(move || {
            let Some(app) = app_ref.upgrade() else {
                return;
            };
            let _ = export_pdf(&app, &state);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions_tests::{test_state, text_document};
    use crate::wire_writer_shared_callbacks;
    use loom_desktop::ScriptedFileDialogs;
    use slint::Model;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_pdf_path(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!(
            "loom-writer-print-preview-{label}-{}-{nanos}.pdf",
            std::process::id()
        ))
    }

    #[test]
    fn preview_updates_dimensions_for_paper_orientation_and_margins() {
        let dialogs: Rc<dyn crate::FileDialogService> = Rc::new(ScriptedFileDialogs::new([], []));
        let (app, state) = test_state(text_document("Hello print preview"), dialogs);
        wire(&app, &state, &None);
        open(&app, &state);

        let preview = app.global::<PrintPreviewState>();
        assert!(preview.get_open());
        assert_eq!(preview.get_page_width_pt(), 595.0);
        assert_eq!(preview.get_page_height_pt(), 842.0);
        assert!(!preview.get_is_empty());

        preview.set_paper_size(1);
        preview.set_orientation(1);
        preview.set_margin_preset(1);
        preview.invoke_settings_changed();
        assert_eq!(preview.get_page_width_pt(), 792.0);
        assert_eq!(preview.get_page_height_pt(), 612.0);
        assert_eq!(preview.get_margin_left_pt(), 36.0);
    }

    #[test]
    fn preview_paginates_multi_page_documents_with_headers_footers_and_tables() {
        let mut doc = WriterDocument::new("multi", "Multi-Page Report");
        doc.header_footer.header_text = "Report Header".into();
        doc.header_footer.footer_text = "Page {page} of {total}".into();
        for i in 0..85 {
            let id = doc.next_id();
            doc.push(RichBlock::new(
                id,
                "paragraph",
                &format!("Paragraph {i}: Detailed project narrative for pagination verification."),
            ));
        }
        doc.insert_table_block(0, 2, 2).expect("insert table");

        let dialogs: Rc<dyn crate::FileDialogService> = Rc::new(ScriptedFileDialogs::new([], []));
        let (app, state) = test_state(doc, dialogs);
        wire(&app, &state, &None);
        open(&app, &state);

        let preview = app.global::<PrintPreviewState>();
        assert!(preview.get_total_pages() >= 2);
        assert_eq!(preview.get_current_page(), 1);
        assert!(preview.get_boxes().row_count() >= 4);

        let lines_p1: Vec<String> = (0..preview.get_lines().row_count())
            .filter_map(|i| preview.get_lines().row_data(i).map(|l| l.text.to_string()))
            .collect();
        assert!(lines_p1.iter().any(|t| t == "Report Header"));
        assert!(lines_p1.iter().any(|t| t.starts_with("Page 1 of ")));

        preview.invoke_next_page();
        assert_eq!(preview.get_current_page(), 2);
        preview.invoke_prev_page();
        assert_eq!(preview.get_current_page(), 1);
    }

    #[test]
    fn apply_page_setup_mutates_document_in_one_undo_step() {
        let dialogs: Rc<dyn crate::FileDialogService> = Rc::new(ScriptedFileDialogs::new([], []));
        let (app, state) = test_state(text_document("Setup target"), dialogs);
        wire_writer_shared_callbacks(&app, &state, None);
        wire(&app, &state, &None);
        open(&app, &state);

        let preview = app.global::<PrintPreviewState>();
        preview.set_paper_size(1);
        preview.set_orientation(1);
        preview.set_margin_preset(2);
        preview.invoke_apply_page_setup();

        assert_eq!(state.current.borrow().page.paper, PaperSize::Letter);
        assert_eq!(
            state.current.borrow().page.orientation,
            PageOrientation::Landscape
        );
        assert_eq!(state.current.borrow().page.margins, PageMarginsPreset::Wide);
        assert_eq!(state.history.borrow().undo_len(), 1);

        // Unchanged reapply does not record a redundant undo entry.
        assert!(!apply_page_setup(&app, &state, None));
        assert_eq!(state.history.borrow().undo_len(), 1);

        // Undoing reverts all three page setup dimensions together.
        app.invoke_undo_action();
        assert_eq!(state.current.borrow().page.paper, PaperSize::A4);
        assert_eq!(
            state.current.borrow().page.orientation,
            PageOrientation::Portrait
        );
        assert_eq!(
            state.current.borrow().page.margins,
            PageMarginsPreset::Normal
        );
    }

    #[test]
    fn revision_view_switch_and_export_pdf_produce_valid_pdf_bytes() {
        let mut doc = WriterDocument::new("rev-doc", "Tracked Report");
        let b1 = doc.next_id();
        doc.push(RichBlock::new(
            b1,
            "paragraph",
            "Approved summary paragraph.",
        ));
        doc.insert_footnote(b1, 8, "Footnote text for preview.")
            .expect("insert footnote");

        let out_final = unique_pdf_path("final");
        let out_review = unique_pdf_path("review");
        let dialogs: Rc<dyn crate::FileDialogService> = Rc::new(ScriptedFileDialogs::new(
            [],
            [Some(out_final.clone()), Some(out_review.clone())],
        ));
        let (app, state) = test_state(doc, dialogs);
        wire(&app, &state, &None);
        open(&app, &state);

        let preview = app.global::<PrintPreviewState>();
        let exported_final = export_pdf(&app, &state).expect("final pdf export");
        assert_eq!(exported_final.as_ref(), Some(&out_final));
        let final_bytes = std::fs::read(&out_final).expect("read final pdf");
        assert!(final_bytes.starts_with(b"%PDF-"));

        preview.set_revision_view(1);
        preview.invoke_settings_changed();
        let lines_review: Vec<String> = (0..preview.get_lines().row_count())
            .filter_map(|i| preview.get_lines().row_data(i).map(|l| l.text.to_string()))
            .collect();
        assert!(lines_review.iter().any(|t| t.contains("PDF view: Review")));

        let exported_review = export_pdf(&app, &state).expect("review pdf export");
        assert_eq!(exported_review.as_ref(), Some(&out_review));
        let review_bytes = std::fs::read(&out_review).expect("read review pdf");
        assert!(review_bytes.starts_with(b"%PDF-"));

        let _ = std::fs::remove_file(&out_final);
        let _ = std::fs::remove_file(&out_review);
    }
}
