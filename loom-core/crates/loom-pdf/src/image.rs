//! Raster images for the PDF writer.
//!
//! JPEG data is embedded as-is (`DCTDecode`); decoded pixels are stored as raw 8-bit RGB with
//! an optional alpha plane written as a soft mask. Nothing is compressed beyond JPEG, so
//! callers should downscale very large pictures before embedding them.

use crate::{PageIndex, PdfDocument};

/// How an image's samples are stored.
#[derive(Debug, Clone)]
enum PdfImageData {
    /// A complete baseline/progressive JPEG file with `components` (1 or 3) colour channels.
    Jpeg { bytes: Vec<u8>, components: u8 },
    /// Raw 8-bit RGB samples, `width * height * 3` bytes, plus optional 8-bit alpha.
    Rgb {
        rgb: Vec<u8>,
        alpha: Option<Vec<u8>>,
    },
}

/// An image that can be drawn on any page of one document.
#[derive(Debug, Clone)]
pub struct PdfImage {
    width: u32,
    height: u32,
    data: PdfImageData,
}

impl PdfImage {
    /// A JPEG file with 1 (gray) or 3 (RGB) components.
    pub fn jpeg(width: u32, height: u32, components: u8, bytes: Vec<u8>) -> Result<Self, String> {
        if width == 0 || height == 0 || !matches!(components, 1 | 3) {
            return Err("unsupported JPEG image".into());
        }
        Ok(Self {
            width,
            height,
            data: PdfImageData::Jpeg { bytes, components },
        })
    }

    /// Raw RGB pixels (3 bytes each); `alpha` is one byte per pixel when present.
    pub fn rgb(
        width: u32,
        height: u32,
        rgb: Vec<u8>,
        alpha: Option<Vec<u8>>,
    ) -> Result<Self, String> {
        let pixels = width as usize * height as usize;
        if width == 0
            || height == 0
            || rgb.len() != pixels * 3
            || alpha.as_ref().is_some_and(|a| a.len() != pixels)
        {
            return Err("image sample count does not match its size".into());
        }
        Ok(Self {
            width,
            height,
            data: PdfImageData::Rgb { rgb, alpha },
        })
    }

    fn has_alpha(&self) -> bool {
        matches!(&self.data, PdfImageData::Rgb { alpha: Some(_), .. })
    }

    /// The image XObject stream (and its soft-mask stream when it has alpha).
    pub(crate) fn objects(&self, smask_ref: Option<i64>) -> (Vec<u8>, Option<Vec<u8>>) {
        let (colour, filter, bytes) = match &self.data {
            PdfImageData::Jpeg { bytes, components } => (
                if *components == 1 {
                    "/DeviceGray"
                } else {
                    "/DeviceRGB"
                },
                " /Filter /DCTDecode",
                bytes.as_slice(),
            ),
            PdfImageData::Rgb { rgb, .. } => ("/DeviceRGB", "", rgb.as_slice()),
        };
        let mask = smask_ref
            .map(|r| format!(" /SMask {r} 0 R"))
            .unwrap_or_default();
        let image = stream(
            &format!(
                "<< /Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace {colour} /BitsPerComponent 8{filter}{mask} /Length {} >>",
                self.width,
                self.height,
                bytes.len()
            ),
            bytes,
        );
        let soft_mask = match &self.data {
            PdfImageData::Rgb {
                alpha: Some(alpha), ..
            } => Some(stream(
                &format!(
                    "<< /Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceGray /BitsPerComponent 8 /Length {} >>",
                    self.width,
                    self.height,
                    alpha.len()
                ),
                alpha,
            )),
            _ => None,
        };
        (image, soft_mask)
    }

    pub(crate) fn needs_mask(&self) -> bool {
        self.has_alpha()
    }
}

fn stream(dict: &str, bytes: &[u8]) -> Vec<u8> {
    let mut out = format!("{dict}\nstream\n").into_bytes();
    out.extend_from_slice(bytes);
    out.extend_from_slice(b"\nendstream");
    out
}

impl PdfDocument {
    /// Registers an image; the returned id can be drawn on any page.
    pub fn add_image(&mut self, image: PdfImage) -> usize {
        self.images.push(image);
        self.images.len() - 1
    }

