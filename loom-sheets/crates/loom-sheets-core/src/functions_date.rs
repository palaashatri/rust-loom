//! Date and time functions: DATE, YEAR, MONTH, DAY, WEEKDAY, EDATE, EOMONTH,
//! DAYS, TIME, HOUR, MINUTE, SECOND, DATEVALUE.
//!
//! Serials follow Excel's 1900 system (including its phantom 1900-02-29); the
//! calendar arithmetic lives in [`crate::dates`].

use crate::dates::{days_in_month, serial_to_ymd, ymd_to_serial, MAX_SERIAL};
use crate::functions_util::{arg_number, opt_number, to_number};
use crate::{eval_expr, CalcError, CellRef, Expr, Value};

type Lookup<'a> = &'a dyn Fn(CellRef) -> Value;
type Calc = Result<f64, CalcError>;

const SECONDS_PER_DAY: f64 = 86_400.0;

/// Dispatch date/time functions from the main evaluator.
pub(crate) fn eval_date_function(name: &str, args: &[Expr], lookup: Lookup) -> Option<Value> {
    let result = match name {
        "DATE" => date(args, lookup),
        "YEAR" => part(args, lookup, |(y, _, _)| y),
        "MONTH" => part(args, lookup, |(_, m, _)| m),
        "DAY" => part(args, lookup, |(_, _, d)| d),
        "WEEKDAY" => weekday(args, lookup),
        "EDATE" => shift_months(args, lookup, false),
        "EOMONTH" => shift_months(args, lookup, true),
        "DAYS" => days(args, lookup),
        "TIME" => time(args, lookup),
        "HOUR" => time_part(args, lookup, 3600, 24),
        "MINUTE" => time_part(args, lookup, 60, 60),
        "SECOND" => time_part(args, lookup, 1, 60),
        "DATEVALUE" => datevalue(args, lookup),
        _ => return None,
    };
    Some(match result {
        Ok(n) => Value::Number(n),
        Err(error) => Value::Error(error),
    })
}

/// A date argument: a number, or text that reads as a date or a time.
pub(crate) fn serial_arg(args: &[Expr], index: usize, lookup: Lookup) -> Calc {
    match eval_expr(&args[index], lookup) {
        Value::Text(text) => match text.trim().parse::<f64>() {
            Ok(n) if n.is_finite() => Ok(n),
            _ => parse_date_text(&text)
                .or_else(|| parse_time_text(&text))
                .ok_or(CalcError::Value),
        },
        other => to_number(other),
    }
}

pub(crate) fn whole_serial(serial: f64) -> Result<i64, CalcError> {
    if !(0.0..(MAX_SERIAL + 1) as f64).contains(&serial) {
        return Err(CalcError::Num);
    }
    Ok(serial.floor() as i64)
}

fn date(args: &[Expr], lookup: Lookup) -> Calc {
    if args.len() != 3 {
        return Err(CalcError::Value);
    }
    let mut year = arg_number(args, 0, lookup)?.trunc();
    let month = arg_number(args, 1, lookup)?.trunc();
    let day = arg_number(args, 2, lookup)?.trunc();
    if !(0.0..10_000.0).contains(&year) {
        return Err(CalcError::Num);
    }
    if year < 1900.0 {
        year += 1900.0;
    }
    ymd_to_serial(year as i64, month as i64, day as i64)
        .map(|serial| serial as f64)
        .ok_or(CalcError::Num)
}

fn part(args: &[Expr], lookup: Lookup, pick: impl Fn((i64, i64, i64)) -> i64) -> Calc {
    if args.len() != 1 {
        return Err(CalcError::Value);
    }
    let serial = whole_serial(serial_arg(args, 0, lookup)?)?;
    serial_to_ymd(serial)
        .map(|ymd| pick(ymd) as f64)
        .ok_or(CalcError::Num)
}

/// Day number of the week with Sunday = 0; Excel's serial 1 is a Sunday.
fn days_since_sunday(serial: i64) -> i64 {
    (serial - 1).rem_euclid(7)
}

