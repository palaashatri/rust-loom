//! Decoded worksheet images shared by every projection.
//!
//! A full projection used to decode every image in the sheet and copy each
//! RGBA buffer into a fresh Slint handle, including objects scrolled far out of
//! sight. Decoded images are immutable document content, so they now live in a
//! bounded LRU keyed by content: embedded buffers by their shared immutable
//! identity (pointer and length), path-backed images by path plus file
//! identity. Moving, reordering, undoing or re-projecting the same image
//! reuses the decoded pixels instead of decoding and copying again. A raster
//! larger than `MAX_DECODED_EDGE` is retained as a bounded display copy, so it
//! is reused like any other image rather than decoded on every projection.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use loom_sheets_core::{SheetObject, SheetObjectKind};
use slint::{Image, Rgba8Pixel, SharedPixelBuffer};

/// Decoded RGBA bytes retained across projections. Roughly eight 1280x800
/// images fit. Every decoded image is capped at `MAX_DECODED_EDGE`, so each one
/// fits and is retained; an oversized original is shown from its bounded copy
/// instead of being decoded again on every projection.
const CACHE_BUDGET_BYTES: usize = 32 * 1024 * 1024;
/// Entry ceiling so many tiny images cannot crowd the LRU bookkeeping.
const CACHE_ENTRIES: usize = 16;
/// Longest decoded edge kept for display. A worksheet object is drawn at
/// spreadsheet scale, so an oversized raster is shown from a copy scaled to
/// this edge: at most 16 MiB of RGBA. Stored document bytes and XLSX export
/// keep the original image untouched.
const MAX_DECODED_EDGE: u32 = 2048;
/// Largest decoded RGBA raster the decoder may allocate: 4096 x 4096, 64 MiB.
/// The declared size is read from the file header before any pixel buffer is
/// allocated, so a larger raster is refused outright. The decoder's own
/// allocation limit is twice this. The RGBA conversion that follows adds at most
/// this figure again. Peak memory for one decode is therefore a few multiples
/// of this value; the cache budget is not a hard memory limit.
const MAX_SOURCE_DECODE_BYTES: u64 = 64 * 1024 * 1024;

// A bounded image must always fit the budget, or it would silently go uncached.
const _: () =
    assert!(MAX_DECODED_EDGE as usize * MAX_DECODED_EDGE as usize * 4 <= CACHE_BUDGET_BYTES);

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
        // Defensive: decoded rasters are bounded by MAX_DECODED_EDGE, so only a
        // budget smaller than one bounded image reaches this guard. Such an
        // image is returned uncached rather than evicting the whole cache.
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
    match decode_raster(bytes) {
        Some(decoded) => bounded_image(decoded.into_rgba8()),
        None => Image::default(),
    }
}

/// Decodes an embedded raster. Returns `None` when it is undecodable or when its
/// header declares a size above `MAX_SOURCE_DECODE_BYTES`. A refused image shows
/// as an empty object, the same as an undecodable one.
fn decode_raster(bytes: &[u8]) -> Option<image::DynamicImage> {
    let reader = || {
        image::ImageReader::new(std::io::Cursor::new(bytes))
            .with_guessed_format()
            .ok()
    };
    let (width, height) = reader()?.into_dimensions().ok()?;
    if !within_decode_budget(width, height) {
        return None;
    }
    let mut reader = reader()?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(MAX_SOURCE_DECODE_BYTES * 2);
    reader.limits(limits);
    reader.decode().ok()
}

/// Whether a raster of this declared size may be decoded. Both sides fit in
/// `u64`, so the comparison cannot overflow at the `u32` extremes.
fn within_decode_budget(width: u32, height: u32) -> bool {
    u64::from(width) * u64::from(height) <= MAX_SOURCE_DECODE_BYTES / 4
}

/// Copies a decoded raster into a Slint handle no larger than `MAX_DECODED_EDGE`.
///
/// The full decode is still allocated once per image identity; what is kept,
/// retained and re-projected is this bounded copy. An image already within the
/// limit is copied exactly, without resampling.
fn bounded_image(rgba: image::RgbaImage) -> Image {
    let (width, height) = rgba.dimensions();
    let longest = width.max(height);
    let rgba = if longest > MAX_DECODED_EDGE {
        image::imageops::thumbnail(
            &rgba,
            scaled_edge(width, longest),
            scaled_edge(height, longest),
        )
    } else {
        rgba
    };
    Image::from_rgba8(SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
        rgba.as_raw(),
        rgba.width(),
        rgba.height(),
    ))
}

