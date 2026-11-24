//! Smart series detection and auto-fill value generation for Sheets.

use crate::refs::shift_formula_references;
use crate::{Cell, CellRange, CellRef, Sheet};

/// Detected auto-fill pattern for a source range.
#[derive(Debug, Clone, PartialEq)]
pub enum FillSeries {
    /// Repeat source values cyclically.
    Copy,
    /// Arithmetic progression (`start + index * step`).
    Linear { start: f64, step: f64 },
    /// Date progression by fixed number of days.
    DateDay { start_days: i64, step_days: i64 },
    /// Date progression by fixed number of months.
    DateMonth {
        start_year: i32,
        start_month: u32,
        start_day: u32,
        step_months: i32,
    },
    /// Weekday name progression (`Mon..Sun` or `Monday..Sunday`).
    Weekday {
        start_index: usize,
        step: isize,
        abbreviated: bool,
    },
    /// Shift relative formula references from source to target.
    FormulaShift,
}

const WEEKDAYS_SHORT: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const WEEKDAYS_FULL: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];

fn parse_weekday(s: &str) -> Option<(usize, bool)> {
    let trimmed = s.trim();
    for (i, w) in WEEKDAYS_SHORT.iter().enumerate() {
        if trimmed.eq_ignore_ascii_case(w) {
            return Some((i, true));
        }
    }
    for (i, w) in WEEKDAYS_FULL.iter().enumerate() {
        if trimmed.eq_ignore_ascii_case(w) {
            return Some((i, false));
        }
    }
    None
}

fn parse_iso_date(s: &str) -> Option<(i32, u32, u32)> {
    let parts: Vec<&str> = s.trim().split('-').collect();
    if parts.len() != 3 || parts[0].len() != 4 {
        return None;
    }
    let y: i32 = parts[0].parse().ok()?;
    let m: u32 = parts[1].parse().ok()?;
    let d: u32 = parts[2].parse().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    Some((y, m, d))
}

fn days_from_civil(y: i32, m: u32, d: u32) -> i64 {
    let y = y as i64 - if m <= 2 { 1 } else { 0 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let m_adj = m as i64 + if m > 2 { -3 } else { 9 };
    let doy = (153 * m_adj + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if m <= 2 { 1 } else { 0 };
    (year as i32, m as u32, d as u32)
}

fn add_months(y: i32, m: u32, d: u32, delta: i32) -> (i32, u32, u32) {
    let total_months = y * 12 + (m as i32 - 1) + delta;
    let new_y = total_months.div_euclid(12);
    let new_m = (total_months.rem_euclid(12) + 1) as u32;
    let max_day = match new_m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if (new_y % 4 == 0 && new_y % 100 != 0) || new_y % 400 == 0 {
                29
            } else {
                28
            }
        }
        _ => 30,
    };
    (new_y, new_m, d.min(max_day))
}

