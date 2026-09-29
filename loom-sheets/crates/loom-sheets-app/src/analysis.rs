//! Derived analysis for Loom Sheets: chart planning, SVG path building,
//! pivot-summary planning, and tab-name allocation. Pure sheet-data logic;
//! Slint wiring stays in `actions`.

use loom_sheets_core::{evaluate, CellRef, ChartKind, PivotAggregation, Sheet, SheetChart};

use crate::{cell_value, selection_from_app, SheetsApp};

/// A pivot formula builder: group key in, live cross-sheet formula out.
type PivotFormula = Box<dyn Fn(&str) -> String>;

/// Plot extents for a bar series using a shared zero baseline.
#[derive(Debug, PartialEq)]
pub(crate) struct BarPlotGeometry {
    pub(crate) baseline: f32,
    pub(crate) signed_heights: Vec<f32>,
}

pub(crate) fn bar_plot_geometry(values: &[f64]) -> BarPlotGeometry {
    let scale = values
        .iter()
        .map(|value| value.abs())
        .fold(0.0_f64, f64::max);
    if scale == 0.0 || !scale.is_finite() {
        return BarPlotGeometry {
            baseline: 1.0,
            signed_heights: vec![0.0; values.len()],
        };
    }

    // Scaling before subtraction keeps finite, opposite-sign extremes from
    // overflowing their range (for example -1e308..1e308).
    let min = values
        .iter()
        .map(|value| value / scale)
        .fold(0.0_f64, f64::min);
    let max = values
        .iter()
        .map(|value| value / scale)
        .fold(0.0_f64, f64::max);
    let span = max - min;
    let baseline = (max / span) as f32;
    let signed_heights = values
        .iter()
        .map(|value| ((value / scale) / span) as f32)
        .collect();
    BarPlotGeometry {
        baseline,
        signed_heights,
    }
}

/// Normalize finite values to the unit axis without overflowing a wide span.
/// `constant_value` selects the coordinate for a series with no range: line Y
/// values use the top of the plot, while scatter X values use its center.
pub(crate) fn normalize_plot_values(values: &[f64], constant_value: f32) -> Vec<f32> {
    let scale = values
        .iter()
        .filter(|value| value.is_finite())
        .map(|value| value.abs())
        .fold(0.0_f64, f64::max);
    if scale == 0.0 || !scale.is_finite() {
        return vec![constant_value; values.len()];
    }
    let min = values
        .iter()
        .map(|value| value / scale)
        .fold(f64::INFINITY, f64::min);
    let max = values
        .iter()
        .map(|value| value / scale)
        .fold(f64::NEG_INFINITY, f64::max);
    let span = max - min;
    if span <= 0.0 || !span.is_finite() {
        return vec![constant_value; values.len()];
    }
    values
        .iter()
        .map(|value| (((value / scale) - min) / span).clamp(0.0, 1.0) as f32)
        .collect()
}

/// Pick label/value source columns: the selection's first two columns when
/// it spans a pair inside the used range, else columns A (labels) and B.
/// Shared by chart insert and pivot summary so both read the same columns.
pub(crate) fn label_value_columns(app: &SheetsApp, sheet: &Sheet) -> (u32, u32) {
    let sel = selection_from_app(app);
    let range = sel.range();
    let min_col = range.start.col.min(range.end.col);
    let max_col = range.start.col.max(range.end.col);
    let dims = sheet.dimensions();
    if max_col > min_col && max_col < dims.cols {
        (min_col, min_col + 1)
    } else {
        (0, 1)
    }
}

/// Plan a live-linked chart from explicit source columns: labels from
/// `cat_col`, numbers from `val_col`, skipping the header row. Pure and
/// unit-testable; the handlers below persist the plan as undoable edits.
/// A unique tab name based on `base`, appending " 2", " 3", ... on clash
/// (case-insensitive, mirroring rename validation).
pub(crate) fn unique_sheet_name(base: &str, sheets: &[Sheet]) -> String {
    if !sheets
        .iter()
        .any(|sheet| sheet.name.eq_ignore_ascii_case(base))
    {
        return base.to_string();
    }
    let mut counter = 2u32;
    loop {
        let candidate = format!("{base} {counter}");
        if !sheets
            .iter()
            .any(|sheet| sheet.name.eq_ignore_ascii_case(&candidate))
        {
            return candidate;
        }
        counter += 1;
    }
}

