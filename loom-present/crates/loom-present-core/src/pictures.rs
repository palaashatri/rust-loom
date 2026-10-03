//! Pictures on slides: embedded image assets, insertion and persistence.
//!
//! A picture is an ordinary [`SlideElement`] of type [`ElementType::Picture`] whose
//! `content` is the id of an [`ImageAsset`] held by the document. The asset carries the
//! original PNG or JPEG bytes, so a saved deck never depends on the file the picture came
//! from. Bytes are shared (`Arc`), so undo snapshots do not copy images.

use crate::{
    fnv1a64, ElementType, PresentationDocument, PresentationSession, SlideElement, SLIDE_HEIGHT,
    SLIDE_WIDTH,
};
use loom_package::manifest::{Checksum, ManifestEntry, MimeType};
use loom_package::zip::{self, PackageArchive};
use std::collections::BTreeMap;
use std::io::Cursor;
use std::sync::Arc;

/// Largest accepted image file.
pub const MAX_IMAGE_BYTES: usize = 64 * 1024 * 1024;
/// Largest accepted image, in pixels (a decoded copy is 4 bytes per pixel).
pub const MAX_IMAGE_PIXELS: u64 = 36_000_000;
/// Share of the slide a newly inserted picture may fill, per side.
pub const INSERT_FIT_FRACTION: f32 = 0.6;

const ASSET_INDEX: &str = "content/assets.json";

/// Image formats a deck can embed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    Jpeg,
}

impl ImageFormat {
    /// File extension, also used in asset ids and package part names.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
        }
    }

    /// Media type of the encoded bytes.
    pub fn mime(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
        }
    }
}

/// One embedded picture: its original encoded bytes plus what was read from the header.
#[derive(Debug, Clone)]
pub struct ImageAsset {
    /// Stable id (`image-<content hash>.<ext>`); equal images share one asset.
    pub id: String,
    /// Human-readable name (the source file name), used as the picture's alternative text.
    pub name: String,
    pub format: ImageFormat,
    pub width: u32,
    pub height: u32,
    /// The original PNG/JPEG file contents.
    pub bytes: Arc<Vec<u8>>,
}

impl ImageAsset {
    /// Validates `bytes` as a PNG or JPEG of acceptable size and reads its dimensions.
    /// Nothing is decoded here.
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
        let (width, height) = reader
            .into_dimensions()
            .map_err(|error| format!("could not read the image: {error}"))?;
        if width == 0 || height == 0 {
            return Err("the image has no pixels".into());
        }
        if u64::from(width) * u64::from(height) > MAX_IMAGE_PIXELS {
            return Err(format!(
                "the image is too large ({width} x {height} pixels; the limit is {} million)",
                MAX_IMAGE_PIXELS / 1_000_000
            ));
        }
        let id = format!("image-{:016x}.{}", fnv1a64(&bytes), format.extension());
        Ok(Self {
            id,
            name: name.to_string(),
            format,
            width,
            height,
            bytes: Arc::new(bytes),
        })
    }
    /// Decodes the picture to straight RGBA, scaled down (keeping its aspect ratio) when
    /// either side exceeds `max_side`. Used for drawing; the stored bytes are untouched.
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
}

impl PresentationDocument {
    /// The asset a `Picture` element points at.
    pub fn asset_for(&self, element: &SlideElement) -> Option<&ImageAsset> {
        (element.element_type == ElementType::Picture)
            .then(|| self.assets.get(&element.content))
            .flatten()
    }

    /// Number of picture elements across all slides.
    pub fn picture_count(&self) -> usize {
        self.slides
            .iter()
            .flat_map(|slide| &slide.elements)
            .filter(|element| element.element_type == ElementType::Picture)
            .count()
    }
}

