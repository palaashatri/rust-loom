use crate::PresentationDocument;
use loom_pdf::{PathStyle, PdfDocument, TextStyle};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrintLayout {
    SingleSlide,
    Handout2,
    Handout4,
    Handout6,
    NotesPage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaperSize {
    Letter,
    A4,
    Legal,
}

impl PaperSize {
    /// Returns `(width_pt, height_pt)` in portrait orientation.
    pub fn portrait_pt(self) -> (f32, f32) {
        match self {
            PaperSize::Letter => (612.0, 792.0),
            PaperSize::A4 => (595.28, 841.89),
            PaperSize::Legal => (612.0, 1008.0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrintOrientation {
    Portrait,
    Landscape,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PrintOptions {
    pub layout: PrintLayout,
    pub paper_size: PaperSize,
    pub orientation: PrintOrientation,
    pub include_slide_numbers: bool,
}

impl Default for PrintOptions {
    fn default() -> Self {
        Self {
            layout: PrintLayout::SingleSlide,
            paper_size: PaperSize::Letter,
            orientation: PrintOrientation::Landscape,
            include_slide_numbers: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PrintPagePreview {
    pub page_number: usize,
    pub width_pt: f32,
    pub height_pt: f32,
    pub slide_indices: Vec<usize>,
}

fn page_dimensions(options: &PrintOptions) -> (f32, f32) {
    let (w, h) = options.paper_size.portrait_pt();
    match options.orientation {
        PrintOrientation::Portrait => (w, h),
        PrintOrientation::Landscape => (h, w),
    }
}

fn slides_per_page(layout: PrintLayout) -> usize {
    match layout {
        PrintLayout::SingleSlide | PrintLayout::NotesPage => 1,
        PrintLayout::Handout2 => 2,
        PrintLayout::Handout4 => 4,
        PrintLayout::Handout6 => 6,
    }
}

/// Computes the paginated print preview metadata for `doc` under `options`.
pub fn preview_print_pages(
    doc: &PresentationDocument,
    options: &PrintOptions,
) -> Vec<PrintPagePreview> {
    let (width_pt, height_pt) = page_dimensions(options);
    let per_page = slides_per_page(options.layout);
    let total = doc.slides.len();
    if total == 0 {
        return Vec::new();
    }
    (0..total)
        .collect::<Vec<_>>()
        .chunks(per_page)
        .enumerate()
        .map(|(idx, chunk)| PrintPagePreview {
            page_number: idx + 1,
            width_pt,
            height_pt,
            slide_indices: chunk.to_vec(),
        })
        .collect()
}

/// Exports `doc` to PDF using the specified [`PrintOptions`] (handouts, notes pages, paper size).
pub fn export_pdf_with_options(doc: &PresentationDocument, options: &PrintOptions) -> Vec<u8> {
    let pages = preview_print_pages(doc, options);
    let mut pdf = PdfDocument::new();
    pdf.set_font_fallback(crate::pdf_export::shared_font_fallback());
    let mut pdf_images = std::collections::BTreeMap::new();
    let notes_style = TextStyle {
        size_pt: 10.0,
        ..Default::default()
    };

    for preview in &pages {
        let page = pdf.add_page(preview.width_pt, preview.height_pt);
        let margin = 36.0_f32;
        let usable_w = (preview.width_pt - 2.0 * margin).max(1.0);
        let usable_h = (preview.height_pt - 2.0 * margin).max(1.0);

        match options.layout {
            PrintLayout::NotesPage => {
                if let Some(&si) = preview.slide_indices.first() {
                    let slide = &doc.slides[si];
                    let slide_h = usable_h * 0.55;
                    let slide_y = preview.height_pt - margin - slide_h;
                    crate::pdf_export::draw_slide_into_rect(
                        &mut pdf,
                        page,
                        doc,
                        slide,
                        (margin, slide_y, usable_w, slide_h),
                        &mut pdf_images,
                    );
                    pdf.draw_rect(
                        page,
                        margin,
                        slide_y,
                        usable_w,
                        slide_h,
                        PathStyle::stroked((0.6, 0.6, 0.6), 1.0),
                    );
                    if !slide.speaker_notes.is_empty() {
                        let notes_w = (usable_w - 24.0).max(1.0);
                        let mut ny = slide_y - 20.0;
                        for line in crate::pdf_export::wrap_paragraphs(
                            &slide.speaker_notes,
                            &notes_style,
                            notes_w,
                        ) {
                            if ny < margin + 12.0 {
                                break;
                            }
                            pdf.draw_text(page, margin + 12.0, ny, &line, &notes_style);
                            ny -= 14.0;
                        }
                    }
                }
            }
            _ => {
                let count = preview.slide_indices.len().max(1);
                let cols = if count <= 2 { 1 } else { 2 };
                let rows = count.div_ceil(cols);
                let cell_w = usable_w / cols as f32;
                let cell_h = usable_h / rows as f32;
                for (slot, &si) in preview.slide_indices.iter().enumerate() {
                    let col = slot % cols;
                    let row = slot / cols;
                    let x = margin + col as f32 * cell_w + 6.0;
                    let y = preview.height_pt - margin - (row + 1) as f32 * cell_h + 6.0;
                    let w = (cell_w - 12.0).max(1.0);
                    let h = (cell_h - 12.0).max(1.0);
                    let slide = &doc.slides[si];
                    crate::pdf_export::draw_slide_into_rect(
                        &mut pdf,
                        page,
                        doc,
                        slide,
                        (x, y, w, h),
                        &mut pdf_images,
                    );
                    pdf.draw_rect(
                        page,
                        x,
                        y,
                        w,
                        h,
                        PathStyle::stroked((0.65, 0.65, 0.65), 1.0),
                    );
                }
            }
        }

        if options.include_slide_numbers {
            pdf.draw_text(
                page,
                preview.width_pt - margin - 42.0,
                18.0,
                &format!("Page {}", preview.page_number),
                &notes_style,
            );
        }
    }

    pdf.serialize()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn print_layout_honors_paper_size_numbers_and_handout_content() {
        let mut doc = PresentationDocument::new("d", "Title");
        doc.slides[0].elements.push(crate::SlideElement {
            id: "b1".to_string(),
            element_type: crate::ElementType::BodyText,
            content: "HandoutBodyMarker".to_string(),
            x: 80.0,
            y: 180.0,
            width: 400.0,
            height: 80.0,
            rotation_deg: 0.0,
            action: None,
            style: Default::default(),
            runs: Vec::new(),
            effects: None,
            group_id: None,
            locked: false,
            flip_h: false,
            flip_v: false,
            paragraphs: Vec::new(),
        });
        let opts_num = PrintOptions {
            layout: PrintLayout::SingleSlide,
            paper_size: PaperSize::Letter,
            orientation: PrintOrientation::Landscape,
            include_slide_numbers: true,
        };
        let pdf_num = export_pdf_with_options(&doc, &opts_num);
        let text_num = loom_pdf::inspect::page_text(&pdf_num).expect("page_text");
        assert!(text_num[0].contains("Page 1"));
        assert!(text_num[0].contains("HandoutBodyMarker"));

        let opts_handout = PrintOptions {
            layout: PrintLayout::Handout2,
            paper_size: PaperSize::Letter,
            orientation: PrintOrientation::Portrait,
            include_slide_numbers: false,
        };
        let pdf_handout = export_pdf_with_options(&doc, &opts_handout);
        let text_handout = loom_pdf::inspect::page_text(&pdf_handout).expect("handout text");
        assert!(!text_handout[0].contains("Page 1"));
        assert!(text_handout[0].contains("HandoutBodyMarker"));

        // Legacy title without Title element + wrapped speaker notes
        let mut legacy = PresentationDocument::new("l", "LegacyDeckTitle");
        legacy.slides[0].elements.clear();
        legacy.slides[0].title = "LegacyOnlyTitle".to_string();
        legacy.slides[0].speaker_notes = "word ".repeat(40);
        let pdf_notes = export_pdf_with_options(
            &legacy,
            &PrintOptions {
                layout: PrintLayout::NotesPage,
                paper_size: PaperSize::Letter,
                orientation: PrintOrientation::Portrait,
                include_slide_numbers: false,
            },
        );
        let text_notes = loom_pdf::inspect::page_text(&pdf_notes).expect("notes text");
        assert!(text_notes[0].contains("LegacyOnlyTitle"));
        assert!(text_notes[0].contains("word"));
    }
}
