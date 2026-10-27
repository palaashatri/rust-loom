//! Document export writers (DOCX, PDF).
//!
//! Extracted from the crate root to keep `lib.rs` within its registered
//! byte ceiling. Public names are re-exported from the crate root so the
//! public API surface is unchanged.

use std::rc::Rc;

#[path = "export_shaped.rs"]
pub(crate) mod shaped_export;

#[cfg(test)]
#[path = "export_pdf_test_support.rs"]
pub(crate) mod test_support;

use crate::export_table::{self, TableLayout};
use crate::text_metrics::pdf_text_width;
use crate::{PageStyle, RichBlock, WriterDocument, WriterError, WriterShapedVisualLine};

/// Exports a document to a `.docx` archive Word opens directly; see
/// [`crate::export_docx`] for what is carried over. Round-trips through
/// [`extract_docx_blocks`] preserving kinds and texts.
pub fn export_document_as_docx(
    doc: &WriterDocument,
) -> Result<Vec<u8>, loom_package::zip::ArchiveError> {
    crate::docx::export_docx(doc).map(|export| export.bytes)
}

/// The gap the editor leaves between a list marker and its text (4 px in
/// `writer_components.slint`): the marker's right edge sits this far before the text.
const LIST_MARKER_GAP_PT: f32 = 4.0;

/// How far a line of `width` moves right inside the text `column` for its
/// alignment: centred and right-aligned lines move, the others stay at the margin.
fn align_shift(alignment: loom_text::Alignment, column: f32, width: f32) -> f32 {
    match alignment {
        loom_text::Alignment::Center => (column - width) / 2.0,
        loom_text::Alignment::End | loom_text::Alignment::Right => column - width,
        _ => 0.0,
    }
    .max(0.0)
}

/// The text of one PDF line: a byte range of its block's text, or one row of a
/// table block.
enum LineContent {
    Text {
        marker: String,
        style: loom_pdf::TextStyle,
        shaped: Option<WriterShapedVisualLine>,
        page_break_after: bool,
    },
    TableRow {
        layout: Rc<TableLayout>,
        row: usize,
    },
    Chart {
        chart: Rc<loom_objects::chart::Chart>,
        width: f32,
        height: f32,
    },
    Picture {
        width: f32,
        height: f32,
    },
}

/// One line of the PDF in document order.
struct PdfLine {
    block: usize,
    content: LineContent,
    /// Space before the line, between blocks.
    gap: f32,
    /// Height of the line box.
    height: f32,
}

/// The style of a block's text: headings take the page style's size for their
/// level (the size the editor draws them at) and are bold, as the DOCX export
/// sets them; other blocks use the body style.
fn block_style(
    kind: &str,
    body: &loom_pdf::TextStyle,
    page_style: &PageStyle,
) -> loom_pdf::TextStyle {
    if crate::is_heading_kind(kind) {
        loom_pdf::TextStyle {
            size_pt: page_style.font_size_for_kind(kind),
            bold: true,
            ..body.clone()
        }
    } else {
        body.clone()
    }
}

/// Lays every block out as PDF lines, wrapped with the PDF's own widths.
fn pdf_lines(
    doc: &WriterDocument,
    page_style: &PageStyle,
    body: &loom_pdf::TextStyle,
    column: f32,
) -> Result<Vec<PdfLine>, WriterError> {
    let mut lines = Vec::new();
    let numbers = doc.list_item_numbers();
    let block_gap = body.size_pt * page_style.line_height;
    for (index, block) in doc.blocks.iter().enumerate() {
        let gap = if index == 0 { 0.0 } else { block_gap };
        let kind = block.kind.as_str();
        let text = block.text.as_str();
        // An indented paragraph wraps and lays out in the narrower column the
        // editor gives it.
        let column = column - block.left_indent_pt(page_style);
        if kind == crate::TABLE_BLOCK_KIND {
            // Row heights depend on the wrapped cells, so the table is laid out
            // here, where pagination needs the heights.
            let table = block
                .typed_table()
                .cloned()
                .unwrap_or_else(|| loom_objects::Table::new(1, 1));
            let layout = Rc::new(TableLayout::new(&table, column)?);
            for row in 0..layout.row_count() {
                lines.push(PdfLine {
                    block: index,
                    content: LineContent::TableRow {
                        layout: Rc::clone(&layout),
                        row,
                    },
                    gap: if row == 0 { gap } else { 0.0 },
                    height: layout.row_height(row),
                });
            }
            continue;
        }
        let usable_height =
            (page_style.height_pt - page_style.margin_top_pt - page_style.margin_bottom_pt)
                .max(1.0);
        if let Some((width, height)) = block.image_display_size(column, usable_height) {
            lines.push(PdfLine {
                block: index,
                content: LineContent::Picture { width, height },
                gap,
                height,
            });
            continue;
        }
        if kind == crate::CHART_BLOCK_KIND {
            if let Some(chart) = crate::chart_from_block(block) {
                let chart = Rc::new(chart);
                let chart_width = column.clamp(200.0, 460.0);
                let chart_height = 200.0f32;
                lines.push(PdfLine {
                    block: index,
                    content: LineContent::Chart {
                        chart,
                        width: chart_width,
                        height: chart_height,
                    },
                    gap,
                    height: chart_height,
                });
                continue;
            }
        }
        // The marker is the one the editor draws for the item's level, so the
        // PDF and the page show the same bullet or number. It hangs in the room
        // left of the text, which every line wraps within, as on the page.
        let marker = block.list_marker(numbers[index]);
        let style = block_style(kind, body, page_style);
        let base_height = crate::line_pitch(&block.runs, style.size_pt)
            * page_style.line_height
            * crate::layout::line_spacing_factor(block);
        let text_column = block.text_column_width(page_style);
        let mut shaped_lines = crate::shape_wrapped_block_lines(block, style.size_pt, text_column)
            .ok_or_else(|| {
                WriterError::Invalid(format!("PDF export could not shape text in block {index}"))
            })?;
        if text.ends_with(['\n', '\u{b}', '\u{c}'])
            && shaped_lines
                .last()
                .is_some_and(|(range, shaped)| range.is_empty() && shaped.is_none())
        {
            shaped_lines.pop();
        }
        for (line_index, (range, shaped)) in shaped_lines.into_iter().enumerate() {
            if !range.is_empty() && shaped.is_none() {
                return Err(WriterError::Invalid(format!(
                    "PDF export could not shape text in block {index}"
                )));
            }
            let shaped_height = shaped.as_ref().map_or(0.0, |line| {
                line.ascent_pt() + line.descent_pt() + line.line_gap_pt()
            });
            let height = base_height.max(shaped_height);
            let page_break_after = text
                .get(range.clone())
                .is_some_and(|line_text| line_text.ends_with('\u{c}'));
            lines.push(PdfLine {
                block: index,
                content: LineContent::Text {
                    marker: if line_index == 0 {
                        marker.clone()
                    } else {
                        String::new()
                    },
                    style: style.clone(),
                    shaped,
                    page_break_after,
                },
                gap: if line_index == 0 { gap } else { 0.0 },
                height,
            });
        }
    }
    Ok(lines)
}