fn weekday(args: &[Expr], lookup: Lookup) -> Calc {
    if args.is_empty() || args.len() > 2 {
        return Err(CalcError::Value);
    }
    let serial = whole_serial(serial_arg(args, 0, lookup)?)?;
    let kind = opt_number(args, 1, 1.0, lookup)?.trunc() as i64;
    let sunday_based = days_since_sunday(serial);
    let (first_day, base) = match kind {
        1 => (0, 1),
        2 => (1, 1),
        3 => (1, 0),
        11..=17 => ((kind - 10) % 7, 1),
        _ => return Err(CalcError::Num),
    };
    Ok(((sunday_based - first_day).rem_euclid(7) + base) as f64)
}

/// EDATE (same day, clamped to the month's end) and EOMONTH (month's last day).
fn shift_months(args: &[Expr], lookup: Lookup, to_month_end: bool) -> Calc {
    if args.len() != 2 {
        return Err(CalcError::Value);
    }
    let serial = whole_serial(serial_arg(args, 0, lookup)?)?;
    let months = arg_number(args, 1, lookup)?.trunc() as i64;
    let (year, month, day) = serial_to_ymd(serial).ok_or(CalcError::Num)?;
    let total = year * 12 + (month - 1) + months;
    let (year, month) = (total.div_euclid(12), total.rem_euclid(12) + 1);
    let last = days_in_month(year, month);
    let target_day = if to_month_end { last } else { day.min(last) };
    ymd_to_serial(year, month, target_day)
        .map(|serial| serial as f64)
        .ok_or(CalcError::Num)
}

/// DAYS(end, start): whole days from `start` to `end`.
fn days(args: &[Expr], lookup: Lookup) -> Calc {
    if args.len() != 2 {
        return Err(CalcError::Value);
    }
    let end = serial_arg(args, 0, lookup)?;
    let start = serial_arg(args, 1, lookup)?;
    Ok(end.floor() - start.floor())
}

fn time(args: &[Expr], lookup: Lookup) -> Calc {
    if args.len() != 3 {
        return Err(CalcError::Value);
    }
    let hours = arg_number(args, 0, lookup)?.trunc();
    let minutes = arg_number(args, 1, lookup)?.trunc();
    let seconds = arg_number(args, 2, lookup)?.trunc();
    if [hours, minutes, seconds]
        .iter()
        .any(|part| !(-32768.0..=32767.0).contains(part))
    {
        return Err(CalcError::Num);
    }
    let total = hours * 3600.0 + minutes * 60.0 + seconds;
    if total < 0.0 {
        return Err(CalcError::Num);
    }
    Ok(total.rem_euclid(SECONDS_PER_DAY) / SECONDS_PER_DAY)
}

/// HOUR/MINUTE/SECOND from the fractional day, rounded to the nearest second.
fn time_part(args: &[Expr], lookup: Lookup, unit_seconds: i64, modulus: i64) -> Calc {
    if args.len() != 1 {
        return Err(CalcError::Value);
    }
    let serial = serial_arg(args, 0, lookup)?;
    if serial < 0.0 {
        return Err(CalcError::Num);
    }
    let total = (serial.fract() * SECONDS_PER_DAY).round() as i64;
    Ok(((total / unit_seconds) % modulus) as f64)
}

fn datevalue(args: &[Expr], lookup: Lookup) -> Calc {
    if args.len() != 1 {
        return Err(CalcError::Value);
    }
    match eval_expr(&args[0], lookup) {
        // Text that is only a time of day has no date part: day 0.
        Value::Text(text) => parse_date_text(&text)
            .or_else(|| parse_time_text(&text).map(|_| 0.0))
            .ok_or(CalcError::Value),
        Value::Error(error) => Err(error),
        _ => Err(CalcError::Value),
    }
}

const MONTHS: [&str; 12] = [
    "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
];

