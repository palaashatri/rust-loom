//! The PDF export of a deck: one page per slide, drawn the way the slide editor
//! draws it. The page is the slide's 16:9 plane at PowerPoint's standard size, and
//! every object keeps its place. Text sits in the editor's own text box, at the
//! editor's sizes, wrapped to that box and centred the way the editor centres it.

use std::collections::BTreeMap;

use loom_pdf::{text_width_pt, PageIndex, PathStyle, PdfDocument, TextStyle};

use crate::{
    normalize_angle_degrees, pictures, ElementType, PresentationDocument, SlideElement,
    SLIDE_HEIGHT, SLIDE_WIDTH,
};

/// The page in points: the 16:9 slide at PowerPoint's standard 13.33 by 7.5 inches.
const PAGE_WIDTH: f32 = 960.0;
const PAGE_HEIGHT: f32 = 540.0;
/// Page points per scene unit.
const POINTS_PER_UNIT: f32 = PAGE_WIDTH / SLIDE_WIDTH;

/// The editor lays out in pixels of an 840 px wide slide, so one of its pixels is
/// this many scene units.
const UNITS_PER_EDITOR_PX: f32 = SLIDE_WIDTH / 840.0;
/// The editor's type scale (`Theme.tokens.typography`): display, heading, body-large.
const DISPLAY_PX: f32 = 24.0;
const HEADING_PX: f32 = 20.0;
const BODY_PX: f32 = 14.0;
/// The editor's spacing tokens (`Theme.tokens.space`): xs, sm and lg.
const SPACE_XS_PX: f32 = 2.0;
const SPACE_SM_PX: f32 = 4.0;
const SPACE_LG_PX: f32 = 12.0;
/// Lines are this many ems apart, and the baseline sits this far below a line's top.
const LINE_EM: f32 = 1.2;
const ASCENT_EM: f32 = 0.97;

/// Fill of a plain shape, and of a stat card.
const SHAPE_RGB: (f32, f32, f32) = (0.84, 0.88, 0.96);
const STAT_RGB: (f32, f32, f32) = (0.16, 0.36, 0.72);

/// Renders the deck to a PDF; see the module documentation for what matches the editor.
pub fn export_pdf(doc: &PresentationDocument) -> Vec<u8> {
    let mut pdf = PdfDocument::new();
    let mut pdf_images: BTreeMap<String, usize> = Default::default();
    let style_title = TextStyle {
        size_pt: 18.0,
        bold: true,
        ..Default::default()
    };
    for slide in &doc.slides {
        let page = pdf.add_page(PAGE_WIDTH, PAGE_HEIGHT);
        let has_title = slide
            .elements
            .iter()
            .any(|element| element.element_type == ElementType::Title);
        if !has_title {
            // Legacy slides may have no title element; keep their title at the top left.
            let title_x = 40.0 * POINTS_PER_UNIT;
            let title_y = (SLIDE_HEIGHT - 70.0) * POINTS_PER_UNIT;
            pdf.draw_text(page, title_x, title_y, &slide.title, &style_title);
        }
        for elem in &slide.elements {
            draw_element(&mut pdf, page, doc, elem, &mut pdf_images);
        }
    }
    pdf.serialize()
}

/// Draws one object: its shape or picture, then its text in the editor's box.
fn draw_element(
    pdf: &mut PdfDocument,
    page: PageIndex,
    doc: &PresentationDocument,
    elem: &SlideElement,
    pdf_images: &mut BTreeMap<String, usize>,
) {
    let transform = element_transform(elem);
    match elem.element_type {
        ElementType::ShapeRectangle | ElementType::ShapeCircle | ElementType::StatCard => {
            let rgb = element_fill(elem).unwrap_or(if elem.element_type == ElementType::StatCard {
                STAT_RGB
            } else {
                SHAPE_RGB
            });
            let (w, h) = (elem.width, elem.height);
            pdf.draw_rect_with_transform(page, (0.0, 0.0, w, h), PathStyle::filled(rgb), transform);
            draw_text_box(pdf, page, transform, elem);
        }
        ElementType::Title | ElementType::Subtitle | ElementType::BodyText => {
            if let Some(rgb) = element_fill(elem) {
                let (w, h) = (elem.width, elem.height);
                pdf.draw_rect_with_transform(
                    page,
                    (0.0, 0.0, w, h),
                    PathStyle::filled(rgb),
                    transform,
                );
            }
            draw_text_box(pdf, page, transform, elem);
        }
        ElementType::Picture => {
            if let Some(asset) = doc.asset_for(elem) {
                let index = match pdf_images.get(&asset.id) {
                    Some(index) => Some(*index),
                    None => pictures::pdf_image(asset).ok().map(|image| {
                        let index = pdf.add_image(image);
                        pdf_images.insert(asset.id.clone(), index);
                        index
                    }),
                };
                if let Some(index) = index {
                    let size = (elem.width, elem.height);
                    pdf.draw_image_with_transform(page, index, size, transform);
                }
            }
        }
        ElementType::Table => {
            if let Some(table) = elem.table() {
                draw_table(pdf, page, transform, elem, &table);
            }
        }
        ElementType::Chart => {
            if let Some(chart) = elem.chart() {
                draw_chart(pdf, page, transform, elem, &chart);
            }
        }
    }
}