/// Plan a live pivot-summary sheet: one row per distinct key in `cat_col`
/// with a cross-sheet aggregation formula over `val_col`, so the summary
/// recomputes when source data changes. Returns the sheet and group count.
/// Pure and unit-testable; the handler persists it as an undoable tab add.
pub(crate) fn plan_pivot_sheet(
    sheet: &Sheet,
    cat_col: u32,
    val_col: u32,
    aggregation: PivotAggregation,
) -> Result<(Sheet, usize), String> {
    use loom_sheets_core::refs::quote_sheet_name;

    let dims = sheet.dimensions();
    if dims.rows < 2 {
        return Err("Pivot needs at least one data row below the header".to_string());
    }
    let vals = evaluate(sheet);
    let col_letter = |col: u32| {
        CellRef { row: 0, col }
            .to_a1()
            .trim_end_matches('1')
            .to_string()
    };
    // Groups in first-appearance order with one numeric witness each.
    let mut order: Vec<String> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for row in 1..dims.rows {
        let key = cell_value(sheet, &vals, row, cat_col);
        let raw = cell_value(sheet, &vals, row, val_col);
        let clean = raw.trim().trim_start_matches('$').trim_end_matches('%');
        if clean.parse::<f64>().is_ok() && seen.insert(key.clone()) {
            order.push(key);
        }
    }
    if order.is_empty() {
        return Err(format!(
            "Pivot needs labels in column {} and numbers in column {}",
            col_letter(cat_col),
            col_letter(val_col)
        ));
    }
    let qualifier = quote_sheet_name(&sheet.name);
    // 1-based data extent for formula ranges (header row 1 skipped).
    let first_data_row = 2u32;
    let last_data_row = dims.rows;
    let cat_range = format!(
        "{qualifier}!${}${first_data_row}:${}${last_data_row}",
        col_letter(cat_col),
        col_letter(cat_col),
    );
    let val_range = format!(
        "{qualifier}!${}${first_data_row}:${}${last_data_row}",
        col_letter(val_col),
        col_letter(val_col),
    );
    let (agg_label, formula_for): (&str, PivotFormula) = match aggregation {
        PivotAggregation::Sum => (
            "Sum",
            Box::new(move |key: &str| format!("=SUMIF({cat_range},\"{key}\",{val_range})")),
        ),
        PivotAggregation::Count => (
            "Count",
            Box::new(move |key: &str| format!("=COUNTIF({cat_range},\"{key}\")")),
        ),
        PivotAggregation::Average => (
            "Average",
            Box::new(move |key: &str| format!("=AVERAGEIF({cat_range},\"{key}\",{val_range})")),
        ),
        PivotAggregation::Min => (
            "Min",
            Box::new(move |key: &str| format!("=MINIFS({val_range},{cat_range},\"{key}\")")),
        ),
        PivotAggregation::Max => (
            "Max",
            Box::new(move |key: &str| format!("=MAXIFS({val_range},{cat_range},\"{key}\")")),
        ),
    };
    let mut pivot = Sheet::new(&format!("Pivot of {}", sheet.name));
    pivot.set_str("A1", "Category");
    pivot.set_str("B1", &format!("{agg_label} of {}", col_letter(val_col)));
    for (index, key) in order.iter().enumerate() {
        let row = index + 2;
        // Criteria with doubled quotes stay valid formula strings.
        let criterion = key.replace('"', "\"\"");
        pivot.set_str(&format!("A{row}"), key);
        pivot.set_str(&format!("B{row}"), &formula_for(&criterion));
    }
    let groups = order.len();
    Ok((pivot, groups))
}

/// Plan a live-linked chart from explicit source columns: labels from
/// `cat_col`, numbers from `val_col`, skipping the header row. Pure and
/// unit-testable; the handlers below persist the plan as undoable edits.
pub(crate) fn plan_chart(sheet: &Sheet, cat_col: u32, val_col: u32) -> Result<SheetChart, String> {
    plan_chart_in_range(
        sheet,
        cat_col,
        val_col,
        1,
        sheet.dimensions().rows.saturating_sub(1),
    )
}

pub(crate) fn plan_chart_in_range(
    sheet: &Sheet,
    cat_col: u32,
    val_col: u32,
    start_row: u32,
    end_row: u32,
) -> Result<SheetChart, String> {
    if end_row < start_row || start_row == 0 {
        return Err("Chart needs at least one data row below the header".to_string());
    }
    let vals = evaluate(sheet);
    let cat_letter = CellRef {
        row: 0,
        col: cat_col,
    }
    .to_a1();
    let cat_letter = cat_letter.trim_end_matches('1');
    let val_letter = CellRef {
        row: 0,
        col: val_col,
    }
    .to_a1();
    let val_letter = val_letter.trim_end_matches('1');
    let mut points = 0usize;
    for row in start_row..=end_row {
        let raw = cell_value(sheet, &vals, row, val_col);
        let clean = raw.trim().trim_start_matches('$').trim_end_matches('%');
        if clean.parse::<f64>().is_ok() {
            points += 1;
        }
    }
    if points == 0 {
        return Err(format!(
            "Chart needs labels in column {cat_letter} and numbers in column {val_letter}"
        ));
    }
    Ok(SheetChart {
        kind: ChartKind::Bar,
        title: format!("{} Chart", sheet.name),
        cat_col,
        val_col,
        start_row,
        end_row: Some(end_row),
    })
}

