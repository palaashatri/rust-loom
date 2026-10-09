//! The PDF export of a deck: one page per slide, drawn the way the slide editor
//! draws it. The page is the slide's 16:9 plane at PowerPoint's standard size, and
//! every object keeps its place. Text sits in the editor's own text box, at the
//! editor's sizes, wrapped to that box and centred the way the editor centres it.

use std::collections::BTreeMap;

use loom_pdf::{text_width_pt, PageIndex, PathStyle, PdfDocument, TextStyle};

use crate::{
    normalize_angle_degrees, pictures, ElementType, PresentationDocument, SlideElement,
    SLIDE_HEIGHT, SLIDE_WIDTH,
};

/// The page in points: the 16:9 slide at PowerPoint's standard 13.33 by 7.5 inches.
const PAGE_WIDTH: f32 = 960.0;
const PAGE_HEIGHT: f32 = 540.0;
/// Page points per scene unit.
const POINTS_PER_UNIT: f32 = PAGE_WIDTH / SLIDE_WIDTH;

/// The editor lays out in pixels of an 840 px wide slide, so one of its pixels is
/// this many scene units.
const UNITS_PER_EDITOR_PX: f32 = SLIDE_WIDTH / 840.0;
/// The editor's type scale (`Theme.tokens.typography`): display, heading, body-large.
const DISPLAY_PX: f32 = 24.0;
const HEADING_PX: f32 = 20.0;
const BODY_PX: f32 = 14.0;
/// The editor's spacing tokens (`Theme.tokens.space`): xs, sm and lg.
const SPACE_XS_PX: f32 = 2.0;
const SPACE_SM_PX: f32 = 4.0;
const SPACE_LG_PX: f32 = 12.0;
/// Lines are this many ems apart, and the baseline sits this far below a line's top.
const LINE_EM: f32 = 1.2;
const ASCENT_EM: f32 = 0.97;

/// Fill of a plain shape, and of a stat card.
const SHAPE_RGB: (f32, f32, f32) = (0.84, 0.88, 0.96);
const STAT_RGB: (f32, f32, f32) = (0.16, 0.36, 0.72);

/// Renders the deck to a PDF; see the module documentation for what matches the editor.
pub fn export_pdf(doc: &PresentationDocument) -> Vec<u8> {
    let mut pdf = PdfDocument::new();
    let mut pdf_images: BTreeMap<String, usize> = Default::default();
    let style_title = TextStyle {
        size_pt: 18.0,
        bold: true,
        ..Default::default()
    };
    for slide in &doc.slides {
        let page = pdf.add_page(PAGE_WIDTH, PAGE_HEIGHT);
        let has_title = slide
            .elements
            .iter()
            .any(|element| element.element_type == ElementType::Title);
        if !has_title {
            // Legacy slides may have no title element; keep their title at the top left.
            let title_x = 40.0 * POINTS_PER_UNIT;
            let title_y = (SLIDE_HEIGHT - 70.0) * POINTS_PER_UNIT;
            pdf.draw_text(page, title_x, title_y, &slide.title, &style_title);
        }
        for elem in &slide.elements {
            draw_element(&mut pdf, page, doc, elem, &mut pdf_images);
        }
    }
    pdf.serialize()
}

/// Draws one object: its shape or picture, then its text in the editor's box.
fn draw_element(
    pdf: &mut PdfDocument,
    page: PageIndex,
    doc: &PresentationDocument,
    elem: &SlideElement,
    pdf_images: &mut BTreeMap<String, usize>,
) {
    let transform = element_transform(elem);
    match elem.element_type {
        ElementType::ShapeRectangle | ElementType::ShapeCircle | ElementType::StatCard => {
            let rgb = if elem.element_type == ElementType::StatCard {
                STAT_RGB
            } else {
                SHAPE_RGB
            };
            let (w, h) = (elem.width, elem.height);
            pdf.draw_rect_with_transform(page, (0.0, 0.0, w, h), PathStyle::filled(rgb), transform);
            draw_text_box(pdf, page, transform, elem);
        }
        ElementType::Title | ElementType::Subtitle | ElementType::BodyText => {
            draw_text_box(pdf, page, transform, elem);
        }
        ElementType::Picture => {
            if let Some(asset) = doc.asset_for(elem) {
                let index = match pdf_images.get(&asset.id) {
                    Some(index) => Some(*index),
                    None => pictures::pdf_image(asset).ok().map(|image| {
                        let index = pdf.add_image(image);
                        pdf_images.insert(asset.id.clone(), index);
                        index
                    }),
                };
                if let Some(index) = index {
                    let size = (elem.width, elem.height);
                    pdf.draw_image_with_transform(page, index, size, transform);
                }
            }
        }
    }
}

