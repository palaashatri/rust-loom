//! Cell style and formatting model for Loom Sheets.

use crate::{format_cell_display, format_number_currency, format_number_percentage, NumberFormat};

/// Text alignment within a spreadsheet cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CellAlignment {
    #[default]
    General,
    Left,
    Center,
    Right,
}

/// Cell fill swatch. Fixed tints stay legible on the paper surface in every
/// theme; the grid maps variants to paint, JSON persists their names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FillColor {
    /// No fill (paper background).
    #[default]
    None,
    Red,
    Orange,
    Yellow,
    Green,
    Blue,
    Purple,
    Gray,
}

impl FillColor {
    /// Canonical lowercase name used in sheet JSON and the palette.
    pub fn as_str(self) -> &'static str {
        match self {
            FillColor::None => "none",
            FillColor::Red => "red",
            FillColor::Orange => "orange",
            FillColor::Yellow => "yellow",
            FillColor::Green => "green",
            FillColor::Blue => "blue",
            FillColor::Purple => "purple",
            FillColor::Gray => "gray",
        }
    }

    /// Parse a fill name; unknown values map to `None`.
    pub fn parse_kind(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "red" => FillColor::Red,
            "orange" => FillColor::Orange,
            "yellow" => FillColor::Yellow,
            "green" => FillColor::Green,
            "blue" => FillColor::Blue,
            "purple" => FillColor::Purple,
            "gray" | "grey" => FillColor::Gray,
            _ => FillColor::None,
        }
    }

    /// Paint tint for the grid surface.
    pub fn hex(self) -> &'static str {
        match self {
            FillColor::None => "transparent",
            FillColor::Red => "#FECACA",
            FillColor::Orange => "#FED7AA",
            FillColor::Yellow => "#FEF08A",
            FillColor::Green => "#BBF7D0",
            FillColor::Blue => "#BFDBFE",
            FillColor::Purple => "#DDD6FE",
            FillColor::Gray => "#E5E7EB",
        }
    }

    /// Index for the Slint swatch row (`-1` means no fill).
    pub fn swatch_index(self) -> i32 {
        match self {
            FillColor::None => -1,
            FillColor::Red => 0,
            FillColor::Orange => 1,
            FillColor::Yellow => 2,
            FillColor::Green => 3,
            FillColor::Blue => 4,
            FillColor::Purple => 5,
            FillColor::Gray => 6,
        }
    }

    /// Next swatch for the cycle control (`None` wraps around).
    pub fn cycle(self) -> Self {
        match self {
            FillColor::None => FillColor::Red,
            FillColor::Red => FillColor::Orange,
            FillColor::Orange => FillColor::Yellow,
            FillColor::Yellow => FillColor::Green,
            FillColor::Green => FillColor::Blue,
            FillColor::Blue => FillColor::Purple,
            FillColor::Purple => FillColor::Gray,
            FillColor::Gray => FillColor::None,
        }
    }
}

/// Visual and numerical formatting style applied to a worksheet cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CellStyle {
    /// Bold text weight.
    pub bold: bool,
    /// Italic text slant.
    pub italic: bool,
    /// Underline text decoration.
    pub underline: bool,
    /// Display number format.
    pub number_format: NumberFormat,
    /// Custom decimal places (if applicable).
    pub decimal_places: Option<u8>,
    /// All-edges cell border.
    pub border: bool,
    /// Cell fill swatch (`None` is paper).
    pub fill: FillColor,
    /// Explicit font size in px (`None` follows the theme body size).
    pub font_size: Option<u8>,
}

impl CellStyle {
    /// Creates a default unstyled cell format.
    pub const fn new() -> Self {
        Self {
            bold: false,
            italic: false,
            underline: false,
            number_format: NumberFormat::General,
            decimal_places: None,
            border: false,
            fill: FillColor::None,
            font_size: None,
        }
    }

    /// Whether this style represents a default, unformatted cell.
    pub fn is_default(&self) -> bool {
        !self.bold
            && !self.italic
            && !self.underline
            && self.number_format == NumberFormat::General
            && self.decimal_places.is_none()
            && !self.border
            && self.fill == FillColor::None
            && self.font_size.is_none()
    }

    /// Formats a raw or evaluated string value for display in the grid.
    pub fn format_value(&self, raw: &str) -> String {
        if raw.is_empty() {
            return String::new();
        }

        // If decimal places are explicitly set, format numerical values accordingly.
        if let Ok(num) = raw.trim().parse::<f64>() {
            let decimals = self.decimal_places.unwrap_or(match self.number_format {
                NumberFormat::Currency => 2,
                NumberFormat::Percentage => 1,
                _ => 2,
            }) as usize;

            match self.number_format {
                NumberFormat::General | NumberFormat::PlainText => {
                    if let Some(dec) = self.decimal_places {
                        format!("{:.1$}", num, dec as usize)
                    } else {
                        raw.to_string()
                    }
                }
                NumberFormat::Number => {
                    format!("{:.1$}", num, decimals)
                }
                NumberFormat::Currency => format_number_currency(num, "$", decimals),
                NumberFormat::Percentage => format_number_percentage(num, decimals),
                NumberFormat::Scientific => format!("{:e}", num),
                NumberFormat::DateIso => raw.to_string(),
            }
        } else {
            format_cell_display(raw, self.number_format)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_style_is_default() {
        let style = CellStyle::default();
        assert!(style.is_default());
        assert!(!style.bold);
        assert!(!style.italic);
        assert!(!style.underline);
    }

    #[test]
    fn test_format_currency_with_decimals() {
        let style = CellStyle {
            number_format: NumberFormat::Currency,
            ..Default::default()
        };
        assert_eq!(style.format_value("1234.5"), "$1,234.50");

        let style = CellStyle {
            number_format: NumberFormat::Currency,
            decimal_places: Some(0),
            ..Default::default()
        };
        assert_eq!(style.format_value("1234.5"), "$1,234");
    }

    #[test]
    fn test_fill_color_cycle_parse_and_indices() {
        assert_eq!(FillColor::None.cycle(), FillColor::Red);
        assert_eq!(FillColor::Gray.cycle(), FillColor::None);
        assert_eq!(FillColor::parse_kind("BLUE"), FillColor::Blue);
        assert_eq!(FillColor::parse_kind("grey"), FillColor::Gray);
        assert_eq!(FillColor::parse_kind("bogus"), FillColor::None);
        assert_eq!(FillColor::Blue.swatch_index(), 4);
        assert_eq!(FillColor::None.swatch_index(), -1);
        assert_eq!(FillColor::Red.hex(), "#FECACA");

        let mut style = CellStyle::default();
        assert!(style.is_default());
        style.border = true;
        assert!(!style.is_default());
        style.border = false;
        style.fill = FillColor::Yellow;
        assert!(!style.is_default());
        style.fill = FillColor::None;
        style.font_size = Some(18);
        assert!(!style.is_default());
    }

    #[test]
    fn test_format_percentage_with_decimals() {
        let style = CellStyle {
            number_format: NumberFormat::Percentage,
            ..Default::default()
        };
        assert_eq!(style.format_value("0.125"), "12.5%");

        let style = CellStyle {
            number_format: NumberFormat::Percentage,
            decimal_places: Some(2),
            ..Default::default()
        };
        assert_eq!(style.format_value("0.125"), "12.50%");
    }
}
