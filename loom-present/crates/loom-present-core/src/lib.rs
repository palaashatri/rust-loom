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

    pub fn remove_element(&mut self, id: &str) -> bool {
        if let Some(pos) = self.elements.iter().position(|e| e.id == id) {
            self.elements.remove(pos);
            true
        } else {
            false
        }
    }

    pub fn bring_to_front(&mut self, id: &str) -> bool {
        if let Some(pos) = self.elements.iter().position(|e| e.id == id) {
            let elem = self.elements.remove(pos);
            self.elements.push(elem);
            true
        } else {
            false
        }
    }

    pub fn send_to_back(&mut self, id: &str) -> bool {
        if let Some(pos) = self.elements.iter().position(|e| e.id == id) {
            let elem = self.elements.remove(pos);
            self.elements.insert(0, elem);
            true
        } else {
            false
        }
    }

    pub fn bring_forward(&mut self, id: &str) -> bool {
        if let Some(pos) = self.elements.iter().position(|e| e.id == id) {
            if pos + 1 < self.elements.len() {
                self.elements.swap(pos, pos + 1);
                return true;
            }
        }
        false
    }

    pub fn send_backward(&mut self, id: &str) -> bool {
        if let Some(pos) = self.elements.iter().position(|e| e.id == id) {
            if pos > 0 {
                self.elements.swap(pos, pos - 1);
                return true;
            }
        }
        false
    }

    pub fn align_left(&mut self, ids: &[&str]) {
        let min_x = ids
            .iter()
            .filter_map(|id| self.elements.iter().find(|e| &e.id == id))
            .map(|e| e.x)
            .fold(f32::INFINITY, f32::min);
        if min_x.is_finite() {
            for e in &mut self.elements {
                if ids.contains(&e.id.as_str()) {
                    e.x = min_x;
                }
            }
        }
    }

    pub fn align_center(&mut self, ids: &[&str]) {
        let centers: Vec<f32> = ids
            .iter()
            .filter_map(|id| self.elements.iter().find(|e| &e.id == id))
            .map(|e| e.x + e.width / 2.0)
            .collect();
        if !centers.is_empty() {
            let avg_center = centers.iter().sum::<f32>() / centers.len() as f32;
            for e in &mut self.elements {
                if ids.contains(&e.id.as_str()) {
                    e.x = avg_center - e.width / 2.0;
                }
            }
        }
    }

    pub fn align_top(&mut self, ids: &[&str]) {
        let min_y = ids
            .iter()
            .filter_map(|id| self.elements.iter().find(|e| &e.id == id))
            .map(|e| e.y)
            .fold(f32::INFINITY, f32::min);
        if min_y.is_finite() {
            for e in &mut self.elements {
                if ids.contains(&e.id.as_str()) {
                    e.y = min_y;
                }
            }
        }
    }

    /// Applies a layout preset, configuring placeholder elements with standard positions.
    pub fn apply_layout_preset(&mut self, preset: SlideLayoutPreset) {
        self.elements.clear();
        match preset {
            SlideLayoutPreset::TitleSlide => {
                self.layout = "title".into();
                self.add_element(SlideElement {
                    id: "title-1".into(),
                    element_type: ElementType::Title,
                    content: self.title.clone(),
                    x: 100.0,
                    y: 250.0,
                    width: 1720.0,
                    height: 120.0,
                });
                self.add_element(SlideElement {
                    id: "sub-1".into(),
                    element_type: ElementType::Subtitle,
                    content: "Subtitle or author name".into(),
                    x: 100.0,
                    y: 400.0,
                    width: 1720.0,
                    height: 80.0,
                });
            }
            SlideLayoutPreset::TitleAndContent => {
                self.layout = "title_content".into();
                self.add_element(SlideElement {
                    id: "title-1".into(),
                    element_type: ElementType::Title,
                    content: self.title.clone(),
                    x: 80.0,
                    y: 60.0,
                    width: 1760.0,
                    height: 100.0,
                });
                self.add_element(SlideElement {
                    id: "body-1".into(),
                    element_type: ElementType::BodyText,
                    content: "• Key point one\n• Key point two\n• Key point three".into(),
                    x: 80.0,
                    y: 200.0,
                    width: 1760.0,
                    height: 780.0,
                });
            }
            SlideLayoutPreset::TwoColumn => {
                self.layout = "two_column".into();
                self.add_element(SlideElement {
                    id: "title-1".into(),
                    element_type: ElementType::Title,
                    content: self.title.clone(),
                    x: 80.0,
                    y: 60.0,
                    width: 1760.0,
                    height: 100.0,
                });
                self.add_element(SlideElement {
                    id: "col-1".into(),
                    element_type: ElementType::BodyText,
                    content: "Column 1 notes and content".into(),
                    x: 80.0,
                    y: 200.0,
                    width: 850.0,
                    height: 780.0,
                });
                self.add_element(SlideElement {
                    id: "col-2".into(),
                    element_type: ElementType::BodyText,
                    content: "Column 2 notes and content".into(),
                    x: 990.0,
                    y: 200.0,
                    width: 850.0,
                    height: 780.0,
                });
            }
            SlideLayoutPreset::Quote => {
                self.layout = "quote".into();
                self.add_element(SlideElement {
                    id: "quote-1".into(),
                    element_type: ElementType::Title,
                    content: "\"Inspiring creative suite quotation\"".into(),
                    x: 150.0,
                    y: 350.0,
                    width: 1620.0,
                    height: 200.0,
                });
            }
            SlideLayoutPreset::BigStat => {
                self.layout = "big_stat".into();
                self.add_element(SlideElement {
                    id: "stat-1".into(),
                    element_type: ElementType::StatCard,
                    content: "99.9%".into(),
                    x: 200.0,
                    y: 200.0,
                    width: 1520.0,
                    height: 250.0,
                });
                self.add_element(SlideElement {
                    id: "label-1".into(),
                    element_type: ElementType::Subtitle,
                    content: "System Reliability".into(),
                    x: 200.0,
                    y: 500.0,
                    width: 1520.0,
                    height: 100.0,
                });
            }
        }
    }

    /// Calculates the tight bounding box (min_x, min_y, width, height) enclosing multiple elements.
    pub fn elements_bounding_box(&self, element_ids: &[&str]) -> Option<(f32, f32, f32, f32)> {
        let matching: Vec<&SlideElement> = self
            .elements
            .iter()
            .filter(|e| element_ids.contains(&e.id.as_str()))
            .collect();
        if matching.is_empty() {
            return None;
        }
        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;
        for el in matching {
            min_x = min_x.min(el.x);
            min_y = min_y.min(el.y);
            max_x = max_x.max(el.x + el.width);
            max_y = max_y.max(el.y + el.height);
        }
        Some((
            min_x,
            min_y,
            (max_x - min_x).max(0.0),
            (max_y - min_y).max(0.0),
        ))
    }
}

