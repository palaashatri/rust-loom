//! Image text-wrapping and anchoring primitives shared across Loom Office.

use serde::{Deserialize, Serialize};

/// How an image interacts with surrounding text flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageWrapMode {
    /// Image flows inline with text characters.
    #[default]
    Inline,
    /// Text wraps around the image's rectangular bounding box.
    Square,
    /// Text wraps tightly around the image's contour.
    Tight,
    /// Text appears above and below the image, but not on the sides.
    TopAndBottom,
    /// Image floats behind text.
    BehindText,
    /// Image floats in front of text.
    InFrontOfText,
}

/// Which sides of a wrapping image text is allowed to flow on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WrapSide {
    #[default]
    Both,
    LeftOnly,
    RightOnly,
    Largest,
}

/// Horizontal reference frame for floating image positioning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HorizontalAnchor {
    #[default]
    Column,
    Margin,
    Page,
}

/// Vertical reference frame for floating image positioning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerticalAnchor {
    #[default]
    Paragraph,
    Margin,
    Page,
}

/// Horizontal alignment or explicit offset for a floating image.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WrapHorizontalAlign {
    #[default]
    Left,
    Center,
    Right,
    OffsetPt(f32),
}

/// Outer spacing (in points) between a wrapping image and surrounding text.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WrapMargins {
    pub top_pt: f32,
    pub bottom_pt: f32,
    pub left_pt: f32,
    pub right_pt: f32,
}

impl Default for WrapMargins {
    fn default() -> Self {
        Self {
            top_pt: 6.0,
            bottom_pt: 6.0,
            left_pt: 9.0,
            right_pt: 9.0,
        }
    }
}

impl WrapMargins {
    /// Zero margins on all sides.
    pub const ZERO: Self = Self {
        top_pt: 0.0,
        bottom_pt: 0.0,
        left_pt: 0.0,
        right_pt: 0.0,
    };
}

/// Complete text-wrapping and anchoring configuration for an image.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ImageWrap {
    pub mode: ImageWrapMode,
    pub side: WrapSide,
    pub horizontal_anchor: HorizontalAnchor,
    pub vertical_anchor: VerticalAnchor,
    pub horizontal_align: WrapHorizontalAlign,
    pub margins: WrapMargins,
}

impl ImageWrap {
    /// Returns `true` if the image is positioned outside the inline text run.
    pub fn is_floating(&self) -> bool {
        self.mode != ImageWrapMode::Inline
    }

    /// Returns `true` if surrounding text wraps around or above/below the image.
    pub fn is_text_wrapping(&self) -> bool {
        matches!(
            self.mode,
            ImageWrapMode::Square | ImageWrapMode::Tight | ImageWrapMode::TopAndBottom
        )
    }

    /// Returns the margins that apply to text layout, or zero when not wrapping.
    pub fn effective_margins(&self) -> WrapMargins {
        if !self.is_text_wrapping() {
            return WrapMargins::ZERO;
        }
        WrapMargins {
            top_pt: self.margins.top_pt.max(0.0),
            bottom_pt: self.margins.bottom_pt.max(0.0),
            left_pt: self.margins.left_pt.max(0.0),
            right_pt: self.margins.right_pt.max(0.0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_spec() {
        let wrap = ImageWrap::default();
        assert_eq!(wrap.mode, ImageWrapMode::Inline);
        assert_eq!(wrap.side, WrapSide::Both);
        assert_eq!(wrap.horizontal_anchor, HorizontalAnchor::Column);
        assert_eq!(wrap.vertical_anchor, VerticalAnchor::Paragraph);
        assert_eq!(wrap.horizontal_align, WrapHorizontalAlign::Left);
        assert_eq!(wrap.margins.top_pt, 6.0);
        assert_eq!(wrap.margins.bottom_pt, 6.0);
        assert_eq!(wrap.margins.left_pt, 9.0);
        assert_eq!(wrap.margins.right_pt, 9.0);
        assert!(!wrap.is_floating());
        assert!(!wrap.is_text_wrapping());
        assert_eq!(wrap.effective_margins(), WrapMargins::ZERO);
    }

    #[test]
    fn floating_and_wrapping_predicates() {
        let mut wrap = ImageWrap {
            mode: ImageWrapMode::Square,
            ..Default::default()
        };
        assert!(wrap.is_floating());
        assert!(wrap.is_text_wrapping());
        assert_eq!(wrap.effective_margins(), WrapMargins::default());

        wrap.mode = ImageWrapMode::Tight;
        assert!(wrap.is_floating());
        assert!(wrap.is_text_wrapping());

        wrap.mode = ImageWrapMode::TopAndBottom;
        assert!(wrap.is_floating());
        assert!(wrap.is_text_wrapping());

        wrap.mode = ImageWrapMode::BehindText;
        assert!(wrap.is_floating());
        assert!(!wrap.is_text_wrapping());
        assert_eq!(wrap.effective_margins(), WrapMargins::ZERO);

        wrap.mode = ImageWrapMode::InFrontOfText;
        assert!(wrap.is_floating());
        assert!(!wrap.is_text_wrapping());
        assert_eq!(wrap.effective_margins(), WrapMargins::ZERO);
    }

    #[test]
    fn effective_margins_clamps_negative_values() {
        let wrap = ImageWrap {
            mode: ImageWrapMode::Square,
            margins: WrapMargins {
                top_pt: -4.0,
                bottom_pt: 8.0,
                left_pt: -1.5,
                right_pt: 12.0,
            },
            ..Default::default()
        };
        assert_eq!(
            wrap.effective_margins(),
            WrapMargins {
                top_pt: 0.0,
                bottom_pt: 8.0,
                left_pt: 0.0,
                right_pt: 12.0,
            }
        );
    }

    #[test]
    fn serde_round_trip_and_partial_defaults() {
        let wrap = ImageWrap {
            mode: ImageWrapMode::Tight,
            side: WrapSide::Largest,
            horizontal_anchor: HorizontalAnchor::Page,
            vertical_anchor: VerticalAnchor::Margin,
            horizontal_align: WrapHorizontalAlign::OffsetPt(42.5),
            margins: WrapMargins {
                top_pt: 4.0,
                bottom_pt: 4.0,
                left_pt: 10.0,
                right_pt: 10.0,
            },
        };
        let json = serde_json::to_string(&wrap).unwrap();
        let decoded: ImageWrap = serde_json::from_str(&json).unwrap();
        assert_eq!(wrap, decoded);

        let from_empty: ImageWrap = serde_json::from_str("{}").unwrap();
        assert_eq!(from_empty, ImageWrap::default());
    }
}
