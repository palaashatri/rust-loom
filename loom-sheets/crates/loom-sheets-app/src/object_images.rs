//! Decoded worksheet images shared by every projection.
//!
//! A full projection used to decode every image in the sheet and copy each
//! RGBA buffer into a fresh Slint handle, including objects scrolled far out of
//! sight. Decoded images are immutable document content, so they now live in a
//! bounded LRU keyed by content: embedded buffers by their shared immutable
//! identity (pointer and length), path-backed images by path plus file
//! identity. Moving, reordering, undoing or re-projecting the same image
//! reuses the decoded pixels instead of decoding and copying again.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use loom_sheets_core::{SheetObject, SheetObjectKind};
use slint::{Image, Rgba8Pixel, SharedPixelBuffer};

/// Decoded RGBA bytes retained across projections. Roughly eight 1280x800
/// images fit; a larger visible set still decodes, it just is not retained.
const CACHE_BUDGET_BYTES: usize = 32 * 1024 * 1024;
/// Entry ceiling so many tiny images cannot crowd the LRU bookkeeping.
const CACHE_ENTRIES: usize = 16;

/// What an image resource is, independent of which object currently uses it.
///
/// Embedded payloads are immutable shared buffers, so identity is their
/// pointer plus length. That is exact (a cached entry keeps its buffer alive,
/// so the address cannot be recycled) and O(1), unlike hashing megabytes of
/// pixels on every projection. Two independently loaded copies of identical
/// bytes are separate identities and decode once each.
#[derive(Clone)]
enum ImageKey {
    Embedded {
        bytes: Arc<[u8]>,
        svg: bool,
    },
    /// A file fallback: same path, same length and same modification time.
    File {
        path: PathBuf,
        len: u64,
        modified_nanos: Option<u128>,
    },
}

impl PartialEq for ImageKey {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::Embedded {
                    bytes: left,
                    svg: left_svg,
                },
                Self::Embedded {
                    bytes: right,
                    svg: right_svg,
                },
            ) => left_svg == right_svg && left.len() == right.len() && Arc::ptr_eq(left, right),
            (
                Self::File {
                    path: left_path,
                    len: left_len,
                    modified_nanos: left_modified,
                },
                Self::File {
                    path: right_path,
                    len: right_len,
                    modified_nanos: right_modified,
                },
            ) => {
                left_path == right_path && left_len == right_len && left_modified == right_modified
            }
            _ => false,
        }
    }
}

impl Eq for ImageKey {}

/// Where the bytes for an identity come from on a miss.
enum Source<'a> {
    Embedded { bytes: &'a [u8], svg: bool },
    File(&'a str),
}

struct CachedImage {
    key: ImageKey,
    image: Image,
    bytes: usize,
}

struct ImageCache {
    /// Most recently used first.
    entries: VecDeque<CachedImage>,
    budget: usize,
    bytes: usize,
}

impl ImageCache {
    const fn new(budget: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            budget,
            bytes: 0,
        }
    }

    fn get(&mut self, key: &ImageKey) -> Option<Image> {
        let index = self.entries.iter().position(|cached| &cached.key == key)?;
        let entry = self.entries.remove(index)?;
        let image = entry.image.clone();
        self.entries.push_front(entry);
        Some(image)
    }

    fn insert(&mut self, key: ImageKey, image: Image) {
        let bytes = decoded_bytes(&image);
        // One image larger than the whole budget is still returned, but it is
        // not worth evicting every reusable image to retain it.
        if bytes > self.budget {
            return;
        }
        while self.entries.len() >= CACHE_ENTRIES || self.bytes.saturating_add(bytes) > self.budget
        {
            let Some(evicted) = self.entries.pop_back() else {
                break;
            };
            self.bytes = self.bytes.saturating_sub(evicted.bytes);
        }
        self.bytes = self.bytes.saturating_add(bytes);
        self.entries.push_front(CachedImage { key, image, bytes });
    }
}

thread_local! {
    static CACHE: RefCell<ImageCache> =
        const { RefCell::new(ImageCache::new(CACHE_BUDGET_BYTES)) };
}

