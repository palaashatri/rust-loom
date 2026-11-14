//! Personal template builders (indices 15–18).

use loom_sheets_core::style::FillColor;
use loom_sheets_core::{NumberFormat, Sheet};

use super::{build_sheet, set_format, set_header_row, set_total_row};

pub(super) fn travel_itinerary() -> Sheet {
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

pub(super) fn habit_workout_log() -> Sheet {
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

pub(super) fn event_budget() -> Sheet {
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

pub(super) fn meal_planner() -> Sheet {
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
