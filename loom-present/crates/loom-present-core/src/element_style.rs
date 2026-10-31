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
    /// A typeface name such as `Aptos`; `None` follows the slide theme's face.
    /// The name is stored trimmed and validated (see [`normalize_font_family`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_family: Option<String>,
}

/// Smallest and largest text size the editor offers, in points.
pub const FONT_SIZE_RANGE: (f32, f32) = (6.0, 200.0);

/// Longest font family name the editor accepts, in characters.
pub const FONT_FAMILY_MAX_CHARS: usize = 128;

/// Trims a font family name and checks it can be written to a deck and a
/// `.pptx` run. Empty names are rejected: following the theme is `None`, not
/// an empty string, and PowerPoint's `+mj-lt` / `+mn-lt` theme references are
/// not family names either.
pub fn normalize_font_family(raw: &str) -> Result<String, String> {
    let name = raw.trim();
    if name.is_empty() {
        return Err("A font family name cannot be empty".to_string());
    }
    if name.starts_with('+') {
        return Err("A theme font reference is not a font family name".to_string());
    }
    if name.chars().count() > FONT_FAMILY_MAX_CHARS {
        return Err(format!(
            "Font family names are limited to {FONT_FAMILY_MAX_CHARS} characters"
        ));
    }
    if name
        .chars()
        .any(|ch| ch.is_control() || matches!(ch, '\u{FFFE}' | '\u{FFFF}'))
    {
        return Err("A font family name cannot contain control characters".to_string());
    }
    Ok(name.to_string())
}

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
            font_family: Some("Aptos".into()),
            ..Default::default()
        };
        let back: ElementStyle =
            serde_json::from_str(&serde_json::to_string(&style).unwrap()).unwrap();
        assert_eq!(back, style);
    }

    #[test]
    fn a_font_family_is_trimmed_capped_and_refuses_empty_and_control_characters() {
        assert_eq!(
            normalize_font_family("  Aptos Display  ").unwrap(),
            "Aptos Display"
        );
        assert_eq!(
            normalize_font_family("Noto Sans עברית").unwrap(),
            "Noto Sans עברית",
            "a right-to-left family name is kept"
        );
        assert!(normalize_font_family("").is_err());
        assert!(normalize_font_family(" \t\n ").is_err(), "trimmed empty");
        assert!(
            normalize_font_family("+mj-lt").is_err(),
            "a theme reference is not a family"
        );
        assert!(
            normalize_font_family("Aptos\u{0007}").is_err(),
            "control characters are rejected"
        );
        let longest = "A".repeat(FONT_FAMILY_MAX_CHARS);
        assert_eq!(normalize_font_family(&longest).unwrap(), longest);
        assert!(
            normalize_font_family(&"A".repeat(FONT_FAMILY_MAX_CHARS + 1)).is_err(),
            "one character over the cap is rejected"
        );
    }

    #[test]
    fn an_unset_font_family_adds_nothing_to_a_document_but_a_set_one_round_trips() {
        let plain = serde_json::to_string(&ElementStyle::default()).unwrap();
        assert_eq!(plain, "{}", "existing documents keep their bytes");
        let styled = ElementStyle {
            font_family: Some("Aptos".into()),
            ..Default::default()
        };
        let json = serde_json::to_string(&styled).unwrap();
        assert!(json.contains("\"font_family\":\"Aptos\""), "{json}");
        let back: ElementStyle = serde_json::from_str(&json).unwrap();
        assert_eq!(back, styled);
        let older: ElementStyle = serde_json::from_str("{}").unwrap();
        assert_eq!(older.font_family, None, "a document from before this field");
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
            font_family: Some("Aptos".into()),
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
            xml.contains("<a:latin typeface=\"Aptos\"/>")
                && xml.contains("<a:ea typeface=\"Aptos\"/>")
                && xml.contains("<a:cs typeface=\"Aptos\"/>"),
            "the element's own family is written in its run properties: {xml}"
        );
        assert!(
            !xml.contains("+mj-lt"),
            "an explicit family replaces the theme's heading face: {xml}"
        );
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
    fn an_unset_family_keeps_the_theme_face_in_pptx() {
        let session = deck_with(vec![
            element(
                "title",
                ElementType::Title,
                "Themed",
                ElementStyle::default(),
            ),
            element(
                "body",
                ElementType::BodyText,
                "Body words",
                ElementStyle::default(),
            ),
        ]);
        let bytes = export_pptx(&session).expect("export");
        let archive = loom_package::zip::PackageArchive::from_bytes(&bytes).unwrap();
        let xml =
            String::from_utf8(archive.get("ppt/slides/slide1.xml").unwrap().to_vec()).unwrap();
        assert!(
            xml.contains("<a:latin typeface=\"+mj-lt\"/>"),
            "a title without its own family still follows the theme's heading face: {xml}"
        );
        assert!(
            xml.contains("Body words") && !xml.contains("typeface=\"Inter\""),
            "an unset body family writes no explicit face: {xml}"
        );
    }

    #[test]
    fn a_powerpoint_latin_typeface_imports_but_theme_references_do_not() {
        let explicit = "<p:sp><p:spPr><a:xfrm><a:off x=\"1219200\" y=\"1219200\"/><a:ext cx=\"6096000\" cy=\"1219200\"/></a:xfrm></p:spPr><p:txBody><a:p><a:r><a:rPr lang=\"en-US\" sz=\"2400\"><a:latin typeface=\"  Georgia  \"/><a:ea typeface=\"Georgia\"/></a:rPr><a:t>Serif</a:t></a:r></a:p></p:txBody></p:sp>";
        let style =
            crate::pptx_import::parse_shape_style_for_tests(explicit, &ElementType::BodyText);
        assert_eq!(
            style.font_family.as_deref(),
            Some("Georgia"),
            "an explicit family is trimmed and kept"
        );

        let themed = explicit.replace("  Georgia  ", "+mj-lt");
        let style = crate::pptx_import::parse_shape_style_for_tests(&themed, &ElementType::Title);
        assert_eq!(
            style.font_family, None,
            "a +mj-lt theme reference is not an element family"
        );

        // PowerPoint can put the theme face on an earlier run and a real family
        // on a later one in the same shape; the explicit family still wins.
        let later = "<p:sp><p:spPr/><p:txBody><a:p><a:r><a:rPr lang=\"en-US\"><a:latin typeface=\"+mn-lt\"/></a:rPr><a:t>One</a:t></a:r><a:r><a:rPr lang=\"en-US\"><a:latin typeface=\"Georgia\"/></a:rPr><a:t>Two</a:t></a:r></a:p></p:txBody></p:sp>";
        let style = crate::pptx_import::parse_shape_style_for_tests(later, &ElementType::BodyText);
        assert_eq!(
            style.font_family.as_deref(),
            Some("Georgia"),
            "the first explicit family is used"
        );
    }

    #[test]
    fn a_family_with_xml_specials_round_trips_through_a_pptx_run() {
        let style = ElementStyle {
            font_family: Some("A&B \"Display\"".into()),
            ..Default::default()
        };
        let session = deck_with(vec![element(
            "body",
            ElementType::BodyText,
            "Words",
            style.clone(),
        )]);
        let bytes = export_pptx(&session).expect("export");
        let archive = loom_package::zip::PackageArchive::from_bytes(&bytes).unwrap();
        let xml =
            String::from_utf8(archive.get("ppt/slides/slide1.xml").unwrap().to_vec()).unwrap();
        assert!(
            xml.contains("typeface=\"A&amp;B &quot;Display&quot;\""),
            "the family is escaped in the attribute: {xml}"
        );
        let back = import_pptx(&bytes).expect("import");
        let body = back.slides[0]
            .elements
            .iter()
            .find(|element| element.id == "body")
            .expect("body element");
        assert_eq!(body.style.font_family, style.font_family);
    }

    #[test]
    fn an_invalid_stored_family_is_never_written_as_a_theme_reference() {
        let session = deck_with(vec![
            element(
                "title",
                ElementType::Title,
                "Themed",
                ElementStyle::default(),
            ),
            element(
                "body",
                ElementType::BodyText,
                "Words",
                ElementStyle {
                    font_family: Some("+mj-lt".into()),
                    ..Default::default()
                },
            ),
        ]);
        let bytes = export_pptx(&session).expect("export");
        let archive = loom_package::zip::PackageArchive::from_bytes(&bytes).unwrap();
        let xml =
            String::from_utf8(archive.get("ppt/slides/slide1.xml").unwrap().to_vec()).unwrap();
        assert_eq!(
            xml.matches("<a:latin").count(),
            1,
            "only the title's own theme reference is written, not the body's hand-edited one: {xml}"
        );
        assert!(xml.contains("<a:latin typeface=\"+mj-lt\"/>"));
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