/// Predefined layout templates for presentation slides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlideLayoutPreset {
    TitleSlide,
    TitleAndContent,
    TwoColumn,
    Quote,
    BigStat,
}

/// Predefined color themes for presentation decks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeckThemePreset {
    ModernDark,
    ClassicLight,
    VibrantGradient,
    MinimalistSlate,
}

impl DeckThemePreset {
    /// Returns (background_hex, primary_text_hex, accent_hex).
    pub fn palette(&self) -> (&'static str, &'static str, &'static str) {
        match self {
            DeckThemePreset::ModernDark => ("#0F172A", "#F8FAFC", "#38BDF8"),
            DeckThemePreset::ClassicLight => ("#FFFFFF", "#0F172A", "#2563EB"),
            DeckThemePreset::VibrantGradient => ("#1E1B4B", "#FDF4FF", "#C084FC"),
            DeckThemePreset::MinimalistSlate => ("#F1F5F9", "#1E293B", "#64748B"),
        }
    }
}

/// Standard slide aspect ratios.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SlideAspectRatio {
    Widescreen16x9,
    Standard4x3,
    Widescreen16x10,
}

impl SlideAspectRatio {
    /// Returns default slide canvas `(width, height)` in points.
    pub fn dimensions(&self) -> (f32, f32) {
        match self {
            SlideAspectRatio::Widescreen16x9 => (960.0, 540.0),
            SlideAspectRatio::Standard4x3 => (720.0, 540.0),
            SlideAspectRatio::Widescreen16x10 => (864.0, 540.0),
        }
    }

