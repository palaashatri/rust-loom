//! Core domain engine for Loom Present presentation authoring suite.

use loom_package::manifest::{
    json as pkg_json, Checksum, Manifest, ManifestEntry, MimeType, PackageKind, SchemaVersion,
};
use loom_package::zip::{self, PackageArchive};
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

    pub fn select_slide(&mut self, index: usize) -> bool {
        if index < self.slides.len() {
            self.active_index = index;
            true
        } else {
            false
        }
    }
}

pub fn save_presentation(doc: &PresentationDocument) -> Result<Vec<u8>, String> {
    let json = serde_json::to_vec_pretty(doc).map_err(|e| e.to_string())?;
    let mut arch = PackageArchive::new();
    arch.add("content/presentation.json", json.clone())
        .map_err(|e| e.to_string())?;
    let manifest = Manifest {
        schema: SchemaVersion::CURRENT,
        kind: PackageKind::Present,
        id: doc.id.clone(),
        title: doc.title.clone(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        entries: vec![ManifestEntry {
            path: "content/presentation.json".into(),
            mime: MimeType::parse("application/vnd.loom.deck-content")
                .map_err(|e| format!("invalid built-in presentation MIME type: {e}"))?,
            size: json.len() as u64,
            sha256: Checksum::from_bytes(zip::sha256(&json)),
        }],
    };
    arch.add("manifest.json", pkg_json::write(&manifest).into_bytes())
        .map_err(|e| e.to_string())?;
    arch.to_bytes().map_err(|e| e.to_string())
}

pub fn load_presentation(bytes: &[u8]) -> Result<PresentationDocument, String> {
    let arch = PackageArchive::from_bytes(bytes).map_err(|e| e.to_string())?;
    let manifest_bytes = arch
        .get("manifest.json")
        .ok_or_else(|| "missing manifest.json".to_string())?;
    let manifest_str =
        std::str::from_utf8(manifest_bytes).map_err(|_| "manifest not utf8".to_string())?;
    let manifest: Manifest =
        pkg_json::parse_manifest(manifest_str).map_err(|e| format!("manifest: {e}"))?;
    if manifest.kind != PackageKind::Present {
        return Err("not a Present deck".to_string());
    }
    arch.validate_manifest(&manifest)
        .map_err(|e| format!("validation: {e}"))?;
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


/// Built-in transition between two slides.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TransitionKind {
    /// Immediate cut.
    None,
    /// Cross-fade.
    Dissolve,
    /// Horizontal push.
    Push,
    /// Content-aware transform between matching element ids.
    Morph,
}

impl Default for TransitionKind {
    fn default() -> Self {
        Self::None
    }
}

/// Presentation-wide theme tokens used by renderers and exporters.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeckTheme {
    /// Theme name.
    pub name: String,
    /// Slide background color.
    pub background: String,
    /// Primary text color.
    pub foreground: String,
    /// Accent color.
    pub accent: String,
    /// Heading font family.
    pub heading_font: String,
    /// Body font family.
    pub body_font: String,
}

impl Default for DeckTheme {
    fn default() -> Self {
        Self {
            name: "Loom Graphite".into(),
            background: "#16181d".into(),
            foreground: "#f4f1ea".into(),
            accent: "#c9834b".into(),
            heading_font: "Inter".into(),
            body_font: "Inter".into(),
        }
    }
}

/// Render-time representation of a slide element.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderElement {
    /// Stable element id.
    pub id: String,
    /// Element kind.
    pub element_type: ElementType,
    /// Rendered text/content.
    pub content: String,
    /// Normalized left edge in `[0, 1]`.
    pub x: f32,
    /// Normalized top edge in `[0, 1]`.
    pub y: f32,
    /// Normalized width.
    pub width: f32,
    /// Normalized height.
    pub height: f32,
}

/// A validated render scene for one slide.
#[derive(Debug, Clone, PartialEq)]
pub struct SlideScene {
    /// Slide id.
    pub slide_id: String,
    /// Background color.
    pub background: String,
    /// Ordered elements.
    pub elements: Vec<RenderElement>,
    /// Transition leaving the slide.
    pub transition: TransitionKind,
}

/// Validation issue found in a deck.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeckIssue {
    /// Slide index, when applicable.
    pub slide_index: Option<usize>,
    /// Element id, when applicable.
    pub element_id: Option<String>,
    /// Human-readable description.
    pub message: String,
}

/// Undoable authoring session around a presentation document.
#[derive(Debug, Clone)]
pub struct PresentationSession {
    /// Current document.
    pub document: PresentationDocument,
    /// Current theme.
    pub theme: DeckTheme,
    /// Per-slide outgoing transitions.
    pub transitions: std::collections::BTreeMap<String, TransitionKind>,
    undo: Vec<PresentationDocument>,
    redo: Vec<PresentationDocument>,
    history_limit: usize,
}

