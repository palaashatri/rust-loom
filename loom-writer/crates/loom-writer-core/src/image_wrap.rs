//! Image text-wrapping and anchoring mutations on [`WriterDocument`].

use crate::WriterDocument;
use loom_objects::{
    HorizontalAnchor, ImageWrap, ImageWrapMode, VerticalAnchor, WrapHorizontalAlign, WrapMargins,
    WrapSide,
};

pub(crate) fn parse_wrap_mode_str(s: &str) -> Option<ImageWrap> {
    let mode = match s {
        "square" => ImageWrapMode::Square,
        "tight" => ImageWrapMode::Tight,
        "top_and_bottom" => ImageWrapMode::TopAndBottom,
        "behind_text" => ImageWrapMode::BehindText,
        "in_front_of_text" => ImageWrapMode::InFrontOfText,
        _ => return None,
    };
    Some(ImageWrap {
        mode,
        ..Default::default()
    })
}

pub(crate) fn parse_docx_anchor_wrap(
    drawing: &str,
    extract_attr: fn(&str, &str) -> Option<String>,
) -> ImageWrap {
    let mut wrap = ImageWrap::default();
    let behind = extract_attr(drawing, "behindDoc")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    wrap.mode = if drawing.contains("<wp:wrapSquare") {
        ImageWrapMode::Square
    } else if drawing.contains("<wp:wrapTight") {
        ImageWrapMode::Tight
    } else if drawing.contains("<wp:wrapTopAndBottom") {
        ImageWrapMode::TopAndBottom
    } else if behind {
        ImageWrapMode::BehindText
    } else {
        ImageWrapMode::InFrontOfText
    };
    if let Some(side) = extract_attr(drawing, "wrapText") {
        wrap.side = match side.as_str() {
            "left" => WrapSide::LeftOnly,
            "right" => WrapSide::RightOnly,
            "largest" => WrapSide::Largest,
            _ => WrapSide::Both,
        };
    }
    let dist_pt = |attr: &str, default_pt: f32| {
        extract_attr(drawing, attr)
            .and_then(|v| v.parse::<f32>().ok())
            .map(|v| v / 12_700.0)
            .unwrap_or(default_pt)
    };
    wrap.margins = WrapMargins {
        top_pt: dist_pt("distT", 6.0),
        bottom_pt: dist_pt("distB", 6.0),
        left_pt: dist_pt("distL", 9.0),
        right_pt: dist_pt("distR", 9.0),
    };
    wrap
}

impl WriterDocument {
    /// Returns the wrapping configuration of picture `id`, or `ImageWrap::default()` if inline.
    pub fn image_wrap(&self, id: u64) -> Option<ImageWrap> {
        self.blocks
            .iter()
            .find(|b| b.id == id && b.is_image())
            .and_then(|b| b.image.as_ref())
            .map(|img| img.wrap.unwrap_or_default())
    }

    /// Returns the vertical offset (in points) of picture `id`.
    pub fn image_wrap_y_offset(&self, id: u64) -> f32 {
        self.blocks
            .iter()
            .find(|b| b.id == id && b.is_image())
            .and_then(|b| b.image.as_ref())
            .map(|img| img.wrap_y_offset_pt)
            .unwrap_or(0.0)
    }

    /// Sets the wrap mode of picture `id`. Returns `true` if the document changed.
    pub fn set_image_wrap_mode(&mut self, id: u64, mode: ImageWrapMode) -> bool {
        let Some(image) = self
            .blocks
            .iter_mut()
            .find(|b| b.id == id && b.is_image())
            .and_then(|b| b.image.as_mut())
        else {
            return false;
        };
        let mut wrap = image.wrap.unwrap_or_default();
        if wrap.mode == mode {
            return false;
        }
        wrap.mode = mode;
        image.wrap = if mode == ImageWrapMode::Inline && image.wrap_y_offset_pt == 0.0 {
            None
        } else {
            Some(wrap)
        };
        true
    }

    /// Sets which sides text wraps around picture `id`. Returns `true` if changed.
    pub fn set_image_wrap_side(&mut self, id: u64, side: WrapSide) -> bool {
        let Some(image) = self
            .blocks
            .iter_mut()
            .find(|b| b.id == id && b.is_image())
            .and_then(|b| b.image.as_mut())
        else {
            return false;
        };
        let mut wrap = image.wrap.unwrap_or_default();
        if wrap.side == side {
            return false;
        }
        wrap.side = side;
        image.wrap = Some(wrap);
        true
    }