    /// Aspect ratio as `(width_ratio, height_ratio)`.
    pub fn ratio(&self) -> (u32, u32) {
        match self {
            SlideAspectRatio::Widescreen16x9 => (16, 9),
            SlideAspectRatio::Standard4x3 => (4, 3),
            SlideAspectRatio::Widescreen16x10 => (16, 10),
        }
    }
}

/// Slide visual transition styles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TransitionType {
    #[default]
    None,
    Fade,
    SlideLeft,
    SlideRight,
    Zoom,
    Flip,
}

/// Slide transition settings.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SlideTransitionConfig {
    pub kind: TransitionType,
    pub duration_seconds: f32,
}

impl Default for SlideTransitionConfig {
    fn default() -> Self {
        Self {
            kind: TransitionType::None,
            duration_seconds: 0.5,
        }
    }
}

/// Border stroke styles for slide shapes and text boxes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum StrokeStyle {
    #[default]
    Solid,
    Dashed,
    Dotted,
    None,
}

/// Stroke / border configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StrokeConfig {
    pub color: String,
    pub width: f32,
    pub style: StrokeStyle,
}

impl Default for StrokeConfig {
    fn default() -> Self {
        Self {
            color: "#000000".into(),
            width: 1.0,
            style: StrokeStyle::Solid,
        }
    }
}

/// Drop shadow effect configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DropShadowConfig {
    pub offset_x: f32,
    pub offset_y: f32,
    pub blur_radius: f32,
    pub color: String,
}

impl Default for DropShadowConfig {
    fn default() -> Self {
        Self {
            offset_x: 2.0,
            offset_y: 2.0,
            blur_radius: 4.0,
            color: "rgba(0, 0, 0, 0.3)".into(),
        }
    }
}

