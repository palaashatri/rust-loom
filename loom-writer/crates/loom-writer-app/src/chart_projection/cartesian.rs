//! Cartesian chart geometry and series marks.

use loom_objects::chart::{Chart, ChartType};
use loom_writer_core::{writer_scatter_axis, ScatterAxis};
use std::fmt::Write as _;

use super::primitives::{
    circle, format_axis, format_value, line, path_element, rect, series_color, short_label, text,
    vertical_text,
};

#[derive(Clone, Copy)]
struct Plot {
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
    min: f64,
    max: f64,
}

pub(super) fn scatter_axis(
    chart: &Chart,
    categories: usize,
) -> Result<Option<ScatterAxis>, String> {
    writer_scatter_axis(chart, categories)
}

pub(super) fn render(
    out: &mut String,
    chart: &Chart,
    width: f32,
    height: f32,
    category_count: usize,
    series_count: usize,
    scatter_axis: Option<ScatterAxis>,
) {
    let width = f64::from(width);
    let height = f64::from(height);
    let horizontal_bar = chart.chart_type == ChartType::Bar;
    let has_legend = chart.style.show_legend;
    let legend_width = if has_legend { 92.0 } else { 8.0 };
    let plot = Plot {
        left: if horizontal_bar { 58.0 } else { 38.0 },
        top: 30.0,
        right: (width - legend_width).max(72.0),
        bottom: (height - 43.0).max(48.0),
        min: 0.0,
        max: 1.0,
    };
    let (min, max) = value_range(chart, category_count, series_count, horizontal_bar);
    let plot = Plot { min, max, ..plot };
    let zero = map_value(0.0, plot.min, plot.max);

    for tick in 0..=4 {
        let fraction = tick as f64 / 4.0;
        let value = plot.min + (plot.max - plot.min) * fraction;
        if horizontal_bar {
            let x = plot.left + fraction * (plot.right - plot.left);
            if chart.value_axis.gridlines {
                line(out, x, plot.top, x, plot.bottom, "#E1E4E8", 0.6);
            }
            text(
                out,
                (x, plot.bottom + 10.0),
                6.4,
                "#5F6368",
                "middle",
                &format_axis(value),
                false,
            );
        } else {
            let y = plot.bottom - fraction * (plot.bottom - plot.top);
            if chart.value_axis.gridlines {
                line(out, plot.left, y, plot.right, y, "#E1E4E8", 0.6);
            }
            text(
                out,
                (plot.left - 4.0, y + 2.2),
                6.4,
                "#5F6368",
                "end",
                &format_axis(value),
                false,
            );
        }
    }

    if chart.category_axis.gridlines {
        if horizontal_bar {
            for category in 0..category_count {
                let group_height = (plot.bottom - plot.top) / category_count as f64;
                let y = plot.top + group_height * (category as f64 + 0.5);
                line(out, plot.left, y, plot.right, y, "#EEF0F2", 0.45);
            }
        } else if scatter_axis.is_some() {
            for tick in 0..=4 {
                let fraction = tick as f64 / 4.0;
                let x = plot.left + fraction * (plot.right - plot.left);
                line(out, x, plot.top, x, plot.bottom, "#EEF0F2", 0.45);
            }
        } else {
            for category in 0..category_count {
                let group_width = (plot.right - plot.left) / category_count as f64;
                let x = plot.left + group_width * (category as f64 + 0.5);
                line(out, x, plot.top, x, plot.bottom, "#EEF0F2", 0.45);
            }
        }
    }

    match chart.chart_type {
        ChartType::Column => draw_columns(out, chart, plot, category_count, series_count),
        ChartType::Bar => draw_bars(out, chart, plot, category_count, series_count),
        ChartType::Line => draw_lines(out, chart, plot, category_count, series_count, false),
        ChartType::Area => draw_lines(out, chart, plot, category_count, series_count, true),
        ChartType::Scatter => draw_scatter(
            out,
            chart,
            plot,
            category_count,
            series_count,
            scatter_axis
                .as_ref()
                .expect("scatter X values are validated"),
        ),
        ChartType::Pie => unreachable!("pie charts use their own projection"),
    }

    if horizontal_bar {
        for (index, category) in chart.categories.iter().take(category_count).enumerate() {
            let group_height = (plot.bottom - plot.top) / category_count as f64;
            let y = plot.top + group_height * (index as f64 + 0.5) + 2.0;
            let label = short_label(category, 13);
            text(
                out,
                (plot.left - 4.0, y),
                5.6,
                "#444444",
                "end",
                &label,
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
                ((plot.left + plot.right) / 2.0, height - 5.0),
                6.3,
                "#555555",
                "middle",
                title,
                false,
            );
        }
        if let Some(title) = chart
            .value_axis
            .title
            .as_deref()
            .filter(|title| !title.trim().is_empty())
        {
            vertical_text(
                out,
                9.0,
                (plot.top + plot.bottom) / 2.0,
                6.3,
                "#555555",
                title,
            );
        }
    } else {
        if let Some(axis) = &scatter_axis {
            for tick in 0..=4 {
                let fraction = tick as f64 / 4.0;
                let x = plot.left + fraction * (plot.right - plot.left);
                let value = axis.min + fraction * (axis.max - axis.min);
                text(
                    out,
                    (x, plot.bottom + 10.0),
                    6.4,
                    "#444444",
                    "middle",
                    &format_axis(value),
                    false,
                );
            }
        } else {
            for (index, category) in chart.categories.iter().take(category_count).enumerate() {
                let group_width = (plot.right - plot.left) / category_count as f64;
                let x = plot.left + group_width * (index as f64 + 0.5);
                let y = plot.bottom + 7.0;
                let label = short_label(category, 12);
                text(out, (x, y), 5.6, "#444444", "end", &label, true);
            }
        }
        if let Some(title) = chart
            .category_axis
            .title
            .as_deref()
            .filter(|title| !title.trim().is_empty())
        {
            text(
                out,
                ((plot.left + plot.right) / 2.0, height - 3.0),
                6.3,
                "#555555",
                "middle",
                title,
                false,
            );
        }
        if let Some(title) = chart
            .value_axis
            .title
            .as_deref()
            .filter(|title| !title.trim().is_empty())
        {
            vertical_text(
                out,
                8.0,
                (plot.top + plot.bottom) / 2.0,
                6.3,
                "#555555",
                title,
            );
        }
    }

    line(
        out,
        plot.left,
        plot.top,
        plot.left,
        plot.bottom,
        "#7A7A7A",
        0.8,
    );
    if !horizontal_bar {
        line(out, plot.left, zero, plot.right, zero, "#7A7A7A", 0.8);
    }
    if has_legend {
        let legend_x = plot.right + 7.0;
        for (index, series) in chart.series.iter().take(series_count).enumerate() {
            let y = plot.top + 7.0 + index as f64 * 13.0;
            rect(
                out,
                legend_x,
                y - 5.0,
                7.0,
                7.0,
                &series_color(series.color.as_deref(), index),
            );
            text(
                out,
                (legend_x + 11.0, y + 1.0),
                6.0,
                "#333333",
                "start",
                &short_label(&series.name, 18),
                false,
            );
        }
    }
}

