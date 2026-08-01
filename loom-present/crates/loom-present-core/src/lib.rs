//! Core domain engine for Loom Present presentation authoring suite.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ElementType {
    Title,
    Subtitle,
    BodyText,
    ShapeRectangle,
    ShapeCircle,
    StatCard,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlideElement {
    pub id: String,
    pub element_type: ElementType,
    pub content: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Slide {
    pub id: String,
    pub title: String,
    pub layout: String,
    pub elements: Vec<SlideElement>,
    pub speaker_notes: String,
    pub bg_color: String,
}

impl Slide {
    pub fn new(id: impl Into<String>, title: impl Into<String>, layout: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            layout: layout.into(),
            elements: Vec::new(),
            speaker_notes: String::new(),
            bg_color: "#ffffff".to_string(),
        }
    }

    pub fn add_element(&mut self, elem: SlideElement) {
        self.elements.push(elem);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresentationDocument {
    pub id: String,
    pub title: String,
    pub author: String,
    pub theme: String,
    pub slides: Vec<Slide>,
    pub active_index: usize,
}

impl PresentationDocument {
    pub fn new(id: impl Into<String>, title: impl Into<String>) -> Self {
        let mut doc = Self {
            id: id.into(),
            title: title.into(),
            author: "Loom User".to_string(),
            theme: "Modern Clean".to_string(),
            slides: Vec::new(),
            active_index: 0,
        };
        let mut cover = Slide::new("slide-1", "Title Slide", "cover");
        cover.add_element(SlideElement {
            id: "elem-1".to_string(),
            element_type: ElementType::Title,
            content: doc.title.clone(),
            x: 100.0,
            y: 200.0,
            width: 800.0,
            height: 100.0,
        });
        doc.slides.push(cover);
        doc
    }

    pub fn add_slide(&mut self, title: impl Into<String>, layout: impl Into<String>) {
        let id = format!("slide-{}", self.slides.len() + 1);
        let slide = Slide::new(id, title, layout);
        self.slides.push(slide);
        self.active_index = self.slides.len() - 1;
    }

    pub fn len(&self) -> usize {
        self.slides.len()
    }

    pub fn is_empty(&self) -> bool {
        self.slides.is_empty()
    }

    pub fn active_slide(&self) -> Option<&Slide> {
        self.slides.get(self.active_index)
    }

    pub fn active_slide_mut(&mut self) -> Option<&mut Slide> {
        self.slides.get_mut(self.active_index)
    }
}

pub fn save_presentation(doc: &PresentationDocument) -> Result<Vec<u8>, String> {
    let json = serde_json::to_vec_pretty(doc).map_err(|e| e.to_string())?;
    let mut arch = loom_package::PackageArchive::new();
    arch.add("content/presentation.json", json)
        .map_err(|e| e.to_string())?;
    arch.to_bytes().map_err(|e| e.to_string())
}

pub fn load_presentation(bytes: &[u8]) -> Result<PresentationDocument, String> {
    let arch = loom_package::PackageArchive::from_bytes(bytes).map_err(|e| e.to_string())?;
    let content = arch
        .get("content/presentation.json")
        .ok_or_else(|| "missing presentation.json".to_string())?;
    serde_json::from_slice(content).map_err(|e| format!("parse payload: {e}"))
}

pub fn export_pdf(doc: &PresentationDocument) -> Vec<u8> {
    use loom_pdf::{PdfDocument, TextStyle};
    let mut pdf = PdfDocument::new();
    let style_title = TextStyle {
        size_pt: 18.0,
        bold: true,
        ..Default::default()
    };
    let style_body = TextStyle {
        size_pt: 12.0,
        bold: false,
        ..Default::default()
    };

    for slide in &doc.slides {
        let page = pdf.add_page(842.0, 595.0); // Landscape presentation page
        pdf.draw_text(page, 56.0, 520.0, &slide.title, &style_title);
        let mut y = 480.0;
        for elem in &slide.elements {
            pdf.draw_text(page, 56.0, y, &elem.content, &style_body);
            y -= 24.0;
        }
    }
    pdf.serialize()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_presentation_creation() {
        let doc = PresentationDocument::new("deck-1", "Quarterly Product Update");
        assert_eq!(doc.len(), 1);
        assert_eq!(doc.slides[0].title, "Title Slide");
    }

    #[test]
    fn test_add_slides() {
        let mut doc = PresentationDocument::new("deck-1", "Quarterly Product Update");
        doc.add_slide("Market Overview", "content");
        doc.add_slide("Financial Performance", "two-column");
        assert_eq!(doc.len(), 3);
        assert_eq!(doc.active_index, 2);
    }

    #[test]
    fn test_save_load_roundtrip() {
        let mut doc = PresentationDocument::new("deck-test", "Architecture Deck");
        doc.add_slide("Slide 2", "blank");
        let bytes = save_presentation(&doc).expect("save failed");
        let loaded = load_presentation(&bytes).expect("load failed");
        assert_eq!(loaded.title, "Architecture Deck");
        assert_eq!(loaded.len(), 2);
    }

    #[test]
    fn test_pdf_export() {
        let doc = PresentationDocument::new("deck-pdf", "PDF Test");
        let pdf_bytes = export_pdf(&doc);
        assert!(!pdf_bytes.is_empty());
    }
}
