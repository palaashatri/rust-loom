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
//! # What is drawn
//!
//! Drawing and measuring first prepare the text identically: invisible format
//! characters (controls, soft hyphen, zero-width and directional marks,
//! variation selectors, line and paragraph separators) are removed, emoji
//! sequences collapse to their base character, and the result is put in
//! canonical composed form (NFC), so `e` + U+0301 is the single glyph `é`.
//! Inter's presentation-form ligature code points it has no glyph for are
//! shown as their letters (`fi`), without a fallback font.
//!
//! # Copying text out
//!
//! Each embedded glyph has one `ToUnicode` entry: the first character that was
//! drawn with it. When a different character is drawn with a glyph that
//! already has an entry (Inter draws U+2019 and U+02BC with one glyph, for
//! example) the glyph is wrapped in an `/ActualText` span carrying the
//! character that was written, so extraction returns what the document said.
//! `.notdef` boxes carry no text.
//!
//! Inter has no CJK glyphs and Loom bundles no fallback font: such characters
//! show Inter's `.notdef` box unless a [`FontFallback`] is installed with
//! [`PdfDocument::set_font_fallback`].
//!
//! The output is validated in tests by re-parsing the xref table and object
//! bodies and by reading the text back through `inspect`, which is part of the
//! public API only with the `test-support` feature.

mod compose_data;
mod embed;
mod fonts;
mod image;
#[cfg(any(test, feature = "test-support"))]
pub mod inspect;
#[cfg(test)]
mod tests;
mod text;

pub use fonts::{FallbackFont, FontFallback};
pub use image::PdfImage;

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::sync::Arc;

use embed::{deflate, stream_object, FaceSlot};

/// Smallest and largest page dimension the PDF 1.4 specification allows, in
/// points (3 to 14,400).
const PAGE_MIN_PT: f32 = 3.0;
const PAGE_MAX_PT: f32 = 14_400.0;
/// US Letter, used when a page size is not a usable number.
const DEFAULT_PAGE_PT: (f32, f32) = (612.0, 792.0);

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

/// How one character is set: with a single font slot, or as the plain
/// characters of its expansion (an index into `PdfDocument::expansions`).
#[derive(Debug, Clone, Copy)]
enum Resolved {
    One(usize),
    Many(usize),
}

