//! Golden-image snapshot harness.
//!
//! Baselines are committed PNGs next to the test that uses them. On
//! mismatch, the actual image, the diff image and a JSON metadata file are
//! written into the same directory (never into the baseline itself) and the
//! test fails. Baselines are only (re)created when the `LOOM_SNAPSHOT_UPDATE`
//! environment variable is set to `1` — never automatically.

use std::path::{Path, PathBuf};

use image::RgbaImage;

use crate::image_diff::{highlight_diff, perceptual_diff, within_tolerance, DiffReport};
use crate::png::save_png;

/// The Loom default visual-QA tolerance (design bible VISUAL_QA.md).
pub const DEFAULT_MAX_MEAN: f32 = 1.0;
/// The Loom default maximum differing-pixel ratio.
pub const DEFAULT_MAX_RATIO: f32 = 0.01;

/// Tolerance parameters for a snapshot comparison.
#[derive(Debug, Clone, Copy)]
pub struct Tolerance {
    /// Maximum allowed mean absolute channel error (0..=255).
    pub max_mean: f32,
    /// Maximum allowed differing-pixel ratio (0..=1).
    pub max_ratio: f32,
}

impl Default for Tolerance {
    fn default() -> Self {
        Self {
            max_mean: DEFAULT_MAX_MEAN,
            max_ratio: DEFAULT_MAX_RATIO,
        }
    }
}

/// Compare `actual` against the baseline PNG at `baseline_path`.
///
/// If the baseline does not exist:
/// * with `LOOM_SNAPSHOT_UPDATE=1` it is created and the test passes
///   (an explicit human decision to seed the baseline);
/// * otherwise the test fails with a clear message.
///
/// On mismatch (outside tolerance) the actual and diff PNGs are written next
/// to the baseline and the test fails.
pub fn assert_matches_baseline(
    actual: &RgbaImage,
    baseline_path: &Path,
    tolerance: Tolerance,
) -> Result<(), SnapshotError> {
    if baseline_path.exists() {
        let baseline = crate::png::load_png(baseline_path)?;
        let report = perceptual_diff(actual, &baseline);
        if within_tolerance(&report, tolerance.max_mean, tolerance.max_ratio) {
            return Ok(());
        }
        if snapshot_update_enabled() {
            save_png(baseline_path, actual)?;
            return Ok(());
        }
        let diff = highlight_diff(actual, &baseline);
        let artifacts = failure_artifacts(baseline_path, actual, &diff, &report);
        return Err(SnapshotError::Mismatch {
            baseline: baseline_path.to_path_buf(),
            artifacts,
            report,
        });
    }
    if snapshot_update_enabled() {
        save_png(baseline_path, actual)?;
        Ok(())
    } else {
        Err(SnapshotError::MissingBaseline(baseline_path.to_path_buf()))
    }
}

/// Whether snapshot baselines may be (re)written.
pub fn snapshot_update_enabled() -> bool {
    std::env::var("LOOM_SNAPSHOT_UPDATE").as_deref() == Ok("1")
}

/// Paths written next to a baseline on failure.
#[derive(Debug)]
pub struct FailureArtifacts {
    /// The rendered image that failed comparison.
    pub actual: PathBuf,
    /// Red-highlighted diff image.
    pub diff: PathBuf,
    /// JSON metadata (report, commit, fonts, renderer).
    pub metadata: PathBuf,
}

fn failure_artifacts(
    baseline: &Path,
    actual: &RgbaImage,
    diff: &RgbaImage,
    report: &DiffReport,
) -> FailureArtifacts {
    let dir = baseline.parent().unwrap_or_else(|| Path::new("."));
    let stem = baseline
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("snapshot");
    let actual_path = dir.join(format!("{stem}.actual.png"));
    let diff_path = dir.join(format!("{stem}.diff.png"));
    let meta_path = dir.join(format!("{stem}.actual.json"));
    save_png(&actual_path, actual).ok();
    save_png(&diff_path, diff).ok();
    let meta = serde_json::json!({
        "baseline": baseline.to_string_lossy(),
        "report": {
            "mean_abs_error": report.mean_abs_error,
            "max_abs_error": report.max_abs_error,
            "differing_ratio": report.differing_ratio,
            "size_mismatch": report.size_mismatch,
        },
        "renderer": "software",
        "timestamp": chrono_now_iso8601(),
    });
    let _ = std::fs::write(
        &meta_path,
        serde_json::to_string_pretty(&meta).unwrap_or_default(),
    );
    FailureArtifacts {
        actual: actual_path,
        diff: diff_path,
        metadata: meta_path,
    }
}

