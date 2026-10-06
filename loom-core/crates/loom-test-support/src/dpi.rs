//! Shared checks for captures taken at a real device scale factor.
//!
//! A scale factor changes pixel density only: the logical layout must stay
//! identical and the image must be `logical * factor` pixels. These helpers
//! hold the arithmetic every app's high-DPI tests need so the apps do not each
//! re-derive it.

use image::RgbaImage;

/// A logical-pixel rectangle of one control: label, x, y, width, height.
pub type LogicalRect = (String, f32, f32, f32, f32);

/// Largest difference, in logical pixels, a control may move between factors.
pub const LAYOUT_TOLERANCE: f32 = 0.5;

/// Expected physical dimensions of a capture.
pub fn physical_size(width: f32, height: f32, factor: f32) -> (u32, u32) {
    (
        (width * factor).round() as u32,
        (height * factor).round() as u32,
    )
}

/// Describe the first control whose logical bounds differ between a capture at
/// 1.0 and a capture at another factor, or `None` when the layouts match.
pub fn layout_difference(base: &[LogicalRect], other: &[LogicalRect]) -> Option<String> {
    if base.len() != other.len() {
        return Some(format!(
            "control count changed: {} at 1.0 vs {} at the other factor",
            base.len(),
            other.len()
        ));
    }
    for (a, b) in base.iter().zip(other) {
        let moved = [(a.1, b.1), (a.2, b.2), (a.3, b.3), (a.4, b.4)]
            .iter()
            .any(|(x, y)| (x - y).abs() > LAYOUT_TOLERANCE);
        if a.0 != b.0 || moved {
            return Some(format!(
                "{:?} ({:.1}, {:.1}, {:.1}, {:.1}) vs {:?} ({:.1}, {:.1}, {:.1}, {:.1})",
                a.0, a.1, a.2, a.3, a.4, b.0, b.1, b.2, b.3, b.4
            ));
        }
    }
    None
}

/// Pixels that differ clearly from their right or lower neighbour: the edges
/// of glyphs, borders and icons. More of them means more drawn detail.
pub fn edge_pixels(image: &RgbaImage) -> usize {
    let (w, h) = image.dimensions();
    let luma = |x: u32, y: u32| {
        let p = image.get_pixel(x, y).0;
        (p[0] as i32 * 3 + p[1] as i32 * 6 + p[2] as i32) / 10
    };
    let mut count = 0;
    for y in 0..h.saturating_sub(1) {
        for x in 0..w.saturating_sub(1) {
            let here = luma(x, y);
            if (here - luma(x + 1, y)).abs() > 24 || (here - luma(x, y + 1)).abs() > 24 {
                count += 1;
            }
        }
    }
    count
}

/// A one-pixel line in the 1.0 capture: its position, colour and the share of
/// the row (or column) it covers.
struct Hairline {
    vertical: bool,
    at: u32,
    color: [u8; 4],
    share: f32,
}

fn line_share(image: &RgbaImage, vertical: bool, at: u32, color: [u8; 4]) -> f32 {
    let (w, h) = image.dimensions();
    let len = if vertical { h } else { w };
    let hits = (0..len)
        .filter(|i| {
            let (x, y) = if vertical { (at, *i) } else { (*i, at) };
            image.get_pixel(x, y).0 == color
        })
        .count();
    hits as f32 / len as f32
}

fn hairlines(image: &RgbaImage) -> Vec<Hairline> {
    let (w, h) = image.dimensions();
    let mut found = Vec::new();
    for vertical in [false, true] {
        let (lines, len) = if vertical { (w, h) } else { (h, w) };
        for at in 1..lines.saturating_sub(1) {
            let mut counts: std::collections::HashMap<[u8; 4], u32> = Default::default();
            for i in 0..len {
                let (x, y) = if vertical { (at, i) } else { (i, at) };
                *counts.entry(image.get_pixel(x, y).0).or_default() += 1;
            }
            let Some((color, n)) = counts.into_iter().max_by_key(|(_, n)| *n) else {
                continue;
            };
            let share = n as f32 / len as f32;
            // A hairline covers a good part of its row and is absent next door.
            if share >= 0.3
                && line_share(image, vertical, at - 1, color) < 0.05
                && line_share(image, vertical, at + 1, color) < 0.05
            {
                found.push(Hairline {
                    vertical,
                    at,
                    color,
                    share,
                });
            }
        }
    }
    found
}

