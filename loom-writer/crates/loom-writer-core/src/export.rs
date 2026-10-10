//! Document export writers (DOCX, PDF).
//!
//! Extracted from the crate root to keep `lib.rs` within its registered
//! byte ceiling. Public names are re-exported from the crate root so the
//! public API surface is unchanged.

use std::rc::Rc;

use crate::export_flow;
use crate::export_table::{self, TableLayout};
use crate::{PageStyle, WriterDocument};

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

/// How far a line of `width` moves right inside the text `column` for its
/// alignment: centred and right-aligned lines move, the others stay at the margin.
fn align_shift(alignment: loom_text::Alignment, column: f32, width: f32) -> f32 {
    match alignment {
        loom_text::Alignment::Center => (column - width) / 2.0,
        loom_text::Alignment::Right => column - width,
        _ => 0.0,
    }
    .max(0.0)
}

/// The text of one PDF line: a byte range of its block's text, or one row of a
/// table block.
enum LineContent {
    Text {
        start: usize,
        end: usize,
        marker: String,
        style: loom_pdf::TextStyle,
    },
    TableRow {
        layout: Rc<TableLayout>,
        row: usize,
    },
}

/// One line of the PDF in document order.
struct PdfLine {
    block: usize,
    content: LineContent,
    /// Space before the line, between blocks.
    gap: f32,
    /// Height of the line box.
    height: f32,
}

/// Whether `kind` is one of the heading block kinds.
fn is_heading(kind: &str) -> bool {
    kind.strip_prefix("heading")
        .is_some_and(|level| matches!(level, "1" | "2" | "3" | "4" | "5" | "6"))
}

/// The style of a block's text: headings take the document model's size and
/// are bold, as the DOCX export sets them; other blocks use the body style.
fn block_style(kind: &str, body: &loom_pdf::TextStyle) -> loom_pdf::TextStyle {
    if is_heading(kind) {
        loom_pdf::TextStyle {
            size_pt: PageStyle::default().font_size_for_kind(kind),
            bold: true,
            ..body.clone()
        }
    } else {
        body.clone()
    }
}

/// Lays every block out as PDF lines, wrapped with the PDF's own widths.
fn pdf_lines(
    doc: &WriterDocument,
    page_style: &PageStyle,
    body: &loom_pdf::TextStyle,
    column: f32,
) -> Vec<PdfLine> {
    let mut lines = Vec::new();
    let mut numbered_index = 0usize;
    let block_gap = body.size_pt * page_style.line_height;
    for (index, block) in doc.blocks.iter().enumerate() {
        let gap = if index == 0 { 0.0 } else { block_gap };
        let kind = block.kind.as_str();
        let text = block.text.as_str();
        if kind == crate::TABLE_BLOCK_KIND {
            // Row heights depend on the wrapped cells, so the table is laid out
            // here, where pagination needs the heights.
            let line_height = body.size_pt * page_style.line_height;
            let layout = Rc::new(TableLayout::new(text, body, column, line_height));
            for row in 0..layout.row_count() {
                lines.push(PdfLine {
                    block: index,
                    content: LineContent::TableRow {
                        layout: Rc::clone(&layout),
                        row,
                    },
                    gap: if row == 0 { gap } else { 0.0 },
                    height: layout.row_height(row),
                });
            }
            continue;
        }
        let marker = match kind {
            "list-bulleted" => {
                numbered_index = 0;
                "- ".to_string()
            }
            "list-numbered" => {
                numbered_index += 1;
                format!("{numbered_index}. ")
            }
            _ => {
                numbered_index = 0;
                String::new()
            }
        };
        let style = block_style(kind, body);
        let height = style.size_pt * page_style.line_height;
        let marker_width = loom_pdf::text_width_pt(&marker, &style);
        let ranges = export_flow::wrap(
            text,
            &block.runs,
            &style,
            (column - marker_width).max(0.0),
            column,
        );
        for (line_index, (start, end)) in ranges.into_iter().enumerate() {
            lines.push(PdfLine {
                block: index,
                content: LineContent::Text {
                    start,
                    end,
                    marker: if line_index == 0 {
                        marker.clone()
                    } else {
                        String::new()
                    },
                    style: style.clone(),
                },
                gap: if line_index == 0 { gap } else { 0.0 },
                height,
            });
        }
    }
    lines
}

