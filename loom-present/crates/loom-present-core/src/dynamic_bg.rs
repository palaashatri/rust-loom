//! Procedural animated Dynamic Background presets and fast pure-Rust RGBA8 renderer.
//!
//! Keynote-style flowing mesh/aurora/ribbon backgrounds for slides. Each preset
//! provides a static fallback hex color, multi-stop OOXML gradient metadata for
//! PowerPoint export, and a deterministic procedural RGBA8 frame renderer for
//! live canvas/thumbnail/presenter display and PDF export.

use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};

/// Built-in procedural dynamic background presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DynamicBgPreset {
    AuroraVeil,
    SolarMesh,
    OceanicSilk,
    PrismGlass,
    ObsidianOrbs,
    BotanicalMist,
}

impl DynamicBgPreset {
    /// All six built-in presets in canonical catalog order.
    pub const ALL: [DynamicBgPreset; 6] = [
        Self::AuroraVeil,
        Self::SolarMesh,
        Self::OceanicSilk,
        Self::PrismGlass,
        Self::ObsidianOrbs,
        Self::BotanicalMist,
    ];

    /// Stable machine identifier used in serialization and template IDs.
    pub const fn id(self) -> &'static str {
        match self {
            Self::AuroraVeil => "aurora-veil",
            Self::SolarMesh => "solar-mesh",
            Self::OceanicSilk => "oceanic-silk",
            Self::PrismGlass => "prism-glass",
            Self::ObsidianOrbs => "obsidian-orbs",
            Self::BotanicalMist => "botanical-mist",
        }
    }

    /// Template catalog ID corresponding to this preset.
    pub const fn template_id(self) -> &'static str {
        match self {
            Self::AuroraVeil => "dynamic-aurora",
            Self::SolarMesh => "dynamic-solar",
            Self::OceanicSilk => "dynamic-oceanic",
            Self::PrismGlass => "dynamic-prism",
            Self::ObsidianOrbs => "dynamic-obsidian",
            Self::BotanicalMist => "dynamic-botanical",
        }
    }

    /// Looks up a preset by its stable ID or template ID.
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "aurora-veil" | "dynamic-aurora" => Some(Self::AuroraVeil),
            "solar-mesh" | "dynamic-solar" => Some(Self::SolarMesh),
            "oceanic-silk" | "dynamic-oceanic" => Some(Self::OceanicSilk),
            "prism-glass" | "dynamic-prism" => Some(Self::PrismGlass),
            "obsidian-orbs" | "dynamic-obsidian" => Some(Self::ObsidianOrbs),
            "botanical-mist" | "dynamic-botanical" => Some(Self::BotanicalMist),
            _ => None,
        }
    }

    /// Zero-based index in [`Self::ALL`].
    pub const fn index(self) -> usize {
        match self {
            Self::AuroraVeil => 0,
            Self::SolarMesh => 1,
            Self::OceanicSilk => 2,
            Self::PrismGlass => 3,
            Self::ObsidianOrbs => 4,
            Self::BotanicalMist => 5,
        }
    }

    /// Looks up a preset by its zero-based index in [`Self::ALL`].
    pub const fn from_index(index: usize) -> Option<Self> {
        match index {
            0 => Some(Self::AuroraVeil),
            1 => Some(Self::SolarMesh),
            2 => Some(Self::OceanicSilk),
            3 => Some(Self::PrismGlass),
            4 => Some(Self::ObsidianOrbs),
            5 => Some(Self::BotanicalMist),
            _ => None,
        }
    }

    /// Human-readable preset title.
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::AuroraVeil => "Aurora Veil",
            Self::SolarMesh => "Solar Mesh",
            Self::OceanicSilk => "Oceanic Silk",
            Self::PrismGlass => "Prism Glass",
            Self::ObsidianOrbs => "Obsidian Orbs",
            Self::BotanicalMist => "Botanical Mist",
        }
    }

    /// Static solid-color fallback hex (`#RRGGBB`) for legacy surfaces.
    pub const fn static_fallback_hex(self) -> &'static str {
        match self {
            Self::AuroraVeil => "#0B1226",
            Self::SolarMesh => "#26110B",
            Self::OceanicSilk => "#061A2B",
            Self::PrismGlass => "#F3F0FF",
            Self::ObsidianOrbs => "#09090B",
            Self::BotanicalMist => "#0A1F18",
        }
    }

    /// Four-color palette `[base, primary_glow, secondary_glow, highlight_glow]`.
    pub const fn default_colors(self) -> [&'static str; 4] {
        match self {
            Self::AuroraVeil => ["#0B1226", "#14B8A6", "#6366F1", "#38BDF8"],
            Self::SolarMesh => ["#26110B", "#EA580C", "#F59E0B", "#FB7185"],
            Self::OceanicSilk => ["#061A2B", "#0284C7", "#0D9488", "#38BDF8"],
            Self::PrismGlass => ["#F3F0FF", "#BAE6FD", "#FBCFE8", "#DDD6FE"],
            Self::ObsidianOrbs => ["#09090B", "#4F46E5", "#9333EA", "#EC4899"],
            Self::BotanicalMist => ["#0A1F18", "#15803D", "#10B981", "#84CC16"],
        }
    }

    /// Multi-stop linear gradient metadata `(ooxml_pos_0_to_100000, "#RRGGBB")`.
    pub const fn gradient_stops(self) -> &'static [(u32, &'static str)] {
        match self {
            Self::AuroraVeil => &[
                (0, "#0B1226"),
                (38_000, "#124E5B"),
                (72_000, "#3D2C6E"),
                (100_000, "#0F172A"),
            ],
            Self::SolarMesh => &[
                (0, "#26110B"),
                (36_000, "#9A3412"),
                (70_000, "#D97706"),
                (100_000, "#431407"),
            ],
            Self::OceanicSilk => &[
                (0, "#061A2B"),
                (40_000, "#0E4F6E"),
                (74_000, "#0D9488"),
                (100_000, "#082F49"),
            ],
            Self::PrismGlass => &[
                (0, "#F3F0FF"),
                (35_000, "#E0F2FE"),
                (70_000, "#FCE7F3"),
                (100_000, "#EDE9FE"),
            ],
            Self::ObsidianOrbs => &[
                (0, "#09090B"),
                (42_000, "#1E1B4B"),
                (75_000, "#4C1D95"),
                (100_000, "#18181B"),
            ],
            Self::BotanicalMist => &[
                (0, "#0A1F18"),
                (38_000, "#14532D"),
                (72_000, "#15803D"),
                (100_000, "#052E16"),
            ],
        }
    }

    /// Linear gradient angle in degrees for OOXML `<a:lin ang="..."/>`.
    pub const fn gradient_angle_deg(self) -> f32 {
        match self {
            Self::AuroraVeil => 135.0,
            Self::SolarMesh => 125.0,
            Self::OceanicSilk => 140.0,
            Self::PrismGlass => 130.0,
            Self::ObsidianOrbs => 135.0,
            Self::BotanicalMist => 130.0,
        }
    }

    /// Whether text on this preset defaults to light ink (dark background).
    pub const fn is_dark(self) -> bool {
        !matches!(self, Self::PrismGlass)
    }
}

