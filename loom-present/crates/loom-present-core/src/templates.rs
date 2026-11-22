use crate::{
    ElementStyle, ElementType, PresentationDocument, PresentationSession, Slide, SlideElement,
};
use serde::{Deserialize, Serialize};

/// Rich slide theme palette and typography tokens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlideTheme {
    pub id: String,
    pub name: String,
    #[serde(default = "default_bg")]
    pub background: String,
    #[serde(default = "default_surface")]
    pub surface: String,
    #[serde(default = "default_ink")]
    pub ink: String,
    #[serde(default = "default_muted_ink")]
    pub muted_ink: String,
    #[serde(default = "default_accent")]
    pub accent: String,
    #[serde(default = "default_accent_secondary")]
    pub accent_secondary: String,
    #[serde(default = "default_heading_font")]
    pub heading_font: String,
    #[serde(default = "default_body_font")]
    pub body_font: String,
}

fn default_bg() -> String {
    "#FFFFFF".to_string()
}
fn default_surface() -> String {
    "#F4F6F8".to_string()
}
fn default_ink() -> String {
    "#111827".to_string()
}
fn default_muted_ink() -> String {
    "#6B7280".to_string()
}
fn default_accent() -> String {
    "#2B579A".to_string()
}
fn default_accent_secondary() -> String {
    "#0F766E".to_string()
}
fn default_heading_font() -> String {
    "Inter".to_string()
}
fn default_body_font() -> String {
    "Inter".to_string()
}

impl Default for SlideTheme {
    fn default() -> Self {
        Self {
            id: "default".to_string(),
            name: "Classic Blue".to_string(),
            background: default_bg(),
            surface: default_surface(),
            ink: default_ink(),
            muted_ink: default_muted_ink(),
            accent: default_accent(),
            accent_secondary: default_accent_secondary(),
            heading_font: default_heading_font(),
            body_font: default_body_font(),
        }
    }
}

/// Metadata for a built-in presentation template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateDescriptor {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub theme: SlideTheme,
}