/// The decoded image for a worksheet object. Shapes and any other non-image
/// object get the empty image without touching the cache.
pub(crate) fn image_for(object: &SheetObject) -> Image {
    if object.kind != SheetObjectKind::Image {
        return Image::default();
    }
    let (key, source) = identity(object);
    if let Some(image) = CACHE.with(|cache| cache.borrow_mut().get(&key)) {
        return image;
    }
    let image = decode(source);
    CACHE.with(|cache| cache.borrow_mut().insert(key, image.clone()));
    image
}

fn identity(object: &SheetObject) -> (ImageKey, Source<'_>) {
    if let Some(bytes) = object.embedded.as_ref() {
        let svg = has_svg_extension(&object.path);
        let key = ImageKey::Embedded {
            bytes: Arc::clone(bytes),
            svg,
        };
        return (
            key,
            Source::Embedded {
                bytes: &bytes[..],
                svg,
            },
        );
    }
    let path = PathBuf::from(&object.path);
    let (len, modified_nanos) = std::fs::metadata(&path).map_or((0, None), |metadata| {
        let modified_nanos = metadata
            .modified()
            .ok()
            .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
            .map(|elapsed| elapsed.as_nanos());
        (metadata.len(), modified_nanos)
    });
    (
        ImageKey::File {
            path,
            len,
            modified_nanos,
        },
        Source::File(&object.path),
    )
}

/// SVGs are selected by file extension, matching the persistence/import path.
fn has_svg_extension(path: &str) -> bool {
    path.rsplit_once('.')
        .is_some_and(|(_, extension)| extension.eq_ignore_ascii_case("svg"))
}

fn decode(source: Source<'_>) -> Image {
    #[cfg(test)]
    DECODE_CALLS.with(|calls| calls.set(calls.get() + 1));
    match source {
        Source::Embedded { bytes, svg } => decode_embedded(bytes, svg),
        Source::File(path) => Image::load_from_path(Path::new(path)).unwrap_or_default(),
    }
}

fn decode_embedded(bytes: &[u8], svg: bool) -> Image {
    if svg {
        return Image::load_from_svg_data(bytes).unwrap_or_default();
    }
    match image::load_from_memory(bytes) {
        Ok(decoded) => {
            let rgba = decoded.to_rgba8();
            Image::from_rgba8(SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
                rgba.as_raw(),
                rgba.width(),
                rgba.height(),
            ))
        }
        Err(_) => Image::default(),
    }
}

fn decoded_bytes(image: &Image) -> usize {
    let size = image.size();
    usize::try_from(u64::from(size.width) * u64::from(size.height) * 4).unwrap_or(usize::MAX)
}