/// A fallback font program the document has been offered, by name.
struct FallbackEntry {
    name: &'static str,
    /// Identity of the font bytes (address and length) that name was used for.
    data: (usize, usize),
    /// The parsed face, or `None` when it was refused.
    font: Option<fonts::Font>,
}

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
    /// How each (character, Inter face) has been set, so the fallback hook
    /// runs once per distinct character and style.
    resolved: HashMap<(char, usize), Resolved>,
    expansions: Vec<Vec<(usize, char)>>,
    fallback_fonts: Vec<FallbackEntry>,
    fallback_problems: Vec<String>,
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

    /// Why fallback faces the hook returned were not used (see
    /// [`FontFallback`]), in the order they were met, without repeats.
    pub fn fallback_problems(&self) -> &[String] {
        &self.fallback_problems
    }

    /// Add a page of the given size (points). A size that is not a finite
    /// number between 3 and 14,400 points (the PDF 1.4 limits) cannot be
    /// written; such a page is added at US Letter size instead. Use
    /// [`Self::try_add_page`] to reject it.
    pub fn add_page(&mut self, width_pt: f32, height_pt: f32) -> PageIndex {
        self.try_add_page(width_pt, height_pt)
            .unwrap_or_else(|_| self.push_page(DEFAULT_PAGE_PT.0, DEFAULT_PAGE_PT.1))
    }

    /// Add a page, rejecting a width or height that is NaN, infinite or
    /// outside 3..=14,400 points.
    pub fn try_add_page(&mut self, width_pt: f32, height_pt: f32) -> Result<PageIndex, String> {
        let usable = |v: f32| v.is_finite() && (PAGE_MIN_PT..=PAGE_MAX_PT).contains(&v);
        if !usable(width_pt) || !usable(height_pt) {
            return Err(format!(
                "page size {width_pt} x {height_pt} pt is outside {PAGE_MIN_PT}..={PAGE_MAX_PT}"
            ));
        }
        Ok(self.push_page(width_pt, height_pt))
    }

    fn push_page(&mut self, width_pt: f32, height_pt: f32) -> PageIndex {
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

    /// The slot of bundled face `index`, creating it on first use.
    fn inter_slot(&mut self, index: usize) -> usize {
        let name = fonts::inter(index).name;
        if let Some(at) = self.faces.iter().position(|s| s.font_name() == name) {
            return at;
        }
        self.faces.push(FaceSlot::new(
            format!("F{}", index + 1),
            fonts::inter(index).clone(),
        ));
        self.faces.len() - 1
    }

    /// The slot of an accepted fallback face, creating it on first use.
    fn fallback_slot(&mut self, font: fonts::Font) -> usize {
        if let Some(at) = self.faces.iter().position(|s| s.font_name() == font.name) {
            return at;
        }
        let custom = self
            .faces
            .iter()
            .filter(|s| !fonts::is_bundled(s.font_name()))
            .count();
        self.faces
            .push(FaceSlot::new(format!("F{}", 5 + custom), font));
        self.faces.len() - 1
    }

    fn note_problem(&mut self, problem: String) {
        if !self.fallback_problems.contains(&problem) {
            self.fallback_problems.push(problem);
        }
    }

    /// The accepted fallback face the hook names, parsed once per name.
    fn fallback_font(&mut self, wanted: FallbackFont) -> Option<fonts::Font> {
        let data = (wanted.data.as_ptr() as usize, wanted.data.len());
        if let Some(entry) = self.fallback_fonts.iter().find(|e| e.name == wanted.name) {
            if entry.data == data {
                return entry.font.clone();
            }
            let problem = format!(
                "fallback font name {:?} was given for two different font programs",
                wanted.name
            );
            self.note_problem(problem);
            return None;
        }
        let font = match fonts::validate_fallback(wanted) {
            Ok(font) => Some(font),
            Err(problem) => {
                self.note_problem(problem);
                None
            }
        };
        self.fallback_fonts.push(FallbackEntry {
            name: wanted.name,
            data,
            font: font.clone(),
        });
        font
    }

    /// How `ch` is set in `style`, decided once per character and style:
    /// Inter's glyph, else a fallback font's, else its plain expansion
    /// (`fi` for U+FB01), else Inter's `.notdef` box.
    fn resolve(&mut self, ch: char, style: &TextStyle) -> Resolved {
        let index = fonts::inter_index(style.bold, style.italic);
        if let Some(known) = self.resolved.get(&(ch, index)) {
            return *known;
        }
        let inter = fonts::inter(index);
        let resolved = if inter.glyph(ch).is_some() {
            Resolved::One(self.inter_slot(index))
        } else if let Some(font) = self.fallback_face_for(ch, style) {
            Resolved::One(self.fallback_slot(font))
        } else if let Some((chars, len)) = inter.expansion(ch) {
            let slot = self.inter_slot(index);
            self.expansions
                .push(chars[..len].iter().map(|part| (slot, *part)).collect());
            Resolved::Many(self.expansions.len() - 1)
        } else {
            Resolved::One(self.inter_slot(index))
        };
        self.resolved.insert((ch, index), resolved);
        resolved
    }

    /// The fallback face covering `ch`, when a hook is installed and accepted.
    fn fallback_face_for(&mut self, ch: char, style: &TextStyle) -> Option<fonts::Font> {
        let hook = self.fallback.clone()?;
        let wanted = hook.font_for(ch, style.bold, style.italic)?;
        let font = self.fallback_font(wanted)?;
        font.glyph(ch).is_some().then_some(font)
    }

    /// The glyph id to write for `ch` in `slot`, and `ch` itself when it needs
    /// an `/ActualText` span because the glyph's `ToUnicode` entry is another
    /// character.
    fn encode_char(&mut self, slot: usize, ch: char) -> (u16, Option<char>) {
        let face = &mut self.faces[slot];
        let gid = face.encode(ch);
        let actual = (gid != 0 && face.mapped_char(gid) != Some(ch)).then_some(ch);
        (gid, actual)
    }

    /// Encode `text` as glyph strings. Returns the `Tf` operator for the first
    /// face and the `Tj` sequence (later faces switch with their own `Tf`),
    /// or `None` when nothing visible remains.
    fn show_text(&mut self, text: &str, style: &TextStyle) -> Option<(String, String)> {
        let text = text::prepare(text);
        let size = if style.size_pt.is_finite() && style.size_pt > 0.0 {
            style.size_pt
        } else {
            TextStyle::default().size_pt
        };
        let size = format!("{size:.2}");
        let mut shown: Vec<(usize, u16, Option<char>)> = Vec::new();
        for ch in text.chars() {
            if text::is_not_drawn(ch) {
                continue;
            }
            match self.resolve(ch, style) {
                Resolved::One(slot) => {
                    let (gid, actual) = self.encode_char(slot, ch);
                    shown.push((slot, gid, actual));
                }
                Resolved::Many(at) => {
                    for part in 0..self.expansions[at].len() {
                        let (slot, plain) = self.expansions[at][part];
                        let (gid, actual) = self.encode_char(slot, plain);
                        shown.push((slot, gid, actual));
                    }
                }
            }
        }
        let first = shown.first()?.0;
        let select = |slot: usize| format!("/{} {size} Tf", self.faces[slot].resource);
        let mut out = String::new();
        let mut hex = String::new();
        let mut current = first;
        for (slot, gid, actual) in shown {
            if slot != current || actual.is_some() {
                if !hex.is_empty() {
                    let _ = write!(out, "<{hex}> Tj ");
                    hex.clear();
                }
                if slot != current {
                    let _ = write!(out, "{} ", select(slot));
                    current = slot;
                }
            }
            match actual {
                Some(ch) => {
                    let mut units = [0u16; 2];
                    let utf16: String = ch
                        .encode_utf16(&mut units)
                        .iter()
                        .map(|unit| format!("{unit:04X}"))
                        .collect();
                    let _ = write!(
                        out,
                        "/Span << /ActualText <FEFF{utf16}> >> BDC <{gid:04X}> Tj EMC "
                    );
                }
                None => {
                    let _ = write!(hex, "{gid:04X}");
                }
            }
        }
        if !hex.is_empty() {
            let _ = write!(out, "<{hex}> Tj ");
        }
        Some((select(first), out.trim_end().to_string()))
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
            finite(x),
            finite(y),
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
        let [a, b_matrix, c, d, e, f] = transform.map(finite);
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
            finite(x),
            finite(y),
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
            finite(x),
            finite(y),
            finite(w),
            finite(h)
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
        let [a, b_matrix, c, d, e, f] = transform.map(finite);
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
            finite(x),
            finite(y),
            finite(w),
            finite(h)
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
            finite(style.width),
            finite(x1),
            finite(y1),
            finite(x2),
            finite(y2)
        ));
    }

    /// Serialize the document to PDF bytes.
    ///
    /// Output is byte-for-byte deterministic for the same input. A document
    /// with no pages is written with one blank US Letter page, because a page
    /// tree with no pages is not a valid PDF.
    pub fn serialize(&self) -> Vec<u8> {
        let blank = [Page {
            width_pt: DEFAULT_PAGE_PT.0,
            height_pt: DEFAULT_PAGE_PT.1,
            ..Page::default()
        }];
        let pages: &[Page] = if self.pages.is_empty() {
            &blank
        } else {
            &self.pages
        };
        let n = pages.len();
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
        for (i, p) in pages.iter().enumerate() {
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
        for p in pages {
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

/// A colour component clamped to 0..=1; a NaN or infinite one is 0.
fn clip01(v: f32) -> f32 {
    if v.is_finite() {
        v.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// A coordinate or length for a content stream; NaN and infinities are not
/// numbers in PDF, so they become 0.
pub(crate) fn finite(v: f32) -> f32 {
    if v.is_finite() {
        v
    } else {
        0.0
    }
}

fn fmt3(v: f32) -> String {
    format!("{:.3}", (v * 1000.0).round() / 1000.0)
}

/// Width of `text` in points when set in the Inter face `style` selects.
///
/// Advances come from the font program the PDF embeds, so layout that wraps
/// with this function matches what a viewer draws. The text is prepared exactly
/// as drawing prepares it (invisible controls removed, NFC composition,
/// emoji sequences reduced), so an accent sequence measures as the one glyph
/// it is drawn as. Kerning is not applied, matching a plain `Tj`. Characters
/// Inter lacks measure as its `.notdef` box (the width they are drawn with
/// unless a fallback font is installed), except the ligature code points and
/// special spaces that are drawn as their plain letters and measure as them.
pub fn text_width_pt(text: &str, style: &TextStyle) -> f32 {
    let font = fonts::inter(fonts::inter_index(style.bold, style.italic));
    let units: u64 = text::prepare(text)
        .chars()
        .map(|ch| u64::from(font.char_advance_units(ch)))
        .sum();
    let width = units as f64 * f64::from(style.size_pt) / f64::from(font.units_per_em());
    if width.is_finite() {
        width as f32
    } else {
        0.0
    }
}

/// Whether [`text_width_pt`] is exact for `ch`: the bundled Inter faces have a
/// glyph for it, draw it as plain letters, or do not draw it at all. When this
/// is `false` the character is drawn with a fallback font or as `.notdef`, and
/// a caller that measures with its own estimate of such characters (a window
/// that has fonts this crate does not) can keep doing so.
pub fn is_measured_exactly(ch: char) -> bool {
    fonts::inter_measures_exactly(ch)
}

#[cfg(test)]
pub mod debug_export {
    pub use super::*;
}
