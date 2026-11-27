//! Publishes the pictures of the projected pages to the page canvas.
//!
//! Decoding a picture is slow next to a keystroke, so decoded RGBA pixels are
//! retained in a bounded LRU cache while their bytes remain in the document,
//! and the canvas model is only replaced when a picture moves, resizes, or
//! changes.

use crate::projection::PageImagePart;
use crate::{PageImages, WriterApp, WriterPageImage};
use loom_writer_core::InlineImage;
use slint::{ComponentHandle, Image, Model, Rgba8Pixel, SharedPixelBuffer, SharedString, VecModel};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::Arc;

/// Longest side a picture is decoded to for the screen.
const SCREEN_SIDE: u32 = 2048;
/// Decoded RGBA byte budget retained across projections (32 MiB).
const CACHE_BUDGET_BYTES: usize = 32 * 1024 * 1024;
/// Maximum number of decoded entries retained at once.
const CACHE_LIMIT: usize = 24;
/// Maximum declared image pixels before attempting an RGBA decode.
const MAX_DECODED_PIXELS: u64 = 36_000_000;

const _: () = assert!(SCREEN_SIDE as usize * SCREEN_SIDE as usize * 4 <= CACHE_BUDGET_BYTES);

#[derive(Clone)]
struct DecodedResult {
    image: Image,
    error: Option<String>,
}

/// A decoded picture and the bytes whose address keys it, kept alive so the
/// address is never reused for another picture.
struct CacheEntry {
    key: usize,
    _bytes: Arc<Vec<u8>>,
    result: DecodedResult,
    rgba_bytes: usize,
}

struct DecodedCache {
    /// Most recently used entry at the front; oldest evicted from the back.
    entries: VecDeque<CacheEntry>,
    total_bytes: usize,
    budget_bytes: usize,
}

impl DecodedCache {
    const fn new(budget_bytes: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            total_bytes: 0,
            budget_bytes,
        }
    }

    fn get(&mut self, key: usize) -> Option<DecodedResult> {
        let index = self.entries.iter().position(|entry| entry.key == key)?;
        let entry = self.entries.remove(index)?;
        let result = entry.result.clone();
        self.entries.push_front(entry);
        Some(result)
    }

    fn insert(&mut self, key: usize, bytes: Arc<Vec<u8>>, result: DecodedResult) {
        let rgba_bytes = decoded_bytes(&result.image);
        if rgba_bytes > self.budget_bytes {
            return;
        }
        while self.entries.len() >= CACHE_LIMIT
            || self.total_bytes.saturating_add(rgba_bytes) > self.budget_bytes
        {
            let Some(evicted) = self.entries.pop_back() else {
                break;
            };
            self.total_bytes = self.total_bytes.saturating_sub(evicted.rgba_bytes);
        }
        self.total_bytes = self.total_bytes.saturating_add(rgba_bytes);
        self.entries.push_front(CacheEntry {
            key,
            _bytes: bytes,
            result,
            rgba_bytes,
        });
    }
}

/// Where a picture was drawn last time: position, size and which bytes.
type Placed = (u32, u32, u32, u32, usize, u8);

thread_local! {
    static DECODED: RefCell<DecodedCache> =
        const { RefCell::new(DecodedCache::new(CACHE_BUDGET_BYTES)) };
    static SHOWN: RefCell<Shown> = RefCell::new(Shown::default());
}

/// What the canvas shows, kept so one picture can be previewed in place and
/// the selection outline can follow the caret without laying the page out again.
#[derive(Default)]
struct Shown {
    /// Where the projection laid each picture out, with its identity.
    placed: Vec<Placed>,
    /// The picture items as laid out, before selection and any preview.
    layout: Vec<WriterPageImage>,
    /// First decode error among the visible pictures, if any.
    first_error: Option<String>,
    /// The identity of the selected picture, if one is on the page.
    selected: Option<usize>,
    /// The model the canvas reads.
    model: Option<Rc<VecModel<WriterPageImage>>>,
}

/// A picture as the projection laid it out, in page points.
pub(crate) struct LaidOut {
    pub key: usize,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Identity of a picture on the page: the address of its bytes. Insertion,
/// loading and pasting each read the bytes afresh, so no two pictures in a
/// document share them, and the document, projection and canvas agree on it.
pub(crate) fn picture_key(image: &InlineImage) -> usize {
    Arc::as_ptr(&image.bytes) as usize
}

fn key(part: &PageImagePart) -> usize {
    picture_key(&part.image)
}

fn decoded_bytes(image: &Image) -> usize {
    let size = image.size();
    usize::try_from(u64::from(size.width) * u64::from(size.height) * 4).unwrap_or(usize::MAX)
}

/// The picture decoded for the screen; an unreadable or oversized picture
/// returns an empty `Image` paired with a user-visible error message so the
/// canvas can show a fallback box instead of disappearing silently.
fn decoded(part: &PageImagePart) -> DecodedResult {
    let cache_key = key(part);
    if let Some(hit) = DECODED.with(|cache| cache.borrow_mut().get(cache_key)) {
        return hit;
    }
    let pixel_count =
        u64::from(part.image.width_px).saturating_mul(u64::from(part.image.height_px));
    let result = if pixel_count == 0 {
        DecodedResult {
            image: Image::default(),
            error: Some("image has zero pixels".into()),
        }
    } else if pixel_count > MAX_DECODED_PIXELS {
        DecodedResult {
            image: Image::default(),
            error: Some(format!(
                "image dimensions {}x{} exceed the {} million pixel limit",
                part.image.width_px,
                part.image.height_px,
                MAX_DECODED_PIXELS / 1_000_000
            )),
        }
    } else {
        match part.image.preview_rgba(SCREEN_SIDE) {
            Ok((w, h, rgba)) => DecodedResult {
                image: Image::from_rgba8(SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
                    &rgba, w, h,
                )),
                error: None,
            },
            Err(error) => DecodedResult {
                image: Image::default(),
                error: Some(error),
            },
        }
    };
    DECODED.with(|cache| {
        cache
            .borrow_mut()
            .insert(cache_key, part.image.bytes.clone(), result.clone());
    });
    result
}