/// Render the document to a PDF. Lines are wrapped with the widths the PDF
/// draws with, so no line or appended run passes the right margin; headings
/// take the document model's sizes; pages break by line height within the
/// margins. Output is byte-for-byte deterministic for the same document.
pub fn export_pdf(doc: &WriterDocument) -> Result<Vec<u8>, WriterError> {
    use loom_pdf::{PdfDocument, TextStyle};
    crate::style_json::validate_native_text_colors(doc)
        .map_err(|error| WriterError::Invalid(error.into()))?;
    let page_style = doc.page.page_style();
    let body = TextStyle {
        size_pt: page_style.body_font_size_pt,
        fill_rgb: (0.0, 0.0, 0.0),
        ..Default::default()
    };
    let column =
        (page_style.width_pt - page_style.margin_left_pt - page_style.margin_right_pt).max(0.0);
    let lines = pdf_lines(doc, &page_style, &body, column)?;
    let mut pdf = PdfDocument::new();
    let top = page_style.height_pt - page_style.margin_top_pt;
    let mut page = pdf
        .try_add_page(page_style.width_pt, page_style.height_pt)
        .map_err(|error| WriterError::Invalid(format!("PDF export: {error}")))?;
    let mut fonts = shaped_export::FontRegistry::default();
    let mut y = top;

    // A page break (form feed) ends its line; the next line starts a page.
    let mut break_pending = false;
    for line in lines {
        if std::mem::take(&mut break_pending) {
            page = pdf
                .try_add_page(page_style.width_pt, page_style.height_pt)
                .map_err(|error| WriterError::Invalid(format!("PDF export: {error}")))?;
            y = top;
        } else {
            y -= line.gap;
        }
        if y - line.height < page_style.margin_bottom_pt && y < top {
            page = pdf
                .try_add_page(page_style.width_pt, page_style.height_pt)
                .map_err(|error| WriterError::Invalid(format!("PDF export: {error}")))?;
            y = top;
        }
        let block = &doc.blocks[line.block];
        let indent = block.left_indent_pt(&page_style);
        let column = column - indent;
        match &line.content {
            LineContent::Text {
                marker,
                style,
                shaped,
                page_break_after,
            } => {
                break_pending = *page_break_after;
                let text_start = block.text_start_pt(&page_style);
                let marker_x = page_style.margin_left_pt + text_start
                    - LIST_MARKER_GAP_PT
                    - pdf_text_width(marker, style);
                let marker_shape = if marker.is_empty() {
                    None
                } else {
                    let marker_text = format!("{marker} ");
                    let marker_width = pdf_text_width(&marker_text, style) + 1.0;
                    let marker_block = RichBlock::new(0, "paragraph", &marker_text);
                    let shaped_marker = crate::shape_wrapped_block_lines(
                        &marker_block,
                        style.size_pt,
                        marker_width,
                    )
                    .and_then(|mut lines| {
                        (lines.len() == 1)
                            .then(|| lines.pop().and_then(|(_, shaped)| shaped))
                            .flatten()
                    })
                    .ok_or_else(|| {
                        WriterError::Invalid(format!(
                            "PDF export could not shape list marker in block {}",
                            line.block
                        ))
                    })?;
                    Some((marker_text, shaped_marker))
                };
                if let Some(shaped) = shaped {
                    let context = format!("block {}", line.block);
                    if let Some((marker_text, marker_line)) = marker_shape.as_ref() {
                        shaped_export::draw_line_with_prefix(
                            &mut pdf,
                            &mut fonts,
                            page,
                            &context,
                            Some((marker_text, marker_line, marker_x)),
                            block.text.as_str(),
                            y,
                            page_style.margin_left_pt + text_start,
                            shaped,
                        )?;
                    } else {
                        shaped_export::draw_line(
                            &mut pdf,
                            &mut fonts,
                            page,
                            &context,
                            block.text.as_str(),
                            y,
                            page_style.margin_left_pt + text_start,
                            shaped,
                        )?;
                    }
                } else if let Some((marker_text, marker_line)) = marker_shape.as_ref() {
                    let context = format!("block {} list marker", line.block);
                    shaped_export::draw_line(
                        &mut pdf,
                        &mut fonts,
                        page,
                        &context,
                        marker_text,
                        y,
                        marker_x,
                        marker_line,
                    )?;
                }
            }
            LineContent::TableRow { layout, row } => {
                let shift = align_shift(
                    block.style.alignment.resolve(block.text.as_str()),
                    column,
                    layout.width,
                );
                let origin = (page_style.margin_left_pt + indent + shift, y);
                export_table::draw_row(&mut pdf, &mut fonts, page, layout, *row, origin)?;
            }
            LineContent::Chart {
                chart,
                width,
                height,
            } => {
                let shift = align_shift(
                    block.style.alignment.resolve(block.text.as_str()),
                    column,
                    *width,
                );
                let origin = (page_style.margin_left_pt + indent + shift, y);
                draw_chart_card(&mut pdf, page, chart, origin, *width, *height);
            }
            LineContent::Picture { width, height } => {
                let shift = align_shift(
                    block.style.alignment.resolve(block.text.as_str()),
                    column,
                    *width,
                );
                let image = block.image.as_ref().ok_or_else(|| {
                    WriterError::Invalid(format!(
                        "PDF export image block {} has no image payload",
                        line.block
                    ))
                })?;
                let image = pdf_image(image).map_err(|error| {
                    WriterError::Invalid(format!(
                        "PDF export image block {} ({}): {}",
                        line.block,
                        bounded_diagnostic(&image.name, 64),
                        bounded_diagnostic(&error, 160),
                    ))
                })?;
                let id = pdf.add_image(image);
                let x = page_style.margin_left_pt + indent + shift;
                pdf.draw_image_with_transform(
                    page,
                    id,
                    (*width, *height),
                    [1.0, 0.0, 0.0, -1.0, x, y],
                );
            }
        }
        y -= line.height;
    }
    Ok(pdf.serialize())
}

