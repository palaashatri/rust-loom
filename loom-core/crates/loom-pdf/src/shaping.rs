//! Optional run shaping: how a drawn run is spaced when the caller installs a
//! [`RunShaper`].
//!
//! Without one, a run is set glyph after glyph at the font's own advances (a
//! plain `Tj`). An editor that draws with a shaper (Slint shapes with kerning)
//! and wraps lines with the shaped widths needs the PDF to place glyphs the
//! same way, or lines that fit on screen would be wider or narrower in the
//! file. A shaper therefore reports, per cluster of the run, the advance it
//! positions the cluster with, and the writer emits the difference to the
//! font's own advances as `TJ` adjustments. Nothing but the spacing changes:
//! the same glyphs are drawn, the `ToUnicode` map and `/ActualText` spans are
//! untouched, and text extraction returns the same characters.

use std::fmt::Debug;

/// One cluster of a run as a shaper positions it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClusterAdvance {
    /// Characters of the text handed to the shaper that this cluster covers.
    /// A grapheme (a base character and its marks) is one cluster.
    pub chars: usize,
    /// The cluster's advance in thousandths of an em with the shaper's
    /// kerning applied, or `None` to leave the cluster at the font's own
    /// advances (a character Inter does not have, say).
    pub advance: Option<f32>,
}

/// Positions the clusters of a run the way an editor's text engine does.
pub trait RunShaper: Debug + Send + Sync {
    /// Splits `text` (the characters that are set: invisible controls are
    /// already removed and the text is composed) into clusters, in order, and
    /// gives each one's advance in the Inter face `bold` and `italic` select.
    /// The cluster character counts must add up to `text.chars().count()`; a
    /// shaper that cannot honour a run returns `None`.
    fn clusters(&self, text: &str, bold: bool, italic: bool) -> Option<Vec<ClusterAdvance>>;
}

/// What the writer knows about one character it has set.
#[derive(Debug, Clone, Copy)]
pub(crate) struct CharSet {
    /// The advance the character's own glyph(s) take in thousandths of an em
    /// when it is set in an Inter face, or `None` for a fallback face or the
    /// `.notdef` box, whose spacing the shaper must not alter.
    pub nominal: Option<f32>,
    /// Number of glyphs shown once this character has been set, so the
    /// character's last glyph is index `glyph_end - 1`.
    pub glyph_end: usize,
}

/// `TJ` adjustments for a run: after glyph `index`, move the pen back by
/// `amount` thousandths of an em (negative moves it forward).
///
/// Each cluster's adjustment is the font's own advance minus the shaper's; it
/// is attached to the cluster's last glyph. Amounts are rounded to a tenth of
/// a thousandth of an em with the rounding error carried forward, so the sum
/// of a run's adjustments stays within half a tenth of the true total.
/// Returns `None` when the clusters do not describe the run.
pub(crate) fn adjustments(
    clusters: &[ClusterAdvance],
    set: &[CharSet],
) -> Option<Vec<(usize, f32)>> {
    if clusters.iter().map(|c| c.chars).sum::<usize>() != set.len() {
        return None;
    }
    let mut out = Vec::new();
    let mut carry = 0.0_f32;
    let mut at = 0usize;
    for cluster in clusters {
        let end = at + cluster.chars;
        let members = &set[at..end];
        at = end;
        let (Some(shaped), true) = (
            cluster.advance,
            !members.is_empty() && members.iter().all(|c| c.nominal.is_some()),
        ) else {
            continue;
        };
        let nominal: f32 = members.iter().filter_map(|c| c.nominal).sum();
        let wanted = nominal - shaped + carry;
        let applied = (wanted * 10.0).round() / 10.0;
        carry = wanted - applied;
        if applied != 0.0 {
            let last = members.last().map_or(0, |c| c.glyph_end);
            out.push((last.saturating_sub(1), applied));
        }
    }
    Some(out)
}

/// A content-stream text run being written: glyph strings, with a `TJ` array
/// only where a spacing adjustment was applied.
#[derive(Default)]
pub(crate) struct TextRun {
    out: String,
    items: Vec<String>,
    hex: String,
    adjusted: bool,
    /// The last array item is an adjustment, not a glyph string.
    tail_adjustment: bool,
}

impl TextRun {
    /// Appends raw operators; the pending glyphs must have been flushed.
    pub(crate) fn raw(&mut self, operators: &str) {
        self.out.push_str(operators);
    }

    pub(crate) fn glyph(&mut self, gid: u16) {
        use std::fmt::Write as _;
        let _ = write!(self.hex, "{gid:04X}");
        self.tail_adjustment = false;
    }

