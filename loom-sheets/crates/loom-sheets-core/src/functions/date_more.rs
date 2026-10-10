//! Working days, week numbers and date differences (spike C): NETWORKDAYS,
//! NETWORKDAYS.INTL, WORKDAY, WORKDAY.INTL, DATEDIF, WEEKNUM, ISOWEEKNUM and
//! YEARFRAC. Dates are 1900-system serials, read the same way DATE reads them.

use std::collections::HashSet;

use crate::dates::{days_in_month, serial_to_ymd, ymd_to_serial, MAX_SERIAL};
use crate::functions_date::{serial_arg, whole_serial};
use crate::functions_util::{arg_number, arg_text, flatten, opt_number, to_number};
use crate::{eval_expr, is_leap_year, CalcError, CellRef, Expr, Value};

type Lookup<'a> = &'a dyn Fn(CellRef) -> Value;
type Calc = Result<f64, CalcError>;

/// Dispatch the date and working-day functions from the main evaluator.
pub(crate) fn eval_date_more_function(name: &str, args: &[Expr], lookup: Lookup) -> Option<Value> {
    let result = match name {
        "NETWORKDAYS" => count_workdays(args, lookup, false),
        "NETWORKDAYS.INTL" => count_workdays(args, lookup, true),
        "WORKDAY" => workday(args, lookup, false),
        "WORKDAY.INTL" => workday(args, lookup, true),
        "DATEDIF" => datedif(args, lookup),
        "WEEKNUM" => weeknum(args, lookup),
        "ISOWEEKNUM" => isoweeknum(args, lookup),
        "YEARFRAC" => yearfrac(args, lookup),
        _ => return None,
    };
    Some(match result {
        Ok(n) => Value::Number(n),
        Err(error) => Value::Error(error),
    })
}

/// A whole date serial from argument `index`.
fn date_at(args: &[Expr], index: usize, lookup: Lookup) -> Result<i64, CalcError> {
    whole_serial(serial_arg(args, index, lookup)?)
}

fn ymd(serial: i64) -> Result<(i64, i64, i64), CalcError> {
    serial_to_ymd(serial).ok_or(CalcError::Num)
}

/// Monday-first weekday index of a serial (Monday = 0, Sunday = 6). Serial 1
/// is a Sunday, as in Excel.
fn monday_index(serial: i64) -> usize {
    ((serial - 1).rem_euclid(7) + 6).rem_euclid(7) as usize
}

/// Non-working weekdays, Monday first. A code 1-7 names a pair of days, 11-17
/// a single day; a seven-character pattern such as "0000011" marks each day.
fn weekend_mask(value: &Value) -> Result<[bool; 7], CalcError> {
    let mut mask = [false; 7];
    match value {
        Value::Text(pattern) => {
            let days: Vec<char> = pattern.chars().collect();
            let well_formed = days.len() == 7 && days.iter().all(|c| *c == '0' || *c == '1');
            if !well_formed || days.iter().all(|c| *c == '1') {
                return Err(CalcError::Value);
            }
            for (slot, day) in mask.iter_mut().zip(&days) {
                *slot = *day == '1';
            }
        }
        Value::Error(error) => return Err(*error),
        other => {
            let code = to_number(other.clone())?.trunc() as i64;
            let (first, second) = match code {
                1..=7 => {
                    let first = ((code - 1 + 5) % 7) as usize;
                    (first, (first + 1) % 7)
                }
                11..=17 => {
                    let only = ((code - 11 + 6) % 7) as usize;
                    (only, only)
                }
                _ => return Err(CalcError::Num),
            };
            mask[first] = true;
            mask[second] = true;
        }
    }
    Ok(mask)
}

/// Holiday serials from an optional range or array; text and blanks are
/// ignored and errors propagate.
fn holiday_set(expr: Option<&Expr>, lookup: Lookup) -> Result<HashSet<i64>, CalcError> {
    let mut days = HashSet::new();
    if let Some(expr) = expr {
        for value in flatten(expr, lookup) {
            match value {
                Value::Number(n) => {
                    days.insert(n.floor() as i64);
                }
                Value::Error(error) => return Err(error),
                _ => {}
            }
        }
    }
    Ok(days)
}

/// The working-day rules of one call.
struct Calendar {
    weekend: [bool; 7],
    holidays: HashSet<i64>,
}

impl Calendar {
    /// Read the optional weekend argument (at `weekend_index`, when the
    /// function has one) and the holiday argument (at `holiday_index`).
    fn from_args(
        args: &[Expr],
        lookup: Lookup,
        weekend_index: Option<usize>,
        holiday_index: usize,
    ) -> Result<Self, CalcError> {
        let weekend = match weekend_index.filter(|index| *index < args.len()) {
            Some(index) => weekend_mask(&eval_expr(&args[index], lookup))?,
            None => weekend_mask(&Value::Number(1.0))?,
        };
        let holidays = holiday_set(args.get(holiday_index), lookup)?;
        Ok(Self { weekend, holidays })
    }