fn bounded_diagnostic(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars();
    let bounded: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{bounded}…")
    } else {
        bounded
    }
}

/// A picture as a PDF image: decoded, shrunk to at most 1,600 pixels on a
/// side (the samples are stored uncompressed) and split into colour and alpha.
fn pdf_image(image: &crate::InlineImage) -> Result<loom_pdf::PdfImage, String> {
    let (width, height, rgba) = image.preview_rgba(1600)?;
    let pixels = width as usize * height as usize;
    let mut rgb = Vec::with_capacity(pixels * 3);
    let mut alpha = Vec::with_capacity(pixels);
    for pixel in rgba.chunks_exact(4) {
        rgb.extend_from_slice(&pixel[..3]);
        alpha.push(pixel[3]);
    }
    let alpha = alpha.iter().any(|a| *a != 255).then_some(alpha);
    loom_pdf::PdfImage::rgb(width, height, rgb, alpha)
}

fn draw_chart_card(
    pdf: &mut loom_pdf::PdfDocument,
    page: loom_pdf::PageIndex,
    chart: &loom_objects::chart::Chart,
    origin: (f32, f32),
    width: f32,
    height: f32,
) {
    use loom_pdf::{PathStyle, TextStyle};
    let (left, top) = origin;
    let bottom = top - height;

    // Card background & border
    pdf.draw_rect(
        page,
        left,
        bottom,
        width,
        height,
        PathStyle::filled((0.98, 0.98, 0.98)),
    );
    pdf.draw_rect(
        page,
        left,
        bottom,
        width,
        height,
        PathStyle::stroked((0.85, 0.88, 0.90), 1.0),
    );

    // Title
    if let Some(title) = &chart.title {
        let title_style = TextStyle {
            size_pt: 11.0,
            bold: true,
            fill_rgb: (0.15, 0.13, 0.11),
            ..Default::default()
        };
        pdf.draw_text(page, left + 14.0, top - 18.0, title, &title_style);
    }

    if chart.series.is_empty() || chart.categories.is_empty() {
        return;
    }
    let colors: [(f32, f32, f32); 4] = [
        (0.145, 0.388, 0.922),
        (0.063, 0.725, 0.506),
        (0.961, 0.620, 0.043),
        (0.937, 0.267, 0.267),
    ];
    let label_style = TextStyle {
        size_pt: 8.0,
        fill_rgb: (0.35, 0.35, 0.35),
        ..Default::default()
    };
    if chart.chart_type == loom_objects::chart::ChartType::Pie {
        let series = &chart.series[0];
        let cat_count = chart.categories.len().max(series.values.len());
        let values: Vec<f64> = (0..cat_count)
            .map(|ci| series.values.get(ci).copied().unwrap_or(0.0).max(0.0))
            .collect();
        let total: f64 = values.iter().sum();
        if total <= 0.0 {
            return;
        }
        let cx = left + width * 0.31;
        let cy = bottom + height * 0.45;
        let r = (width * 0.20).min(height * 0.32).max(12.0);
        let d = (r / 3.8).max(3.0);
        for ring in 1..=5usize {
            let rad = (ring as f32 - 0.3) / 5.0 * r;
            let steps = (ring * 12).max(16);
            for step in 0..steps {
                let turn = (step as f64 + 0.5) / steps as f64;
                let mut acc = 0.0;
                let mut idx = cat_count.saturating_sub(1);
                for (ci, v) in values.iter().copied().enumerate() {
                    acc += v;
                    if turn * total <= acc {
                        idx = ci;
                        break;
                    }
                }
                let theta = std::f32::consts::FRAC_PI_2 - (turn as f32) * std::f32::consts::TAU;
                let x = cx + rad * theta.cos() - d / 2.0;
                let y = cy + rad * theta.sin() - d / 2.0;
                pdf.draw_rect(
                    page,
                    x,
                    y,
                    d,
                    d,
                    PathStyle::filled(colors[idx % colors.len()]),
                );
            }
        }
        let row_step = (height * 0.62 / cat_count.max(1) as f32).min(20.0);
        for (ci, v) in values.iter().copied().enumerate() {
            let ry = top - 42.0 - ci as f32 * row_step;
            let color = colors[ci % colors.len()];
            pdf.draw_rect(
                page,
                left + width * 0.58,
                ry,
                7.0,
                7.0,
                PathStyle::filled(color),
            );
            let cat = chart.categories.get(ci).map(String::as_str).unwrap_or("");
            let pct = (v / total * 100.0).round() as i64;
            pdf.draw_text(
                page,
                left + width * 0.63,
                ry,
                &format!("{cat} ({pct}%)"),
                &label_style,
            );
        }
        return;
    }

    let norm_values = chart.normalized_series_values_from_zero();
    let cat_count = chart.categories.len();
    let series_count = chart.series.len();
    let plot_top = top - 32.0;
    let plot_bottom = bottom + 24.0;
    let plot_h = (plot_top - plot_bottom).max(20.0);
    let plot_left = left + 20.0;
    let plot_right = left + width - 20.0;
    let plot_w = (plot_right - plot_left).max(40.0);
    let cat_w = plot_w / cat_count as f32;
    let bar_slot = cat_w * 0.75;
    let bar_w = (bar_slot / series_count as f32).max(2.0);
    let is_point = matches!(
        chart.chart_type,
        loom_objects::chart::ChartType::Line
            | loom_objects::chart::ChartType::Area
            | loom_objects::chart::ChartType::Scatter
    );
    for (s_idx, s_norm) in norm_values.iter().enumerate() {
        let color = colors[s_idx % colors.len()];
        if is_point {
            let pts: Vec<(f32, f32)> = (0..cat_count)
                .map(|c_idx| {
                    let norm = s_norm.get(c_idx).copied().unwrap_or(0.0) as f32;
                    (
                        plot_left + (c_idx as f32 + 0.5) * cat_w,
                        plot_bottom + plot_h * norm,
                    )
                })
                .collect();
            if chart.chart_type != loom_objects::chart::ChartType::Scatter {
                for pair in pts.windows(2) {
                    pdf.draw_line(
                        page,
                        pair[0].0,
                        pair[0].1,
                        pair[1].0,
                        pair[1].1,
                        PathStyle::stroked(color, 1.5),
                    );
                }
            }
            for (px, py) in pts {
                pdf.draw_rect(page, px - 2.5, py - 2.5, 5.0, 5.0, PathStyle::filled(color));
            }
        } else {
            for c_idx in 0..cat_count {
                let cat_x = plot_left + c_idx as f32 * cat_w + (cat_w - bar_slot) / 2.0;
                let norm = s_norm.get(c_idx).copied().unwrap_or(0.0) as f32;
                let bar_h = (plot_h * norm).max(2.0);
                let x = cat_x + s_idx as f32 * bar_w;
                pdf.draw_rect(
                    page,
                    x,
                    plot_bottom,
                    (bar_w - 1.0).max(1.0),
                    bar_h,
                    PathStyle::filled(color),
                );
            }
        }
    }
    for (c_idx, cat) in chart.categories.iter().enumerate() {
        let cat_x = plot_left + c_idx as f32 * cat_w + (cat_w - bar_slot) / 2.0;
        pdf.draw_text(page, cat_x, bottom + 10.0, cat, &label_style);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PageSetup, RichBlock, WriterDocument};

    fn export_valid(document: &WriterDocument) -> Vec<u8> {
        export_pdf(document).expect("valid PDF fixture")
    }

    #[test]
    fn export_pdf_emits_every_paginated_fragment() {
        let mut document = WriterDocument::new("export-test", "Export test");
        for index in 0..80 {
            document.push(RichBlock::new(
                document.next_id(),
                "paragraph",
                &format!("unique-paragraph-{index:03}"),
            ));
        }

        let expected_pages = document
            .paginate(&PageSetup::default().page_style())
            .expect("valid page layout");
        assert!(expected_pages.len() > 1, "fixture must span multiple pages");

        let pdf_bytes = export_valid(&document);
        let pdf = loom_pdf::inspect::readable_content(&pdf_bytes).expect("readable PDF");
        for index in 0..80 {
            let text = format!("unique-paragraph-{index:03}");
            assert_eq!(
                pdf.matches(&text).count(),
                1,
                "missing or duplicated {text}"
            );
        }
    }

    #[test]
    fn export_pdf_keeps_paragraph_text_when_a_table_follows() {
        let introduction = "Introduction paragraph with unique words Alpha Beta Gamma.";
        let conclusion = "Conclusion paragraph with Delta Epsilon Zeta.";

        let mut paragraph_only = WriterDocument::new("table-text-loss", "Table text loss");
        paragraph_only.replace_paragraphs(introduction);
        paragraph_only
            .blocks
            .push(RichBlock::new(99, "paragraph", conclusion));
        let paragraph_text = loom_pdf::inspect::readable_content(&export_valid(&paragraph_only))
            .expect("readable paragraph-only PDF");
        assert!(
            paragraph_text.contains("Alpha"),
            "paragraph-only PDF is the readable baseline: {paragraph_text:?}"
        );

        let mut paragraph_with_table = WriterDocument::new("table-text-loss", "Table text loss");
        paragraph_with_table.replace_paragraphs(introduction);
        let table_id = paragraph_with_table
            .insert_table_block(usize::MAX, 2, 2)
            .expect("insert table after introduction");
        let mut table = paragraph_with_table
            .table_from_block(table_id)
            .expect("inserted table is typed");
        table.set(0, 0, "CellOneOne");
        table.set(0, 1, "CellOneTwo");
        table.set(1, 0, "CellTwoOne");
        table.set(1, 1, "CellTwoTwo");
        paragraph_with_table
            .blocks
            .iter_mut()
            .find(|block| block.id == table_id)
            .expect("inserted table block")
            .set_table(table.to_loom_table());
        paragraph_with_table
            .blocks
            .push(RichBlock::new(99, "paragraph", conclusion));

        let table_text = loom_pdf::inspect::readable_content(&export_valid(&paragraph_with_table))
            .expect("readable paragraph-plus-table PDF");
        assert!(
            table_text.contains("Alpha"),
            "adding a table must not drop preceding paragraph text: {table_text:?}"
        );
        assert!(
            table_text.contains("CellOneOne") && table_text.contains("Delta"),
            "the PDF should also retain table cells and the following paragraph: {table_text:?}"
        );
    }

    #[test]
    fn export_pdf_draws_a_larger_run_at_its_own_size() {
        use loom_text::{CharacterStyle, StyleRun};

        let mut document = WriterDocument::new("export-size", "Size");
        let mut block = RichBlock::new(document.next_id(), "paragraph", "Write a large word");
        block.runs = vec![StyleRun {
            start: 8,
            end: 13,
            style: CharacterStyle {
                font_size: 24.0,
                explicit_size: true,
                ..Default::default()
            },
        }];
        document.push(block);

        let pdf = export_valid(&document);
        let text = loom_pdf::inspect::readable_content(&pdf).expect("readable PDF");
        assert!(
            text.contains("24.00 Tf") && text.contains("(large)"),
            "the chosen run is drawn at 24 pt: {text}"
        );
        assert!(
            text.contains("11.00 Tf"),
            "the rest of the paragraph keeps the 11 pt body size: {text}"
        );
    }

    #[test]
    fn export_pdf_keeps_inline_bold_italic_and_underline_runs() {
        use loom_text::{CharacterStyle, FontWeight, StyleRun};

        let mut document = WriterDocument::new("export-runs", "Runs");
        let mut block = RichBlock::new(document.next_id(), "paragraph", "one two three four");
        block.runs = vec![
            StyleRun {
                start: 4,
                end: 7,
                style: CharacterStyle {
                    weight: FontWeight::Bold,
                    ..Default::default()
                },
            },
            StyleRun {
                start: 8,
                end: 13,
                style: CharacterStyle {
                    italic: true,
                    underline: true,
                    ..Default::default()
                },
            },
        ];
        document.push(block);

        let pdf = export_valid(&document);
        let text = loom_pdf::inspect::readable_content(&pdf).expect("readable PDF");
        assert!(text.contains("(one )"), "leading plain run: {text}");
        assert!(
            text.contains("/F2 ") && text.contains("(two)"),
            "bold run uses the bold face"
        );
        assert!(
            text.contains("/F3 ") && text.contains("(three)"),
            "italic run uses the oblique face"
        );
        assert!(
            text.contains("( four)"),
            "trailing plain run keeps its space"
        );
        assert_eq!(
            text.matches(" l S").count(),
            1,
            "only the underlined word draws a stroked rule"
        );
    }

    #[test]
    fn export_pdf_keeps_headings_bold_and_accents_readable() {
        let mut document = WriterDocument::new("export-fonts", "Fonts");
        document.push(RichBlock::new(
            document.next_id(),
            "heading1",
            "Caf\u{e9} menu",
        ));
        document.push(RichBlock::new(
            document.next_id(),
            "paragraph",
            "It\u{2019}s a plain body line.",
        ));

        let pdf = export_valid(&document);
        let text = loom_pdf::inspect::readable_content(&pdf).expect("readable PDF");
        let runs = test_support::drawn_text_runs(&pdf).expect("semantic text runs");
        assert!(
            runs.iter().any(|run| {
                run.text == "Caf\u{e9} menu"
                    && run.font == "/F2"
                    && run.bold == Some(true)
                    && (run.size - 24.0).abs() < 0.01
            }),
            "heading is drawn in the bold face with its accent: {text}"
        );
        assert!(
            runs.iter().any(|run| {
                run.text == "It\u{2019}s a plain body line."
                    && run.font == "/F1"
                    && run.bold == Some(false)
            }),
            "body keeps the regular face and the typographic apostrophe: {text}"
        );
        assert_eq!(
            loom_pdf::inspect::page_text(&pdf).expect("page text"),
            ["Caf\u{e9} menu\nIt\u{2019}s a plain body line."],
            "the text reads back exactly, with no '?' substitutes"
        );
        let faces: Vec<String> = loom_pdf::inspect::embedded_fonts(&pdf)
            .expect("fonts")
            .into_iter()
            .map(|font| font.base_font)
            .collect();
        assert!(
            faces.iter().any(|f| f.ends_with("+Inter-Bold"))
                && faces.iter().any(|f| f.ends_with("+Inter-Regular")),
            "headings and body embed their own Inter faces: {faces:?}"
        );
    }

    #[test]
    fn export_pdf_carries_non_latin_scripts_instead_of_question_marks() {
        let mut document = WriterDocument::new("export-scripts", "Scripts");
        document.push(RichBlock::new(
            document.next_id(),
            "paragraph",
            "\u{395}\u{3bb}\u{3bb}\u{3b7}\u{3bd}\u{3b9}\u{3ba}\u{3ac} \u{41f}\u{440}\u{438}\u{432}\u{435}\u{442}",
        ));
        let pdf = export_valid(&document);
        assert_eq!(
            loom_pdf::inspect::page_text(&pdf).expect("page text"),
            ["\u{395}\u{3bb}\u{3bb}\u{3b7}\u{3bd}\u{3b9}\u{3ba}\u{3ac} \u{41f}\u{440}\u{438}\u{432}\u{435}\u{442}"]
        );
    }

    #[test]
    fn export_pdf_does_not_inject_title_into_body() {
        let mut document = WriterDocument::new("title-test", "Untitled");
        document.push(RichBlock::new(document.next_id(), "paragraph", "Hello"));

        let pdf_bytes = export_valid(&document);
        let pdf = loom_pdf::inspect::readable_content(&pdf_bytes).expect("readable PDF");

        // The PDF should contain "Hello" exactly once
        assert_eq!(
            pdf.matches("Hello").count(),
            1,
            "body text Hello must appear exactly once in PDF"
        );

        // The PDF should NOT contain "Untitled" because it's not a body block
        assert_eq!(
            pdf.matches("Untitled").count(),
            0,
            "title 'Untitled' must not appear in PDF body text"
        );
    }

    #[test]
    fn export_pdf_multi_page_does_not_duplicate_text() {
        let mut document = WriterDocument::new("multi-test", "My Document");
        document.push(RichBlock::new(
            document.next_id(),
            "paragraph",
            "unique-text-marker-AAA",
        ));
        for i in 0..75 {
            document.push(RichBlock::new(
                document.next_id(),
                "paragraph",
                &format!("Line number {i:03}"),
            ));
        }

        let pdf_bytes = export_valid(&document);
        let pdf = loom_pdf::inspect::readable_content(&pdf_bytes).expect("readable PDF");

        // "My Document" should not appear at all (title not in body)
        assert_eq!(
            pdf.matches("My Document").count(),
            0,
            "title must not appear in PDF body"
        );

        // Marker should appear exactly once
        assert_eq!(
            pdf.matches("unique-text-marker-AAA").count(),
            1,
            "unique marker must appear exactly once"
        );

        // Spot-check that some lines appear
        assert!(pdf.contains("Line number 000"), "first line");
        assert!(pdf.contains("Line number 050"), "middle line");
        assert!(pdf.contains("Line number 074"), "last line");
    }

    /// Text x position as read from the run's text matrix.
    fn line_x(pdf: &[u8], text: &str) -> f32 {
        test_support::drawn_text_runs(pdf)
            .expect("semantic text runs")
            .into_iter()
            .find(|run| run.text == text)
            .unwrap_or_else(|| panic!("{text:?} is drawn"))
            .x
    }

    /// Text position as read from the run's text matrix.
    fn text_position(pdf: &[u8], text: &str) -> (f32, f32) {
        let run = test_support::drawn_text_runs(pdf)
            .expect("semantic text runs")
            .into_iter()
            .find(|run| run.text == text)
            .unwrap_or_else(|| panic!("{text:?} is drawn"));
        (run.x, run.y)
    }

    #[test]
    fn export_pdf_draws_table_cells_in_aligned_columns_without_pipes() {
        let mut document = WriterDocument::new("table", "Table");
        document.push(crate::tables::markdown_table_block(
            document.next_id(),
            "| Name | Qty |\n| --- | --- |\n| Ada | 3 |\n| Grace | 12 |",
        ));
        let pdf = export_valid(&document);
        let content = loom_pdf::inspect::readable_content(&pdf).expect("readable PDF");
        assert!(!content.contains('|'), "no pipe characters are printed");
        assert!(!content.contains("---"), "the separator row is not printed");

        let (name_x, name_y) = text_position(&pdf, "Name");
        let (qty_x, qty_y) = text_position(&pdf, "Qty");
        let (ada_x, ada_y) = text_position(&pdf, "Ada");
        let (three_x, three_y) = text_position(&pdf, "3");
        let (grace_x, grace_y) = text_position(&pdf, "Grace");
        let (twelve_x, twelve_y) = text_position(&pdf, "12");
        assert_eq!(name_y, qty_y, "header cells share a row");
        assert_eq!(ada_y, three_y, "the first data row shares a row");
        assert_eq!(grace_y, twelve_y, "the second data row shares a row");
        assert!(name_y > ada_y && ada_y > grace_y, "rows run down the page");
        assert!(
            (ada_x - name_x).abs() < 0.01 && (grace_x - name_x).abs() < 0.01,
            "the first column is aligned"
        );
        assert!(
            (three_x - qty_x).abs() < 0.01 && (twelve_x - qty_x).abs() < 0.01,
            "the second column is aligned"
        );
        assert!(
            qty_x > name_x + 30.0,
            "the second column starts after the first"
        );
    }

    #[test]
    fn export_pdf_honors_center_and_right_alignment() {
        use loom_text::Alignment;

        let mut document = WriterDocument::new("align", "Align");
        for (text, alignment) in [
            ("left line", Alignment::Left),
            ("centered line", Alignment::Center),
            ("right line", Alignment::Right),
        ] {
            let mut block = RichBlock::new(document.next_id(), "paragraph", text);
            block.style.alignment = alignment;
            document.push(block);
        }
        let style = document.page.page_style();
        let pdf = export_valid(&document);

        let column = style.width_pt - style.margin_left_pt - style.margin_right_pt;
        // The body is drawn at the page style's body size, which is the size
        // the editor draws it at.
        let body = loom_pdf::TextStyle {
            size_pt: style.body_font_size_pt,
            ..Default::default()
        };
        let width = |text: &str| crate::text_metrics::pdf_text_width(text, &body);
        let left = line_x(&pdf, "left line");
        let center = line_x(&pdf, "centered line");
        let right = line_x(&pdf, "right line");
        assert!(
            (left - style.margin_left_pt).abs() < 0.01,
            "left stays at the margin"
        );
        let want_center = style.margin_left_pt + (column - width("centered line")) / 2.0;
        assert!(
            (center - want_center).abs() < 3.0 && center > left + 50.0,
            "centered line starts at {center}, expected about {want_center}"
        );
        let want_right = style.margin_left_pt + column - width("right line");
        assert!(
            (right - want_right).abs() < 3.0,
            "right line starts at {right}, expected about {want_right}"
        );
    }

    #[test]
    fn export_pdf_uses_the_document_model_sizes_for_headings_and_body() {
        let mut document = WriterDocument::new("sizes", "Sizes");
        document.push(RichBlock::new(document.next_id(), "heading1", "Title"));
        document.push(RichBlock::new(
            document.next_id(),
            "paragraph",
            "Body words",
        ));
        let pdf = export_valid(&document);
        let drawn = test_support::drawn_text_runs(&pdf).expect("semantic text runs");
        let size_of = |text: &str| {
            drawn
                .iter()
                .find(|d| d.text == text)
                .unwrap_or_else(|| panic!("{text:?} is drawn"))
                .size
        };
        assert!((size_of("Title") - 24.0).abs() < 0.01, "H1 is 24 pt");
        let body = PageStyle::default().body_font_size_pt;
        assert!(
            (size_of("Body words") - body).abs() < 0.01,
            "body is the page style's {body} pt, the size the editor draws"
        );
        assert!(
            drawn
                .iter()
                .find(|d| d.text == "Title")
                .expect("title")
                .bold
                == Some(true),
            "H1 is bold"
        );
    }

    #[test]
    fn export_pdf_keeps_every_line_and_appended_bold_run_inside_the_right_margin() {
        let mut document = WriterDocument::new("wrap", "Wrap");
        let body = "lorem ipsum dolor sit amet consectetur adipiscing elit sed do eiusmod tempor "
            .repeat(4);
        let mut block =
            RichBlock::new(document.next_id(), "paragraph", &format!("{body}Bold tail"));
        // Right alignment is where a bold run appended to a line can run past
        // the margin: the line is placed from its regular-face width.
        block.style.alignment = loom_text::Alignment::Right;
        let start = body.len();
        block.runs.push(loom_text::StyleRun {
            start,
            end: start + "Bold tail".len(),
            style: loom_text::CharacterStyle {
                weight: loom_text::FontWeight::Bold,
                ..Default::default()
            },
        });
        document.push(block);
        let pdf = export_valid(&document);
        let style = document.page.page_style();
        let right = style.width_pt - style.margin_right_pt;
        let left = style.margin_left_pt;
        let drawn = test_support::drawn_text_runs(&pdf).expect("semantic text runs");
        assert!(drawn.len() > 3, "the paragraph wraps onto several lines");
        for piece in &drawn {
            let width = crate::text_metrics::pdf_text_width(
                &piece.text,
                &loom_pdf::TextStyle {
                    size_pt: piece.size,
                    bold: piece.bold.expect("Inter bold resource"),
                    ..Default::default()
                },
            );
            assert!(
                piece.x >= left - 0.05 && piece.x + width <= right + 0.05,
                "{:?} spans {}..{} outside the margins {left}..{right}",
                piece.text,
                piece.x,
                piece.x + width
            );
        }
    }

    /// ASCII sample paragraphs: a long sentence, a short line, and a word wider
    /// than a line.
    const PARAGRAPHS: [&str; 3] = [
        "Loom Writer keeps your documents on your computer. Files are saved as \
         packages that any copy of the app can read, and nothing is uploaded \
         unless you choose to export it. Headings, lists and tables export too.",
        "A short paragraph.",
        "Before the long word, then xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx\
         xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx after it.",
    ];

    #[test]
    fn the_pdf_breaks_lines_where_the_editor_does() {
        let mut document = WriterDocument::new("parity", "Parity");
        for text in PARAGRAPHS {
            document.push(RichBlock::new(document.next_id(), "paragraph", text));
        }
        let style = document.page.page_style();
        assert_eq!(
            style.body_font_size_pt, 11.0,
            "the editor draws body text at 11 pt"
        );

        // Editor: the page fragments, one per wrapped line.
        let editor: Vec<String> = document
            .paginate(&style)
            .expect("valid page layout")
            .iter()
            .flat_map(|page| page.fragments.iter())
            .map(|fragment| fragment.text.trim_end().to_string())
            .collect();
        assert!(editor.len() > 6, "the sample wraps: {editor:?}");

        // PDF: one drawn run per line, in reading order.
        let pdf = export_valid(&document);
        let exported: Vec<String> = loom_pdf::inspect::page_text(&pdf)
            .expect("page text")
            .iter()
            .flat_map(|page| {
                page.lines()
                    .map(|line| line.trim_end().to_string())
                    .collect::<Vec<_>>()
            })
            .collect();
        assert_eq!(exported, editor, "same lines, same breaks");

        // And at the size the editor draws them: every run is 11 pt.
        let runs = test_support::drawn_text_runs(&pdf).expect("semantic text runs");
        assert!(
            runs.iter()
                .all(|run| run.font == "/F1" && (run.size - 11.0).abs() < 0.01),
            "every body run uses regular Inter at 11 pt: {runs:?}"
        );
        assert!(
            runs.iter().all(|run| (run.size - 12.0).abs() >= 0.01),
            "no run uses 12 pt"
        );
    }

    #[test]
    fn semibold_runs_are_regular_in_the_pdf_as_in_the_editor() {
        use loom_text::{CharacterStyle, FontWeight, StyleRun};
        let mut document = WriterDocument::new("weights", "Weights");
        let mut block = RichBlock::new(document.next_id(), "paragraph", "one two three");
        let run = |start, end, weight| StyleRun {
            start,
            end,
            style: CharacterStyle {
                weight,
                ..Default::default()
            },
        };
        block.runs = vec![
            run(0, 3, FontWeight::Semibold),
            run(8, 13, FontWeight::Bold),
        ];
        document.push(block);
        let pdf = export_valid(&document);
        let faces: Vec<String> = loom_pdf::inspect::embedded_fonts(&pdf)
            .expect("fonts")
            .into_iter()
            .map(|font| {
                font.base_font
                    .split('+')
                    .nth(1)
                    .unwrap_or_default()
                    .to_string()
            })
            .collect();
        assert_eq!(
            faces,
            ["Inter-Regular", "Inter-Bold"],
            "semibold stays in the regular face; only bold uses the bold one"
        );
    }

    #[test]
    fn a_freshly_inserted_empty_table_exports_visible_columns() {
        let mut document = WriterDocument::new("table", "Table");
        document
            .insert_table_block(usize::MAX, 2, 3)
            .expect("insert");
        let pdf = export_valid(&document);
        let content = loom_pdf::inspect::readable_content(&pdf).expect("readable PDF");
        // Each rule is "x y m x2 y2 l S"; a vertical rule has equal x values.
        let mut vertical_x: Vec<i64> = Vec::new();
        let mut from = 0;
        while let Some(found) = content[from..].find(" l S") {
            let at = from + found;
            let words: Vec<&str> = content[..at].split_whitespace().collect();
            let n = words.len();
            // "... x1 y1 m x2 y2 l S"
            let x1: f32 = words[n - 5].parse().expect("x1");
            let x2: f32 = words[n - 2].parse().expect("x2");
            if (x1 - x2).abs() < 0.01 {
                let key = (x1 * 100.0).round() as i64;
                if !vertical_x.contains(&key) {
                    vertical_x.push(key);
                }
            }
            from = at + " l S".len();
        }
        assert!(
            vertical_x.len() >= 4,
            "three columns need four vertical rules, found {}",
            vertical_x.len()
        );
    }

    #[test]
    fn pie_and_line_charts_export_slice_percentages_and_line_strokes_in_pdf() {
        use loom_objects::chart::{Chart, ChartType};
        let mut document = WriterDocument::new("charts", "Charts");
        let mut pie = Chart::new("pie-1", ChartType::Pie);
        pie.title = Some("Budget".into());
        pie.categories = vec!["Eng".into(), "Design".into()];
        pie.add_series("Spend", vec![60.0, 40.0], None);
        document.insert_chart_block(usize::MAX, pie).expect("pie");

        let mut line = Chart::new("line-1", ChartType::Line);
        line.title = Some("Growth".into());
        line.categories = vec!["Jan".into(), "Feb".into(), "Mar".into()];
        line.add_series("Users", vec![10.0, 25.0, 40.0], None);
        document.insert_chart_block(usize::MAX, line).expect("line");

        let pdf = export_valid(&document);
        let content = loom_pdf::inspect::readable_content(&pdf).expect("readable PDF");
        assert!(
            content.contains(r"Eng \(60%\)") && content.contains(r"Design \(40%\)"),
            "pie slice percentage labels must be rendered in Writer PDF, got:\n{content}"
        );
        assert!(
            content.contains(" l S"),
            "line chart must emit stroked line segments in Writer PDF"
        );
    }
}

