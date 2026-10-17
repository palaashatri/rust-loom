//! Inline pictures.
//!
//! A picture is a block of kind [`IMAGE_BLOCK_KIND`] whose text is one object
//! replacement character plus a private-use marker, so it takes one place in
//! the editor text stream and can be selected, deleted, undone and restored
//! like any other character. The marker makes the text of every picture
//! distinct, which keeps the paragraph matcher from handing the payload of a
//! deleted picture to one that stayed. The encoded PNG or JPEG bytes ride on
//! the block (shared, so undo snapshots do not copy them) and are saved inside
//! the document, so a saved file never depends on where the picture came from.

use crate::{RichBlock, WriterDocument};
use std::io::Cursor;
use std::sync::Arc;

/// Block kind of an inline picture.
pub const IMAGE_BLOCK_KIND: &str = "image";
/// Largest accepted image file.
pub const MAX_IMAGE_BYTES: usize = 32 * 1024 * 1024;
/// Largest accepted image, in pixels (a decoded copy is 4 bytes per pixel).
pub const MAX_IMAGE_PIXELS: u64 = 36_000_000;

const OBJECT_CHAR: char = '\u{FFFC}';
const MARKER_BASE: u32 = 0xE000;
const MARKER_COUNT: u32 = 0x1800;
/// A picture is shown at 96 pixels to the inch unless the page is narrower.
const POINTS_PER_PIXEL: f32 = 0.75;

/// Image formats a document can embed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    Jpeg,
}

impl ImageFormat {
    /// Media type of the encoded bytes.
    pub fn mime(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
        }
    }

    /// File extension, also used for package part names.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
        }
    }
}

/// One embedded picture: the original encoded bytes plus what was read from
/// the header and the size it is laid out at.
#[derive(Debug, Clone, PartialEq)]
pub struct InlineImage {
    /// The source file name, used as the picture's alternative text.
    pub name: String,
    pub format: ImageFormat,
    pub width_px: u32,
    pub height_px: u32,
    /// Size on the page before it is fitted to the text width, in points.
    pub width_pt: f32,
    pub height_pt: f32,
    pub bytes: Arc<Vec<u8>>,
}

impl InlineImage {
    /// Validates `bytes` as a PNG or JPEG of acceptable size and reads its
    /// dimensions. Nothing is decoded here.
    pub fn from_bytes(name: &str, bytes: Vec<u8>) -> Result<Self, String> {
        if bytes.is_empty() {
            return Err("the image file is empty".into());
        }
        if bytes.len() > MAX_IMAGE_BYTES {
            return Err(format!(
                "the image is larger than {} MB",
                MAX_IMAGE_BYTES / (1024 * 1024)
            ));
        }
        let reader = image::ImageReader::new(Cursor::new(&bytes))
            .with_guessed_format()
            .map_err(|error| format!("could not read the image: {error}"))?;
        let format = match reader.format() {
            Some(image::ImageFormat::Png) => ImageFormat::Png,
            Some(image::ImageFormat::Jpeg) => ImageFormat::Jpeg,
            _ => return Err("only PNG and JPEG pictures can be inserted".into()),
        };
        let (width_px, height_px) = reader
            .into_dimensions()
            .map_err(|error| format!("could not read the image: {error}"))?;
        if width_px == 0 || height_px == 0 {
            return Err("the image has no pixels".into());
        }
        if u64::from(width_px) * u64::from(height_px) > MAX_IMAGE_PIXELS {
            return Err(format!(
                "the image is too large ({width_px} x {height_px} pixels; the limit is {} million)",
                MAX_IMAGE_PIXELS / 1_000_000
            ));
        }
        Ok(Self {
            name: name.to_string(),
            format,
            width_px,
            height_px,
            width_pt: width_px as f32 * POINTS_PER_PIXEL,
            height_pt: height_px as f32 * POINTS_PER_PIXEL,
            bytes: Arc::new(bytes),
        })
    }

    /// Decodes the picture to straight RGBA, scaled down (keeping its aspect
    /// ratio) when either side exceeds `max_side`. Used for drawing; the
    /// stored bytes are untouched.
    pub fn preview_rgba(&self, max_side: u32) -> Result<(u32, u32, Vec<u8>), String> {
        let mut decoded = image::load_from_memory(&self.bytes)
            .map_err(|error| format!("could not decode picture '{}': {error}", self.name))?;
        if max_side > 0 && decoded.width().max(decoded.height()) > max_side {
            decoded = decoded.resize(max_side, max_side, image::imageops::FilterType::Triangle);
        }
        let rgba = decoded.to_rgba8();
        let (width, height) = rgba.dimensions();
        Ok((width, height, rgba.into_raw()))
    }

