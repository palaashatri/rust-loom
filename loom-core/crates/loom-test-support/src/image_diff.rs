//! Perceptual image diffing with documented tolerances.
//!
//! Two images are compared pixel by pixel. The report carries three numbers:
//!
//! * `mean_abs_error` — mean of the per-pixel absolute channel differences
//!   (0..=255 scale).
//! * `max_abs_error` — worst single channel difference.
//! * `differing_ratio` — fraction of pixels whose channel differences exceed
//!   `epsilon` (default 2.0).
//!
//! The Loom visual-QA tolerance (see `loom-design-bible/VISUAL_QA.md`) is
//! `mean_abs_error < 1.0` and `differing_ratio < 0.01`. Baselines are never
//! auto-approved; a mismatch always requires a human decision.

use image::RgbaImage;

/// Difference of two pixels, used to compare channel pairs.
pub const DEFAULT_EPSILON: f32 = 2.0;

/// Result of comparing two images.
#[derive(Debug, Clone, PartialEq)]
pub struct DiffReport {
    /// Mean of the absolute channel differences (0..=255).
    pub mean_abs_error: f32,
    /// Largest absolute channel difference (0..=255).
    pub max_abs_error: f32,
    /// Fraction of pixels (0..=1) with any channel difference > `epsilon`.
    pub differing_ratio: f32,
    /// Whether the two images have different dimensions.
    pub size_mismatch: bool,
}

/// Compare `actual` with `baseline` and produce a [`DiffReport`].
///
/// Returns a report with `size_mismatch == true` and a maximal diff when the
/// dimensions differ; no panic.
pub fn perceptual_diff(actual: &RgbaImage, baseline: &RgbaImage) -> DiffReport {
    let (aw, ah) = actual.dimensions();
    let (bw, bh) = baseline.dimensions();
    if (aw, ah) != (bw, bh) {
        return DiffReport {
            mean_abs_error: f32::MAX,
            max_abs_error: 255.0,
            differing_ratio: 1.0,
            size_mismatch: true,
        };
    }
    let n = (aw * ah) as usize;
    let a = actual.as_raw();
    let b = baseline.as_raw();
    let mut sum: f64 = 0.0;
    let mut max_err: f32 = 0.0;
    let mut differing: usize = 0;
    for (pa, pb) in a.chunks_exact(4).zip(b.chunks_exact(4)) {
        for c in 0..3 {
            let da = (pa[c] as i32 - pb[c] as i32).abs() as f32;
            sum += da as f64;
            max_err = max_err.max(da);
            if da > DEFAULT_EPSILON {
                differing += 1;
            }
        }
    }
    DiffReport {
        mean_abs_error: (sum / (n as f64 * 3.0)) as f32,
        max_abs_error: max_err,
        differing_ratio: differing as f32 / (n as f32 * 3.0),
        size_mismatch: false,
    }
}

/// True when the report is within the Loom visual-QA tolerances.
pub fn within_tolerance(report: &DiffReport, max_mean: f32, max_ratio: f32) -> bool {
    !report.size_mismatch && report.mean_abs_error < max_mean && report.differing_ratio < max_ratio
}

/// Produce a visual diff image: baseline in gray where equal, actual color
/// where different (with a red highlight overlay). Used for failure artifacts.
pub fn highlight_diff(actual: &RgbaImage, baseline: &RgbaImage) -> RgbaImage {
    let (aw, ah) = actual.dimensions();
    let (bw, bh) = baseline.dimensions();
    if (aw, ah) != (bw, bh) {
        return actual.clone();
    }
    let mut out = RgbaImage::new(aw, ah);
    let a = actual.as_raw();
    let b = baseline.as_raw();
    for (i, (pa, pb)) in a.chunks_exact(4).zip(b.chunks_exact(4)).enumerate() {
        let diff = (0..3).any(|c| (pa[c] as i32 - pb[c] as i32).abs() > DEFAULT_EPSILON as i32);
        if diff {
            let x = i as u32 % aw;
            let y = i as u32 / aw;
            out.put_pixel(x, y, image::Rgba([255, 40, 40, 255]));
        } else {
            let g = ((pa[0] as u32 + pa[1] as u32 + pa[2] as u32) / 3) as u8;
            out.put_pixel(i as u32 % aw, i as u32 / aw, image::Rgba([g, g, g, 255]));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn img(data: &[u8]) -> RgbaImage {
        RgbaImage::from_raw(2, 2, data.to_vec()).unwrap()
    }

    #[test]
    fn identical_images_diff_zero() {
        let a = img(&[
            10, 20, 30, 255, 40, 50, 60, 255, 70, 80, 90, 255, 100, 110, 120, 255,
        ]);
        let r = perceptual_diff(&a, &a);
        assert_eq!(r.mean_abs_error, 0.0);
        assert_eq!(r.max_abs_error, 0.0);
        assert_eq!(r.differing_ratio, 0.0);
        assert!(!r.size_mismatch);
        assert!(within_tolerance(&r, 1.0, 0.01));
    }

    #[test]
    fn single_channel_shift_is_measured() {
        let a = img(&[
            10, 20, 30, 255, 40, 50, 60, 255, 70, 80, 90, 255, 100, 110, 120, 255,
        ]);
        let mut b = a.clone();
        b.get_pixel_mut(0, 0).0[0] = 15; // one channel of one pixel: +5
        let r = perceptual_diff(&a, &b);
        assert_eq!(r.max_abs_error, 5.0);
        assert!((r.mean_abs_error - 5.0 / 12.0).abs() < 1e-5);
        assert!((r.differing_ratio - 1.0 / 12.0).abs() < 1e-5);
        assert!(!within_tolerance(&r, 1.0, 0.01));
    }

    #[test]
    fn size_mismatch_is_reported() {
        let a = img(&[0; 16]);
        let b = RgbaImage::new(3, 1);
        let r = perceptual_diff(&a, &b);
        assert!(r.size_mismatch);
        assert!(!within_tolerance(&r, 1.0, 0.01));
    }

    #[test]
    fn highlight_marks_only_changed_pixels() {
        let a = img(&[
            10, 20, 30, 255, 40, 50, 60, 255, 70, 80, 90, 255, 100, 110, 120, 255,
        ]);
        let mut b = a.clone();
        b.get_pixel_mut(0, 0).0[0] = 15;
        let d = highlight_diff(&a, &b);
        assert_eq!(d.get_pixel(0, 0), &image::Rgba([255, 40, 40, 255]));
        assert_ne!(d.get_pixel(1, 0), &image::Rgba([255, 40, 40, 255]));
    }
}