#[cfg(test)]
#[path = "export_justification_tests.rs"]
mod export_justification_tests;

#[cfg(test)]
#[path = "export_shaped_tests.rs"]
mod export_shaped_tests;

#[cfg(test)]
#[path = "export_image_error_tests.rs"]
mod export_image_error_tests;

#[cfg(test)]
#[path = "export_style_tests.rs"]
mod export_style_tests;

#[cfg(test)]
mod pdf_dump_tests {
    use crate::export::test_support::export_valid_pdf as export_valid;
    use crate::{RichBlock, WriterDocument};

    /// Writes the PDF of a document with a heading, a sized word and a table, so
    /// it can be rendered next to the same document in the editor.
    #[test]
    fn dump_the_pdf_when_asked() {
        let Ok(dir) = std::env::var("LOOM_PDF_DUMP") else {
            return;
        };
        let mut document = WriterDocument::new("dump", "Dump");
        let heading = RichBlock::new(document.next_id(), "heading1", "Quarterly report");
        document.push(heading);
        let mut body = RichBlock::new(
            document.next_id(),
            "paragraph",
            "A large word, a huge heading, and plain text.",
        );
        body.runs = vec![loom_text::StyleRun {
            start: 2,
            end: 7,
            style: loom_text::CharacterStyle {
                font_size: 24.0,
                explicit_size: true,
                ..Default::default()
            },
        }];
        document.push(body);
        let table_id = document.next_id();
        document.push(crate::markdown_table_block(
            table_id,
            "| Region | Sessions | Change |\n| --- | --- | --- |\n| North | 1,204 | +8.1% |\n| South | 987 | +5.4% |",
        ));
        std::fs::write(format!("{dir}/dump.pdf"), export_valid(&document)).expect("write pdf");
    }
}