    /// The size the picture is laid out at in a text area of the given size:
    /// its own size, scaled down to fit and never distorted.
    pub fn fitted_size(&self, usable_width: f32, usable_height: f32) -> (f32, f32) {
        let (w, h) = (self.width_pt.max(1.0), self.height_pt.max(1.0));
        let scale = (usable_width / w).min(usable_height / h).min(1.0);
        let scale = if scale.is_finite() && scale > 0.0 {
            scale
        } else {
            1.0
        };
        (w * scale, h * scale)
    }
}

impl RichBlock {
    /// Whether this block is a picture that is still intact: its kind, its
    /// object character and its payload all agree.
    pub fn is_image(&self) -> bool {
        self.kind == IMAGE_BLOCK_KIND
            && self.image.is_some()
            && self.text.as_str().starts_with(OBJECT_CHAR)
    }

    /// What a picture reads as where only text can go (Markdown, HTML): its
    /// name in brackets; `None` for everything else.
    pub fn image_placeholder(&self) -> Option<String> {
        let image = self.image.as_ref().filter(|_| self.is_image())?;
        Some(format!("[Image: {}]", image.name))
    }

    /// The picture's size in a text area of the given size, for a picture
    /// block; `None` for everything else.
    pub fn image_display_size(&self, usable_width: f32, usable_height: f32) -> Option<(f32, f32)> {
        if !self.is_image() {
            return None;
        }
        self.image
            .as_ref()
            .map(|image| image.fitted_size(usable_width, usable_height))
    }
}

impl WriterDocument {
    /// Inserts a picture as its own block at `offset` of the editor text and
    /// returns the block id. A paragraph is added after it when it would be
    /// the last block, so typing can continue below the picture.
    pub fn insert_image_block(&mut self, offset: usize, image: InlineImage) -> Option<u64> {
        let marker = self.free_marker()?;
        let mut next_id = self.blocks.iter().map(|block| block.id).max().unwrap_or(0) + 1;
        while self.blocks.iter().any(|block| block.id == next_id) {
            next_id += 1;
        }
        let mut block =
            RichBlock::new(next_id, IMAGE_BLOCK_KIND, &format!("{OBJECT_CHAR}{marker}"));
        block.image = Some(image);
        let insert_at = crate::tables::block_insert_index(self, offset);
        self.blocks.insert(insert_at, block);
        if insert_at + 1 == self.blocks.len() {
            self.blocks
                .push(RichBlock::new(next_id + 1, "paragraph", ""));
        }
        Some(next_id)
    }

    /// The smallest private-use marker no other picture uses.
    fn free_marker(&self) -> Option<char> {
        (0..MARKER_COUNT)
            .filter_map(|n| char::from_u32(MARKER_BASE + n))
            .find(|marker| {
                !self.blocks.iter().any(|block| {
                    block.kind == IMAGE_BLOCK_KIND && block.text.as_str().contains(*marker)
                })
            })
    }
}

/// The text of a picture block whose marker is number `n`.
pub(crate) fn picture_text(n: u32) -> String {
    let marker = char::from_u32(MARKER_BASE + n % MARKER_COUNT).unwrap_or('\u{E000}');
    format!("{OBJECT_CHAR}{marker}")
}

/// `{"name":..,"wpt":..,"hpt":..,"data":"<base64>"}`: how a picture is saved.
pub(crate) fn image_json(image: &InlineImage) -> String {
    format!(
        "{{\"name\":{},\"wpt\":{},\"hpt\":{},\"data\":\"{}\"}}",
        crate::pkg_json::escape(&image.name),
        image.width_pt,
        image.height_pt,
        base64_encode(&image.bytes)
    )
}

/// Reads a picture written by [`image_json`]; `None` when the bytes are not an
/// acceptable PNG or JPEG.
pub(crate) fn image_from_fields(
    name: &str,
    width_pt: Option<f32>,
    height_pt: Option<f32>,
    data: &str,
) -> Option<InlineImage> {
    let bytes = base64_decode(data)?;
    let mut image = InlineImage::from_bytes(name, bytes).ok()?;
    if let (Some(w), Some(h)) = (width_pt, height_pt) {
        if w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0 {
            image.width_pt = w;
            image.height_pt = h;
        }
    }
    Some(image)
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let (mut acc, mut bits) = (0u32, 0u32);
    let mut padding = 0;
    for byte in text.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => {
                padding += 1;
                continue;
            }
            b'\n' | b'\r' | b' ' => continue,
            _ => return None,
        };
        if padding > 0 {
            return None;
        }
        acc = (acc << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests;
