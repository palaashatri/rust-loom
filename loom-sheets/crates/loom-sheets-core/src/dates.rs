//! Spreadsheet date serial numbers.
//!
//! Excel and Loom store a date as a day count. Day 1 is 1900-01-01 in the
//! 1900 date system, and Excel treats 1900 as a leap year, so serial 60 is a
//! day that never existed (1900-02-29). The time of day is the fraction.

/// Days from 1970-01-01 for a proleptic Gregorian civil date.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month_index = (month + 9) % 12;
    let day_of_year = (153 * month_index + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    (if month <= 2 { year + 1 } else { year }, month, day)
}

/// `yyyy-mm-dd` for a 1900-system date serial; `None` when the number is not
/// a representable date (negative, zero, or beyond year 9999).
pub fn serial_to_iso(serial: f64) -> Option<String> {
    if !serial.is_finite() || serial < 1.0 || serial >= 2_958_466.0 {
        return None;
    }
    let day = serial.floor() as i64;
    if day == 60 {
        return Some("1900-02-29".to_string());
    }
    // Serial 1 is 1900-01-01; the phantom leap day shifts later dates by one.
    let epoch = days_from_civil(1899, 12, 31);
    let days = epoch + if day > 60 { day - 1 } else { day };
    let (year, month, day) = civil_from_days(days);
    Some(format!("{year:04}-{month:02}-{day:02}"))
}

/// Largest valid 1900-system serial (9999-12-31).
pub(crate) const MAX_SERIAL: i64 = 2_958_465;

/// Calendar date `(year, month, day)` of a whole 1900-system serial, with
/// Excel's phantom 1900-02-29 at serial 60 and serial 0 as "1900-01-00".
pub(crate) fn serial_to_ymd(serial: i64) -> Option<(i64, i64, i64)> {
    match serial {
        0 => Some((1900, 1, 0)),
        60 => Some((1900, 2, 29)),
        1..=MAX_SERIAL => {
            let epoch = days_from_civil(1899, 12, 31);
            Some(civil_from_days(
                epoch + if serial > 60 { serial - 1 } else { serial },
            ))
        }
        _ => None,
    }
}

/// Serial of a date; month and day may overflow or be zero/negative and roll
/// into neighbouring months and years like Excel's `DATE`.
pub(crate) fn ymd_to_serial(year: i64, month: i64, day: i64) -> Option<i64> {
    if (year, month, day) == (1900, 2, 29) {
        return Some(60);
    }
    let months = year.checked_mul(12)?.checked_add(month - 1)?;
    let (year, month) = (months.div_euclid(12), months.rem_euclid(12) + 1);
    let days = days_from_civil(year, month, 1).checked_add(day - 1)?;
    let plain = days - days_from_civil(1899, 12, 31);
    let serial = if plain >= 60 { plain + 1 } else { plain };
    (0..=MAX_SERIAL).contains(&serial).then_some(serial)
}

/// Days in a calendar month.
pub(crate) fn days_in_month(year: i64, month: i64) -> i64 {
    let next = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    days_from_civil(next.0, next.1, 1) - days_from_civil(year, month, 1)
}

#[cfg(test)]
mod tests {
    use super::serial_to_iso;

    #[test]
    fn serials_match_what_excel_displays() {
        assert_eq!(serial_to_iso(1.0).as_deref(), Some("1900-01-01"));
        assert_eq!(serial_to_iso(59.0).as_deref(), Some("1900-02-28"));
        assert_eq!(serial_to_iso(60.0).as_deref(), Some("1900-02-29"));
        assert_eq!(serial_to_iso(61.0).as_deref(), Some("1900-03-01"));
        assert_eq!(serial_to_iso(25_569.0).as_deref(), Some("1970-01-01"));
        assert_eq!(serial_to_iso(45_292.0).as_deref(), Some("2024-01-01"));
        assert_eq!(serial_to_iso(45_292.75).as_deref(), Some("2024-01-01"));
        assert_eq!(serial_to_iso(2_958_465.0).as_deref(), Some("9999-12-31"));
    }

    #[test]
    fn non_dates_are_left_alone() {
        assert_eq!(serial_to_iso(0.0), None);
        assert_eq!(serial_to_iso(-3.0), None);
        assert_eq!(serial_to_iso(f64::NAN), None);
        assert_eq!(serial_to_iso(3_000_000.0), None);
    }
}