/// Returns all 12 built-in presentation templates with stable IDs.
pub fn template_catalog() -> Vec<TemplateDescriptor> {
    vec![
        TemplateDescriptor {
            id: "blank",
            name: "Blank",
            description: "Clean single-slide canvas",
            theme: SlideTheme::default(),
        },
        TemplateDescriptor {
            id: "title-content",
            name: "Title & Content",
            description: "Classic title slide with body content",
            theme: SlideTheme::default(),
        },
        TemplateDescriptor {
            id: "two-columns",
            name: "Two Columns",
            description: "Side-by-side comparison layout",
            theme: SlideTheme::default(),
        },
        TemplateDescriptor {
            id: "image-text",
            name: "Image & Text",
            description: "Visual feature paired with narrative",
            theme: SlideTheme::default(),
        },
        TemplateDescriptor {
            id: "executive-pitch",
            name: "Executive Pitch",
            description: "High-contrast navy & gold boardroom deck",
            theme: SlideTheme {
                id: "executive-pitch".into(),
                name: "Executive Pitch".into(),
                background: "#0F172A".into(),
                surface: "#1E293B".into(),
                ink: "#F8FAFC".into(),
                muted_ink: "#94A3B8".into(),
                accent: "#F59E0B".into(),
                accent_secondary: "#38BDF8".into(),
                heading_font: "Georgia".into(),
                body_font: "Inter".into(),
            },
        },
        TemplateDescriptor {
            id: "quarterly-review",
            name: "Quarterly Review",
            description: "Structured KPI and milestone review",
            theme: SlideTheme {
                id: "quarterly-review".into(),
                name: "Quarterly Review".into(),
                background: "#FFFFFF".into(),
                surface: "#EFF6FF".into(),
                ink: "#1E293B".into(),
                muted_ink: "#64748B".into(),
                accent: "#1D4ED8".into(),
                accent_secondary: "#059669".into(),
                heading_font: "Inter".into(),
                body_font: "Inter".into(),
            },
        },
        TemplateDescriptor {
            id: "product-launch",
            name: "Product Launch",
            description: "Bold violet & coral announcement deck",
            theme: SlideTheme {
                id: "product-launch".into(),
                name: "Product Launch".into(),
                background: "#18181B".into(),
                surface: "#27272A".into(),
                ink: "#FAFAFA".into(),
                muted_ink: "#A1A1AA".into(),
                accent: "#8B5CF6".into(),
                accent_secondary: "#F43F5E".into(),
                heading_font: "Inter".into(),
                body_font: "Inter".into(),
            },
        },
        TemplateDescriptor {
            id: "data-report",
            name: "Data Report",
            description: "Crisp slate & teal analytical deck",
            theme: SlideTheme {
                id: "data-report".into(),
                name: "Data Report".into(),
                background: "#F8FAFC".into(),
                surface: "#FFFFFF".into(),
                ink: "#0F172A".into(),
                muted_ink: "#475569".into(),
                accent: "#0D9488".into(),
                accent_secondary: "#0284C7".into(),
                heading_font: "Inter".into(),
                body_font: "Inter".into(),
            },
        },
        TemplateDescriptor {
            id: "editorial-portfolio",
            name: "Editorial Portfolio",
            description: "Warm paper & terracotta showcase",
            theme: SlideTheme {
                id: "editorial-portfolio".into(),
                name: "Editorial Portfolio".into(),
                background: "#FAF7F2".into(),
                surface: "#F2ECE1".into(),
                ink: "#292524".into(),
                muted_ink: "#78716C".into(),
                accent: "#C2410C".into(),
                accent_secondary: "#B45309".into(),
                heading_font: "Georgia".into(),
                body_font: "Georgia".into(),
            },
        },
        TemplateDescriptor {
            id: "academic-lecture",
            name: "Academic Lecture",
            description: "Readable serif & forest green lecture slides",
            theme: SlideTheme {
                id: "academic-lecture".into(),
                name: "Academic Lecture".into(),
                background: "#FFFFFF".into(),
                surface: "#F0FDF4".into(),
                ink: "#14532D".into(),
                muted_ink: "#4B5563".into(),
                accent: "#15803D".into(),
                accent_secondary: "#0369A1".into(),
                heading_font: "Georgia".into(),
                body_font: "Inter".into(),
            },
        },
        TemplateDescriptor {
            id: "workshop-deck",
            name: "Workshop Deck",
            description: "Energetic indigo & amber interactive deck",
            theme: SlideTheme {
                id: "workshop-deck".into(),
                name: "Workshop Deck".into(),
                background: "#FFFBEB".into(),
                surface: "#FEF3C7".into(),
                ink: "#1E1B4B".into(),
                muted_ink: "#4338CA".into(),
                accent: "#4F46E5".into(),
                accent_secondary: "#D97706".into(),
                heading_font: "Inter".into(),
                body_font: "Inter".into(),
            },
        },
        TemplateDescriptor {
            id: "minimal-keynote",
            name: "Minimal Keynote",
            description: "Monochrome high-whitespace keynote",
            theme: SlideTheme {
                id: "minimal-keynote".into(),
                name: "Minimal Keynote".into(),
                background: "#FFFFFF".into(),
                surface: "#F4F4F5".into(),
                ink: "#09090B".into(),
                muted_ink: "#71717A".into(),
                accent: "#18181B".into(),
                accent_secondary: "#52525B".into(),
                heading_font: "Inter".into(),
                body_font: "Inter".into(),
            },
        },
    ]
}

fn make_elem(
    id: &str,
    element_type: ElementType,
    content: &str,
    rect: (f32, f32, f32, f32),
    style: ElementStyle,
) -> SlideElement {
    SlideElement {
        id: id.to_string(),
        element_type,
        content: content.to_string(),
        x: rect.0,
        y: rect.1,
        width: rect.2,
        height: rect.3,
        rotation_deg: 0.0,
        action: None,
        style,
        runs: Vec::new(),
    }
}

