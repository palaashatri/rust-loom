//! The drawable pieces of a table or chart element, in fractions of the
//! element's box, so the slide canvas can draw them at any size.

use crate::ObjectPart;
use loom_objects::{
    Chart, ChartType, HorizontalAlign, Table, TableStyleCatalog, TableStyleOptions,
};
use loom_present_core::{ElementType, SlideElement, TextAlign};
use slint::{Color, SharedString};

/// Series colours used when a series carries none.
const SERIES: [u32; 6] = [
    0xFF2B_579A,
    0xFFED_7D31,
    0xFF70_AD47,
    0xFFFF_C000,
    0xFF5B_9BD5,
    0xFFA5_A5A5,
];

pub(crate) fn color(hex: &str) -> Color {
    let clean = hex.trim_start_matches('#');
    let rgb = u32::from_str_radix(clean.get(..6).unwrap_or(clean), 16).unwrap_or(0);
    Color::from_argb_encoded(0xFF00_0000 | rgb)
}

fn part(fx: f32, fy: f32, fw: f32, fh: f32) -> ObjectPart {
    ObjectPart {
        fx,
        fy,
        fw,
        fh,
        text: SharedString::default(),
        has_bg: false,
        bg: Color::from_argb_encoded(0),
        fg: Color::from_argb_encoded(0xFF20_2020),
        bold: false,
        align: 0,
        fs: 12.0,
        has_border: false,
        wrap: false,
        pad: false,
        italic: false,
        family: SharedString::default(),
        ink: 0,
        round: false,
    }
}

/// Points a slide unit is worth: a 1000-unit slide is PowerPoint's 960 pt wide.
pub(crate) const POINTS_PER_UNIT: f32 = 0.96;

/// What an unstyled text object looks like, so a style that changes one thing
/// (bold, say) keeps the rest: `(size in pt, bold, ink, centred)`. The sizes
/// are the canvas's 24, 14 and 20 px tokens on an 840 px slide.
fn type_defaults(kind: &ElementType) -> Option<(f32, bool, i32, bool)> {
    match kind {
        ElementType::Title => Some((27.43, true, 1, true)),
        ElementType::Subtitle => Some((16.0, false, 2, false)),
        ElementType::BodyText => Some((16.0, false, 1, false)),
        ElementType::StatCard => Some((22.86, true, 1, true)),
        ElementType::ShapeRectangle | ElementType::ShapeCircle => Some((16.0, false, 3, false)),
        _ => None,
    }
}

/// How a text object or shape is drawn now, as the inspector shows it.
pub(crate) struct TextLook {
    pub size_pt: f32,
    pub bold: bool,
    pub italic: bool,
    /// 0 left, 1 centre, 2 right.
    pub align: i32,
}

/// The look of a text object or shape, its own style over its type's; `None`
/// for a picture, table or chart.
pub(crate) fn look(element: &SlideElement) -> Option<TextLook> {
    let (size_pt, bold, _, centred) = type_defaults(&element.element_type)?;
    let style = &element.style;
    Some(TextLook {
        size_pt: style.size_or(size_pt),
        bold: style.bold.unwrap_or(bold),
        italic: style.italic,
        align: match style.align {
            Some(TextAlign::Left) => 0,
            Some(TextAlign::Center) => 1,
            Some(TextAlign::Right) => 2,
            None => i32::from(centred),
        },
    })
}

/// The text of a text object or shape, as one part over its whole box.
fn styled_text_part(element: &SlideElement) -> Option<ObjectPart> {
    let (size_pt, bold, ink, centred) = type_defaults(&element.element_type)?;
    let style = &element.style;
    let mut text = part(0.0, 0.0, 1.0, 1.0);
    text.text = SharedString::from(element.content.clone());
    text.fs = style.size_or(size_pt) / POINTS_PER_UNIT;
    text.bold = style.bold.unwrap_or(bold);
    text.italic = style.italic;
    text.wrap = true;
    text.pad = true;
    text.align = match style.align {
        Some(TextAlign::Left) => 0,
        Some(TextAlign::Center) => 1,
        Some(TextAlign::Right) => 2,
        None => i32::from(centred),
    };
    match style.color.as_deref() {
        Some(hex) => {
            text.fg = color(hex);
            text.ink = 0;
        }
        None => text.ink = ink,
    }
    if let Some(family) = style
        .font_family
        .as_deref()
        .and_then(|name| loom_present_core::normalize_font_family(name).ok())
    {
        text.family = SharedString::from(family);
    }
    Some(text)
}

