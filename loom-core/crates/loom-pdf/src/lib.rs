//! Deterministic PDF 1.4 writer with embedded fonts.
//!
//! Used by Loom Writer, Present and Sheets for PDF export. Scope is
//! deliberately small (see the Internal PDF Writer ADR in root `AGENTS.md`):
//!
//! * Pages with Unicode text, rectangles, lines, RGB fill and stroke colors.
//!   Text is set in the bundled Inter faces (regular, bold, italic, bold
//!   italic), embedded as subset TrueType `CIDFontType2` programs with a
//!   `ToUnicode` CMap. Measurement ([`text_width_pt`]) reads the same glyph
//!   advances the page uses.
//! * JPEG and raw RGB(A) images (see [`PdfImage`]); no interactive features.
//! * Deterministic output: no timestamps are written unless the caller
//!   provides one (`PdfDocument::set_creation_date`).
//!
//! Inter has no CJK glyphs and Loom bundles no fallback font: such characters
//! show Inter's `.notdef` box unless a [`FontFallback`] is installed with
//! [`PdfDocument::set_font_fallback`].
//!
//! The output is validated in tests by re-parsing the xref table and object
//! bodies and by reading the text back through [`inspect`].

mod embed;
mod fonts;
mod image;
pub mod inspect;
#[cfg(test)]
mod tests;

pub use fonts::{FallbackFont, FontFallback};
pub use image::PdfImage;

use std::collections::BTreeMap;
use std::sync::Arc;

use embed::{deflate, stream_object, FaceSlot};

/// A text string, optionally styled.
#[derive(Debug, Clone)]
pub struct TextStyle {
    /// Font size in points.
    pub size_pt: f32,
    /// RGB stroke color 0..=1 used for fills.
    pub fill_rgb: (f32, f32, f32),
    /// Bold (Inter Bold).
    pub bold: bool,
    /// Italic (Inter Italic; bold and italic together use Inter Bold Italic).
    pub italic: bool,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            size_pt: 12.0,
            fill_rgb: (0.0, 0.0, 0.0),
            bold: false,
            italic: false,
        }
    }
}

/// Appearance of a vector primitive (rectangle or line).
#[derive(Debug, Clone, Copy)]
pub struct PathStyle {
    /// RGB color 0..=1.
    pub rgb: (f32, f32, f32),
    /// Stroke width in points (ignored when `filled`).
    pub width: f32,
    /// Fill the shape instead of stroking it.
    pub filled: bool,
}

impl PathStyle {
    /// A filled shape in the given RGB color.
    pub fn filled(rgb: (f32, f32, f32)) -> Self {
        Self {
            rgb,
            width: 1.0,
            filled: true,
        }
    }

    /// A stroked outline with the given color and width.
    pub fn stroked(rgb: (f32, f32, f32), width: f32) -> Self {
        Self {
            rgb,
            width,
            filled: false,
        }
    }
}

/// One page's content stream (operator text).
#[derive(Debug, Default)]
struct Page {
    width_pt: f32,
    height_pt: f32,
    ops: Vec<String>,
    /// Indexes of the document images this page draws.
    images: Vec<usize>,
}

/// A handle to a page being built.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PageIndex(pub usize);

/// A deterministic PDF document builder.
#[derive(Default)]
pub struct PdfDocument {
    pages: Vec<Page>,
    creation_date: String,
    images: Vec<PdfImage>,
    /// Fonts used so far, in first-use order. Inter faces take `F1`..`F4`
    /// (regular, bold, italic, bold italic); fallback faces follow.
    faces: Vec<FaceSlot>,
    fallback: Option<Arc<dyn FontFallback>>,
}

impl std::fmt::Debug for PdfDocument {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PdfDocument")
            .field("pages", &self.pages)
            .field("images", &self.images.len())
            .field("fonts", &self.faces.len())
            .finish()
    }
}

impl PdfDocument {
    /// Create a new document. Deterministic by default: the creation date
    /// is a fixed value unless overridden with [`Self::set_creation_date`].
    pub fn new() -> Self {
        Self {
            creation_date: "(D:20260101000000Z)".to_string(),
            ..Self::default()
        }
    }

    /// Override the PDF creation date (PDF string syntax, e.g.
    /// `(D:20240101120000Z)`). Affects output determinism.
    pub fn set_creation_date(&mut self, date: impl Into<String>) {
        self.creation_date = date.into();
    }