/// The matrix that places an object's local space on the page. Local space has its
/// origin at the object's top left and runs down, as the editor's does, so text and
/// rectangles are drawn in scene units and the matrix does the scaling and turning.
fn element_transform(elem: &SlideElement) -> [f32; 6] {
    let radians = normalize_angle_degrees(elem.rotation_deg).to_radians();
    let (sin, cos) = radians.sin_cos();
    let scale = POINTS_PER_UNIT;
    let center_x = (elem.x + elem.width / 2.0) * scale;
    let center_y = PAGE_HEIGHT - (elem.y + elem.height / 2.0) * scale;
    // Present's scene is y-down, PDF's is y-up; the clockwise turn composes with that flip.
    let a = scale * cos;
    let b = -scale * sin;
    let c = -scale * sin;
    let d = -scale * cos;
    let e = center_x - (a * elem.width / 2.0 + c * elem.height / 2.0);
    let f = center_y - (b * elem.width / 2.0 + d * elem.height / 2.0);
    [a, b, c, d, e, f]
}

/// `#RRGGBB` as RGB in 0..=1.
fn parse_rgb(hex: &str) -> Option<(f32, f32, f32)> {
    let hex = hex.trim().trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }
    let channel = |range: std::ops::Range<usize>| {
        u8::from_str_radix(&hex[range], 16)
            .ok()
            .map(|v| f32::from(v) / 255.0)
    };
    Some((channel(0..2)?, channel(2..4)?, channel(4..6)?))
}

/// The background fill an element's own style gives it, if any.
fn element_fill(elem: &SlideElement) -> Option<(f32, f32, f32)> {
    elem.style.fill.as_deref().and_then(parse_rgb)
}

/// The text of an element as the editor sets it: its type's size and weight, and
/// titles and stat cards centred across the box, with the element's own style
/// (size in points, weight, italics, colour, alignment) laid over that. Alignment
/// is 0 left, 1 centre, 2 right.
fn text_style(elem: &SlideElement) -> (TextStyle, u8) {
    let (px, bold, centred) = match elem.element_type {
        ElementType::Title => (DISPLAY_PX, true, true),
        ElementType::StatCard => (HEADING_PX, true, true),
        _ => (BODY_PX, false, false),
    };
    let own = &elem.style;
    let mut style = TextStyle {
        // The page transform scales scene units to points, so a size in points is
        // divided back into scene units here.
        size_pt: own
            .font_size_pt
            .map_or(px * UNITS_PER_EDITOR_PX, |pt| pt / POINTS_PER_UNIT),
        bold: own.bold.unwrap_or(bold),
        italic: own.italic,
        ..TextStyle::default()
    };
    if let Some(rgb) = own.color.as_deref().and_then(parse_rgb) {
        style.fill_rgb = rgb;
    }
    let align = match own.align {
        Some(crate::TextAlign::Left) => 0,
        Some(crate::TextAlign::Center) => 1,
        Some(crate::TextAlign::Right) => 2,
        None => u8::from(centred),
    };
    (style, align)
}