/// `edge` scaled so that `longest` maps onto `MAX_DECODED_EDGE`, keeping the
/// aspect ratio. Integer arithmetic keeps the longest edge exact.
fn scaled_edge(edge: u32, longest: u32) -> u32 {
    let scaled = u64::from(edge) * u64::from(MAX_DECODED_EDGE) / u64::from(longest);
    u32::try_from(scaled).unwrap_or(MAX_DECODED_EDGE).max(1)
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

    fn solid_png(width: u32, height: u32) -> Vec<u8> {
        let image = image::RgbaImage::from_pixel(width, height, image::Rgba([12, 34, 56, 255]));
        let mut bytes = Vec::new();
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
            .expect("encode solid image");
        bytes
    }

    #[test]
    fn oversized_image_is_decoded_once_and_retained_at_bounded_size() {
        // 3000x3000 RGBA is 36 MB decoded, above the whole 32 MiB budget. It was
        // decoded again on every projection and never retained.
        reset_cache_for_tests(CACHE_BUDGET_BYTES);
        let huge = object(solid_png(3000, 3000), "huge.png");
        let first = image_for(&huge);
        let second = image_for(&huge);
        assert_eq!(
            decode_calls_for_tests(),
            1,
            "a repeated projection must reuse the bounded decoded copy"
        );
        assert_eq!(first.size(), second.size());
        assert_eq!(first.size().width, MAX_DECODED_EDGE);
        assert_eq!(first.size().height, MAX_DECODED_EDGE);
        assert_eq!(cached_entries_for_tests(), 1);
        assert!(cached_bytes_for_tests() <= CACHE_BUDGET_BYTES);
    }

    #[test]
    fn oversized_image_keeps_its_aspect_ratio() {
        reset_cache_for_tests(CACHE_BUDGET_BYTES);
        let wide = image_for(&object(solid_png(4000, 1000), "wide.png"));
        assert_eq!(wide.size().width, MAX_DECODED_EDGE);
        assert_eq!(wide.size().height, MAX_DECODED_EDGE / 4);
    }

    #[test]
    fn image_at_the_edge_limit_is_not_resampled() {
        reset_cache_for_tests(CACHE_BUDGET_BYTES);
        let edge = image_for(&object(solid_png(MAX_DECODED_EDGE, 16), "edge.png"));
        assert_eq!(edge.size().width, MAX_DECODED_EDGE);
        assert_eq!(edge.size().height, 16);
    }

    #[test]
    fn the_decode_budget_is_inclusive_and_refuses_larger_declared_sizes() {
        // 4096 x 4096 RGBA is exactly the 64 MiB budget and is allowed.
        assert!(within_decode_budget(4096, 4096));
        assert!(!within_decode_budget(4097, 4096));
        assert!(!within_decode_budget(10_000, 10_000));
        // The extremes must not overflow the comparison. A one-pixel-wide strip of
        // u32::MAX rows is still about 4 billion pixels and must be refused.
        assert!(!within_decode_budget(u32::MAX, u32::MAX));
        assert!(!within_decode_budget(1, u32::MAX));
        assert!(within_decode_budget(1, 16_777_216));
    }

    /// A PNG whose header declares `width` x `height` RGBA, with a valid but empty
    /// image stream. It is never fully decoded, so the test stays cheap.
    fn png_declaring(width: u32, height: u32) -> Vec<u8> {
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&width.to_be_bytes());
        ihdr.extend_from_slice(&height.to_be_bytes());
        // 8-bit RGBA, deflate, adaptive filtering, no interlace.
        ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
        // A zlib stream holding one final stored block of zero bytes.
        let empty_stream = [
            0x78, 0x01, 0x01, 0x00, 0x00, 0xff, 0xff, 0x00, 0x00, 0x00, 0x01,
        ];
        let chunks: [(&[u8; 4], &[u8]); 3] =
            [(b"IHDR", &ihdr), (b"IDAT", &empty_stream), (b"IEND", &[])];
        let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
        for (kind, data) in chunks {
            bytes.extend_from_slice(&(data.len() as u32).to_be_bytes());
            let mut body = kind.to_vec();
            body.extend_from_slice(data);
            bytes.extend_from_slice(&body);
            bytes.extend_from_slice(&crc32(&body).to_be_bytes());
        }
        bytes
    }

    fn crc32(data: &[u8]) -> u32 {
        let mut crc = u32::MAX;
        for &byte in data {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                crc = if crc & 1 == 1 {
                    (crc >> 1) ^ 0xEDB8_8320
                } else {
                    crc >> 1
                };
            }
        }
        !crc
    }

    #[test]
    fn an_oversized_decode_is_refused_before_pixels_are_allocated() {
        // A real 3000 x 3000 image is 36 MB decoded: inside the budget, so it decodes.
        assert!(decode_raster(&solid_png(3000, 3000)).is_some());
        // The header declares 5000 x 5000 (100 MB decoded) with no pixel data.
        let header = png_declaring(5000, 5000);
        let declared = image::ImageReader::new(Cursor::new(&header))
            .with_guessed_format()
            .expect("format")
            .into_dimensions()
            .expect("the header is readable");
        assert_eq!(
            declared,
            (5000, 5000),
            "the refusal must come from the declared size"
        );
        assert!(decode_raster(&header).is_none());
        assert_eq!(image_for(&object(header, "refused.png")).size().width, 0);
    }
}
