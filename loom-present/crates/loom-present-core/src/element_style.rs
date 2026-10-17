//! Text and shape styling for slide elements.
//!
//! Every field is optional or off by default, so an element that was never
//! styled keeps its type's own look (a title is large and bold, a subtitle
//! muted) and its file stays as small as before. Sizes are PowerPoint points,
//! so a style survives a `.pptx` round trip unchanged.

use serde::{Deserialize, Serialize};

use crate::PresentationSession;

/// Horizontal alignment of an element's text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// How an element's text and background are drawn, beyond what its type implies.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ElementStyle {
    /// Text size in points; `None` follows the element type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_size_pt: Option<f32>,
    /// `None` follows the element type (titles are bold).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bold: Option<bool>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub italic: bool,
    /// Text colour as `#RRGGBB`; `None` follows the element type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Background fill as `#RRGGBB`; `None` follows the element type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<String>,
    /// `None` follows the element type (titles are centred).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align: Option<TextAlign>,
}

/// Smallest and largest text size the editor offers, in points.
pub const FONT_SIZE_RANGE: (f32, f32) = (6.0, 200.0);

/// The colours the inspector offers for text and fills.
pub const PALETTE: [&str; 7] = [
    "#C62828", "#E65100", "#2E7D32", "#1565C0", "#6A1B9A", "#616161", "#000000",
];

impl ElementStyle {
    /// Whether nothing is overridden, so the element looks as its type says.
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// The text size to draw in points, given the size the element's type would use.
    pub fn size_or(&self, default_pt: f32) -> f32 {
        self.font_size_pt.unwrap_or(default_pt)
    }

    /// A size one step larger or smaller than `current`, kept in range.
    pub fn stepped_size(current: f32, delta: i32) -> f32 {
        let next = if delta >= 0 {
            current + delta as f32 * if current < 24.0 { 2.0 } else { 4.0 }
        } else {
            current + delta as f32 * if current <= 24.0 { 2.0 } else { 4.0 }
        };
        next.round().clamp(FONT_SIZE_RANGE.0, FONT_SIZE_RANGE.1)
    }
}