/// Draws an element's text inside the editor's text box: wrapped to its width,
/// centred vertically, and centred across or set at the left as its type says. Text
/// that does not fit its box ends in an ellipsis on its last visible line.
fn draw_text_box(pdf: &mut PdfDocument, page: PageIndex, transform: [f32; 6], elem: &SlideElement) {
    if elem.content.trim().is_empty() {
        return;
    }
    let (style, align) = text_style(elem);
    let left = SPACE_SM_PX * UNITS_PER_EDITOR_PX;
    let top = SPACE_XS_PX * UNITS_PER_EDITOR_PX;
    let width = (elem.width - 2.0 * SPACE_LG_PX * UNITS_PER_EDITOR_PX).max(1.0);
    let height = (elem.height - 2.0 * SPACE_SM_PX * UNITS_PER_EDITOR_PX).max(1.0);
    let line_height = style.size_pt * LINE_EM;

    let mut lines = wrap_paragraphs(&elem.content, &style, width);
    let fits = ((height / line_height).floor() as usize).max(1);
    if lines.len() > fits {
        lines.truncate(fits);
        if let Some(last) = lines.last_mut() {
            *last = with_ellipsis(last, &style, width);
        }
    }
    let block = lines.len() as f32 * line_height;
    let first_top = top + (height - block) / 2.0;
    for (index, line) in lines.iter().enumerate() {
        if line.is_empty() {
            continue;
        }
        let x = match align {
            1 => left + (width - text_width_pt(line, &style)) / 2.0,
            2 => left + width - text_width_pt(line, &style),
            _ => left,
        };
        let baseline = first_top + index as f32 * line_height + ASCENT_EM * style.size_pt;
        pdf.draw_text_with_transform(page, x, baseline, line, &style, transform);
    }
}

/// Every paragraph of `text`, wrapped to `width`. A blank paragraph keeps one empty
/// line, so the space between paragraphs survives.
fn wrap_paragraphs(text: &str, style: &TextStyle, width: f32) -> Vec<String> {
    text.split('\n')
        .flat_map(|paragraph| wrap_paragraph(paragraph.trim_end_matches('\r'), style, width))
        .collect()
}

/// One paragraph wrapped to `width`: words stay whole where they fit, and a word
/// wider than the box breaks between characters, so no text is lost.
fn wrap_paragraph(paragraph: &str, style: &TextStyle, width: f32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in paragraph.split_whitespace() {
        let joined = if line.is_empty() {
            word.to_string()
        } else {
            format!("{line} {word}")
        };
        if text_width_pt(&joined, style) <= width {
            line = joined;
            continue;
        }
        if !line.is_empty() {
            lines.push(std::mem::take(&mut line));
        }
        for ch in word.chars() {
            line.push(ch);
            if line.chars().count() > 1 && text_width_pt(&line, style) > width {
                let next = line.pop().unwrap_or(ch);
                lines.push(std::mem::take(&mut line));
                line.push(next);
            }
        }
    }
    lines.push(line);
    lines
}

/// `line` shortened from its end until it and an ellipsis fit `width`.
fn with_ellipsis(line: &str, style: &TextStyle, width: f32) -> String {
    let mut kept: String = line.to_string();
    while !kept.is_empty() && text_width_pt(&format!("{kept}\u{2026}"), style) > width {
        kept.pop();
    }
    format!("{kept}\u{2026}")
}