    /// Install the source of fonts for characters Inter cannot show. The hook
    /// only affects drawing; [`text_width_pt`] still measures those characters
    /// with Inter's `.notdef` advance.
    pub fn set_font_fallback(&mut self, fallback: Arc<dyn FontFallback>) {
        self.fallback = Some(fallback);
    }

    /// Add a page of the given size (points).
    pub fn add_page(&mut self, width_pt: f32, height_pt: f32) -> PageIndex {
        self.pages.push(Page {
            width_pt,
            height_pt,
            ops: Vec::new(),
            images: Vec::new(),
        });
        PageIndex(self.pages.len() - 1)
    }

    fn page_mut(&mut self, page: PageIndex) -> &mut Page {
        &mut self.pages[page.0]
    }

    /// The slot for the font that shows `ch` in `style`, creating it on first use.
    fn slot_for(&mut self, ch: char, style: &TextStyle) -> usize {
        let index = fonts::inter_index(style.bold, style.italic);
        let inter = fonts::inter(index);
        let mut chosen = inter.clone();
        if inter.glyph(ch).is_none() {
            let extra = self
                .fallback
                .as_ref()
                .and_then(|hook| hook.font_for(ch, style.bold, style.italic))
                .and_then(|face| fonts::Font::new(face.name, face.data))
                .filter(|font| font.glyph(ch).is_some());
            if let Some(font) = extra {
                chosen = font;
            }
        }
        if let Some(at) = self.faces.iter().position(|s| s.font_name() == chosen.name) {
            return at;
        }
        let resource = if chosen.name == inter.name {
            format!("F{}", index + 1)
        } else {
            let custom = self
                .faces
                .iter()
                .filter(|s| !fonts::is_bundled(s.font_name()))
                .count();
            format!("F{}", 5 + custom)
        };
        self.faces.push(FaceSlot::new(resource, chosen));
        self.faces.len() - 1
    }

    /// Encode `text` as glyph strings. Returns the `Tf` operator for the first
    /// face and the `Tj` sequence (later faces switch with their own `Tf`),
    /// or `None` when nothing visible remains.
    fn show_text(&mut self, text: &str, style: &TextStyle) -> Option<(String, String)> {
        let size = format!("{:.2}", style.size_pt);
        let mut runs: Vec<(usize, String)> = Vec::new();
        for ch in text.chars() {
            if fonts::is_invisible_control(ch) {
                continue;
            }
            let slot = self.slot_for(ch, style);
            let glyph = format!("{:04X}", self.faces[slot].encode(ch));
            match runs.last_mut() {
                Some((last, hex)) if *last == slot => hex.push_str(&glyph),
                _ => runs.push((slot, glyph)),
            }
        }
        let mut runs = runs.into_iter();
        let (first, hex) = runs.next()?;
        let select = |slot: usize| format!("/{} {size} Tf", self.faces[slot].resource);
        let mut shown = format!("<{hex}> Tj");
        for (slot, hex) in runs {
            shown.push_str(&format!(" {} <{hex}> Tj", select(slot)));
        }
        Some((select(first), shown))
    }

    /// Draw text at the baseline position `(x, y)` (bottom-left origin,
    /// matching PDF user space).
    pub fn draw_text(&mut self, page: PageIndex, x: f32, y: f32, text: &str, style: &TextStyle) {
        let Some((font, shown)) = self.show_text(text, style) else {
            return;
        };
        let (r, g, b) = style.fill_rgb;
        let (r, g, b) = (clip01(r), clip01(g), clip01(b));
        let p = self.page_mut(page);
        p.ops.push(format!(
            "{} {} {} rg {font} BT {:.2} {:.2} Td {shown} ET",
            fmt3(r),
            fmt3(g),
            fmt3(b),
            x,
            y,
        ));
    }

    /// Draw text in a caller-supplied PDF transformation matrix. The matrix
    /// is `[a, b, c, d, e, f]` as defined by the PDF `cm` operator and is
    /// applied to the text's local coordinates. Keeping the operation here
    /// lets scene exporters preserve position, scale, and rotation without
    /// reaching into the PDF page stream.
    pub fn draw_text_with_transform(
        &mut self,
        page: PageIndex,
        x: f32,
        y: f32,
        text: &str,
        style: &TextStyle,
        transform: [f32; 6],
    ) {
        let Some((font, shown)) = self.show_text(text, style) else {
            return;
        };
        let (r, g, b) = (
            clip01(style.fill_rgb.0),
            clip01(style.fill_rgb.1),
            clip01(style.fill_rgb.2),
        );
        let [a, b_matrix, c, d, e, f] = transform;
        let p = self.page_mut(page);
        p.ops.push(format!(
            "q {:.5} {:.5} {:.5} {:.5} {:.5} {:.5} cm {} {} {} rg {font} BT 1 0 0 -1 {:.2} {:.2} Tm {shown} ET Q",
            a,
            b_matrix,
            c,
            d,
            e,
            f,
            fmt3(r),
            fmt3(g),
            fmt3(b),
            x,
            y,
        ));
    }