fn draw_columns(out: &mut String, chart: &Chart, plot: Plot, categories: usize, series: usize) {
    let group = (plot.right - plot.left) / categories as f64;
    let bar_width = (group * 0.78 / series as f64).clamp(1.0, 16.0);
    let zero_y = y_value(0.0, plot);
    for category in 0..categories {
        let center = plot.left + group * (category as f64 + 0.5);
        for series_index in 0..series {
            let value = value_at(chart, series_index, category);
            let y = y_value(value, plot);
            let x = center - group * 0.39 + series_index as f64 * bar_width;
            rect(
                out,
                x,
                y.min(zero_y),
                bar_width.max(0.8),
                (zero_y - y).abs().max(0.7),
                &series_color(chart.series[series_index].color.as_deref(), series_index),
            );
            if chart.style.show_data_labels {
                let label_y = if value >= 0.0 {
                    y.min(zero_y) - 2.0
                } else {
                    y.max(zero_y) + 7.0
                };
                text(
                    out,
                    (x + bar_width / 2.0, label_y),
                    5.2,
                    "#333333",
                    "middle",
                    &format_value(value),
                    false,
                );
            }
        }
    }
}

fn draw_bars(out: &mut String, chart: &Chart, plot: Plot, categories: usize, series: usize) {
    let group = (plot.bottom - plot.top) / categories as f64;
    let bar_height = (group * 0.78 / series as f64).clamp(1.0, 12.0);
    let zero_x = x_value(0.0, plot);
    for category in 0..categories {
        let center = plot.top + group * (category as f64 + 0.5);
        for series_index in 0..series {
            let value = value_at(chart, series_index, category);
            let x = x_value(value, plot);
            let y = center - group * 0.39 + series_index as f64 * bar_height;
            rect(
                out,
                x.min(zero_x),
                y,
                (x - zero_x).abs().max(0.7),
                bar_height.max(0.8),
                &series_color(chart.series[series_index].color.as_deref(), series_index),
            );
            if chart.style.show_data_labels {
                let (label_x, anchor) = if value >= 0.0 {
                    (x + 2.0, "start")
                } else {
                    (x - 2.0, "end")
                };
                text(
                    out,
                    (label_x, y + bar_height.min(8.0)),
                    5.0,
                    "#333333",
                    anchor,
                    &format_value(value),
                    false,
                );
            }
        }
    }
}