fn month_from_name(word: &str) -> Option<i64> {
    let lower = word.trim_end_matches('.').to_ascii_lowercase();
    if lower.len() < 3 {
        return None;
    }
    MONTHS
        .iter()
        .position(|short| lower.starts_with(short))
        .map(|index| index as i64 + 1)
}

/// Whole-day serial of a date written as `2024-01-31`, `1/31/2024`,
/// `Jan 31, 2024`, `31 Jan 2024` or `31-Jan-2024`; a trailing time is ignored.
pub(crate) fn parse_date_text(text: &str) -> Option<f64> {
    let text = text.trim();
    let date_part = text
        .split_once('T')
        .map(|(date, _)| date)
        .or_else(|| {
            text.split_once(' ')
                .filter(|(_, t)| t.contains(':'))
                .map(|(d, _)| d)
        })
        .unwrap_or(text);
    let number = |s: &str| s.parse::<i64>().ok();
    let (year, month, day) = if let Some(parts) = split3(date_part, '-') {
        if parts.0.len() == 4 {
            // 2024-01-31 or 2024-Jan-31
            (
                number(parts.0)?,
                number(parts.1).or_else(|| month_from_name(parts.1))?,
                number(parts.2)?,
            )
        } else {
            // 31-Jan-2024
            (
                number(parts.2)?,
                number(parts.1).or_else(|| month_from_name(parts.1))?,
                number(parts.0)?,
            )
        }
    } else if let Some(parts) = split3(date_part, '/') {
        if parts.0.len() == 4 {
            // 2024/03/15
            (number(parts.0)?, number(parts.1)?, number(parts.2)?)
        } else {
            // US order, as Excel reads it in an en-US locale.
            (number(parts.2)?, number(parts.0)?, number(parts.1)?)
        }
    } else {
        let words: Vec<&str> = text.split([' ', ',']).filter(|w| !w.is_empty()).collect();
        match words.as_slice() {
            [a, b, y] => match month_from_name(a) {
                // `Jan 31, 2024`
                Some(month) => (number(y)?, month, number(b)?),
                // `31 Jan 2024`
                None => (number(y)?, month_from_name(b)?, number(a)?),
            },
            _ => return None,
        }
    };
    let year = if year < 100 {
        if year < 30 {
            2000 + year
        } else {
            1900 + year
        }
    } else {
        year
    };
    if !(1..=12).contains(&month) || day < 1 || day > days_in_month(year, month) {
        return None;
    }
    ymd_to_serial(year, month, day).map(|serial| serial as f64)
}

fn split3(text: &str, sep: char) -> Option<(&str, &str, &str)> {
    let mut parts = text.split(sep);
    let triple = (parts.next()?, parts.next()?, parts.next()?);
    parts.next().is_none().then_some(triple)
}

/// Fraction of a day for `13:45`, `13:45:30` or `1:45 PM`.
pub(crate) fn parse_time_text(text: &str) -> Option<f64> {
    let upper = text.trim().to_ascii_uppercase();
    let (clock, meridiem) = match upper.strip_suffix("PM") {
        Some(rest) => (rest.trim(), Some(true)),
        None => match upper.strip_suffix("AM") {
            Some(rest) => (rest.trim(), Some(false)),
            None => (upper.as_str(), None),
        },
    };
    let mut parts = clock.split(':');
    let mut hours: f64 = parts.next()?.trim().parse().ok()?;
    let minutes: f64 = parts.next()?.trim().parse().ok()?;
    let seconds: f64 = parts.next().map_or(Some(0.0), |s| s.trim().parse().ok())?;
    if parts.next().is_some() || hours < 0.0 || !(0.0..60.0).contains(&minutes) {
        return None;
    }
    if let Some(pm) = meridiem {
        if !(1.0..=12.0).contains(&hours) {
            return None;
        }
        hours = hours % 12.0 + if pm { 12.0 } else { 0.0 };
    }
    if hours >= 24.0 || !(0.0..60.0).contains(&seconds) {
        return None;
    }
    Some((hours * 3600.0 + minutes * 60.0 + seconds) / SECONDS_PER_DAY)
}

