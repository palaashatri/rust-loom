//! Fraction formats such as `# ?/?`, `?/8` and `# ??/16` (spike C). The integer
//! placeholders keep their place. The numerator and denominator are either
//! placeholders (the denominator is the closest fraction within its digits) or a
//! fixed denominator. A value with no fraction part leaves the fraction blank,
//! and a format with no integer placeholder folds the whole part into the
//! numerator, so `?/?` shows 3/2 for 1.5.

use super::{layout_integer, push_literal, tokenize};
use crate::CalcError;

/// A parsed fraction section, split at its `/`.
#[derive(Debug, PartialEq)]
pub(super) struct Fraction {
    /// Literal text before the integer placeholders.
    prefix: String,
    /// Integer placeholders with the literal text among them (maybe empty).
    integer: String,
    /// Numerator placeholders such as `?` or `??`.
    numerator: String,
    /// Denominator placeholders (`?`) or fixed digits (`8`).
    denominator: String,
    /// Literal text after the denominator.
    suffix: String,
}

const PLACEHOLDERS: [char; 3] = ['?', '#', '0'];

/// The first `/` outside quotes, brackets and escapes.
fn find_slash(chars: &[char]) -> Option<usize> {
    let (mut quoted, mut bracket, mut escaped) = (false, false, false);
    for (index, c) in chars.iter().enumerate() {
        if escaped {
            escaped = false;
            continue;
        }
        match c {
            '\\' if !quoted => escaped = true,
            '"' => quoted = !quoted,
            '[' if !quoted => bracket = true,
            ']' if !quoted => bracket = false,
            '/' if !quoted && !bracket => return Some(index),
            _ => {}
        }
    }
    None
}

/// Parse `section` as a fraction format, or `None` when no `/` sits between
/// placeholders.
pub(super) fn parse(section: &str) -> Option<Fraction> {
    let chars: Vec<char> = section.chars().collect();
    let slash = find_slash(&chars)?;
    let mut start = slash;
    while start > 0 && PLACEHOLDERS.contains(&chars[start - 1]) {
        start -= 1;
    }
    if start == slash {
        return None;
    }
    let mut end = slash + 1;
    while end < chars.len() && (PLACEHOLDERS.contains(&chars[end]) || chars[end].is_ascii_digit()) {
        end += 1;
    }
    if end == slash + 1 {
        return None;
    }
    let text = |range: std::ops::Range<usize>| chars[range].iter().collect::<String>();
    let integer_start = chars[..start]
        .iter()
        .position(|c| PLACEHOLDERS.contains(c))
        .unwrap_or(start);
    Some(Fraction {
        prefix: text(0..integer_start),
        integer: text(integer_start..start),
        numerator: text(start..slash),
        denominator: text(slash + 1..end),
        suffix: text(end..chars.len()),
    })
}

/// The closest fraction p/q to `x` (0 <= x < 1) with q at most `max_den`, from
/// the convergents of `x` and the best semiconvergent, as in Python's
/// `limit_denominator`.
fn best_fraction(x: f64, max_den: u64) -> (u64, u64) {
    let (mut p0, mut q0, mut p1, mut q1) = (0u64, 1u64, 1u64, 0u64);
    let mut rest = x;
    for _ in 0..64 {
        let whole = rest.floor();
        let term = whole as u64;
        let q2 = term.saturating_mul(q1).saturating_add(q0);
        if q2 > max_den {
            break;
        }
        let p2 = term.saturating_mul(p1).saturating_add(p0);
        (p0, q0, p1, q1) = (p1, q1, p2, q2);
        let fraction = rest - whole;
        if fraction < 1e-15 {
            break;
        }
        rest = 1.0 / fraction;
    }
    if q1 == 0 {
        return (0, 1);
    }
    let k = (max_den - q0) / q1;
    let semi = (p0 + k * p1, q0 + k * q1);
    let convergent = (p1, q1);
    let error = |(p, q): (u64, u64)| (p as f64 / q as f64 - x).abs();
    if error(convergent) <= error(semi) {
        convergent
    } else {
        semi
    }
}

