//! Personal Finance template builders (indices 11–14).

use loom_sheets_core::style::FillColor;
use loom_sheets_core::{NumberFormat, Sheet};

use super::{build_sheet, set_format, set_header_row, set_total_row};

pub(super) fn loan_amortization() -> Sheet {
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
            ("A15", "6-Mo Subtotal"),
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

pub(super) fn retirement_planner() -> Sheet {
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

pub(super) fn debt_payoff() -> Sheet {
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

pub(super) fn tax_summary() -> Sheet {
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