fn format_number(v: f64) -> String {
    if (v - v.round()).abs() < 1e-9 && v.abs() < 1e15 {
        format!("{}", v.round() as i64)
    } else {
        let s = format!("{v:.6}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

/// Detects the most appropriate fill series for a sequence of raw cell strings.
pub fn detect_series(cells: &[&str]) -> FillSeries {
    if cells.is_empty() {
        return FillSeries::Copy;
    }
    if cells.iter().any(|c| c.trim_start().starts_with('=')) {
        return FillSeries::FormulaShift;
    }

    // Numeric series
    let nums: Option<Vec<f64>> = cells.iter().map(|c| c.trim().parse::<f64>().ok()).collect();
    if let Some(nums) = nums {
        if nums.len() == 1 {
            return FillSeries::Linear {
                start: nums[0],
                step: 0.0,
            };
        }
        let step = nums[1] - nums[0];
        let is_linear = nums.windows(2).all(|w| ((w[1] - w[0]) - step).abs() < 1e-9);
        if is_linear {
            return FillSeries::Linear {
                start: nums[0],
                step,
            };
        }
        return FillSeries::Copy;
    }

    // ISO date series (YYYY-MM-DD)
    let dates: Option<Vec<(i32, u32, u32)>> = cells.iter().map(|c| parse_iso_date(c)).collect();
    if let Some(dates) = dates {
        if dates.len() == 1 {
            let (y, m, d) = dates[0];
            return FillSeries::DateDay {
                start_days: days_from_civil(y, m, d),
                step_days: 1,
            };
        }
        let same_day = dates.iter().all(|(_, _, d)| *d == dates[0].2);
        if same_day {
            let step_m = (dates[1].0 - dates[0].0) * 12 + (dates[1].1 as i32 - dates[0].1 as i32);
            let is_month_series = dates
                .windows(2)
                .all(|w| (w[1].0 - w[0].0) * 12 + (w[1].1 as i32 - w[0].1 as i32) == step_m);
            if is_month_series && step_m != 0 {
                return FillSeries::DateMonth {
                    start_year: dates[0].0,
                    start_month: dates[0].1,
                    start_day: dates[0].2,
                    step_months: step_m,
                };
            }
        }
        let days: Vec<i64> = dates
            .iter()
            .map(|&(y, m, d)| days_from_civil(y, m, d))
            .collect();
        let step_d = days[1] - days[0];
        if days.windows(2).all(|w| w[1] - w[0] == step_d) {
            return FillSeries::DateDay {
                start_days: days[0],
                step_days: step_d,
            };
        }
        return FillSeries::Copy;
    }

    // Weekday series
    let weekdays: Option<Vec<(usize, bool)>> = cells.iter().map(|c| parse_weekday(c)).collect();
    if let Some(wd) = weekdays {
        let abbreviated = wd[0].1;
        if wd.len() == 1 {
            return FillSeries::Weekday {
                start_index: wd[0].0,
                step: 1,
                abbreviated,
            };
        }
        let step = (wd[1].0 as isize - wd[0].0 as isize).rem_euclid(7);
        let consistent = wd
            .windows(2)
            .all(|w| (w[1].0 as isize - w[0].0 as isize).rem_euclid(7) == step);
        if consistent {
            return FillSeries::Weekday {
                start_index: wd[0].0,
                step,
                abbreviated,
            };
        }
    }

    FillSeries::Copy
}

fn generate_value_at(
    series: &FillSeries,
    source_cells: &[Option<&Cell>],
    seq_index: isize,
    delta_cols: i32,
    delta_rows: i32,
) -> Cell {
    let src_len = source_cells.len().max(1) as isize;
    let src_idx = seq_index.rem_euclid(src_len) as usize;
    let src_cell = source_cells[src_idx];

    match series {
        FillSeries::Copy => src_cell.cloned().unwrap_or_else(|| Cell::new("")),
        FillSeries::Linear { start, step } => {
            let val = start + (seq_index as f64) * step;
            Cell::new(format_number(val))
        }
        FillSeries::DateDay {
            start_days,
            step_days,
        } => {
            let days = start_days + (seq_index as i64) * step_days;
            let (y, m, d) = civil_from_days(days);
            Cell::new(format!("{y:04}-{m:02}-{d:02}"))
        }
        FillSeries::DateMonth {
            start_year,
            start_month,
            start_day,
            step_months,
        } => {
            let (y, m, d) = add_months(
                *start_year,
                *start_month,
                *start_day,
                (seq_index as i32) * step_months,
            );
            Cell::new(format!("{y:04}-{m:02}-{d:02}"))
        }
        FillSeries::Weekday {
            start_index,
            step,
            abbreviated,
        } => {
            let idx = (*start_index as isize + seq_index * step).rem_euclid(7) as usize;
            let name = if *abbreviated {
                WEEKDAYS_SHORT[idx]
            } else {
                WEEKDAYS_FULL[idx]
            };
            Cell::new(name)
        }
        FillSeries::FormulaShift => {
            if let Some(cell) = src_cell {
                if cell.is_formula() {
                    Cell::new(shift_formula_references(&cell.raw, delta_cols, delta_rows))
                } else {
                    cell.clone()
                }
            } else {
                Cell::new("")
            }
        }
    }
}

/// Generates `(CellRef, Cell)` pairs for all cells in `target` outside `source`.
pub fn generate_fill_values(
    source: CellRange,
    target: CellRange,
    sheet: &Sheet,
) -> Vec<(CellRef, Cell)> {
    let mut out = Vec::new();
    let src_rows = (source.end.row - source.start.row + 1) as isize;
    let src_cols = (source.end.col - source.start.col + 1) as isize;

    // Vertical fill (target extends above or below source)
    if target.start.row < source.start.row || target.end.row > source.end.row {
        for col in source.start.col..=source.end.col {
            let src_cells: Vec<Option<&Cell>> = (source.start.row..=source.end.row)
                .map(|r| sheet.cells.get(&CellRef { row: r, col }))
                .collect();
            let raw_strs: Vec<&str> = src_cells
                .iter()
                .map(|c| c.map(|cell| cell.raw.as_str()).unwrap_or(""))
                .collect();
            let series = detect_series(&raw_strs);

            for row in target.start.row..=target.end.row {
                if row >= source.start.row && row <= source.end.row {
                    continue;
                }
                let seq_index = row as isize - source.start.row as isize;
                let src_row_for_shift = source.start.row as isize + seq_index.rem_euclid(src_rows);
                let delta_rows = row as i32 - src_row_for_shift as i32;
                let cell = generate_value_at(&series, &src_cells, seq_index, 0, delta_rows);
                out.push((CellRef { row, col }, cell));
            }
        }
    }

    // Horizontal fill (target extends left or right of source)
    if target.start.col < source.start.col || target.end.col > source.end.col {
        for row in source.start.row..=source.end.row {
            let src_cells: Vec<Option<&Cell>> = (source.start.col..=source.end.col)
                .map(|c| sheet.cells.get(&CellRef { row, col: c }))
                .collect();
            let raw_strs: Vec<&str> = src_cells
                .iter()
                .map(|c| c.map(|cell| cell.raw.as_str()).unwrap_or(""))
                .collect();
            let series = detect_series(&raw_strs);

            for col in target.start.col..=target.end.col {
                if col >= source.start.col && col <= source.end.col {
                    continue;
                }
                let seq_index = col as isize - source.start.col as isize;
                let src_col_for_shift = source.start.col as isize + seq_index.rem_euclid(src_cols);
                let delta_cols = col as i32 - src_col_for_shift as i32;
                let cell = generate_value_at(&series, &src_cells, seq_index, delta_cols, 0);
                out.push((CellRef { row, col }, cell));
            }
        }
    }

    out
}

/// Detects the target range for a double-click on the fill handle by scanning
/// adjacent left or right columns for contiguous non-empty cells.
pub fn double_click_fill_extent(source: CellRange, sheet: &Sheet) -> Option<CellRange> {
    let check_col = |col: u32| -> Option<u32> {
        let mut last_row = source.end.row;
        let mut r = source.end.row + 1;
        while let Some(cell) = sheet.cells.get(&CellRef { row: r, col }) {
            if cell.raw.trim().is_empty() {
                break;
            }
            last_row = r;
            r += 1;
        }
        if last_row > source.end.row {
            Some(last_row)
        } else {
            None
        }
    };

    let end_row = if source.start.col > 1 {
        check_col(source.start.col - 1)
    } else {
        None
    }
    .or_else(|| check_col(source.end.col + 1))?;

    Some(CellRange::new(
        source.start,
        CellRef {
            row: end_row,
            col: source.end.col,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_linear_and_date_and_weekday_series() {
        assert_eq!(
            detect_series(&["1", "2", "3"]),
            FillSeries::Linear {
                start: 1.0,
                step: 1.0
            }
        );
        assert_eq!(
            detect_series(&["10", "20"]),
            FillSeries::Linear {
                start: 10.0,
                step: 10.0
            }
        );
        assert_eq!(
            detect_series(&["2026-01-01"]),
            FillSeries::DateDay {
                start_days: days_from_civil(2026, 1, 1),
                step_days: 1
            }
        );
        assert_eq!(
            detect_series(&["2026-01-15", "2026-02-15"]),
            FillSeries::DateMonth {
                start_year: 2026,
                start_month: 1,
                start_day: 15,
                step_months: 1
            }
        );
        assert_eq!(
            detect_series(&["Mon", "Tue"]),
            FillSeries::Weekday {
                start_index: 0,
                step: 1,
                abbreviated: true
            }
        );
        assert_eq!(detect_series(&["=A1+B1"]), FillSeries::FormulaShift);
    }

    #[test]
    fn generates_linear_formula_and_double_click_extent() {
        let mut sheet = Sheet::new("Sheet1");
        let cr = |row, col| CellRef { row, col };
        sheet.cells.insert(cr(1, 1), Cell::new("10"));
        sheet.cells.insert(cr(2, 1), Cell::new("20"));
        sheet.cells.insert(cr(1, 2), Cell::new("=A1*2"));
        sheet.cells.insert(cr(3, 1), Cell::new("30"));
        sheet.cells.insert(cr(4, 1), Cell::new("40"));

        let src = CellRange::new(cr(1, 1), cr(2, 1));
        let tgt = CellRange::new(cr(1, 1), cr(4, 1));
        let filled = generate_fill_values(src, tgt, &sheet);
        assert_eq!(filled.len(), 2);
        assert_eq!(filled[0], (cr(3, 1), Cell::new("30")));
        assert_eq!(filled[1], (cr(4, 1), Cell::new("40")));

        let f_src = CellRange::new(cr(1, 2), cr(1, 2));
        let f_tgt = CellRange::new(cr(1, 2), cr(3, 2));
        let f_filled = generate_fill_values(f_src, f_tgt, &sheet);
        assert_eq!(f_filled[0].1.raw, "=A2*2");
        assert_eq!(f_filled[1].1.raw, "=A3*2");

        let extent = double_click_fill_extent(f_src, &sheet);
        assert_eq!(extent, Some(CellRange::new(cr(1, 2), cr(4, 2))));
    }
}