/// Fill and text of a text object or shape whose style overrides its type's
/// look; nothing for an element that was never styled, which the canvas draws
/// itself.
fn styled_parts(element: &SlideElement) -> Vec<ObjectPart> {
    if element.style.is_default() {
        return Vec::new();
    }
    let Some(text) = styled_text_part(element) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if let Some(hex) = element.style.fill.as_deref() {
        let mut fill = part(0.0, 0.0, 1.0, 1.0);
        fill.has_bg = true;
        fill.bg = color(hex);
        fill.round = element.element_type == ElementType::ShapeCircle;
        out.push(fill);
    }
    out.push(text);
    out
}

/// Parts for a table or chart element; empty for every other element.
pub(crate) fn parts_for(element: &SlideElement) -> Vec<ObjectPart> {
    match element.element_type {
        ElementType::Table => element
            .table()
            .map(|table| table_parts(&table, element.height))
            .unwrap_or_default(),
        ElementType::Chart => element
            .chart()
            .map(|chart| chart_parts(&chart, element.width, element.height))
            .unwrap_or_default(),
        _ => styled_parts(element),
    }
}

/// The cell the keyboard is in for a table element (box and text); the whole
/// element, with no text, for anything else.
pub(crate) fn active_cell_for(element: &SlideElement) -> ObjectPart {
    let whole = part(0.0, 0.0, 1.0, 1.0);
    let Some(table) = element.table() else {
        return if element.style.is_default() {
            whole
        } else {
            styled_text_part(element).unwrap_or(whole)
        };
    };
    let (rows, cols) = (table.row_count(), table.col_count());
    if rows == 0 || cols == 0 {
        return whole;
    }
    let (r, c) = crate::table_actions::get_active_table_cell(&element.id);
    let (r, c) = (r.min(rows - 1), c.min(cols - 1));
    let (r0, c0) = table
        .effective_cell(r, c)
        .map_or((r, c), |(r0, c0, _)| (r0, c0));
    let widths: Vec<f32> = table.columns.iter().map(|c| c.width_pt.max(1.0)).collect();
    let heights: Vec<f32> = table
        .rows
        .iter()
        .map(|r| r.height_pt.unwrap_or(24.0).max(1.0))
        .collect();
    let (total_w, total_h): (f32, f32) = (widths.iter().sum(), heights.iter().sum());
    let (rs, cs) = table
        .cell(r0, c0)
        .map_or((1, 1), |cell| (cell.row_span.max(1), cell.col_span.max(1)));
    let x: f32 = widths.iter().take(c0).sum();
    let y: f32 = heights.iter().take(r0).sum();
    let w: f32 = widths.iter().skip(c0).take(cs).sum();
    let h: f32 = heights.iter().skip(r0).take(rs).sum();
    let mut cell = part(x / total_w, y / total_h, w / total_w, h / total_h);
    cell.text = SharedString::from(table.cell_text(r0, c0).unwrap_or_default());
    cell.fs = loom_present_core::table_text_size(&table).map_or_else(
        || (heights[r0] * 0.42).clamp(9.0, 30.0),
        |pt| pt.clamp(6.0, 72.0),
    );
    cell
}

