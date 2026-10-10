//! Excel number/date format codes for the `TEXT` function.
//!
//! Supported: up to four `;` sections (positive; negative; zero; text) and
//! conditional sections (`[>=1000]`, `[<0]`), `0 # ?` digit placeholders, `.`,
//! thousands `,` and trailing-comma scaling, `%`, `E+00` and `E-00` scientific
//! (with engineering exponents such as `##0.0E+0`), fraction formats (`# ?/?`,
//! `?/8`), quoted and escaped literals, `[colour]` and `[$sym-locale]`
//! brackets, `@` text, `General`, and date/time codes (`yyyy yy mmmm mmm mm m
//! dddd ddd dd d hh h mm ss AM/PM [h] [m] [s]`). Repeat (`*x`) and fill (`_x`)
//! codes add no width here: `*x` is dropped and `_x` is one space.

mod conditions;
mod fractions;

use crate::dates::{serial_to_ymd, MAX_SERIAL};
use crate::CalcError;

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const DAYS: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];

/// Format a number with an Excel format code.
pub(crate) fn format_number(n: f64, code: &str) -> Result<String, CalcError> {
    if !n.is_finite() {
        return Err(CalcError::Num);
    }
    let sections = split_sections(code);
    if sections.len() > 4 {
        return Err(CalcError::Value);
    }
    let split: Vec<(Option<conditions::Condition>, &str)> = sections
        .iter()
        .map(|section| conditions::split_leading(section))
        .collect();
    if split.iter().any(|(condition, _)| condition.is_some()) {
        // A conditional section shows its own sign: the condition, not the
        // section position, decides which section applies.
        let rules: Vec<_> = split.iter().map(|(condition, _)| *condition).collect();
        let Some(index) = conditions::choose(&rules, n) else {
            return Ok(String::new());
        };
        let body = format_section(split[index].1, n.abs(), n < 0.0)?;
        return Ok(if n < 0.0 && !body.is_empty() {
            format!("-{body}")
        } else {
            body
        });
    }
    let (section, add_minus) = match (sections.len(), n) {
        (1, _) => (&sections[0], n < 0.0),
        (_, n) if n > 0.0 => (&sections[0], false),
        (_, n) if n < 0.0 => (&sections[1], false),
        (len, _) if len >= 3 => (&sections[2], false),
        _ => (&sections[0], false),
    };
    let body = format_section(section, n.abs(), n < 0.0)?;
    Ok(if add_minus && !body.is_empty() {
        format!("-{body}")
    } else {
        body
    })
}

/// Format text with a format code: its text section (or `@` placeholder).
pub(crate) fn format_text(text: &str, code: &str) -> String {
    let sections = split_sections(code);
    let section = if sections.len() >= 4 {
        &sections[3]
    } else {
        &sections[0]
    };
    if !section.contains('@') {
        return text.to_string();
    }
    let mut out = String::new();
    for token in tokenize(section) {
        match token {
            Tok::Text => out.push_str(text),
            Tok::Lit(lit) => out.push_str(&lit),
            _ => {}
        }
    }
    out
}

/// Split on `;` outside quotes and brackets.
fn split_sections(code: &str) -> Vec<String> {
    let mut sections = vec![String::new()];
    let (mut quoted, mut bracket, mut escaped) = (false, false, false);
    for c in code.chars() {
        let current = sections.last_mut().expect("at least one section");
        if escaped {
            escaped = false;
        } else if c == '\\' && !quoted {
            escaped = true;
        } else if c == '"' {
            quoted = !quoted;
        } else if !quoted && c == '[' {
            bracket = true;
        } else if !quoted && c == ']' {
            bracket = false;
        } else if c == ';' && !quoted && !bracket {
            sections.push(String::new());
            continue;
        }
        current.push(c);
    }
    sections
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Digit(char),
    Point,
    Comma,
    Percent,
    /// `E+0` or `E-0`: the digit width, and whether a positive exponent gets a sign.
    Exp(usize, bool),
    Text,
    Lit(String),
    /// Date/time codes kept as the raw run of one letter, e.g. `mmm`.
    Run(char, usize),
    /// `[h]`, `[m]`, `[s]` elapsed-time codes.
    Elapsed(char, usize),
    /// `AM/PM` or `A/P`.
    Meridiem(bool),
}

