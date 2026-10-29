//! Pie chart projection, including bounded small multiples for multiple series.

use loom_objects::chart::Chart;

use super::cartesian::value_at;
use super::{
    primitives::{circle, format_value, path_element, rect, short_label, text},
    PALETTE,
};

pub(super) fn render(
    out: &mut String,
    chart: &Chart,
    width: f32,
    height: f32,
    categories: usize,
    series: usize,
) {
    let width = f64::from(width);
    let height = f64::from(height);
    let has_legend = chart.style.show_legend;
    let legend_width = if has_legend { 95.0 } else { 8.0 };
    let left = 10.0;
    let right = (width - legend_width).max(left + 30.0);
    let top = 32.0;
    let bottom = (height - 11.0).max(top + 20.0);

    if series == 1 {
        let cx = (left + right) / 2.0;
        let cy = (top + bottom) / 2.0 + 3.0;
        let radius = ((right - left).min(bottom - top) * 0.39).max(5.0);
        pie(out, chart, 0, categories, cx, cy, radius);
        text(
            out,
            (cx, top + 4.0),
            6.2,
            "#444444",
            "middle",
            &short_label(&chart.series[0].name, 20),
            false,
        );
    } else {
        let columns = if series > 3 { 2 } else { 1 };
        let rows = series.div_ceil(columns);
        let cell_width = (right - left) / columns as f64;
        let cell_height = (bottom - top) / rows as f64;
        for series_index in 0..series {
            let column = series_index % columns;
            let row = series_index / columns;
            let cell_left = left + column as f64 * cell_width;
            let cy = top + (row as f64 + 0.5) * cell_height;
            let radius = (cell_height * 0.34).min(cell_width * 0.22).max(4.0);
            let cx = cell_left + radius + 4.0;
            pie(out, chart, series_index, categories, cx, cy, radius);
            text(
                out,
                (cx + radius + 4.0, cy + 2.0),
                5.8,
                "#444444",
                "start",
                &short_label(&chart.series[series_index].name, 16),
                false,
            );
        }
    }

    if has_legend {
        let legend_x = right + 5.0;
        let legend_entries = categories.min(12);
        for (index, category) in chart.categories.iter().take(legend_entries).enumerate() {
            let y = top + 6.0 + index as f64 * 10.0;
            rect(
                out,
                legend_x,
                y - 5.0,
                6.0,
                6.0,
                PALETTE[index % PALETTE.len()],
            );
            text(
                out,
                (legend_x + 9.0, y),
                5.7,
                "#333333",
                "start",
                &short_label(category, 17),
                false,
            );
        }
        if categories > legend_entries {
            text(
                out,
                (legend_x + 9.0, top + 6.0 + legend_entries as f64 * 10.0),
                5.7,
                "#555555",
                "start",
                &format!("+ {} more", categories - legend_entries),
                false,
            );
        }
    }
    if let Some(unit) = chart
        .value_axis
        .title
        .as_deref()
        .filter(|unit| !unit.trim().is_empty())
    {
        text(
            out,
            (left, height - 2.0),
            6.0,
            "#555555",
            "start",
            unit,
            false,
        );
    }
    if let Some(title) = chart
        .category_axis
        .title
        .as_deref()
        .filter(|title| !title.trim().is_empty())
    {
        text(
            out,
            ((left + right) / 2.0, height - 2.0),
            6.0,
            "#555555",
            "middle",
            title,
            false,
        );
    }
}

fn pie(
    out: &mut String,
    chart: &Chart,
    series: usize,
    categories: usize,
    cx: f64,
    cy: f64,
    radius: f64,
) {
    let values: Vec<f64> = (0..categories)
        .map(|category| value_at(chart, series, category).max(0.0))
        .collect();
    let total: f64 = values.iter().sum();
    if let Some((index, value)) = values
        .iter()
        .copied()
        .enumerate()
        .find(|(_, value)| *value > 0.0)
        .filter(|_| values.iter().filter(|value| **value > 0.0).count() == 1)
    {
        circle(out, cx, cy, radius, PALETTE[index % PALETTE.len()]);
        if chart.style.show_data_labels {
            text(
                out,
                (cx, cy + 2.0),
                6.0,
                "#FFFFFF",
                "middle",
                &format_value(value),
                false,
            );
        }
        return;
    }
    let mut angle = -std::f64::consts::FRAC_PI_2;
    for (index, value) in values.into_iter().enumerate() {
        if value <= 0.0 {
            continue;
        }
        let next = angle + std::f64::consts::TAU * value / total;
        let (x1, y1) = (cx + radius * angle.cos(), cy + radius * angle.sin());
        let (x2, y2) = (cx + radius * next.cos(), cy + radius * next.sin());
        let large_arc = if next - angle > std::f64::consts::PI {
            1
        } else {
            0
        };
        let path = format!(
            "M {:.2} {:.2} L {:.2} {:.2} A {:.2} {:.2} 0 {large_arc} 1 {:.2} {:.2} Z",
            cx, cy, x1, y1, radius, radius, x2, y2
        );
        path_element(
            out,
            &path,
            PALETTE[index % PALETTE.len()],
            "#FFFFFF",
            0.6,
            1.0,
        );
        if chart.style.show_data_labels && radius >= 12.0 {
            let middle = (angle + next) / 2.0;
            let lx = cx + radius * 0.62 * middle.cos();
            let ly = cy + radius * 0.62 * middle.sin() + 1.5;
            text(
                out,
                (lx, ly),
                5.0,
                "#FFFFFF",
                "middle",
                &format_value(value),
                false,
            );
        }
        angle = next;
    }
}