    fn is_workday(&self, serial: i64) -> bool {
        !self.weekend[monday_index(serial)] && !self.holidays.contains(&serial)
    }
}

/// NETWORKDAYS(start, end, [holidays]) and NETWORKDAYS.INTL(start, end,
/// [weekend], [holidays]): working days from start to end inclusive. A
/// reversed range counts negatively.
fn count_workdays(args: &[Expr], lookup: Lookup, intl: bool) -> Calc {
    let longest = if intl { 4 } else { 3 };
    if !(2..=longest).contains(&args.len()) {
        return Err(CalcError::Value);
    }
    let start = date_at(args, 0, lookup)?;
    let end = date_at(args, 1, lookup)?;
    let calendar = if intl {
        Calendar::from_args(args, lookup, Some(2), 3)?
    } else {
        Calendar::from_args(args, lookup, None, 2)?
    };
    let (low, high, sign) = if start <= end {
        (start, end, 1.0)
    } else {
        (end, start, -1.0)
    };
    let count = (low..=high).filter(|day| calendar.is_workday(*day)).count();
    Ok(sign * count as f64)
}

/// WORKDAY(start, days, [holidays]) and WORKDAY.INTL(start, days, [weekend],
/// [holidays]): the date `days` working days after (or before) start.
fn workday(args: &[Expr], lookup: Lookup, intl: bool) -> Calc {
    let longest = if intl { 4 } else { 3 };
    if !(2..=longest).contains(&args.len()) {
        return Err(CalcError::Value);
    }
    let start = date_at(args, 0, lookup)?;
    let days = arg_number(args, 1, lookup)?.trunc() as i64;
    let calendar = if intl {
        Calendar::from_args(args, lookup, Some(2), 3)?
    } else {
        Calendar::from_args(args, lookup, None, 2)?
    };
    if calendar.weekend.iter().all(|day| *day) {
        return Err(CalcError::Value);
    }
    if days == 0 {
        return Ok(start as f64);
    }
    let step = days.signum();
    let mut remaining = days.abs();
    let mut day = start;
    while remaining > 0 {
        day += step;
        if !(0..=MAX_SERIAL).contains(&day) {
            return Err(CalcError::Num);
        }
        if calendar.is_workday(day) {
            remaining -= 1;
        }
    }
    Ok(day as f64)
}

/// Whole months from the first date to the last, not counting a month that
/// is not yet complete on the day of the month.
fn whole_months(first: (i64, i64, i64), last: (i64, i64, i64)) -> i64 {
    let (_, m1, d1) = first;
    let (y2, m2, d2) = last;
    let (y1, _, _) = first;
    (y2 - y1) * 12 + (m2 - m1) - i64::from(d2 < d1)
}

/// DATEDIF(start, end, unit): "Y", "M" and "D" count whole years, months and
/// days; "YM", "YD" and "MD" count the remainder after whole years and months.
fn datedif(args: &[Expr], lookup: Lookup) -> Calc {
    if args.len() != 3 {
        return Err(CalcError::Value);
    }
    let start = date_at(args, 0, lookup)?;
    let end = date_at(args, 1, lookup)?;
    let unit = arg_text(args, 2, lookup)?.trim().to_ascii_uppercase();
    if start > end {
        return Err(CalcError::Num);
    }
    let first = ymd(start)?;
    let last = ymd(end)?;
    let (_, m1, d1) = first;
    let (y2, m2, d2) = last;
    let months = whole_months(first, last);
    let value = match unit.as_str() {
        "Y" => months.div_euclid(12),
        "M" => months,
        "D" => end - start,
        "YM" => months.rem_euclid(12),
        "MD" => {
            if d2 >= d1 {
                d2 - d1
            } else {
                let (year, month) = if m2 == 1 { (y2 - 1, 12) } else { (y2, m2 - 1) };
                (days_in_month(year, month) + d2 - d1).max(0)
            }
        }
        "YD" => {
            let year = if (m1, d1) <= (m2, d2) { y2 } else { y2 - 1 };
            let anchor = ymd_to_serial(year, m1, d1).ok_or(CalcError::Num)?;
            end - anchor
        }
        _ => return Err(CalcError::Num),
    };
    Ok(value as f64)
}

/// The ISO-8601 week number: the week holds the Thursday of its days.
fn iso_week(serial: i64) -> Result<i64, CalcError> {
    let thursday = serial - monday_index(serial) as i64 + 3;
    let (year, _, _) = ymd(thursday)?;
    let jan1 = ymd_to_serial(year, 1, 1).ok_or(CalcError::Num)?;
    Ok((thursday - jan1) / 7 + 1)
}