fn tokenize(section: &str) -> Vec<Tok> {
    let chars: Vec<char> = section.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '"' => {
                let end = chars[i + 1..].iter().position(|x| *x == '"');
                let end = end.map_or(chars.len(), |p| i + 1 + p);
                tokens.push(Tok::Lit(chars[i + 1..end].iter().collect()));
                i = end + 1;
                continue;
            }
            '\\' => {
                if let Some(next) = chars.get(i + 1) {
                    tokens.push(Tok::Lit(next.to_string()));
                }
                i += 2;
                continue;
            }
            '_' => {
                tokens.push(Tok::Lit(" ".into()));
                i += 2;
                continue;
            }
            '*' => {
                i += 2;
                continue;
            }
            '[' => {
                let end = chars[i..]
                    .iter()
                    .position(|x| *x == ']')
                    .map_or(chars.len(), |p| i + p);
                let inner: String = chars[i + 1..end.min(chars.len())].iter().collect();
                let lower = inner.to_ascii_lowercase();
                let letter = lower.chars().next().unwrap_or(' ');
                if !lower.is_empty() && lower.chars().all(|x| x == letter) && "hms".contains(letter)
                {
                    tokens.push(Tok::Elapsed(letter, lower.len()));
                } else if let Some(currency) = inner.strip_prefix('$') {
                    tokens.push(Tok::Lit(
                        currency.split('-').next().unwrap_or("").to_string(),
                    ));
                }
                i = end + 1;
                continue;
            }
            '0' | '#' | '?' => tokens.push(Tok::Digit(c)),
            '.' => tokens.push(Tok::Point),
            ',' => tokens.push(Tok::Comma),
            '%' => tokens.push(Tok::Percent),
            '@' => tokens.push(Tok::Text),
            'E' | 'e' if matches!(chars.get(i + 1), Some('+') | Some('-')) => {
                let always_sign = chars[i + 1] == '+';
                let zeros = chars[i + 2..].iter().take_while(|x| **x == '0').count();
                tokens.push(Tok::Exp(zeros.max(1), always_sign));
                i += 2 + zeros;
                continue;
            }
            'A' | 'a'
                if section[char_offset(&chars, i)..]
                    .to_ascii_uppercase()
                    .starts_with("AM/PM") =>
            {
                tokens.push(Tok::Meridiem(true));
                i += 5;
                continue;
            }
            'A' | 'a'
                if section[char_offset(&chars, i)..]
                    .to_ascii_uppercase()
                    .starts_with("A/P") =>
            {
                tokens.push(Tok::Meridiem(false));
                i += 3;
                continue;
            }
            'y' | 'Y' | 'm' | 'M' | 'd' | 'D' | 'h' | 'H' | 's' | 'S' => {
                let letter = c.to_ascii_lowercase();
                let run = chars[i..]
                    .iter()
                    .take_while(|x| x.to_ascii_lowercase() == letter)
                    .count();
                tokens.push(Tok::Run(letter, run));
                i += run;
                continue;
            }
            other => tokens.push(Tok::Lit(other.to_string())),
        }
        i += 1;
    }
    tokens
}

fn char_offset(chars: &[char], index: usize) -> usize {
    chars[..index].iter().map(|c| c.len_utf8()).sum()
}

fn format_section(section: &str, n: f64, negative: bool) -> Result<String, CalcError> {
    if section.is_empty() {
        return Ok(String::new());
    }
    if section.trim().eq_ignore_ascii_case("general") {
        return Ok(crate::Value::Number(n).display());
    }
    let tokens = tokenize(section);
    let is_date = tokens
        .iter()
        .any(|t| matches!(t, Tok::Run(..) | Tok::Elapsed(..) | Tok::Meridiem(_)));
    if is_date {
        // A negative number is not a date or time.
        return if negative {
            Err(CalcError::Value)
        } else {
            format_date(&tokens, n)
        };
    }
    if let Some(fraction) = fractions::parse(section) {
        return fractions::render(&fraction, n);
    }
    format_digits(&tokens, n)
}

