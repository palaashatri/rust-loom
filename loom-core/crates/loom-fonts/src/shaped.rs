//! The result of shaping one run, and caret geometry derived from it.

use unicode_segmentation::UnicodeSegmentation;

/// One positioned glyph. Distances are in the unit of the requested size
/// (points when the size is in points).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShapedGlyph {
    /// Glyph index in the face (what a PDF writer needs).
    pub glyph_id: u16,
    /// Byte offset in the shaped text of the first character this glyph
    /// represents. Ligatures and combining sequences share one cluster.
    pub cluster: usize,
    /// Distance the pen moves after this glyph, kerning included.
    pub advance: f32,
    /// Horizontal drawing offset that does not move the pen.
    pub x_offset: f32,
    /// Vertical drawing offset that does not move the pen.
    pub y_offset: f32,
}

/// A caret position: the horizontal coordinate of the boundary before the
/// character at byte `offset` (and after the last one at `text.len()`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CaretStop {
    /// Byte offset of a grapheme boundary in the shaped text.
    pub offset: usize,
    /// Distance from the left edge of the run as drawn.
    pub x: f32,
}

/// A shaped run: glyphs in visual (left to right) order.
#[derive(Clone, Debug, PartialEq)]
pub struct Shaped {
    /// Glyphs, left to right on screen. For right-to-left text this is the
    /// reverse of logical order.
    pub glyphs: Vec<ShapedGlyph>,
    /// Sum of advances: the run's width.
    pub width: f32,
    /// True when the run was shaped right to left.
    pub rtl: bool,
}

impl Shaped {
    pub(crate) fn empty(rtl: bool) -> Self {
        Self {
            glyphs: Vec::new(),
            width: 0.0,
            rtl,
        }
    }

    /// Caret stops at every grapheme boundary of `text` that the glyph
    /// clusters can place, ascending by byte offset.
    ///
    /// A cluster that holds several graphemes (an `fi` ligature) is divided
    /// evenly between them, which is what editors do for ligature carets. For
    /// a right-to-left run the stop for the boundary *before* a character is
    /// that character's right edge.
    pub fn caret_stops(&self, text: &str) -> Vec<CaretStop> {
        let trailing = if self.rtl { 0.0 } else { self.width };
        if text.is_empty() || self.glyphs.is_empty() {
            return vec![CaretStop {
                offset: text.len(),
                x: trailing,
            }];
        }
        // Visual groups of glyphs that share a cluster.
        let mut groups: Vec<(usize, f32, f32)> = Vec::new();
        let mut x = 0.0_f32;
        for glyph in &self.glyphs {
            let right = x + glyph.advance;
            match groups.last_mut() {
                Some(group) if group.0 == glyph.cluster => group.2 = right,
                _ => groups.push((glyph.cluster, x, right)),
            }
            x = right;
        }
        let mut starts: Vec<usize> = groups.iter().map(|g| g.0).collect();
        starts.sort_unstable();
        starts.dedup();

        let mut stops = Vec::with_capacity(groups.len() + 1);
        for &(cluster, left, right) in &groups {
            if !text.is_char_boundary(cluster) {
                continue;
            }
            let end = starts
                .iter()
                .copied()
                .find(|s| *s > cluster)
                .unwrap_or(text.len());
            let graphemes: Vec<usize> = text[cluster..end.max(cluster)]
                .grapheme_indices(true)
                .map(|(i, _)| cluster + i)
                .collect();
            let count = graphemes.len().max(1) as f32;
            let width = right - left;
            for (k, offset) in graphemes.into_iter().enumerate() {
                let fraction = k as f32 / count;
                let x = if self.rtl {
                    right - width * fraction
                } else {
                    left + width * fraction
                };
                stops.push(CaretStop { offset, x });
            }
        }
        stops.sort_by_key(|s| s.offset);
        stops.dedup_by_key(|s| s.offset);
        stops.push(CaretStop {
            offset: text.len(),
            x: trailing,
        });
        stops
    }

    /// The caret x for byte `offset`: the stop at that offset, or at the next
    /// boundary the glyphs can place.
    pub fn x_at_offset(&self, text: &str, offset: usize) -> f32 {
        let stops = self.caret_stops(text);
        stops
            .iter()
            .find(|s| s.offset >= offset)
            .or(stops.last())
            .map_or(0.0, |s| s.x)
    }

    /// The byte offset of the boundary nearest to `x` (distance from the run's
    /// left edge). Clicks beyond either end map to the first or last boundary.
    pub fn offset_at_x(&self, text: &str, x: f32) -> usize {
        let stops = self.caret_stops(text);
        let mut best = stops[0];
        for stop in &stops[1..] {
            if (stop.x - x).abs() < (best.x - x).abs() {
                best = *stop;
            }
        }
        best.offset
    }
}