/// Collect live (category, value, display) triples for a chart plan.
pub(crate) fn chart_points(sheet: &Sheet, chart: &SheetChart) -> Vec<(String, f64, String)> {
    let vals = evaluate(sheet);
    let dims = sheet.dimensions();
    let mut out = Vec::new();
    for row in chart.start_row
        ..=chart
            .end_row
            .unwrap_or(dims.rows.saturating_sub(1))
            .min(dims.rows.saturating_sub(1))
    {
        let cat = cell_value(sheet, &vals, row, chart.cat_col);
        let raw = cell_value(sheet, &vals, row, chart.val_col);
        let clean = raw.trim().trim_start_matches('$').trim_end_matches('%');
        if let Ok(num) = clean.parse::<f64>() {
            if num.is_finite() {
                out.push((cat, num, raw));
            }
        }
    }
    out
}

/// SVG path for a line series in a 100x60 plot space.
pub(crate) fn line_path_commands(normalized: &[f32]) -> String {
    if normalized.is_empty() {
        return String::new();
    }
    let mut path = String::from("M");
    for (i, &n) in normalized.iter().enumerate() {
        let x = if normalized.len() == 1 {
            50.0
        } else {
            i as f32 * 100.0 / (normalized.len() - 1) as f32
        };
        let y = 58.0 - n.clamp(0.0, 1.0) * 54.0;
        if i > 0 {
            path.push_str(" L");
        }
        path.push_str(&format!("{x:.1},{y:.1}"));
    }
    path
}

/// SVG wedge paths for a pie series in a 100x100 space (center 50,50, r 46).
/// Non-positive values contribute no angle. An all-nonpositive series has no
/// slices; the chart UI explains why instead of inventing a proportion.
pub(crate) fn pie_wedge_commands(values: &[f64]) -> Vec<String> {
    let maximum = values
        .iter()
        .copied()
        .filter(|value| value.is_finite() && *value > 0.0)
        .fold(0.0, f64::max);
    if maximum == 0.0 {
        return Vec::new();
    }
    let positive_count = values
        .iter()
        .filter(|value| value.is_finite() && **value > 0.0)
        .count();
    let scaled: Vec<f64> = values
        .iter()
        .map(|value| {
            if value.is_finite() && *value > 0.0 {
                *value / maximum
            } else {
                0.0
            }
        })
        .collect();
    let total: f64 = scaled.iter().sum();
    if positive_count == 1 {
        return values
            .iter()
            .map(|value| {
                if value.is_finite() && *value > 0.0 {
                    "M50,50 L50,4 A46,46 0 1,1 50,96 A46,46 0 1,1 50,4 Z".to_string()
                } else {
                    String::new()
                }
            })
            .collect();
    }
    let mut wedges = Vec::with_capacity(values.len());
    let mut angle = -std::f64::consts::FRAC_PI_2;
    for value in scaled {
        let span = value / total * std::f64::consts::TAU;
        if span <= 0.0 {
            wedges.push(String::new());
            continue;
        }
        let end = angle + span;
        let (x1, y1) = (50.0 + 46.0 * angle.cos(), 50.0 + 46.0 * angle.sin());
        let (x2, y2) = (50.0 + 46.0 * end.cos(), 50.0 + 46.0 * end.sin());
        if (x1 * 10.0).round() == (x2 * 10.0).round() && (y1 * 10.0).round() == (y2 * 10.0).round()
        {
            let middle = angle + span / 2.0;
            let (xm, ym) = (50.0 + 46.0 * middle.cos(), 50.0 + 46.0 * middle.sin());
            wedges.push(format!(
                "M50,50 L{x1:.1},{y1:.1} A46,46 0 0,1 {xm:.1},{ym:.1} A46,46 0 0,1 {x2:.1},{y2:.1} Z"
            ));
        } else {
            let large = if span > std::f64::consts::PI { 1 } else { 0 };
            wedges.push(format!(
                "M50,50 L{x1:.1},{y1:.1} A46,46 0 {large},1 {x2:.1},{y2:.1} Z"
            ));
        }
        angle = end;
    }
    wedges
}
