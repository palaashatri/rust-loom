//! Document export writers (DOCX, PDF).
//!
//! Extracted from the crate root to keep `lib.rs` within its registered
//! byte ceiling. Public names are re-exported from the crate root so the
//! public API surface is unchanged.

use std::rc::Rc;

use crate::export_flow;
use crate::export_table::{self, TableLayout};
use crate::text_metrics::{is_bold_weight, pdf_text_width, InterShaper};
use crate::{PageStyle, WriterDocument};

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

/// One laid-out line: an optional list marker drawn at `marker_x` in the room
/// left of the text, then `text` at `x`. `text` is the block's bytes starting at
/// `text_start` so character runs can be matched.
struct StyledLine<'a> {
    x: f32,
    marker_x: f32,
    y: f32,
    marker: &'a str,
    text: &'a str,
    text_start: usize,
    runs: &'a [loom_text::StyleRun],
}

/// Draw a line as consecutive segments, one per change of inline style, so
/// bold, italic and underline survive the export instead of being flattened
/// to the block's face.
fn draw_styled_line(
    pdf: &mut loom_pdf::PdfDocument,
    page: loom_pdf::PageIndex,
    line: &StyledLine<'_>,
    base: &loom_pdf::TextStyle,
) {
    use loom_pdf::PathStyle;

    if !line.marker.is_empty() {
        pdf.draw_text(page, line.marker_x, line.y, line.marker, base);
    }
    let mut x = line.x;
    let end = line.text_start + line.text.len();
    let mut cuts = vec![0, line.text.len()];
    for run in line.runs {
        for edge in [run.start, run.end] {
            if (line.text_start..=end).contains(&edge)
                && line.text.is_char_boundary(edge - line.text_start)
            {
                cuts.push(edge - line.text_start);
            }
        }
    }
    cuts.sort_unstable();
    cuts.dedup();
    // One piece per stretch of one style. Adjacent runs that draw alike are
    // one piece, because the editor shapes (and so kerns) them as one.
    let mut pieces: Vec<(usize, usize, loom_pdf::TextStyle, bool, bool)> = Vec::new();
    for pair in cuts.windows(2) {
        let (from, to) = (pair[0], pair[1]);
        let position = line.text_start + from;
        let run = line
            .runs
            .iter()
            .find(|run| run.start <= position && position < run.end);
        let mut style = base.clone();
        let (mut underline, mut strikethrough) = (false, false);
        if let Some(run) = run {
            style.bold |= is_bold_weight(run.style.weight);
            style.italic |= run.style.italic;
            style.size_pt = crate::run_size(&run.style, base.size_pt);
            underline = run.style.underline;
            strikethrough = run.style.strikethrough;
        }
        match pieces.last_mut() {
            Some(last)
                if last.1 == from
                    && last.2.bold == style.bold
                    && last.2.italic == style.italic
                    && last.2.size_pt == style.size_pt
                    && last.3 == underline
                    && last.4 == strikethrough =>
            {
                last.1 = to;
            }
            _ => pieces.push((from, to, style, underline, strikethrough)),
        }
    }
    for (from, to, style, underline, _) in pieces {
        let piece = &line.text[from..to];
        pdf.draw_text(page, x, line.y, piece, &style);
        let width = pdf_text_width(piece, &style);
        if underline && !piece.trim().is_empty() {
            let rule_y = line.y - style.size_pt * 0.12;
            pdf.draw_line(
                page,
                x,
                rule_y,
                x + width,
                rule_y,
                PathStyle::stroked(style.fill_rgb, (style.size_pt / 24.0).max(0.5)),
            );
        }
        x += width;
    }
}

/// How far a line of `width` moves right inside the text `column` for its
/// alignment: centred and right-aligned lines move, the others stay at the margin.
fn align_shift(alignment: loom_text::Alignment, column: f32, width: f32) -> f32 {
    match alignment {
        loom_text::Alignment::Center => (column - width) / 2.0,
        loom_text::Alignment::Right => column - width,
        _ => 0.0,
    }
    .max(0.0)
}