#[cfg(test)]
thread_local! {
    /// Real decode attempts, so culling and reuse can be asserted rather than
    /// inferred from timing.
    static DECODE_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn reset_cache_for_tests(budget: usize) {
    CACHE.with(|cache| *cache.borrow_mut() = ImageCache::new(budget));
    DECODE_CALLS.with(|calls| calls.set(0));
}

#[cfg(test)]
pub(crate) fn decode_calls_for_tests() -> usize {
    DECODE_CALLS.with(std::cell::Cell::get)
}

#[cfg(test)]
pub(crate) fn cached_entries_for_tests() -> usize {
    CACHE.with(|cache| cache.borrow().entries.len())
}

#[cfg(test)]
pub(crate) fn cached_bytes_for_tests() -> usize {
    CACHE.with(|cache| cache.borrow().bytes)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use loom_sheets_core::CellRef;

    use super::*;

    fn png(width: u32, height: u32, seed: u32) -> Vec<u8> {
        let mut pixels = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            for x in 0..width {
                let value = x
                    .wrapping_mul(31)
                    .wrapping_add(y.wrapping_mul(17))
                    .wrapping_add(seed.wrapping_mul(37));
                pixels.extend_from_slice(&[
                    (value & 0xff) as u8,
                    ((value >> 8) & 0xff) as u8,
                    ((value >> 16) & 0xff) as u8,
                    0xff,
                ]);
            }
        }
        let image = image::RgbaImage::from_raw(width, height, pixels).expect("pixel count matches");
        let mut bytes = Vec::new();
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
            .expect("encode benchmark image");
        bytes
    }

    fn object(bytes: impl Into<Arc<[u8]>>, name: &str) -> SheetObject {
        let mut object = SheetObject::image(CellRef { row: 0, col: 0 }, name)
            .expect("image object needs a path");
        object.embedded = Some(bytes.into());
        object
    }

    #[test]
    fn shared_content_buffer_reuses_one_decode_across_object_instances() {
        reset_cache_for_tests(CACHE_BUDGET_BYTES);
        let bytes: Arc<[u8]> = png(16, 16, 1).into();
        let first = image_for(&object(Arc::clone(&bytes), "first.png"));
        let second = image_for(&object(bytes, "renamed-copy.png"));
        assert_eq!(decode_calls_for_tests(), 1);
        assert_eq!(first.size(), second.size());
        assert_eq!(
            first.to_rgba8().expect("raster image").as_bytes(),
            second.to_rgba8().expect("raster image").as_bytes()
        );
    }

    #[test]
    fn separately_loaded_copies_of_identical_bytes_are_distinct_identities() {
        // Identity is the immutable buffer, not a content hash: a reopened or
        // re-imported copy decodes once, then reuses its own entry.
        reset_cache_for_tests(CACHE_BUDGET_BYTES);
        let bytes = png(16, 16, 1);
        image_for(&object(bytes.clone(), "first.png"));
        image_for(&object(bytes, "second.png"));
        assert_eq!(decode_calls_for_tests(), 2);
    }

    #[test]
    fn different_content_does_not_share_a_cache_entry() {
        reset_cache_for_tests(CACHE_BUDGET_BYTES);
        image_for(&object(png(16, 16, 1), "one.png"));
        image_for(&object(png(16, 16, 2), "two.png"));
        assert_eq!(decode_calls_for_tests(), 2);
    }

    #[test]
    fn non_image_objects_never_decode() {
        reset_cache_for_tests(CACHE_BUDGET_BYTES);
        let shape = SheetObject::shape(CellRef { row: 0, col: 0 }, "Callout");
        let image = image_for(&shape);
        assert_eq!(decode_calls_for_tests(), 0);
        assert_eq!(image.size().width, 0);
    }

    #[test]
    fn cache_evicts_the_least_recently_used_image_past_its_budget() {
        // Two 8x8 images fit exactly; a third evicts the least recently used.
        let budget = 2 * 8 * 8 * 4;
        reset_cache_for_tests(budget);
        let a = object(png(8, 8, 1), "a.png");
        let b = object(png(8, 8, 2), "b.png");
        let c = object(png(8, 8, 3), "c.png");
        image_for(&a);
        image_for(&b);
        image_for(&a); // a is now the most recently used.
        assert_eq!(decode_calls_for_tests(), 2);
        assert_eq!(cached_bytes_for_tests(), budget);

        image_for(&c);
        assert_eq!(decode_calls_for_tests(), 3);
        assert!(cached_bytes_for_tests() <= budget);
        assert!(cached_entries_for_tests() <= 2);

        image_for(&a);
        assert_eq!(
            decode_calls_for_tests(),
            3,
            "a was just used and must remain"
        );
        image_for(&b);
        assert_eq!(
            decode_calls_for_tests(),
            4,
            "b was least recently used and must have been evicted"
        );
    }

    #[test]
    fn an_image_larger_than_the_budget_is_not_retained() {
        reset_cache_for_tests(8 * 8 * 4);
        let large = object(png(16, 16, 1), "large.png");
        image_for(&large);
        image_for(&large);
        assert_eq!(decode_calls_for_tests(), 2);
        assert_eq!(cached_bytes_for_tests(), 0);
        assert_eq!(cached_entries_for_tests(), 0);
    }
}
