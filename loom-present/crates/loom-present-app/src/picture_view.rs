//! Pictures on slides: the "Insert Image" flow and everything the views need to
//! draw them (decoded images for the canvas and slide-strip thumbnails).

use std::cell::RefCell;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::Path;

use loom_desktop::{FileFilter, OpenFileRequest};
use loom_present_core::{ElementType, ImageAsset, PresentationDocument, Slide, SlideElement};
use slint::{
    ComponentHandle, Image, Model, ModelRc, Rgba8Pixel, SharedPixelBuffer, SharedString, VecModel,
};

use crate::model_sync::synced;
use crate::{element_type_index, GuiState, PresentApp, ThumbRows};

/// Longest side, in pixels, a picture is decoded to for drawing at 1x. A display
/// with a scale factor of N draws it at N times this size, so a 2x display never
/// shows a 1x bitmap stretched and blurred.
const DRAW_SIDE: u32 = 1280;
/// Hard ceiling on the decoded side, whatever the display density.
const MAX_DRAW_SIDE: u32 = 4096;
/// Decoded pixels kept between refreshes; the cache is dropped past this budget
/// (48 pictures of the 1x draw size).
const CACHE_PIXEL_BUDGET: u64 = 48 * (DRAW_SIDE as u64) * (DRAW_SIDE as u64);

#[derive(Default)]
struct DrawCache {
    images: HashMap<String, Image>,
    pixels: u64,
}

thread_local! {
    static DECODED: RefCell<DrawCache> = RefCell::new(DrawCache::default());
    /// Whole device pixels per logical pixel of the window being drawn.
    static DRAW_DENSITY: std::cell::Cell<u32> = const { std::cell::Cell::new(1) };
}

/// Tell the picture cache how dense the display is. Called from `sync` with the
/// window's scale factor so pictures are decoded for the pixels they will cover.
fn set_draw_density(scale_factor: f32) {
    DRAW_DENSITY.with(|d| d.set(scale_factor.ceil().clamp(1.0, 4.0) as u32));
}

fn draw_side() -> u32 {
    (DRAW_SIDE * DRAW_DENSITY.with(|d| d.get())).min(MAX_DRAW_SIDE)
}

/// The picture ready to draw. Decoding happens once per asset and density, not
/// once per refresh.
pub(crate) fn image_for(asset: &ImageAsset) -> Image {
    let side = draw_side();
    let key = format!("{}@{side}", asset.id);
    DECODED.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(image) = cache.images.get(&key) {
            return image.clone();
        }
        let image = match asset.preview_rgba(side) {
            Ok((width, height, rgba)) => Image::from_rgba8(
                SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(&rgba, width, height),
            ),
            Err(_) => Image::default(),
        };
        let size = image.size();
        let pixels = u64::from(size.width) * u64::from(size.height);
        if cache.pixels + pixels > CACHE_PIXEL_BUDGET {
            cache.images.clear();
            cache.pixels = 0;
        }
        cache.pixels += pixels;
        cache.images.insert(key, image.clone());
        image
    })
}

/// What a picture is called to the user: its file name.
pub(crate) fn picture_name(document: &PresentationDocument, element: &SlideElement) -> String {
    document
        .asset_for(element)
        .map_or_else(|| "Picture".to_string(), |asset| asset.name.clone())
}

/// One image per element of `slide`; elements that are not pictures get an empty image.
pub(crate) fn images_for(document: &PresentationDocument, slide: Option<&Slide>) -> Vec<Image> {
    slide.map_or_else(Vec::new, |slide| {
        slide
            .elements
            .iter()
            .map(|element| {
                document
                    .asset_for(element)
                    .map_or_else(Image::default, image_for)
            })
            .collect()
    })
}

fn fingerprint(document: &PresentationDocument, slide: &Slide) -> String {
    let mut hasher = DefaultHasher::new();
    for element in &slide.elements {
        element.id.hash(&mut hasher);
        element_type_index(&element.element_type).hash(&mut hasher);
        element.content.hash(&mut hasher);
        for value in [
            element.x,
            element.y,
            element.width,
            element.height,
            element.rotation_deg,
        ] {
            value.to_bits().hash(&mut hasher);
        }
        if element.element_type == ElementType::Picture {
            document.asset_for(element).is_some().hash(&mut hasher);
        }
    }
    format!("{:016x}", hasher.finish())
}

