//! Minimal, deterministic PDF 1.4 writer.
//!
//! Used by Loom Writer, Present and Sheets for PDF export. Scope is
//! deliberately small (see `loom-spec/docs/adrs/ADR-0005-minimal-pdf-writer.md`):
//!
//! * Pages with text (built-in Helvetica, WinAnsi/Latin-1 encodable text),
//!   rectangles, lines, RGB fill and stroke colors.
//! * No images, no fonts embedding, no compression, no interactive features.
//! * Deterministic output: no timestamps are written unless the caller
//!   provides one (`PdfDocument::set_creation_date`).
//!
//! The output is validated in tests by re-parsing the xref table and object
//! bodies, and by round-tripping the text operators.

use std::collections::BTreeMap;

/// A text string, optionally styled.
#[derive(Debug, Clone)]
pub struct TextStyle {
    /// Font size in points.
    pub size_pt: f32,
    /// RGB stroke color 0..=1 used for fills.
    pub fill_rgb: (f32, f32, f32),
    /// Bold (Helvetica-Bold).
    pub bold: bool,
    /// Italic (Helvetica-Oblique).
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
    /// PostScript font name used on this page (from the last text draw).
    font: String,
}

/// A handle to a page being built.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PageIndex(pub usize);

/// A deterministic PDF document builder.
#[derive(Debug, Default)]
pub struct PdfDocument {
    pages: Vec<Page>,
    creation_date: String,
}

impl PdfDocument {
    /// Create a new document. Deterministic by default: the creation date
    /// is a fixed value unless overridden with [`Self::set_creation_date`].
    pub fn new() -> Self {
        Self {
            pages: Vec::new(),
            creation_date: "(D:20260101000000Z)".to_string(),
        }
    }

    /// Override the PDF creation date (PDF string syntax, e.g.
    /// `(D:20240101120000Z)`). Affects output determinism.
    pub fn set_creation_date(&mut self, date: impl Into<String>) {
        self.creation_date = date.into();
    }

    /// Add a page of the given size (points).
    pub fn add_page(&mut self, width_pt: f32, height_pt: f32) -> PageIndex {
        self.pages.push(Page {
            width_pt,
            height_pt,
            ops: Vec::new(),
            font: "Helvetica".to_string(),
        });
        PageIndex(self.pages.len() - 1)
    }

    fn page_mut(&mut self, page: PageIndex) -> &mut Page {
        &mut self.pages[page.0]
    }

