//! Map Excel number-format codes onto the formats Loom can display.
//!
//! Loom has seven display formats. A code that maps onto one of them without
//! changing what the user reads is `exact`; a code that needs something Loom
//! cannot show (a time of day, a fraction, a currency symbol other than `$`)
//! is mapped to the closest format and flagged `lossy` so the import warning
//! can say so instead of silently showing different text.

use crate::NumberFormat;

/// The result of reading one format code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ParsedNumberFormat {
    pub(super) format: NumberFormat,
    pub(super) decimals: Option<u8>,
    /// Loom shows something different from Excel for this code.
    pub(super) lossy: bool,
}

impl ParsedNumberFormat {
    const fn exact(format: NumberFormat, decimals: Option<u8>) -> Self {
        Self {
            format,
            decimals,
            lossy: false,
        }
    }

    const fn lossy(format: NumberFormat, decimals: Option<u8>) -> Self {
        Self {
            format,
            decimals,
            lossy: true,
        }
    }
}

/// Excel's built-in format codes (ECMA-376 18.8.30) that have a code.
pub(super) fn builtin_format_code(id: u32) -> Option<&'static str> {
    Some(match id {
        0 => "General",
        1 => "0",
        2 => "0.00",
        3 => "#,##0",
        4 => "#,##0.00",
        5 => "$#,##0_);($#,##0)",
        6 => "$#,##0_);[Red]($#,##0)",
        7 => "$#,##0.00_);($#,##0.00)",
        8 => "$#,##0.00_);[Red]($#,##0.00)",
        9 => "0%",
        10 => "0.00%",
        11 => "0.00E+00",
        12 => "# ?/?",
        13 => "# ??/??",
        14 => "m/d/yy",
        15 => "d-mmm-yy",
        16 => "d-mmm",
        17 => "mmm-yy",
        18 => "h:mm AM/PM",
        19 => "h:mm:ss AM/PM",
        20 => "h:mm",
        21 => "h:mm:ss",
        22 => "m/d/yy h:mm",
        37 => "#,##0_);(#,##0)",
        38 => "#,##0_);[Red](#,##0)",
        39 => "#,##0.00_);(#,##0.00)",
        40 => "#,##0.00_);[Red](#,##0.00)",
        41 => "_(* #,##0_);_(* (#,##0);_(* \"-\"_);_(@_)",
        42 => "_($* #,##0_);_($* (#,##0);_($* \"-\"_);_(@_)",
        43 => "_(* #,##0.00_);_(* (#,##0.00);_(* \"-\"??_);_(@_)",
        44 => "_($* #,##0.00_);_($* (#,##0.00);_($* \"-\"??_);_(@_)",
        45 => "mm:ss",
        46 => "[h]:mm:ss",
        47 => "mm:ss.0",
        48 => "##0.0E+0",
        49 => "@",
        _ => return None,
    })
}

/// What the positive section of a code is made of once quoted text, escapes
/// and bracketed items have been taken out.
struct Pattern {
    /// Characters that form the number/date pattern, lower-case.
    body: String,
    /// Symbols from `[$symbol-locale]` items, in order.
    currency_symbols: Vec<String>,
}

fn is_currency_symbol(text: &str) -> bool {
    matches!(text, "$" | "€" | "£" | "¥")
}

fn pattern(code: &str) -> Pattern {
    let mut body = String::new();
    let mut currency_symbols = Vec::new();
    let characters: Vec<char> = code.chars().collect();
    let mut position = 0;
    while position < characters.len() {
        match characters[position] {
            ';' => break,
            '"' => {
                // Literal text between quotes carries no format meaning, except
                // a currency symbol written as a literal.
                position += 1;
                let start = position;
                while position < characters.len() && characters[position] != '"' {
                    position += 1;
                }
                let literal: String = characters[start..position].iter().collect();
                body.push_str(if is_currency_symbol(&literal) {
                    &literal
                } else {
                    "_"
                });
            }
            '\\' => {
                position += 1;
                if let Some(escaped) = characters
                    .get(position)
                    .filter(|c| is_currency_symbol(&c.to_string()))
                {
                    body.push(*escaped);
                }
            }
            '_' | '*' => position += 1,
            '[' => {
                let end = characters[position..]
                    .iter()
                    .position(|c| *c == ']')
                    .map_or(characters.len(), |offset| position + offset);
                let inside: String = characters[position + 1..end.min(characters.len())]
                    .iter()
                    .collect();
                if let Some(currency) = inside.strip_prefix('$') {
                    let symbol = currency.split('-').next().unwrap_or("").to_string();
                    currency_symbols.push(symbol);
                } else if inside.eq_ignore_ascii_case("h")
                    || inside.eq_ignore_ascii_case("hh")
                    || inside.eq_ignore_ascii_case("m")
                    || inside.eq_ignore_ascii_case("mm")
                    || inside.eq_ignore_ascii_case("s")
                    || inside.eq_ignore_ascii_case("ss")
                {
                    // Elapsed-time token such as [h]; counts as a time field.
                    body.push_str(&inside.to_ascii_lowercase());
                }
                position = end;
            }
            character => body.push(character.to_ascii_lowercase()),
        }
        position += 1;
    }
    Pattern {
        body,
        currency_symbols,
    }
}

fn fractional_digits(body: &str) -> Option<u8> {
    let (_, fraction) = body.split_once('.')?;
    let count = fraction
        .chars()
        .take_while(|c| matches!(c, '0' | '#' | '?'))
        .count();
    Some(count.min(usize::from(u8::MAX)) as u8)
}

