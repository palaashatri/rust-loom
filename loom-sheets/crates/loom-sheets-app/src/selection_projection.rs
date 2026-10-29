//! Selection metadata and cell formatting controls projected into the inspector.

use loom_sheets_core::{CellAlignment, CellRef, GridSelection, NumberFormat, Sheet, Value};
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
    app.set_selection_count((range.cells().len() as i32).max(1));
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
    app.set_cell_border(style.border);
    app.set_cell_fill(style.fill.swatch_index());
    crate::ink_actions::publish_selected(app, style);
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
    let cell_count = range.cells().len();
    if cell_count > 1 {
        let mut sum = 0.0;
        let mut count = 0;
        for cell in range.cells() {
            let value = cell_value(sheet, vals, cell.row, cell.col);
            let clean = value.trim().trim_start_matches('$').trim_end_matches('%');
            if let Ok(number) = clean.parse::<f64>() {
                sum += number;
                count += 1;
            }
        }
        if count > 0 {
            let average = sum / count as f64;
            app.set_status_right(SharedString::from(format!(
                "SUM: {sum:.2}  AVG: {average:.2}  COUNT: {count}"
            )));
        }
    }
}
