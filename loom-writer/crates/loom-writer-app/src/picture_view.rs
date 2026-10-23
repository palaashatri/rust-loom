//! Publishes the pictures of the projected pages to the page canvas.
//!
//! Decoding a picture is slow next to a keystroke, so a decoded picture is
//! kept for as long as its bytes are in the document, and the model behind the
//! canvas is only replaced when a picture moved, resized or changed.

use crate::projection::PageImagePart;
use crate::{PageImages, WriterApp, WriterPageImage};
use loom_writer_core::InlineImage;
use slint::{ComponentHandle, Image, Model, Rgba8Pixel, SharedPixelBuffer, VecModel};
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
            )
        })
        .collect();
    SHOWN.with(|shown| {
        let mut shown = shown.borrow_mut();
        if shown.placed != placed {
            shown.layout = parts
                .iter()
                .map(|p| WriterPageImage {
                    x: p.x,
                    y: p.y,
                    width: p.width,
                    height: p.height,
                    source: decoded(p),
                    label: p.label.clone().into(),
                    selected: false,
                })
                .collect();
            shown.placed = placed;
        } else if shown.model.is_some() && shown.selected == selected {
            return;
        }
        shown.selected = selected;
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
