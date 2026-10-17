//! Publishes the pictures of the projected pages to the page canvas.
//!
//! Decoding a picture is slow next to a keystroke, so a decoded picture is
//! kept for as long as its bytes are in the document, and the model behind the
//! canvas is only replaced when a picture moved, resized or changed.

use crate::projection::PageImagePart;
use crate::{PageImages, WriterApp, WriterPageImage};
use slint::{ComponentHandle, Image, Rgba8Pixel, SharedPixelBuffer, VecModel};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

/// Longest side a picture is decoded to for the screen.
const SCREEN_SIDE: u32 = 2048;
/// Decoded pictures kept at once.
const CACHE_LIMIT: usize = 24;

/// A decoded picture and the bytes whose address keys it, kept alive so the
/// address is never reused for another picture.
type Decoded = (Arc<Vec<u8>>, Image);

/// Where a picture was drawn last time: position, size and which bytes.
type Placed = (u32, u32, u32, u32, usize);

thread_local! {
    static DECODED: RefCell<HashMap<usize, Decoded>> = RefCell::new(HashMap::new());
    static LAST: RefCell<Vec<Placed>> = const { RefCell::new(Vec::new()) };
}

fn key(part: &PageImagePart) -> usize {
    Arc::as_ptr(&part.image.bytes) as usize
}

/// The picture decoded for the screen; an unreadable picture is a blank
/// placeholder so the page keeps its layout.
fn decoded(part: &PageImagePart) -> Image {
    DECODED.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some((_, image)) = cache.get(&key(part)) {
            return image.clone();
        }
        let image = match part.image.preview_rgba(SCREEN_SIDE) {
            Ok((w, h, rgba)) => Image::from_rgba8(
                SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(&rgba, w, h),
            ),
            Err(_) => Image::default(),
        };
        if cache.len() >= CACHE_LIMIT {
            cache.clear();
        }
        cache.insert(key(part), (part.image.bytes.clone(), image.clone()));
        image
    })
}

/// Sets the canvas's pictures; does nothing when they are as before.
pub(crate) fn publish(app: &WriterApp, parts: &[PageImagePart]) {
    let placed: Vec<Placed> = parts
        .iter()
        .map(|p| {
            (
                p.x.to_bits(),
                p.y.to_bits(),
                p.width.to_bits(),
                p.height.to_bits(),
                key(p),
            )
        })
        .collect();
    if LAST.with(|last| *last.borrow() == placed) {
        return;
    }
    LAST.with(|last| *last.borrow_mut() = placed);
    let items: Vec<WriterPageImage> = parts
        .iter()
        .map(|p| WriterPageImage {
            x: p.x,
            y: p.y,
            width: p.width,
            height: p.height,
            source: decoded(p),
            label: p.label.clone().into(),
        })
        .collect();
    app.global::<PageImages>()
        .set_items(Rc::new(VecModel::from(items)).into());
}
