//! Line-by-line exclusion-rect text wrapping for floating/wrapped images.

use loom_objects::{
    HorizontalAnchor, ImageWrap, ImageWrapMode, VerticalAnchor, WrapHorizontalAlign, WrapMargins,
    WrapSide,
};

use crate::InlineImage;

/// A floating or text-wrapping image's exclusion rectangle in content-area coordinates.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ExclusionRect {
    pub x_pt: f32,
    pub y_pt: f32,
    pub width_pt: f32,
    pub height_pt: f32,
    pub mode: ImageWrapMode,
    pub side: WrapSide,
    pub margins: WrapMargins,
}

/// Result of testing a single text line against an exclusion rectangle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum LineExclusionResult {
    FullWidth,
    Span { x_offset_pt: f32, width_pt: f32 },
    PushBelow(f32),
}

impl ExclusionRect {
    /// Computes the horizontal span available for a line at `[line_y, line_y + line_height]`
    /// within a column of width `col_width`.
    pub(crate) fn available_span_for_line(
        &self,
        line_y: f32,
        line_height: f32,
        col_width: f32,
    ) -> LineExclusionResult {
        if !matches!(
            self.mode,
            ImageWrapMode::Square | ImageWrapMode::Tight | ImageWrapMode::TopAndBottom
        ) {
            return LineExclusionResult::FullWidth;
        }
        let top = self.y_pt - self.margins.top_pt;
        let bottom = self.y_pt + self.height_pt + self.margins.bottom_pt;
        if line_y + line_height <= top || line_y >= bottom {
            return LineExclusionResult::FullWidth;
        }
        if self.mode == ImageWrapMode::TopAndBottom {
            return LineExclusionResult::PushBelow(bottom);
        }
        let left = (self.x_pt - self.margins.left_pt).clamp(0.0, col_width);
        let right = (self.x_pt + self.width_pt + self.margins.right_pt).clamp(0.0, col_width);
        let left_avail = left.max(0.0);
        let right_avail = (col_width - right).max(0.0);

        let (x_offset_pt, width_pt) = match self.side {
            WrapSide::LeftOnly => (0.0, left_avail),
            WrapSide::RightOnly => (right, right_avail),
            WrapSide::Largest | WrapSide::Both => {
                if left_avail >= right_avail {
                    (0.0, left_avail)
                } else {
                    (right, right_avail)
                }
            }
        };
        LineExclusionResult::Span {
            x_offset_pt,
            width_pt,
        }
    }
}

/// Computes `(x_offset_pt, available_width_pt, push_below_y)` for a line against all active
/// exclusions on the current page.
pub(crate) fn compute_line_span(
    exclusions: &[ExclusionRect],
    line_y: f32,
    line_height: f32,
    col_width: f32,
) -> (f32, f32, Option<f32>) {
    let mut x_start = 0.0f32;
    let mut x_end = col_width;
    let mut push_below: Option<f32> = None;

    for excl in exclusions {
        match excl.available_span_for_line(line_y, line_height, col_width) {
            LineExclusionResult::FullWidth => {}
            LineExclusionResult::PushBelow(bottom) => {
                push_below = Some(push_below.map_or(bottom, |b| b.max(bottom)));
            }
            LineExclusionResult::Span {
                x_offset_pt,
                width_pt,
            } => {
                let span_end = x_offset_pt + width_pt;
                x_start = x_start.max(x_offset_pt);
                x_end = x_end.min(span_end);
            }
        }
    }

    let avail = (x_end - x_start).max(0.0);
    (x_start, avail, push_below)
}

/// Computes `(x_pt, y_pt, w_pt, h_pt)` in content-area coordinates for a floating image.
pub(crate) fn compute_floating_image_rect(
    image: &InlineImage,
    wrap: &ImageWrap,
    usable_width: f32,
    usable_height: f32,
    para_y: f32,
) -> (f32, f32, f32, f32) {
    let (w_pt, h_pt) = image.fitted_size(usable_width, usable_height);
    let x_pt = match wrap.horizontal_align {
        WrapHorizontalAlign::Left => 0.0,
        WrapHorizontalAlign::Center => ((usable_width - w_pt) * 0.5).max(0.0),
        WrapHorizontalAlign::Right => (usable_width - w_pt).max(0.0),
        WrapHorizontalAlign::OffsetPt(offset) => offset,
    };
    let y_base = match wrap.vertical_anchor {
        VerticalAnchor::Paragraph => para_y,
        VerticalAnchor::Margin | VerticalAnchor::Page => 0.0,
    };
    let _ = HorizontalAnchor::Column; // reserved for page/margin anchor differentiation
    let y_pt = (y_base + image.wrap_y_offset_pt).max(0.0);
    (x_pt, y_pt, w_pt, h_pt)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square_excl(x: f32, y: f32, w: f32, h: f32, side: WrapSide) -> ExclusionRect {
        ExclusionRect {
            x_pt: x,
            y_pt: y,
            width_pt: w,
            height_pt: h,
            mode: ImageWrapMode::Square,
            side,
            margins: WrapMargins::ZERO,
        }
    }

    #[test]
    fn square_wrap_picks_largest_and_respects_sides() {
        let excl = square_excl(100.0, 20.0, 100.0, 60.0, WrapSide::Largest);
        // Column width 400: left is 100, right is 200 -> picks right [200..400]
        let (x, w, push) = compute_line_span(std::slice::from_ref(&excl), 30.0, 14.0, 400.0);
        assert_eq!(x, 200.0);
        assert_eq!(w, 200.0);
        assert_eq!(push, None);

        let left_only = square_excl(100.0, 20.0, 100.0, 60.0, WrapSide::LeftOnly);
        let (x_l, w_l, _) = compute_line_span(&[left_only], 30.0, 14.0, 400.0);
        assert_eq!(x_l, 0.0);
        assert_eq!(w_l, 100.0);
    }

    #[test]
    fn top_and_bottom_pushes_below_image() {
        let excl = ExclusionRect {
            x_pt: 50.0,
            y_pt: 20.0,
            width_pt: 100.0,
            height_pt: 60.0,
            mode: ImageWrapMode::TopAndBottom,
            side: WrapSide::Both,
            margins: WrapMargins {
                top_pt: 6.0,
                bottom_pt: 6.0,
                left_pt: 9.0,
                right_pt: 9.0,
            },
        };
        let (_, _, push) = compute_line_span(&[excl], 30.0, 14.0, 400.0);
        assert_eq!(push, Some(86.0));
    }

    #[test]
    fn behind_and_in_front_do_not_exclude_text() {
        let excl = ExclusionRect {
            x_pt: 50.0,
            y_pt: 20.0,
            width_pt: 100.0,
            height_pt: 60.0,
            mode: ImageWrapMode::BehindText,
            side: WrapSide::Both,
            margins: WrapMargins::default(),
        };
        let (x, w, push) = compute_line_span(&[excl], 30.0, 14.0, 400.0);
        assert_eq!(x, 0.0);
        assert_eq!(w, 400.0);
        assert_eq!(push, None);
    }
}