/// Round half away from zero at `decimals` places, ignoring binary noise.
fn round_decimals(value: f64, decimals: usize) -> f64 {
    let scale = 10f64.powi(decimals as i32);
    let scaled = value * scale;
    let cleaned: f64 = format!("{scaled:.14e}").parse().unwrap_or(scaled);
    cleaned.round() / scale
}

fn format_digits(tokens: &[Tok], n: f64) -> Result<String, CalcError> {
    let first_digit = tokens
        .iter()
        .position(|t| matches!(t, Tok::Digit(_) | Tok::Point));
    let Some(first) = first_digit else {
        // No digit placeholder: only literals (e.g. "Total"); `@` is the number as text.
        return Ok(tokens
            .iter()
            .map(|t| match t {
                Tok::Lit(l) => l.clone(),
                Tok::Text => crate::Value::Number(n).display(),
                _ => String::new(),
            })
            .collect());
    };
    let last = tokens
        .iter()
        .rposition(|t| matches!(t, Tok::Digit(_) | Tok::Point | Tok::Comma | Tok::Exp(..)))
        .unwrap_or(first);
    let (prefix, run, suffix) = (&tokens[..first], &tokens[first..=last], &tokens[last + 1..]);

    let percents = tokens.iter().filter(|t| **t == Tok::Percent).count();
    let point = run.iter().position(|t| *t == Tok::Point);
    let exponent = run.iter().find_map(|t| match t {
        Tok::Exp(width, always_sign) => Some((*width, *always_sign)),
        _ => None,
    });
    let digit_end = run
        .iter()
        .rposition(|t| matches!(t, Tok::Digit(_)))
        .map_or(0, |p| p + 1);
    // Commas right after the last digit placeholder divide by 1000 each.
    let scale_commas = run[digit_end..]
        .iter()
        .filter(|t| **t == Tok::Comma)
        .count();
    let grouping = run[..digit_end].contains(&Tok::Comma);
    let digits_of = |part: &[Tok]| -> Vec<char> {
        part.iter()
            .filter_map(|t| {
                if let Tok::Digit(c) = t {
                    Some(*c)
                } else {
                    None
                }
            })
            .collect()
    };
    let (int_tokens, frac_tokens) = match point {
        Some(p) => (&run[..p], &run[p + 1..]),
        None => (&run[..digit_end], &run[digit_end..digit_end]),
    };
    let int_places = digits_of(int_tokens);
    let frac_places = digits_of(frac_tokens);
    let mut value = n * 100f64.powi(percents as i32) / 1000f64.powi(scale_commas as i32);

    let mut exp_text = String::new();
    if let Some((width, always_sign)) = exponent {
        // With several integer placeholders the exponent is a multiple of their
        // count (`##0.0E+0` shows 12345 as 12.3E+3), as in engineering notation.
        let places = int_places.len().max(1) as i32;
        let floor_log = if value == 0.0 {
            0
        } else {
            value.log10().floor() as i32
        };
        let mut power = floor_log.div_euclid(places) * places;
        let mut mantissa = value / 10f64.powi(power);
        if round_decimals(mantissa, frac_places.len()) >= 10f64.powi(places) {
            power += places;
            mantissa = value / 10f64.powi(power);
        }
        value = mantissa;
        // `E-` shows a sign only when the exponent is negative.
        let sign = if power < 0 {
            "-"
        } else if always_sign {
            "+"
        } else {
            ""
        };
        exp_text = format!("E{sign}{:0width$}", power.abs(), width = width);
    }

    let rounded = round_decimals(value, frac_places.len());
    let fixed = format!("{rounded:.*}", frac_places.len());
    let (int_digits, frac_digits) = fixed.split_once('.').unwrap_or((fixed.as_str(), ""));
    let mut int_digits = int_digits.to_string();
    // A zero integer part shows nothing unless a `0` or `?` placeholder asks for it.
    if int_digits == "0"
        && (!frac_places.is_empty() || !int_places.is_empty())
        && int_places.iter().all(|place| *place == '#')
    {
        int_digits.clear();
    }

    let mut out = String::new();
    for token in prefix {
        push_literal(&mut out, token);
    }
    out.push_str(&layout_integer(int_tokens, &int_digits, grouping));
    if point.is_some() {
        out.push('.');
        let mut frac: Vec<char> = frac_digits.chars().collect();
        for (place, slot) in frac_places.iter().zip(frac.iter_mut()) {
            if *slot == '0' && *place != '0' {
                *slot = if *place == '?' { ' ' } else { '\0' };
            }
        }
        // Optional trailing zeros (#, ?) are dropped from the right only.
        while let Some(last) = frac.last() {
            if *last == '\0' || *last == ' ' {
                frac.pop();
            } else {
                break;
            }
        }
        out.extend(frac.iter().filter(|c| **c != '\0'));
    }
    out.push_str(&exp_text);
    for token in suffix {
        push_literal(&mut out, token);
    }
    Ok(out)
}