/// Errors produced by the snapshot harness.
#[derive(Debug)]
pub enum SnapshotError {
    /// The baseline file does not exist and `LOOM_SNAPSHOT_UPDATE` is not set.
    MissingBaseline(PathBuf),
    /// The rendered image differs beyond tolerance.
    Mismatch {
        /// Path of the baseline that was compared.
        baseline: PathBuf,
        /// Artifacts written for inspection.
        artifacts: FailureArtifacts,
        /// The measured difference.
        report: DiffReport,
    },
    /// Baseline or artifact I/O failed.
    Io(std::io::Error),
}

impl std::fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SnapshotError::MissingBaseline(p) => write!(
                f,
                "missing baseline {} (set LOOM_SNAPSHOT_UPDATE=1 to create it after review)",
                p.display()
            ),
            SnapshotError::Mismatch {
                baseline,
                artifacts,
                report,
            } => write!(
                f,
                "snapshot mismatch vs {}: mean={:.4} (limit {:.2}), ratio={:.6} (limit {:.3}); \
                 actual: {}, diff: {}, metadata: {}",
                baseline.display(),
                report.mean_abs_error,
                DEFAULT_MAX_MEAN,
                report.differing_ratio,
                DEFAULT_MAX_RATIO,
                artifacts.actual.display(),
                artifacts.diff.display(),
                artifacts.metadata.display(),
            ),
            SnapshotError::Io(e) => write!(f, "snapshot I/O error: {e}"),
        }
    }
}

impl std::error::Error for SnapshotError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SnapshotError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<Box<dyn std::error::Error>> for SnapshotError {
    fn from(e: Box<dyn std::error::Error>) -> Self {
        SnapshotError::Io(std::io::Error::other(e.to_string()))
    }
}

fn chrono_now_iso8601() -> String {
    // Avoid a time dependency: seconds since epoch, ISO-like for humans.
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{secs}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Serializes tests that mutate the process-global LOOM_SNAPSHOT_UPDATE var.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("loom-snapshot-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn missing_baseline_fails_without_update_flag() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::remove_var("LOOM_SNAPSHOT_UPDATE");
        let dir = temp_dir("missing");
        let img = RgbaImage::new(8, 8);
        let err = assert_matches_baseline(&img, &dir.join("a.png"), Tolerance::default());
        assert!(matches!(err, Err(SnapshotError::MissingBaseline(_))));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn update_flag_creates_baseline() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::set_var("LOOM_SNAPSHOT_UPDATE", "1");
        let dir = temp_dir("create");
        let img = RgbaImage::new(8, 8);
        let path = dir.join("a.png");
        assert!(assert_matches_baseline(&img, &path, Tolerance::default()).is_ok());
        assert!(path.exists());
        std::env::remove_var("LOOM_SNAPSHOT_UPDATE");
        // Without the flag, matching baseline passes.
        assert!(assert_matches_baseline(&img, &path, Tolerance::default()).is_ok());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn mismatch_writes_artifacts() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::remove_var("LOOM_SNAPSHOT_UPDATE");
        let dir = temp_dir("mismatch");
        let baseline_path = dir.join("b.png");
        crate::png::save_png(&baseline_path, &RgbaImage::new(8, 8)).unwrap();
        let mut actual = RgbaImage::new(8, 8);
        actual.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
        let err = assert_matches_baseline(&actual, &baseline_path, Tolerance::default());
        match err {
            Err(SnapshotError::Mismatch {
                artifacts, report, ..
            }) => {
                assert!(artifacts.actual.exists());
                assert!(artifacts.diff.exists());
                assert!(artifacts.metadata.exists());
                assert!(report.max_abs_error > 0.0);
            }
            other => panic!("expected Mismatch, got {other:?}"),
        }
        std::fs::remove_dir_all(&dir).ok();
    }
}
