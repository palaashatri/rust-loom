//! Document export writers (DOCX, PDF).
//!
//! Extracted from the crate root to keep `lib.rs` within its registered
//! byte ceiling. Public names are re-exported from the crate root so the
//! public API surface is unchanged.

use crate::WriterDocument;

/// Exports a document to a `.docx` archive Word opens directly; see
/// [`crate::export_docx`] for what is carried over. Round-trips through
/// [`extract_docx_blocks`] preserving kinds and texts.
pub fn export_document_as_docx(
    doc: &WriterDocument,
) -> Result<Vec<u8>, loom_package::zip::ArchiveError> {
    crate::docx::export_docx(doc).map(|export| export.bytes)
}

/// One laid-out line: an optional list marker, then `text`, which is the
/// block's bytes starting at `text_start` so character runs can be matched.
struct StyledLine<'a> {
    x: f32,
    y: f32,
    marker: &'a str,
    text: &'a str,
    text_start: usize,
    runs: &'a [loom_text::StyleRun],
}

/// Draw a line as consecutive segments, one per change of inline style, so
/// bold, italic and underline survive the export instead of being flattened
/// to the block's face.
fn draw_styled_line(
    pdf: &mut loom_pdf::PdfDocument,
    page: loom_pdf::PageIndex,
    line: &StyledLine<'_>,
    base: &loom_pdf::TextStyle,
) {
    use loom_pdf::{text_width_pt, PathStyle};
    use loom_text::FontWeight;

    let mut x = line.x;
    if !line.marker.is_empty() {
        pdf.draw_text(page, x, line.y, line.marker, base);
        x += text_width_pt(line.marker, base);
    }
    let end = line.text_start + line.text.len();
    let mut cuts = vec![0, line.text.len()];
    for run in line.runs {
        for edge in [run.start, run.end] {
            if (line.text_start..=end).contains(&edge)
                && line.text.is_char_boundary(edge - line.text_start)
            {
                cuts.push(edge - line.text_start);
            }
        }
    }
    cuts.sort_unstable();
    cuts.dedup();
    for pair in cuts.windows(2) {
        let (from, to) = (pair[0], pair[1]);
        let piece = &line.text[from..to];
        let position = line.text_start + from;
        let run = line
            .runs
            .iter()
            .find(|run| run.start <= position && position < run.end);
        let mut style = base.clone();
        let mut underline = false;
        if let Some(run) = run {
            style.bold |= matches!(
                run.style.weight,
                FontWeight::Semibold | FontWeight::Bold | FontWeight::Black
            );
            style.italic |= run.style.italic;
            underline = run.style.underline;
        }
        pdf.draw_text(page, x, line.y, piece, &style);
        let width = text_width_pt(piece, &style);
        if underline && !piece.trim().is_empty() {
            let rule_y = line.y - style.size_pt * 0.12;
            pdf.draw_line(
                page,
                x,
                rule_y,
                x + width,
                rule_y,
                PathStyle::stroked(style.fill_rgb, (style.size_pt / 24.0).max(0.5)),
            );
        }
        x += width;
    }
}