    /// Moves the pen back by `thousandths` of an em after the last glyph.
    pub(crate) fn adjust(&mut self, thousandths: f32) {
        if self.hex.is_empty() && self.items.is_empty() {
            return;
        }
        self.settle_hex();
        self.items.push(number(thousandths));
        self.adjusted = true;
        self.tail_adjustment = true;
    }

    fn settle_hex(&mut self) {
        if !self.hex.is_empty() {
            self.items
                .push(format!("<{}>", std::mem::take(&mut self.hex)));
        }
    }

    /// Writes the pending glyphs as `<hex> Tj` or, when any adjustment was
    /// applied, as a `[ ... ] TJ` array. `last` drops an adjustment that
    /// trails the whole text, which has nothing left to move.
    pub(crate) fn flush(&mut self, last: bool) {
        self.settle_hex();
        if last && self.tail_adjustment {
            self.items.pop();
            self.tail_adjustment = false;
            self.adjusted = self.items.len() > 1;
        }
        if self.items.is_empty() {
            return;
        }
        if self.adjusted {
            self.out.push_str("[ ");
            self.out.push_str(&self.items.join(" "));
            self.out.push_str(" ] TJ ");
        } else {
            self.out.push_str(&self.items.join(" "));
            self.out.push_str(" Tj ");
        }
        self.items.clear();
        self.adjusted = false;
        self.tail_adjustment = false;
    }

    pub(crate) fn finish(self) -> String {
        self.out
    }
}

/// A `TJ` number: one decimal, no trailing zero.
fn number(value: f32) -> String {
    let text = format!("{value:.1}");
    if let Some(whole) = text.strip_suffix(".0") {
        return whole.to_string();
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(nominal: &[Option<f32>]) -> Vec<CharSet> {
        nominal
            .iter()
            .enumerate()
            .map(|(i, nominal)| CharSet {
                nominal: *nominal,
                glyph_end: i + 1,
            })
            .collect()
    }

    fn cluster(chars: usize, advance: Option<f32>) -> ClusterAdvance {
        ClusterAdvance { chars, advance }
    }

    #[test]
    fn a_tighter_cluster_adjusts_after_its_last_glyph() {
        let set = set(&[Some(700.0), Some(650.0), Some(600.0)]);
        let clusters = [
            cluster(1, Some(640.0)),
            cluster(1, Some(650.0)),
            cluster(1, None),
        ];
        assert_eq!(adjustments(&clusters, &set), Some(vec![(0, 60.0)]));
    }

    #[test]
    fn a_cluster_with_a_fallback_character_is_left_alone() {
        let set = set(&[Some(700.0), None]);
        let clusters = [cluster(2, Some(100.0))];
        assert_eq!(adjustments(&clusters, &set), Some(Vec::new()));
    }

    #[test]
    fn clusters_that_do_not_cover_the_run_are_refused() {
        let set = set(&[Some(700.0), Some(650.0)]);
        assert_eq!(adjustments(&[cluster(1, Some(700.0))], &set), None);
    }

    #[test]
    fn rounding_error_is_carried_so_a_run_stays_on_its_true_total() {
        let set = set(&[Some(500.0); 40]);
        // 0.04 of a thousandth short of nominal for each: 1.6 in total.
        let clusters: Vec<_> = (0..40).map(|_| cluster(1, Some(499.96))).collect();
        let total: f32 = adjustments(&clusters, &set)
            .expect("adjustments")
            .iter()
            .map(|(_, amount)| amount)
            .sum();
        assert!((total - 1.6).abs() < 0.051, "{total}");
    }

    #[test]
    fn runs_without_adjustments_are_plain_tj_and_adjusted_runs_are_tj_arrays() {
        let mut plain = TextRun::default();
        plain.glyph(1);
        plain.glyph(0x2A);
        plain.flush(true);
        assert_eq!(plain.finish(), "<0001002A> Tj ");

        let mut kerned = TextRun::default();
        kerned.glyph(1);
        kerned.adjust(60.0);
        kerned.glyph(2);
        kerned.adjust(-3.5);
        kerned.flush(true);
        assert_eq!(kerned.finish(), "[ <0001> 60 <0002> ] TJ ");

        let mut mid = TextRun::default();
        mid.glyph(1);
        mid.adjust(12.5);
        mid.flush(false);
        mid.raw("/F2 12.00 Tf ");
        mid.glyph(3);
        mid.flush(true);
        assert_eq!(mid.finish(), "[ <0001> 12.5 ] TJ /F2 12.00 Tf <0003> Tj ");
    }
}
