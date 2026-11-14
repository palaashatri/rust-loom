//! Business template builders (indices 19–22).

use loom_sheets_core::style::FillColor;
use loom_sheets_core::{NumberFormat, Sheet};

use super::{build_sheet, set_format, set_header_row, set_total_row};

pub(super) fn profit_and_loss() -> Sheet {
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

pub(super) fn employee_timesheet() -> Sheet {
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

pub(super) fn inventory_tracker() -> Sheet {
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

pub(super) fn project_schedule() -> Sheet {
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
