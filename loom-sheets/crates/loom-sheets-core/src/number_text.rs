//! Excel's General number-to-text conversion: 15 significant digits, plain
//! digits up to 1E+19 and for modest decimals, otherwise scientific.

/// Text of a number as Excel's General format and `&` produce it.
pub(crate) fn general_text(n: f64) -> String {
    if n == 0.0 {
        return "0".to_string();
    }
    if !n.is_finite() {
        return n.to_string();
    }
    let sci = format!("{:.14e}", n.abs());
    let (mantissa, exp) = sci.split_once('e').unwrap_or((&sci, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let digits = digits.trim_end_matches('0');
    let digits = if digits.is_empty() { "0" } else { digits };
    let sign = if n < 0.0 { "-" } else { "" };
    let sig = digits.len() as i32;
    let scientific = exp >= 20 || (exp < 0 && -exp - 1 + sig > 20);
    if scientific {
        let frac = if sig > 1 {
            format!(".{}", &digits[1..])
        } else {
            String::new()
        };
        let exp_sign = if exp < 0 { '-' } else { '+' };
        return format!("{sign}{}{frac}E{exp_sign}{:02}", &digits[..1], exp.abs());
    }
    if exp >= 0 {
        let int_len = exp as usize + 1;
        if digits.len() <= int_len {
            format!("{sign}{digits}{}", "0".repeat(int_len - digits.len()))
        } else {
            format!("{sign}{}.{}", &digits[..int_len], &digits[int_len..])
        }
    } else {
        format!("{sign}0.{}{digits}", "0".repeat((-exp - 1) as usize))
    }
}

#[cfg(test)]
mod tests {
    use super::general_text;

    #[test]
    fn general_text_follows_excel() {
        for (n, want) in [
            (1.0 / 3.0, "0.333333333333333"),
            (1e15, "1000000000000000"),
            (1e19, "10000000000000000000"),
            (123456789012345678.0, "123456789012346000"),
            (1e20, "1E+20"),
            (-1.5e20, "-1.5E+20"),
            (1.5e-5, "0.000015"),
            (1e-11, "0.00000000001"),
            (1.23456789012345e-12, "1.23456789012345E-12"),
            (0.1 + 0.2, "0.3"),
            (-0.0, "0"),
            (255.0, "255"),
            (123_456_789.123_456_79, "123456789.123457"),
        ] {
            assert_eq!(general_text(n), want, "{n:?}");
        }
    }
}
