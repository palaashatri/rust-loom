//! Cell style and formatting model for Loom Sheets.

use crate::banded::StoredValue;
use crate::{format_cell_display, format_number_currency, format_number_percentage, NumberFormat};

/// Text alignment within a spreadsheet cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum CellAlignment {
    #[default]
    General,
    Left,
    Center,
    Right,
}

impl StoredValue for CellAlignment {
    /// The saved file records only non-default alignments.
    fn is_persisted(&self) -> bool {
        *self != CellAlignment::General
    }
}

/// Cell fill swatch. Fixed tints stay legible on the paper surface in every
/// theme; the grid maps variants to paint, JSON persists their names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
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

/// Per-edge cell border flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct CellBorders {
    pub top: bool,
    pub right: bool,
    pub bottom: bool,
    pub left: bool,
}

impl CellBorders {
    /// No borders on any edge.
    pub const NONE: Self = Self {
        top: false,
        right: false,
        bottom: false,
        left: false,
    };

    /// Thin borders on all four edges.
    pub const ALL: Self = Self {
        top: true,
        right: true,
        bottom: true,
        left: true,
    };

    /// Whether any edge has a border.
    pub const fn any(self) -> bool {
        self.top || self.right || self.bottom || self.left
    }

    /// Whether all four edges have a border.
    pub const fn all(self) -> bool {
        self.top && self.right && self.bottom && self.left
    }

    /// Encode edges into a 4-bit mask (top=1, right=2, bottom=4, left=8).
    pub const fn to_mask(self) -> u8 {
        (self.top as u8)
            | ((self.right as u8) << 1)
            | ((self.bottom as u8) << 2)
            | ((self.left as u8) << 3)
    }

    /// Decode edges from a 4-bit mask (top=1, right=2, bottom=4, left=8).
    pub const fn from_mask(mask: u8) -> Self {
        Self {
            top: (mask & 1) != 0,
            right: (mask & 2) != 0,
            bottom: (mask & 4) != 0,
            left: (mask & 8) != 0,
        }
    }
}

/// Border preset applied to a cell or range selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BorderPreset {
    All,
    Outer,
    Top,
    Bottom,
    Left,
    Right,
    None,
}

impl BorderPreset {
    /// Parse a border preset name (case-insensitive).
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "all" => Some(Self::All),
            "outer" => Some(Self::Outer),
            "top" => Some(Self::Top),
            "bottom" => Some(Self::Bottom),
            "left" => Some(Self::Left),
            "right" => Some(Self::Right),
            "none" | "clear" => Some(Self::None),
            _ => None,
        }
    }
}

/// Visual and numerical formatting style applied to a worksheet cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct CellStyle {
    /// Bold text weight.
    pub bold: bool,
    /// Italic text slant.
    pub italic: bool,
    /// Underline text decoration.
    pub underline: bool,
    /// Strikethrough text decoration.
    pub strikethrough: bool,
    /// Display number format.
    pub number_format: NumberFormat,
    /// Custom decimal places (if applicable).
    pub decimal_places: Option<u8>,
    /// Per-edge cell borders.
    pub borders: CellBorders,
    /// Cell fill swatch (`None` is paper).
    pub fill: FillColor,
    /// Explicit font size in px (`None` follows the theme body size).
    pub font_size: Option<u8>,
    /// Text colour as `0xRRGGBB` (`None` follows the paper's ink).
    pub text_color: Option<u32>,
    /// Wrap long text onto further lines inside the cell instead of cutting it
    /// off or spilling it into empty neighbours.
    pub wrap: bool,
}

impl StoredValue for CellStyle {
    /// The saved file records only styles that differ from the default.
    fn is_persisted(&self) -> bool {
        !self.is_default()
    }
}

impl CellStyle {
    /// Creates a default unstyled cell format.
    pub const fn new() -> Self {
        Self {
            bold: false,
            italic: false,
            underline: false,
            strikethrough: false,
            number_format: NumberFormat::General,
            decimal_places: None,
            borders: CellBorders::NONE,
            fill: FillColor::None,
            font_size: None,
            text_color: None,
            wrap: false,
        }
    }

    /// Whether this style represents a default, unformatted cell.
    pub fn is_default(&self) -> bool {
        !self.bold
            && !self.italic
            && !self.underline
            && !self.strikethrough
            && self.number_format == NumberFormat::General
            && self.decimal_places.is_none()
            && !self.borders.any()
            && self.fill == FillColor::None
            && self.font_size.is_none()
            && self.text_color.is_none()
            && !self.wrap
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
                        let decimals = dec as usize;
                        format!(
                            "{:.1$}",
                            crate::functions_format::round_decimals(num, decimals),
                            decimals
                        )
                    } else if self.number_format == NumberFormat::General {
                        general_display(num, raw)
                    } else {
                        raw.to_string()
                    }
                }
                NumberFormat::Number => format!(
                    "{:.1$}",
                    crate::functions_format::round_decimals(num, decimals),
                    decimals
                ),
                NumberFormat::Currency => format_number_currency(num, "$", decimals),
                NumberFormat::Percentage => format_number_percentage(num, decimals),
                NumberFormat::Scientific => format!("{:e}", num),
                NumberFormat::DateIso => {
                    crate::dates::serial_to_iso(num).unwrap_or_else(|| raw.to_string())
                }
            }
        } else {
            format_cell_display(raw, self.number_format)
        }
    }
}

/// General format shows at most 10 significant digits, like other spreadsheets,
/// so an Average of 1100/3 reads `366.6666667` instead of a 16-digit float. The
/// stored value is untouched; only the grid text is shortened.
fn general_display(num: f64, raw: &str) -> String {
    let raw = raw.trim();
    if !num.is_finite() || !raw.contains('.') || raw.len() <= 11 {
        return raw.to_string();
    }
    let magnitude = num.abs().log10().floor() as i32;
    if !(-5..10).contains(&magnitude) {
        return raw.to_string();
    }
    let decimals = (9 - magnitude).max(0) as usize;
    let text = format!("{num:.decimals$}");
    if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn general_format_shortens_long_floats_but_keeps_short_values() {
        let style = CellStyle::default();
        assert_eq!(style.format_value("366.6666666666667"), "366.6666667");
        assert_eq!(style.format_value("0.30000000000000004"), "0.3");
        assert_eq!(style.format_value("1800"), "1800");
        assert_eq!(style.format_value("3.14"), "3.14");
        assert_eq!(style.format_value("-2.5"), "-2.5");
    }

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
        assert_eq!(style.format_value("1234.5"), "$1,235");
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
        style.borders = CellBorders::ALL;
        assert!(!style.is_default());
        style.borders = CellBorders::NONE;
        style.strikethrough = true;
        assert!(!style.is_default());
        style.strikethrough = false;
        style.fill = FillColor::Yellow;
        assert!(!style.is_default());
        style.fill = FillColor::None;
        style.font_size = Some(18);
        assert!(!style.is_default());
    }

    #[test]
    fn test_cell_borders_mask_roundtrip() {
        let b = CellBorders {
            top: true,
            right: false,
            bottom: true,
            left: true,
        };
        assert!(b.any());
        assert!(!b.all());
        let mask = b.to_mask();
        assert_eq!(CellBorders::from_mask(mask), b);
        assert_eq!(CellBorders::NONE.to_mask(), 0);
        assert_eq!(CellBorders::ALL.to_mask(), 0b1111);
        assert!(CellBorders::ALL.all());
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