fn draw_table(
    pdf: &mut PdfDocument,
    page: PageIndex,
    transform: [f32; 6],
    elem: &SlideElement,
    table: &loom_objects::Table,
) {
    let rows = table.row_count();
    let cols = table.col_count();
    if rows == 0 || cols == 0 {
        return;
    }
    let total_w: f32 = table.columns.iter().map(|c| c.width_pt).sum();
    let total_h: f32 = table.rows.iter().map(|r| r.height_pt.unwrap_or(20.0)).sum();
    let scale_x = if total_w > 0.0 {
        elem.width / total_w
    } else {
        1.0
    };
    let scale_y = if total_h > 0.0 {
        elem.height / total_h
    } else {
        1.0
    };

    let style = loom_objects::styles::TableStyleCatalog::get(&table.style_id)
        .unwrap_or_else(loom_objects::styles::TableStyleCatalog::default_style);
    let options = loom_objects::styles::TableStyleOptions {
        header_row: table.header_row,
        total_row: table.total_row,
        first_column: table.first_column,
        last_column: table.last_column,
        banded_rows: table.banded_rows,
        banded_columns: table.banded_columns,
    };

    let mut curr_y = 0.0;
    for (r, row) in table.rows.iter().enumerate() {
        let row_h = row.height_pt.unwrap_or(20.0) * scale_y;
        let mut curr_x = 0.0;
        for (c, cell) in row.cells.iter().enumerate() {
            let col_w = table.columns.get(c).map(|col| col.width_pt).unwrap_or(80.0) * scale_x;
            if !cell.is_covered {
                let resolved = style.resolve_cell(r, c, rows, cols, &options, Some(cell));
                if let Some(bg) = &resolved.background_color {
                    if let Some(rgb) = parse_rgb(bg) {
                        pdf.draw_rect_with_transform(
                            page,
                            (curr_x, curr_y, col_w, row_h),
                            PathStyle::filled(rgb),
                            transform,
                        );
                    }
                }
                pdf.draw_rect_with_transform(
                    page,
                    (curr_x, curr_y, col_w, row_h),
                    PathStyle::stroked((0.7, 0.7, 0.7), 0.5),
                    transform,
                );
                let text = cell.content.text();
                if !text.is_empty() {
                    let text_style = TextStyle {
                        size_pt: crate::table_text_size(table)
                            .map_or(11.0 * UNITS_PER_EDITOR_PX, |pt| pt.clamp(6.0, 72.0)),
                        bold: resolved.bold,
                        ..TextStyle::default()
                    };
                    pdf.draw_text_with_transform(
                        page,
                        curr_x + 4.0 * UNITS_PER_EDITOR_PX,
                        curr_y + row_h * 0.7,
                        &text,
                        &text_style,
                        transform,
                    );
                }
            }
            curr_x += col_w;
        }
        curr_y += row_h;
    }
}

const CHART_COLORS: [(f32, f32, f32); 6] = [
    (0.169, 0.341, 0.604),
    (0.929, 0.490, 0.192),
    (0.439, 0.678, 0.278),
    (1.000, 0.753, 0.000),
    (0.357, 0.608, 0.835),
    (0.647, 0.647, 0.647),
];

fn draw_pie_chart(
    pdf: &mut PdfDocument,
    page: PageIndex,
    transform: [f32; 6],
    elem: &SlideElement,
    chart: &loom_objects::Chart,
) {
    let series = &chart.series[0];
    let cat_count = chart.categories.len().max(series.values.len());
    let values: Vec<f64> = (0..cat_count)
        .map(|ci| series.values.get(ci).copied().unwrap_or(0.0).max(0.0))
        .collect();
    let total: f64 = values.iter().sum();
    if total <= 0.0 {
        return;
    }
    let cx = elem.width * 0.31;
    let cy = elem.height * 0.55;
    let r = (elem.width * 0.22).min(elem.height * 0.32).max(12.0);
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
            let theta = -std::f32::consts::FRAC_PI_2 + (turn as f32) * std::f32::consts::TAU;
            let x = cx + rad * theta.cos() - d / 2.0;
            let y = cy + rad * theta.sin() - d / 2.0;
            let color = CHART_COLORS[idx % CHART_COLORS.len()];
            pdf.draw_rect_with_transform(page, (x, y, d, d), PathStyle::filled(color), transform);
        }
    }
    let label_style = TextStyle {
        size_pt: 9.5 * UNITS_PER_EDITOR_PX,
        ..TextStyle::default()
    };
    let row_step = (elem.height * 0.62 / cat_count.max(1) as f32).min(24.0 * UNITS_PER_EDITOR_PX);
    for (ci, v) in values.iter().copied().enumerate() {
        let ry = elem.height * 0.24 + ci as f32 * row_step;
        let color = CHART_COLORS[ci % CHART_COLORS.len()];
        pdf.draw_rect_with_transform(
            page,
            (
                elem.width * 0.58,
                ry - 7.0 * UNITS_PER_EDITOR_PX,
                8.0 * UNITS_PER_EDITOR_PX,
                8.0 * UNITS_PER_EDITOR_PX,
            ),
            PathStyle::filled(color),
            transform,
        );
        let cat = chart.categories.get(ci).map(String::as_str).unwrap_or("");
        let pct = (v / total * 100.0).round() as i64;
        pdf.draw_text_with_transform(
            page,
            elem.width * 0.63,
            ry,
            &format!("{cat} ({pct}%)"),
            &label_style,
            transform,
        );
    }
}