/// Sets the canvas's pictures, marking the one whose identity is `selected`;
/// does nothing when the pictures and the selection are as before.
pub(crate) fn publish(app: &WriterApp, parts: &[PageImagePart], selected: Option<usize>) {
    let placed: Vec<Placed> = parts
        .iter()
        .map(|p| {
            (
                p.x.to_bits(),
                p.y.to_bits(),
                p.width.to_bits(),
                p.height.to_bits(),
                key(p),
                p.z_order.rank(),
            )
        })
        .collect();
    SHOWN.with(|shown| {
        let mut shown = shown.borrow_mut();
        if shown.placed != placed {
            let mut first_error = None;
            shown.layout = parts
                .iter()
                .map(|p| {
                    let result = decoded(p);
                    let label = match &result.error {
                        Some(error) => {
                            if first_error.is_none() {
                                first_error = Some(error.clone());
                            }
                            format!("{}: image unavailable ({error})", p.label)
                        }
                        None => p.label.clone(),
                    };
                    WriterPageImage {
                        x: p.x,
                        y: p.y,
                        width: p.width,
                        height: p.height,
                        source: result.image,
                        label: label.into(),
                        selected: false,
                        z_order: p.z_order.rank() as i32,
                    }
                })
                .collect();
            shown.first_error = first_error;
            shown.placed = placed;
        } else if shown.model.is_some() && shown.selected == selected {
            if let Some(error) = &shown.first_error {
                app.set_status_right(SharedString::from(format!("Picture unavailable: {error}")));
            }
            return;
        }
        shown.selected = selected;
        if let Some(error) = &shown.first_error {
            app.set_status_right(SharedString::from(format!("Picture unavailable: {error}")));
        } else if app
            .get_status_right()
            .as_str()
            .starts_with("Picture unavailable:")
        {
            app.set_status_right(SharedString::new());
        }
        show(app, &mut shown);
    });
}

/// Rebuilds the canvas model from the layout, marking the selected picture.
fn show(app: &WriterApp, shown: &mut Shown) {
    let selected = shown.selected;
    let items: Vec<WriterPageImage> = shown
        .layout
        .iter()
        .zip(&shown.placed)
        .map(|(item, placed)| {
            let mut item = item.clone();
            item.selected = Some(placed.4) == selected;
            item
        })
        .collect();
    let model = Rc::new(VecModel::from(items));
    app.global::<PageImages>().set_items(model.clone().into());
    shown.model = Some(model);
}

/// Where the projection laid each picture out, not where a drag previews it:
/// the geometry that pointer hit tests use.
pub(crate) fn laid_out() -> Vec<LaidOut> {
    SHOWN.with(|shown| {
        shown
            .borrow()
            .placed
            .iter()
            .map(|p| LaidOut {
                key: p.4,
                x: f32::from_bits(p.0),
                y: f32::from_bits(p.1),
                width: f32::from_bits(p.2),
                height: f32::from_bits(p.3),
            })
            .collect()
    })
}

/// Shows picture `key` at the given geometry on the canvas only; the document
/// is untouched. Used for the live preview of a resize drag.
pub(crate) fn preview(key: usize, x: f32, y: f32, width: f32, height: f32) {
    let (model, row) = SHOWN.with(|shown| {
        let shown = shown.borrow();
        (
            shown.model.clone(),
            shown.placed.iter().position(|p| p.4 == key),
        )
    });
    let (Some(model), Some(row)) = (model, row) else {
        return;
    };
    if let Some(mut item) = model.row_data(row) {
        item.x = x;
        item.y = y;
        item.width = width;
        item.height = height;
        model.set_row_data(row, item);
    }
}

/// Shows picture `key` as the projection laid it out, ending a preview.
pub(crate) fn restore(key: usize) {
    if let Some(laid) = laid_out().into_iter().find(|laid| laid.key == key) {
        preview(laid.key, laid.x, laid.y, laid.width, laid.height);
    }
}

#[cfg(test)]
pub(crate) fn reset_cache_for_tests(budget_bytes: usize) {
    DECODED.with(|cache| *cache.borrow_mut() = DecodedCache::new(budget_bytes));
    SHOWN.with(|shown| *shown.borrow_mut() = Shown::default());
}

#[cfg(test)]
pub(crate) fn cached_bytes_for_tests() -> usize {
    DECODED.with(|cache| cache.borrow().total_bytes)
}

#[cfg(test)]
pub(crate) fn cached_entries_for_tests() -> usize {
    DECODED.with(|cache| cache.borrow().entries.len())
}

#[cfg(test)]
pub(crate) fn is_cached_for_tests(image: &InlineImage) -> bool {
    let target = picture_key(image);
    DECODED.with(|cache| {
        cache
            .borrow()
            .entries
            .iter()
            .any(|entry| entry.key == target)
    })
}

#[cfg(test)]
pub(crate) fn default_cache_budget_bytes() -> usize {
    CACHE_BUDGET_BYTES
}