fn push_literal(out: &mut String, token: &Tok) {
    match token {
        Tok::Lit(lit) => out.push_str(lit),
        Tok::Percent => out.push('%'),
        _ => {}
    }
}

/// Lay integer digits into the placeholders from the right; extra leading
/// digits stay in front. Literals between placeholders keep their place.
fn layout_integer(tokens: &[Tok], digits: &str, grouping: bool) -> String {
    let places = tokens.iter().filter(|t| matches!(t, Tok::Digit(_))).count();
    let mut digits: Vec<char> = digits.chars().collect();
    if places == 0 {
        return digits.into_iter().collect();
    }
    if grouping {
        let min_zeros = tokens
            .iter()
            .filter(|t| matches!(t, Tok::Digit('0')))
            .count();
        while digits.len() < min_zeros {
            digits.insert(0, '0');
        }
        let mut grouped = String::new();
        for (index, digit) in digits.iter().enumerate() {
            if index > 0 && (digits.len() - index) % 3 == 0 {
                grouped.push(',');
            }
            grouped.push(*digit);
        }
        return grouped;
    }
    let mut out: Vec<String> = Vec::new();
    let mut remaining = places;
    for token in tokens.iter().rev() {
        match token {
            Tok::Digit(place) => {
                remaining -= 1;
                let digit = digits.pop();
                let text = match (digit, place) {
                    (Some(d), _) if remaining == 0 => {
                        let mut lead = digits.drain(..).collect::<String>();
                        lead.push(d);
                        lead
                    }
                    (Some(d), _) => d.to_string(),
                    (None, '0') => "0".into(),
                    (None, '?') => " ".into(),
                    (None, _) => String::new(),
                };
                out.push(text);
            }
            Tok::Lit(lit) => out.push(lit.clone()),
            _ => {}
        }
    }
    out.reverse();
    out.concat()
}