    /// Draws an image into the local rectangle `(0, 0, width, height)` of a y-down
    /// coordinate system placed by `transform` (the same convention as
    /// [`PdfDocument::draw_rect_with_transform`]).
    pub fn draw_image_with_transform(
        &mut self,
        page: PageIndex,
        image: usize,
        size: (f32, f32),
        transform: [f32; 6],
    ) {
        if image >= self.images.len() {
            return;
        }
        let [a, b, c, d, e, f] = transform;
        let (w, h) = size;
        let p = self.page_mut(page);
        if !p.images.contains(&image) {
            p.images.push(image);
        }
        // The image's unit square is y-up; flip it into the y-down local space.
        p.ops.push(format!(
            "q {a:.5} {b:.5} {c:.5} {d:.5} {e:.5} {f:.5} cm {w:.4} 0 0 {:.4} 0 {h:.4} cm /Im{image} Do Q",
            -h
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contains(haystack: &[u8], needle: &[u8]) -> bool {
        haystack.windows(needle.len()).any(|w| w == needle)
    }

    #[test]
    fn jpeg_image_is_an_xobject_drawn_on_the_page() {
        let mut doc = PdfDocument::new();
        let page = doc.add_page(200.0, 100.0);
        let id =
            doc.add_image(PdfImage::jpeg(4, 2, 3, vec![0xFF, 0xD8, 1, 2, 3, 0xFF, 0xD9]).unwrap());
        doc.draw_image_with_transform(page, id, (40.0, 20.0), [1.0, 0.0, 0.0, -1.0, 10.0, 90.0]);
        let out = doc.serialize();
        assert!(contains(&out, b"/XObject << /Im0 10 0 R >>"));
        assert!(contains(
            &out,
            b"/Subtype /Image /Width 4 /Height 2 /ColorSpace /DeviceRGB"
        ));
        assert!(contains(&out, b"/Filter /DCTDecode"));
        assert!(contains(&out, &[0xFF, 0xD8, 1, 2, 3, 0xFF, 0xD9]));
        assert!(contains(&out, b"/Im0 Do Q"));
        // Object 10 (= 8 + 2n for one page) is the image; the xref covers every object.
        assert!(contains(&out, b"\n10 0 obj\n<< /Type /XObject"));
        assert!(contains(&out, b"/Size 11 "));
    }

    #[test]
    fn rgba_image_gets_a_soft_mask_and_validates_sample_counts() {
        assert!(PdfImage::rgb(2, 1, vec![0; 5], None).is_err());
        assert!(PdfImage::rgb(2, 1, vec![0; 6], Some(vec![0; 1])).is_err());
        let mut doc = PdfDocument::new();
        let page = doc.add_page(50.0, 50.0);
        let id = doc.add_image(PdfImage::rgb(2, 1, vec![9; 6], Some(vec![255, 128])).unwrap());
        doc.draw_image_with_transform(page, id, (10.0, 5.0), [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
        let out = doc.serialize();
        assert!(contains(&out, b"/SMask 11 0 R"));
        assert!(contains(
            &out,
            b"
11 0 obj
<< /Type /XObject /Subtype /Image /Width 2 /Height 1 /ColorSpace /DeviceGray"
        ));
        assert!(contains(&out, &[255, 128]));
        assert!(contains(&out, b"/Size 12 "));
    }

    #[test]
    fn unused_images_are_not_listed_on_pages_and_output_is_deterministic() {
        let build = || {
            let mut doc = PdfDocument::new();
            let first = doc.add_page(50.0, 50.0);
            let second = doc.add_page(50.0, 50.0);
            let id = doc.add_image(PdfImage::rgb(1, 1, vec![1, 2, 3], None).unwrap());
            doc.draw_image_with_transform(second, id, (5.0, 5.0), [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
            let _ = first;
            doc.serialize()
        };
        let out = build();
        assert_eq!(out, build());
        let text = String::from_utf8_lossy(&out);
        assert_eq!(text.matches("/XObject <<").count(), 1);
    }
}