    /// Draw a filled or stroked rectangle at `(x, y)` (bottom-left) of the
    /// given size, using [`PathStyle`].
    pub fn draw_rect(&mut self, page: PageIndex, x: f32, y: f32, w: f32, h: f32, style: PathStyle) {
        let (r, g, b) = (
            clip01(style.rgb.0),
            clip01(style.rgb.1),
            clip01(style.rgb.2),
        );
        let op = if style.filled { "f" } else { "S" };
        let p = self.page_mut(page);
        p.ops.push(format!(
            "{} {} {} rg {:.2} {:.2} {:.2} {:.2} re {op}",
            fmt3(r),
            fmt3(g),
            fmt3(b),
            x,
            y,
            w,
            h
        ));
    }

    /// Draw a rectangle in a caller-supplied PDF transformation matrix. The
    /// rectangle is emitted in local coordinates and the matrix carries the
    /// scene position, scale, and rotation.
    pub fn draw_rect_with_transform(
        &mut self,
        page: PageIndex,
        rect: (f32, f32, f32, f32),
        style: PathStyle,
        transform: [f32; 6],
    ) {
        let (x, y, w, h) = rect;
        let (r, g, b) = (
            clip01(style.rgb.0),
            clip01(style.rgb.1),
            clip01(style.rgb.2),
        );
        let op = if style.filled { "f" } else { "S" };
        let [a, b_matrix, c, d, e, f] = transform;
        let p = self.page_mut(page);
        p.ops.push(format!(
            "q {:.5} {:.5} {:.5} {:.5} {:.5} {:.5} cm {} {} {} rg {:.2} {:.2} {:.2} {:.2} re {op} Q",
            a,
            b_matrix,
            c,
            d,
            e,
            f,
            fmt3(r),
            fmt3(g),
            fmt3(b),
            x,
            y,
            w,
            h
        ));
    }

    /// Draw a line segment with the given stroke width and color.
    pub fn draw_line(
        &mut self,
        page: PageIndex,
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        style: PathStyle,
    ) {
        let (r, g, b) = (
            clip01(style.rgb.0),
            clip01(style.rgb.1),
            clip01(style.rgb.2),
        );
        let p = self.page_mut(page);
        p.ops.push(format!(
            "{} {} {} RG {:.2} w {:.2} {:.2} m {:.2} {:.2} l S",
            fmt3(r),
            fmt3(g),
            fmt3(b),
            style.width,
            x1,
            y1,
            x2,
            y2
        ));
    }