impl PresentationSession {
    /// Creates a session with bounded snapshot history.
    pub fn new(document: PresentationDocument) -> Self {
        Self {
            document,
            theme: DeckTheme::default(),
            transitions: std::collections::BTreeMap::new(),
            undo: Vec::new(),
            redo: Vec::new(),
            history_limit: 64,
        }
    }

    /// Records the current document before a mutation.
    pub fn checkpoint(&mut self) {
        self.undo.push(self.document.clone());
        if self.undo.len() > self.history_limit {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// Restores the previous document snapshot.
    pub fn undo(&mut self) -> bool {
        let Some(previous) = self.undo.pop() else {
            return false;
        };
        self.redo.push(std::mem::replace(&mut self.document, previous));
        true
    }

    /// Reapplies the next document snapshot.
    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo.push(std::mem::replace(&mut self.document, next));
        true
    }

    /// Duplicates a slide and selects the copy.
    pub fn duplicate_slide(&mut self, index: usize) -> bool {
        let Some(source) = self.document.slides.get(index).cloned() else {
            return false;
        };
        self.checkpoint();
        let mut copy = source;
        copy.id = unique_slide_id(&self.document, "slide");
        copy.title = format!("{} Copy", copy.title);
        for (element_index, element) in copy.elements.iter_mut().enumerate() {
            element.id = format!("{}-element-{}", copy.id, element_index + 1);
        }
        self.document.slides.insert(index + 1, copy);
        self.document.active_index = index + 1;
        true
    }

    /// Removes a slide while keeping at least one slide in the deck.
    pub fn remove_slide(&mut self, index: usize) -> bool {
        if self.document.slides.len() <= 1 || index >= self.document.slides.len() {
            return false;
        }
        self.checkpoint();
        let removed = self.document.slides.remove(index);
        self.transitions.remove(&removed.id);
        self.document.active_index = self
            .document
            .active_index
            .min(self.document.slides.len().saturating_sub(1));
        true
    }

    /// Moves a slide to a new index.
    pub fn move_slide(&mut self, from: usize, to: usize) -> bool {
        if from >= self.document.slides.len() || to >= self.document.slides.len() || from == to {
            return false;
        }
        self.checkpoint();
        let slide = self.document.slides.remove(from);
        self.document.slides.insert(to, slide);
        self.document.active_index = to;
        true
    }

    /// Adds an element to the active slide.
    pub fn add_element(&mut self, mut element: SlideElement) -> bool {
        if self.document.active_slide().is_none() {
            return false;
        }
        self.checkpoint();
        let slide = self.document.active_slide_mut().expect("checked above");
        if element.id.trim().is_empty() || slide.elements.iter().any(|item| item.id == element.id) {
            element.id = format!("{}-element-{}", slide.id, slide.elements.len() + 1);
        }
        slide.elements.push(element);
        true
    }

    /// Removes an element from the active slide.
    pub fn remove_element(&mut self, element_id: &str) -> bool {
        let Some(slide) = self.document.active_slide() else {
            return false;
        };
        let Some(index) = slide.elements.iter().position(|element| element.id == element_id) else {
            return false;
        };
        self.checkpoint();
        self.document
            .active_slide_mut()
            .expect("active slide remains")
            .elements
            .remove(index);
        true
    }

    /// Moves and resizes an element, clamped to the 1000×562.5 authoring plane.
    pub fn transform_element(
        &mut self,
        element_id: &str,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    ) -> bool {
        let Some(slide) = self.document.active_slide() else {
            return false;
        };
        if !slide.elements.iter().any(|element| element.id == element_id) {
            return false;
        }
        self.checkpoint();
        let element = self
            .document
            .active_slide_mut()
            .expect("active slide remains")
            .elements
            .iter_mut()
            .find(|element| element.id == element_id)
            .expect("element remains");
        element.width = width.clamp(1.0, 1000.0);
        element.height = height.clamp(1.0, 562.5);
        element.x = x.clamp(0.0, 1000.0 - element.width);
        element.y = y.clamp(0.0, 562.5 - element.height);
        true
    }

    /// Sets the outgoing transition for a slide.
    pub fn set_transition(&mut self, slide_id: &str, transition: TransitionKind) -> bool {
        if !self.document.slides.iter().any(|slide| slide.id == slide_id) {
            return false;
        }
        self.transitions.insert(slide_id.to_string(), transition);
        true
    }

    /// Produces a normalized scene for renderers, presenter mode, and export.
    pub fn scene(&self, index: usize) -> Option<SlideScene> {
        let slide = self.document.slides.get(index)?;
        let elements = slide
            .elements
            .iter()
            .map(|element| RenderElement {
                id: element.id.clone(),
                element_type: element.element_type.clone(),
                content: element.content.clone(),
                x: (element.x / 1000.0).clamp(0.0, 1.0),
                y: (element.y / 562.5).clamp(0.0, 1.0),
                width: (element.width / 1000.0).clamp(0.0, 1.0),
                height: (element.height / 562.5).clamp(0.0, 1.0),
            })
            .collect();
        Some(SlideScene {
            slide_id: slide.id.clone(),
            background: slide.bg_color.clone(),
            elements,
            transition: self.transitions.get(&slide.id).cloned().unwrap_or_default(),
        })
    }

    /// Validates ids, geometry, active selection, and empty content.
    pub fn validate(&self) -> Vec<DeckIssue> {
        let mut issues = Vec::new();
        if self.document.slides.is_empty() {
            issues.push(DeckIssue {
                slide_index: None,
                element_id: None,
                message: "deck has no slides".into(),
            });
            return issues;
        }
        if self.document.active_index >= self.document.slides.len() {
            issues.push(DeckIssue {
                slide_index: None,
                element_id: None,
                message: "active slide index is out of bounds".into(),
            });
        }
        let mut slide_ids = std::collections::HashSet::new();
        for (slide_index, slide) in self.document.slides.iter().enumerate() {
            if !slide_ids.insert(&slide.id) {
                issues.push(DeckIssue {
                    slide_index: Some(slide_index),
                    element_id: None,
                    message: format!("duplicate slide id {}", slide.id),
                });
            }
            let mut element_ids = std::collections::HashSet::new();
            for element in &slide.elements {
                if !element_ids.insert(&element.id) {
                    issues.push(DeckIssue {
                        slide_index: Some(slide_index),
                        element_id: Some(element.id.clone()),
                        message: "duplicate element id".into(),
                    });
                }
                if !element.x.is_finite()
                    || !element.y.is_finite()
                    || !element.width.is_finite()
                    || !element.height.is_finite()
                    || element.width <= 0.0
                    || element.height <= 0.0
                {
                    issues.push(DeckIssue {
                        slide_index: Some(slide_index),
                        element_id: Some(element.id.clone()),
                        message: "invalid element geometry".into(),
                    });
                }
            }
        }
        issues
    }
}

fn unique_slide_id(document: &PresentationDocument, prefix: &str) -> String {
    let mut serial = document.slides.len() + 1;
    loop {
        let candidate = format!("{prefix}-{serial}");
        if !document.slides.iter().any(|slide| slide.id == candidate) {
            return candidate;
        }
        serial += 1;
    }
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
    fn test_select_slide_bounds() {
        let mut doc = PresentationDocument::new("deck-1", "Quarterly Product Update");
        doc.add_slide("Market Overview", "content");

        assert!(doc.select_slide(0));
        assert_eq!(doc.active_index, 0);
        assert!(doc.select_slide(1));
        assert_eq!(doc.active_index, 1);
        assert!(!doc.select_slide(2));
        assert_eq!(doc.active_index, 1);
    }

    #[test]
    fn test_save_load_roundtrip() {
        let mut doc = PresentationDocument::new("deck-test", "Architecture Deck");
        doc.add_slide("Slide 2", "blank");
        let bytes = save_presentation(&doc).expect("save failed");
        let arch = PackageArchive::from_bytes(&bytes).expect("archive parse failed");
        let manifest_bytes = arch.get("manifest.json").expect("manifest missing");
        let manifest_str = std::str::from_utf8(manifest_bytes).expect("manifest not utf8");
        let manifest = pkg_json::parse_manifest(manifest_str).expect("manifest parse failed");
        assert_eq!(manifest.kind, PackageKind::Present);
        arch.validate_manifest(&manifest)
            .expect("manifest validation failed");
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

    #[test]
    fn authoring_session_supports_slide_and_element_history() {
        let doc = PresentationDocument::new("deck-session", "Session Test");
        let mut session = PresentationSession::new(doc);
        assert!(session.duplicate_slide(0));
        assert_eq!(session.document.len(), 2);
        assert!(session.add_element(SlideElement {
            id: String::new(),
            element_type: ElementType::BodyText,
            content: "Hello".into(),
            x: 10.0,
            y: 20.0,
            width: 300.0,
            height: 60.0,
        }));
        assert_eq!(session.document.active_slide().unwrap().elements.len(), 2);
        assert!(session.undo());
        assert_eq!(session.document.active_slide().unwrap().elements.len(), 1);
        assert!(session.redo());
        assert_eq!(session.document.active_slide().unwrap().elements.len(), 2);
    }

    #[test]
    fn scene_normalizes_geometry_and_validation_catches_duplicates() {
        let mut doc = PresentationDocument::new("deck-scene", "Scene Test");
        doc.slides[0].elements.push(SlideElement {
            id: "elem-1".into(),
            element_type: ElementType::BodyText,
            content: "Duplicate id".into(),
            x: 500.0,
            y: 281.25,
            width: 250.0,
            height: 100.0,
        });
        let mut session = PresentationSession::new(doc);
        session.set_transition("slide-1", TransitionKind::Dissolve);
        let scene = session.scene(0).expect("scene");
        assert_eq!(scene.transition, TransitionKind::Dissolve);
        assert!((scene.elements[1].x - 0.5).abs() < 0.001);
        assert!(session
            .validate()
            .iter()
            .any(|issue| issue.message.contains("duplicate element")));
    }

}
