//! Extended number formatting: Accounting, Fraction, Time/Duration, and custom patterns.

/// Formats a numeric value using Accounting format (currency symbol + parenthesized negatives).
pub fn format_accounting(value: f64, decimals: u8, symbol: &str) -> String {
    let dec = decimals as usize;
    if value < 0.0 {
        let abs_s = format_grouped_number(-value, dec);
        format!("{symbol} ({abs_s})")
    } else if value == 0.0 {
        format!("{symbol} -")
    } else {
        let s = format_grouped_number(value, dec);
        format!("{symbol} {s}")
    }
}

/// Formats a numeric value as a mixed fraction (e.g. `1 3/4` or `1/2`).
pub fn format_fraction(value: f64, max_denom: u32) -> String {
    if !value.is_finite() {
        return value.to_string();
    }
    let sign = if value < 0.0 { "-" } else { "" };
    let abs_v = value.abs();
    let whole = abs_v.floor() as u64;
    let frac = abs_v - whole as f64;
    if frac < 1e-9 {
        return format!("{sign}{whole}");
    }

    let (num, den) = best_rational_approx(frac, max_denom.max(2));
    if num == 0 {
        format!("{sign}{whole}")
    } else if num == den {
        format!("{sign}{}", whole + 1)
    } else if whole == 0 {
        format!("{sign}{num}/{den}")
    } else {
        format!("{sign}{whole} {num}/{den}")
    }
}

fn best_rational_approx(x: f64, max_denom: u32) -> (u32, u32) {
    let mut best_num = 0u32;
    let mut best_den = 1u32;
    let mut best_err = x;
    for d in 1..=max_denom {
        let n = (x * d as f64).round() as u32;
        let err = (x - n as f64 / d as f64).abs();
        if err < best_err {
            best_err = err;
            best_num = n;
            best_den = d;
            if err < 1e-9 {
                break;
            }
        }
    }
    (best_num, best_den)
}

/// Formats a fractional-day serial number as time or duration.
///
/// Supports `h:mm`, `h:mm:ss`, `[h]:mm:ss` (elapsed hours), and `AM/PM`.
pub fn format_time_duration(serial_days: f64, pattern: &str) -> String {
    let total_secs = (serial_days * 86400.0).round() as i64;
    let sign = if total_secs < 0 { "-" } else { "" };
    let abs_secs = total_secs.unsigned_abs();
    let elapsed_hours = abs_secs / 3600;
    let mins = (abs_secs % 3600) / 60;
    let secs = abs_secs % 60;

    let lower = pattern.to_ascii_lowercase();
    let has_seconds = lower.contains("ss") || lower.contains(":s");
    let is_elapsed = lower.contains("[h]");
    let is_ampm = lower.contains("am/pm") || lower.contains("a/p");

    if is_elapsed {
        if has_seconds {
            format!("{sign}{elapsed_hours}:{mins:02}:{secs:02}")
        } else {
            format!("{sign}{elapsed_hours}:{mins:02}")
        }
    } else if is_ampm {
        let h24 = (elapsed_hours % 24) as u32;
        let suffix = if h24 < 12 { "AM" } else { "PM" };
        let h12 = match h24 % 12 {
            0 => 12,
            h => h,
        };
        if has_seconds {
            format!("{sign}{h12}:{mins:02}:{secs:02} {suffix}")
        } else {
            format!("{sign}{h12}:{mins:02} {suffix}")
        }
    } else {
        let h24 = elapsed_hours % 24;
        if has_seconds {
            format!("{sign}{h24:02}:{mins:02}:{secs:02}")
        } else {
            format!("{sign}{h24:02}:{mins:02}")
        }
    }
}