impl PresentationSession {
    /// Places `asset` on the active slide as one undoable step: centered, as large as fits
    /// within 60% of the slide while keeping the image's aspect ratio. The new picture is
    /// selected. Returns the element id.
    pub fn insert_picture(&mut self, asset: ImageAsset) -> Result<String, String> {
        if self.document.active_slide().is_none() {
            return Err("there is no slide to insert the picture on".into());
        }
        let (width, height, x, y) = crate::fit_rect_into_box(
            f64::from(asset.width),
            f64::from(asset.height),
            f64::from(SLIDE_WIDTH * (1.0 - INSERT_FIT_FRACTION) / 2.0),
            f64::from(SLIDE_HEIGHT * (1.0 - INSERT_FIT_FRACTION) / 2.0),
            f64::from(SLIDE_WIDTH * INSERT_FIT_FRACTION),
            f64::from(SLIDE_HEIGHT * INSERT_FIT_FRACTION),
        )
        .map(|(x, y, w, h)| (w as f32, h as f32, x as f32, y as f32))?;
        self.checkpoint();
        let asset_id = asset.id.clone();
        self.document.assets.insert(asset_id.clone(), asset);
        let slide = self.document.active_slide_mut().expect("checked above");
        let mut serial = slide.elements.len() + 1;
        let element_id = loop {
            let candidate = format!("{}-picture-{serial}", slide.id);
            if !slide.elements.iter().any(|item| item.id == candidate) {
                break candidate;
            }
            serial += 1;
        };
        slide.elements.push(SlideElement {
            id: element_id.clone(),
            element_type: ElementType::Picture,
            content: asset_id,
            x,
            y,
            width,
            height,
            rotation_deg: 0.0,
            action: None,
        });
        self.selected_elements = vec![element_id.clone()];
        Ok(element_id)
    }
}

/// Where a corner resize of a picture ends up when its aspect ratio is locked. `before` is
/// the element's `(x, y, width, height)` when the gesture began, `requested` the free-form
/// result. The corner opposite the dragged one stays put and the dimension the pointer moved
/// further (relative to the starting size) decides the scale.
pub fn lock_aspect(
    before: (f32, f32, f32, f32),
    requested: (f32, f32, f32, f32),
    moves_left_edge: bool,
    moves_top_edge: bool,
) -> (f32, f32, f32, f32) {
    let (x0, y0, w0, h0) = before;
    let (_, _, w, h) = requested;
    if w0 <= 0.0 || h0 <= 0.0 {
        return requested;
    }
    let scale = (w / w0).max(h / h0).max(1.0 / w0.min(h0));
    let (width, height) = (w0 * scale, h0 * scale);
    let x = if moves_left_edge { x0 + w0 - width } else { x0 };
    let y = if moves_top_edge { y0 + h0 - height } else { y0 };
    (x, y, width, height)
}

fn asset_part(id: &str) -> String {
    format!("assets/{id}")
}

/// Adds every picture asset still used by a slide to a package, with its manifest entry
/// and the name index.
pub(crate) fn write_assets(
    document: &PresentationDocument,
    archive: &mut PackageArchive,
    entries: &mut Vec<ManifestEntry>,
) -> Result<(), String> {
    let mut names = BTreeMap::new();
    for slide in &document.slides {
        for element in &slide.elements {
            if element.element_type != ElementType::Picture {
                continue;
            }
            let Some(asset) = document.assets.get(&element.content) else {
                return Err(format!(
                    "picture '{}' has no image data and cannot be saved",
                    element.id
                ));
            };
            if names.insert(asset.id.clone(), asset.name.clone()).is_some() {
                continue;
            }
            let path = asset_part(&asset.id);
            archive
                .add(&path, asset.bytes.as_ref().clone())
                .map_err(|error| error.to_string())?;
            entries.push(ManifestEntry {
                path,
                mime: MimeType::parse(asset.format.mime())
                    .map_err(|error| format!("invalid image media type: {error}"))?,
                size: asset.bytes.len() as u64,
                sha256: Checksum::from_bytes(zip::sha256(&asset.bytes)),
            });
        }
    }
    if names.is_empty() {
        return Ok(());
    }
    let index = serde_json::to_vec_pretty(&names).map_err(|error| error.to_string())?;
    entries.push(ManifestEntry {
        path: ASSET_INDEX.into(),
        mime: MimeType::parse("application/json")
            .map_err(|error| format!("invalid index media type: {error}"))?,
        size: index.len() as u64,
        sha256: Checksum::from_bytes(zip::sha256(&index)),
    });
    archive
        .add(ASSET_INDEX, index)
        .map_err(|error| error.to_string())
}

