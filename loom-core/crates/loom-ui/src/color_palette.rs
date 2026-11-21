//! Shared 48-color office palette metadata and RGB hex conversion helpers.
//!
//! Used by Writer, Sheets, and Present to populate the shared Slint
//! `LoomColorWell` and `LoomColorPalette` foundation components with a
//! consistent 6x8 swatch matrix and normalized `#RRGGBB` / `#RRGGBBAA` parsing.

/// One named swatch entry in Loom's shared 48-color palette.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaletteSwatch {
    pub name: &'static str,
    pub hex: &'static str,
    pub rgb: u32,
}

const fn swatch(name: &'static str, hex: &'static str, rgb: u32) -> PaletteSwatch {
    PaletteSwatch { name, hex, rgb }
}

/// Canonical 6-row by 8-column office palette (48 swatches total).
pub const OFFICE_SWATCHES: [PaletteSwatch; 48] = [
    // Row 0: Neutrals
    swatch("Black", "#000000", 0x000000),
    swatch("Charcoal", "#262626", 0x262626),
    swatch("Dark Gray", "#525252", 0x525252),
    swatch("Gray", "#737373", 0x737373),
    swatch("Silver", "#A3A3A3", 0xA3A3A3),
    swatch("Light Gray", "#D4D4D4", 0xD4D4D4),
    swatch("Off White", "#EDEDED", 0xEDEDED),
    swatch("White", "#FFFFFF", 0xFFFFFF),
    // Row 1: Primary Ink
    swatch("Crimson", "#C62828", 0xC62828),
    swatch("Orange", "#E65100", 0xE65100),
    swatch("Amber", "#B45309", 0xB45309),
    swatch("Forest Green", "#2E7D32", 0x2E7D32),
    swatch("Teal", "#0F766E", 0x0F766E),
    swatch("Royal Blue", "#1565C0", 0x1565C0),
    swatch("Purple", "#6A1B9A", 0x6A1B9A),
    swatch("Rose", "#BE185D", 0xBE185D),
    // Row 2: Deep Shades
    swatch("Burgundy", "#7F1D1D", 0x7F1D1D),
    swatch("Rust", "#9A3412", 0x9A3412),
    swatch("Umber", "#78350F", 0x78350F),
    swatch("Pine", "#14532D", 0x14532D),
    swatch("Deep Teal", "#134E4A", 0x134E4A),
    swatch("Navy", "#1E3A8A", 0x1E3A8A),
    swatch("Deep Violet", "#4C1D95", 0x4C1D95),
    swatch("Plum", "#831843", 0x831843),
    // Row 3: Vivid Midtones
    swatch("Red", "#EF4444", 0xEF4444),
    swatch("Tangerine", "#F97316", 0xF97316),
    swatch("Gold", "#EAB308", 0xEAB308),
    swatch("Emerald", "#22C55E", 0x22C55E),
    swatch("Cyan Teal", "#14B8A6", 0x14B8A6),
    swatch("Blue", "#3B82F6", 0x3B82F6),
    swatch("Violet", "#8B5CF6", 0x8B5CF6),
    swatch("Pink", "#EC4899", 0xEC4899),
    // Row 4: Soft Tints
    swatch("Coral Tint", "#FCA5A5", 0xFCA5A5),
    swatch("Apricot", "#FDBA74", 0xFDBA74),
    swatch("Lemon", "#FDE047", 0xFDE047),
    swatch("Mint", "#86EFAC", 0x86EFAC),
    swatch("Aqua", "#5EEAD4", 0x5EEAD4),
    swatch("Sky", "#93C5FD", 0x93C5FD),
    swatch("Lavender", "#C4B5FD", 0xC4B5FD),
    swatch("Blush", "#F9A8D4", 0xF9A8D4),
    // Row 5: Pastels
    swatch("Blush Red", "#FECACA", 0xFECACA),
    swatch("Soft Peach", "#FED7AA", 0xFED7AA),
    swatch("Soft Yellow", "#FEF08A", 0xFEF08A),
    swatch("Soft Mint", "#BBF7D0", 0xBBF7D0),
    swatch("Ice Teal", "#CCFBF1", 0xCCFBF1),
    swatch("Soft Blue", "#BFDBFE", 0xBFDBFE),
    swatch("Soft Violet", "#DDD6FE", 0xDDD6FE),
    swatch("Soft Rose", "#FCE7F3", 0xFCE7F3),
];

/// Parses a `#RRGGBB` or `#RRGGBBAA` hexadecimal color string (trimming surrounding whitespace)
/// into a 24-bit `0x00RRGGBB` integer.
pub fn parse_hex_rgb(raw: &str) -> Option<u32> {
    let digits = raw.trim().strip_prefix('#')?;
    if !matches!(digits.len(), 6 | 8) || !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(&digits[..6], 16).ok()
}

/// Formats a 24-bit RGB value as uppercase `#RRGGBB`.
pub fn format_hex_rgb(rgb: u32) -> String {
    format!("#{:06X}", rgb & 0x00FF_FFFF)
}

/// Formats a 24-bit RGB value as opaque uppercase `#RRGGBBFF`.
pub fn format_hex_rgba(rgb: u32) -> String {
    format!("#{:06X}FF", rgb & 0x00FF_FFFF)
}

/// Returns the canonical swatch name when `rgb` matches an entry in [`OFFICE_SWATCHES`].
pub fn swatch_name_for_rgb(rgb: u32) -> Option<&'static str> {
    OFFICE_SWATCHES
        .iter()
        .find(|swatch| swatch.rgb == rgb)
        .map(|swatch| swatch.name)
}