/// Formats a numeric value using a custom Excel-style format string.
///
/// Handles multi-section formats (`pos;neg;zero`), grouped thousands (`#,##0.00`),
/// percentages (`0.00%`), scientific (`0.00E+00`), accounting, fractions, and time/duration.
pub fn format_custom_number(value: f64, pattern: &str) -> String {
    let sections: Vec<&str> = pattern.split(';').collect();
    let section = if value > 0.0 {
        sections[0]
    } else if value < 0.0 {
        sections.get(1).copied().unwrap_or(sections[0])
    } else {
        sections.get(2).copied().unwrap_or_else(|| sections[0])
    };

    let lower = section.to_ascii_lowercase();
    if lower.contains("h:mm") || lower.contains("[h]") {
        return format_time_duration(value, section);
    }
    if section.contains('?') && section.contains('/') {
        let max_denom = if section.contains("???") {
            999
        } else if section.contains("??") {
            99
        } else {
            9
        };
        return format_fraction(value, max_denom);
    }
    if section.contains('$') && section.contains("* ") {
        let dec = count_decimals(section);
        return format_accounting(value, dec, "$");
    }
    if section.contains('%') {
        let dec = count_decimals(section);
        let pct = value * 100.0;
        return format!("{:.*}%", dec as usize, pct);
    }
    if lower.contains("e+") || lower.contains("e-") {
        let dec = count_decimals(section);
        return format!("{:.*E}", dec as usize, value);
    }

    // Standard numeric pattern (e.g. `#,##0.00` or `0.00`)
    let clean = section.replace('"', "");
    if clean == "-" && value == 0.0 {
        return "-".to_string();
    }
    let dec = count_decimals(section);
    let grouped = section.contains(',');
    let prefix = if section.trim_start().starts_with('$') {
        "$"
    } else {
        ""
    };
    let use_parens = sections.len() >= 2 && value < 0.0 && section.contains('(');
    let abs_v = value.abs();
    let num_str = if grouped {
        format_grouped_number(abs_v, dec as usize)
    } else {
        format!("{abs_v:.*}", dec as usize)
    };

    if use_parens {
        format!("({prefix}{num_str})")
    } else if value < 0.0 && sections.len() == 1 {
        format!("-{prefix}{num_str}")
    } else {
        format!("{prefix}{num_str}")
    }
}

fn count_decimals(pattern: &str) -> u8 {
    if let Some((_, frac)) = pattern.split_once('.') {
        frac.chars().take_while(|c| *c == '0' || *c == '#').count() as u8
    } else {
        0
    }
}

fn format_grouped_number(abs_val: f64, decimals: usize) -> String {
    let raw = format!("{abs_val:.decimals$}");
    let (int_part, frac_part) = match raw.split_once('.') {
        Some((i, f)) => (i, Some(f)),
        None => (raw.as_str(), None),
    };
    let mut grouped = String::with_capacity(int_part.len() + int_part.len() / 3);
    for (idx, ch) in int_part.chars().rev().enumerate() {
        if idx > 0 && idx % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    let int_grouped: String = grouped.chars().rev().collect();
    match frac_part {
        Some(f) => format!("{int_grouped}.{f}"),
        None => int_grouped,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_accounting_fraction_time_and_custom() {
        assert_eq!(format_accounting(1234.5, 2, "$"), "$ 1,234.50");
        assert_eq!(format_accounting(-42.0, 2, "$"), "$ (42.00)");
        assert_eq!(format_accounting(0.0, 2, "$"), "$ -");

        assert_eq!(format_fraction(1.75, 9), "1 3/4");
        assert_eq!(format_fraction(0.5, 9), "1/2");

        assert_eq!(format_time_duration(0.5, "h:mm"), "12:00");
        assert_eq!(format_time_duration(1.25, "[h]:mm:ss"), "30:00:00");
        assert_eq!(format_time_duration(0.75, "h:mm AM/PM"), "6:00 PM");

        assert_eq!(format_custom_number(1234.567, "#,##0.00"), "1,234.57");
        assert_eq!(format_custom_number(0.256, "0.0%"), "25.6%");
    }
}