fn default_speed() -> f32 {
    1.0
}

fn default_animate() -> bool {
    true
}

/// Per-slide dynamic background configuration persisted inside `.loomdeck`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DynamicBackground {
    pub preset: DynamicBgPreset,
    #[serde(default = "default_speed")]
    pub speed: f32,
    #[serde(default = "default_animate")]
    pub animate: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub custom_colors: Vec<String>,
}

impl DynamicBackground {
    /// Creates an animated background for `preset` at calm (`1.0`) speed.
    pub fn new(preset: DynamicBgPreset) -> Self {
        Self {
            preset,
            speed: 1.0,
            animate: true,
            custom_colors: Vec::new(),
        }
    }

    /// Static hex color fallback (`#RRGGBB`) for solid-only renderers.
    pub fn static_fallback_hex(&self) -> String {
        if let Some(first) = self.custom_colors.first() {
            if parse_hex_rgb(first).is_some() {
                return first.clone();
            }
        }
        self.preset.static_fallback_hex().to_string()
    }

    /// Effective OOXML gradient stops `(pos_0_to_100000, "RRGGBB")` (uppercase, no `#`).
    pub fn ooxml_gradient_stops(&self) -> Vec<(u32, String)> {
        let defaults = self.preset.gradient_stops();
        defaults
            .iter()
            .enumerate()
            .map(|(idx, &(pos, hex))| {
                let chosen = self
                    .custom_colors
                    .get(idx)
                    .filter(|c| parse_hex_rgb(c).is_some())
                    .map(String::as_str)
                    .unwrap_or(hex);
                (pos, strip_hex(chosen))
            })
            .collect()
    }