/// Converts a 24-bit `0x00RRGGBB` integer into an opaque [`slint::Color`].
pub fn color_from_rgb(rgb: u32) -> slint::Color {
    slint::Color::from_rgb_u8(
        ((rgb >> 16) & 0xFF) as u8,
        ((rgb >> 8) & 0xFF) as u8,
        (rgb & 0xFF) as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn office_swatches_has_48_unique_valid_entries_in_6x8_grid() {
        assert_eq!(OFFICE_SWATCHES.len(), 48);

        let mut names = HashSet::new();
        let mut hexes = HashSet::new();
        let mut rgbs = HashSet::new();

        for swatch in OFFICE_SWATCHES {
            assert!(
                names.insert(swatch.name),
                "duplicate swatch name: {}",
                swatch.name
            );
            assert!(
                hexes.insert(swatch.hex),
                "duplicate swatch hex: {}",
                swatch.hex
            );
            assert!(
                rgbs.insert(swatch.rgb),
                "duplicate swatch rgb: {:#08X}",
                swatch.rgb
            );
            assert_eq!(swatch.hex, format_hex_rgb(swatch.rgb));
            assert_eq!(parse_hex_rgb(swatch.hex), Some(swatch.rgb));
            assert_eq!(swatch_name_for_rgb(swatch.rgb), Some(swatch.name));
        }

        // Spot-check row boundaries (0..=5 x 0..=7).
        assert_eq!(OFFICE_SWATCHES[0], swatch("Black", "#000000", 0x000000));
        assert_eq!(OFFICE_SWATCHES[7], swatch("White", "#FFFFFF", 0xFFFFFF));
        assert_eq!(
            OFFICE_SWATCHES[13],
            swatch("Royal Blue", "#1565C0", 0x1565C0)
        );
        assert_eq!(OFFICE_SWATCHES[21], swatch("Navy", "#1E3A8A", 0x1E3A8A));
        assert_eq!(OFFICE_SWATCHES[26], swatch("Gold", "#EAB308", 0xEAB308));
        assert_eq!(
            OFFICE_SWATCHES[32],
            swatch("Coral Tint", "#FCA5A5", 0xFCA5A5)
        );
        assert_eq!(
            OFFICE_SWATCHES[47],
            swatch("Soft Rose", "#FCE7F3", 0xFCE7F3)
        );
    }

    #[test]
    fn parse_hex_rgb_accepts_trimmed_6_and_8_digit_hex_and_rejects_invalid() {
        assert_eq!(parse_hex_rgb("#1565C0"), Some(0x1565C0));
        assert_eq!(parse_hex_rgb("  #1565c0 \n\t"), Some(0x1565C0));
        assert_eq!(parse_hex_rgb("#1565c0ff"), Some(0x1565C0));
        assert_eq!(parse_hex_rgb(" #00000080 "), Some(0x000000));
        assert_eq!(parse_hex_rgb("#ABCDEF00"), Some(0xABCDEF));

        assert_eq!(parse_hex_rgb(""), None);
        assert_eq!(parse_hex_rgb("#"), None);
        assert_eq!(parse_hex_rgb("1565C0"), None);
        assert_eq!(parse_hex_rgb("#123"), None);
        assert_eq!(parse_hex_rgb("#12345"), None);
        assert_eq!(parse_hex_rgb("#1234567"), None);
        assert_eq!(parse_hex_rgb("#123456789"), None);
        assert_eq!(parse_hex_rgb("#1565G0"), None);
        assert_eq!(parse_hex_rgb("#1565C0ZZ"), None);
        assert_eq!(parse_hex_rgb("#+12345"), None);
        assert_eq!(parse_hex_rgb("0x1565C0"), None);
    }

    #[test]
    fn format_helpers_output_uppercase_hex_strings() {
        assert_eq!(format_hex_rgb(0x000000), "#000000");
        assert_eq!(format_hex_rgb(0x0f766e), "#0F766E");
        assert_eq!(format_hex_rgb(0xffffff), "#FFFFFF");
        assert_eq!(format_hex_rgb(0xFF15_65C0), "#1565C0");

        assert_eq!(format_hex_rgba(0x000000), "#000000FF");
        assert_eq!(format_hex_rgba(0x1565c0), "#1565C0FF");
        assert_eq!(format_hex_rgba(0xFFBE_185D), "#BE185DFF");
    }

    #[test]
    fn swatch_name_for_rgb_returns_none_for_custom_colors() {
        assert_eq!(swatch_name_for_rgb(0x000000), Some("Black"));
        assert_eq!(swatch_name_for_rgb(0x0F766E), Some("Teal"));
        assert_eq!(swatch_name_for_rgb(0x123456), None);
        assert_eq!(swatch_name_for_rgb(0xFF00_0000), None);
    }

    #[test]
    fn color_from_rgb_constructs_opaque_slint_color() {
        let color = color_from_rgb(0x1565C0);
        assert_eq!(color.red(), 0x15);
        assert_eq!(color.green(), 0x65);
        assert_eq!(color.blue(), 0xC0);
        assert_eq!(color.alpha(), 0xFF);

        let black = color_from_rgb(0x000000);
        assert_eq!(black, slint::Color::from_argb_encoded(0xFF00_0000));

        let white = color_from_rgb(0xFFFFFF);
        assert_eq!(white, slint::Color::from_argb_encoded(0xFFFF_FFFF));
    }
}
