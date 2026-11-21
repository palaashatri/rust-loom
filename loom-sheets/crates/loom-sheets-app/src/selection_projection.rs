//! Selection metadata and cell formatting controls projected into the inspector.

use loom_sheets_core::{
    CellAlignment, CellRange, CellRef, GridSelection, NumberFormat, Sheet, Value,
};
use slint::SharedString;

use crate::{cell_value, formatting, SheetsApp};

pub(crate) fn update_selection_range(
    app: &SheetsApp,
    sheet: &Sheet,
    vals: &std::collections::HashMap<CellRef, Value>,
    selection: GridSelection,
) {
    let selected = selection.focus;
    let range = selection.range();
    let formula = sheet
        .raw(selected)
        .map(SharedString::from)
        .unwrap_or_default();
    app.set_inspector_formula(if formula.starts_with('=') {
        formula.clone()
    } else {
        SharedString::new()
    });
    app.set_selection_formula(formula);
    app.invoke_reset_formula_edit_buffer();
    app.set_selected_cell(selected.to_a1().into());
    app.set_selected_row(selected.row as i32);
    app.set_selected_col(selected.col as i32);
    app.set_selection_anchor_row(selection.anchor.row as i32);
    app.set_selection_anchor_col(selection.anchor.col as i32);
    app.set_selection_start_row(range.start.row as i32);
    app.set_selection_start_col(range.start.col as i32);
    app.set_selection_end_row(range.end.row as i32);
    app.set_selection_end_col(range.end.col as i32);
    app.set_selection_range(range.to_a1().into());
    let span_rows = (range.end.row - range.start.row + 1) as u64;
    let span_cols = (range.end.col - range.start.col + 1) as u64;
    let cell_count = span_rows.saturating_mul(span_cols);
    app.set_selection_count((cell_count.min(i32::MAX as u64) as i32).max(1));
    let align_code = match sheet.cell_alignment(selected) {
        CellAlignment::General | CellAlignment::Left => 0,
        CellAlignment::Center => 1,
        CellAlignment::Right => 2,
    };
    app.set_cell_alignment(align_code);
    let style = sheet.cell_style(selected);
    app.set_cell_bold(style.bold);
    app.set_cell_italic(style.italic);
    app.set_cell_underline(style.underline);
    let fmt_code = match style.number_format {
        NumberFormat::General
        | NumberFormat::PlainText
        | NumberFormat::Scientific
        | NumberFormat::DateIso => 0,
        NumberFormat::Currency => 1,
        NumberFormat::Percentage => 2,
        NumberFormat::Number => 3,
    };
    app.set_cell_format(fmt_code);
    let decimals = style.decimal_places.unwrap_or(2);
    app.set_cell_decimals(decimals as i32);
    app.set_cell_decimals_label(SharedString::from(decimals.to_string()));
    app.set_cell_border(style.borders.any());
    app.set_cell_fill(style.fill.swatch_index());
    crate::ink_actions::publish_selected(app, sheet, range, style);
    crate::rich_format_actions::publish_selected(app, sheet, range);
    let font_size = formatting::effective_font_size(style);
    app.set_cell_font_size(font_size as i32);
    app.set_cell_font_label(SharedString::from(font_size.to_string()));

    let raw_display = cell_value(sheet, vals, selected.row, selected.col);
    let display = style.format_value(&raw_display);
    let formula_text = sheet.raw(selected).unwrap_or("");
    let value_text = if display.is_empty() {
        "Empty".to_string()
    } else {
        display.clone()
    };
    let formula_suffix = if formula_text.is_empty() {
        String::new()
    } else {
        format!("; formula: {formula_text}")
    };
    app.set_selection_announcement(SharedString::from(format!(
        "{} selected; value: {}{}",
        selection.label(),
        value_text,
        formula_suffix
    )));
    app.set_selection_value(SharedString::from(display));
    match format_selection_summary(sheet, vals, range) {
        Some(summary) => app.set_status_right(SharedString::from(summary)),
        None => app.set_status_right(SharedString::new()),
    }
}

fn format_stat_number(value: f64) -> String {
    let normalized = if value.abs() < 1e-12 { 0.0 } else { value };
    if (normalized - normalized.round()).abs() < 1e-9 && normalized.abs() < 1e12 {
        return format!("{}", normalized.round() as i64);
    }
    let formatted = format!("{normalized:.2}");
    let trimmed = formatted.trim_end_matches('0').trim_end_matches('.');
    if trimmed == "-0" {
        "0".to_string()
    } else {
        trimmed.to_string()
    }
}