fn table_parts(table: &Table, element_height: f32) -> Vec<ObjectPart> {
    let (rows, cols) = (table.row_count(), table.col_count());
    if rows == 0 || cols == 0 {
        return Vec::new();
    }
    let widths: Vec<f32> = table.columns.iter().map(|c| c.width_pt.max(1.0)).collect();
    let heights: Vec<f32> = table
        .rows
        .iter()
        .map(|r| r.height_pt.unwrap_or(24.0).max(1.0))
        .collect();
    let (total_w, total_h): (f32, f32) = (widths.iter().sum(), heights.iter().sum());
    let style =
        TableStyleCatalog::get(&table.style_id).unwrap_or_else(TableStyleCatalog::default_style);
    let options = TableStyleOptions {
        header_row: table.header_row,
        total_row: table.total_row,
        first_column: table.first_column,
        last_column: table.last_column,
        banded_rows: table.banded_rows,
        banded_columns: table.banded_columns,
    };

    let mut out = Vec::with_capacity(rows * cols);
    let mut y0 = 0.0f32;
    for (r, row_h) in heights.iter().enumerate() {
        let mut x0 = 0.0f32;
        for (c, col_w) in widths.iter().enumerate() {
            let cell = table.cell(r, c);
            if cell.is_some_and(|cell| cell.is_covered) {
                x0 += col_w;
                continue;
            }
            let (rs, cs) = cell.map_or((1, 1), |cell| (cell.row_span.max(1), cell.col_span.max(1)));
            let w: f32 = widths.iter().skip(c).take(cs).sum();
            let h: f32 = heights.iter().skip(r).take(rs).sum();
            let resolved = style.resolve_cell(r, c, rows, cols, &options, cell);
            let mut p = part(x0 / total_w, y0 / total_h, w / total_w, h / total_h);
            p.text = SharedString::from(table.cell_text(r, c).unwrap_or_default());
            if let Some(bg) = &resolved.background_color {
                p.has_bg = true;
                p.bg = color(bg);
            }
            p.fg = color(resolved.text_color.as_deref().unwrap_or("#202020"));
            p.bold = resolved.bold;
            p.align = match resolved.horizontal_align {
                HorizontalAlign::Center => 1,
                HorizontalAlign::Right => 2,
                _ => 0,
            };
            // A taller row reads larger, within limits. The row's share of the
            // element is what is drawn, not its authored points.
            p.fs = loom_present_core::table_text_size(table).map_or_else(
                || (row_h / total_h * element_height * 0.42).clamp(9.0, 30.0),
                |pt| pt.clamp(6.0, 72.0),
            );
            p.has_border = true;
            out.push(p);
            x0 += col_w;
        }
        y0 += row_h;
    }
    out
}

fn dot_part(x: f32, y: f32, d: f32, w: f32, h: f32, fill: Color) -> ObjectPart {
    let mut p = part(x - d / w / 2.0, y - d / h / 2.0, d / w, d / h);
    p.has_bg = true;
    p.bg = fill;
    p.round = true;
    p
}

fn pie_chart_parts(
    chart: &Chart,
    categories: usize,
    width: f32,
    height: f32,
    out: &mut Vec<ObjectPart>,
) {
    let series = &chart.series[0];
    let values: Vec<f64> = (0..categories)
        .map(|ci| series.values.get(ci).copied().unwrap_or(0.0).max(0.0))
        .collect();
    let total: f64 = values.iter().sum();
    if total <= 0.0 {
        return;
    }
    let slice_fill = |ci: usize| {
        if ci == 0 {
            if let Some(hex) = series.color.as_deref() {
                return color(hex);
            }
        }
        Color::from_argb_encoded(SERIES[ci % SERIES.len()])
    };
    let (w, h) = (width.max(1.0), height.max(1.0));
    let (cx, cy) = (0.31f32, 0.55f32);
    let r_px = (w * 0.23).min(h * 0.34).max(12.0);
    let dot_d = (r_px / 3.8).max(4.0);
    out.push(dot_part(cx, cy, dot_d, w, h, slice_fill(0)));
    for ring in 1..=6usize {
        let rad = (ring as f32 - 0.3) / 6.0 * r_px;
        let steps = (ring * 12).max(16);
        for step in 0..steps {
            let turn = (step as f64 + 0.5) / steps as f64;
            let target = turn * total;
            let mut acc = 0.0;
            let mut slice_idx = categories.saturating_sub(1);
            for (ci, v) in values.iter().copied().enumerate() {
                acc += v;
                if target <= acc {
                    slice_idx = ci;
                    break;
                }
            }
            let theta = -std::f32::consts::FRAC_PI_2 + (turn as f32) * std::f32::consts::TAU;
            let x = cx + rad * theta.cos() / w;
            let y = cy + rad * theta.sin() / h;
            out.push(dot_part(x, y, dot_d, w, h, slice_fill(slice_idx)));
        }
    }
    let row_step = (0.66 / categories.max(1) as f32).min(0.13);
    for (ci, v) in values.iter().copied().enumerate() {
        let ry = 0.22 + ci as f32 * row_step;
        let mut swatch = part(0.58, ry + 0.015, 0.04, 0.055);
        swatch.has_bg = true;
        swatch.bg = slice_fill(ci);
        out.push(swatch);
        let cat = chart.categories.get(ci).map(String::as_str).unwrap_or("");
        let pct = (v / total * 100.0).round() as i64;
        let mut label = part(0.64, ry, 0.32, 0.09);
        label.text = SharedString::from(format!("{cat} ({pct}%)"));
        label.fs = 11.0;
        out.push(label);
    }
}