/// Creates a new [`PresentationDocument`] from one of the 12 template IDs.
pub fn document_from_template(template_id: &str, title: &str) -> PresentationDocument {
    let catalog = template_catalog();
    let desc = catalog
        .into_iter()
        .find(|t| t.id == template_id)
        .unwrap_or_else(|| template_catalog().into_iter().next().unwrap());
    let mut doc = PresentationDocument::new(format!("deck-{template_id}"), title);
    let mut slide1 = Slide::new("slide-1", title, "16:9");
    slide1.bg_color = desc.theme.background.clone();

    if template_id != "blank" {
        slide1.elements.push(make_elem(
            "title-1",
            ElementType::Title,
            title,
            (60.0, 48.0, 880.0, 90.0),
            ElementStyle {
                color: Some(desc.theme.ink.clone()),
                font_family: Some(desc.theme.heading_font.clone()),
                ..Default::default()
            },
        ));
        slide1.elements.push(make_elem(
            "body-1",
            ElementType::BodyText,
            "Click to add content",
            (60.0, 170.0, 880.0, 320.0),
            ElementStyle {
                color: Some(desc.theme.muted_ink.clone()),
                font_family: Some(desc.theme.body_font.clone()),
                ..Default::default()
            },
        ));
    }

    if matches!(
        template_id,
        "executive-pitch"
            | "quarterly-review"
            | "product-launch"
            | "data-report"
            | "editorial-portfolio"
            | "academic-lecture"
            | "workshop-deck"
            | "minimal-keynote"
    ) {
        slide1.elements.push(make_elem(
            "accent-bar-1",
            ElementType::ShapeRectangle,
            "",
            (60.0, 32.0, 96.0, 8.0),
            ElementStyle {
                fill: Some(desc.theme.accent.clone()),
                ..Default::default()
            },
        ));
        let mut slide2 = Slide::new("slide-2", "Key Highlights", "16:9");
        slide2.bg_color = desc.theme.background.clone();
        slide2.elements.push(make_elem(
            "title-2",
            ElementType::Title,
            "Key Highlights",
            (60.0, 40.0, 880.0, 72.0),
            ElementStyle {
                color: Some(desc.theme.ink.clone()),
                font_family: Some(desc.theme.heading_font.clone()),
                ..Default::default()
            },
        ));
        slide2.elements.push(make_elem(
            "stat-2",
            ElementType::StatCard,
            "98% Ready",
            (60.0, 140.0, 260.0, 140.0),
            ElementStyle {
                fill: Some(desc.theme.accent.clone()),
                color: Some("#ffffff".to_string()),
                font_family: Some(desc.theme.heading_font.clone()),
                ..Default::default()
            },
        ));
        slide2.elements.push(make_elem(
            "body-2",
            ElementType::BodyText,
            "Summarize key metrics, milestones, and takeaways.",
            (350.0, 140.0, 590.0, 320.0),
            ElementStyle {
                color: Some(desc.theme.ink.clone()),
                font_family: Some(desc.theme.body_font.clone()),
                ..Default::default()
            },
        ));
        doc.slides = vec![slide1, slide2];
    } else {
        doc.slides = vec![slide1];
    }
    doc
}

/// Applies a theme preset by ID to all slides in `session` as a single undoable step.
pub fn apply_theme_preset(session: &mut PresentationSession, theme_id: &str) -> Result<(), String> {
    let desc = template_catalog()
        .into_iter()
        .find(|t| t.id == theme_id)
        .ok_or_else(|| format!("Unknown theme preset '{theme_id}'"))?;
    session.checkpoint();
    for slide in &mut session.document.slides {
        slide.bg_color = desc.theme.background.clone();
        for elem in &mut slide.elements {
            match elem.element_type {
                ElementType::Title | ElementType::Subtitle => {
                    elem.style.color = Some(desc.theme.ink.clone());
                    elem.style.font_family = Some(desc.theme.heading_font.clone());
                }
                ElementType::BodyText => {
                    elem.style.color = Some(desc.theme.ink.clone());
                    elem.style.font_family = Some(desc.theme.body_font.clone());
                }
                ElementType::ShapeRectangle
                | ElementType::ShapeCircle
                | ElementType::ShapeRoundedRect
                | ElementType::ShapeArrowRight
                | ElementType::ShapeLine
                | ElementType::ShapeCallout
                | ElementType::StatCard => {
                    elem.style.fill = Some(desc.theme.accent.clone());
                }
                ElementType::Picture | ElementType::Table | ElementType::Chart => {}
            }
        }
    }
    Ok(())
}
