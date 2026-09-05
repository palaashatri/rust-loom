//! Document export writers (DOCX, PDF).
//!
//! Extracted from the crate root to keep `lib.rs` within its registered
//! byte ceiling. Public names are re-exported from the crate root so the
//! public API surface is unchanged.

use crate::{xml_escape_text, WriterDocument};
use loom_package::zip::PackageArchive;

/// Exports a document to a minimal valid `.docx` archive: each block becomes one `<w:p>`
/// paragraph; heading blocks carry `<w:pStyle w:val="HeadingN"/>`. Round-trips through
/// [`extract_docx_blocks`] preserving kinds and texts.
pub fn export_document_as_docx(
    doc: &WriterDocument,
) -> Result<Vec<u8>, loom_package::zip::ArchiveError> {
    let mut body = String::new();
    for block in &doc.blocks {
        if block.text.as_str().trim().is_empty() {
            continue;
        }
        // "heading3" -> "Heading3"; anything else exports as a plain paragraph.
        let style = if let Some(digits) = block.kind.strip_prefix("heading") {
            if digits.chars().all(|c| c.is_ascii_digit()) {
                let mut styled = String::from("Heading");
                styled.push_str(digits);
                format!("<w:pPr><w:pStyle w:val=\"{styled}\"/></w:pPr>")
            } else {
                String::new()
            }
        } else {
            String::new()
        };
        body.push_str(&format!(
            "<w:p>{style}<w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
            xml_escape_text(block.text.as_str())
        ));
    }
    let document_xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
         <w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">\
         <w:body>{body}</w:body></w:document>"
    );
    let mut arch = PackageArchive::new();
    arch.add("[Content_Types].xml", br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#.to_vec())?;
    arch.add("_rels/.rels", br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#.to_vec())?;
    arch.add("word/document.xml", document_xml.into_bytes())?;
    arch.to_bytes()
}

/// Render the document to a single PDF page sized by the document's page
/// setup, using the shared deterministic PDF writer. Output is
/// byte-for-byte deterministic for the same document.
pub fn export_pdf(doc: &WriterDocument) -> Vec<u8> {
    use loom_pdf::{PdfDocument, TextStyle};
    let page_style = doc.page.page_style();
    let x = page_style.margin_left_pt;
    let mut y = page_style.height_pt - page_style.margin_top_pt;
    let mut pdf = PdfDocument::new();
    let page = pdf.add_page(page_style.width_pt, page_style.height_pt);
    pdf.draw_text(
        page,
        x,
        y,
        &doc.title,
        &TextStyle {
            size_pt: 20.0,
            bold: true,
            ..Default::default()
        },
    );
    let body = TextStyle {
        size_pt: page_style.body_font_size_pt,
        fill_rgb: (0.15, 0.13, 0.11),
        ..Default::default()
    };
    let line_step = page_style.body_font_size_pt * 2.0;
    y -= line_step + 8.0;
    let mut numbered_index = 0usize;
    for b in &doc.blocks {
        let marker = match b.kind.as_str() {
            "list-bulleted" => {
                numbered_index = 0;
                Some("- ".to_string())
            }
            "list-numbered" => {
                numbered_index += 1;
                Some(format!("{numbered_index}. "))
            }
            _ => {
                numbered_index = 0;
                None
            }
        };
        let style = match b.kind.as_str() {
            "heading1" => TextStyle {
                size_pt: 15.0,
                bold: true,
                ..Default::default()
            },
            "heading2" => TextStyle {
                size_pt: 13.0,
                bold: true,
                ..Default::default()
            },
            _ => body.clone(),
        };
        let text = match &marker {
            Some(prefix) => format!("{prefix}{}", b.text.as_str()),
            None => b.text.as_str().to_string(),
        };
        pdf.draw_text(page, x, y, &text, &style);
        y -= line_step;
        if y < page_style.margin_bottom_pt {
            break;
        }
    }
    pdf.serialize()
}