    /// Effective 4-color floating-point RGB palette `[base, c1, c2, c3]` in `0.0..=1.0`.
    pub fn effective_rgb_palette(&self) -> [[f32; 3]; 4] {
        let defaults = self.preset.default_colors();
        let mut out = [[0.0f32; 3]; 4];
        for i in 0..4 {
            let custom = self.custom_colors.get(i).and_then(|s| parse_hex_rgb(s));
            out[i] =
                custom.unwrap_or_else(|| parse_hex_rgb(defaults[i]).unwrap_or([0.0, 0.0, 0.0]));
        }
        out
    }

    /// Hashes all visual fields for thumbnail and cache invalidation.
    pub fn hash_key<H: Hasher>(&self, state: &mut H) {
        self.preset.hash(state);
        self.speed.to_bits().hash(state);
        self.animate.hash(state);
        self.custom_colors.hash(state);
    }

    /// Compact stable cache key string (ignores phase).
    pub fn cache_key(&self) -> String {
        format!(
            "dyn:{}:{}:{}",
            self.preset.id(),
            self.speed.to_bits(),
            self.custom_colors.join(",")
        )
    }
}

fn strip_hex(raw: &str) -> String {
    let trimmed = raw.trim().trim_start_matches('#');
    if trimmed.len() == 6 && trimmed.bytes().all(|b| b.is_ascii_hexdigit()) {
        trimmed.to_ascii_uppercase()
    } else {
        "0B1226".to_string()
    }
}

/// Parses `#RRGGBB` or `RRGGBB` into normalized `[r, g, b]` in `0.0..=1.0`.
pub fn parse_hex_rgb(raw: &str) -> Option<[f32; 3]> {
    let hex = raw.trim().trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some([r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0])
}

