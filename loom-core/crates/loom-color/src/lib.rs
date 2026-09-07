//! `loom-color` provides color spaces, conversions, and blending used across
//! the Loom rendering pipeline. All conversions are pure functions with
//! deterministic behavior and are unit-tested against known reference values.

/// An RGBA color with linear-light RGB components when marked linear.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    /// Red in `[0,1]`.
    pub r: f32,
    /// Green in `[0,1]`.
    pub g: f32,
    /// Blue in `[0,1]`.
    pub b: f32,
    /// Alpha in `[0,1]`.
    pub a: f32,
}

impl Color {
    /// Create a new color.
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// Opaque color.
    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self::new(r, g, b, 1.0)
    }

    /// Black (opaque).
    pub const fn black() -> Self {
        Self::rgb(0.0, 0.0, 0.0)
    }

    /// White (opaque).
    pub const fn white() -> Self {
        Self::rgb(1.0, 1.0, 1.0)
    }

    /// Transparent black.
    pub const fn transparent() -> Self {
        Self::new(0.0, 0.0, 0.0, 0.0)
    }

    /// Clamp components to `[0,1]`.
    pub fn clamp(self) -> Self {
        Self {
            r: self.r.clamp(0.0, 1.0),
            g: self.g.clamp(0.0, 1.0),
            b: self.b.clamp(0.0, 1.0),
            a: self.a.clamp(0.0, 1.0),
        }
    }

    /// Premultiplied alpha components.
    pub fn premultiplied(self) -> [f32; 4] {
        [self.r * self.a, self.g * self.a, self.b * self.a, self.a]
    }

    /// Convert to a linear-light RGB using sRGB transfer function.
    pub fn to_linear(self) -> Self {
        Self {
            r: srgb_to_linear(self.r),
            g: srgb_to_linear(self.g),
            b: srgb_to_linear(self.b),
            a: self.a,
        }
    }

    /// Convert from linear-light RGB to sRGB.
    pub fn to_srgb(self) -> Self {
        Self {
            r: linear_to_srgb(self.r),
            g: linear_to_srgb(self.g),
            b: linear_to_srgb(self.b),
            a: self.a,
        }
    }
}

/// Convert a single sRGB component (0..1) to linear light.
pub fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Convert a linear-light component to sRGB.
pub fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.0031308 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// CIE standard illuminant D65 in XYZ (Y normalized to 1.0).
pub const D65: [f32; 3] = [0.95047, 1.0, 1.08883];

/// Convert sRGB (non-linear) to CIE XYZ (D65) with color management.
pub fn srgb_to_xyz(c: Color) -> [f32; 3] {
    let lin = c.to_linear();
    // sRGB => linear => XYZ via the canonical matrix.
    let r = lin.r;
    let g = lin.g;
    let b = lin.b;
    let x = 0.4124564 * r + 0.3575761 * g + 0.1804375 * b;
    let y = 0.2126729 * r + 0.7151522 * g + 0.0721750 * b;
    let z = 0.0193339 * r + 0.119_192 * g + 0.9503041 * b;
    [x, y, z]
}

/// Convert CIE XYZ (D65) to sRGB.
pub fn xyz_to_srgb(xyz: [f32; 3]) -> Color {
    let (x, y, z) = (xyz[0], xyz[1], xyz[2]);
    let r = 3.2404542 * x - 1.5371385 * y - 0.4985314 * z;
    let g = -0.969_266 * x + 1.8760108 * y + 0.0415560 * z;
    let b = 0.0556434 * x - 0.2040259 * y + 1.0572252 * z;
    Color::rgb(r, g, b).to_srgb().clamp()
}

/// Conversion reference values (sRGB to XYZ).
pub fn srgb_white_xyz() -> [f32; 3] {
    srgb_to_xyz(Color::white())
}