fn format_date(tokens: &[Tok], serial: f64) -> Result<String, CalcError> {
    if !(0.0..(MAX_SERIAL + 1) as f64).contains(&serial) {
        return Err(CalcError::Value);
    }
    let mut seconds = (serial.fract() * 86_400.0).round() as i64;
    let mut day = serial.floor() as i64;
    if seconds >= 86_400 {
        seconds -= 86_400;
        day += 1;
    }
    let (year, month, dom) = serial_to_ymd(day).ok_or(CalcError::Value)?;
    let weekday = (day - 1).rem_euclid(7) as usize;
    let twelve_hour = tokens.iter().any(|t| matches!(t, Tok::Meridiem(_)));
    let (hour, minute, second) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    let total_seconds = day * 86_400 + seconds;

    let mut out = String::new();
    for (index, token) in tokens.iter().enumerate() {
        match token {
            Tok::Run('y', n) => {
                if *n <= 2 {
                    out.push_str(&format!("{:02}", year % 100));
                } else {
                    out.push_str(&format!("{year:04}"));
                }
            }
            Tok::Run('m', n) if is_minute(tokens, index) => {
                out.push_str(&pad(minute, *n));
            }
            Tok::Run('m', n) => match n {
                1 => out.push_str(&month.to_string()),
                2 => out.push_str(&format!("{month:02}")),
                3 => out.push_str(&MONTHS[month as usize - 1][..3]),
                4 => out.push_str(MONTHS[month as usize - 1]),
                _ => out.push_str(&MONTHS[month as usize - 1][..1]),
            },
            Tok::Run('d', n) => match n {
                1 => out.push_str(&dom.to_string()),
                2 => out.push_str(&format!("{dom:02}")),
                3 => out.push_str(&DAYS[weekday][..3]),
                _ => out.push_str(DAYS[weekday]),
            },
            Tok::Run('h', n) => {
                let h = if twelve_hour {
                    match hour % 12 {
                        0 => 12,
                        other => other,
                    }
                } else {
                    hour
                };
                out.push_str(&pad(h, *n));
            }
            Tok::Run('s', n) => {
                out.push_str(&pad(second, *n));
                if matches!(tokens.get(index + 1), Some(Tok::Point))
                    && matches!(tokens.get(index + 2), Some(Tok::Digit('0')))
                {
                    let places = tokens[index + 2..]
                        .iter()
                        .take_while(|t| matches!(t, Tok::Digit('0')))
                        .count();
                    let fraction = (serial.fract() * 86_400.0).fract();
                    let digits = format!("{:.*}", places, fraction);
                    out.push_str(digits.trim_start_matches('0'));
                }
            }
            Tok::Run(_, _) => {}
            Tok::Elapsed(unit, n) => {
                let value = match unit {
                    'h' => total_seconds / 3600,
                    'm' => total_seconds / 60,
                    _ => total_seconds,
                };
                out.push_str(&pad(value, *n));
            }
            Tok::Meridiem(long) => {
                let pm = hour >= 12;
                out.push_str(match (*long, pm) {
                    (true, false) => "AM",
                    (true, true) => "PM",
                    (false, false) => "A",
                    (false, true) => "P",
                });
            }
            Tok::Lit(lit) => out.push_str(lit),
            // `.00` after seconds was already written with the seconds.
            Tok::Point | Tok::Digit('0') if follows_seconds(tokens, index) => {}
            Tok::Point => out.push('.'),
            Tok::Comma => out.push(','),
            Tok::Percent => out.push('%'),
            Tok::Digit(d) => out.push(*d),
            Tok::Exp(..) | Tok::Text => {}
        }
    }
    Ok(out)
}

/// Whether the token at `index` is the `.` or a `0` of fractional seconds.
fn follows_seconds(tokens: &[Tok], index: usize) -> bool {
    let mut at = index;
    while at > 0 && matches!(tokens[at], Tok::Digit('0')) {
        at -= 1;
    }
    at > 0 && tokens[at] == Tok::Point && matches!(tokens[at - 1], Tok::Run('s', _))
}

fn pad(value: i64, width: usize) -> String {
    if width >= 2 {
        format!("{value:02}")
    } else {
        value.to_string()
    }
}