impl PresentationSession {
    /// Changes the style of the given elements on the active slide as one
    /// undoable step. Returns false, with no history entry, when nothing changed.
    pub fn restyle_elements(
        &mut self,
        element_ids: &[String],
        change: impl Fn(&mut ElementStyle),
    ) -> bool {
        let Some(slide) = self.document.active_slide() else {
            return false;
        };
        let changes = slide
            .elements
            .iter()
            .filter(|element| element_ids.contains(&element.id))
            .any(|element| {
                let mut next = element.style.clone();
                change(&mut next);
                next != element.style
            });
        if !changes {
            return false;
        }
        self.checkpoint();
        if let Some(slide) = self.document.active_slide_mut() {
            for element in slide
                .elements
                .iter_mut()
                .filter(|element| element_ids.contains(&element.id))
            {
                change(&mut element.style);
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_steps_are_small_for_small_text_and_large_for_large_text_and_stay_in_range() {
        assert_eq!(ElementStyle::stepped_size(18.0, 1), 20.0);
        assert_eq!(ElementStyle::stepped_size(18.0, -1), 16.0);
        assert_eq!(ElementStyle::stepped_size(32.0, 1), 36.0);
        assert_eq!(ElementStyle::stepped_size(6.0, -1), 6.0);
        assert_eq!(ElementStyle::stepped_size(200.0, 1), 200.0);
    }

    #[test]
    fn an_unstyled_element_writes_nothing_and_a_styled_one_round_trips() {
        assert!(ElementStyle::default().is_default());
        assert_eq!(
            serde_json::to_string(&ElementStyle::default()).unwrap(),
            "{}"
        );
        let style = ElementStyle {
            font_size_pt: Some(28.0),
            bold: Some(true),
            italic: true,
            color: Some("#1565C0".into()),
            align: Some(TextAlign::Right),
            ..Default::default()
        };
        let back: ElementStyle =
            serde_json::from_str(&serde_json::to_string(&style).unwrap()).unwrap();
        assert_eq!(back, style);
    }

    use crate::{
        export_pdf, export_pptx, import_pptx, ElementType, PresentationDocument, SlideElement,
    };

    fn deck_with(elements: Vec<SlideElement>) -> PresentationSession {
        let mut document = PresentationDocument::new("deck", "Styled");
        if let Some(slide) = document.active_slide_mut() {
            slide.elements = elements;
        }
        PresentationSession::new(document)
    }

    fn element(id: &str, kind: ElementType, content: &str, style: ElementStyle) -> SlideElement {
        SlideElement {
            id: id.into(),
            element_type: kind,
            content: content.into(),
            x: 100.0,
            y: 100.0,
            width: 500.0,
            height: 120.0,
            rotation_deg: 0.0,
            action: None,
            style,
        }
    }

    #[test]
    fn a_styled_object_survives_a_pptx_round_trip_and_an_unstyled_one_stays_unstyled() {
        let style = ElementStyle {
            font_size_pt: Some(40.0),
            bold: Some(false),
            italic: true,
            color: Some("#1565C0".into()),
            fill: Some("#E65100".into()),
            align: Some(TextAlign::Right),
        };
        let session = deck_with(vec![
            element("styled", ElementType::Title, "Styled title", style.clone()),
            element(
                "plain",
                ElementType::BodyText,
                "Plain body",
                ElementStyle::default(),
            ),
            element(
                "rect",
                ElementType::ShapeRectangle,
                "Box",
                ElementStyle::default(),
            ),
        ]);
        let bytes = export_pptx(&session).expect("export");
        let archive = loom_package::zip::PackageArchive::from_bytes(&bytes).unwrap();
        let xml =
            String::from_utf8(archive.get("ppt/slides/slide1.xml").unwrap().to_vec()).unwrap();
        assert!(
            xml.contains("sz=\"4000\""),
            "size in hundredths of a point: {xml}"
        );
        assert!(xml.contains(" i=\"1\""), "italic is written");
        assert!(xml.contains("algn=\"r\""), "alignment is written");
        assert!(
            xml.contains("1565C0") && xml.contains("E65100"),
            "text colour and fill"
        );

        let back = import_pptx(&bytes).expect("import");
        let elements = &back.slides[0].elements;
        let find = |id: &str| {
            elements
                .iter()
                .find(|e| e.id == id)
                .unwrap_or_else(|| panic!("{id} missing"))
        };
        assert_eq!(find("styled").style, style, "the style comes back exactly");
        assert!(
            find("plain").style.is_default(),
            "an unstyled body stays unstyled"
        );
        assert!(
            find("rect").style.is_default(),
            "so does a default-filled shape"
        );
    }

    #[test]
    fn a_real_powerpoint_run_size_and_bold_import_as_a_style() {
        // PowerPoint writes the size and weight on the run; a deck from it keeps them.
        let sp = "<p:sp><p:nvSpPr><p:cNvPr id=\"2\" name=\"Title 1\"/><p:cNvSpPr/><p:nvPr><p:ph type=\"title\"/></p:nvPr></p:nvSpPr><p:spPr><a:xfrm><a:off x=\"1219200\" y=\"1219200\"/><a:ext cx=\"6096000\" cy=\"1219200\"/></a:xfrm></p:spPr><p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:pPr algn=\"l\"/><a:r><a:rPr lang=\"en-US\" sz=\"4400\" b=\"0\" i=\"1\"><a:solidFill><a:srgbClr val=\"C00000\"/></a:solidFill></a:rPr><a:t>Big</a:t></a:r></a:p></p:txBody></p:sp>";
        let style = crate::pptx_import::parse_shape_style_for_tests(sp, &ElementType::Title);
        assert_eq!(style.font_size_pt, Some(44.0));
        assert_eq!(
            style.bold,
            Some(false),
            "a title is bold unless the run says otherwise"
        );
        assert!(style.italic);
        assert_eq!(style.color.as_deref(), Some("#C00000"));
        assert_eq!(
            style.align,
            Some(TextAlign::Left),
            "a left title is not the centred default"
        );
    }

    #[test]
    fn the_pdf_draws_styled_text_and_fills_without_losing_the_words() {
        let session = deck_with(vec![element(
            "a",
            ElementType::BodyText,
            "Right aligned words",
            ElementStyle {
                font_size_pt: Some(30.0),
                align: Some(TextAlign::Right),
                color: Some("#C62828".into()),
                fill: Some("#FEF08A".into()),
                ..Default::default()
            },
        )]);
        let pdf = export_pdf(&session.document);
        let text = loom_pdf::inspect::readable_content(&pdf).expect("readable PDF");
        assert!(text.contains("Right aligned words"), "{text}");
        let plain = export_pdf(
            &deck_with(vec![element(
                "a",
                ElementType::BodyText,
                "Right aligned words",
                ElementStyle::default(),
            )])
            .document,
        );
        assert_ne!(pdf, plain, "the style changes what is drawn");
    }
}