/// Read one Excel format code.
pub(super) fn parse_number_format(code: Option<&str>) -> ParsedNumberFormat {
    let Some(code) = code else {
        return ParsedNumberFormat::exact(NumberFormat::General, None);
    };
    let pattern = pattern(code);
    let body = pattern.body.as_str();
    if body == "@" {
        return ParsedNumberFormat::exact(NumberFormat::PlainText, None);
    }
    if body == "general" || body.is_empty() {
        return ParsedNumberFormat::exact(NumberFormat::General, None);
    }
    let has_time = body.contains('h') || body.contains('s') || body.contains("am/pm");
    let has_date = body.contains('y') || body.contains('d') || (body.contains('m') && !has_time);
    if has_date || has_time {
        return match (has_date, has_time) {
            (true, false) => ParsedNumberFormat::exact(NumberFormat::DateIso, None),
            // A time of day or a date with a time cannot be shown.
            (true, true) => ParsedNumberFormat::lossy(NumberFormat::DateIso, None),
            _ => ParsedNumberFormat::lossy(NumberFormat::General, None),
        };
    }
    let has_fraction_slash = body.contains('/');
    let decimals = Some(fractional_digits(body).unwrap_or(0));
    if body.contains('%') {
        return ParsedNumberFormat::exact(NumberFormat::Percentage, decimals);
    }
    if body.contains("e+") || body.contains("e-") {
        return ParsedNumberFormat::exact(NumberFormat::Scientific, decimals);
    }
    if has_fraction_slash {
        return ParsedNumberFormat::lossy(NumberFormat::Number, decimals);
    }
    let foreign_symbol = pattern
        .currency_symbols
        .iter()
        .any(|symbol| !symbol.is_empty() && symbol != "$")
        || body.contains(['€', '£', '¥']);
    if foreign_symbol {
        return ParsedNumberFormat::lossy(NumberFormat::Number, decimals);
    }
    if body.contains('$') || !pattern.currency_symbols.is_empty() {
        return ParsedNumberFormat::exact(NumberFormat::Currency, decimals);
    }
    ParsedNumberFormat::exact(NumberFormat::Number, decimals)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(code: &str) -> (NumberFormat, Option<u8>, bool) {
        let result = parse_number_format(Some(code));
        (result.format, result.decimals, result.lossy)
    }

    #[test]
    fn plain_numbers_keep_their_decimal_places() {
        assert_eq!(parsed("0"), (NumberFormat::Number, Some(0), false));
        assert_eq!(parsed("#,##0"), (NumberFormat::Number, Some(0), false));
        assert_eq!(parsed("0.000"), (NumberFormat::Number, Some(3), false));
        assert_eq!(parsed("0%"), (NumberFormat::Percentage, Some(0), false));
        assert_eq!(parsed("0.0%"), (NumberFormat::Percentage, Some(1), false));
        assert_eq!(
            parsed("0.00E+00"),
            (NumberFormat::Scientific, Some(2), false)
        );
    }

    #[test]
    fn currency_is_exact_only_for_the_dollar_sign() {
        assert_eq!(
            parsed("$#,##0.00"),
            (NumberFormat::Currency, Some(2), false)
        );
        assert_eq!(
            parsed("[$$-409]#,##0.00"),
            (NumberFormat::Currency, Some(2), false)
        );
        assert_eq!(
            parsed("$#,##0.00_);[Red]($#,##0.00)"),
            (NumberFormat::Currency, Some(2), false)
        );
        // The letters of "EUR" must not read as scientific notation.
        assert_eq!(
            parsed("[$EUR] #,##0.00"),
            (NumberFormat::Number, Some(2), true)
        );
        assert_eq!(
            parsed("#,##0.00 [$€-407]"),
            (NumberFormat::Number, Some(2), true)
        );
    }

    #[test]
    fn dates_are_exact_but_times_and_fractions_are_flagged() {
        assert_eq!(parsed("yyyy-mm-dd"), (NumberFormat::DateIso, None, false));
        assert_eq!(parsed("m/d/yy"), (NumberFormat::DateIso, None, false));
        assert_eq!(parsed("dd-mmm-yy"), (NumberFormat::DateIso, None, false));
        assert_eq!(parsed("mmm-yy"), (NumberFormat::DateIso, None, false));
        assert_eq!(parsed("m/d/yy h:mm"), (NumberFormat::DateIso, None, true));
        assert_eq!(parsed("h:mm:ss"), (NumberFormat::General, None, true));
        assert_eq!(parsed("[h]:mm"), (NumberFormat::General, None, true));
        assert_eq!(parsed("# ?/?"), (NumberFormat::Number, Some(0), true));
    }

    #[test]
    fn text_and_general_and_quoted_literals() {
        assert_eq!(
            parsed("\\$#,##0.00"),
            (NumberFormat::Currency, Some(2), false)
        );
        assert_eq!(
            parsed("\"$\"#,##0"),
            (NumberFormat::Currency, Some(0), false)
        );
        assert_eq!(parsed("@"), (NumberFormat::PlainText, None, false));
        assert_eq!(parsed("General"), (NumberFormat::General, None, false));
        // Text in quotes is not a date or scientific token.
        assert_eq!(
            parsed("\"Days: \"0"),
            (NumberFormat::Number, Some(0), false)
        );
        assert_eq!(builtin_format_code(14), Some("m/d/yy"));
        assert_eq!(builtin_format_code(164), None);
    }
}
