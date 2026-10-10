//! The result of shaping one run, and caret geometry derived from it.

use crate::work;
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
    /// that character's right edge. No stop falls inside a grapheme cluster
    /// even when the shaper split one across glyph clusters.
    ///
    /// Work is `O(n log n)` in the number of glyphs. A line layout computes
    /// this once per run and answers every caret query from the result.
    pub fn caret_stops(&self, text: &str) -> Vec<CaretStop> {
        work::add_build();
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
            work::add_steps(1);
            let right = x + glyph.advance;
            match groups.last_mut() {
                Some(group) if group.0 == glyph.cluster => group.2 = right,
                _ => groups.push((glyph.cluster, x, right)),
            }
            x = right;
        }
        let mut starts: Vec<usize> = groups.iter().map(|g| g.0).collect();
        starts.sort_unstable_by(|a, b| {
            work::add_steps(1);
            a.cmp(b)
        });
        starts.dedup();

        let mut stops = Vec::with_capacity(groups.len() + 1);
        for &(cluster, left, right) in &groups {
            if !text.is_char_boundary(cluster) {
                continue;
            }
            // The cluster ends where the next cluster (in logical order) starts.
            let next = work::lower_bound(&starts, |s| *s <= cluster);
            let end = starts.get(next).copied().unwrap_or(text.len());
            let slice = text.get(cluster..end).unwrap_or(&text[cluster..]);
            let graphemes: Vec<usize> = slice
                .grapheme_indices(true)
                .map(|(i, _)| cluster + i)
                .collect();
            let count = graphemes.len().max(1) as f32;
            let width = right - left;
            for (k, offset) in graphemes.into_iter().enumerate() {
                work::add_steps(1);
                let fraction = k as f32 / count;
                let x = if self.rtl {
                    right - width * fraction
                } else {
                    left + width * fraction
                };
                stops.push(CaretStop { offset, x });
            }
        }
        stops.sort_by_key(|s| {
            work::add_steps(1);
            s.offset
        });
        stops.dedup_by_key(|s| s.offset);

        // Drop stops the shaper placed inside a grapheme cluster of the text.
        let mut boundaries = text.grapheme_indices(true).map(|(i, _)| i).peekable();
        stops.retain(|stop| {
            work::add_steps(1);
            while boundaries.next_if(|b| *b < stop.offset).is_some() {}
            boundaries.peek() == Some(&stop.offset)
        });
        stops.push(CaretStop {
            offset: text.len(),
            x: trailing,
        });
        stops
    }

    /// The caret x for byte `offset`: the stop at that offset, or at the next
    /// boundary the glyphs can place. Recomputes the stops; use a
    /// [`crate::LineLayout`] to answer many queries.
    pub fn x_at_offset(&self, text: &str, offset: usize) -> f32 {
        let stops = self.caret_stops(text);
        let at = stops.partition_point(|s| s.offset < offset);
        stops.get(at).or(stops.last()).map_or(0.0, |s| s.x)
    }

    /// The byte offset of the boundary nearest to `x` (distance from the run's
    /// left edge). Clicks beyond either end map to the first or last boundary.
    /// Recomputes the stops, like [`Shaped::x_at_offset`].
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

#[cfg(test)]
mod tests {
    use super::*;

    fn glyph(cluster: usize, advance: f32) -> ShapedGlyph {
        ShapedGlyph {
            glyph_id: 1,
            cluster,
            advance,
            x_offset: 0.0,
            y_offset: 0.0,
        }
    }

    fn shaped(glyphs: Vec<ShapedGlyph>, rtl: bool) -> Shaped {
        let width = glyphs.iter().map(|g| g.advance).sum();
        Shaped { glyphs, width, rtl }
    }

    fn xs(stops: &[CaretStop]) -> Vec<(usize, f32)> {
        stops.iter().map(|s| (s.offset, s.x)).collect()
    }

    #[test]
    fn a_ligature_cluster_is_divided_between_its_graphemes() {
        // "fix": one glyph for "fi" (10 wide), one for "x" (6 wide).
        let run = shaped(vec![glyph(0, 10.0), glyph(2, 6.0)], false);
        assert_eq!(
            xs(&run.caret_stops("fix")),
            [(0, 0.0), (1, 5.0), (2, 10.0), (3, 16.0)]
        );
        // Three letters in one glyph divide into thirds.
        let triple = shaped(vec![glyph(0, 9.0)], false);
        let stops = triple.caret_stops("ffi");
        assert_eq!(xs(&stops), [(0, 0.0), (1, 3.0), (2, 6.0), (3, 9.0)]);
    }

    #[test]
    fn glyphs_sharing_a_cluster_are_one_group_spanning_all_their_advances() {
        // A decomposed ligature: two glyphs (4 and 6 wide) for "fi", then "x".
        let run = shaped(vec![glyph(0, 4.0), glyph(0, 6.0), glyph(2, 5.0)], false);
        assert_eq!(
            xs(&run.caret_stops("fix")),
            [(0, 0.0), (1, 5.0), (2, 10.0), (3, 15.0)]
        );
    }

    #[test]
    fn a_right_to_left_ligature_is_divided_from_its_right_edge() {
        // Two Hebrew letters (4 bytes) drawn as one 10-wide ligature glyph.
        let text = "\u{5dc}\u{5d0}";
        let run = shaped(vec![glyph(0, 10.0)], true);
        assert_eq!(xs(&run.caret_stops(text)), [(0, 10.0), (2, 5.0), (4, 0.0)]);
    }

    #[test]
    fn a_ligature_is_not_divided_at_a_combining_mark() {
        // "f" + "i" + U+0301: two graphemes ("f", "i\u{301}") in one glyph.
        let text = "fi\u{301}";
        let run = shaped(vec![glyph(0, 8.0)], false);
        let stops = run.caret_stops(text);
        assert_eq!(xs(&stops), [(0, 0.0), (1, 4.0), (4, 8.0)]);
    }

    #[test]
    fn stops_inside_a_grapheme_cluster_are_dropped() {
        // A shaper that split "e" and its accent into two clusters.
        let text = "e\u{301}x";
        let run = shaped(vec![glyph(0, 5.0), glyph(1, 0.0), glyph(3, 6.0)], false);
        let offsets: Vec<usize> = run.caret_stops(text).iter().map(|s| s.offset).collect();
        assert_eq!(offsets, [0, 3, 4]);
    }

    #[test]
    fn stop_lookup_rounds_inside_clusters_to_the_next_boundary() {
        let run = shaped(vec![glyph(0, 5.0), glyph(1, 0.0), glyph(3, 6.0)], false);
        let text = "e\u{301}x";
        assert_eq!(run.x_at_offset(text, 0), 0.0);
        assert_eq!(run.x_at_offset(text, 1), 5.0);
        assert_eq!(run.x_at_offset(text, 3), 5.0);
        assert_eq!(run.x_at_offset(text, 4), 11.0);
        assert_eq!(run.x_at_offset(text, 99), 11.0);
        assert_eq!(run.offset_at_x(text, 0.0), 0);
        assert_eq!(run.offset_at_x(text, 100.0), 4);
    }

    #[test]
    fn empty_input_has_a_single_end_stop() {
        let empty = Shaped::empty(false);
        assert_eq!(xs(&empty.caret_stops("")), [(0, 0.0)]);
        let rtl = Shaped::empty(true);
        assert_eq!(xs(&rtl.caret_stops("")), [(0, 0.0)]);
    }
}