    /// Sets horizontal alignment and anchor frames of picture `id`. Returns `true` if changed.
    pub fn set_image_wrap_align(
        &mut self,
        id: u64,
        horizontal_anchor: HorizontalAnchor,
        vertical_anchor: VerticalAnchor,
        horizontal_align: WrapHorizontalAlign,
    ) -> bool {
        let Some(image) = self
            .blocks
            .iter_mut()
            .find(|b| b.id == id && b.is_image())
            .and_then(|b| b.image.as_mut())
        else {
            return false;
        };
        let mut wrap = image.wrap.unwrap_or_default();
        if wrap.horizontal_anchor == horizontal_anchor
            && wrap.vertical_anchor == vertical_anchor
            && wrap.horizontal_align == horizontal_align
        {
            return false;
        }
        wrap.horizontal_anchor = horizontal_anchor;
        wrap.vertical_anchor = vertical_anchor;
        wrap.horizontal_align = horizontal_align;
        image.wrap = Some(wrap);
        true
    }

    /// Sets the explicit `(x_offset_pt, y_offset_pt)` of picture `id`. Returns `true` if changed.
    pub fn set_image_wrap_offset(&mut self, id: u64, x_offset_pt: f32, y_offset_pt: f32) -> bool {
        if !x_offset_pt.is_finite() || !y_offset_pt.is_finite() {
            return false;
        }
        let Some(image) = self
            .blocks
            .iter_mut()
            .find(|b| b.id == id && b.is_image())
            .and_then(|b| b.image.as_mut())
        else {
            return false;
        };
        let mut wrap = image.wrap.unwrap_or_default();
        let new_align = WrapHorizontalAlign::OffsetPt(x_offset_pt);
        if wrap.horizontal_align == new_align
            && (image.wrap_y_offset_pt - y_offset_pt).abs() < 0.001
        {
            return false;
        }
        wrap.horizontal_align = new_align;
        image.wrap = Some(wrap);
        image.wrap_y_offset_pt = y_offset_pt;
        true
    }

    /// Sets the outer wrap margins (in points) of picture `id`. Returns `true` if changed.
    pub fn set_image_wrap_margins(&mut self, id: u64, margins: WrapMargins) -> bool {
        let clamped = WrapMargins {
            top_pt: margins.top_pt.max(0.0),
            bottom_pt: margins.bottom_pt.max(0.0),
            left_pt: margins.left_pt.max(0.0),
            right_pt: margins.right_pt.max(0.0),
        };
        let Some(image) = self
            .blocks
            .iter_mut()
            .find(|b| b.id == id && b.is_image())
            .and_then(|b| b.image.as_mut())
        else {
            return false;
        };
        let mut wrap = image.wrap.unwrap_or_default();
        if wrap.margins == clamped {
            return false;
        }
        wrap.margins = clamped;
        image.wrap = Some(wrap);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::InlineImage;
    use std::io::Cursor;

    fn tiny_png() -> Vec<u8> {
        let mut bytes = Vec::new();
        let img = image::RgbaImage::from_pixel(4, 4, image::Rgba([10, 20, 30, 255]));
        img.write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
            .unwrap();
        bytes
    }

    #[test]
    fn wrap_mutations_and_loomdoc_roundtrip() {
        let mut doc = WriterDocument::new("doc-1", "Test");
        let img = InlineImage::from_bytes("pic.png", tiny_png()).unwrap();
        let id = doc.insert_image_block(0, img).unwrap();
        assert_eq!(doc.image_wrap(id), Some(ImageWrap::default()));
        assert!(doc.set_image_wrap_mode(id, ImageWrapMode::Square));
        assert!(doc.set_image_wrap_side(id, WrapSide::LeftOnly));
        assert!(doc.set_image_wrap_offset(id, 42.0, 18.0));
        assert!(doc.set_image_wrap_margins(
            id,
            WrapMargins {
                top_pt: 8.0,
                bottom_pt: 8.0,
                left_pt: 12.0,
                right_pt: 12.0,
            }
        ));
        let wrap = doc.image_wrap(id).unwrap();
        assert_eq!(wrap.mode, ImageWrapMode::Square);
        assert_eq!(wrap.side, WrapSide::LeftOnly);
        assert_eq!(wrap.horizontal_align, WrapHorizontalAlign::OffsetPt(42.0));
        assert_eq!(doc.image_wrap_y_offset(id), 18.0);
        assert_eq!(wrap.margins.left_pt, 12.0);
    }
}
