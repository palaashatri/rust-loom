//! Original Loom Sheets template catalog builders for Personal Finance,
//! Personal, Business, and Education categories (template indices 11–26).

use loom_sheets_core::style::{CellStyle, FillColor};
use loom_sheets_core::{CellRef, ChartKind, NumberFormat, Sheet, SheetChart};

/// Build one of the 16 expanded catalog templates (`11..=26`), or `None` for
/// indices handled by the core starter set (`0..=10`).
pub(crate) fn catalog_template_sheet(idx: i32) -> Option<Sheet> {
    let sheet = match idx {
        11 => loan_amortization(),
        12 => retirement_planner(),
        13 => debt_payoff(),
        14 => tax_summary(),
        15 => travel_itinerary(),
        16 => habit_workout_log(),
        17 => event_budget(),
        18 => meal_planner(),
        19 => profit_and_loss(),
        20 => employee_timesheet(),
        21 => inventory_tracker(),
        22 => project_schedule(),
        23 => gradebook(),
        24 => attendance_tracker(),
        25 => gpa_calculator(),
        26 => lab_data_and_chart(),
        _ => return None,
    };
    Some(sheet)
}

fn build_sheet(name: &str, cells: &[(&str, &str)], widths: &[(u32, f32)]) -> Sheet {
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

fn set_header_row(sheet: &mut Sheet, refs: &[&str], fill: FillColor) {
    style_cells(sheet, refs, |s| {
        s.bold = true;
        s.fill = fill;
        s.borders = loom_sheets_core::style::CellBorders::ALL;
    });
}

fn set_total_row(sheet: &mut Sheet, refs: &[&str]) {
    style_cells(sheet, refs, |s| {
        s.bold = true;
        s.fill = FillColor::Gray;
        s.borders = loom_sheets_core::style::CellBorders::ALL;
    });
}

fn set_format(sheet: &mut Sheet, refs: &[&str], format: NumberFormat, decimals: Option<u8>) {
    style_cells(sheet, refs, |s| {
        s.number_format = format;
        s.decimal_places = decimals;
    });
}

fn loan_amortization() -> Sheet {
    let mut sheet = build_sheet(
        "Loan Amortization",
        &[
            ("A1", "Loan Parameters"),
            ("B1", "Value"),
            ("A2", "Loan Amount"),
            ("B2", "250000"),
            ("A3", "Annual Interest Rate"),
            ("B3", "0.055"),
            ("A4", "Term (Years)"),
            ("B4", "30"),
            ("A5", "Payments / Year"),
            ("B5", "12"),
            ("A6", "Monthly Payment"),
            ("B6", "=-PMT(B3/B5, B4*B5, B2)"),
            ("A7", "Total Payments"),
            ("B7", "=B6*B4*B5"),
            ("A8", "Total Interest"),
            ("B8", "=B7-B2"),
            ("A10", "Month"),
            ("B10", "Beginning"),
            ("C10", "Payment"),
            ("D10", "Principal"),
            ("E10", "Interest"),
            ("F10", "Ending"),
            ("A11", "1"),
            ("B11", "=B2"),
            ("C11", "=$B$6"),
            ("D11", "=C11-E11"),
            ("E11", "=B11*$B$3/$B$5"),
            ("F11", "=B11-D11"),
            ("A12", "2"),
            ("B12", "=F11"),
            ("C12", "=$B$6"),
            ("D12", "=C12-E12"),
            ("E12", "=B12*$B$3/$B$5"),
            ("F12", "=B12-D12"),
            ("A13", "3"),
            ("B13", "=F12"),
            ("C13", "=$B$6"),
            ("D13", "=C13-E13"),
            ("E13", "=B13*$B$3/$B$5"),
            ("F13", "=B13-D13"),
            ("A14", "4"),
            ("B14", "=F13"),
            ("C14", "=$B$6"),
            ("D14", "=C14-E14"),
            ("E14", "=B14*$B$3/$B$5"),
            ("F14", "=B14-D14"),
            ("A15", "4-Mo Subtotal"),
            ("C15", "=SUM(C11:C14)"),
            ("D15", "=SUM(D11:D14)"),
            ("E15", "=SUM(E11:E14)"),
        ],
        &[
            (0, 150.0),
            (1, 110.0),
            (2, 105.0),
            (3, 105.0),
            (4, 105.0),
            (5, 110.0),
        ],
    );
    set_header_row(&mut sheet, &["A1", "B1"], FillColor::Green);
    set_header_row(
        &mut sheet,
        &["A10", "B10", "C10", "D10", "E10", "F10"],
        FillColor::Green,
    );
    set_total_row(&mut sheet, &["A15", "B15", "C15", "D15", "E15", "F15"]);
    set_format(
        &mut sheet,
        &[
            "B2", "B6", "B7", "B8", "B11", "C11", "D11", "E11", "F11", "B12", "C12", "D12", "E12",
            "F12", "B13", "C13", "D13", "E13", "F13", "B14", "C14", "D14", "E14", "F14", "C15",
            "D15", "E15",
        ],
        NumberFormat::Currency,
        Some(2),
    );
    set_format(&mut sheet, &["B3"], NumberFormat::Percentage, Some(2));
    set_format(
        &mut sheet,
        &["B4", "B5", "A11", "A12", "A13", "A14"],
        NumberFormat::Number,
        Some(0),
    );
    sheet
}

fn retirement_planner() -> Sheet {
    let mut sheet = build_sheet(
        "Retirement Planner",
        &[
            ("A1", "Retirement Assumptions"),
            ("B1", "Value"),
            ("A2", "Current Age"),
            ("B2", "32"),
            ("A3", "Retirement Age"),
            ("B3", "65"),
            ("A4", "Current Savings"),
            ("B4", "45000"),
            ("A5", "Monthly Contribution"),
            ("B5", "850"),
            ("A6", "Expected Annual Return"),
            ("B6", "0.07"),
            ("A7", "Years to Retirement"),
            ("B7", "=B3-B2"),
            ("A8", "Projected Nest Egg"),
            ("B8", "=FV(B6/12, B7*12, -B5, -B4)"),
            ("A9", "Total Contributions"),
            ("B9", "=B4+B5*12*B7"),
            ("A10", "Investment Growth"),
            ("B10", "=B8-B9"),
            ("A12", "Horizon (Years)"),
            ("B12", "Projected Balance"),
            ("A13", "5"),
            ("B13", "=FV($B$6/12, A13*12, -$B$5, -$B$4)"),
            ("A14", "10"),
            ("B14", "=FV($B$6/12, A14*12, -$B$5, -$B$4)"),
            ("A15", "20"),
            ("B15", "=FV($B$6/12, A15*12, -$B$5, -$B$4)"),
            ("A16", "30"),
            ("B16", "=FV($B$6/12, A16*12, -$B$5, -$B$4)"),
        ],
        &[(0, 170.0), (1, 140.0)],
    );
    set_header_row(&mut sheet, &["A1", "B1"], FillColor::Blue);
    set_header_row(&mut sheet, &["A12", "B12"], FillColor::Blue);
    set_total_row(&mut sheet, &["A8", "B8"]);
    set_format(
        &mut sheet,
        &["B4", "B5", "B8", "B9", "B10", "B13", "B14", "B15", "B16"],
        NumberFormat::Currency,
        Some(0),
    );
    set_format(&mut sheet, &["B6"], NumberFormat::Percentage, Some(1));
    set_format(
        &mut sheet,
        &["B2", "B3", "B7", "A13", "A14", "A15", "A16"],
        NumberFormat::Number,
        Some(0),
    );
    sheet
}

fn debt_payoff() -> Sheet {
    let mut sheet = build_sheet(
        "Debt Payoff",
        &[
            ("A1", "Debt Account"),
            ("B1", "Balance"),
            ("C1", "APR"),
            ("D1", "Min Payment"),
            ("E1", "Extra Payment"),
            ("F1", "Total Monthly"),
            ("G1", "Est. Months"),
            ("A2", "Credit Card"),
            ("B2", "4800"),
            ("C2", "0.1899"),
            ("D2", "140"),
            ("E2", "110"),
            ("F2", "=D2+E2"),
            ("G2", "=NPER(C2/12, -F2, B2)"),
            ("A3", "Auto Loan"),
            ("B3", "14500"),
            ("C3", "0.059"),
            ("D3", "310"),
            ("E3", "50"),
            ("F3", "=D3+E3"),
            ("G3", "=NPER(C3/12, -F3, B3)"),
            ("A4", "Student Loan"),
            ("B4", "18200"),
            ("C4", "0.045"),
            ("D4", "220"),
            ("E4", "80"),
            ("F4", "=D4+E4"),
            ("G4", "=NPER(C4/12, -F4, B4)"),
            ("A5", "Personal Loan"),
            ("B5", "3600"),
            ("C5", "0.095"),
            ("D5", "125"),
            ("E5", "75"),
            ("F5", "=D5+E5"),
            ("G5", "=NPER(C5/12, -F5, B5)"),
            ("A6", "Total / Summary"),
            ("B6", "=SUM(B2:B5)"),
            ("C6", "=AVERAGE(C2:C5)"),
            ("D6", "=SUM(D2:D5)"),
            ("E6", "=SUM(E2:E5)"),
            ("F6", "=SUM(F2:F5)"),
            ("G6", "=MAX(G2:G5)"),
        ],
        &[
            (0, 130.0),
            (1, 100.0),
            (2, 85.0),
            (3, 100.0),
            (4, 105.0),
            (5, 105.0),
            (6, 95.0),
        ],
    );
    set_header_row(
        &mut sheet,
        &["A1", "B1", "C1", "D1", "E1", "F1", "G1"],
        FillColor::Purple,
    );
    set_total_row(&mut sheet, &["A6", "B6", "C6", "D6", "E6", "F6", "G6"]);
    set_format(
        &mut sheet,
        &[
            "B2", "D2", "E2", "F2", "B3", "D3", "E3", "F3", "B4", "D4", "E4", "F4", "B5", "D5",
            "E5", "F5", "B6", "D6", "E6", "F6",
        ],
        NumberFormat::Currency,
        Some(2),
    );
    set_format(
        &mut sheet,
        &["C2", "C3", "C4", "C5", "C6"],
        NumberFormat::Percentage,
        Some(2),
    );
    set_format(
        &mut sheet,
        &["G2", "G3", "G4", "G5", "G6"],
        NumberFormat::Number,
        Some(1),
    );
    sheet
}

fn tax_summary() -> Sheet {
    let mut sheet = build_sheet(
        "Tax Summary",
        &[
            ("A1", "Income Source"),
            ("B1", "Amount"),
            ("A2", "W-2 Wages"),
            ("B2", "92000"),
            ("A3", "Freelance Income"),
            ("B3", "14500"),
            ("A4", "Qualified Dividends"),
            ("B4", "1850"),
            ("A5", "Bank Interest"),
            ("B5", "620"),
            ("A6", "Total Gross Income"),
            ("B6", "=SUM(B2:B5)"),
            ("A8", "Deduction / Adjustment"),
            ("B8", "Amount"),
            ("A9", "Standard Deduction"),
            ("B9", "14600"),
            ("A10", "401(k) Contribution"),
            ("B10", "8500"),
            ("A11", "HSA Contribution"),
            ("B11", "3200"),
            ("A12", "Total Deductions"),
            ("B12", "=SUM(B9:B11)"),
            ("A14", "Taxable Income"),
            ("B14", "=MAX(0, B6-B12)"),
            ("A15", "Effective Tax Rate"),
            ("B15", "0.22"),
            ("A16", "Estimated Tax"),
            ("B16", "=B14*B15"),
            ("A17", "Tax Withheld"),
            ("B17", "19200"),
            ("A18", "Balance Due / (Refund)"),
            ("B18", "=B16-B17"),
            ("A19", "Filing Status"),
            ("B19", "=IF(B18>0, \"Owed\", \"Refund\")"),
        ],
        &[(0, 175.0), (1, 120.0)],
    );
    set_header_row(&mut sheet, &["A1", "B1"], FillColor::Green);
    set_header_row(&mut sheet, &["A8", "B8"], FillColor::Green);
    set_total_row(&mut sheet, &["A6", "B6"]);
    set_total_row(&mut sheet, &["A12", "B12"]);
    set_total_row(&mut sheet, &["A18", "B18"]);
    set_format(
        &mut sheet,
        &[
            "B2", "B3", "B4", "B5", "B6", "B9", "B10", "B11", "B12", "B14", "B16", "B17", "B18",
        ],
        NumberFormat::Currency,
        Some(2),
    );
    set_format(&mut sheet, &["B15"], NumberFormat::Percentage, Some(1));
    sheet
}

fn travel_itinerary() -> Sheet {
    let mut sheet = build_sheet(
        "Travel Itinerary",
        &[
            ("A1", "Destination"),
            ("B1", "Kyoto & Osaka"),
            ("A2", "Travelers"),
            ("B2", "2"),
            ("A3", "Trip Days"),
            ("B3", "5"),
            ("A5", "Day"),
            ("B5", "Activity"),
            ("C5", "Category"),
            ("D5", "Estimated"),
            ("E5", "Actual"),
            ("F5", "Variance"),
            ("A6", "Day 1"),
            ("B6", "Rail Pass & Airport Transfer"),
            ("C6", "Transport"),
            ("D6", "320"),
            ("E6", "305"),
            ("F6", "=D6-E6"),
            ("A7", "Day 1"),
            ("B7", "Boutique Hotel (4 Nights)"),
            ("C7", "Lodging"),
            ("D7", "960"),
            ("E7", "980"),
            ("F7", "=D7-E7"),
            ("A8", "Day 2"),
            ("B8", "Temple Passes & Guided Walk"),
            ("C8", "Activity"),
            ("D8", "140"),
            ("E8", "130"),
            ("F8", "=D8-E8"),
            ("A9", "Day 3"),
            ("B9", "Market & Dinner Reservations"),
            ("C9", "Food"),
            ("D9", "240"),
            ("E9", "215"),
            ("F9", "=D9-E9"),
            ("A10", "Day 4"),
            ("B10", "Osaka Day Trip & Museum"),
            ("C10", "Activity"),
            ("D10", "180"),
            ("E10", "165"),
            ("F10", "=D10-E10"),
            ("A11", "Total"),
            ("D11", "=SUM(D6:D10)"),
            ("E11", "=SUM(E6:E10)"),
            ("F11", "=SUM(F6:F10)"),
            ("A12", "Actual Cost / Traveler"),
            ("E12", "=E11/B2"),
        ],
        &[
            (0, 110.0),
            (1, 200.0),
            (2, 100.0),
            (3, 95.0),
            (4, 95.0),
            (5, 95.0),
        ],
    );
    set_header_row(
        &mut sheet,
        &["A5", "B5", "C5", "D5", "E5", "F5"],
        FillColor::Blue,
    );
    set_total_row(&mut sheet, &["A11", "B11", "C11", "D11", "E11", "F11"]);
    set_format(
        &mut sheet,
        &[
            "D6", "E6", "F6", "D7", "E7", "F7", "D8", "E8", "F8", "D9", "E9", "F9", "D10", "E10",
            "F10", "D11", "E11", "F11", "E12",
        ],
        NumberFormat::Currency,
        Some(2),
    );
    set_format(&mut sheet, &["B2", "B3"], NumberFormat::Number, Some(0));
    sheet
}

fn habit_workout_log() -> Sheet {
    let mut sheet = build_sheet(
        "Habit & Workout Log",
        &[
            ("A1", "Day"),
            ("B1", "Workout (min)"),
            ("C1", "Steps"),
            ("D1", "Water (L)"),
            ("E1", "Sleep (hrs)"),
            ("F1", "Daily Goal Met"),
            ("A2", "Mon"),
            ("B2", "45"),
            ("C2", "9400"),
            ("D2", "2.4"),
            ("E2", "7.5"),
            ("F2", "=IF(AND(B2>=30, C2>=8000), \"Yes\", \"No\")"),
            ("A3", "Tue"),
            ("B3", "30"),
            ("C3", "8200"),
            ("D3", "2.1"),
            ("E3", "7.0"),
            ("F3", "=IF(AND(B3>=30, C3>=8000), \"Yes\", \"No\")"),
            ("A4", "Wed"),
            ("B4", "20"),
            ("C4", "6500"),
            ("D4", "1.8"),
            ("E4", "6.5"),
            ("F4", "=IF(AND(B4>=30, C4>=8000), \"Yes\", \"No\")"),
            ("A5", "Thu"),
            ("B5", "50"),
            ("C5", "10500"),
            ("D5", "2.6"),
            ("E5", "8.0"),
            ("F5", "=IF(AND(B5>=30, C5>=8000), \"Yes\", \"No\")"),
            ("A6", "Fri"),
            ("B6", "35"),
            ("C6", "8900"),
            ("D6", "2.3"),
            ("E6", "7.5"),
            ("F6", "=IF(AND(B6>=30, C6>=8000), \"Yes\", \"No\")"),
            ("A7", "Sat"),
            ("B7", "60"),
            ("C7", "11800"),
            ("D7", "2.8"),
            ("E7", "8.5"),
            ("F7", "=IF(AND(B7>=30, C7>=8000), \"Yes\", \"No\")"),
            ("A8", "Sun"),
            ("B8", "0"),
            ("C8", "5400"),
            ("D8", "2.0"),
            ("E8", "8.0"),
            ("F8", "=IF(AND(B8>=30, C8>=8000), \"Yes\", \"No\")"),
            ("A10", "Weekly Totals / Avg"),
            ("B10", "=SUM(B2:B8)"),
            ("C10", "=AVERAGE(C2:C8)"),
            ("D10", "=AVERAGE(D2:D8)"),
            ("E10", "=AVERAGE(E2:E8)"),
            ("A11", "Days Goal Met"),
            ("B11", "=COUNTIF(F2:F8, \"Yes\")"),
            ("A12", "Goal Completion Rate"),
            ("B12", "=B11/7"),
        ],
        &[
            (0, 140.0),
            (1, 105.0),
            (2, 90.0),
            (3, 90.0),
            (4, 95.0),
            (5, 110.0),
        ],
    );
    set_header_row(
        &mut sheet,
        &["A1", "B1", "C1", "D1", "E1", "F1"],
        FillColor::Orange,
    );
    set_total_row(&mut sheet, &["A10", "B10", "C10", "D10", "E10", "F10"]);
    set_format(
        &mut sheet,
        &[
            "B2", "C2", "B3", "C3", "B4", "C4", "B5", "C5", "B6", "C6", "B7", "C7", "B8", "C8",
            "B10", "C10", "B11",
        ],
        NumberFormat::Number,
        Some(0),
    );
    set_format(
        &mut sheet,
        &[
            "D2", "E2", "D3", "E3", "D4", "E4", "D5", "E5", "D6", "E6", "D7", "E7", "D8", "E8",
            "D10", "E10",
        ],
        NumberFormat::Number,
        Some(1),
    );
    set_format(&mut sheet, &["B12"], NumberFormat::Percentage, Some(0));
    sheet
}

fn event_budget() -> Sheet {
    let mut sheet = build_sheet(
        "Event Budget",
        &[
            ("A1", "Event"),
            ("B1", "Annual Community Gala"),
            ("A2", "Guest Count"),
            ("B2", "80"),
            ("A4", "Item"),
            ("B4", "Category"),
            ("C4", "Estimated"),
            ("D4", "Actual"),
            ("E4", "Difference"),
            ("F4", "Status"),
            ("A5", "Hall Rental"),
            ("B5", "Venue"),
            ("C5", "2200"),
            ("D5", "2200"),
            ("E5", "=C5-D5"),
            ("F5", "=IF(D5<=C5, \"Under\", \"Over\")"),
            ("A6", "Dinner Buffet"),
            ("B6", "Catering"),
            ("C6", "3200"),
            ("D6", "3450"),
            ("E6", "=C6-D6"),
            ("F6", "=IF(D6<=C6, \"Under\", \"Over\")"),
            ("A7", "Beverage Service"),
            ("B7", "Catering"),
            ("C7", "950"),
            ("D7", "880"),
            ("E7", "=C7-D7"),
            ("F7", "=IF(D7<=C7, \"Under\", \"Over\")"),
            ("A8", "Lighting & Florals"),
            ("B8", "Decor"),
            ("C8", "650"),
            ("D8", "610"),
            ("E8", "=C8-D8"),
            ("F8", "=IF(D8<=C8, \"Under\", \"Over\")"),
            ("A9", "Acoustic Trio"),
            ("B9", "Entertainment"),
            ("C9", "800"),
            ("D9", "750"),
            ("E9", "=C9-D9"),
            ("F9", "=IF(D9<=C9, \"Under\", \"Over\")"),
            ("A10", "Total"),
            ("C10", "=SUM(C5:C9)"),
            ("D10", "=SUM(D5:D9)"),
            ("E10", "=SUM(E5:E9)"),
            ("A11", "Cost per Guest"),
            ("D11", "=D10/B2"),
            ("A12", "Catering Subtotal"),
            ("D12", "=SUMIF(B5:B9, \"Catering\", D5:D9)"),
        ],
        &[
            (0, 150.0),
            (1, 120.0),
            (2, 100.0),
            (3, 100.0),
            (4, 100.0),
            (5, 85.0),
        ],
    );
    set_header_row(
        &mut sheet,
        &["A4", "B4", "C4", "D4", "E4", "F4"],
        FillColor::Purple,
    );
    set_total_row(&mut sheet, &["A10", "B10", "C10", "D10", "E10", "F10"]);
    set_format(
        &mut sheet,
        &[
            "C5", "D5", "E5", "C6", "D6", "E6", "C7", "D7", "E7", "C8", "D8", "E8", "C9", "D9",
            "E9", "C10", "D10", "E10", "D11", "D12",
        ],
        NumberFormat::Currency,
        Some(2),
    );
    set_format(&mut sheet, &["B2"], NumberFormat::Number, Some(0));
    sheet
}

fn meal_planner() -> Sheet {
    let mut sheet = build_sheet(
        "Meal Planner",
        &[
            ("A1", "Day"),
            ("B1", "Dinner Plan"),
            ("C1", "Calories"),
            ("D1", "Protein (g)"),
            ("E1", "Est. Cost"),
            ("A2", "Mon"),
            ("B2", "Herb Roasted Chicken & Greens"),
            ("C2", "620"),
            ("D2", "44"),
            ("E2", "11.5"),
            ("A3", "Tue"),
            ("B3", "Lentil & Tomato Soup"),
            ("C3", "490"),
            ("D3", "24"),
            ("E3", "7.25"),
            ("A4", "Wed"),
            ("B4", "Grilled Salmon & Quinoa"),
            ("C4", "680"),
            ("D4", "46"),
            ("E4", "14.8"),
            ("A5", "Thu"),
            ("B5", "Vegetable Stir-Fry & Tofu"),
            ("C5", "540"),
            ("D5", "28"),
            ("E5", "8.9"),
            ("A6", "Fri"),
            ("B6", "Whole-Grain Pasta Primavera"),
            ("C6", "610"),
            ("D6", "22"),
            ("E6", "9.4"),
            ("A7", "Sat"),
            ("B7", "Turkey Chili Bowl"),
            ("C7", "590"),
            ("D7", "40"),
            ("E7", "10.2"),
            ("A8", "Sun"),
            ("B8", "Sheet-Pan Cod & Root Veg"),
            ("C8", "560"),
            ("D8", "38"),
            ("E8", "12.6"),
            ("A9", "Weekly Total"),
            ("C9", "=SUM(C2:C8)"),
            ("D9", "=SUM(D2:D8)"),
            ("E9", "=SUM(E2:E8)"),
            ("A10", "Daily Average"),
            ("C10", "=AVERAGE(C2:C8)"),
            ("D10", "=AVERAGE(D2:D8)"),
            ("E10", "=AVERAGE(E2:E8)"),
        ],
        &[(0, 95.0), (1, 210.0), (2, 90.0), (3, 95.0), (4, 95.0)],
    );
    set_header_row(
        &mut sheet,
        &["A1", "B1", "C1", "D1", "E1"],
        FillColor::Green,
    );
    set_total_row(&mut sheet, &["A9", "B9", "C9", "D9", "E9"]);
    set_format(
        &mut sheet,
        &[
            "C2", "D2", "C3", "D3", "C4", "D4", "C5", "D5", "C6", "D6", "C7", "D7", "C8", "D8",
            "C9", "D9", "C10", "D10",
        ],
        NumberFormat::Number,
        Some(0),
    );
    set_format(
        &mut sheet,
        &["E2", "E3", "E4", "E5", "E6", "E7", "E8", "E9", "E10"],
        NumberFormat::Currency,
        Some(2),
    );
    sheet
}

fn profit_and_loss() -> Sheet {
    let mut sheet = build_sheet(
        "Profit & Loss",
        &[
            ("A1", "Line Item"),
            ("B1", "Q1"),
            ("C1", "Q2"),
            ("D1", "Q3"),
            ("E1", "Q4"),
            ("F1", "Full Year"),
            ("A2", "Product Sales"),
            ("B2", "42000"),
            ("C2", "46500"),
            ("D2", "51000"),
            ("E2", "58000"),
            ("F2", "=SUM(B2:E2)"),
            ("A3", "Consulting Services"),
            ("B3", "18000"),
            ("C3", "21000"),
            ("D3", "19500"),
            ("E3", "24000"),
            ("F3", "=SUM(B3:E3)"),
            ("A4", "Subscriptions"),
            ("B4", "9500"),
            ("C4", "11200"),
            ("D4", "12800"),
            ("E4", "14500"),
            ("F4", "=SUM(B4:E4)"),
            ("A5", "Total Revenue"),
            ("B5", "=SUM(B2:B4)"),
            ("C5", "=SUM(C2:C4)"),
            ("D5", "=SUM(D2:D4)"),
            ("E5", "=SUM(E2:E4)"),
            ("F5", "=SUM(F2:F4)"),
            ("A6", "Cost of Goods Sold"),
            ("B6", "16800"),
            ("C6", "18200"),
            ("D6", "19900"),
            ("E6", "22400"),
            ("F6", "=SUM(B6:E6)"),
            ("A7", "Payroll & Benefits"),
            ("B7", "26000"),
            ("C7", "26500"),
            ("D7", "27000"),
            ("E7", "28500"),
            ("F7", "=SUM(B7:E7)"),
            ("A8", "Marketing"),
            ("B8", "4500"),
            ("C8", "5200"),
            ("D8", "4800"),
            ("E8", "6100"),
            ("F8", "=SUM(B8:E8)"),
            ("A9", "Operations & Rent"),
            ("B9", "6200"),
            ("C9", "6200"),
            ("D9", "6400"),
            ("E9", "6400"),
            ("F9", "=SUM(B9:E9)"),
            ("A10", "Total Expenses"),
            ("B10", "=SUM(B6:B9)"),
            ("C10", "=SUM(C6:C9)"),
            ("D10", "=SUM(D6:D9)"),
            ("E10", "=SUM(E6:E9)"),
            ("F10", "=SUM(F6:F9)"),
            ("A11", "Net Income"),
            ("B11", "=B5-B10"),
            ("C11", "=C5-C10"),
            ("D11", "=D5-D10"),
            ("E11", "=E5-E10"),
            ("F11", "=F5-F10"),
            ("A12", "Operating Margin"),
            ("B12", "=B11/B5"),
            ("C12", "=C11/C5"),
            ("D12", "=D11/D5"),
            ("E12", "=E11/E5"),
            ("F12", "=F11/F5"),
        ],
        &[
            (0, 155.0),
            (1, 95.0),
            (2, 95.0),
            (3, 95.0),
            (4, 95.0),
            (5, 105.0),
        ],
    );
    set_header_row(
        &mut sheet,
        &["A1", "B1", "C1", "D1", "E1", "F1"],
        FillColor::Blue,
    );
    set_total_row(&mut sheet, &["A5", "B5", "C5", "D5", "E5", "F5"]);
    set_total_row(&mut sheet, &["A10", "B10", "C10", "D10", "E10", "F10"]);
    set_total_row(&mut sheet, &["A11", "B11", "C11", "D11", "E11", "F11"]);
    set_format(
        &mut sheet,
        &[
            "B2", "C2", "D2", "E2", "F2", "B3", "C3", "D3", "E3", "F3", "B4", "C4", "D4", "E4",
            "F4", "B5", "C5", "D5", "E5", "F5", "B6", "C6", "D6", "E6", "F6", "B7", "C7", "D7",
            "E7", "F7", "B8", "C8", "D8", "E8", "F8", "B9", "C9", "D9", "E9", "F9", "B10", "C10",
            "D10", "E10", "F10", "B11", "C11", "D11", "E11", "F11",
        ],
        NumberFormat::Currency,
        Some(0),
    );
    set_format(
        &mut sheet,
        &["B12", "C12", "D12", "E12", "F12"],
        NumberFormat::Percentage,
        Some(1),
    );
    sheet
}

fn employee_timesheet() -> Sheet {
    let mut sheet = build_sheet(
        "Employee Timesheet",
        &[
            ("A1", "Employee"),
            ("B1", "Jordan Lee"),
            ("A2", "Hourly Rate"),
            ("B2", "45"),
            ("A3", "Overtime Multiplier"),
            ("B3", "1.5"),
            ("A5", "Day"),
            ("B5", "Regular Hrs"),
            ("C5", "Overtime Hrs"),
            ("D5", "Total Hrs"),
            ("E5", "Daily Pay"),
            ("A6", "Monday"),
            ("B6", "8"),
            ("C6", "0.5"),
            ("D6", "=B6+C6"),
            ("E6", "=B6*$B$2+C6*$B$2*$B$3"),
            ("A7", "Tuesday"),
            ("B7", "8"),
            ("C7", "1"),
            ("D7", "=B7+C7"),
            ("E7", "=B7*$B$2+C7*$B$2*$B$3"),
            ("A8", "Wednesday"),
            ("B8", "8"),
            ("C8", "0"),
            ("D8", "=B8+C8"),
            ("E8", "=B8*$B$2+C8*$B$2*$B$3"),
            ("A9", "Thursday"),
            ("B9", "8"),
            ("C9", "1.5"),
            ("D9", "=B9+C9"),
            ("E9", "=B9*$B$2+C9*$B$2*$B$3"),
            ("A10", "Friday"),
            ("B10", "7.5"),
            ("C10", "0"),
            ("D10", "=B10+C10"),
            ("E10", "=B10*$B$2+C10*$B$2*$B$3"),
            ("A11", "Weekly Totals"),
            ("B11", "=SUM(B6:B10)"),
            ("C11", "=SUM(C6:C10)"),
            ("D11", "=SUM(D6:D10)"),
            ("E11", "=SUM(E6:E10)"),
        ],
        &[(0, 145.0), (1, 100.0), (2, 105.0), (3, 95.0), (4, 105.0)],
    );
    set_header_row(
        &mut sheet,
        &["A5", "B5", "C5", "D5", "E5"],
        FillColor::Green,
    );
    set_total_row(&mut sheet, &["A11", "B11", "C11", "D11", "E11"]);
    set_format(
        &mut sheet,
        &["B2", "E6", "E7", "E8", "E9", "E10", "E11"],
        NumberFormat::Currency,
        Some(2),
    );
    set_format(
        &mut sheet,
        &[
            "B3", "B6", "C6", "D6", "B7", "C7", "D7", "B8", "C8", "D8", "B9", "C9", "D9", "B10",
            "C10", "D10", "B11", "C11", "D11",
        ],
        NumberFormat::Number,
        Some(1),
    );
    sheet
}

fn inventory_tracker() -> Sheet {
    let mut sheet = build_sheet(
        "Inventory Tracker",
        &[
            ("A1", "SKU"),
            ("B1", "Item Name"),
            ("C1", "In Stock"),
            ("D1", "Reorder Level"),
            ("E1", "Unit Cost"),
            ("F1", "Stock Value"),
            ("G1", "Status"),
            ("A2", "LM-101"),
            ("B2", "USB-C Dock"),
            ("C2", "24"),
            ("D2", "15"),
            ("E2", "68"),
            ("F2", "=C2*E2"),
            ("G2", "=IF(C2<=D2, \"Reorder\", \"OK\")"),
            ("A3", "LM-102"),
            ("B3", "Mechanical Keycap Set"),
            ("C3", "8"),
            ("D3", "10"),
            ("E3", "42"),
            ("F3", "=C3*E3"),
            ("G3", "=IF(C3<=D3, \"Reorder\", \"OK\")"),
            ("A4", "LM-103"),
            ("B4", "Aluminum Laptop Stand"),
            ("C4", "31"),
            ("D4", "12"),
            ("E4", "54"),
            ("F4", "=C4*E4"),
            ("G4", "=IF(C4<=D4, \"Reorder\", \"OK\")"),
            ("A5", "LM-104"),
            ("B5", "Braided Cable (2m)"),
            ("C5", "11"),
            ("D5", "20"),
            ("E5", "14"),
            ("F5", "=C5*E5"),
            ("G5", "=IF(C5<=D5, \"Reorder\", \"OK\")"),
            ("A6", "LM-105"),
            ("B6", "Desk Mat (Wool)"),
            ("C6", "19"),
            ("D6", "10"),
            ("E6", "29"),
            ("F6", "=C6*E6"),
            ("G6", "=IF(C6<=D6, \"Reorder\", \"OK\")"),
            ("A7", "Total / Summary"),
            ("C7", "=SUM(C2:C6)"),
            ("F7", "=SUM(F2:F6)"),
            ("A8", "Items Below Reorder"),
            ("C8", "=COUNTIF(G2:G6, \"Reorder\")"),
        ],
        &[
            (0, 85.0),
            (1, 175.0),
            (2, 85.0),
            (3, 105.0),
            (4, 90.0),
            (5, 105.0),
            (6, 90.0),
        ],
    );
    set_header_row(
        &mut sheet,
        &["A1", "B1", "C1", "D1", "E1", "F1", "G1"],
        FillColor::Yellow,
    );
    set_total_row(&mut sheet, &["A7", "B7", "C7", "D7", "E7", "F7", "G7"]);
    set_format(
        &mut sheet,
        &[
            "C2", "D2", "C3", "D3", "C4", "D4", "C5", "D5", "C6", "D6", "C7", "C8",
        ],
        NumberFormat::Number,
        Some(0),
    );
    set_format(
        &mut sheet,
        &[
            "E2", "F2", "E3", "F3", "E4", "F4", "E5", "F5", "E6", "F6", "F7",
        ],
        NumberFormat::Currency,
        Some(2),
    );
    sheet
}

fn project_schedule() -> Sheet {
    let mut sheet = build_sheet(
        "Project Schedule",
        &[
            ("A1", "Task / Milestone"),
            ("B1", "Owner"),
            ("C1", "Est. Hrs"),
            ("D1", "Actual Hrs"),
            ("E1", "Variance"),
            ("F1", "% Complete"),
            ("G1", "Status"),
            ("A2", "Requirements & Scope"),
            ("B2", "Maya"),
            ("C2", "16"),
            ("D2", "14"),
            ("E2", "=C2-D2"),
            ("F2", "1"),
            (
                "G2",
                "=IF(F2=1, \"Done\", IF(F2>0, \"In Progress\", \"Planned\"))",
            ),
            ("A3", "UI Wireframes"),
            ("B3", "Liam"),
            ("C3", "24"),
            ("D3", "26"),
            ("E3", "=C3-D3"),
            ("F3", "1"),
            (
                "G3",
                "=IF(F3=1, \"Done\", IF(F3>0, \"In Progress\", \"Planned\"))",
            ),
            ("A4", "Core Engine Build"),
            ("B4", "Priya"),
            ("C4", "40"),
            ("D4", "28"),
            ("E4", "=C4-D4"),
            ("F4", "0.75"),
            (
                "G4",
                "=IF(F4=1, \"Done\", IF(F4>0, \"In Progress\", \"Planned\"))",
            ),
            ("A5", "Integration Testing"),
            ("B5", "Noah"),
            ("C5", "20"),
            ("D5", "6"),
            ("E5", "=C5-D5"),
            ("F5", "0.3"),
            (
                "G5",
                "=IF(F5=1, \"Done\", IF(F5>0, \"In Progress\", \"Planned\"))",
            ),
            ("A6", "Release Packaging"),
            ("B6", "Maya"),
            ("C6", "12"),
            ("D6", "0"),
            ("E6", "=C6-D6"),
            ("F6", "0"),
            (
                "G6",
                "=IF(F6=1, \"Done\", IF(F6>0, \"In Progress\", \"Planned\"))",
            ),
            ("A7", "Project Totals"),
            ("C7", "=SUM(C2:C6)"),
            ("D7", "=SUM(D2:D6)"),
            ("E7", "=SUM(E2:E6)"),
            ("F7", "=AVERAGE(F2:F6)"),
            ("A8", "Completed Tasks"),
            ("C8", "=COUNTIF(G2:G6, \"Done\")"),
        ],
        &[
            (0, 165.0),
            (1, 90.0),
            (2, 80.0),
            (3, 90.0),
            (4, 85.0),
            (5, 95.0),
            (6, 105.0),
        ],
    );
    set_header_row(
        &mut sheet,
        &["A1", "B1", "C1", "D1", "E1", "F1", "G1"],
        FillColor::Blue,
    );
    set_total_row(&mut sheet, &["A7", "B7", "C7", "D7", "E7", "F7", "G7"]);
    set_format(
        &mut sheet,
        &[
            "C2", "D2", "E2", "C3", "D3", "E3", "C4", "D4", "E4", "C5", "D5", "E5", "C6", "D6",
            "E6", "C7", "D7", "E7", "C8",
        ],
        NumberFormat::Number,
        Some(0),
    );
    set_format(
        &mut sheet,
        &["F2", "F3", "F4", "F5", "F6", "F7"],
        NumberFormat::Percentage,
        Some(0),
    );
    sheet
}

fn gradebook() -> Sheet {
    let mut sheet = build_sheet(
        "Gradebook",
        &[
            ("A1", "Student"),
            ("B1", "Homework (30%)"),
            ("C1", "Midterm (30%)"),
            ("D1", "Final (40%)"),
            ("E1", "Weighted Score"),
            ("F1", "Letter"),
            ("A2", "Avery Chen"),
            ("B2", "94"),
            ("C2", "89"),
            ("D2", "92"),
            ("E2", "=B2*0.3+C2*0.3+D2*0.4"),
            (
                "F2",
                "=IF(E2>=90, \"A\", IF(E2>=80, \"B\", IF(E2>=70, \"C\", \"D\")))",
            ),
            ("A3", "Devon Brooks"),
            ("B3", "86"),
            ("C3", "82"),
            ("D3", "85"),
            ("E3", "=B3*0.3+C3*0.3+D3*0.4"),
            (
                "F3",
                "=IF(E3>=90, \"A\", IF(E3>=80, \"B\", IF(E3>=70, \"C\", \"D\")))",
            ),
            ("A4", "Elena Rostova"),
            ("B4", "98"),
            ("C4", "95"),
            ("D4", "96"),
            ("E4", "=B4*0.3+C4*0.3+D4*0.4"),
            (
                "F4",
                "=IF(E4>=90, \"A\", IF(E4>=80, \"B\", IF(E4>=70, \"C\", \"D\")))",
            ),
            ("A5", "Marcus Hill"),
            ("B5", "78"),
            ("C5", "74"),
            ("D5", "80"),
            ("E5", "=B5*0.3+C5*0.3+D5*0.4"),
            (
                "F5",
                "=IF(E5>=90, \"A\", IF(E5>=80, \"B\", IF(E5>=70, \"C\", \"D\")))",
            ),
            ("A6", "Sora Takahashi"),
            ("B6", "90"),
            ("C6", "87"),
            ("D6", "91"),
            ("E6", "=B6*0.3+C6*0.3+D6*0.4"),
            (
                "F6",
                "=IF(E6>=90, \"A\", IF(E6>=80, \"B\", IF(E6>=70, \"C\", \"D\")))",
            ),
            ("A8", "Class Average"),
            ("E8", "=AVERAGE(E2:E6)"),
            ("A9", "Highest Score"),
            ("E9", "=MAX(E2:E6)"),
            ("A10", "Lowest Score"),
            ("E10", "=MIN(E2:E6)"),
            ("A11", "A Grades"),
            ("E11", "=COUNTIF(F2:F6, \"A\")"),
        ],
        &[
            (0, 145.0),
            (1, 115.0),
            (2, 105.0),
            (3, 95.0),
            (4, 115.0),
            (5, 75.0),
        ],
    );
    set_header_row(
        &mut sheet,
        &["A1", "B1", "C1", "D1", "E1", "F1"],
        FillColor::Purple,
    );
    set_total_row(&mut sheet, &["A8", "E8"]);
    set_format(
        &mut sheet,
        &[
            "B2", "C2", "D2", "B3", "C3", "D3", "B4", "C4", "D4", "B5", "C5", "D5", "B6", "C6",
            "D6", "E11",
        ],
        NumberFormat::Number,
        Some(0),
    );
    set_format(
        &mut sheet,
        &["E2", "E3", "E4", "E5", "E6", "E8", "E9", "E10"],
        NumberFormat::Number,
        Some(1),
    );
    sheet
}

fn attendance_tracker() -> Sheet {
    let mut sheet = build_sheet(
        "Attendance Tracker",
        &[
            ("A1", "Student"),
            ("B1", "Mon"),
            ("C1", "Tue"),
            ("D1", "Wed"),
            ("E1", "Thu"),
            ("F1", "Fri"),
            ("G1", "Present"),
            ("H1", "Rate"),
            ("A2", "Amara Patel"),
            ("B2", "P"),
            ("C2", "P"),
            ("D2", "P"),
            ("E2", "P"),
            ("F2", "P"),
            ("G2", "=COUNTIF(B2:F2, \"P\")"),
            ("H2", "=G2/5"),
            ("A3", "Ben Carter"),
            ("B3", "P"),
            ("C3", "A"),
            ("D3", "P"),
            ("E3", "P"),
            ("F3", "P"),
            ("G3", "=COUNTIF(B3:F3, \"P\")"),
            ("H3", "=G3/5"),
            ("A4", "Chloe Kim"),
            ("B4", "P"),
            ("C4", "P"),
            ("D4", "P"),
            ("E4", "P"),
            ("F4", "P"),
            ("G4", "=COUNTIF(B4:F4, \"P\")"),
            ("H4", "=G4/5"),
            ("A5", "David Ortiz"),
            ("B5", "P"),
            ("C5", "P"),
            ("D5", "A"),
            ("E5", "P"),
            ("F5", "A"),
            ("G5", "=COUNTIF(B5:F5, \"P\")"),
            ("H5", "=G5/5"),
            ("A6", "Freya Lind"),
            ("B6", "P"),
            ("C6", "P"),
            ("D6", "P"),
            ("E6", "P"),
            ("F6", "P"),
            ("G6", "=COUNTIF(B6:F6, \"P\")"),
            ("H6", "=G6/5"),
            ("A8", "Class Avg Attendance"),
            ("H8", "=AVERAGE(H2:H6)"),
            ("A9", "Perfect Attendance"),
            ("G9", "=COUNTIF(G2:G6, 5)"),
        ],
        &[
            (0, 145.0),
            (1, 55.0),
            (2, 55.0),
            (3, 55.0),
            (4, 55.0),
            (5, 55.0),
            (6, 75.0),
            (7, 80.0),
        ],
    );
    set_header_row(
        &mut sheet,
        &["A1", "B1", "C1", "D1", "E1", "F1", "G1", "H1"],
        FillColor::Green,
    );
    set_total_row(&mut sheet, &["A8", "H8"]);
    set_format(
        &mut sheet,
        &["G2", "G3", "G4", "G5", "G6", "G9"],
        NumberFormat::Number,
        Some(0),
    );
    set_format(
        &mut sheet,
        &["H2", "H3", "H4", "H5", "H6", "H8"],
        NumberFormat::Percentage,
        Some(0),
    );
    sheet
}

fn gpa_calculator() -> Sheet {
    let mut sheet = build_sheet(
        "GPA Calculator",
        &[
            ("A1", "Course"),
            ("B1", "Credits"),
            ("C1", "Grade Points"),
            ("D1", "Quality Points"),
            ("A2", "Linear Algebra"),
            ("B2", "4"),
            ("C2", "3.7"),
            ("D2", "=B2*C2"),
            ("A3", "Data Structures"),
            ("B3", "4"),
            ("C3", "4"),
            ("D3", "=B3*C3"),
            ("A4", "Modern History"),
            ("B4", "3"),
            ("C4", "3.3"),
            ("D4", "=B4*C4"),
            ("A5", "Technical Writing"),
            ("B5", "3"),
            ("C5", "3.7"),
            ("D5", "=B5*C5"),
            ("A6", "Organic Chemistry"),
            ("B6", "4"),
            ("C6", "3.0"),
            ("D6", "=B6*C6"),
            ("A7", "Totals"),
            ("B7", "=SUM(B2:B6)"),
            ("D7", "=SUM(D2:D6)"),
            ("A8", "Cumulative GPA"),
            ("D8", "=D7/B7"),
            ("A9", "Academic Standing"),
            ("D9", "=IF(D8>=3.5, \"Honors\", \"Good Standing\")"),
        ],
        &[(0, 155.0), (1, 80.0), (2, 105.0), (3, 120.0)],
    );
    set_header_row(&mut sheet, &["A1", "B1", "C1", "D1"], FillColor::Blue);
    set_total_row(&mut sheet, &["A7", "B7", "C7", "D7"]);
    set_total_row(&mut sheet, &["A8", "D8"]);
    set_format(
        &mut sheet,
        &["B2", "B3", "B4", "B5", "B6", "B7"],
        NumberFormat::Number,
        Some(0),
    );
    set_format(
        &mut sheet,
        &[
            "C2", "D2", "C3", "D3", "C4", "D4", "C5", "D5", "C6", "D6", "D7", "D8",
        ],
        NumberFormat::Number,
        Some(2),
    );
    sheet
}

fn lab_data_and_chart() -> Sheet {
    let mut sheet = build_sheet(
        "Lab Data & Chart",
        &[
            ("A1", "Time (s)"),
            ("B1", "Trial 1 (m/s)"),
            ("C1", "Trial 2 (m/s)"),
            ("D1", "Mean Velocity"),
            ("A2", "0"),
            ("B2", "0"),
            ("C2", "0"),
            ("D2", "=AVERAGE(B2:C2)"),
            ("A3", "2"),
            ("B3", "4.1"),
            ("C3", "3.9"),
            ("D3", "=AVERAGE(B3:C3)"),
            ("A4", "4"),
            ("B4", "8.3"),
            ("C4", "8.1"),
            ("D4", "=AVERAGE(B4:C4)"),
            ("A5", "6"),
            ("B5", "12.2"),
            ("C5", "12.6"),
            ("D5", "=AVERAGE(B5:C5)"),
            ("A6", "8"),
            ("B6", "16.0"),
            ("C6", "16.4"),
            ("D6", "=AVERAGE(B6:C6)"),
            ("A7", "10"),
            ("B7", "19.8"),
            ("C7", "20.2"),
            ("D7", "=AVERAGE(B7:C7)"),
            ("A9", "Peak Mean Velocity"),
            ("D9", "=MAX(D2:D7)"),
            ("A10", "Overall Mean"),
            ("D10", "=AVERAGE(D2:D7)"),
            ("A11", "Minimum Velocity"),
            ("D11", "=MIN(D2:D7)"),
        ],
        &[(0, 130.0), (1, 105.0), (2, 105.0), (3, 115.0)],
    );
    set_header_row(&mut sheet, &["A1", "B1", "C1", "D1"], FillColor::Blue);
    set_total_row(&mut sheet, &["A9", "D9"]);
    set_format(
        &mut sheet,
        &["A2", "A3", "A4", "A5", "A6", "A7"],
        NumberFormat::Number,
        Some(0),
    );
    set_format(
        &mut sheet,
        &[
            "B2", "C2", "D2", "B3", "C3", "D3", "B4", "C4", "D4", "B5", "C5", "D5", "B6", "C6",
            "D6", "B7", "C7", "D7", "D9", "D10", "D11",
        ],
        NumberFormat::Number,
        Some(2),
    );
    sheet.chart = Some(SheetChart {
        kind: ChartKind::Line,
        title: "Mean Velocity vs Time".to_string(),
        cat_col: 0,
        val_col: 3,
        start_row: 1,
        end_row: Some(6),
        series_name: "Mean Velocity".to_string(),
        show_legend: true,
        x_axis_label: "Time (s)".to_string(),
        y_axis_label: "Velocity (m/s)".to_string(),
    });
    sheet
}
