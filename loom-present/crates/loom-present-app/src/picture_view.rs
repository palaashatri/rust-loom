//! Pictures on slides: the "Insert Image" flow and everything the views need to
//! draw them (decoded images for the canvas and slide-strip thumbnails).

use std::cell::RefCell;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::Path;

use loom_desktop::{FileFilter, OpenFileRequest};
use loom_present_core::{ElementType, ImageAsset, PresentationDocument, Slide, SlideElement};
use slint::{Image, Model, ModelRc, Rgba8Pixel, SharedPixelBuffer, SharedString, VecModel};

use crate::model_sync::synced;
use crate::{element_type_index, GuiState, PresentApp, ThumbRows};

/// Longest side, in pixels, a picture is decoded to for drawing.
const DRAW_SIDE: u32 = 1280;
/// Decoded images kept between refreshes; the cache is dropped when it grows past this.
const CACHE_LIMIT: usize = 48;

thread_local! {
    static DECODED: RefCell<HashMap<String, Image>> = RefCell::new(HashMap::new());
}

/// The picture ready to draw. Decoding happens once per asset, not once per refresh.
pub(crate) fn image_for(asset: &ImageAsset) -> Image {
    DECODED.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(image) = cache.get(&asset.id) {
            return image.clone();
        }
        let image = match asset.preview_rgba(DRAW_SIDE) {
            Ok((width, height, rgba)) => Image::from_rgba8(
                SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(&rgba, width, height),
            ),
            Err(_) => Image::default(),
        };
        if cache.len() >= CACHE_LIMIT {
            cache.clear();
        }
        cache.insert(asset.id.clone(), image.clone());
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

/// Brings the slide strip's thumbnails and the canvas pictures in line with the
/// document. A thumbnail is rewritten only when its slide changed.
pub(crate) fn sync(app: &PresentApp, document: &PresentationDocument) {
    app.set_element_images(synced(
        app.get_element_images(),
        images_for(document, document.active_slide()),
    ));
    let current = app.get_slide_thumbs();
    let model = current.as_any().downcast_ref::<VecModel<ThumbRows>>();
    match model {
        Some(model) if model.row_count() == document.slides.len() => {
            for (index, slide) in document.slides.iter().enumerate() {
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
                    .map(|slide| rows_for(document, Some(slide)))
                    .collect::<Vec<_>>(),
            )));
        }
    }
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
