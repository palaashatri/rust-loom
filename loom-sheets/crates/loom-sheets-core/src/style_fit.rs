//! Fitting a displayed number into its column: a number is never shown cut off.
//! General numbers drop decimals and then switch to scientific notation; fixed
//! formats that do not fit show `####`, as other spreadsheets do. Only the
//! display changes; stored and edited values keep full precision.

use crate::style::CellStyle;
use crate::NumberFormat;

/// Rough advance of one digit as a fraction of the font size (matches the
/// text-overflow estimate in the app).
const CHAR_EM: f32 = 0.6;
const BOLD_EXTRA: f32 = 0.06;
/// Cell padding on both sides.
const PADDING: f32 = 16.0;

/// How many characters of this size fit in a column `width_px` wide.
pub fn chars_that_fit(width_px: f32, font_size: f32, bold: bool) -> usize {
    let em = CHAR_EM + if bold { BOLD_EXTRA } else { 0.0 };
    let per_char = (font_size * em).max(1.0);
    (((width_px - PADDING) / per_char + 1e-3).floor().max(1.0)) as usize
}

impl CellStyle {
    /// [`CellStyle::format_value`] shortened so that a number fits a column of
    /// `width_px` at `font_size`. Text is never altered here.
    pub fn format_value_fit(&self, raw: &str, width_px: f32, font_size: f32) -> String {
        let shown = self.format_value(raw);
        let Ok(num) = raw.trim().parse::<f64>() else {
            return shown;
        };
        if !num.is_finite() {
            return shown;
        }
        let max = chars_that_fit(width_px, font_size, self.bold);
        if shown.chars().count() <= max {
            return shown;
        }
        let general = self.number_format == NumberFormat::General && self.decimal_places.is_none();
        if general {
            if let Some(text) = general_fit(num, max) {
                return text;
            }
        }
        "#".repeat(max.clamp(1, 4))
    }
}

fn general_fit(num: f64, max: usize) -> Option<String> {
    if num.abs() < 1e15 {
        for decimals in (0..=9).rev() {
            let text = format!("{num:.decimals$}");
            let text = if text.contains('.') {
                text.trim_end_matches('0').trim_end_matches('.').to_string()
            } else {
                text
            };
            let lost = num != 0.0 && text.parse::<f64>().ok() == Some(0.0);
            if !lost && text.chars().count() <= max {
                return Some(text);
            }
        }
    }
    (0..=9).rev().find_map(|digits| {
        let text = scientific(num, digits);
        (text.chars().count() <= max).then_some(text)
    })
}

fn scientific(num: f64, digits: usize) -> String {
    let text = format!("{num:.digits$e}");
    let (mantissa, exp) = text.split_once('e').unwrap_or((&text, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let mantissa = if mantissa.contains('.') {
        mantissa.trim_end_matches('0').trim_end_matches('.')
    } else {
        mantissa
    };
    format!(
        "{mantissa}E{}{:02}",
        if exp < 0 { '-' } else { '+' },
        exp.abs()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wide_integer_becomes_scientific_in_the_default_column() {
        let style = CellStyle::default();
        let shown = style.format_value_fit("12345678901234567890", 80.0, 14.0);
        assert_eq!(shown, "1.2E+19");
        assert!(shown.chars().count() <= chars_that_fit(80.0, 14.0, false));
    }

    #[test]
    fn a_general_decimal_rounds_to_fit() {
        let style = CellStyle::default();
        assert_eq!(style.format_value_fit("1234.5678", 70.0, 14.0), "1234.6");
        assert_eq!(style.format_value_fit("1234.5678", 60.0, 14.0), "1235");
        assert_eq!(
            style.format_value_fit("1234.5678", 200.0, 14.0),
            "1234.5678"
        );
    }

    #[test]
    fn a_fixed_format_that_does_not_fit_shows_hashes() {
        let style = CellStyle {
            number_format: NumberFormat::Number,
            ..CellStyle::default()
        };
        assert_eq!(style.format_value_fit("123456789012.5", 80.0, 14.0), "####");
        assert_eq!(style.format_value_fit("1.5", 80.0, 14.0), "1.50");
    }

    #[test]
    fn widening_the_column_shows_the_full_number() {
        let style = CellStyle::default();
        let raw = "12345678901234567890";
        assert_eq!(
            style.format_value_fit(raw, 400.0, 14.0),
            style.format_value(raw)
        );
        assert_eq!(style.format_value(raw), raw);
    }

    #[test]
    fn text_is_never_altered() {
        let style = CellStyle::default();
        assert_eq!(
            style.format_value_fit("a very long label here", 40.0, 14.0),
            "a very long label here"
        );
    }
}
