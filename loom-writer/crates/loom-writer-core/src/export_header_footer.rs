//! PDF rendering for document-owned headers and footers.

use crate::{PageStyle, WriterDocument, WriterError};
use loom_pdf::{PageIndex, PdfDocument, TextStyle};

#[cfg(test)]
#[path = "export_header_footer_tests.rs"]
mod tests;

use crate::header_footer::{HeaderFooterLineKind, HEADER_FOOTER_FONT_SIZE_PT};

pub(super) fn draw(
    pdf: &mut PdfDocument,
    pages: &[PageIndex],
    document: &WriterDocument,
    page_style: &PageStyle,
) -> Result<(), WriterError> {
    let settings = &document.header_footer;
    if settings.header_text.is_empty() && settings.footer_text.is_empty() {
        return Ok(());
    }
    settings
        .validate_for_page_style(page_style)
        .map_err(|error| WriterError::Invalid(format!("PDF export: {error}")))?;

    let style = TextStyle {
        size_pt: HEADER_FOOTER_FONT_SIZE_PT,
        fill_rgb: if document.page.appearance.is_black() {
            (1.0, 1.0, 1.0)
        } else {
            (0.0, 0.0, 0.0)
        },
        ..Default::default()
    };
    let total_pages = pages.len().max(1);
    for (index, page) in pages.iter().copied().enumerate() {
        let page_number = index + 1;
        for (kind, text) in [
            (
                HeaderFooterLineKind::Header,
                settings.format_header(page_number, total_pages),
            ),
            (
                HeaderFooterLineKind::Footer,
                settings.format_footer(page_number, total_pages),
            ),
        ] {
            let Some(text) = text else { continue };
            let geometry = settings
                .line_geometry(&text, page_style, kind)
                .map_err(|error| WriterError::Invalid(format!("PDF export: {error}")))?;
            pdf.draw_text(
                page,
                geometry.text_x_pt,
                geometry.pdf_baseline_y_pt,
                &text,
                &style,
            );
        }
    }
    Ok(())
}