/// WEEKNUM(serial, [return_type]): the week containing January 1 is week 1.
/// Return types 1 (Sunday start), 2 (Monday), 11-17 (start on the given day)
/// and 21 (ISO-8601).
fn weeknum(args: &[Expr], lookup: Lookup) -> Calc {
    if !(1..=2).contains(&args.len()) {
        return Err(CalcError::Value);
    }
    let serial = date_at(args, 0, lookup)?;
    let kind = opt_number(args, 1, 1.0, lookup)?.trunc() as i64;
    if kind == 21 {
        return Ok(iso_week(serial)? as f64);
    }
    // Index of the week's first day, Sunday = 0.
    let first_day: i64 = match kind {
        1 => 0,
        2 => 1,
        11..=17 => (kind - 10) % 7,
        _ => return Err(CalcError::Num),
    };
    let (year, _, _) = ymd(serial)?;
    let jan1 = ymd_to_serial(year, 1, 1).ok_or(CalcError::Num)?;
    let jan1_sunday = (jan1 - 1).rem_euclid(7);
    let offset = (jan1_sunday - first_day).rem_euclid(7);
    let day_of_year = serial - jan1;
    Ok(((day_of_year + offset) / 7 + 1) as f64)
}

/// ISOWEEKNUM(serial): the ISO-8601 week number.
fn isoweeknum(args: &[Expr], lookup: Lookup) -> Calc {
    if args.len() != 1 {
        return Err(CalcError::Value);
    }
    Ok(iso_week(date_at(args, 0, lookup)?)? as f64)
}

/// US (NASD) 30/360 day count, with Excel's February end-of-month rules.
fn us_30_360(first: (i64, i64, i64), last: (i64, i64, i64)) -> i64 {
    let (y1, m1, mut d1) = first;
    let (y2, m2, mut d2) = last;
    let last_of_feb = |y: i64, m: i64, d: i64| m == 2 && d == days_in_month(y, 2);
    if last_of_feb(y1, m1, d1) && last_of_feb(y2, m2, d2) {
        d2 = 30;
    }
    if last_of_feb(y1, m1, d1) {
        d1 = 30;
    }
    if d1 == 31 {
        d1 = 30;
    }
    if d2 == 31 && d1 >= 30 {
        d2 = 30;
    }
    360 * (y2 - y1) + 30 * (m2 - m1) + (d2 - d1)
}

/// European 30/360 day count: any 31st becomes the 30th.
fn eu_30_360(first: (i64, i64, i64), last: (i64, i64, i64)) -> i64 {
    let (y1, m1, d1) = first;
    let (y2, m2, d2) = last;
    let d1 = if d1 == 31 { 30 } else { d1 };
    let d2 = if d2 == 31 { 30 } else { d2 };
    360 * (y2 - y1) + 30 * (m2 - m1) + (d2 - d1)
}

fn year_days(year: i64) -> f64 {
    if is_leap_year(year as i32) {
        366.0
    } else {
        365.0
    }
}

/// The year length used by actual/actual: the year itself within one year,
/// 366 when the period contains February 29, otherwise the mean year length
/// over the years the period touches.
fn actual_year_length(first: (i64, i64, i64), last: (i64, i64, i64)) -> f64 {
    let (y1, m1, d1) = first;
    let (y2, m2, d2) = last;
    if y1 == y2 {
        return year_days(y1);
    }
    if y2 == y1 + 1 && (m2, d2) <= (m1, d1) {
        let leap_day_inside = (is_leap_year(y1 as i32) && (m1, d1) <= (2, 29))
            || (is_leap_year(y2 as i32) && (m2, d2) >= (2, 29));
        return if leap_day_inside { 366.0 } else { 365.0 };
    }
    let total: f64 = (y1..=y2).map(year_days).sum();
    total / (y2 - y1 + 1) as f64
}

/// YEARFRAC(start, end, [basis]): the fraction of a year between two dates.
/// Basis 0 is US 30/360, 1 actual/actual, 2 actual/360, 3 actual/365 and 4
/// European 30/360. The dates may come in either order.
fn yearfrac(args: &[Expr], lookup: Lookup) -> Calc {
    if !(2..=3).contains(&args.len()) {
        return Err(CalcError::Value);
    }
    let (mut start, mut end) = (date_at(args, 0, lookup)?, date_at(args, 1, lookup)?);
    if start > end {
        std::mem::swap(&mut start, &mut end);
    }
    let basis = opt_number(args, 2, 0.0, lookup)?.trunc() as i64;
    if !(0..=4).contains(&basis) {
        return Err(CalcError::Num);
    }
    let first = ymd(start)?;
    let last = ymd(end)?;
    let days = (end - start) as f64;
    Ok(match basis {
        0 => us_30_360(first, last) as f64 / 360.0,
        1 => days / actual_year_length(first, last),
        2 => days / 360.0,
        3 => days / 365.0,
        _ => eu_30_360(first, last) as f64 / 360.0,
    })
}