fn draw_lines(
    out: &mut String,
    chart: &Chart,
    plot: Plot,
    categories: usize,
    series: usize,
    area: bool,
) {
    let group = (plot.right - plot.left) / categories as f64;
    for series_index in 0..series {
        let color = series_color(chart.series[series_index].color.as_deref(), series_index);
        let points: Vec<(f64, f64)> = (0..categories)
            .map(|category| {
                let x = plot.left + group * (category as f64 + 0.5);
                let y = y_value(value_at(chart, series_index, category), plot);
                (x, y)
            })
            .collect();
        if area && points.len() > 1 {
            let baseline = y_value(0.0, plot);
            let mut path = String::with_capacity(points.len() * 24 + 48);
            write!(path, "M {:.2} {:.2}", points[0].0, baseline).expect("string write");
            for (x, y) in &points {
                write!(path, " L {:.2} {:.2}", x, y).expect("string write");
            }
            write!(
                path,
                " L {:.2} {:.2} Z",
                points[points.len() - 1].0,
                baseline
            )
            .expect("string write");
            path_element(out, &path, &color, "none", 0.0, 0.18);
        }
        let mut path = String::with_capacity(points.len() * 24);
        for (index, (x, y)) in points.iter().enumerate() {
            write!(
                path,
                "{} {:.2} {:.2}",
                if index == 0 { "M" } else { "L" },
                x,
                y
            )
            .expect("string write");
        }
        path_element(out, &path, "none", &color, 1.5, 1.0);
        for (category, (x, y)) in points.into_iter().enumerate() {
            circle(out, x, y, 2.2, &color);
            if chart.style.show_data_labels {
                text(
                    out,
                    (x, y - 4.0),
                    5.2,
                    "#333333",
                    "middle",
                    &format_value(value_at(chart, series_index, category)),
                    false,
                );
            }
        }
    }
}

fn draw_scatter(
    out: &mut String,
    chart: &Chart,
    plot: Plot,
    categories: usize,
    series: usize,
    axis: &ScatterAxis,
) {
    for series_index in 0..series {
        let color = series_color(chart.series[series_index].color.as_deref(), series_index);
        for category in 0..categories {
            let x_value = axis.values[category];
            let x =
                plot.left + (plot.right - plot.left) * (x_value - axis.min) / (axis.max - axis.min);
            let value = value_at(chart, series_index, category);
            let y = y_value(value, plot);
            circle(out, x, y, 2.5, &color);
            if chart.style.show_data_labels {
                text(
                    out,
                    (x, y - 4.0),
                    5.0,
                    "#333333",
                    "middle",
                    &format_value(value),
                    false,
                );
            }
        }
    }
}

fn value_range(chart: &Chart, categories: usize, series: usize, include_zero: bool) -> (f64, f64) {
    let mut min = if include_zero { 0.0 } else { f64::INFINITY };
    let mut max = if include_zero { 0.0 } else { f64::NEG_INFINITY };
    for series_index in 0..series {
        for category in 0..categories {
            let value = value_at(chart, series_index, category);
            min = min.min(value);
            max = max.max(value);
        }
    }
    if let Some(axis_min) = chart
        .value_axis
        .min
        .filter(|value| value.is_finite() && value.abs() <= 1.0e100)
    {
        min = axis_min;
    }
    if let Some(axis_max) = chart
        .value_axis
        .max
        .filter(|value| value.is_finite() && value.abs() <= 1.0e100)
    {
        max = axis_max;
    }
    if !min.is_finite() || !max.is_finite() {
        return (0.0, 1.0);
    }
    if max <= min {
        let padding = min.abs().max(1.0) * 0.1;
        return (min - padding, max + padding);
    }
    let padding = (max - min) * 0.05;
    (min - padding, max + padding)
}

pub(super) fn value_at(chart: &Chart, series: usize, category: usize) -> f64 {
    chart.series[series]
        .values
        .get(category)
        .copied()
        .unwrap_or(0.0)
}

fn map_value(value: f64, min: f64, max: f64) -> f64 {
    ((value - min) / (max - min)).clamp(0.0, 1.0)
}

fn y_value(value: f64, plot: Plot) -> f64 {
    plot.bottom - map_value(value, plot.min, plot.max) * (plot.bottom - plot.top)
}

fn x_value(value: f64, plot: Plot) -> f64 {
    plot.left + map_value(value, plot.min, plot.max) * (plot.right - plot.left)
}