fn chart_parts(chart: &Chart, width: f32, height: f32) -> Vec<ObjectPart> {
    let mut out = Vec::new();
    let mut frame = part(0.0, 0.0, 1.0, 1.0);
    frame.has_bg = true;
    frame.bg = Color::from_argb_encoded(0xFFFF_FFFF);
    frame.has_border = true;
    out.push(frame);

    let mut title = part(0.04, 0.02, 0.92, 0.12);
    title.text = SharedString::from(chart.title.clone().unwrap_or_else(|| "Chart".to_string()));
    title.bold = true;
    title.fs = 16.0;
    out.push(title);

    let categories = chart.categories.len().max(
        chart
            .series
            .iter()
            .map(|s| s.values.len())
            .max()
            .unwrap_or(0),
    );
    if categories == 0 || chart.series.is_empty() {
        return out;
    }
    if chart.chart_type == ChartType::Pie {
        pie_chart_parts(chart, categories, width, height, &mut out);
        return out;
    }
    let max = chart
        .series
        .iter()
        .flat_map(|s| s.values.iter().copied())
        .fold(0.0f64, f64::max)
        .max(f64::MIN_POSITIVE);
    let (px, py, pw, ph) = (0.08f32, 0.18f32, 0.88f32, 0.62f32);
    let group_w = pw / categories as f32;
    let colour = |si: usize, series: &loom_objects::ChartSeries| {
        series
            .color
            .as_deref()
            .map(color)
            .unwrap_or_else(|| Color::from_argb_encoded(SERIES[si % SERIES.len()]))
    };
    let at = |ci: usize, value: f64| {
        let frac = (value.max(0.0) / max) as f32;
        (px + (ci as f32 + 0.5) * group_w, py + ph - ph * frac)
    };
    if matches!(
        chart.chart_type,
        ChartType::Line | ChartType::Area | ChartType::Scatter
    ) {
        let (w, h) = (width.max(1.0), height.max(1.0));
        for (si, series) in chart.series.iter().enumerate() {
            let fill = colour(si, series);
            let points: Vec<(f32, f32)> = series
                .values
                .iter()
                .enumerate()
                .take(categories)
                .map(|(ci, v)| at(ci, *v))
                .collect();
            if chart.chart_type == ChartType::Area {
                let steps = 60usize;
                for pair in points.windows(2) {
                    for k in 0..steps {
                        let t = k as f32 / steps as f32;
                        let x = pair[0].0 + (pair[1].0 - pair[0].0) * t;
                        let y = pair[0].1 + (pair[1].1 - pair[0].1) * t;
                        let mut col = part(
                            x,
                            y,
                            (pair[1].0 - pair[0].0) / steps as f32 + 0.0006,
                            py + ph - y,
                        );
                        col.has_bg = true;
                        col.bg = Color::from_argb_encoded(
                            (fill.as_argb_encoded() & 0x00FF_FFFF) | 0x8000_0000,
                        );
                        out.push(col);
                    }
                }
            }
            if chart.chart_type != ChartType::Scatter {
                for pair in points.windows(2) {
                    let (dx, dy) = ((pair[1].0 - pair[0].0) * w, (pair[1].1 - pair[0].1) * h);
                    let n = ((dx.hypot(dy) / 2.5).ceil() as usize).clamp(1, 120);
                    for k in 0..=n {
                        let t = k as f32 / n as f32;
                        out.push(dot_part(
                            pair[0].0 + (pair[1].0 - pair[0].0) * t,
                            pair[0].1 + (pair[1].1 - pair[0].1) * t,
                            3.0,
                            w,
                            h,
                            fill,
                        ));
                    }
                }
            }
            for (x, y) in points {
                out.push(dot_part(x, y, 8.0, w, h, fill));
            }
        }
    } else if chart.chart_type == ChartType::Bar {
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
        let plot_x = 0.32f32;
        let plot_w = 0.64f32;
        let zero_x = plot_x + plot_w * (-min / span) as f32;
        let row_h = ph / categories as f32;
        let bar_h = row_h * 0.8 / chart.series.len() as f32;

        let mut baseline = part(zero_x, py, 0.0015, ph);
        baseline.has_bg = true;
        baseline.bg = Color::from_argb_encoded(0xFF90_9090);
        out.push(baseline);

        for (si, series) in chart.series.iter().enumerate() {
            let fill = colour(si, series);
            for (ci, value) in series.values.iter().copied().enumerate().take(categories) {
                if !value.is_finite() {
                    continue;
                }
                let end = plot_x + plot_w * (((value / scale - min) / span) as f32);
                let mut bar = part(
                    zero_x.min(end),
                    py + ci as f32 * row_h + row_h * 0.1 + si as f32 * bar_h,
                    (end - zero_x).abs(),
                    bar_h * 0.94,
                );
                bar.has_bg = true;
                bar.bg = fill;
                out.push(bar);
            }
        }
        for ci in 0..categories {
            let mut label = part(0.04, py + ci as f32 * row_h, 0.24, row_h);
            label.text = SharedString::from(chart.categories.get(ci).cloned().unwrap_or_default());
            label.align = 2;
            label.fs = 11.0;
            label.wrap = true;
            out.push(label);
        }
    } else {
        let bar_w = group_w * 0.8 / chart.series.len() as f32;
        for (si, series) in chart.series.iter().enumerate() {
            let fill = colour(si, series);
            for (ci, value) in series.values.iter().enumerate().take(categories) {
                let frac = (value.max(0.0) / max) as f32;
                let h = (ph * frac).max(0.004);
                let mut bar = part(
                    px + ci as f32 * group_w + group_w * 0.1 + si as f32 * bar_w,
                    py + ph - h,
                    bar_w * 0.94,
                    h,
                );
                bar.has_bg = true;
                bar.bg = fill;
                out.push(bar);
            }
        }
    }
    for ci in 0..categories {
        let mut label = part(px + ci as f32 * group_w, py + ph + 0.02, group_w, 0.1);
        label.text = SharedString::from(chart.categories.get(ci).cloned().unwrap_or_default());
        label.align = 1;
        label.fs = 11.0;
        out.push(label);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_table_becomes_one_part_per_visible_cell_covering_the_element() {
        let mut table = Table::new(2, 3);
        table.set_cell_text(0, 0, "Item");
        let parts = {
            let element = SlideElement::new_table("t", 0.0, 0.0, 300.0, 100.0, table);
            parts_for(&element)
        };
        assert_eq!(parts.len(), 6);
        let right_edge = parts.iter().map(|p| p.fx + p.fw).fold(0.0, f32::max);
        let bottom = parts.iter().map(|p| p.fy + p.fh).fold(0.0, f32::max);
        assert!((right_edge - 1.0).abs() < 1e-4 && (bottom - 1.0).abs() < 1e-4);
        assert_eq!(parts[0].text.as_str(), "Item");
        assert!(parts[0].has_bg, "the header row has its style fill");
    }

    #[test]
    fn the_edit_target_is_the_active_cell_with_that_cells_own_text() {
        let mut table = Table::new(2, 3);
        table.set_cell_text(1, 2, "Pears");
        let element = SlideElement::new_table("tbl-edit", 0.0, 0.0, 300.0, 100.0, table);
        crate::table_actions::set_active_table_cell("tbl-edit", 1, 2);
        let cell = active_cell_for(&element);
        assert_eq!(cell.text.as_str(), "Pears", "not the accessibility label");
        assert!(
            (cell.fx - 2.0 / 3.0).abs() < 0.02,
            "third column: {}",
            cell.fx
        );
        assert!(cell.fy > 0.4, "second row: {}", cell.fy);
        // Anything that is not a table edits as a whole.
        let plain = SlideElement::new_chart(
            "c",
            0.0,
            0.0,
            100.0,
            100.0,
            Chart::new("c", ChartType::Column),
        );
        let whole = active_cell_for(&plain);
        assert!(whole.fx == 0.0 && whole.fw == 1.0 && whole.text.is_empty());
    }

    #[test]
    fn line_area_and_scatter_charts_draw_points_not_columns() {
        let make = |kind: ChartType| {
            let mut chart = Chart::new("c", kind);
            chart.categories = vec!["a".into(), "b".into(), "c".into()];
            chart.add_series("s", vec![1.0, 3.0, 2.0], None);
            let element = SlideElement::new_chart("c", 0.0, 0.0, 400.0, 300.0, chart);
            parts_for(&element)
        };
        let columns = make(ChartType::Column);
        let line = make(ChartType::Line);
        let scatter = make(ChartType::Scatter);
        let area = make(ChartType::Area);
        assert!(
            line.iter().filter(|p| p.round).count() > 3,
            "the line is dots"
        );
        assert!(line.len() > columns.len());
        assert_eq!(
            scatter.iter().filter(|p| p.round).count(),
            3,
            "markers only"
        );
        assert!(area.len() > line.len(), "the area adds a fill");
        assert!(!line.iter().any(|p| p.text.contains("drawn as columns")));
    }

    #[test]
    fn pie_charts_draw_a_circular_disc_and_slice_legend_not_columns() {
        let mut chart = Chart::new("c", ChartType::Pie);
        chart.categories = vec!["R&D".into(), "Sales".into(), "Ops".into()];
        chart.add_series("Budget", vec![50.0, 30.0, 20.0], None);
        let element = SlideElement::new_chart("c", 0.0, 0.0, 400.0, 300.0, chart);
        let parts = parts_for(&element);
        assert!(
            !parts.iter().any(|p| p.text.contains("drawn as columns")),
            "pie charts must not fall back to columns"
        );
        assert!(
            parts.iter().filter(|p| p.round).count() > 20,
            "pie chart disc should be drawn with round sector dots"
        );
        assert!(parts.iter().any(|p| p.text.contains("R&D (50%)")));
        assert!(parts.iter().any(|p| p.text.contains("Sales (30%)")));
        assert!(parts.iter().any(|p| p.text.contains("Ops (20%)")));
        let mut slice_colors: Vec<u32> = parts
            .iter()
            .filter(|p| p.round && p.has_bg)
            .map(|p| p.bg.as_argb_encoded())
            .collect();
        slice_colors.sort_unstable();
        slice_colors.dedup();
        assert!(
            slice_colors.len() >= 3,
            "each slice must get its own colour, got {slice_colors:?}"
        );
    }

    #[test]
    fn bar_charts_draw_horizontal_rows_around_zero_for_positive_negative_and_zero_values() {
        let mut chart = Chart::new("c", ChartType::Bar);
        chart.categories = vec!["Positive".into(), "Negative".into(), "Zero".into()];
        chart.add_series("One", vec![10.0, -5.0, 0.0], Some("#ff0000".into()));
        chart.add_series("Two", vec![-10.0, 5.0, 0.0], Some("#00ff00".into()));
        let element = SlideElement::new_chart("c", 0.0, 0.0, 400.0, 300.0, chart);
        let parts = parts_for(&element);
        let bars: Vec<_> = parts
            .iter()
            .filter(|p| p.has_bg && p.bg.as_argb_encoded() != 0xFF90_9090 && !p.has_border)
            .collect();
        assert_eq!(bars.len(), 6);
        let near = |actual: f32, expected: f32| (actual - expected).abs() < 0.001;
        let zero_x = 0.64;
        assert!(near(bars[0].fx, zero_x));
        assert!(near(bars[0].fw, 0.32));
        assert!(near(bars[1].fx, 0.48));
        assert!(near(bars[1].fw, 0.16));
        assert!(near(bars[2].fx, zero_x) && bars[2].fw == 0.0);
        assert!(near(bars[3].fx, 0.32));
        assert!(near(bars[3].fw, 0.32));
        assert!(near(bars[5].fx, zero_x) && bars[5].fw == 0.0);
        let baseline = parts
            .iter()
            .find(|part| part.has_bg && part.bg.as_argb_encoded() == 0xFF90_9090)
            .expect("the zero axis is visible");
        assert!(near(baseline.fx, zero_x) && near(baseline.fy, 0.18) && near(baseline.fh, 0.62));
        assert!(
            parts
                .iter()
                .any(|p| p.text.as_str() == "Positive" && p.align == 2)
                && parts
                    .iter()
                    .any(|p| p.text.as_str() == "Negative" && p.align == 2)
                && parts
                    .iter()
                    .any(|p| p.text.as_str() == "Zero" && p.align == 2),
            "categories label their horizontal rows"
        );
    }

    #[test]
    fn a_styled_text_object_carries_its_family_into_the_canvas_and_edit_surface() {
        let element = SlideElement {
            id: "family".into(),
            element_type: ElementType::BodyText,
            content: "Words".into(),
            x: 0.0,
            y: 0.0,
            width: 300.0,
            height: 80.0,
            rotation_deg: 0.0,
            action: None,
            style: loom_present_core::ElementStyle {
                font_family: Some("Aptos Display".into()),
                ..Default::default()
            },
        };
        let parts = parts_for(&element);
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].family.as_str(), "Aptos Display");
        let edit = active_cell_for(&element);
        assert_eq!(
            edit.family.as_str(),
            "Aptos Display",
            "editing replaces the text without changing its face"
        );
    }

    #[test]
    fn an_unstyled_text_object_leaves_the_family_empty_so_the_theme_applies() {
        let element = SlideElement {
            id: "plain".into(),
            element_type: ElementType::BodyText,
            content: "Words".into(),
            x: 0.0,
            y: 0.0,
            width: 300.0,
            height: 80.0,
            rotation_deg: 0.0,
            action: None,
            style: Default::default(),
        };
        assert!(parts_for(&element).is_empty());
        assert_eq!(active_cell_for(&element).family.as_str(), "");
    }

    #[test]
    fn bar_charts_keep_extreme_and_zero_data_finite_and_skip_invalid_values() {
        let mut chart = Chart::new("c", ChartType::Bar);
        chart.categories = vec![
            "large positive".into(),
            "nan".into(),
            "infinite".into(),
            "large negative".into(),
        ];
        chart.add_series(
            "s",
            vec![f64::MAX, f64::NAN, f64::INFINITY, -f64::MAX],
            None,
        );
        let parts = chart_parts(&chart, 400.0, 300.0);
        let bars: Vec<_> = parts
            .iter()
            .filter(|part| {
                part.has_bg && !part.has_border && part.bg.as_argb_encoded() != 0xFF90_9090
            })
            .collect();
        assert_eq!(bars.len(), 2);
        assert!(bars.iter().all(|part| {
            part.fx.is_finite() && part.fy.is_finite() && part.fw.is_finite() && part.fh.is_finite()
        }));
        assert!(
            parts.iter().any(|part| part.text.as_str() == "Chart"),
            "invalid chart values must not remove the title"
        );

        let mut zero_chart = Chart::new("zero", ChartType::Bar);
        zero_chart.categories = vec!["zero one".into(), "zero two".into()];
        zero_chart.add_series("s", vec![0.0, 0.0], None);
        let zero_parts = chart_parts(&zero_chart, 400.0, 300.0);
        let zero_bars: Vec<_> = zero_parts
            .iter()
            .filter(|part| {
                part.has_bg && !part.has_border && part.bg.as_argb_encoded() != 0xFF90_9090
            })
            .collect();
        assert_eq!(zero_bars.len(), 2);
        assert!(zero_bars.iter().all(|bar| bar.fw == 0.0 && bar.fx == 0.32));
        assert!(
            zero_parts.iter().any(|part| {
                part.has_bg && part.bg.as_argb_encoded() == 0xFF90_9090 && part.fx == 0.32
            }),
            "all-zero charts retain a finite zero axis"
        );
    }
}
