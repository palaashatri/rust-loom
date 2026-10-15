//! The drawable pieces of a table or chart element, in fractions of the
//! element's box, so the slide canvas can draw them at any size.

use crate::ObjectPart;
use loom_objects::{
    Chart, ChartType, HorizontalAlign, Table, TableStyleCatalog, TableStyleOptions,
};
use loom_present_core::{ElementType, SlideElement};
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

fn color(hex: &str) -> Color {
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
    }
}

/// Parts for a table or chart element; empty for every other element.
pub(crate) fn parts_for(element: &SlideElement) -> Vec<ObjectPart> {
    match element.element_type {
        ElementType::Table => element
            .table()
            .map(|table| table_parts(&table))
            .unwrap_or_default(),
        ElementType::Chart => element
            .chart()
            .map(|chart| chart_parts(&chart))
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// The cell the keyboard is in for a table element (box and text); the whole
/// element, with no text, for anything else.
pub(crate) fn active_cell_for(element: &SlideElement) -> ObjectPart {
    let whole = part(0.0, 0.0, 1.0, 1.0);
    let Some(table) = element.table() else {
        return whole;
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
    cell.fs = (heights[r0] * 0.42).clamp(9.0, 30.0);
    cell
}

fn table_parts(table: &Table) -> Vec<ObjectPart> {
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
            // A taller row reads larger, within limits.
            p.fs = (row_h * 0.42).clamp(9.0, 30.0);
            p.has_border = true;
            out.push(p);
            x0 += col_w;
        }
        y0 += row_h;
    }
    out
}

fn chart_parts(chart: &Chart) -> Vec<ObjectPart> {
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
    let max = chart
        .series
        .iter()
        .flat_map(|s| s.values.iter().copied())
        .fold(0.0f64, f64::max)
        .max(f64::MIN_POSITIVE);
    let (px, py, pw, ph) = (0.08f32, 0.18f32, 0.88f32, 0.62f32);
    let group_w = pw / categories as f32;
    let bar_w = group_w * 0.8 / chart.series.len() as f32;
    for (si, series) in chart.series.iter().enumerate() {
        let fill = series
            .color
            .as_deref()
            .map(color)
            .unwrap_or_else(|| Color::from_argb_encoded(SERIES[si % SERIES.len()]));
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
    for ci in 0..categories {
        let mut label = part(px + ci as f32 * group_w, py + ph + 0.02, group_w, 0.1);
        label.text = SharedString::from(chart.categories.get(ci).cloned().unwrap_or_default());
        label.align = 1;
        label.fs = 11.0;
        out.push(label);
    }
    if !matches!(chart.chart_type, ChartType::Bar | ChartType::Column) {
        let mut note = part(0.04, 0.92, 0.92, 0.07);
        note.text = SharedString::from(format!("{:?} chart, drawn as columns", chart.chart_type));
        note.fs = 10.0;
        note.fg = Color::from_argb_encoded(0xFF70_7070);
        out.push(note);
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
}