/// The text of one PDF line: a byte range of its block's text, or one row of a
/// table block.
enum LineContent {
    Text {
        start: usize,
        end: usize,
        marker: String,
        style: loom_pdf::TextStyle,
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
) -> Vec<PdfLine> {
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
            let line_height = body.size_pt * page_style.line_height;
            let table = block
                .typed_table()
                .cloned()
                .unwrap_or_else(|| loom_objects::Table::new(1, 1));
            let layout = Rc::new(TableLayout::new(&table, body, column, line_height));
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
        let height = crate::line_pitch(&block.runs, style.size_pt)
            * page_style.line_height
            * crate::layout::line_spacing_factor(block);
        let text_column = block.text_column_width(page_style);
        let ranges = export_flow::wrap(text, &block.runs, &style, text_column, text_column);
        for (line_index, (start, end)) in ranges.into_iter().enumerate() {
            lines.push(PdfLine {
                block: index,
                content: LineContent::Text {
                    start,
                    end,
                    marker: if line_index == 0 {
                        marker.clone()
                    } else {
                        String::new()
                    },
                    style: style.clone(),
                },
                gap: if line_index == 0 { gap } else { 0.0 },
                height,
            });
        }
    }
    lines
}

/// Render the document to a PDF. Lines are wrapped with the widths the PDF
/// draws with, so no line or appended run passes the right margin; headings
/// take the document model's sizes; pages break by line height within the
/// margins. Output is byte-for-byte deterministic for the same document.
pub fn export_pdf(doc: &WriterDocument) -> Vec<u8> {
    use loom_pdf::{PdfDocument, TextStyle};
    let page_style = doc.page.page_style();
    let mut pdf = PdfDocument::new();
    // Runs are spaced as the editor shapes them (kerning), the way lines were
    // measured and wrapped, so a line that fits on the page fits in the file.
    pdf.set_run_shaper(std::sync::Arc::new(InterShaper));
    // The editor draws body text at the page style's size, so the PDF sets it
    // at that size too and wraps at the same places.
    let body = TextStyle {
        size_pt: page_style.body_font_size_pt,
        fill_rgb: (0.15, 0.13, 0.11),
        ..Default::default()
    };
    let column =
        (page_style.width_pt - page_style.margin_left_pt - page_style.margin_right_pt).max(0.0);
    let top = page_style.height_pt - page_style.margin_top_pt;
    let mut page = pdf.add_page(page_style.width_pt, page_style.height_pt);
    let mut y = top;

    // A page break (form feed) ends its line; the next line starts a page.
    let mut break_pending = false;
    for line in pdf_lines(doc, &page_style, &body, column) {
        if std::mem::take(&mut break_pending) {
            page = pdf.add_page(page_style.width_pt, page_style.height_pt);
            y = top;
        } else {
            y -= line.gap;
        }
        if y - line.height < page_style.margin_bottom_pt && y < top {
            page = pdf.add_page(page_style.width_pt, page_style.height_pt);
            y = top;
        }
        let block = &doc.blocks[line.block];
        let indent = block.left_indent_pt(&page_style);
        let column = column - indent;
        match &line.content {
            LineContent::Text {
                start,
                end,
                marker,
                style,
            } => {
                let text = &block.text.as_str()[*start..*end];
                break_pending = block.text.as_str()[*end..].starts_with('\u{c}');
                // The text starts where the editor starts it, past a list item's
                // marker room; the marker hangs in that room.
                let text_start = block.text_start_pt(&page_style);
                let text_column = block.text_column_width(&page_style);
                let width = export_flow::line_width("", text, &block.runs, style, *start);
                let shift = align_shift(
                    block.style.alignment.resolve(block.text.as_str()),
                    text_column,
                    width,
                );
                let marker_x = page_style.margin_left_pt + text_start
                    - LIST_MARKER_GAP_PT
                    - pdf_text_width(marker, style);
                let text_line = StyledLine {
                    x: page_style.margin_left_pt + text_start + shift,
                    marker_x,
                    y,
                    marker,
                    text,
                    text_start: *start,
                    runs: &block.runs,
                };
                draw_styled_line(&mut pdf, page, &text_line, style);
            }
            LineContent::TableRow { layout, row } => {
                let shift = align_shift(
                    block.style.alignment.resolve(block.text.as_str()),
                    column,
                    layout.width,
                );
                let origin = (page_style.margin_left_pt + indent + shift, y);
                export_table::draw_row(&mut pdf, page, layout, *row, origin);
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
                if let Some(image) = block.image.as_ref().and_then(|i| pdf_image(i).ok()) {
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
        }
        y -= line.height;
    }
    pdf.serialize()
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

        let pdf_bytes = export_pdf(&document);
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

        let pdf = export_pdf(&document);
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

        let pdf = export_pdf(&document);
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

        let pdf = export_pdf(&document);
        let text = loom_pdf::inspect::readable_content(&pdf).expect("readable PDF");
        assert!(
            text.contains("/F2 ") && text.contains("Tf BT") && text.contains("(Caf\u{e9} menu)"),
            "heading is drawn in the bold face with its accent: {text}"
        );
        assert!(
            text.contains("/F1 ") && text.contains("(It\u{2019}s a plain body line.)"),
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
        let pdf = export_pdf(&document);
        assert_eq!(
            loom_pdf::inspect::page_text(&pdf).expect("page text"),
            ["\u{395}\u{3bb}\u{3bb}\u{3b7}\u{3bd}\u{3b9}\u{3ba}\u{3ac} \u{41f}\u{440}\u{438}\u{432}\u{435}\u{442}"]
        );
    }

    #[test]
    fn export_pdf_does_not_inject_title_into_body() {
        let mut document = WriterDocument::new("title-test", "Untitled");
        document.push(RichBlock::new(document.next_id(), "paragraph", "Hello"));

        let pdf_bytes = export_pdf(&document);
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

        let pdf_bytes = export_pdf(&document);
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

    /// Text x position of the line drawn as `(text) Tj`.
    fn line_x(pdf: &[u8], text: &str) -> f32 {
        let content = loom_pdf::inspect::readable_content(pdf).expect("readable PDF");
        let at = content
            .find(&format!(" Td ({text}) Tj"))
            .unwrap_or_else(|| panic!("{text:?} is drawn"));
        let before: Vec<&str> = content[..at].split_whitespace().collect();
        before[before.len() - 2].parse().expect("x coordinate")
    }

    /// Text position (x, y) of the line drawn as `(text) Tj`.
    fn text_position(pdf: &[u8], text: &str) -> (f32, f32) {
        let content = loom_pdf::inspect::readable_content(pdf).expect("readable PDF");
        let at = content
            .find(&format!(" Td ({text}) Tj"))
            .unwrap_or_else(|| panic!("{text:?} is drawn"));
        let before: Vec<&str> = content[..at].split_whitespace().collect();
        (
            before[before.len() - 2].parse().expect("x coordinate"),
            before[before.len() - 1].parse().expect("y coordinate"),
        )
    }

    #[test]
    fn export_pdf_draws_table_cells_in_aligned_columns_without_pipes() {
        let mut document = WriterDocument::new("table", "Table");
        document.push(crate::tables::markdown_table_block(
            document.next_id(),
            "| Name | Qty |\n| --- | --- |\n| Ada | 3 |\n| Grace | 12 |",
        ));
        let pdf = export_pdf(&document);
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
        let pdf = export_pdf(&document);

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

    /// One text draw read back from the content stream.
    struct Drawn {
        x: f32,
        size: f32,
        bold: bool,
        text: String,
    }

    /// Every `(text) Tj` in the page content, with its position and font.
    fn drawn_text(pdf: &[u8]) -> Vec<Drawn> {
        let content = loom_pdf::inspect::readable_content(pdf).expect("readable PDF");
        let mut out = Vec::new();
        let mut from = 0;
        while let Some(found) = content[from..].find(" Td (") {
            let at = from + found;
            let tail = &content[at.saturating_sub(120)..at];
            let words: Vec<&str> = tail.split_whitespace().collect();
            // "... /F2 12.00 Tf BT 72.00 700.00 Td (text) Tj"
            let n = words.len();
            let x: f32 = words[n - 2].parse().expect("x");
            let size: f32 = words[n - 5].parse().expect("size");
            let font = words[n - 6];
            let text_start = at + " Td (".len();
            let text_end = text_start + content[text_start..].find(") Tj").expect("end");
            out.push(Drawn {
                x,
                size,
                bold: font == "/F2" || font == "/F4",
                text: content[text_start..text_end].to_string(),
            });
            from = text_end;
        }
        out
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
        let pdf = export_pdf(&document);
        let drawn = drawn_text(&pdf);
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
                .bold,
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
        let pdf = export_pdf(&document);
        let style = document.page.page_style();
        let right = style.width_pt - style.margin_right_pt;
        let left = style.margin_left_pt;
        let drawn = drawn_text(&pdf);
        assert!(drawn.len() > 3, "the paragraph wraps onto several lines");
        for piece in &drawn {
            let width = crate::text_metrics::pdf_text_width(
                &piece.text,
                &loom_pdf::TextStyle {
                    size_pt: piece.size,
                    bold: piece.bold,
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
        let pdf = export_pdf(&document);
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
        let content = loom_pdf::inspect::readable_content(&pdf).expect("readable PDF");
        assert!(content.contains("/F1 11.00 Tf"), "{content}");
        assert!(!content.contains("12.00 Tf"), "no 12 pt text");
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
        let pdf = export_pdf(&document);
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
        let pdf = export_pdf(&document);
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

        let pdf = export_pdf(&document);
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
mod pdf_dump_tests {
    use super::export_pdf;
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
        std::fs::write(format!("{dir}/dump.pdf"), export_pdf(&document)).expect("write pdf");
    }
}