/// The canvas rows of one slide as a thumbnail or presenter view draws them.
/// Selection is never shown on a thumbnail.
pub(crate) fn rows_for(document: &PresentationDocument, slide: Option<&Slide>) -> ThumbRows {
    #[cfg(test)]
    ROWS_BUILT.with(|built| built.set(built.get() + 1));
    let elements = slide.map(|slide| slide.elements.as_slice()).unwrap_or(&[]);
    let floats = |values: Vec<f32>| ModelRc::new(VecModel::from(values));
    let text = |element: &SlideElement| {
        if element.element_type == ElementType::Picture {
            SharedString::from(picture_name(document, element))
        } else {
            SharedString::from(element.content.as_str())
        }
    };
    ThumbRows {
        key: slide
            .map_or_else(String::new, |slide| fingerprint(document, slide))
            .into(),
        labels: ModelRc::new(VecModel::from(
            elements.iter().map(text).collect::<Vec<_>>(),
        )),
        contents: ModelRc::new(VecModel::from(
            elements.iter().map(text).collect::<Vec<_>>(),
        )),
        xs: floats(elements.iter().map(|e| e.x).collect()),
        ys: floats(elements.iter().map(|e| e.y).collect()),
        widths: floats(elements.iter().map(|e| e.width).collect()),
        heights: floats(elements.iter().map(|e| e.height).collect()),
        rotations: floats(elements.iter().map(|e| e.rotation_deg).collect()),
        types: ModelRc::new(VecModel::from(
            elements
                .iter()
                .map(|e| element_type_index(&e.element_type))
                .collect::<Vec<_>>(),
        )),
        images: ModelRc::new(VecModel::from(images_for(document, slide))),
    }
}

/// Brings the canvas pictures and the slide strip's thumbnails in line with the
/// document.
pub(crate) fn sync(app: &PresentApp, document: &PresentationDocument) {
    set_draw_density(app.window().scale_factor());
    app.set_element_images(synced(
        app.get_element_images(),
        images_for(document, document.active_slide()),
    ));
    sync_thumbnails(app, document);
}

/// Thumbnails of the slides the strip is drawing (it reports them through
/// `strip-first` and `strip-count`); every other row stays an empty
/// placeholder until the strip scrolls to it. A thumbnail is rewritten only
/// when its slide changed, so an edit costs the window, not the deck.
pub(crate) fn sync_thumbnails(app: &PresentApp, document: &PresentationDocument) {
    let first = usize::try_from(app.get_strip_first()).unwrap_or(0);
    let count = usize::try_from(app.get_strip_count()).unwrap_or(0);
    let window =
        first.min(document.slides.len())..first.saturating_add(count).min(document.slides.len());
    let current = app.get_slide_thumbs();
    let model = current.as_any().downcast_ref::<VecModel<ThumbRows>>();
    match model {
        Some(model) if model.row_count() == document.slides.len() => {
            for index in window {
                let slide = &document.slides[index];
                let key = fingerprint(document, slide);
                let stale = model
                    .row_data(index)
                    .map_or(true, |row| row.key.as_str() != key);
                if stale {
                    model.set_row_data(index, rows_for(document, Some(slide)));
                }
            }
        }
        _ => {
            app.set_slide_thumbs(ModelRc::new(VecModel::from(
                document
                    .slides
                    .iter()
                    .enumerate()
                    .map(|(index, slide)| {
                        if window.contains(&index) {
                            rows_for(document, Some(slide))
                        } else {
                            ThumbRows::default()
                        }
                    })
                    .collect::<Vec<_>>(),
            )));
        }
    }
}

#[cfg(test)]
thread_local! {
    /// Thumbnail row sets built on this thread, for tests of how much work an edit does.
    pub(crate) static ROWS_BUILT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Reads and checks a PNG or JPEG file. Nothing in the deck changes on failure.
pub(crate) fn load_asset(path: &Path) -> Result<ImageAsset, String> {
    let name = path.file_name().map_or_else(
        || "Picture".to_string(),
        |name| name.to_string_lossy().into_owned(),
    );
    let bytes = std::fs::read(path)
        .map_err(|error| format!("could not read '{}': {error}", path.display()))?;
    ImageAsset::from_bytes(&name, bytes)
}

fn image_request(state: &GuiState) -> OpenFileRequest {
    OpenFileRequest {
        title: "Insert Image".into(),
        initial_directory: crate::initial_directory(state.save_path.borrow().as_deref()),
        suggested_name: None,
        filters: vec![FileFilter::new("PNG or JPEG image", ["png", "jpg", "jpeg"])
            .expect("static image filter is valid")],
    }
}

/// "Insert Image...": pick a file, add it to the current slide centered and selected.
pub(crate) fn insert_from_picker(app: &PresentApp, state: &GuiState) {
    let path = match state.dialogs.open_file(&image_request(state)) {
        Ok(Some(path)) => path,
        Ok(None) => return crate::set_status(app, "Insert image cancelled"),
        Err(error) => return crate::set_status(app, format!("Image dialog failed: {error}")),
    };
    let asset = match load_asset(&path) {
        Ok(asset) => asset,
        Err(error) => return crate::set_status(app, format!("Could not insert image: {error}")),
    };
    let name = asset.name.clone();
    let inserted = state.session.borrow_mut().insert_picture(asset);
    match inserted {
        Ok(_) => {
            crate::refresh(app, state);
            crate::set_status(app, format!("Inserted {name}"));
        }
        Err(error) => crate::set_status(app, format!("Could not insert image: {error}")),
    }
}