/// `m` is minutes (not month) right after an hour code or right before seconds.
fn is_minute(tokens: &[Tok], index: usize) -> bool {
    let neighbour = |range: Box<dyn Iterator<Item = &Tok> + '_>| {
        range
            .filter(|t| !matches!(t, Tok::Lit(_) | Tok::Point | Tok::Comma))
            .find_map(|t| match t {
                Tok::Run(letter, _) => Some(*letter),
                Tok::Elapsed(letter, _) => Some(*letter),
                _ => None,
            })
    };
    let before = neighbour(Box::new(tokens[..index].iter().rev()));
    let after = neighbour(Box::new(tokens[index + 1..].iter()));
    before == Some('h') || after == Some('s')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(value: f64, code: &str) -> String {
        format_number(value, code).unwrap()
    }

    #[test]
    fn plain_digit_placeholders() {
        assert_eq!(n(3.14259, "0.00"), "3.14");
        assert_eq!(n(2.675, "0.00"), "2.68");
        assert_eq!(n(7.0, "000"), "007");
        assert_eq!(n(7.5, "0"), "8");
        assert_eq!(n(0.5, ".00"), ".50");
        // Excel keeps the point when every decimal placeholder is optional.
        assert_eq!(n(3.0, "0.##"), "3.");
        assert_eq!(n(3.5, "0.##"), "3.5");
        assert_eq!(n(12.0, "General"), "12");
    }

    #[test]
    fn thousands_percent_and_scaling() {
        assert_eq!(n(1234567.891, "#,##0.00"), "1,234,567.89");
        assert_eq!(n(-0.5, "#,##0.00"), "-0.50");
        assert_eq!(n(1234.0, "#,##0"), "1,234");
        assert_eq!(n(0.256, "0%"), "26%");
        assert_eq!(n(0.2564, "0.0%"), "25.6%");
        assert_eq!(n(1234567.0, "0.0,,\"M\""), "1.2M");
        assert_eq!(n(5000.0, "#,"), "5");
    }

    #[test]
    fn literals_currency_and_sections() {
        assert_eq!(n(1234.5, "$#,##0.00"), "$1,234.50");
        assert_eq!(n(1234.5, "\"USD \"0.0"), "USD 1234.5");
        assert_eq!(n(5.0, "0\" units\""), "5 units");
        assert_eq!(n(-5.0, "0;(0)"), "(5)");
        assert_eq!(n(0.0, "0;-0;\"zero\""), "zero");
        assert_eq!(n(1234.5, "[$€-407]#,##0.00"), "€1,234.50");
        assert_eq!(n(5551234567.0, "(000) 000-0000"), "(555) 123-4567");
        assert_eq!(n(12.0, "[Red]0"), "12");
    }

    #[test]
    fn scientific_notation() {
        assert_eq!(n(12345.0, "0.00E+00"), "1.23E+04");
        assert_eq!(n(0.000123, "0.0E+00"), "1.2E-04");
        assert_eq!(n(0.0, "0.00E+00"), "0.00E+00");
    }

    #[test]
    fn conditional_sections_choose_by_the_value() {
        let scaled = "[>=1000000]0.0,,\"M\";[>=1000]0.0,\"K\";0";
        assert_eq!(n(1_500_000.0, scaled), "1.5M");
        assert_eq!(n(2500.0, scaled), "2.5K");
        assert_eq!(n(7.0, scaled), "7");
        // A condition decides the sign: the matched section shows the minus.
        assert_eq!(n(-5.0, "[<0]0;0"), "-5");
        assert_eq!(n(4.0, "[<0]0;0"), "4");
    }

    #[test]
    fn exponent_signs_and_engineering_multiples() {
        // `E-` shows a minus only for a negative exponent; `E+` always shows the sign.
        assert_eq!(n(12345.0, "0.00E-00"), "1.23E04");
        assert_eq!(n(0.00123, "0.00E-00"), "1.23E-03");
        // Several integer placeholders make the exponent a multiple of their count.
        assert_eq!(n(12345.0, "##0.0E+0"), "12.3E+3");
        assert_eq!(n(0.000_123_45, "##0.0E+0"), "123.5E-6");
        assert_eq!(n(99_999.9, "##0.0E+0"), "100.0E+3");
    }

    #[test]
    fn dates_and_times() {
        assert_eq!(n(45292.0, "yyyy-mm-dd"), "2024-01-01");
        assert_eq!(n(45292.0, "dd/mm/yyyy"), "01/01/2024");
        assert_eq!(n(45292.0, "mmm d, yyyy"), "Jan 1, 2024");
        assert_eq!(n(45292.0, "dddd, mmmm d"), "Monday, January 1");
        assert_eq!(n(45292.75, "h:mm AM/PM"), "6:00 PM");
        assert_eq!(n(45292.5, "hh:mm:ss"), "12:00:00");
        assert_eq!(n(45292.0 + 90.0 / 1440.0, "[h]:mm"), "1087009:30");
        assert_eq!(n(0.5 + 0.25 / 86_400.0, "hh:mm:ss.00"), "12:00:00.25");
        assert_eq!(n(45292.0, "yy"), "24");
        assert_eq!(n(45323.0, "m/d/yyyy"), "2/1/2024");
        assert_eq!(n(0.5, "h"), "12");
    }

    #[test]
    fn text_sections_and_unsupported_codes() {
        assert_eq!(format_text("abc", "@"), "abc");
        assert_eq!(format_text("abc", "\"<\"@\">\""), "<abc>");
        assert_eq!(format_text("abc", "0.00"), "abc");
        assert_eq!(n(0.5, "# ?/?"), " 1/2");
        assert_eq!(format_number(-1.0, "yyyy"), Err(CalcError::Value));
    }
}
