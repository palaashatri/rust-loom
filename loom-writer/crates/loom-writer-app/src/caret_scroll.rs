//! Keeping the caret on screen.
//!
//! Typing, pasting, undo and keyboard navigation move the caret; the page has
//! to scroll with it, or a user working below the first screen cannot see what
//! they are doing.

/// Space kept between the caret and the top or bottom edge of the view.
const REVEAL_MARGIN: f32 = 48.0;

/// The page sits this far below the top of the scrolling area.
pub(crate) const PAGE_TOP_INSET: f32 = 24.0;

/// Scrolling up to within this distance of the top goes all the way to the top,
/// so reaching the first line shows the page's own top margin.
const SNAP_TO_TOP: f32 = 100.0;

/// The new scroll offset (pixels scrolled down from the top, never negative)
/// that shows a caret line occupying `top..bottom` pixels below the top of a
/// view `view_height` tall, or `scroll` unchanged if it is already visible with
/// a margin.
pub(crate) fn scroll_to_reveal(scroll: f32, top: f32, bottom: f32, view_height: f32) -> f32 {
    if view_height.is_nan() || view_height <= 0.0 || !top.is_finite() || !bottom.is_finite() {
        return scroll;
    }
    let margin = REVEAL_MARGIN.min(view_height / 4.0);
    if top < margin {
        let target = (scroll + top - margin).max(0.0);
        if target < SNAP_TO_TOP {
            0.0
        } else {
            target
        }
    } else if bottom > view_height - margin {
        scroll + (bottom - (view_height - margin))
    } else {
        scroll
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_visible_caret_does_not_scroll() {
        assert_eq!(scroll_to_reveal(200.0, 300.0, 316.0, 700.0), 200.0);
        assert_eq!(scroll_to_reveal(0.0, 60.0, 76.0, 700.0), 0.0);
    }

    #[test]
    fn a_caret_below_the_view_scrolls_it_into_view_with_a_margin() {
        // Caret line 900..916 px below the top of a 700 px view.
        let next = scroll_to_reveal(0.0, 900.0, 916.0, 700.0);
        assert!((next - (916.0 - (700.0 - 48.0))).abs() < 1e-3, "got {next}");
        // After scrolling, the line's bottom edge sits one margin above the bottom.
        let bottom_after = 916.0 - next;
        assert!((bottom_after - (700.0 - 48.0)).abs() < 1e-3);
    }

    #[test]
    fn a_caret_above_the_view_scrolls_up_but_never_past_the_top() {
        let up = scroll_to_reveal(500.0, -120.0, -104.0, 700.0);
        assert!((up - (500.0 - 120.0 - 48.0)).abs() < 1e-3, "got {up}");
        assert_eq!(scroll_to_reveal(30.0, -200.0, -184.0, 700.0), 0.0);
    }

    #[test]
    fn scrolling_up_to_near_the_top_snaps_to_the_top() {
        // The margin rule alone would leave a small scroll; show the top instead.
        assert_eq!(scroll_to_reveal(400.0, -310.0, -294.0, 700.0), 0.0);
        assert_eq!(scroll_to_reveal(150.0, -40.0, -24.0, 700.0), 0.0);
        // Further down than the snap distance, the margin rule applies as usual.
        assert!((scroll_to_reveal(400.0, -40.0, -24.0, 700.0) - 312.0).abs() < 1e-3);
    }

    #[test]
    fn degenerate_inputs_leave_the_scroll_alone() {
        assert_eq!(scroll_to_reveal(120.0, 10.0, 20.0, 0.0), 120.0);
        assert_eq!(scroll_to_reveal(120.0, f32::NAN, 20.0, 600.0), 120.0);
        assert_eq!(scroll_to_reveal(120.0, 10.0, f32::INFINITY, 600.0), 120.0);
        // A tiny view shrinks the margin instead of fighting itself.
        let tiny = scroll_to_reveal(0.0, 100.0, 116.0, 80.0);
        assert!(tiny > 0.0 && tiny.is_finite());
    }
}
