#![allow(dead_code)]

//! Image projection types and Z-order helpers.

use loom_objects::ImageWrapMode;
use loom_writer_core::InlineImage;

/// Z-order layer for rendering a picture relative to page text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ImageZOrder {
    BehindText,
    #[default]
    Inline,
    InFrontOfText,
}

impl ImageZOrder {
    pub(crate) fn from_image(image: &InlineImage) -> Self {
        match image.wrap.map(|w| w.mode) {
            Some(ImageWrapMode::BehindText) => Self::BehindText,
            Some(ImageWrapMode::InFrontOfText) => Self::InFrontOfText,
            _ => Self::Inline,
        }
    }
}

/// One picture to draw: where it sits, its z-order, and what it shows.
#[derive(Clone)]
pub(crate) struct PageImagePart {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub label: String,
    pub image: InlineImage,
    pub z_order: ImageZOrder,
}