#[cfg(test)]
mod tests {
    use crate::{evaluate, CalcError, CellRef, Sheet, Value};

    fn eval(formula: &str) -> Value {
        let mut sheet = Sheet::new("t");
        sheet.set_str("Z1", formula);
        evaluate(&sheet)
            .get(&CellRef::parse("Z1").unwrap())
            .cloned()
            .unwrap_or(Value::Empty)
    }

    fn num(formula: &str) -> f64 {
        match eval(formula) {
            Value::Number(n) => n,
            other => panic!("{formula} -> {other:?}"),
        }
    }

    #[test]
    fn date_matches_excel_serials_including_the_1900_quirk() {
        assert_eq!(num("=DATE(2024,1,1)"), 45292.0);
        assert_eq!(num("=DATE(1900,1,1)"), 1.0);
        assert_eq!(num("=DATE(1900,2,28)"), 59.0);
        assert_eq!(num("=DATE(1900,2,29)"), 60.0);
        assert_eq!(num("=DATE(1900,3,1)"), 61.0);
        assert_eq!(num("=DATE(9999,12,31)"), 2_958_465.0);
        assert_eq!(num("=DATE(99,1,1)"), 36_161.0);
    }

    #[test]
    fn date_rolls_months_and_days_over_like_excel() {
        assert_eq!(num("=DATE(2024,13,1)"), 45_658.0);
        assert_eq!(num("=DATE(2024,3,0)"), 45_351.0);
        assert_eq!(num("=DATE(2024,0,1)"), 45_261.0);
        assert_eq!(num("=DATE(2024,1,32)"), 45_323.0);
        assert_eq!(num("=DATE(2024.9,1.9,1.9)"), 45_292.0);
        assert_eq!(eval("=DATE(10000,1,1)"), Value::Error(CalcError::Num));
        assert_eq!(eval("=DATE(2024,1)"), Value::Error(CalcError::Value));
    }

    #[test]
    fn year_month_day_are_the_inverse_of_date() {
        assert_eq!(num("=YEAR(45292)"), 2024.0);
        assert_eq!(num("=MONTH(45351)"), 2.0);
        assert_eq!(num("=DAY(45351)"), 29.0);
        assert_eq!(
            num("=YEAR(DATE(2023,12,31))*10000+MONTH(DATE(2023,12,31))*100+DAY(DATE(2023,12,31))"),
            20_231_231.0
        );
        // Serial 60 is the day that never existed; serial 0 is "1900-01-00".
        assert_eq!(num("=MONTH(60)"), 2.0);
        assert_eq!(num("=DAY(60)"), 29.0);
        assert_eq!(num("=DAY(0)"), 0.0);
        assert_eq!(eval("=YEAR(-1)"), Value::Error(CalcError::Num));
        assert_eq!(eval("=YEAR(3000000)"), Value::Error(CalcError::Num));
        assert_eq!(num("=DAY(45351.99)"), 29.0);
    }

    #[test]
    fn weekday_return_types() {
        // 2024-01-01 was a Monday.
        assert_eq!(num("=WEEKDAY(45292)"), 2.0);
        assert_eq!(num("=WEEKDAY(45292,1)"), 2.0);
        assert_eq!(num("=WEEKDAY(45292,2)"), 1.0);
        assert_eq!(num("=WEEKDAY(45292,3)"), 0.0);
        assert_eq!(num("=WEEKDAY(45292,11)"), 1.0);
        assert_eq!(num("=WEEKDAY(45292,17)"), 2.0);
        assert_eq!(num("=WEEKDAY(45298)"), 1.0);
        assert_eq!(num("=WEEKDAY(45298,3)"), 6.0);
        assert_eq!(num("=WEEKDAY(DATE(1900,3,1))"), 5.0);
        assert_eq!(eval("=WEEKDAY(45292,4)"), Value::Error(CalcError::Num));
    }