/// Reads the picture assets a document's elements point at out of a package. A picture whose
/// image is missing or unreadable is an error: the deck is damaged and opening it silently
/// would lose the picture on the next save.
pub(crate) fn read_assets(
    document: &mut PresentationDocument,
    archive: &PackageArchive,
) -> Result<(), String> {
    let names: BTreeMap<String, String> = match archive.get(ASSET_INDEX) {
        Some(bytes) => serde_json::from_slice(bytes)
            .map_err(|error| format!("picture index is damaged: {error}"))?,
        None => BTreeMap::new(),
    };
    let wanted: Vec<String> = document
        .slides
        .iter()
        .flat_map(|slide| &slide.elements)
        .filter(|element| element.element_type == ElementType::Picture)
        .map(|element| element.content.clone())
        .collect();
    for id in wanted {
        if document.assets.contains_key(&id) {
            continue;
        }
        let bytes = archive
            .get(&asset_part(&id))
            .ok_or_else(|| format!("picture data '{id}' is missing from the deck"))?
            .to_vec();
        let name = names.get(&id).cloned().unwrap_or_else(|| "Picture".into());
        let asset = ImageAsset::from_bytes(&name, bytes)
            .map_err(|error| format!("picture '{id}' is damaged: {error}"))?;
        if asset.id != id {
            return Err(format!("picture '{id}' does not match its stored data"));
        }
        document.assets.insert(id, asset);
    }
    Ok(())
}

/// Longest side, in pixels, of a decoded picture written to a PDF (PDF images are stored
/// uncompressed unless they are JPEG, so large pictures are scaled down first).
const PDF_MAX_SIDE: u32 = 1600;

/// Builds the PDF image for an asset. JPEG files with gray or RGB data are embedded as they
/// are; everything else is decoded to RGB(A) and scaled down when larger than
/// [`PDF_MAX_SIDE`].
pub(crate) fn pdf_image(asset: &ImageAsset) -> Result<loom_pdf::PdfImage, String> {
    use image::ImageDecoder;
    if asset.format == ImageFormat::Jpeg {
        if let Ok(decoder) =
            image::codecs::jpeg::JpegDecoder::new(Cursor::new(asset.bytes.as_slice()))
        {
            let components = match decoder.color_type() {
                image::ColorType::L8 => Some(1),
                image::ColorType::Rgb8 => Some(3),
                _ => None,
            };
            if let Some(components) = components {
                return loom_pdf::PdfImage::jpeg(
                    asset.width,
                    asset.height,
                    components,
                    asset.bytes.as_ref().clone(),
                );
            }
        }
    }
    let mut decoded = image::load_from_memory(&asset.bytes)
        .map_err(|error| format!("could not decode picture '{}': {error}", asset.name))?;
    if decoded.width().max(decoded.height()) > PDF_MAX_SIDE {
        decoded = decoded.resize(
            PDF_MAX_SIDE,
            PDF_MAX_SIDE,
            image::imageops::FilterType::Triangle,
        );
    }
    let rgba = decoded.to_rgba8();
    let (width, height) = rgba.dimensions();
    let mut rgb = Vec::with_capacity(rgba.len() / 4 * 3);
    let mut alpha = Vec::with_capacity(rgba.len() / 4);
    for pixel in rgba.pixels() {
        rgb.extend_from_slice(&pixel.0[..3]);
        alpha.push(pixel.0[3]);
    }
    let alpha = alpha.iter().any(|a| *a != 255).then_some(alpha);
    loom_pdf::PdfImage::rgb(width, height, rgb, alpha)
}