/// Normalizes an angle in degrees into the standard `[0.0, 360.0)` range.
pub fn normalize_angle_degrees(degrees: f32) -> f32 {
    let rem = degrees % 360.0;
    if rem < 0.0 {
        rem + 360.0
    } else {
        rem
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

    pub fn duplicate_slide(&mut self, index: usize) -> Option<usize> {
        if index < self.slides.len() {
            let mut dup = self.slides[index].clone();
            dup.id = format!("slide-{}-copy", self.slides.len() + 1);
            let new_index = index + 1;
            self.slides.insert(new_index, dup);
            self.active_index = new_index;
            Some(new_index)
        } else {
            None
        }
    }

    pub fn remove_slide(&mut self, index: usize) -> Option<Slide> {
        if self.slides.len() > 1 && index < self.slides.len() {
            let slide = self.slides.remove(index);
            self.active_index = self.active_index.min(self.slides.len() - 1);
            Some(slide)
        } else {
            None
        }
    }

    pub fn move_slide(&mut self, from: usize, to: usize) -> bool {
        if from >= self.slides.len() || to >= self.slides.len() || from == to {
            return false;
        }
        let slide = self.slides.remove(from);
        self.slides.insert(to, slide);
        self.active_index = to;
        true
    }

    /// Extracts all speaker notes into an organized Markdown document for rehearsal and export.
    pub fn speaker_notes_markdown(&self) -> String {
        let mut out = format!("# Speaker Notes: {}\n\n", self.title);
        for (i, slide) in self.slides.iter().enumerate() {
            out.push_str(&format!("## Slide {} — {}\n", i + 1, slide.title));
            if slide.speaker_notes.trim().is_empty() {
                out.push_str("*(No notes)*\n\n");
            } else {
                out.push_str(slide.speaker_notes.trim());
                out.push_str("\n\n");
            }
        }
        out
    }

    /// Counts the total number of elements across all slides in the deck.
    pub fn total_elements(&self) -> usize {
        self.slides.iter().map(|s| s.elements.len()).sum()
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
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub enum TransitionKind {
    /// Immediate cut.
    #[default]
    None,
    /// Cross-fade.
    Dissolve,
    /// Horizontal push.
    Push,
    /// Content-aware transform between matching element ids.
    Morph,
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
        self.redo
            .push(std::mem::replace(&mut self.document, previous));
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

    /// Returns whether the session has an undo snapshot.
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// Returns whether the session has a redo snapshot.
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Returns the outgoing transition for a slide.
    pub fn transition_for(&self, slide_id: &str) -> TransitionKind {
        self.transitions.get(slide_id).cloned().unwrap_or_default()
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
        let Some(index) = slide
            .elements
            .iter()
            .position(|element| element.id == element_id)
        else {
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
        if !slide
            .elements
            .iter()
            .any(|element| element.id == element_id)
        {
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
        if !self
            .document
            .slides
            .iter()
            .any(|slide| slide.id == slide_id)
        {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedPresentationSession {
    document: PresentationDocument,
    theme: DeckTheme,
    transitions: std::collections::BTreeMap<String, TransitionKind>,
}

/// Serializes the complete presentation session, including theme and transitions.
pub fn save_presentation_session(session: &PresentationSession) -> Result<Vec<u8>, String> {
    let persisted = PersistedPresentationSession {
        document: session.document.clone(),
        theme: session.theme.clone(),
        transitions: session.transitions.clone(),
    };
    let json = serde_json::to_vec_pretty(&persisted).map_err(|error| error.to_string())?;
    let mut archive = PackageArchive::new();
    archive
        .add("content/presentation-session.json", json.clone())
        .map_err(|error| error.to_string())?;
    let manifest = Manifest {
        schema: SchemaVersion::CURRENT,
        kind: PackageKind::Present,
        id: session.document.id.clone(),
        title: session.document.title.clone(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        entries: vec![ManifestEntry {
            path: "content/presentation-session.json".into(),
            mime: MimeType::parse("application/vnd.loom.deck-session")
                .map_err(|error| format!("invalid built-in presentation MIME type: {error}"))?,
            size: json.len() as u64,
            sha256: Checksum::from_bytes(zip::sha256(&json)),
        }],
    };
    archive
        .add("manifest.json", pkg_json::write(&manifest).into_bytes())
        .map_err(|error| error.to_string())?;
    archive.to_bytes().map_err(|error| error.to_string())
}

/// Loads a complete presentation session, accepting legacy document-only decks.
pub fn load_presentation_session(bytes: &[u8]) -> Result<PresentationSession, String> {
    let archive = PackageArchive::from_bytes(bytes).map_err(|error| error.to_string())?;
    let manifest_bytes = archive
        .get("manifest.json")
        .ok_or_else(|| "missing manifest.json".to_string())?;
    let manifest_text =
        std::str::from_utf8(manifest_bytes).map_err(|_| "manifest is not UTF-8".to_string())?;
    let manifest =
        pkg_json::parse_manifest(manifest_text).map_err(|error| format!("manifest: {error}"))?;
    if manifest.kind != PackageKind::Present {
        return Err("not a Loom Present deck".into());
    }
    archive
        .validate_manifest(&manifest)
        .map_err(|error| format!("validation: {error}"))?;
    if let Some(content) = archive.get("content/presentation-session.json") {
        let persisted: PersistedPresentationSession = serde_json::from_slice(content)
            .map_err(|error| format!("parse presentation session: {error}"))?;
        let mut session = PresentationSession::new(persisted.document);
        session.theme = persisted.theme;
        session.transitions = persisted.transitions;
        return Ok(session);
    }
    load_presentation(bytes).map(PresentationSession::new)
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

    #[test]
    fn session_persistence_preserves_theme_and_transitions() {
        let mut session = PresentationSession::new(PresentationDocument::new("deck", "Deck"));
        session.theme.accent = "#ff8800".into();
        let slide_id = session.document.slides[0].id.clone();
        assert!(session.set_transition(&slide_id, TransitionKind::Dissolve));
        let bytes = save_presentation_session(&session).unwrap();
        let loaded = load_presentation_session(&bytes).unwrap();
        assert_eq!(loaded.theme.accent, "#ff8800");
        assert_eq!(loaded.transition_for(&slide_id), TransitionKind::Dissolve);
    }

    #[test]
    fn history_capabilities_track_mutations() {
        let mut session = PresentationSession::new(PresentationDocument::new("deck", "Deck"));
        assert!(!session.can_undo());
        session.duplicate_slide(0);
        assert!(session.can_undo());
        session.undo();
        assert!(session.can_redo());
    }

    #[test]
    fn slide_element_ordering_and_alignment_operations() {
        let mut slide = Slide::new("slide-test", "Test Slide", "blank");
        slide.add_element(SlideElement {
            id: "e1".into(),
            element_type: ElementType::Title,
            content: "First".into(),
            x: 50.0,
            y: 100.0,
            width: 200.0,
            height: 50.0,
        });
        slide.add_element(SlideElement {
            id: "e2".into(),
            element_type: ElementType::BodyText,
            content: "Second".into(),
            x: 150.0,
            y: 200.0,
            width: 200.0,
            height: 50.0,
        });

        assert_eq!(slide.elements.len(), 2);
        assert!(slide.bring_to_front("e1"));
        assert_eq!(slide.elements[1].id, "e1");

        assert!(slide.send_to_back("e1"));
        assert_eq!(slide.elements[0].id, "e1");

        slide.align_left(&["e1", "e2"]);
        assert_eq!(slide.elements[0].x, 50.0);
        assert_eq!(slide.elements[1].x, 50.0);

        assert!(slide.remove_element("e1"));
        assert_eq!(slide.elements.len(), 1);
    }

    #[test]
    fn slide_deck_duplicate_move_and_remove() {
        let mut doc = PresentationDocument::new("deck-ops", "Deck Operations");
        assert_eq!(doc.len(), 1);
        doc.add_slide("Slide 2", "bullets");
        assert_eq!(doc.len(), 2);

        // Duplicate slide 0
        let dup_idx = doc.duplicate_slide(0).unwrap();
        assert_eq!(dup_idx, 1);
        assert_eq!(doc.len(), 3);

        // Move slide 0 to 2
        assert!(doc.move_slide(0, 2));
        assert_eq!(doc.active_index, 2);

        // Remove slide 1
        let removed = doc.remove_slide(1);
        assert!(removed.is_some());
        assert_eq!(doc.len(), 2);
    }

    #[test]
    fn slide_layout_presets_configure_elements() {
        let mut slide = Slide::new("slide-p", "Q3 Review", "blank");
        slide.apply_layout_preset(SlideLayoutPreset::TitleSlide);
        assert_eq!(slide.layout, "title");
        assert_eq!(slide.elements.len(), 2);
        assert_eq!(slide.elements[0].content, "Q3 Review");

        slide.apply_layout_preset(SlideLayoutPreset::TwoColumn);
        assert_eq!(slide.layout, "two_column");
        assert_eq!(slide.elements.len(), 3);

        slide.apply_layout_preset(SlideLayoutPreset::BigStat);
        assert_eq!(slide.layout, "big_stat");
        assert_eq!(slide.elements.len(), 2);
    }

    #[test]
    fn speaker_notes_markdown_and_total_elements() {
        let mut doc = PresentationDocument::new("deck-notes", "Keynote 2026");
        doc.slides[0].speaker_notes = "Welcome the audience.".to_string();
        doc.add_slide("Product Demo", "content");
        doc.slides[1].speaker_notes = "Show live preview.".to_string();

        assert_eq!(doc.total_elements(), 1); // 1 on title slide, 0 on new slide

        let md = doc.speaker_notes_markdown();
        assert!(md.contains("# Speaker Notes: Keynote 2026"));
        assert!(md.contains("## Slide 1 — Title Slide"));
        assert!(md.contains("Welcome the audience."));
        assert!(md.contains("## Slide 2 — Product Demo"));
        assert!(md.contains("Show live preview."));
    }

    #[test]
    fn elements_bounding_box_union() {
        let mut slide = Slide::new("s1", "Slide 1", "custom");
        slide.add_element(SlideElement {
            id: "e1".into(),
            element_type: ElementType::BodyText,
            content: "Box 1".into(),
            x: 100.0,
            y: 100.0,
            width: 200.0,
            height: 100.0,
        });
        slide.add_element(SlideElement {
            id: "e2".into(),
            element_type: ElementType::ShapeRectangle,
            content: "Box 2".into(),
            x: 250.0,
            y: 150.0,
            width: 150.0,
            height: 100.0,
        });

        let bbox = slide.elements_bounding_box(&["e1", "e2"]).unwrap();
        // min_x = 100, min_y = 100, max_x = 400 (width = 300), max_y = 250 (height = 150)
        assert_eq!(bbox, (100.0, 100.0, 300.0, 150.0));

        // Missing elements return None
        assert!(slide.elements_bounding_box(&["e99"]).is_none());
    }

    #[test]
    fn deck_theme_presets() {
        let (dark_bg, dark_fg, dark_accent) = DeckThemePreset::ModernDark.palette();
        assert!(dark_bg.starts_with('#'));
        assert!(dark_fg.starts_with('#'));
        assert!(dark_accent.starts_with('#'));

        let (light_bg, light_fg, _) = DeckThemePreset::ClassicLight.palette();
        assert_eq!(light_bg, "#FFFFFF");
        assert_eq!(light_fg, "#0F172A");
    }

    #[test]
    fn slide_aspect_ratio_presets() {
        assert_eq!(
            SlideAspectRatio::Widescreen16x9.dimensions(),
            (960.0, 540.0)
        );
        assert_eq!(SlideAspectRatio::Widescreen16x9.ratio(), (16, 9));

        assert_eq!(SlideAspectRatio::Standard4x3.dimensions(), (720.0, 540.0));
        assert_eq!(SlideAspectRatio::Standard4x3.ratio(), (4, 3));
    }

    #[test]
    fn slide_transition_config() {
        let def = SlideTransitionConfig::default();
        assert_eq!(def.kind, TransitionType::None);
        assert_eq!(def.duration_seconds, 0.5);

        let fade = SlideTransitionConfig {
            kind: TransitionType::Fade,
            duration_seconds: 1.0,
        };
        assert_eq!(fade.kind, TransitionType::Fade);
        assert_eq!(fade.duration_seconds, 1.0);
    }

    #[test]
    fn stroke_and_shadow_styling() {
        let stroke = StrokeConfig {
            color: "#ff0000".into(),
            width: 2.5,
            style: StrokeStyle::Dashed,
        };
        assert_eq!(stroke.width, 2.5);
        assert_eq!(stroke.style, StrokeStyle::Dashed);

        let shadow = DropShadowConfig::default();
        assert_eq!(shadow.blur_radius, 4.0);

        assert_eq!(normalize_angle_degrees(0.0), 0.0);
        assert_eq!(normalize_angle_degrees(360.0), 0.0);
        assert_eq!(normalize_angle_degrees(450.0), 90.0);
        assert_eq!(normalize_angle_degrees(-90.0), 270.0);
    }
}