    #[test]
    fn edate_clamps_the_day_and_eomonth_finds_the_last_day() {
        assert_eq!(num("=EDATE(DATE(2024,1,31),1)"), 45_351.0);
        assert_eq!(num("=EDATE(DATE(2024,3,31),-1)"), 45_351.0);
        assert_eq!(num("=EDATE(DATE(2024,1,15),12)"), num("=DATE(2025,1,15)"));
        assert_eq!(num("=EOMONTH(DATE(2024,1,15),0)"), 45_322.0);
        assert_eq!(num("=EOMONTH(DATE(2024,1,15),-1)"), 45_291.0);
        assert_eq!(num("=EOMONTH(DATE(2024,1,15),13)"), num("=DATE(2025,2,28)"));
        assert_eq!(eval("=EDATE(-5,1)"), Value::Error(CalcError::Num));
    }

    #[test]
    fn days_counts_from_start_to_end() {
        assert_eq!(num("=DAYS(DATE(2024,1,5),DATE(2024,1,1))"), 4.0);
        assert_eq!(num("=DAYS(DATE(2024,1,1),DATE(2024,1,5))"), -4.0);
        assert_eq!(num("=DAYS(\"2024-03-01\",\"2024-02-01\")"), 29.0);
        assert_eq!(num("=DAYS(45292.9,45292.1)"), 0.0);
    }

    #[test]
    fn time_and_its_parts() {
        assert!((num("=TIME(12,30,0)") - 0.520_833_333_333_333_3).abs() < 1e-12);
        assert!((num("=TIME(25,0,0)") - 1.0 / 24.0).abs() < 1e-12);
        assert!((num("=TIME(0,90,0)") - 0.0625).abs() < 1e-12);
        assert_eq!(eval("=TIME(0,0,-1)"), Value::Error(CalcError::Num));
        assert_eq!(num("=HOUR(0.75)"), 18.0);
        assert_eq!(num("=HOUR(TIME(14,30,45))"), 14.0);
        assert_eq!(num("=MINUTE(TIME(14,30,45))"), 30.0);
        assert_eq!(num("=SECOND(TIME(14,30,45))"), 45.0);
        assert_eq!(num("=HOUR(45292.5)"), 12.0);
        // Rounds to the nearest second instead of truncating.
        assert_eq!(num("=SECOND(0.5+0.4/86400)"), 0.0);
        assert_eq!(num("=SECOND(0.5+0.6/86400)"), 1.0);
        assert_eq!(num("=HOUR(\"13:45\")"), 13.0);
        assert_eq!(eval("=HOUR(-0.5)"), Value::Error(CalcError::Num));
    }

    #[test]
    fn datevalue_reads_common_date_text() {
        assert_eq!(num("=DATEVALUE(\"2024-01-01\")"), 45_292.0);
        assert_eq!(num("=DATEVALUE(\"1/31/2024\")"), 45_322.0);
        assert_eq!(num("=DATEVALUE(\"Jan 31, 2024\")"), 45_322.0);
        assert_eq!(num("=DATEVALUE(\"31 January 2024\")"), 45_322.0);
        assert_eq!(num("=DATEVALUE(\"31-Jan-2024\")"), 45_322.0);
        assert_eq!(num("=DATEVALUE(\"2024-01-01 10:30\")"), 45_292.0);
        assert_eq!(
            eval("=DATEVALUE(\"2024-02-30\")"),
            Value::Error(CalcError::Value)
        );
        assert_eq!(eval("=DATEVALUE(\"nope\")"), Value::Error(CalcError::Value));
        assert_eq!(eval("=DATEVALUE(45292)"), Value::Error(CalcError::Value));
    }

    #[test]
    fn date_text_is_accepted_where_a_date_is_expected() {
        assert_eq!(num("=YEAR(\"2024-06-15\")"), 2024.0);
        assert_eq!(num("=MONTH(\"2024-06-15\")"), 6.0);
        assert_eq!(eval("=YEAR(\"hello\")"), Value::Error(CalcError::Value));
        assert_eq!(eval("=YEAR(1/0)"), Value::Error(CalcError::DivZero));
    }
}