/// The first one-pixel line of the 1.0 capture that has no counterpart near
/// its scaled position in the capture at `factor`.
pub fn vanished_hairline(base: &RgbaImage, scaled: &RgbaImage, factor: f32) -> Option<String> {
    for line in hairlines(base) {
        let (lo, hi) = (
            (line.at as f32 * factor).floor() as i64 - 1,
            ((line.at + 1) as f32 * factor).ceil() as i64 + 1,
        );
        let limit = if line.vertical {
            scaled.width()
        } else {
            scaled.height()
        } as i64;
        let seen = (lo.max(0)..hi.min(limit))
            .any(|at| line_share(scaled, line.vertical, at as u32, line.color) >= line.share * 0.5);
        if !seen {
            return Some(format!(
                "1px {} line at {} (colour {:?}, covers {:.0}%) has no counterpart at {factor}x",
                if line.vertical {
                    "vertical"
                } else {
                    "horizontal"
                },
                line.at,
                line.color,
                line.share * 100.0
            ));
        }
    }
    None
}

/// Number of distinct colours, capped: a blank or flat capture has very few.
pub fn distinct_colors(image: &RgbaImage) -> usize {
    let mut seen = std::collections::HashSet::new();
    for p in image.pixels() {
        seen.insert(p.0);
        if seen.len() >= 4096 {
            break;
        }
    }
    seen.len()
}

/// Pixel-level sanity for a capture at `factor` relative to the 1.0 capture of
/// the same surface. Returns a description of the first problem.
pub fn pixel_problem(
    base: &RgbaImage,
    scaled: &RgbaImage,
    logical: (f32, f32),
    factor: f32,
) -> Option<String> {
    let expected = physical_size(logical.0, logical.1, factor);
    if scaled.dimensions() != expected {
        return Some(format!(
            "image is {:?}, expected {:?} (logical {:?} x {factor})",
            scaled.dimensions(),
            expected,
            logical
        ));
    }
    if distinct_colors(scaled) < 8 {
        return Some("capture looks blank".to_string());
    }
    let (e1, es) = (edge_pixels(base) as f32, edge_pixels(scaled) as f32);
    if std::env::var_os("LOOM_DPI_DUMP").is_some() {
        eprintln!(
            "dpi {logical:?} x{factor}: edges {e1} -> {es} (ratio to proportional growth {:.2})",
            es / (e1 * factor)
        );
    }
    // Lines grow in length with the factor, so drawn edges must grow at least
    // roughly in proportion. Hairlines that vanish at a fractional factor, or
    // content drawn at 1x and upscaled (blurred, fewer sharp edges), fall short.
    if es < e1 * factor * 0.75 {
        return Some(format!(
            "edge detail {es} at {factor}x is below 75% of the 1.0 detail {e1} scaled by the factor"
        ));
    }
    if let Some(problem) = vanished_hairline(base, scaled, factor) {
        return Some(problem);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    fn page(w: u32, h: u32, line_at: Option<u32>) -> RgbaImage {
        let mut img = RgbaImage::from_pixel(w, h, Rgba([255, 255, 255, 255]));
        if let Some(y) = line_at {
            for x in 0..w {
                img.put_pixel(x, y, Rgba([200, 200, 210, 255]));
            }
        }
        img
    }

    #[test]
    fn a_hairline_that_vanishes_at_a_fractional_scale_is_reported() {
        let base = page(100, 40, Some(20));
        let kept = page(125, 50, Some(25));
        let lost = page(125, 50, None);
        assert!(vanished_hairline(&base, &kept, 1.25).is_none());
        assert!(vanished_hairline(&base, &lost, 1.25)
            .unwrap()
            .contains("horizontal line at 20"));
    }

    #[test]
    fn layouts_that_move_or_change_count_are_reported() {
        let a = vec![("Save".to_string(), 10.0, 5.0, 40.0, 20.0)];
        let mut b = a.clone();
        assert!(layout_difference(&a, &b).is_none());
        b[0].1 += 0.4;
        assert!(layout_difference(&a, &b).is_none());
        b[0].1 += 0.2;
        assert!(layout_difference(&a, &b).unwrap().contains("Save"));
        assert!(layout_difference(&a, &[]).unwrap().contains("count"));
    }

    #[test]
    fn size_and_blank_captures_are_reported() {
        let base = page(100, 40, Some(20));
        let wrong = page(100, 40, Some(20));
        assert!(pixel_problem(&base, &wrong, (100.0, 40.0), 2.0)
            .unwrap()
            .contains("expected"));
        let flat = RgbaImage::from_pixel(200, 80, Rgba([255, 255, 255, 255]));
        assert!(pixel_problem(&base, &flat, (100.0, 40.0), 2.0)
            .unwrap()
            .contains("blank"));
    }
}