/// Render a non-negative number in a fraction format. The caller adds a sign.
pub(super) fn render(fraction: &Fraction, n: f64) -> Result<String, CalcError> {
    let numerator_width = fraction.numerator.chars().count();
    let denominator_width = fraction.denominator.chars().count();
    let fixed = fraction
        .denominator
        .chars()
        .all(|c| c.is_ascii_digit())
        .then(|| fraction.denominator.parse::<u64>().ok())
        .flatten()
        .filter(|d| *d > 0);
    let (mut whole, mut numerator, denominator) = match fixed {
        Some(d) => {
            let total = (n * d as f64).round() as u64;
            let whole = total / d;
            (whole, total - whole * d, d)
        }
        None => {
            let places = denominator_width.clamp(1, 9) as u32;
            let max_den = 10u64.pow(places) - 1;
            let whole = n.floor() as u64;
            let (p, q) = best_fraction(n - n.floor(), max_den);
            (whole, p, q)
        }
    };
    let has_integer_places = fraction.integer.chars().any(|c| PLACEHOLDERS.contains(&c));
    if has_integer_places {
        // Carry a full fraction into the integer part, as `# ?/?` shows 2 for 1.999.
        whole += numerator / denominator;
        numerator %= denominator;
    } else {
        // With no integer placeholder the whole part is the numerator: `?/?` shows 3/2.
        numerator += whole * denominator;
        whole = 0;
    }

    let mut out = String::new();
    for token in tokenize(&fraction.prefix) {
        push_literal(&mut out, &token);
    }
    let digits = if whole > 0 || (numerator == 0 && has_integer_places) {
        whole.to_string()
    } else {
        String::new()
    };
    out.push_str(&layout_integer(
        &tokenize(&fraction.integer),
        &digits,
        false,
    ));
    if numerator == 0 {
        // A zero fraction part is blank, slash included.
        out.push_str(&" ".repeat(numerator_width + 1 + denominator_width));
    } else {
        out.push_str(&format!("{numerator:>numerator_width$}/"));
        match fixed {
            Some(_) => out.push_str(&denominator.to_string()),
            None => out.push_str(&format!("{denominator:>denominator_width$}")),
        }
    }
    for token in tokenize(&fraction.suffix) {
        push_literal(&mut out, &token);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn show(value: f64, code: &str) -> String {
        let fraction = parse(code).expect("a fraction format");
        render(&fraction, value).unwrap()
    }

    #[test]
    fn closest_fraction_within_the_denominator_digits() {
        assert_eq!(best_fraction(0.4, 9), (2, 5));
        assert_eq!(best_fraction(1.0 / 3.0, 9), (1, 3));
        assert_eq!(best_fraction(0.142_857, 9), (1, 7));
        assert_eq!(best_fraction(0.0, 9), (0, 1));
    }

    #[test]
    fn mixed_and_improper_fractions() {
        assert_eq!(show(1.5, "# ?/?"), "1 1/2");
        assert_eq!(show(0.5, "# ?/?"), " 1/2");
        assert_eq!(show(1.5, "?/?"), "3/2");
        assert_eq!(show(0.25, "?/?"), "1/4");
    }

    #[test]
    fn fixed_denominators_and_blank_zero_fraction() {
        assert_eq!(show(0.375, "?/8"), "3/8");
        assert_eq!(show(0.3, "?/8"), "2/8");
        assert_eq!(show(2.0, "# ?/?"), "2    ");
    }

    #[test]
    fn rounding_up_to_a_whole_number_carries() {
        assert_eq!(show(0.999, "# ?/?"), "1    ");
    }

    #[test]
    fn non_fraction_slashes_are_not_fractions() {
        assert!(parse("mm/dd").is_none());
        assert!(parse("\"a/b\"0").is_none());
    }
}