/// Perceptual distance using the simple CIE76 / (a,b) proxy for tests.
pub fn perceptual_distance(a: Color, b: Color) -> f32 {
    let [r1, g1, b1] = a.to_linear().premultiplied()[..3].try_into().unwrap();
    let [r2, g2, b2] = b.to_linear().premultiplied()[..3].try_into().unwrap();
    ((r1 - r2).powi(2) + (g1 - g2).powi(2) + (b1 - b2).powi(2)).sqrt()
}

/// Blend modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendMode {
    /// Normal (source over).
    Normal,
    /// Multiply.
    Multiply,
    /// Screen.
    Screen,
    /// Overlay.
    Overlay,
    /// Darken.
    Darken,
    /// Lighten.
    Lighten,
    /// Difference.
    Difference,
    /// Exclusion.
    Exclusion,
}

/// Blend `src` over `dst` using `mode` with standard composite math.
pub fn blend(src: Color, dst: Color, mode: BlendMode) -> Color {
    let (sb, db) = (src.to_linear(), dst.to_linear());
    let mixed = match mode {
        BlendMode::Normal => sb,
        BlendMode::Multiply => Color::rgb(sb.r * db.r, sb.g * db.g, sb.b * db.b),
        BlendMode::Screen => Color::rgb(
            1.0 - (1.0 - sb.r) * (1.0 - db.r),
            1.0 - (1.0 - sb.g) * (1.0 - db.g),
            1.0 - (1.0 - sb.b) * (1.0 - db.b),
        ),
        BlendMode::Overlay => Color::rgb(
            overlay(sb.r, db.r),
            overlay(sb.g, db.g),
            overlay(sb.b, db.b),
        ),
        BlendMode::Darken => Color::rgb(sb.r.min(db.r), sb.g.min(db.g), sb.b.min(db.b)),
        BlendMode::Lighten => Color::rgb(sb.r.max(db.r), sb.g.max(db.g), sb.b.max(db.b)),
        BlendMode::Difference => Color::rgb(
            (sb.r - db.r).abs(),
            (sb.g - db.g).abs(),
            (sb.b - db.b).abs(),
        ),
        BlendMode::Exclusion => Color::rgb(
            sb.r + db.r - 2.0 * sb.r * db.r,
            sb.g + db.g - 2.0 * sb.g * db.g,
            sb.b + db.b - 2.0 * sb.b * db.b,
        ),
    };
    // Alpha-composite: out = src.a * src + (1 - src.a) * dst
    let a = src.a + dst.a * (1.0 - src.a);
    let mixed_srgb = mixed.to_srgb().clamp();
    let dst_srgb = dst.to_srgb().clamp();
    if a > 0.0 {
        Color::new(
            (src.a * mixed_srgb.r + (1.0 - src.a) * dst_srgb.r) / a,
            (src.a * mixed_srgb.g + (1.0 - src.a) * dst_srgb.g) / a,
            (src.a * mixed_srgb.b + (1.0 - src.a) * dst_srgb.b) / a,
            a,
        )
    } else {
        Color::transparent()
    }
}

fn overlay(s: f32, d: f32) -> f32 {
    if d <= 0.5 {
        2.0 * s * d
    } else {
        1.0 - 2.0 * (1.0 - s) * (1.0 - d)
    }
}

/// Parse an sRGB hex string like `#RRGGBB` or `#RRGGBBAA` into a `Color`.
pub fn parse_hex(s: &str) -> Option<Color> {
    let s = s.strip_prefix('#')?;
    let (r, g, b, a) = match s.len() {
        6 => (
            u8::from_str_radix(&s[0..2], 16).ok()?,
            u8::from_str_radix(&s[2..4], 16).ok()?,
            u8::from_str_radix(&s[4..6], 16).ok()?,
            255u8,
        ),
        8 => (
            u8::from_str_radix(&s[0..2], 16).ok()?,
            u8::from_str_radix(&s[2..4], 16).ok()?,
            u8::from_str_radix(&s[4..6], 16).ok()?,
            u8::from_str_radix(&s[6..8], 16).ok()?,
        ),
        _ => return None,
    };
    Some(Color::new(
        r as f32 / 255.0,
        g as f32 / 255.0,
        b as f32 / 255.0,
        a as f32 / 255.0,
    ))
}