fn draw_chart(
    pdf: &mut PdfDocument,
    page: PageIndex,
    transform: [f32; 6],
    elem: &SlideElement,
    chart: &loom_objects::Chart,
) {
    pdf.draw_rect_with_transform(
        page,
        (0.0, 0.0, elem.width, elem.height),
        PathStyle::filled((0.98, 0.98, 0.98)),
        transform,
    );
    pdf.draw_rect_with_transform(
        page,
        (0.0, 0.0, elem.width, elem.height),
        PathStyle::stroked((0.85, 0.88, 0.90), 1.0),
        transform,
    );
    if let Some(title) = &chart.title {
        let title_style = TextStyle {
            size_pt: 12.0 * UNITS_PER_EDITOR_PX,
            bold: true,
            ..TextStyle::default()
        };
        pdf.draw_text_with_transform(
            page,
            12.0 * UNITS_PER_EDITOR_PX,
            18.0 * UNITS_PER_EDITOR_PX,
            title,
            &title_style,
            transform,
        );
    }
    if chart.series.is_empty() || chart.categories.is_empty() {
        return;
    }
    if chart.chart_type == loom_objects::ChartType::Pie {
        draw_pie_chart(pdf, page, transform, elem, chart);
        return;
    }
    let cat_count = chart.categories.len();
    let series_count = chart.series.len();
    let plot_top = 28.0 * UNITS_PER_EDITOR_PX;
    let plot_bottom = elem.height - 22.0 * UNITS_PER_EDITOR_PX;
    let plot_h = (plot_bottom - plot_top).max(20.0);
    let plot_left = 24.0 * UNITS_PER_EDITOR_PX;
    let plot_w = (elem.width - 48.0 * UNITS_PER_EDITOR_PX).max(40.0);
    if chart.chart_type == loom_objects::ChartType::Bar {
        let max_abs = chart
            .series
            .iter()
            .flat_map(|series| series.values.iter().copied())
            .filter(|value| value.is_finite())
            .map(f64::abs)
            .fold(0.0f64, f64::max);
        let scale = if max_abs > 0.0 { max_abs } else { 1.0 };
        let min = chart
            .series
            .iter()
            .flat_map(|series| series.values.iter().copied())
            .filter(|value| value.is_finite())
            .map(|value| value / scale)
            .fold(0.0f64, f64::min);
        let max = chart
            .series
            .iter()
            .flat_map(|series| series.values.iter().copied())
            .filter(|value| value.is_finite())
            .map(|value| value / scale)
            .fold(0.0f64, f64::max);
        let span = max - min;
        let span = if span > 0.0 { span } else { 1.0 };
        let zero = (-min / span) as f32;
        let label_width = (elem.width * 0.2).min(80.0 * UNITS_PER_EDITOR_PX);
        let left = plot_left + label_width;
        let width = (plot_w - label_width).max(1.0);
        let row_height = plot_h / cat_count as f32;
        let bar_height = row_height * 0.8 / series_count as f32;
        for (s_idx, series) in chart.series.iter().enumerate() {
            for (c_idx, value) in series.values.iter().copied().enumerate().take(cat_count) {
                if !value.is_finite() {
                    continue;
                }
                let end = ((value / scale - min) / span) as f32;
                pdf.draw_rect_with_transform(
                    page,
                    (
                        left + width * zero.min(end),
                        plot_top + row_height * (c_idx as f32 + 0.1) + bar_height * s_idx as f32,
                        width * (end - zero).abs(),
                        (bar_height - 1.0).max(1.0),
                    ),
                    PathStyle::filled(CHART_COLORS[s_idx % CHART_COLORS.len()]),
                    transform,
                );
            }
        }
        let label_style = TextStyle {
            size_pt: 8.5 * UNITS_PER_EDITOR_PX,
            ..TextStyle::default()
        };
        for (c_idx, cat) in chart.categories.iter().enumerate() {
            pdf.draw_text_with_transform(
                page,
                2.0 * UNITS_PER_EDITOR_PX,
                plot_top + row_height * (c_idx as f32 + 0.5),
                cat,
                &label_style,
                transform,
            );
        }
        return;
    }
    let norm_values = chart.normalized_series_values_from_zero();
    let cat_w = plot_w / cat_count as f32;
    let bar_slot = cat_w * 0.8;
    let bar_w = (bar_slot / series_count as f32).max(2.0);
    let is_point_chart = matches!(
        chart.chart_type,
        loom_objects::ChartType::Line
            | loom_objects::ChartType::Area
            | loom_objects::ChartType::Scatter
    );
    for (s_idx, s_norm) in norm_values.iter().enumerate() {
        let color = CHART_COLORS[s_idx % CHART_COLORS.len()];
        let pts: Vec<(f32, f32)> = s_norm
            .iter()
            .copied()
            .enumerate()
            .take(cat_count)
            .map(|(c_idx, norm)| {
                (
                    plot_left + (c_idx as f32 + 0.5) * cat_w,
                    plot_bottom - plot_h * (norm as f32),
                )
            })
            .collect();
        if is_point_chart {
            if chart.chart_type != loom_objects::ChartType::Scatter {
                for pair in pts.windows(2) {
                    for k in 0..=20usize {
                        let t = k as f32 / 20.0;
                        let x = pair[0].0 + (pair[1].0 - pair[0].0) * t;
                        let y = pair[0].1 + (pair[1].1 - pair[0].1) * t;
                        let h = if chart.chart_type == loom_objects::ChartType::Area {
                            (plot_bottom - y).max(2.0)
                        } else {
                            2.5
                        };
                        pdf.draw_rect_with_transform(
                            page,
                            (x - 1.25, y - 1.25, 2.5, h),
                            PathStyle::filled(color),
                            transform,
                        );
                    }
                }
            }
            for (cx, cy) in pts {
                pdf.draw_rect_with_transform(
                    page,
                    (cx - 3.0, cy - 3.0, 6.0, 6.0),
                    PathStyle::filled(color),
                    transform,
                );
            }
        } else {
            for (c_idx, norm) in s_norm.iter().copied().enumerate().take(cat_count) {
                let bar_h = (plot_h * norm as f32).max(2.0);
                let cat_x = plot_left + c_idx as f32 * cat_w + (cat_w - bar_slot) / 2.0;
                let x = cat_x + s_idx as f32 * bar_w;
                let y = plot_bottom - bar_h;
                pdf.draw_rect_with_transform(
                    page,
                    (x, y, bar_w - 1.0, bar_h),
                    PathStyle::filled(color),
                    transform,
                );
            }
        }
    }
    let label_style = TextStyle {
        size_pt: 8.5 * UNITS_PER_EDITOR_PX,
        ..TextStyle::default()
    };
    for (c_idx, cat) in chart.categories.iter().enumerate() {
        let lx = plot_left + c_idx as f32 * cat_w + cat_w * 0.25;
        pdf.draw_text_with_transform(
            page,
            lx,
            elem.height - 6.0 * UNITS_PER_EDITOR_PX,
            cat,
            &label_style,
            transform,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Slide;
    use loom_objects::{Chart, ChartType};

    #[test]
    fn pdf_export_draws_pie_slice_percentages_and_distinct_line_chart_geometry() {
        let mut doc = PresentationDocument::new("c-deck", "Charts");
        doc.slides.clear();
        let mut slide = Slide::new("s1", "Charts", "blank");

        let mut pie = Chart::new("pie-1", ChartType::Pie);
        pie.title = Some("Spend".into());
        pie.categories = vec!["Alpha".into(), "Beta".into()];
        pie.add_series("Share", vec![75.0, 25.0], None);
        slide.add_element(SlideElement::new_chart(
            "el-pie", 40.0, 80.0, 400.0, 260.0, pie,
        ));

        let mut line = Chart::new("line-1", ChartType::Line);
        line.title = Some("Trend".into());
        line.categories = vec!["Q1".into(), "Q2".into(), "Q3".into()];
        line.add_series("Rev", vec![10.0, 30.0, 20.0], None);
        slide.add_element(SlideElement::new_chart(
            "el-line", 480.0, 80.0, 400.0, 260.0, line,
        ));

        doc.slides.push(slide);
        let pdf_bytes = export_pdf(&doc);
        let content = loom_pdf::inspect::readable_content(&pdf_bytes).expect("readable PDF");
        assert!(
            content.contains(r"Alpha \(75%\)") && content.contains(r"Beta \(25%\)"),
            "pie chart PDF export must include slice percentage labels, got:\n{content}"
        );
        assert!(
            content.contains("(Q1)") && content.contains("(Q2)") && content.contains("(Q3)"),
            "line chart PDF export must include category labels, got:\n{content}"
        );
    }
}