fn numeric_cell_value(
    sheet: &Sheet,
    vals: &std::collections::HashMap<CellRef, Value>,
    cell: CellRef,
) -> Option<f64> {
    match vals.get(&cell) {
        Some(Value::Number(n)) if n.is_finite() => return Some(*n),
        Some(Value::Empty) | None => {}
        Some(_) => return None,
    }
    let raw = sheet.raw(cell)?.trim();
    if raw.is_empty() || raw.starts_with('=') {
        return None;
    }
    let clean = raw.trim_start_matches('$').trim_end_matches('%');
    clean.parse::<f64>().ok().filter(|n| n.is_finite())
}

pub(crate) fn format_selection_summary(
    sheet: &Sheet,
    vals: &std::collections::HashMap<CellRef, Value>,
    range: CellRange,
) -> Option<String> {
    if range.start == range.end {
        return None;
    }
    let span_rows = (range.end.row - range.start.row + 1) as u64;
    let span_cols = (range.end.col - range.start.col + 1) as u64;
    let total_cells = span_rows.saturating_mul(span_cols);

    let mut sum = 0.0_f64;
    let mut count = 0usize;

    if total_cells <= 4096 {
        for row in range.start.row..=range.end.row {
            for col in range.start.col..=range.end.col {
                if let Some(n) = numeric_cell_value(sheet, vals, CellRef { row, col }) {
                    sum += n;
                    count += 1;
                }
            }
        }
    } else {
        for (cell, _) in sheet.cells.iter() {
            if cell.row >= range.start.row
                && cell.row <= range.end.row
                && cell.col >= range.start.col
                && cell.col <= range.end.col
            {
                if let Some(n) = numeric_cell_value(sheet, vals, *cell) {
                    sum += n;
                    count += 1;
                }
            }
        }
        for (cell, value) in vals.iter() {
            if cell.row >= range.start.row
                && cell.row <= range.end.row
                && cell.col >= range.start.col
                && cell.col <= range.end.col
                && !sheet.cells.contains_key(cell)
            {
                if let Value::Number(n) = value {
                    if n.is_finite() {
                        sum += *n;
                        count += 1;
                    }
                }
            }
        }
    }

    if count == 0 {
        return None;
    }
    let average = sum / count as f64;
    Some(format!(
        "Sum: {}  Avg: {}  Count: {count}",
        format_stat_number(sum),
        format_stat_number(average)
    ))
}

#[cfg(test)]
mod tests {
    use super::format_selection_summary;
    use loom_sheets_core::{evaluate, CellRange, CellRef, Sheet};

    #[test]
    fn single_cell_selection_has_no_summary() {
        let mut sheet = Sheet::new("S");
        sheet.set_str("A1", "42");
        let vals = evaluate(&sheet);
        let single = CellRange {
            start: CellRef::parse("A1").unwrap(),
            end: CellRef::parse("A1").unwrap(),
        };
        assert_eq!(format_selection_summary(&sheet, &vals, single), None);
    }

    #[test]
    fn multi_cell_numeric_selection_computes_sum_avg_count() {
        let mut sheet = Sheet::new("S");
        sheet.set_str("A1", "10");
        sheet.set_str("A2", "20");
        sheet.set_str("A3", "label");
        sheet.set_str("A4", "=A1+A2");
        let vals = evaluate(&sheet);
        let range = CellRange {
            start: CellRef::parse("A1").unwrap(),
            end: CellRef::parse("A4").unwrap(),
        };
        assert_eq!(
            format_selection_summary(&sheet, &vals, range).as_deref(),
            Some("Sum: 60  Avg: 20  Count: 3")
        );
    }

    #[test]
    fn multi_cell_non_numeric_selection_returns_none() {
        let mut sheet = Sheet::new("S");
        sheet.set_str("A1", "hello");
        sheet.set_str("A2", "world");
        let vals = evaluate(&sheet);
        let range = CellRange {
            start: CellRef::parse("A1").unwrap(),
            end: CellRef::parse("A2").unwrap(),
        };
        assert_eq!(format_selection_summary(&sheet, &vals, range), None);
    }

    #[test]
    fn large_sparse_selection_uses_sparse_scan() {
        let mut sheet = Sheet::new("S");
        sheet.set_str("A1", "12.5");
        sheet.set_str("Z500", "7.5");
        let vals = evaluate(&sheet);
        let range = CellRange {
            start: CellRef::parse("A1").unwrap(),
            end: CellRef::parse("Z500").unwrap(),
        };
        assert_eq!(
            format_selection_summary(&sheet, &vals, range).as_deref(),
            Some("Sum: 20  Avg: 10  Count: 2")
        );
    }
}