/// Render a color as `#RRGGBBAA`.
pub fn to_hex(c: Color) -> String {
    let c = c.clamp();
    format!(
        "#{:02x}{:02x}{:02x}{:02x}",
        (c.r * 255.0).round() as u8,
        (c.g * 255.0).round() as u8,
        (c.b * 255.0).round() as u8,
        (c.a * 255.0).round() as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srgb_linear_known_vector() {
        // sRGB 0.5 non-linear maps to a known linear value.
        let l = srgb_to_linear(0.5);
        assert!((l - 0.21404114).abs() < 1e-5);
        // Round trip.
        assert!((linear_to_srgb(l) - 0.5).abs() < 1e-4);
    }

    #[test]
    fn white_black_extremes() {
        assert_eq!(srgb_to_linear(0.0), 0.0);
        assert_eq!(srgb_to_linear(1.0), 1.0);
        assert_eq!(linear_to_srgb(0.0), 0.0);
        // 1.0^(1/2.4) has fp error; allow small tolerance.
        assert!((linear_to_srgb(1.0) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn xyz_reference() {
        // sRGB white => D65 white (Y=1, X~0.95, Z~1.09)
        let w = srgb_white_xyz();
        assert!((w[1] - 1.0).abs() < 1e-5, "Y should be ~1, got {}", w[1]);
        assert!((w[0] - D65[0]).abs() < 1e-3);
        assert!((w[2] - D65[2]).abs() < 1e-3);
        // Black => 0.
        let b = srgb_to_xyz(Color::black());
        assert!(b.iter().all(|c| c.abs() < 1e-5));
    }

    #[test]
    fn xyz_roundtrip() {
        let c = Color::rgb(0.2, 0.5, 0.8);
        let xyz = srgb_to_xyz(c);
        let back = xyz_to_srgb(xyz);
        // Tolerances for matrix round-trip (matrix is near-invertible).
        assert!((c.r - back.r).abs() < 0.02, "r {:?} vs {:?}", c.r, back.r);
        assert!((c.g - back.g).abs() < 0.02);
        assert!((c.b - back.b).abs() < 0.02);
    }

    #[test]
    fn blend_normal() {
        // Opaque source over transparent = source.
        let s = Color::rgb(1.0, 0.0, 0.0);
        let d = Color::transparent();
        let out = blend(s, d, BlendMode::Normal);
        assert!((out.r - 1.0).abs() < 1e-5);
        assert_eq!(out.a, 1.0);
    }

    #[test]
    fn blend_multiply_known() {
        let s = Color::rgb(1.0, 1.0, 1.0); // white
        let d = Color::rgb(0.5, 0.25, 0.125);
        let out = blend(s, d, BlendMode::Multiply);
        assert!((out.r - 0.5).abs() < 1e-3);
        assert!((out.g - 0.25).abs() < 1e-3);
        assert!((out.b - 0.125).abs() < 1e-3);
    }

    #[test]
    fn hex_parse_format() {
        let c = parse_hex("#ff8000").unwrap();
        assert!((c.r - 1.0).abs() < 0.001);
        assert!((c.g - 0x80 as f32 / 255.0).abs() < 0.001);
        assert_eq!(c.a, 1.0);
        let with_alpha = parse_hex("#ff800040").unwrap();
        assert!((with_alpha.a - 0x40 as f32 / 255.0).abs() < 0.001);
        assert_eq!(to_hex(with_alpha), "#ff800040");
        assert!(parse_hex("invalid").is_none());
        assert!(parse_hex("#xyz").is_none());
    }

    #[test]
    fn premultiply() {
        let c = Color::new(0.5, 0.5, 0.5, 0.5);
        let p = c.premultiplied();
        assert!((p[0] - 0.25).abs() < 1e-6);
        assert_eq!(p[3], 0.5);
    }
}