    /// Serialize the document to PDF bytes.
    ///
    /// Output is byte-for-byte deterministic for the same input.
    pub fn serialize(&self) -> Vec<u8> {
        let n = self.pages.len();
        // Object layout:
        //   1               catalog
        //   2..2+n          pages
        //   2+n             page tree
        //   3+n..3+2n       content streams (page i is 3+n+i)
        //   3+2n            info object
        //   4+2n..          image XObjects, their soft masks, then font objects
        let pages_ref = 2 + n;
        let stream_ref = |i: usize| 3 + n + i;
        let info_ref = 3 + 2 * n;
        let image_ref = |i: usize| 4 + 2 * n + i;
        let mut next = 4 + 2 * n + self.images.len();
        let mask_refs: Vec<Option<usize>> = self
            .images
            .iter()
            .map(|image| {
                image.needs_mask().then(|| {
                    let this = next;
                    next += 1;
                    this
                })
            })
            .collect();

        // Font objects: a failed subset adds a CIDToGIDMap stream, so settle
        // the programs first and number the objects from their counts.
        let subsets: Vec<Option<Vec<u8>>> = self.faces.iter().map(FaceSlot::subset).collect();
        let mut font_refs = Vec::with_capacity(self.faces.len());
        for subset in &subsets {
            font_refs.push(next);
            next += 5 + usize::from(subset.is_none());
        }
        let font_resources = if self.faces.is_empty() {
            String::new()
        } else {
            let list: Vec<String> = self
                .faces
                .iter()
                .zip(&font_refs)
                .map(|(face, at)| format!("/{} {at} 0 R", face.resource))
                .collect();
            format!(" /Font << {} >>", list.join(" "))
        };

        let mut objects: Vec<Vec<u8>> = Vec::new();
        objects.push(format!("<< /Type /Catalog /Pages {pages_ref} 0 R >>").into_bytes());
        for (i, p) in self.pages.iter().enumerate() {
            let xobjects = if p.images.is_empty() {
                String::new()
            } else {
                let list: Vec<String> = p
                    .images
                    .iter()
                    .map(|i| format!("/Im{i} {} 0 R", image_ref(*i)))
                    .collect();
                format!(" /XObject << {} >>", list.join(" "))
            };
            objects.push(
                format!(
                    "<< /Type /Page /Parent {pages_ref} 0 R /MediaBox [0 0 {:.2} {:.2}] \
                     /Resources <<{font_resources}{xobjects} >> /Contents {} 0 R >>",
                    p.width_pt,
                    p.height_pt,
                    stream_ref(i)
                )
                .into_bytes(),
            );
        }
        objects.push(
            format!(
                "<< /Type /Pages /Kids [{}] /Count {} >>",
                (2..2 + n)
                    .map(|i| format!("{i} 0 R"))
                    .collect::<Vec<_>>()
                    .join(" "),
                n
            )
            .into_bytes(),
        );
        for p in &self.pages {
            objects.push(stream_object(
                "/Filter /FlateDecode",
                &deflate(p.ops.join("\n").as_bytes()),
            ));
        }
        objects.push(
            format!(
                "<< /Producer (Loom) /Creator (Loom) /CreationDate {} >>",
                self.creation_date
            )
            .into_bytes(),
        );
        debug_assert_eq!(objects.len(), info_ref);
        let mut masks: Vec<Vec<u8>> = Vec::new();
        for (image, mask_ref) in self.images.iter().zip(&mask_refs) {
            let (object, mask) = image.objects(mask_ref.map(|m| m as i64));
            objects.push(object);
            masks.extend(mask);
        }
        objects.extend(masks);
        for ((face, first), subset) in self.faces.iter().zip(&font_refs).zip(subsets) {
            objects.extend(face.objects_with(*first, subset));
        }

        // Assemble with an xref table.
        let mut out = Vec::new();
        out.extend_from_slice(b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n");
        let mut offsets: BTreeMap<usize, usize> = BTreeMap::new();
        for (i, obj) in objects.iter().enumerate() {
            offsets.insert(1 + i, out.len());
            out.extend_from_slice(format!("{} 0 obj\n", 1 + i).as_bytes());
            out.extend_from_slice(obj);
            out.extend_from_slice(b"\nendobj\n");
        }
        let xref_pos = out.len();
        out.extend_from_slice(format!("xref\n0 {}\n", objects.len() + 1).as_bytes());
        out.extend_from_slice(b"0000000000 65535 f \n");
        for i in 1..=objects.len() {
            out.extend_from_slice(format!("{:010} 00000 n \n", offsets[&i]).as_bytes());
        }
        out.extend_from_slice(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R /Info {info_ref} 0 R >>\nstartxref\n{xref_pos}\n%%EOF\n",
                objects.len() + 1,
            )
            .as_bytes(),
        );
        out
    }
}

fn clip01(v: f32) -> f32 {
    v.clamp(0.0, 1.0)
}

fn fmt3(v: f32) -> String {
    format!("{:.3}", (v * 1000.0).round() / 1000.0)
}

/// Width of `text` in points when set in the Inter face `style` selects.
///
/// Advances come from the font program the PDF embeds, so layout that wraps
/// with this function matches what a viewer draws. Kerning is not applied,
/// matching a plain `Tj`. Characters Inter lacks measure as its `.notdef` box
/// (the width they are drawn with unless a fallback font is installed), and
/// control characters measure zero.
pub fn text_width_pt(text: &str, style: &TextStyle) -> f32 {
    let font = fonts::inter(fonts::inter_index(style.bold, style.italic));
    let units: u32 = text
        .chars()
        .map(|ch| u32::from(font.char_advance_units(ch)))
        .sum();
    units as f32 * style.size_pt / font.units_per_em()
}

#[cfg(test)]
pub mod debug_export {
    pub use super::*;
}
