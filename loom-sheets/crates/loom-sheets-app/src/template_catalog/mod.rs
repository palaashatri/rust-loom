//! Original Loom Sheets template catalog builders for Personal Finance,
//! Personal, Business, and Education categories (template indices 11–26).

mod business;
mod education;
mod finance;
mod personal;

use loom_sheets_core::style::{CellBorders, CellStyle, FillColor};
use loom_sheets_core::{CellRef, NumberFormat, Sheet};

/// Build one of the 16 expanded catalog templates (`11..=26`), or `None` for
/// indices handled by the core starter set (`0..=10`).
pub(crate) fn catalog_template_sheet(idx: i32) -> Option<Sheet> {
    let sheet = match idx {
        11 => finance::loan_amortization(),
        12 => finance::retirement_planner(),
        13 => finance::debt_payoff(),
        14 => finance::tax_summary(),
        15 => personal::travel_itinerary(),
        16 => personal::habit_workout_log(),
        17 => personal::event_budget(),
        18 => personal::meal_planner(),
        19 => business::profit_and_loss(),
        20 => business::employee_timesheet(),
        21 => business::inventory_tracker(),
        22 => business::project_schedule(),
        23 => education::gradebook(),
        24 => education::attendance_tracker(),
        25 => education::gpa_calculator(),
        26 => education::lab_data_and_chart(),
        _ => return None,
    };
    Some(sheet)
}

pub(super) fn build_sheet(name: &str, cells: &[(&str, &str)], widths: &[(u32, f32)]) -> Sheet {
    let mut sheet = Sheet::new(name);
    for (cell, raw) in cells {
        sheet.set_str(cell, raw);
    }
    for &(col, width) in widths {
        sheet.set_col_width(col, width);
    }
    sheet
}

fn style_cells(sheet: &mut Sheet, refs: &[&str], edit: impl Fn(&mut CellStyle)) {
    for a1 in refs {
        if let Some(cell) = CellRef::parse(a1) {
            let mut style = sheet.cell_style(cell);
            edit(&mut style);
            sheet.set_cell_style(cell, style);
        }
    }
}

pub(super) fn set_header_row(sheet: &mut Sheet, refs: &[&str], fill: FillColor) {
    style_cells(sheet, refs, |s| {
        s.bold = true;
        s.fill = fill;
        s.borders = CellBorders::ALL;
    });
}

pub(super) fn set_total_row(sheet: &mut Sheet, refs: &[&str]) {
    style_cells(sheet, refs, |s| {
        s.bold = true;
        s.fill = FillColor::Gray;
        s.borders = CellBorders::ALL;
    });
}

pub(super) fn set_format(
    sheet: &mut Sheet,
    refs: &[&str],
    format: NumberFormat,
    decimals: Option<u8>,
) {
    style_cells(sheet, refs, |s| {
        s.number_format = format;
        s.decimal_places = decimals;
    });
}
