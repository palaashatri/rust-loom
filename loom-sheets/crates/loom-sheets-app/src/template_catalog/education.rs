//! Education template builders (indices 23–26).

use loom_sheets_core::style::FillColor;
use loom_sheets_core::{ChartKind, NumberFormat, Sheet, SheetChart};

use super::{build_sheet, set_format, set_header_row, set_total_row};

pub(super) fn gradebook() -> Sheet {
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

pub(super) fn attendance_tracker() -> Sheet {
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

pub(super) fn gpa_calculator() -> Sheet {
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

pub(super) fn lab_data_and_chart() -> Sheet {
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
