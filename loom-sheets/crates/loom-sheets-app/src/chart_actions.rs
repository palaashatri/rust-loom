//! Chart projection helpers for Loom Sheets.

use std::rc::Rc;

use loom_sheets_core::{CellRange, CellRef, ChartKind, ChartSeries, ChartSpec, Sheet};
use slint::{SharedString, VecModel};

use crate::analysis::{
    bar_plot_geometry, chart_points, line_path_commands, normalize_plot_values, pie_wedge_commands,
};
use crate::SheetsApp;

fn clear_chart_points(app: &SheetsApp) {
    app.set_chart_categories(Rc::new(VecModel::<SharedString>::from(Vec::new())).into());
    app.set_chart_values_display(Rc::new(VecModel::<SharedString>::from(Vec::new())).into());
    app.set_chart_normalized(Rc::new(VecModel::<f32>::from(Vec::new())).into());
    app.set_chart_bar_baseline_fraction(1.0);
    app.set_chart_line_commands(SharedString::default());
    app.set_chart_pie_commands(Rc::new(VecModel::<SharedString>::from(Vec::new())).into());
    app.set_chart_pie_opacities(Rc::new(VecModel::<f32>::from(Vec::new())).into());
}

fn hide_chart(app: &SheetsApp, announcement: String) {
    let was_focused = app.get_chart_focused();
    clear_chart_points(app);
    if was_focused {
        app.set_status_left(announcement.into());
        app.set_chart_focused(false);
        app.invoke_focus_grid();
    }
    app.set_chart_visible(false);
}

fn numeric_axis_value(raw: &str) -> Option<f64> {
    let raw = raw.trim();
    let raw = raw.strip_prefix('$').unwrap_or(raw).trim();
    let text = raw.strip_suffix('%').unwrap_or(raw).trim();
    if text.contains(',') {
        let integer_end = text
            .find(['.', 'e', 'E'])
            .unwrap_or(text.len());
        if text[integer_end..].contains(',') {
            return None;
        }
        let integer = text[..integer_end].trim_start_matches(['+', '-']);
        let groups: Vec<_> = integer.split(',').collect();
        if groups.is_empty()
            || !(1..=3).contains(&groups[0].len())
            || !groups[0].bytes().all(|byte| byte.is_ascii_digit())
            || groups
                .iter()
                .skip(1)
                .any(|group| group.len() != 3 || !group.bytes().all(|byte| byte.is_ascii_digit()))
        {
            return None;
        }
    }
    text.replace(',', "")
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
}

/// Synchronize active chart data from the worksheet model to the Slint UI.
pub(crate) fn sync_chart_to_app(app: &SheetsApp, sheet: &Sheet) {
    let Some(chart) = sheet.chart.clone() else {
        let was_focused = app.get_chart_focused();
        clear_chart_points(app);
        app.set_chart_source_range(SharedString::default());
        app.set_chart_series_label("Values".into());
        app.set_chart_unit_label("Not specified".into());
        app.set_chart_title("Chart".into());
        if was_focused {
            app.set_status_left(
                "Chart unavailable because this worksheet no longer contains a chart.".into(),
            );
            app.set_chart_focused(false);
            app.invoke_focus_grid();
        }
        app.set_chart_visible(false);
        return;
    };
    if !app.get_chart_visible() {
        return;
    }
    let end_row = chart
        .end_row
        .unwrap_or(sheet.dimensions().rows.saturating_sub(1));
    let source = CellRange::new(
        CellRef {
            row: chart.start_row.saturating_sub(1),
            col: chart.cat_col,
        },
        CellRef {
            row: end_row,
            col: chart.val_col,
        },
    )
    .to_a1();
    let header = sheet.cells.get(&CellRef {
        row: chart.start_row.saturating_sub(1),
        col: chart.val_col,
    });
    let column_header = header
        .map(|cell| cell.raw.as_str())
        .filter(|label| !label.is_empty())
        .unwrap_or("Values");
    let series = column_header
        .split_once('(')
        .map(|(label, _)| label.trim())
        .filter(|label| !label.is_empty())
        .unwrap_or(column_header);
    let unit = column_header
        .split_once('(')
        .and_then(|(_, suffix)| suffix.strip_suffix(')'))
        .map(str::trim)
        .filter(|label| !label.is_empty())
        .unwrap_or("Not specified");
    app.set_chart_source_range(source.clone().into());
    app.set_chart_series_label(series.into());
    app.set_chart_unit_label(unit.into());
    app.set_chart_title(SharedString::from(chart.title.as_str()));
    app.set_chart_kind(SharedString::from(chart.kind.as_str()));
    let mut points = chart_points(sheet, &chart);
    let mut scatter_x = Vec::new();
    if chart.kind == ChartKind::Scatter {
        let mut valid_points = Vec::with_capacity(points.len());
        for point in points {
            if let Some(x) = numeric_axis_value(&point.0) {
                scatter_x.push(x);
                valid_points.push(point);
            }
        }
        points = valid_points;
    }
    if points.is_empty() {
        let reason = if chart.kind == ChartKind::Scatter {
            "no numeric X/Y pairs"
        } else {
            "no plottable data points"
        };
        hide_chart(
            app,
            format!("Chart hidden because source range {source} has {reason}."),
        );
        return;
    }
    let categories: Vec<SharedString> = points
        .iter()
        .map(|(cat, _, _)| SharedString::from(cat))
        .collect();
    let values: Vec<f64> = points.iter().map(|(_, num, _)| *num).collect();
    let display_values: Vec<SharedString> = points
        .iter()
        .map(|(_, _, raw)| SharedString::from(raw))
        .collect();
    let normalized_y = normalize_plot_values(&values, 1.0);
    let bar_geometry = bar_plot_geometry(&values);
    let spec = ChartSpec {
        kind: chart.kind,
        title: chart.title.clone(),
        series: vec![ChartSeries {
            name: "Series 1".into(),
            categories: points.iter().map(|(cat, _, _)| cat.clone()).collect(),
            values,
        }],
    };
    let Ok(_) = spec.normalized_points() else {
        hide_chart(
            app,
            format!("Chart hidden because source range {source} cannot be plotted."),
        );
        return;
    };
    let norm = if chart.kind == ChartKind::Bar {
        bar_geometry.signed_heights.clone()
    } else if chart.kind == ChartKind::Scatter {
        let normalized_x = normalize_plot_values(&scatter_x, 0.5);
        normalized_y
            .iter()
            .zip(normalized_x.iter())
            .flat_map(|(y, x)| [*y, *x])
            .collect()
    } else {
        normalized_y
    };
    app.set_chart_categories(Rc::new(VecModel::from(categories)).into());
    app.set_chart_values_display(Rc::new(VecModel::from(display_values)).into());
    app.set_chart_normalized(Rc::new(VecModel::from(norm.clone())).into());
    app.set_chart_bar_baseline_fraction(bar_geometry.baseline);
    app.set_chart_line_commands(SharedString::from(line_path_commands(&norm)));
    let wedges: Vec<SharedString> =
        pie_wedge_commands(&points.iter().map(|(_, num, _)| *num).collect::<Vec<_>>())
            .into_iter()
            .map(SharedString::from)
            .collect();
    app.set_chart_pie_commands(Rc::new(VecModel::from(wedges)).into());
    let opacities: Vec<f32> = (0..points.len())
        .map(|i| 1.0 - (i % 5) as f32 * 0.15)
        .collect();
    app.set_chart_pie_opacities(Rc::new(VecModel::from(opacities)).into());
}
