//! Chart projection helpers for Loom Sheets.

use std::rc::Rc;

use loom_sheets_core::{CellRange, CellRef, ChartSeries, ChartSpec, Sheet};
use slint::{SharedString, VecModel};

use crate::analysis::{chart_points, line_path_commands, pie_wedge_commands};
use crate::SheetsApp;

/// Synchronize active chart data from the worksheet model to the Slint UI.
pub(crate) fn sync_chart_to_app(app: &SheetsApp, sheet: &Sheet) {
    let Some(chart) = sheet.chart.clone() else {
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
    app.set_chart_source_range(source.into());
    app.set_chart_series_label(series.into());
    app.set_chart_unit_label(unit.into());
    let points = chart_points(sheet, &chart);
    if points.is_empty() {
        // Source data was deleted after the chart was inserted: hide rather
        // than show a stale or empty plot.
        app.set_chart_visible(false);
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
    let spec = ChartSpec {
        kind: chart.kind,
        title: chart.title.clone(),
        series: vec![ChartSeries {
            name: "Series 1".into(),
            categories: points.iter().map(|(cat, _, _)| cat.clone()).collect(),
            values,
        }],
    };
    let Ok(normalized_series) = spec.normalized_points() else {
        app.set_chart_visible(false);
        return;
    };
    let norm = normalized_series[0]
        .iter()
        .map(|&v| v as f32)
        .collect::<Vec<f32>>();
    app.set_chart_title(SharedString::from(chart.title));
    app.set_chart_kind(SharedString::from(chart.kind.as_str()));
    app.set_chart_categories(Rc::new(VecModel::from(categories)).into());
    app.set_chart_values_display(Rc::new(VecModel::from(display_values)).into());
    app.set_chart_normalized(Rc::new(VecModel::from(norm.clone())).into());
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