/// The matrix that places an object's local space on the page. Local space has its
/// origin at the object's top left and runs down, as the editor's does, so text and
/// rectangles are drawn in scene units and the matrix does the scaling and turning.
fn element_transform(elem: &SlideElement) -> [f32; 6] {
    let radians = normalize_angle_degrees(elem.rotation_deg).to_radians();
    let (sin, cos) = radians.sin_cos();
    let scale = POINTS_PER_UNIT;
    let center_x = (elem.x + elem.width / 2.0) * scale;
    let center_y = PAGE_HEIGHT - (elem.y + elem.height / 2.0) * scale;
    // Present's scene is y-down, PDF's is y-up; the clockwise turn composes with that flip.
    let a = scale * cos;
    let b = -scale * sin;
    let c = -scale * sin;
    let d = -scale * cos;
    let e = center_x - (a * elem.width / 2.0 + c * elem.height / 2.0);
    let f = center_y - (b * elem.width / 2.0 + d * elem.height / 2.0);
    [a, b, c, d, e, f]
}

/// The text of an element as the editor sets it: its type's size and weight, and
/// titles and stat cards centred across the box.
fn text_style(element_type: &ElementType) -> (TextStyle, bool) {
    let (px, bold, centred) = match element_type {
        ElementType::Title => (DISPLAY_PX, true, true),
        ElementType::StatCard => (HEADING_PX, true, true),
        _ => (BODY_PX, false, false),
    };
    let style = TextStyle {
        size_pt: px * UNITS_PER_EDITOR_PX,
        bold,
        ..TextStyle::default()
    };
    (style, centred)
}

/// Draws an element's text inside the editor's text box: wrapped to its width,
/// centred vertically, and centred across or set at the left as its type says. Text
/// that does not fit its box ends in an ellipsis on its last visible line.
fn draw_text_box(pdf: &mut PdfDocument, page: PageIndex, transform: [f32; 6], elem: &SlideElement) {
    if elem.content.trim().is_empty() {
        return;
    }
    let (style, centred) = text_style(&elem.element_type);
    let left = SPACE_SM_PX * UNITS_PER_EDITOR_PX;
    let top = SPACE_XS_PX * UNITS_PER_EDITOR_PX;
    let width = (elem.width - 2.0 * SPACE_LG_PX * UNITS_PER_EDITOR_PX).max(1.0);
    let height = (elem.height - 2.0 * SPACE_SM_PX * UNITS_PER_EDITOR_PX).max(1.0);
    let line_height = style.size_pt * LINE_EM;

    let mut lines = wrap_paragraphs(&elem.content, &style, width);
    let fits = ((height / line_height).floor() as usize).max(1);
    if lines.len() > fits {
        lines.truncate(fits);
        if let Some(last) = lines.last_mut() {
            *last = with_ellipsis(last, &style, width);
        }
    }
    let block = lines.len() as f32 * line_height;
    let first_top = top + (height - block) / 2.0;
    for (index, line) in lines.iter().enumerate() {
        if line.is_empty() {
            continue;
        }
        let x = if centred {
            left + (width - text_width_pt(line, &style)) / 2.0
        } else {
            left
        };
        let baseline = first_top + index as f32 * line_height + ASCENT_EM * style.size_pt;
        pdf.draw_text_with_transform(page, x, baseline, line, &style, transform);
    }
}

/// Every paragraph of `text`, wrapped to `width`. A blank paragraph keeps one empty
/// line, so the space between paragraphs survives.
fn wrap_paragraphs(text: &str, style: &TextStyle, width: f32) -> Vec<String> {
    text.split('\n')
        .flat_map(|paragraph| wrap_paragraph(paragraph.trim_end_matches('\r'), style, width))
        .collect()
}

/// One paragraph wrapped to `width`: words stay whole where they fit, and a word
/// wider than the box breaks between characters, so no text is lost.
fn wrap_paragraph(paragraph: &str, style: &TextStyle, width: f32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in paragraph.split_whitespace() {
        let joined = if line.is_empty() {
            word.to_string()
        } else {
            format!("{line} {word}")
        };
        if text_width_pt(&joined, style) <= width {
            line = joined;
            continue;
        }
        if !line.is_empty() {
            lines.push(std::mem::take(&mut line));
        }
        for ch in word.chars() {
            line.push(ch);
            if line.chars().count() > 1 && text_width_pt(&line, style) > width {
                let next = line.pop().unwrap_or(ch);
                lines.push(std::mem::take(&mut line));
                line.push(next);
            }
        }
    }
    lines.push(line);
    lines
}

/// `line` shortened from its end until it and an ellipsis fit `width`.
fn with_ellipsis(line: &str, style: &TextStyle, width: f32) -> String {
    let mut kept: String = line.to_string();
    while !kept.is_empty() && text_width_pt(&format!("{kept}\u{2026}"), style) > width {
        kept.pop();
    }
    format!("{kept}\u{2026}")
}