#[inline]
fn lerp_rgb(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    let t = t.clamp(0.0, 1.0);
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

#[inline]
fn screen_blend(base: [f32; 3], glow: [f32; 3], weight: f32) -> [f32; 3] {
    let w = weight.clamp(0.0, 1.0);
    [
        base[0] + (1.0 - base[0]) * glow[0] * w,
        base[1] + (1.0 - base[1]) * glow[1] * w,
        base[2] + (1.0 - base[2]) * glow[2] * w,
    ]
}

#[inline]
fn soft_gauss(dist_sq: f32, inv_radius_sq: f32) -> f32 {
    // Fast smooth bell curve 1 / (1 + d^2 / r^2)^2 — avoids per-pixel exp()!
    let q = 1.0 + dist_sq * inv_radius_sq;
    1.0 / (q * q)
}

#[inline]
fn to_u8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

/// Renders one procedural RGBA8 frame for `bg` at time `phase_secs` into `out_rgba`.
///
/// Reuses `out_rgba`'s allocation across frames. Always produces `width * height * 4`
/// tightly packed opaque RGBA8 bytes (`alpha == 255`).
pub fn render_dynamic_frame(
    bg: &DynamicBackground,
    phase_secs: f32,
    width: u32,
    height: u32,
    out_rgba: &mut Vec<u8>,
) {
    if width == 0 || height == 0 {
        out_rgba.clear();
        return;
    }
    let total = (width as usize)
        .saturating_mul(height as usize)
        .saturating_mul(4);
    out_rgba.resize(total, 255);

    let speed = if bg.speed.is_finite() {
        bg.speed.clamp(0.05, 4.0)
    } else {
        1.0
    };
    let t = if bg.animate && phase_secs.is_finite() {
        phase_secs * speed
    } else {
        0.0
    };

    let [base, c1, c2, c3] = bg.effective_rgb_palette();
    let w = width as usize;
    let h = height as usize;
    let inv_w = if w > 1 { 1.0 / (w - 1) as f32 } else { 0.0 };
    let inv_h = if h > 1 { 1.0 / (h - 1) as f32 } else { 0.0 };

    match bg.preset {
        DynamicBgPreset::AuroraVeil => {
            // Precompute per-column ribbon wave offsets so the inner loop is pure arithmetic.
            let mut rib1_y = vec![0.0f32; w];
            let mut rib2_y = vec![0.0f32; w];
            let mut shimmer = vec![0.0f32; w];
            for x in 0..w {
                let u = x as f32 * inv_w;
                rib1_y[x] =
                    0.34 + 0.14 * (u * 5.2 + t * 0.72).sin() + 0.06 * (u * 10.1 - t * 0.45).cos();
                rib2_y[x] = 0.64
                    + 0.13 * (u * 4.3 - t * 0.58 + 1.4).sin()
                    + 0.05 * (u * 8.7 + t * 0.62).cos();
                shimmer[x] = 0.55 + 0.45 * (u * 6.5 + t * 0.9).sin();
            }
            for y in 0..h {
                let v = y as f32 * inv_h;
                let sky = lerp_rgb(base, c2, 0.18 * (1.0 - v));
                let row = y * w * 4;
                for x in 0..w {
                    let d1 = v - rib1_y[x];
                    let d2 = v - rib2_y[x];
                    let w1 = soft_gauss(d1 * d1, 22.0) * (0.68 * shimmer[x]);
                    let w2 = soft_gauss(d2 * d2, 18.0) * (0.60 * (1.15 - 0.4 * shimmer[x]));
                    let w3 = soft_gauss((d1 + 0.06) * (d1 + 0.06), 45.0) * 0.35;
                    let col = screen_blend(screen_blend(screen_blend(sky, c1, w1), c2, w2), c3, w3);
                    let idx = row + x * 4;
                    out_rgba[idx] = to_u8(col[0]);
                    out_rgba[idx + 1] = to_u8(col[1]);
                    out_rgba[idx + 2] = to_u8(col[2]);
                    out_rgba[idx + 3] = 255;
                }
            }
        }
        DynamicBgPreset::SolarMesh => {
            let p1 = (
                0.24 + 0.16 * (t * 0.55).sin(),
                0.28 + 0.14 * (t * 0.42).cos(),
            );
            let p2 = (
                0.76 + 0.14 * (t * 0.48 + 2.1).cos(),
                0.34 + 0.18 * (t * 0.51).sin(),
            );
            let p3 = (
                0.52 + 0.18 * (t * 0.39 + 4.0).sin(),
                0.76 + 0.12 * (t * 0.63).cos(),
            );
            for y in 0..h {
                let v = y as f32 * inv_h;
                let dy1 = (v - p1.1) * 0.72;
                let dy2 = (v - p2.1) * 0.72;
                let dy3 = (v - p3.1) * 0.72;
                let dy1s = dy1 * dy1;
                let dy2s = dy2 * dy2;
                let dy3s = dy3 * dy3;
                let row = y * w * 4;
                for x in 0..w {
                    let u = x as f32 * inv_w;
                    let w1 = soft_gauss((u - p1.0) * (u - p1.0) + dy1s, 4.8) * 0.85;
                    let w2 = soft_gauss((u - p2.0) * (u - p2.0) + dy2s, 4.2) * 0.78;
                    let w3 = soft_gauss((u - p3.0) * (u - p3.0) + dy3s, 5.5) * 0.62;
                    let col =
                        screen_blend(screen_blend(screen_blend(base, c1, w1), c2, w2), c3, w3);
                    let idx = row + x * 4;
                    out_rgba[idx] = to_u8(col[0]);
                    out_rgba[idx + 1] = to_u8(col[1]);
                    out_rgba[idx + 2] = to_u8(col[2]);
                    out_rgba[idx + 3] = 255;
                }
            }
        }
        DynamicBgPreset::OceanicSilk => {
            let mut wave_a = vec![0.0f32; w];
            let mut wave_b = vec![0.0f32; w];
            for x in 0..w {
                let u = x as f32 * inv_w;
                wave_a[x] = (u * 5.8 + t * 0.85).sin();
                wave_b[x] = (u * 3.4 - t * 0.52 + 0.9).cos();
            }
            for y in 0..h {
                let v = y as f32 * inv_h;
                let sv = (v * 4.2 - t * 0.48).sin();
                let depth = lerp_rgb(base, c1, 0.25 + 0.25 * (1.0 - v));
                let row = y * w * 4;
                for x in 0..w {
                    let u = x as f32 * inv_w;
                    let crest1 = (0.5 + 0.5 * (wave_a[x] * 0.65 + sv * 0.55)).powi(2);
                    let crest2 = (0.5
                        + 0.5 * (wave_b[x] * 0.7 + (v * 6.0 + u * 2.0 + t * 0.6).sin() * 0.5))
                        .powi(2);
                    let col =
                        screen_blend(screen_blend(depth, c2, crest1 * 0.58), c3, crest2 * 0.34);
                    let idx = row + x * 4;
                    out_rgba[idx] = to_u8(col[0]);
                    out_rgba[idx + 1] = to_u8(col[1]);
                    out_rgba[idx + 2] = to_u8(col[2]);
                    out_rgba[idx + 3] = 255;
                }
            }
        }
        DynamicBgPreset::PrismGlass => {
            let p1 = (
                0.28 + 0.14 * (t * 0.5).cos(),
                0.30 + 0.15 * (t * 0.45).sin(),
            );
            let p2 = (
                0.74 + 0.15 * (t * 0.44 + 1.8).sin(),
                0.66 + 0.14 * (t * 0.52).cos(),
            );
            let diag_band: Vec<f32> = (0..w)
                .map(|x| (x as f32 * inv_w) * 6.0 + t * 0.65)
                .collect();
            for y in 0..h {
                let v = y as f32 * inv_h;
                let dy1s = (v - p1.1) * (v - p1.1) * 0.65;
                let dy2s = (v - p2.1) * (v - p2.1) * 0.65;
                let row = y * w * 4;
                for (x, &band) in diag_band.iter().enumerate() {
                    let u = x as f32 * inv_w;
                    let w1 = soft_gauss((u - p1.0) * (u - p1.0) + dy1s, 3.6);
                    let w2 = soft_gauss((u - p2.0) * (u - p2.0) + dy2s, 3.8);
                    let caustic = 0.5 + 0.5 * (band - v * 4.4).sin();
                    let col = lerp_rgb(
                        lerp_rgb(lerp_rgb(base, c1, w1 * 0.72), c2, w2 * 0.68),
                        c3,
                        caustic * 0.32,
                    );
                    let idx = row + x * 4;
                    out_rgba[idx] = to_u8(col[0]);
                    out_rgba[idx + 1] = to_u8(col[1]);
                    out_rgba[idx + 2] = to_u8(col[2]);
                    out_rgba[idx + 3] = 255;
                }
            }
        }
        DynamicBgPreset::ObsidianOrbs => {
            let o1 = (
                0.22 + 0.18 * (t * 0.46).cos(),
                0.36 + 0.20 * (t * 0.58).sin(),
            );
            let o2 = (
                0.78 + 0.16 * (t * 0.52 + 1.2).sin(),
                0.62 + 0.18 * (t * 0.41).cos(),
            );
            let o3 = (
                0.50 + 0.22 * (t * 0.37 + 2.9).sin(),
                0.24 + 0.16 * (t * 0.64).cos(),
            );
            for y in 0..h {
                let v = y as f32 * inv_h;
                let dy1s = (v - o1.1) * (v - o1.1) * 0.5625;
                let dy2s = (v - o2.1) * (v - o2.1) * 0.5625;
                let dy3s = (v - o3.1) * (v - o3.1) * 0.5625;
                let row = y * w * 4;
                for x in 0..w {
                    let u = x as f32 * inv_w;
                    let w1 = soft_gauss((u - o1.0) * (u - o1.0) + dy1s, 12.0) * 0.85;
                    let w2 = soft_gauss((u - o2.0) * (u - o2.0) + dy2s, 10.5) * 0.82;
                    let w3 = soft_gauss((u - o3.0) * (u - o3.0) + dy3s, 14.0) * 0.72;
                    let col =
                        screen_blend(screen_blend(screen_blend(base, c1, w1), c2, w2), c3, w3);
                    let idx = row + x * 4;
                    out_rgba[idx] = to_u8(col[0]);
                    out_rgba[idx + 1] = to_u8(col[1]);
                    out_rgba[idx + 2] = to_u8(col[2]);
                    out_rgba[idx + 3] = 255;
                }
            }
        }
        DynamicBgPreset::BotanicalMist => {
            let m1 = (
                0.30 + 0.15 * (t * 0.38).sin(),
                0.42 + 0.16 * (t * 0.44).cos(),
            );
            let m2 = (
                0.72 + 0.14 * (t * 0.42 + 2.4).cos(),
                0.60 + 0.14 * (t * 0.35).sin(),
            );
            let drift: Vec<f32> = (0..w)
                .map(|x| 0.5 + 0.5 * ((x as f32 * inv_w) * 4.8 - t * 0.40).sin())
                .collect();
            for y in 0..h {
                let v = y as f32 * inv_h;
                let dy1s = (v - m1.1) * (v - m1.1) * 0.62;
                let dy2s = (v - m2.1) * (v - m2.1) * 0.62;
                let mist_band = 0.5 + 0.5 * (v * 3.6 + t * 0.33).cos();
                let row = y * w * 4;
                for (x, &dx) in drift.iter().enumerate() {
                    let u = x as f32 * inv_w;
                    let w1 = soft_gauss((u - m1.0) * (u - m1.0) + dy1s, 5.2) * 0.75;
                    let w2 = soft_gauss((u - m2.0) * (u - m2.0) + dy2s, 4.6) * 0.68;
                    let w3 = dx * mist_band * 0.24;
                    let col =
                        screen_blend(screen_blend(screen_blend(base, c1, w1), c2, w2), c3, w3);
                    let idx = row + x * 4;
                    out_rgba[idx] = to_u8(col[0]);
                    out_rgba[idx + 1] = to_u8(col[1]);
                    out_rgba[idx + 2] = to_u8(col[2]);
                    out_rgba[idx + 3] = 255;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_presets_render_valid_distinct_frames() {
        let mut frames = Vec::new();
        let mut buf = Vec::new();
        for preset in DynamicBgPreset::ALL {
            let bg = DynamicBackground::new(preset);
            render_dynamic_frame(&bg, 0.0, 64, 36, &mut buf);
            assert_eq!(buf.len(), 64 * 36 * 4);
            assert!(buf.chunks_exact(4).all(|px| px[3] == 255));
            // Must not be a flat single-color image.
            let first = &buf[0..4];
            assert!(
                buf.chunks_exact(4).any(|px| px != first),
                "{preset:?} rendered flat"
            );
            assert!(parse_hex_rgb(preset.static_fallback_hex()).is_some());
            assert!(preset.gradient_stops().len() >= 3);
            frames.push((preset, buf.clone()));
        }
        for i in 0..frames.len() {
            for j in (i + 1)..frames.len() {
                assert_ne!(
                    frames[i].1, frames[j].1,
                    "{:?} and {:?} must render distinct frames",
                    frames[i].0, frames[j].0
                );
            }
        }
    }

    #[test]
    fn animation_phase_and_custom_colors_affect_output() {
        let mut bg = DynamicBackground::new(DynamicBgPreset::AuroraVeil);
        let mut f0 = Vec::new();
        let mut f1 = Vec::new();
        render_dynamic_frame(&bg, 0.0, 48, 27, &mut f0);
        render_dynamic_frame(&bg, 1.25, 48, 27, &mut f1);
        assert_ne!(f0, f1, "advancing phase must animate the frame");

        // When animate == false, phase changes are ignored (static frame).
        bg.animate = false;
        let mut f_static_a = Vec::new();
        let mut f_static_b = Vec::new();
        render_dynamic_frame(&bg, 0.0, 48, 27, &mut f_static_a);
        render_dynamic_frame(&bg, 3.5, 48, 27, &mut f_static_b);
        assert_eq!(f_static_a, f_static_b);

        // Custom colors override default palette.
        bg.custom_colors = vec!["#112233".into(), "#FF00AA".into()];
        let mut f_custom = Vec::new();
        render_dynamic_frame(&bg, 0.0, 48, 27, &mut f_custom);
        assert_ne!(f_static_a, f_custom);
        assert_eq!(bg.static_fallback_hex(), "#112233");
    }

    #[test]
    fn serde_round_trip_and_zero_size_safety() {
        let bg = DynamicBackground {
            preset: DynamicBgPreset::OceanicSilk,
            speed: 1.8,
            animate: false,
            custom_colors: vec!["#061A2B".into(), "#0EA5E9".into()],
        };
        let json = serde_json::to_string(&bg).expect("serialize");
        let parsed: DynamicBackground = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed, bg);

        let mut buf = vec![1, 2, 3];
        render_dynamic_frame(&bg, 0.0, 0, 10, &mut buf);
        assert!(buf.is_empty());
    }
}