/// Render the document to a PDF. Lines are wrapped with the widths the PDF
/// draws with, so no line or appended run passes the right margin; headings
/// take the document model's sizes; pages break by line height within the
/// margins. Output is byte-for-byte deterministic for the same document.
pub fn export_pdf(doc: &WriterDocument) -> Vec<u8> {
    use loom_pdf::{PdfDocument, TextStyle};
    let page_style = doc.page.page_style();
    let mut pdf = PdfDocument::new();
    let body = TextStyle {
        size_pt: loom_text::CharacterStyle::default().font_size,
        fill_rgb: (0.15, 0.13, 0.11),
        ..Default::default()
    };
    let column =
        (page_style.width_pt - page_style.margin_left_pt - page_style.margin_right_pt).max(0.0);
    let top = page_style.height_pt - page_style.margin_top_pt;
    let mut page = pdf.add_page(page_style.width_pt, page_style.height_pt);
    let mut y = top;

    for line in pdf_lines(doc, &page_style, &body, column) {
        y -= line.gap;
        if y - line.height < page_style.margin_bottom_pt && y < top {
            page = pdf.add_page(page_style.width_pt, page_style.height_pt);
            y = top;
        }
        let block = &doc.blocks[line.block];
        match &line.content {
            LineContent::Text {
                start,
                end,
                marker,
                style,
            } => {
                let text = &block.text.as_str()[*start..*end];
                let width = export_flow::line_width(marker, text, &block.runs, style, *start);
                let shift = align_shift(block.style.alignment, column, width);
                let text_line = StyledLine {
                    x: page_style.margin_left_pt + shift,
                    y,
                    marker,
                    text,
                    text_start: *start,
                    runs: &block.runs,
                };
                draw_styled_line(&mut pdf, page, &text_line, style);
            }
            LineContent::TableRow { layout, row } => {
                let shift = align_shift(block.style.alignment, column, layout.width);
                let origin = (page_style.margin_left_pt + shift, y);
                export_table::draw_row(&mut pdf, page, layout, *row, origin);
            }
        }
        y -= line.height;
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
        let pdf = loom_pdf::inspect::readable_content(&pdf_bytes).expect("readable PDF");
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
        let text = loom_pdf::inspect::readable_content(&pdf).expect("readable PDF");
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
        let text = loom_pdf::inspect::readable_content(&pdf).expect("readable PDF");
        assert!(
            text.contains("/F2 ") && text.contains("Tf BT") && text.contains("(Caf\u{e9} menu)"),
            "heading is drawn in the bold face with its accent: {text}"
        );
        assert!(
            text.contains("/F1 ") && text.contains("(It\u{2019}s a plain body line.)"),
            "body keeps the regular face and the typographic apostrophe: {text}"
        );
        assert_eq!(
            loom_pdf::inspect::page_text(&pdf).expect("page text"),
            ["Caf\u{e9} menu\nIt\u{2019}s a plain body line."],
            "the text reads back exactly, with no '?' substitutes"
        );
        let faces: Vec<String> = loom_pdf::inspect::embedded_fonts(&pdf)
            .expect("fonts")
            .into_iter()
            .map(|font| font.base_font)
            .collect();
        assert!(
            faces.iter().any(|f| f.ends_with("+Inter-Bold"))
                && faces.iter().any(|f| f.ends_with("+Inter-Regular")),
            "headings and body embed their own Inter faces: {faces:?}"
        );
    }

    #[test]
    fn export_pdf_carries_non_latin_scripts_instead_of_question_marks() {
        let mut document = WriterDocument::new("export-scripts", "Scripts");
        document.push(RichBlock::new(
            document.next_id(),
            "paragraph",
            "\u{395}\u{3bb}\u{3bb}\u{3b7}\u{3bd}\u{3b9}\u{3ba}\u{3ac} \u{41f}\u{440}\u{438}\u{432}\u{435}\u{442}",
        ));
        let pdf = export_pdf(&document);
        assert_eq!(
            loom_pdf::inspect::page_text(&pdf).expect("page text"),
            ["\u{395}\u{3bb}\u{3bb}\u{3b7}\u{3bd}\u{3b9}\u{3ba}\u{3ac} \u{41f}\u{440}\u{438}\u{432}\u{435}\u{442}"]
        );
    }

    #[test]
    fn export_pdf_does_not_inject_title_into_body() {
        let mut document = WriterDocument::new("title-test", "Untitled");
        document.push(RichBlock::new(document.next_id(), "paragraph", "Hello"));

        let pdf_bytes = export_pdf(&document);
        let pdf = loom_pdf::inspect::readable_content(&pdf_bytes).expect("readable PDF");

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
        let pdf = loom_pdf::inspect::readable_content(&pdf_bytes).expect("readable PDF");

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

    /// Text x position of the line drawn as `(text) Tj`.
    fn line_x(pdf: &[u8], text: &str) -> f32 {
        let content = loom_pdf::inspect::readable_content(pdf).expect("readable PDF");
        let at = content
            .find(&format!(" Td ({text}) Tj"))
            .unwrap_or_else(|| panic!("{text:?} is drawn"));
        let before: Vec<&str> = content[..at].split_whitespace().collect();
        before[before.len() - 2].parse().expect("x coordinate")
    }

    /// Text position (x, y) of the line drawn as `(text) Tj`.
    fn text_position(pdf: &[u8], text: &str) -> (f32, f32) {
        let content = loom_pdf::inspect::readable_content(pdf).expect("readable PDF");
        let at = content
            .find(&format!(" Td ({text}) Tj"))
            .unwrap_or_else(|| panic!("{text:?} is drawn"));
        let before: Vec<&str> = content[..at].split_whitespace().collect();
        (
            before[before.len() - 2].parse().expect("x coordinate"),
            before[before.len() - 1].parse().expect("y coordinate"),
        )
    }

    #[test]
    fn export_pdf_draws_table_cells_in_aligned_columns_without_pipes() {
        let mut document = WriterDocument::new("table", "Table");
        document.push(RichBlock::new(
            document.next_id(),
            crate::TABLE_BLOCK_KIND,
            "| Name | Qty |\n| --- | --- |\n| Ada | 3 |\n| Grace | 12 |",
        ));
        let pdf = export_pdf(&document);
        let content = loom_pdf::inspect::readable_content(&pdf).expect("readable PDF");
        assert!(!content.contains('|'), "no pipe characters are printed");
        assert!(!content.contains("---"), "the separator row is not printed");

        let (name_x, name_y) = text_position(&pdf, "Name");
        let (qty_x, qty_y) = text_position(&pdf, "Qty");
        let (ada_x, ada_y) = text_position(&pdf, "Ada");
        let (three_x, three_y) = text_position(&pdf, "3");
        let (grace_x, grace_y) = text_position(&pdf, "Grace");
        let (twelve_x, twelve_y) = text_position(&pdf, "12");
        assert_eq!(name_y, qty_y, "header cells share a row");
        assert_eq!(ada_y, three_y, "the first data row shares a row");
        assert_eq!(grace_y, twelve_y, "the second data row shares a row");
        assert!(name_y > ada_y && ada_y > grace_y, "rows run down the page");
        assert!(
            (ada_x - name_x).abs() < 0.01 && (grace_x - name_x).abs() < 0.01,
            "the first column is aligned"
        );
        assert!(
            (three_x - qty_x).abs() < 0.01 && (twelve_x - qty_x).abs() < 0.01,
            "the second column is aligned"
        );
        assert!(
            qty_x > name_x + 30.0,
            "the second column starts after the first"
        );
    }

    #[test]
    fn export_pdf_honors_center_and_right_alignment() {
        use loom_text::Alignment;

        let mut document = WriterDocument::new("align", "Align");
        for (text, alignment) in [
            ("left line", Alignment::Left),
            ("centered line", Alignment::Center),
            ("right line", Alignment::Right),
        ] {
            let mut block = RichBlock::new(document.next_id(), "paragraph", text);
            block.style.alignment = alignment;
            document.push(block);
        }
        let style = document.page.page_style();
        let pdf = export_pdf(&document);

        let column = style.width_pt - style.margin_left_pt - style.margin_right_pt;
        // The body is drawn at the document model's body size.
        let body = loom_pdf::TextStyle {
            size_pt: loom_text::CharacterStyle::default().font_size,
            ..Default::default()
        };
        let width = |text: &str| loom_pdf::text_width_pt(text, &body);
        let left = line_x(&pdf, "left line");
        let center = line_x(&pdf, "centered line");
        let right = line_x(&pdf, "right line");
        assert!(
            (left - style.margin_left_pt).abs() < 0.01,
            "left stays at the margin"
        );
        let want_center = style.margin_left_pt + (column - width("centered line")) / 2.0;
        assert!(
            (center - want_center).abs() < 3.0 && center > left + 50.0,
            "centered line starts at {center}, expected about {want_center}"
        );
        let want_right = style.margin_left_pt + column - width("right line");
        assert!(
            (right - want_right).abs() < 3.0,
            "right line starts at {right}, expected about {want_right}"
        );
    }

    /// One text draw read back from the content stream.
    struct Drawn {
        x: f32,
        size: f32,
        bold: bool,
        text: String,
    }

    /// Every `(text) Tj` in the page content, with its position and font.
    fn drawn_text(pdf: &[u8]) -> Vec<Drawn> {
        let content = loom_pdf::inspect::readable_content(pdf).expect("readable PDF");
        let mut out = Vec::new();
        let mut from = 0;
        while let Some(found) = content[from..].find(" Td (") {
            let at = from + found;
            let tail = &content[at.saturating_sub(120)..at];
            let words: Vec<&str> = tail.split_whitespace().collect();
            // "... /F2 12.00 Tf BT 72.00 700.00 Td (text) Tj"
            let n = words.len();
            let x: f32 = words[n - 2].parse().expect("x");
            let size: f32 = words[n - 5].parse().expect("size");
            let font = words[n - 6];
            let text_start = at + " Td (".len();
            let text_end = text_start + content[text_start..].find(") Tj").expect("end");
            out.push(Drawn {
                x,
                size,
                bold: font == "/F2" || font == "/F4",
                text: content[text_start..text_end].to_string(),
            });
            from = text_end;
        }
        out
    }

    #[test]
    fn export_pdf_uses_the_document_model_sizes_for_headings_and_body() {
        let mut document = WriterDocument::new("sizes", "Sizes");
        document.push(RichBlock::new(document.next_id(), "heading1", "Title"));
        document.push(RichBlock::new(
            document.next_id(),
            "paragraph",
            "Body words",
        ));
        let pdf = export_pdf(&document);
        let drawn = drawn_text(&pdf);
        let size_of = |text: &str| {
            drawn
                .iter()
                .find(|d| d.text == text)
                .unwrap_or_else(|| panic!("{text:?} is drawn"))
                .size
        };
        assert!((size_of("Title") - 24.0).abs() < 0.01, "H1 is 24 pt");
        assert!((size_of("Body words") - 12.0).abs() < 0.01, "body is 12 pt");
        assert!(
            drawn
                .iter()
                .find(|d| d.text == "Title")
                .expect("title")
                .bold,
            "H1 is bold"
        );
    }

    #[test]
    fn export_pdf_keeps_every_line_and_appended_bold_run_inside_the_right_margin() {
        let mut document = WriterDocument::new("wrap", "Wrap");
        let body = "lorem ipsum dolor sit amet consectetur adipiscing elit sed do eiusmod tempor "
            .repeat(4);
        let mut block =
            RichBlock::new(document.next_id(), "paragraph", &format!("{body}Bold tail"));
        // Right alignment is where a bold run appended to a line can run past
        // the margin: the line is placed from its regular-face width.
        block.style.alignment = loom_text::Alignment::Right;
        let start = body.len();
        block.runs.push(loom_text::StyleRun {
            start,
            end: start + "Bold tail".len(),
            style: loom_text::CharacterStyle {
                weight: loom_text::FontWeight::Bold,
                ..Default::default()
            },
        });
        document.push(block);
        let pdf = export_pdf(&document);
        let style = document.page.page_style();
        let right = style.width_pt - style.margin_right_pt;
        let left = style.margin_left_pt;
        let drawn = drawn_text(&pdf);
        assert!(drawn.len() > 3, "the paragraph wraps onto several lines");
        for piece in &drawn {
            let width = loom_pdf::text_width_pt(
                &piece.text,
                &loom_pdf::TextStyle {
                    size_pt: piece.size,
                    bold: piece.bold,
                    ..Default::default()
                },
            );
            assert!(
                piece.x >= left - 0.05 && piece.x + width <= right + 0.05,
                "{:?} spans {}..{} outside the margins {left}..{right}",
                piece.text,
                piece.x,
                piece.x + width
            );
        }
    }

    #[test]
    fn a_freshly_inserted_empty_table_exports_visible_columns() {
        let mut document = WriterDocument::new("table", "Table");
        document
            .insert_table_block(usize::MAX, 2, 3)
            .expect("insert");
        let pdf = export_pdf(&document);
        let content = loom_pdf::inspect::readable_content(&pdf).expect("readable PDF");
        // Each rule is "x y m x2 y2 l S"; a vertical rule has equal x values.
        let mut vertical_x: Vec<i64> = Vec::new();
        let mut from = 0;
        while let Some(found) = content[from..].find(" l S") {
            let at = from + found;
            let words: Vec<&str> = content[..at].split_whitespace().collect();
            let n = words.len();
            // "... x1 y1 m x2 y2 l S"
            let x1: f32 = words[n - 5].parse().expect("x1");
            let x2: f32 = words[n - 2].parse().expect("x2");
            if (x1 - x2).abs() < 0.01 {
                let key = (x1 * 100.0).round() as i64;
                if !vertical_x.contains(&key) {
                    vertical_x.push(key);
                }
            }
            from = at + " l S".len();
        }
        assert!(
            vertical_x.len() >= 4,
            "three columns need four vertical rules, found {}",
            vertical_x.len()
        );
    }
}