/// Render the document to a paginated PDF using the same deterministic page
/// fragments as the editor preview. Output is byte-for-byte deterministic for
/// the same document.
pub fn export_pdf(doc: &WriterDocument) -> Vec<u8> {
    use loom_pdf::{PdfDocument, TextStyle};
    let page_style = doc.page.page_style();
    let mut pdf = PdfDocument::new();
    let pages = doc.paginate(&page_style).unwrap_or_default();
    let body = TextStyle {
        size_pt: page_style.body_font_size_pt,
        fill_rgb: (0.15, 0.13, 0.11),
        ..Default::default()
    };

    // Pagination is authoritative.  Every fragment is emitted on the page
    // selected by `WriterDocument::paginate`, so a long document can never be
    // silently truncated when the first page fills up.
    let mut numbered_index = 0usize;
    for page_data in &pages {
        let page = pdf.add_page(page_style.width_pt, page_style.height_pt);
        let mut y = page_style.height_pt - page_style.margin_top_pt;

        let mut previous_block_id = None;
        for fragment in &page_data.fragments {
            let block = doc
                .blocks
                .iter()
                .find(|block| block.id == fragment.block_id);
            let Some(block) = block else { continue };
            let font_size = page_style.font_size_for_kind(block.kind.as_str());
            let line_height = font_size * page_style.line_height;
            if previous_block_id.is_some() && previous_block_id != Some(fragment.block_id) {
                y -= page_style.body_font_size_pt * page_style.line_height;
            }

            let marker = if previous_block_id != Some(fragment.block_id) {
                match block.kind.as_str() {
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
                }
            } else {
                None
            };
            let marker = marker.unwrap_or_default();
            // `wrap_utf8_ranges` includes a hard newline in the fragment that
            // precedes it.  The newline consumes the line box but must not be
            // emitted as a second PDF text line.
            let line = fragment.text.strip_suffix('\n').unwrap_or(&fragment.text);
            if !marker.is_empty() || !line.is_empty() {
                let style = match block.kind.as_str() {
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
                let start = StyledLine {
                    x: page_style.margin_left_pt,
                    y,
                    marker: &marker,
                    text: line,
                    text_start: fragment.start,
                    runs: &block.runs,
                };
                draw_styled_line(&mut pdf, page, &start, &style);
            }
            y -= line_height;
            previous_block_id = Some(fragment.block_id);
        }
    }
    pdf.serialize()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PageSetup, RichBlock, WriterDocument};

    #[test]
    fn export_pdf_emits_every_paginated_fragment() {
        let mut document = WriterDocument::new("export-test", "Export test");
        for index in 0..80 {
            document.push(RichBlock::new(
                document.next_id(),
                "paragraph",
                &format!("unique-paragraph-{index:03}"),
            ));
        }

        let expected_pages = document
            .paginate(&PageSetup::default().page_style())
            .expect("valid page layout");
        assert!(expected_pages.len() > 1, "fixture must span multiple pages");

        let pdf_bytes = export_pdf(&document);
        let pdf = String::from_utf8_lossy(&pdf_bytes);
        for index in 0..80 {
            let text = format!("unique-paragraph-{index:03}");
            assert_eq!(
                pdf.matches(&text).count(),
                1,
                "missing or duplicated {text}"
            );
        }
    }

    #[test]
    fn export_pdf_keeps_inline_bold_italic_and_underline_runs() {
        use loom_text::{CharacterStyle, FontWeight, StyleRun};

        let mut document = WriterDocument::new("export-runs", "Runs");
        let mut block = RichBlock::new(document.next_id(), "paragraph", "one two three four");
        block.runs = vec![
            StyleRun {
                start: 4,
                end: 7,
                style: CharacterStyle {
                    weight: FontWeight::Bold,
                    ..Default::default()
                },
            },
            StyleRun {
                start: 8,
                end: 13,
                style: CharacterStyle {
                    italic: true,
                    underline: true,
                    ..Default::default()
                },
            },
        ];
        document.push(block);

        let pdf = export_pdf(&document);
        let text: String = pdf.iter().map(|&byte| char::from(byte)).collect();
        assert!(text.contains("(one )"), "leading plain run: {text}");
        assert!(
            text.contains("/F2 ") && text.contains("(two)"),
            "bold run uses the bold face"
        );
        assert!(
            text.contains("/F3 ") && text.contains("(three)"),
            "italic run uses the oblique face"
        );
        assert!(
            text.contains("( four)"),
            "trailing plain run keeps its space"
        );
        assert_eq!(
            text.matches(" l S").count(),
            1,
            "only the underlined word draws a stroked rule"
        );
    }

    #[test]
    fn export_pdf_keeps_headings_bold_and_accents_readable() {
        let mut document = WriterDocument::new("export-fonts", "Fonts");
        document.push(RichBlock::new(
            document.next_id(),
            "heading1",
            "Caf\u{e9} menu",
        ));
        document.push(RichBlock::new(
            document.next_id(),
            "paragraph",
            "It\u{2019}s a plain body line.",
        ));

        let pdf = export_pdf(&document);
        let text: String = pdf.iter().map(|&byte| char::from(byte)).collect();
        assert!(
            text.contains("/F2 ") && text.contains("Tf BT") && text.contains("(Caf\u{e9} menu)"),
            "heading must be a single WinAnsi byte string in the bold face"
        );
        assert!(
            text.contains("/F1 ") && text.contains("(It\u{92}s a plain body line.)"),
            "body keeps the regular face and a WinAnsi apostrophe"
        );
        assert!(
            !text.contains('\u{c3}'),
            "no UTF-8 lead byte may reach the PDF"
        );
    }

    #[test]
    fn export_pdf_does_not_inject_title_into_body() {
        let mut document = WriterDocument::new("title-test", "Untitled");
        document.push(RichBlock::new(document.next_id(), "paragraph", "Hello"));

        let pdf_bytes = export_pdf(&document);
        let pdf = String::from_utf8_lossy(&pdf_bytes);

        // The PDF should contain "Hello" exactly once
        assert_eq!(
            pdf.matches("Hello").count(),
            1,
            "body text Hello must appear exactly once in PDF"
        );

        // The PDF should NOT contain "Untitled" because it's not a body block
        assert_eq!(
            pdf.matches("Untitled").count(),
            0,
            "title 'Untitled' must not appear in PDF body text"
        );
    }

    #[test]
    fn export_pdf_multi_page_does_not_duplicate_text() {
        let mut document = WriterDocument::new("multi-test", "My Document");
        document.push(RichBlock::new(
            document.next_id(),
            "paragraph",
            "unique-text-marker-AAA",
        ));
        for i in 0..75 {
            document.push(RichBlock::new(
                document.next_id(),
                "paragraph",
                &format!("Line number {i:03}"),
            ));
        }

        let pdf_bytes = export_pdf(&document);
        let pdf = String::from_utf8_lossy(&pdf_bytes);

        // "My Document" should not appear at all (title not in body)
        assert_eq!(
            pdf.matches("My Document").count(),
            0,
            "title must not appear in PDF body"
        );

        // Marker should appear exactly once
        assert_eq!(
            pdf.matches("unique-text-marker-AAA").count(),
            1,
            "unique marker must appear exactly once"
        );

        // Spot-check that some lines appear
        assert!(pdf.contains("Line number 000"), "first line");
        assert!(pdf.contains("Line number 050"), "middle line");
        assert!(pdf.contains("Line number 074"), "last line");
    }
}
