//! Right-to-left checks: the control layout of a window in RTL must be the
//! mirror image of the same window in LTR.
//!
//! Positions are logical pixels from the accessibility element tree, the same
//! rectangles `dpi` uses: label, x, y, width, height.

use crate::dpi::LogicalRect;

/// Largest difference, in logical pixels, between a mirrored position and the
/// position actually laid out.
pub const MIRROR_TOLERANCE: f32 = 1.5;

/// Describe every control whose RTL position is not the horizontal mirror of
/// its LTR position in a window `width` wide, or `None` when they all match.
/// `exempt` lists label prefixes that deliberately do not mirror (document
/// content such as a page of text or a grid column order).
pub fn mirror_difference(
    ltr: &[LogicalRect],
    rtl: &[LogicalRect],
    width: f32,
    exempt: &[&str],
) -> Option<String> {
    let skip = |label: &str| exempt.iter().any(|prefix| label.starts_with(prefix));
    let ltr: Vec<_> = ltr.iter().filter(|r| !skip(&r.0)).collect();
    let rtl: Vec<_> = rtl.iter().filter(|r| !skip(&r.0)).collect();
    let mut problems = Vec::new();
    if ltr.len() != rtl.len() {
        problems.push(format!(
            "control count differs: {} in LTR, {} in RTL",
            ltr.len(),
            rtl.len()
        ));
    }
    let mut used = vec![false; rtl.len()];
    for a in &ltr {
        let want_x = width - a.1 - a.3;
        let found = rtl.iter().enumerate().position(|(i, b)| {
            !used[i]
                && b.0 == a.0
                && (b.1 - want_x).abs() <= MIRROR_TOLERANCE
                && (b.2 - a.2).abs() <= MIRROR_TOLERANCE
                && (b.3 - a.3).abs() <= MIRROR_TOLERANCE
                && (b.4 - a.4).abs() <= MIRROR_TOLERANCE
        });
        match found {
            Some(i) => used[i] = true,
            None => {
                let near = rtl
                    .iter()
                    .find(|b| b.0 == a.0 && (b.2 - a.2).abs() <= MIRROR_TOLERANCE)
                    .map(|b| format!("{:.0}", b.1))
                    .unwrap_or_else(|| "absent".into());
                problems.push(format!(
                    "{:?} at x={:.0} w={:.0} y={:.0} should be at x={:.0} in RTL, found {near}",
                    a.0, a.1, a.3, a.2, want_x
                ));
            }
        }
    }
    (!problems.is_empty()).then(|| problems.join("\n  "))
}

/// Every control lies fully inside the window and none overlap partially. The
/// same check the text-scale tests use, kept here so RTL tests can state it.
pub fn inside_window(items: &[LogicalRect], width: f32, height: f32) -> Option<String> {
    items
        .iter()
        .find(|(_, x, y, w, h)| {
            *x < -1.0 || *y < -1.0 || x + w > width + 1.0 || y + h > height + 1.0
        })
        .map(|(label, x, y, w, h)| {
            format!("{label:?} leaves the {width}x{height} window: ({x}, {y}, {w}, {h})")
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(label: &str, x: f32, w: f32) -> LogicalRect {
        (label.to_string(), x, 10.0, w, 20.0)
    }

    #[test]
    fn a_true_mirror_matches_and_a_copy_does_not() {
        let ltr = vec![r("File", 10.0, 60.0), r("Close", 900.0, 40.0)];
        let mirrored = vec![r("File", 930.0, 60.0), r("Close", 60.0, 40.0)];
        assert!(mirror_difference(&ltr, &mirrored, 1000.0, &[]).is_none());
        let unchanged = ltr.clone();
        let problem = mirror_difference(&ltr, &unchanged, 1000.0, &[]).unwrap();
        assert!(problem.contains("\"File\""), "{problem}");
    }

    #[test]
    fn exempt_labels_are_ignored() {
        let ltr = vec![r("Document body", 100.0, 300.0)];
        assert!(mirror_difference(&ltr, &ltr, 1000.0, &["Document"]).is_none());
    }
}