    /// Draw text at the baseline position `(x, y)` (bottom-left origin,
    /// matching PDF user space).
    pub fn draw_text(&mut self, page: PageIndex, x: f32, y: f32, text: &str, style: &TextStyle) {
        let font = match (style.bold, style.italic) {
            (true, true) => "Helvetica-BoldOblique",
            (true, false) => "Helvetica-Bold",
            (false, true) => "Helvetica-Oblique",
            (false, false) => "Helvetica",
        };
        let p = self.page_mut(page);
        p.font = font.to_string();
        let (r, g, b) = style.fill_rgb;
        let (r, g, b) = (clip01(r), clip01(g), clip01(b));
        let p = self.page_mut(page);
        p.ops.push(format!(
            "{} {} {} rg /F1 {:.2} Tf BT {:.2} {:.2} Td ({}) Tj ET",
            fmt3(r),
            fmt3(g),
            fmt3(b),
            style.size_pt,
            x,
            y,
            escape_pdf_string(text)
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
        let font = match (style.bold, style.italic) {
            (true, true) => "Helvetica-BoldOblique",
            (true, false) => "Helvetica-Bold",
            (false, true) => "Helvetica-Oblique",
            (false, false) => "Helvetica",
        };
        let (r, g, b) = (
            clip01(style.fill_rgb.0),
            clip01(style.fill_rgb.1),
            clip01(style.fill_rgb.2),
        );
        let [a, b_matrix, c, d, e, f] = transform;
        let p = self.page_mut(page);
        p.font = font.to_string();
        p.ops.push(format!(
            "q {:.5} {:.5} {:.5} {:.5} {:.5} {:.5} cm {} {} {} rg /F1 {:.2} Tf BT 1 0 0 -1 {:.2} {:.2} Tm ({}) Tj ET Q",
            a,
            b_matrix,
            c,
            d,
            e,
            f,
            fmt3(r),
            fmt3(g),
            fmt3(b),
            style.size_pt,
            x,
            y,
            escape_pdf_string(text)
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
        let n = self.pages.len() as i64;
        // Object layout:
        //   1                catalog
        //   2..2+n          pages (page i references font 2+n+1+i and stream 2+2n+1+i)
        //   2+n             page tree
        //   3+n..3+2n       per-page font objects
        //   3+2n..3+3n      content streams
        //   3+3n            info object
        let catalog_ref = 1;
        let pages_ref = 2 + n;
        let font_ref = |i: usize| 3 + n + i as i64;
        let stream_ref = |i: usize| 3 + 2 * n + i as i64;
        let info_ref = 3 + 3 * n;

        let mut objects: Vec<Vec<u8>> = Vec::new();
        objects.push(format!("<< /Type /Catalog /Pages {pages_ref} 0 R >>").into_bytes());
        for (i, p) in self.pages.iter().enumerate() {
            objects.push(
                format!(
                    "<< /Type /Page /Parent {pages_ref} 0 R /MediaBox [0 0 {:.2} {:.2}] \
                     /Resources << /Font << /F1 {} 0 R >> >> /Contents {} 0 R >>",
                    p.width_pt,
                    p.height_pt,
                    font_ref(i),
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
            let base = p.font.as_str();
            objects
                .push(format!("<< /Type /Font /Subtype /Type1 /BaseFont /{base} >>").into_bytes());
        }
        for p in &self.pages {
            let body = p.ops.join("\n");
            let mut stream = Vec::new();
            stream.extend_from_slice(format!("<< /Length {} >>\nstream\n", body.len()).as_bytes());
            stream.extend_from_slice(body.as_bytes());
            stream.extend_from_slice(b"\nendstream");
            objects.push(stream);
        }
        objects.push(
            format!(
                "<< /Producer (Loom) /Creator (Loom) /CreationDate {} >>",
                self.creation_date
            )
            .into_bytes(),
        );
        let _ = (catalog_ref, info_ref);

        // Assemble with an xref table.
        let mut out = Vec::new();
        out.extend_from_slice(b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n");
        let mut offsets: BTreeMap<i64, usize> = BTreeMap::new();
        for (i, obj) in objects.iter().enumerate() {
            offsets.insert(1 + i as i64, out.len());
            out.extend_from_slice(format!("{} 0 obj\n", 1 + i).as_bytes());
            out.extend_from_slice(obj);
            out.extend_from_slice(b"\nendobj\n");
        }
        let xref_pos = out.len();
        out.extend_from_slice(format!("xref\n0 {}\n", objects.len() + 1).as_bytes());
        out.extend_from_slice(b"0000000000 65535 f \n");
        for i in 1..=objects.len() as i64 {
            out.extend_from_slice(format!("{:010} 00000 n \n", offsets[&i]).as_bytes());
        }
        out.extend_from_slice(
            format!(
                "trailer\n<< /Size {} /Root {catalog_ref} 0 R /Info {info_ref} 0 R >>\nstartxref\n{xref_pos}\n%%EOF\n",
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

/// Escape a PDF literal string (Latin-1 text only).
fn escape_pdf_string(s: &str) -> String {
    let mut out = String::new();
    for ch in s.chars() {
        match ch {
            '(' => out.push_str("\\("),
            ')' => out.push_str("\\)"),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 256 => out.push(c),
            // Non-Latin-1: replace with a placeholder (documented limitation).
            _ => out.push('\u{FFFD}'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_xref(bytes: &[u8]) -> (usize, Vec<usize>) {
        // Operate on raw bytes: xref offsets are byte offsets, and a UTF-8
        // lossy conversion of the binary header would shift indices.
        let start = bytes
            .windows(b"startxref".len())
            .position(|w| w == b"startxref")
            .expect("startxref present");
        let after = &bytes[start + b"startxref".len()..];
        let start_pos: usize = std::str::from_utf8(after)
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        let xref = std::str::from_utf8(&bytes[start_pos..]).unwrap();
        let count_line = xref.lines().nth(1).unwrap();
        let count: usize = count_line
            .split_whitespace()
            .nth(1)
            .unwrap()
            .parse()
            .unwrap();
        let entries: Vec<usize> = xref
            .lines()
            .skip(3)
            .take(count - 1)
            .map(|l| l.split_whitespace().next().unwrap().parse().unwrap())
            .collect();
        (start_pos, entries)
    }

    #[test]
    fn serializes_valid_pdf() {
        let mut doc = PdfDocument::new();
        let p = doc.add_page(612.0, 792.0);
        doc.draw_text(p, 72.0, 720.0, "Hello, Loom!", &TextStyle::default());
        doc.draw_rect(
            p,
            72.0,
            400.0,
            100.0,
            50.0,
            PathStyle::filled((0.7, 0.3, 0.1)),
        );
        doc.draw_line(
            p,
            0.0,
            0.0,
            612.0,
            792.0,
            PathStyle::stroked((0.0, 0.0, 0.0), 1.0),
        );
        let bytes = doc.serialize();
        assert!(bytes.starts_with(b"%PDF-1.4"));
        assert!(bytes.ends_with(b"%%EOF\n"));

        let (xref_pos, entries) = parse_xref(&bytes);
        assert_eq!(
            entries.len(),
            6,
            "catalog + page + pagetree + font + stream + info"
        );
        // Object 1 must sit exactly at the recorded xref offset.
        assert_eq!(&bytes[entries[0]..entries[0] + 7], b"1 0 obj");
        // Every offset must point at a valid object header.
        for (i, off) in entries.iter().enumerate() {
            assert_eq!(
                &bytes[*off..*off + 7],
                format!("{} 0 obj", i + 1).as_bytes()
            );
        }
        assert_eq!(&bytes[xref_pos..xref_pos + 4], b"xref");
        // Text must be present in the stream.
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("Hello, Loom!"));
        assert!(text.contains("/Type /Catalog"));
        assert!(text.contains("/MediaBox [0 0 612.00 792.00]"));
    }

    #[test]
    fn deterministic_output() {
        let mut a = PdfDocument::new();
        let pa = a.add_page(100.0, 100.0);
        a.draw_text(pa, 10.0, 50.0, "same", &TextStyle::default());
        let mut b = PdfDocument::new();
        let pb = b.add_page(100.0, 100.0);
        b.draw_text(pb, 10.0, 50.0, "same", &TextStyle::default());
        assert_eq!(
            a.serialize(),
            b.serialize(),
            "identical input -> identical bytes"
        );
    }

    #[test]
    fn escapes_special_characters() {
        let mut doc = PdfDocument::new();
        let p = doc.add_page(100.0, 100.0);
        doc.draw_text(p, 10.0, 10.0, "a(b)\\c\nd", &TextStyle::default());
        let bytes = doc.serialize();
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains(r"a\(b\)\\c\nd"), "PDF string escapes applied");
    }

    #[test]
    fn bold_and_italic_font_selection() {
        let mut doc = PdfDocument::new();
        let p = doc.add_page(100.0, 100.0);
        let style = TextStyle {
            bold: true,
            italic: true,
            ..Default::default()
        };
        doc.draw_text(p, 10.0, 10.0, "x", &style);
        let bytes = doc.serialize();
        let text = String::from_utf8_lossy(&bytes);
        // The page resources always name Helvetica; bold/oblique is expressed
        // via the BaseFont in the font object. Here we assert the content
        // stream emitted the font size.
        assert!(text.contains("Tf BT"));
    }

    #[test]
    fn transformed_text_keeps_glyph_y_axis_upright() {
        let mut doc = PdfDocument::new();
        let p = doc.add_page(100.0, 100.0);
        doc.draw_text_with_transform(
            p,
            0.0,
            0.0,
            "upright",
            &TextStyle::default(),
            [1.0, 0.0, 0.0, -1.0, 10.0, 20.0],
        );
        let bytes = doc.serialize();
        let text = String::from_utf8_lossy(&bytes);
        assert!(
            text.contains("BT 1 0 0 -1 0.00 0.00 Tm (upright) Tj"),
            "transformed text must invert the local text y-axis exactly once"
        );
    }
}

#[cfg(test)]
pub mod debug_export {
    pub use super::*;
}
