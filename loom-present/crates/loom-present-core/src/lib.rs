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

/// Interactive click action trigger for presentation slides and elements.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SlideActionTrigger {
    NextSlide,
    PreviousSlide,
    FirstSlide,
    LastSlide,
    JumpToSlide(usize),
    OpenUrl(String),
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
    #[serde(default)]
    pub action: Option<SlideActionTrigger>,
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

    /// Distributes selected element centers evenly between the first and last element
    /// (by current center) along the horizontal axis. Fewer than three selected
    /// elements is a no-op.
    pub fn distribute_horizontally(&mut self, ids: &[&str]) {
        let mut indices: Vec<usize> = self
            .elements
            .iter()
            .enumerate()
            .filter(|(_, e)| ids.contains(&e.id.as_str()))
            .map(|(i, _)| i)
            .collect();
        if indices.len() < 3 {
            return;
        }
        indices.sort_by(|&a, &b| {
            let ca = self.elements[a].x + self.elements[a].width / 2.0;
            let cb = self.elements[b].x + self.elements[b].width / 2.0;
            ca.partial_cmp(&cb).unwrap_or(std::cmp::Ordering::Equal)
        });
        let first = indices[0];
        let last = *indices.last().expect("non-empty checked above");
        let start_center = self.elements[first].x + self.elements[first].width / 2.0;
        let end_center = self.elements[last].x + self.elements[last].width / 2.0;
        let count = indices.len();
        for (slot, &idx) in indices.iter().enumerate().skip(1).take(count - 2) {
            let target =
                start_center + (end_center - start_center) * slot as f32 / (count - 1) as f32;
            let elem = &mut self.elements[idx];
            elem.x = target - elem.width / 2.0;
        }
    }

    /// Distributes selected element centers evenly between the first and last element
    /// (by current center) along the vertical axis. Fewer than three selected
    /// elements is a no-op.
    pub fn distribute_vertically(&mut self, ids: &[&str]) {
        let mut indices: Vec<usize> = self
            .elements
            .iter()
            .enumerate()
            .filter(|(_, e)| ids.contains(&e.id.as_str()))
            .map(|(i, _)| i)
            .collect();
        if indices.len() < 3 {
            return;
        }
        indices.sort_by(|&a, &b| {
            let ca = self.elements[a].y + self.elements[a].height / 2.0;
            let cb = self.elements[b].y + self.elements[b].height / 2.0;
            ca.partial_cmp(&cb).unwrap_or(std::cmp::Ordering::Equal)
        });
        let first = indices[0];
        let last = *indices.last().expect("non-empty checked above");
        let start_center = self.elements[first].y + self.elements[first].height / 2.0;
        let end_center = self.elements[last].y + self.elements[last].height / 2.0;
        let count = indices.len();
        for (slot, &idx) in indices.iter().enumerate().skip(1).take(count - 2) {
            let target =
                start_center + (end_center - start_center) * slot as f32 / (count - 1) as f32;
            let elem = &mut self.elements[idx];
            elem.y = target - elem.height / 2.0;
        }
    }

    /// Arranges the given element ids into a row-major grid of `columns` columns filling
    /// `area` (x, y, width, height) with equal cell sizes and `gutter` spacing between cells.
    /// Each element is resized to fill its cell exactly (gutter lives between cells, not
    /// around the grid). Unknown ids are skipped; extra ids continue past the last row using
    /// the same cell pitch. Returns the number of elements arranged. Zero columns is a no-op
    /// returning 0.
    #[allow(clippy::too_many_arguments)]
    pub fn arrange_grid(
        &mut self,
        ids: &[&str],
        area_x: f64,
        area_y: f64,
        area_w: f64,
        area_h: f64,
        columns: usize,
        gutter: f64,
    ) -> usize {
        if columns == 0 {
            return 0;
        }
        let arranged: Vec<usize> = ids
            .iter()
            .filter_map(|id| self.elements.iter().position(|e| e.id == *id))
            .collect();
        let count = arranged.len();
        if count == 0 {
            return 0;
        }
        let rows = count.div_ceil(columns);
        let cell_w = (area_w - gutter * (columns as f64 - 1.0)) / columns as f64;
        let cell_h = (area_h - gutter * (rows as f64 - 1.0)) / rows as f64;
        for (slot, &idx) in arranged.iter().enumerate() {
            let col = (slot % columns) as f64;
            let row = (slot / columns) as f64;
            let elem = &mut self.elements[idx];
            elem.x = (area_x + col * (cell_w + gutter)) as f32;
            elem.y = (area_y + row * (cell_h + gutter)) as f32;
            elem.width = cell_w as f32;
            elem.height = cell_h as f32;
        }
        count
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
                    action: None,
                });
                self.add_element(SlideElement {
                    id: "sub-1".into(),
                    element_type: ElementType::Subtitle,
                    content: "Subtitle or author name".into(),
                    x: 100.0,
                    y: 400.0,
                    width: 1720.0,
                    height: 80.0,
                    action: None,
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
                    action: None,
                });
                self.add_element(SlideElement {
                    id: "body-1".into(),
                    element_type: ElementType::BodyText,
                    content: "• Key point one\n• Key point two\n• Key point three".into(),
                    x: 80.0,
                    y: 200.0,
                    width: 1760.0,
                    height: 780.0,
                    action: None,
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
                    action: None,
                });
                self.add_element(SlideElement {
                    id: "col-1".into(),
                    element_type: ElementType::BodyText,
                    content: "Column 1 notes and content".into(),
                    x: 80.0,
                    y: 200.0,
                    width: 850.0,
                    height: 780.0,
                    action: None,
                });
                self.add_element(SlideElement {
                    id: "col-2".into(),
                    element_type: ElementType::BodyText,
                    content: "Column 2 notes and content".into(),
                    x: 990.0,
                    y: 200.0,
                    width: 850.0,
                    height: 780.0,
                    action: None,
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
                    action: None,
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
                    action: None,
                });
                self.add_element(SlideElement {
                    id: "label-1".into(),
                    element_type: ElementType::Subtitle,
                    content: "System Reliability".into(),
                    x: 200.0,
                    y: 500.0,
                    width: 1520.0,
                    height: 100.0,
                    action: None,
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

/// Slide master template for uniform slide deck styling and layouts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MasterSlide {
    pub id: String,
    pub name: String,
    pub bg_color: String,
    pub default_title_color: String,
    pub default_body_color: String,
    pub footer_text: String,
}

impl Default for MasterSlide {
    fn default() -> Self {
        Self {
            id: "master-default".into(),
            name: "Standard Master".into(),
            bg_color: "#1e1e2e".into(),
            default_title_color: "#cdd6f4".into(),
            default_body_color: "#a6adc8".into(),
            footer_text: "Loom Presentation".into(),
        }
    }
}

/// Applies a master slide template's styling to a target slide.
pub fn apply_master_to_slide(slide: &mut Slide, master: &MasterSlide) {
    slide.bg_color = master.bg_color.clone();
}

/// Type of scene node within a slide hierarchy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SceneNodeType {
    Shape,
    Text,
    Image,
    Group,
}

/// Hierarchical scene graph node with local transforms and composite bounding boxes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneNode {
    pub id: String,
    pub node_type: SceneNodeType,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub rotation_deg: f32,
    pub children: Vec<SceneNode>,
}

impl SceneNode {
    pub fn new(
        id: impl Into<String>,
        node_type: SceneNodeType,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    ) -> Self {
        Self {
            id: id.into(),
            node_type,
            x,
            y,
            width: w,
            height: h,
            rotation_deg: 0.0,
            children: Vec::new(),
        }
    }

    pub fn add_child(&mut self, child: SceneNode) {
        self.children.push(child);
    }

    /// Computes the accumulated global bounding box (x, y, w, h) in canvas coordinates.
    pub fn calculate_global_bounds(
        &self,
        parent_offset_x: f32,
        parent_offset_y: f32,
    ) -> (f32, f32, f32, f32) {
        let global_x = parent_offset_x + self.x;
        let global_y = parent_offset_y + self.y;

        if self.children.is_empty() {
            return (global_x, global_y, self.width, self.height);
        }

        let mut min_x = global_x;
        let mut min_y = global_y;
        let mut max_x = global_x + self.width;
        let mut max_y = global_y + self.height;

        for child in &self.children {
            let (cx, cy, cw, ch) = child.calculate_global_bounds(global_x, global_y);
            min_x = min_x.min(cx);
            min_y = min_y.min(cy);
            max_x = max_x.max(cx + cw);
            max_y = max_y.max(cy + ch);
        }

        (min_x, min_y, max_x - min_x, max_y - min_y)
    }
}

/// Alignment guide line generated when objects snap to common edges or centers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SnapGuide {
    pub position: f32,
    pub is_vertical: bool,
}

/// Result of smart snapping computation containing adjusted coordinates and active guide lines.
#[derive(Debug, Clone, PartialEq)]
pub struct SnapResult {
    pub snapped_x: f32,
    pub snapped_y: f32,
    pub guides: Vec<SnapGuide>,
}

/// Computes intelligent object alignment snapping against neighboring elements.
pub fn calculate_smart_snapping(
    moving_bounds: (f32, f32, f32, f32),
    reference_bounds: &[(f32, f32, f32, f32)],
    threshold_px: f32,
) -> SnapResult {
    let (mx, my, mw, mh) = moving_bounds;
    let mut snapped_x = mx;
    let mut snapped_y = my;
    let mut guides = Vec::new();

    let m_left = mx;
    let m_center_x = mx + mw / 2.0;
    let m_right = mx + mw;

    let m_top = my;
    let m_center_y = my + mh / 2.0;
    let m_bottom = my + mh;

    let mut x_snapped = false;
    let mut y_snapped = false;

    for &(rx, ry, rw, rh) in reference_bounds {
        let r_left = rx;
        let r_center_x = rx + rw / 2.0;
        let r_right = rx + rw;

        let r_top = ry;
        let r_center_y = ry + rh / 2.0;
        let r_bottom = ry + rh;

        // X snapping (left-to-left, center-to-center, right-to-right)
        if !x_snapped {
            if (m_left - r_left).abs() <= threshold_px {
                snapped_x = r_left;
                guides.push(SnapGuide {
                    position: r_left,
                    is_vertical: true,
                });
                x_snapped = true;
            } else if (m_center_x - r_center_x).abs() <= threshold_px {
                snapped_x = r_center_x - mw / 2.0;
                guides.push(SnapGuide {
                    position: r_center_x,
                    is_vertical: true,
                });
                x_snapped = true;
            } else if (m_right - r_right).abs() <= threshold_px {
                snapped_x = r_right - mw;
                guides.push(SnapGuide {
                    position: r_right,
                    is_vertical: true,
                });
                x_snapped = true;
            }
        }

        // Y snapping (top-to-top, center-to-center, bottom-to-bottom)
        if !y_snapped {
            if (m_top - r_top).abs() <= threshold_px {
                snapped_y = r_top;
                guides.push(SnapGuide {
                    position: r_top,
                    is_vertical: false,
                });
                y_snapped = true;
            } else if (m_center_y - r_center_y).abs() <= threshold_px {
                snapped_y = r_center_y - mh / 2.0;
                guides.push(SnapGuide {
                    position: r_center_y,
                    is_vertical: false,
                });
                y_snapped = true;
            } else if (m_bottom - r_bottom).abs() <= threshold_px {
                snapped_y = r_bottom - mh;
                guides.push(SnapGuide {
                    position: r_bottom,
                    is_vertical: false,
                });
                y_snapped = true;
            }
        }
    }

    SnapResult {
        snapped_x,
        snapped_y,
        guides,
    }
}

/// Live presentation session state tracking slide progression, timers, and display modes.
/// Per-slide rehearsal duration record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SlideTimingRecord {
    pub slide_index: usize,
    pub duration_seconds: f64,
}

/// Summary report of a presentation rehearsal session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RehearsalReport {
    pub total_duration_seconds: f64,
    pub slide_timings: Vec<SlideTimingRecord>,
}

impl RehearsalReport {
    /// Computes the average speaking time per slide in seconds.
    pub fn average_seconds_per_slide(&self) -> f64 {
        if self.slide_timings.is_empty() {
            0.0
        } else {
            self.total_duration_seconds / self.slide_timings.len() as f64
        }
    }

    /// Finds the slide index with the longest presentation duration.
    pub fn longest_slide(&self) -> Option<(usize, f64)> {
        self.slide_timings
            .iter()
            .max_by(|a, b| {
                a.duration_seconds
                    .partial_cmp(&b.duration_seconds)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|rec| (rec.slide_index, rec.duration_seconds))
    }
}

/// Live presenter drawing annotation tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AnnotationDrawingTool {
    #[default]
    Pen,
    Highlighter,
    LaserPointer,
    Eraser,
}

/// A freehand vector stroke drawn on a presentation slide during a live session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnnotationStroke {
    pub tool: AnnotationDrawingTool,
    pub color: String,
    pub width: f32,
    pub points: Vec<(f32, f32)>,
}

/// Collection of annotations drawn over a single slide.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct SlideAnnotationOverlay {
    pub slide_index: usize,
    pub strokes: Vec<AnnotationStroke>,
}

impl SlideAnnotationOverlay {
    pub fn new(slide_index: usize) -> Self {
        Self {
            slide_index,
            strokes: Vec::new(),
        }
    }

    pub fn add_stroke(&mut self, stroke: AnnotationStroke) {
        self.strokes.push(stroke);
    }

    pub fn clear(&mut self) {
        self.strokes.clear();
    }

    pub fn total_points(&self) -> usize {
        self.strokes.iter().map(|s| s.points.len()).sum()
    }
}

/// Live presentation session state tracking slide progression, timers, display modes, and drawing annotations.
#[derive(Debug, Clone, PartialEq)]
pub struct PresenterSession {
    pub current_slide_index: usize,
    pub total_slides: usize,
    pub elapsed_seconds: f64,
    pub current_slide_seconds: f64,
    pub is_paused: bool,
    pub is_blanked: bool,
    pub recorded_timings: Vec<SlideTimingRecord>,
    pub annotations: std::collections::BTreeMap<usize, SlideAnnotationOverlay>,
}

impl PresenterSession {
    pub fn new(total_slides: usize) -> Self {
        Self {
            current_slide_index: 0,
            total_slides: total_slides.max(1),
            elapsed_seconds: 0.0,
            current_slide_seconds: 0.0,
            is_paused: false,
            is_blanked: false,
            recorded_timings: Vec::new(),
            annotations: std::collections::BTreeMap::new(),
        }
    }

    /// Adds an annotation stroke to the current slide.
    pub fn add_annotation_stroke(&mut self, stroke: AnnotationStroke) {
        let entry = self
            .annotations
            .entry(self.current_slide_index)
            .or_insert_with(|| SlideAnnotationOverlay::new(self.current_slide_index));
        entry.add_stroke(stroke);
    }

    /// Clears annotations on the specified slide index.
    pub fn clear_annotations(&mut self, slide_index: usize) {
        if let Some(overlay) = self.annotations.get_mut(&slide_index) {
            overlay.clear();
        }
    }

    /// Records time on current slide and advances to the next slide if not on the last slide.
    pub fn advance_slide(&mut self) -> bool {
        if self.current_slide_index + 1 < self.total_slides {
            self.recorded_timings.push(SlideTimingRecord {
                slide_index: self.current_slide_index,
                duration_seconds: self.current_slide_seconds,
            });
            self.current_slide_index += 1;
            self.current_slide_seconds = 0.0;
            true
        } else {
            false
        }
    }

    /// Returns to the previous slide if not on the first slide.
    pub fn previous_slide(&mut self) -> bool {
        if self.current_slide_index > 0 {
            self.current_slide_index -= 1;
            self.current_slide_seconds = 0.0;
            true
        } else {
            false
        }
    }

    /// Toggles the elapsed presentation timer pause state.
    pub fn toggle_pause(&mut self) {
        self.is_paused = !self.is_paused;
    }

    /// Toggles screen blanking (e.g. audience focus on speaker).
    pub fn toggle_blank(&mut self) {
        self.is_blanked = !self.is_blanked;
    }

    /// Advances the presentation timer.
    pub fn tick(&mut self, delta_seconds: f64) {
        if !self.is_paused && delta_seconds > 0.0 {
            self.elapsed_seconds += delta_seconds;
            self.current_slide_seconds += delta_seconds;
        }
    }

    /// Finalizes the rehearsal and returns a summary report.
    pub fn finish_rehearsal(&mut self) -> RehearsalReport {
        let mut timings = self.recorded_timings.clone();
        timings.push(SlideTimingRecord {
            slide_index: self.current_slide_index,
            duration_seconds: self.current_slide_seconds,
        });

        RehearsalReport {
            total_duration_seconds: self.elapsed_seconds,
            slide_timings: timings,
        }
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
            action: None,
        });
        doc.slides.push(cover);
        doc
    }

    /// Evaluates an interactive action trigger and returns the target slide index if navigation occurs.
    pub fn execute_action(
        &self,
        action: &SlideActionTrigger,
        current_slide_index: usize,
    ) -> Option<usize> {
        if self.slides.is_empty() {
            return None;
        }
        match action {
            SlideActionTrigger::NextSlide => {
                if current_slide_index + 1 < self.slides.len() {
                    Some(current_slide_index + 1)
                } else {
                    None
                }
            }
            SlideActionTrigger::PreviousSlide => {
                if current_slide_index > 0 {
                    Some(current_slide_index - 1)
                } else {
                    None
                }
            }
            SlideActionTrigger::FirstSlide => Some(0),
            SlideActionTrigger::LastSlide => Some(self.slides.len() - 1),
            SlideActionTrigger::JumpToSlide(idx) => {
                if *idx < self.slides.len() {
                    Some(*idx)
                } else {
                    None
                }
            }
            SlideActionTrigger::OpenUrl(_) => None,
        }
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

    /// Searches speaker notes across all slides for a query string, returning (slide_index, slide_title).
    pub fn search_speaker_notes(&self, query: &str, case_sensitive: bool) -> Vec<(usize, String)> {
        let q = if case_sensitive {
            query.to_string()
        } else {
            query.to_lowercase()
        };

        let mut matches = Vec::new();
        for (i, slide) in self.slides.iter().enumerate() {
            let notes = if case_sensitive {
                slide.speaker_notes.clone()
            } else {
                slide.speaker_notes.to_lowercase()
            };

            if notes.contains(&q) {
                matches.push((i, slide.title.clone()));
            }
        }
        matches
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

/// Builds slides from an indented text outline: top-level lines become slide titles,
/// indented child lines (any consistent leading whitespace >= 2 spaces or one tab) become
/// bullet content elements on that slide. Lines before the first top-level entry are ignored.
///
/// Deterministic identifier scheme:
/// - slide ids: `outline-slide-N` where N is the 1-based slide position in the returned vector;
/// - each slide gets exactly one [`ElementType::Title`] element with id `title-N`;
/// - each child line becomes one [`ElementType::BodyText`] element with id
///   `bullet-N-K` where K is the 1-based bullet index within that slide.
///
/// Child lines are stripped of their leading whitespace and of a single leading `- `
/// bullet marker. Blank lines are skipped. Returns Err when no top-level entries exist.
pub fn deck_from_outline(outline: &str) -> Result<Vec<Slide>, String> {
    let mut slides: Vec<Slide> = Vec::new();
    for raw_line in outline.lines() {
        let line = raw_line.trim_end_matches('\r');
        if line.trim().is_empty() {
            continue;
        }
        let is_child = line.starts_with("  ") || line.starts_with('\t');
        let slide_number = slides.len();
        if is_child {
            if let Some(slide) = slides.last_mut() {
                let text = line.trim_start();
                let text = text.strip_prefix("- ").unwrap_or(text);
                let bullet_number = slide.elements.len();
                slide.add_element(SlideElement {
                    id: format!("bullet-{}-{}", slide_number, bullet_number),
                    element_type: ElementType::BodyText,
                    content: text.to_string(),
                    x: 80.0,
                    y: 200.0,
                    width: 1760.0,
                    height: 780.0,
                    action: None,
                });
            }
        } else {
            let title_text = line.trim();
            let index = slides.len() + 1;
            let mut slide = Slide::new(format!("outline-slide-{}", index), title_text, "outline");
            slide.add_element(SlideElement {
                id: format!("title-{}", index),
                element_type: ElementType::Title,
                content: title_text.to_string(),
                x: 80.0,
                y: 60.0,
                width: 1760.0,
                height: 100.0,
                action: None,
            });
            slides.push(slide);
        }
    }
    if slides.is_empty() {
        return Err("outline contains no top-level entries".to_string());
    }
    Ok(slides)
}

/// Serializes slides into an indented text outline, inverting [`deck_from_outline`]: each
/// slide's title element content becomes one top-level line and each body-text element
/// becomes a two-space-indented child line. Returns Err when the deck has no slides. Output
/// lines end with \n including the final line.
pub fn deck_to_text_outline(slides: &[Slide]) -> Result<String, String> {
    if slides.is_empty() {
        return Err("deck contains no slides".to_string());
    }
    let mut outline = String::new();
    for slide in slides {
        // Title: prefer the dedicated title element; fall back to the slide title field.
        let title = slide
            .elements
            .iter()
            .find(|e| e.element_type == ElementType::Title)
            .map(|e| e.content.clone())
            .unwrap_or_else(|| slide.title.clone());
        outline.push_str(title.trim_end());
        outline.push('\n');
        for element in &slide.elements {
            if element.element_type == ElementType::BodyText {
                outline.push_str("  ");
                outline.push_str(element.content.trim_end());
                outline.push('\n');
            }
        }
    }
    Ok(outline)
}

/// Archive prefix under which PPTX slide parts live.
const PPTX_SLIDE_PART_PREFIX: &str = "ppt/slides/slide";

/// Extracts slide titles from a .pptx archive in slide order. Discovers slide parts among
/// archive paths matching `ppt/slides/slide<N>.xml`, sorts N numerically, then per slide
/// extracts the FIRST `<a:p>...</a:p>` paragraph's concatenated `<a:t>` runs as the title
/// (documented heuristic: first paragraph = title line for extraction purposes). Slides
/// whose first paragraph has no text yield an empty-string title. The five predefined XML
/// entities are unescaped in run text. Returns Err on unreadable archives or when no slide
/// parts exist.
///
/// This is a targeted byte scan, not a validating XML parser: malformed or unusual slide
/// markup degrades to an empty title rather than failing the whole import.
/// Exports slide titles into a minimal valid `.pptx` archive: one slide part per title,
/// each carrying a single text paragraph. Round-trips through [`extract_pptx_titles`]
/// in order; empty titles are preserved as slides with empty first paragraphs.
pub fn export_pptx_from_titles(titles: &[String]) -> Result<Vec<u8>, String> {
    let mut content_overrides = String::new();
    let mut presentation_refs = String::new();
    let mut presentation_rels = String::new();
    let mut slide_parts: Vec<(String, Vec<u8>)> = Vec::new();

    for (index, title) in titles.iter().enumerate() {
        let number = index + 1;
        content_overrides.push_str(&format!(
            "<Override PartName=\"/ppt/slides/slide{number}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slide+xml\"/>"
        ));
        presentation_refs.push_str(&format!(
            "<p:sldId id=\"{number}\" r:id=\"rIdSlide{number}\"/>"
        ));
        presentation_rels.push_str(&format!(
            "<Relationship Id=\"rIdSlide{number}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide\" Target=\"slides/slide{number}.xml\"/>"
        ));

        let escaped = xml_escape_pptx(title);
        let slide_xml = format!(
            "<?xml version=\"1.0\"?><p:sld xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"><p:cSld><p:spTree><p:sp><p:txBody><a:p><a:r><a:t>{escaped}</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:sld>"
        );
        slide_parts.push((
            format!("ppt/slides/slide{number}.xml"),
            slide_xml.into_bytes(),
        ));
    }

    let mut parts: Vec<(String, Vec<u8>)> = vec![
        (
            "[Content_Types].xml".to_string(),
            format!(
                "<?xml version=\"1.0\"?><Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/ppt/presentation.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml\"/>{content_overrides}</Types>"
            )
            .into_bytes(),
        ),
        (
            "_rels/.rels".to_string(),
            "<?xml version=\"1.0\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"ppt/presentation.xml\"/></Relationships>".to_string().into_bytes(),
        ),
        (
            "ppt/presentation.xml".to_string(),
            format!(
                "<?xml version=\"1.0\"?><p:presentation xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"><p:sldIdLst>{presentation_refs}</p:sldIdLst></p:presentation>"
            )
            .into_bytes(),
        ),
        (
            "ppt/_rels/presentation.xml.rels".to_string(),
            format!(
                "<?xml version=\"1.0\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">{presentation_rels}</Relationships>"
            )
            .into_bytes(),
        ),
    ];
    parts.extend(slide_parts);

    let mut archive = PackageArchive::new();
    for (path, data) in &parts {
        archive
            .add(path, data.clone())
            .map_err(|e| format!("pptx export failed: {e}"))?;
    }
    archive
        .to_bytes()
        .map_err(|e| format!("pptx export failed: {e}"))
}

/// Escapes XML attribute/text characters for presentation parts.
fn xml_escape_pptx(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

pub fn extract_pptx_titles(pptx_bytes: &[u8]) -> Result<Vec<String>, String> {
    let archive = PackageArchive::from_bytes(pptx_bytes)
        .map_err(|err| format!("unreadable pptx archive: {err}"))?;
    let mut slide_parts: Vec<(u64, &str)> = Vec::new();
    for path in archive.paths() {
        let Some(number) = path
            .strip_prefix(PPTX_SLIDE_PART_PREFIX)
            .and_then(|rest| rest.strip_suffix(".xml"))
        else {
            continue;
        };
        if number.is_empty() || !number.bytes().all(|byte| byte.is_ascii_digit()) {
            continue;
        }
        if let Ok(parsed) = number.parse::<u64>() {
            slide_parts.push((parsed, path));
        }
    }
    if slide_parts.is_empty() {
        return Err(
            "pptx archive contains no slide parts (expected ppt/slides/slide<N>.xml)".to_string(),
        );
    }
    slide_parts.sort_unstable_by_key(|(number, _)| *number);
    slide_parts
        .iter()
        .map(|(_, path)| {
            let data = archive
                .get(path)
                .ok_or_else(|| format!("missing slide part {path}"))?;
            Ok(pptx_first_paragraph_title(&String::from_utf8_lossy(data)))
        })
        .collect()
}

/// Convenience building a full outline-importable deck skeleton from a .pptx archive: one
/// [`Slide`] per extracted title (see [`extract_pptx_titles`]) with ids `pptx-slide-N`
/// (N is the 1-based slide position) and layout `imported-pptx`. Error conditions are
/// inherited from [`extract_pptx_titles`].
pub fn slides_from_pptx(pptx_bytes: &[u8]) -> Result<Vec<Slide>, String> {
    Ok(extract_pptx_titles(pptx_bytes)?
        .into_iter()
        .enumerate()
        .map(|(index, title)| {
            Slide::new(format!("pptx-slide-{}", index + 1), title, "imported-pptx")
        })
        .collect())
}

/// Extracts the concatenated `<a:t>` run text of the FIRST `<a:p>` paragraph in a slide
/// part, or an empty string when the slide has no usable first paragraph.
fn pptx_first_paragraph_title(slide_xml: &str) -> String {
    let Some((open, close)) = next_xml_element_inner(slide_xml, 0, "a:p") else {
        return String::new();
    };
    let paragraph = &slide_xml[open..close];
    let mut title = String::new();
    let mut cursor = 0usize;
    while let Some((run_open, run_close)) = next_xml_element_inner(paragraph, cursor, "a:t") {
        title.push_str(&unescape_xml_entities(&paragraph[run_open..run_close]));
        cursor = run_close;
    }
    title
}

/// Scans `xml` from byte offset `from` for the inner span of the next `<tag ...>...</tag>`
/// element, returning `(inner_start, inner_end)`. Handles attribute-bearing open tags by
/// skipping ahead to the open tag's closing `>`; a self-closing `<tag/>` yields an empty
/// span so callers observe that the element existed without content. Longer tags that share
/// the prefix (such as `<a:pPr>` for tag `a:p`) are skipped. Returns None when no further
/// occurrence exists or an open tag is never closed.
fn next_xml_element_inner(xml: &str, from: usize, tag: &str) -> Option<(usize, usize)> {
    let open_needle = format!("<{tag}");
    let close_needle = format!("</{tag}>");
    let bytes = xml.as_bytes();
    let mut pos = from;
    while let Some(rel) = xml[pos..].find(&open_needle) {
        let after_name = pos + rel + open_needle.len();
        match bytes.get(after_name) {
            Some(b'>') | Some(b' ') | Some(b'\t') | Some(b'\r') | Some(b'\n') => {
                let inner_start =
                    after_name + bytes[after_name..].iter().position(|&b| b == b'>')? + 1;
                let close_rel = xml[inner_start..].find(&close_needle)?;
                return Some((inner_start, inner_start + close_rel));
            }
            Some(b'/') if bytes.get(after_name + 1) == Some(&b'>') => {
                return Some((after_name + 2, after_name + 2));
            }
            _ => pos = after_name,
        }
    }
    None
}

/// Unescapes the five predefined XML entities (`&amp;`, `&lt;`, `&gt;`, `&quot;`,
/// `&apos;`) in a single left-to-right pass, so escaped text such as `&amp;lt;` decodes to
/// `&lt;` rather than being double-decoded to `<`. Unknown or malformed entities pass
/// through unchanged.
fn unescape_xml_entities(text: &str) -> String {
    const ENTITIES: [(&str, char); 5] = [
        ("&amp;", '&'),
        ("&lt;", '<'),
        ("&gt;", '>'),
        ("&quot;", '"'),
        ("&apos;", '\''),
    ];
    if !text.contains('&') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut chars = text.char_indices();
    while let Some((index, ch)) = chars.next() {
        if ch == '&' {
            let rest = &text[index..];
            if let Some((entity, decoded)) =
                ENTITIES.iter().find(|(name, _)| rest.starts_with(*name))
            {
                out.push(*decoded);
                for _ in 1..entity.chars().count() {
                    chars.next();
                }
                continue;
            }
        }
        out.push(ch);
    }
    out
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

/// Entrance animation effect presets for slide elements.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AnimationEffect {
    #[default]
    Appear,
    FadeIn,
    FlyFromLeft,
    FlyFromRight,
    ZoomIn,
}

/// One object animation entry bound to a slide element.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnimationEntry {
    pub element_id: String,
    pub effect: AnimationEffect,
    /// Zero-based position in the click-build sequence.
    pub build_order: usize,
    pub duration_ms: u32,
    pub delay_ms: u32,
}

/// Sorts animation entries into their build order (stable).
pub fn sort_animation_builds(entries: &mut [AnimationEntry]) {
    entries.sort_by_key(|entry| entry.build_order);
}

/// Computes normalized eased progress 0.0..=1.0 of an entry at elapsed ms (after delay), using
/// smoothstep easing (3t^2 - 2t^3) clamped to [0,1]. Before delay elapses => 0.0; past end => 1.0.
pub fn animation_progress(entry: &AnimationEntry, elapsed_ms: u32) -> f64 {
    if elapsed_ms < entry.delay_ms {
        return 0.0;
    }
    if entry.duration_ms == 0 || elapsed_ms - entry.delay_ms >= entry.duration_ms {
        return 1.0;
    }
    let t = f64::from(elapsed_ms - entry.delay_ms) / f64::from(entry.duration_ms);
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Accessibility reading-order assignment for one slide's elements.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReadingOrder {
    pub slide_index: usize,
    /// Element ids in intended reading sequence (screen readers announce in this order).
    pub ordered_element_ids: Vec<String>,
}

impl ReadingOrder {
    pub fn new(slide_index: usize) -> Self {
        Self {
            slide_index,
            ordered_element_ids: Vec::new(),
        }
    }

    /// Moves an element id to position `to`; no-op if absent. Returns true when order changed.
    pub fn move_element(&mut self, element_id: &str, to: usize) -> bool {
        let Some(from) = self
            .ordered_element_ids
            .iter()
            .position(|id| id == element_id)
        else {
            return false;
        };
        if from == to || to >= self.ordered_element_ids.len() {
            return false;
        }
        let id = self.ordered_element_ids.remove(from);
        self.ordered_element_ids.insert(to, id);
        true
    }

    /// Appends ids from `element_ids` that are not already present (dedup).
    pub fn append_missing(&mut self, element_ids: &[String]) {
        for element_id in element_ids {
            if !self.ordered_element_ids.contains(element_id) {
                self.ordered_element_ids.push(element_id.clone());
            }
        }
    }

    /// True when every id occurs exactly once and none is empty.
    pub fn is_valid(&self) -> bool {
        let mut seen = std::collections::HashSet::with_capacity(self.ordered_element_ids.len());
        self.ordered_element_ids
            .iter()
            .all(|id| !id.is_empty() && seen.insert(id.as_str()))
    }
}

/// Computes the largest x/y/w/h rectangle fitting entirely inside the target box while
/// preserving the source aspect ratio, centered within it. Returns (x, y, width, height) f64s.
/// Degenerate source (zero/negative w or h) or target => Err naming the problem.
pub fn fit_rect_into_box(
    src_w: f64,
    src_h: f64,
    box_x: f64,
    box_y: f64,
    box_w: f64,
    box_h: f64,
) -> Result<(f64, f64, f64, f64), String> {
    if src_w <= 0.0 {
        return Err("source width must be positive".into());
    }
    if src_h <= 0.0 {
        return Err("source height must be positive".into());
    }
    if box_w <= 0.0 {
        return Err("box width must be positive".into());
    }
    if box_h <= 0.0 {
        return Err("box height must be positive".into());
    }
    let scale = (box_w / src_w).min(box_h / src_h);
    let width = src_w * scale;
    let height = src_h * scale;
    let x = box_x + (box_w - width) / 2.0;
    let y = box_y + (box_h - height) / 2.0;
    Ok((x, y, width, height))
}

/// Applies fit_rect_into_box to a slide element's geometry, mutating its x/y/width/height.
pub fn fit_element_to_box(
    element: &mut SlideElement,
    box_x: f64,
    box_y: f64,
    box_w: f64,
    box_h: f64,
) -> Result<(), String> {
    let (x, y, width, height) = fit_rect_into_box(
        f64::from(element.width),
        f64::from(element.height),
        box_x,
        box_y,
        box_w,
        box_h,
    )?;
    element.x = x as f32;
    element.y = y as f32;
    element.width = width as f32;
    element.height = height as f32;
    Ok(())
}

/// Lifecycle state of an externally linked asset placed on a slide.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum LinkedAssetState {
    /// Path resolves and content hash matches.
    #[default]
    Linked,
    /// Path no longer resolves on disk.
    Missing,
    /// Path resolves but the content hash changed since placement.
    Modified,
    /// User replaced the file and confirmed the new link target.
    Relinked,
}

/// An external asset referenced by a slide deck.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LinkedAsset {
    pub asset_id: String,
    /// Absolute or relative path at link time.
    pub path: String,
    /// FNV-1a style 64-bit content hash captured when linked.
    pub content_hash: u64,
    pub state: LinkedAssetState,
}

impl LinkedAsset {
    pub fn new(asset_id: impl Into<String>, path: impl Into<String>, content_hash: u64) -> Self {
        Self {
            asset_id: asset_id.into(),
            path: path.into(),
            content_hash,
            state: LinkedAssetState::Linked,
        }
    }

    /// Recomputes state against disk reality using injected predicates (`path_exists`,
    /// `hash_for_path`). A missing path yields [`LinkedAssetState::Missing`]; a hash
    /// mismatch yields [`LinkedAssetState::Modified`]; a match yields
    /// [`LinkedAssetState::Linked`]. Never sets [`LinkedAssetState::Relinked`] — that
    /// is user-confirmed via [`LinkedAsset::relink`].
    pub fn refresh_state<F: Fn(&str) -> bool, G: Fn(&str) -> u64>(
        &mut self,
        path_exists: F,
        hash_for_path: G,
    ) {
        if !path_exists(&self.path) {
            self.state = LinkedAssetState::Missing;
        } else if hash_for_path(&self.path) != self.content_hash {
            self.state = LinkedAssetState::Modified;
        } else {
            self.state = LinkedAssetState::Linked;
        }
    }

    /// Points the asset at a replacement path and captures its new hash, marking
    /// the asset [`LinkedAssetState::Relinked`].
    pub fn relink<G: Fn(&str) -> u64>(&mut self, new_path: &str, hash_for_path: G) {
        self.path = new_path.to_string();
        self.content_hash = hash_for_path(new_path);
        self.state = LinkedAssetState::Relinked;
    }

    /// FNV-1a 64-bit over bytes — public so callers and tests share one implementation.
    pub fn hash_bytes(bytes: &[u8]) -> u64 {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash
    }
}

/// One accessibility finding for a slide element.
#[derive(Debug, Clone, PartialEq)]
pub struct AccessibilityFinding {
    pub slide_index: usize,
    pub element_id: String,
    /// "empty-content" when a visual element carries no textual content.
    pub issue: String,
}

/// Audits a deck for accessibility basics: visual elements ([`ElementType::ShapeRectangle`],
/// [`ElementType::ShapeCircle`]) whose content carries no textual content produce
/// "empty-content" findings; slides whose reading order was never assigned are not flagged
/// here. Deterministic order: slide order then element order.
pub fn audit_accessibility(slides: &[Slide]) -> Vec<AccessibilityFinding> {
    let mut findings = Vec::new();
    for (slide_index, slide) in slides.iter().enumerate() {
        for element in &slide.elements {
            let is_visual = matches!(
                element.element_type,
                ElementType::ShapeRectangle | ElementType::ShapeCircle
            );
            if is_visual && element.content.trim().is_empty() {
                findings.push(AccessibilityFinding {
                    slide_index,
                    element_id: element.id.clone(),
                    issue: "empty-content".to_string(),
                });
            }
        }
    }
    findings
}

/// Coverage summary over an audit result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AccessibilitySummary {
    pub total_elements: usize,
    pub flagged_elements: usize,
}

impl AccessibilitySummary {
    /// Fraction of clean elements in [0,1]; empty decks yield 1.0.
    pub fn clean_fraction(&self) -> f64 {
        if self.total_elements == 0 {
            return 1.0;
        }
        (self.total_elements - self.flagged_elements) as f64 / self.total_elements as f64
    }
}

/// Builds a coverage summary from a deck's total element count and its audit findings.
pub fn summarize_findings(
    total_elements: usize,
    findings: &[AccessibilityFinding],
) -> AccessibilitySummary {
    AccessibilitySummary {
        total_elements,
        flagged_elements: findings.len(),
    }
}

/// FNV-1a 64-bit hash over bytes.
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Fixed string marker for an [`ElementType`] variant so variant identity can feed a digest
/// (`std::mem::discriminant` values are not directly hashable).
fn element_type_marker(element_type: &ElementType) -> &'static str {
    match element_type {
        ElementType::Title => "title",
        ElementType::Subtitle => "subtitle",
        ElementType::BodyText => "body-text",
        ElementType::ShapeRectangle => "shape-rectangle",
        ElementType::ShapeCircle => "shape-circle",
        ElementType::StatCard => "stat-card",
    }
}

impl PresentationDocument {
    /// Stable integrity digest over deck content, for save/reload verification.
    ///
    /// Feeds `"slides:<count>"`, then per slide its id/title/layout and speaker notes, then
    /// every element's id/type/content/geometry in document order. Speaker notes participate
    /// by choice: they are user-authored content that must survive save/reload intact.
    /// Background colour, action triggers, author/theme metadata, and `active_index`
    /// deliberately do not participate so view-state edits do not invalidate comparisons.
    pub fn integrity_digest(&self) -> u64 {
        let mut feed = format!("slides:{}\n", self.slides.len());
        for slide in &self.slides {
            let Slide {
                id,
                title,
                layout,
                elements,
                speaker_notes,
                ..
            } = slide;
            feed.push_str(&format!("slide:{id}:{title}:{layout}\n"));
            feed.push_str(&format!("notes:{speaker_notes}\n"));
            for elem in elements {
                let SlideElement {
                    id,
                    element_type,
                    content,
                    x,
                    y,
                    width,
                    height,
                    ..
                } = elem;
                let marker = element_type_marker(element_type);
                feed.push_str(&format!(
                    "el:{id}:{marker}:{content}:{x},{y},{width},{height}\n"
                ));
            }
        }
        fnv1a64(feed.as_bytes())
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
            action: None,
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
            action: None,
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
            action: None,
        });
        slide.add_element(SlideElement {
            id: "e2".into(),
            element_type: ElementType::BodyText,
            content: "Second".into(),
            x: 150.0,
            y: 200.0,
            width: 200.0,
            height: 50.0,
            action: None,
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
    fn element_distribution_spacing() {
        let mut slide = Slide::new("slide-dist", "Distribution", "blank");
        let make = |id: &str, x: f32, y: f32| {
            let (width, height) = match id {
                "e2" => (150.0, 150.0),
                "e4" => (200.0, 200.0),
                _ => (100.0, 100.0),
            };
            SlideElement {
                id: id.into(),
                element_type: ElementType::ShapeRectangle,
                content: "Box".into(),
                x,
                y,
                width,
                height,
                action: None,
            }
        };
        // Centers: e1=(50,50) e2=(525,425) e3=(950,750) e4=(1850,1850)
        slide.add_element(make("e1", 0.0, 0.0));
        slide.add_element(make("e2", 450.0, 350.0));
        slide.add_element(make("e3", 900.0, 700.0));
        slide.add_element(make("e4", 1750.0, 1750.0));

        slide.distribute_horizontally(&["e1", "e2", "e3", "e4"]);
        // First (50) and last (1850) centers fixed; intermediates at 650 and 1250.
        assert_eq!(slide.elements[0].x, 0.0);
        assert_eq!(slide.elements[1].x, 650.0 - 75.0);
        assert_eq!(slide.elements[2].x, 1250.0 - 50.0);
        assert_eq!(slide.elements[3].x, 1750.0);

        slide.distribute_vertically(&["e1", "e2", "e3", "e4"]);
        // First (50) and last (1850) centers fixed; intermediates at 650 and 1250.
        assert_eq!(slide.elements[0].y, 0.0);
        assert_eq!(slide.elements[1].y, 650.0 - 75.0);
        assert_eq!(slide.elements[2].y, 1250.0 - 50.0);
        assert_eq!(slide.elements[3].y, 1750.0);

        // Two-element distribution is a no-op.
        let mut pair = Slide::new("slide-pair", "Pair", "blank");
        pair.add_element(make("e1", 10.0, 20.0));
        pair.add_element(make("e3", 300.0, 400.0));
        pair.distribute_horizontally(&["e1", "e3"]);
        pair.distribute_vertically(&["e1", "e3"]);
        assert_eq!(pair.elements[0].x, 10.0);
        assert_eq!(pair.elements[0].y, 20.0);
        assert_eq!(pair.elements[1].x, 300.0);
        assert_eq!(pair.elements[1].y, 400.0);

        // Single-element distribution is a no-op.
        let mut single = Slide::new("slide-single", "Single", "blank");
        single.add_element(make("e1", 42.0, 24.0));
        single.distribute_horizontally(&["e1"]);
        single.distribute_vertically(&["e1"]);
        assert_eq!(single.elements[0].x, 42.0);
        assert_eq!(single.elements[0].y, 24.0);
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
            action: None,
        });
        slide.add_element(SlideElement {
            id: "e2".into(),
            element_type: ElementType::ShapeRectangle,
            content: "Box 2".into(),
            x: 250.0,
            y: 150.0,
            width: 150.0,
            height: 100.0,
            action: None,
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
    fn deck_outline_import_structure() {
        let outline = "
  orphan bullet before the first top-level entry

Roadmap Review
  - Q1 milestones
\tTeam updates
Hiring Plan
";
        let slides = deck_from_outline(outline).expect("outline should parse");
        assert_eq!(slides.len(), 2);

        // Slide 1: title element plus two bullets with deterministic ids.
        assert_eq!(slides[0].id, "outline-slide-1");
        assert_eq!(slides[0].title, "Roadmap Review");
        assert_eq!(slides[0].elements[0].id, "title-1");
        assert_eq!(slides[0].elements[0].element_type, ElementType::Title);
        assert_eq!(slides[0].elements[0].content, "Roadmap Review");
        assert_eq!(slides[0].elements[1].id, "bullet-1-1");
        assert_eq!(slides[0].elements[1].element_type, ElementType::BodyText);
        assert_eq!(slides[0].elements[1].content, "Q1 milestones");
        assert_eq!(slides[0].elements[2].id, "bullet-1-2");
        assert_eq!(slides[0].elements[2].content, "Team updates");
        assert_eq!(slides[0].elements.len(), 3);

        // Slide 2: zero children, so only its title element exists.
        assert_eq!(slides[1].id, "outline-slide-2");
        assert_eq!(slides[1].title, "Hiring Plan");
        assert_eq!(slides[1].elements.len(), 1);
        assert_eq!(slides[1].elements[0].id, "title-2");
        assert_eq!(slides[1].elements[0].element_type, ElementType::Title);
        assert_eq!(slides[1].elements[0].content, "Hiring Plan");

        // All-indented input has no top-level entries and must error.
        let err = deck_from_outline("  - orphan\n\tmore\n").unwrap_err();
        assert!(!err.is_empty());
        assert!(deck_from_outline("").is_err());
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

    #[test]
    fn master_slide_application() {
        let master = MasterSlide::default();
        assert_eq!(master.bg_color, "#1e1e2e");

        let mut slide = Slide::new("s1", "My Slide", "TitleAndContent");
        assert_eq!(slide.bg_color, "#ffffff");

        apply_master_to_slide(&mut slide, &master);
        assert_eq!(slide.bg_color, "#1e1e2e");
    }

    #[test]
    fn scene_node_hierarchical_bounds() {
        let mut root_group = SceneNode::new("g1", SceneNodeType::Group, 10.0, 10.0, 0.0, 0.0);
        let child1 = SceneNode::new("c1", SceneNodeType::Shape, 5.0, 5.0, 50.0, 50.0);
        let child2 = SceneNode::new("c2", SceneNodeType::Text, 20.0, 30.0, 100.0, 40.0);

        root_group.add_child(child1);
        root_group.add_child(child2);

        // Global bounds:
        // child1: x = 10+5=15, y = 10+5=15, right = 15+50=65, bottom = 15+50=65
        // child2: x = 10+20=30, y = 10+30=40, right = 30+100=130, bottom = 40+40=80
        // Combined min_x = 10 (root), max_x = 130 -> width = 120, min_y = 10, max_y = 80 -> height = 70
        let (gx, gy, gw, gh) = root_group.calculate_global_bounds(0.0, 0.0);
        assert_eq!(gx, 10.0);
        assert_eq!(gy, 10.0);
        assert_eq!(gw, 120.0);
        assert_eq!(gh, 70.0);
    }

    #[test]
    fn smart_snapping_to_reference_bounds() {
        let reference = vec![(100.0, 100.0, 200.0, 200.0)];
        // Moving element at (98.0, 100.0) -> left edge 98 is within 5px of reference left edge 100
        let moving = (98.0, 100.0, 50.0, 50.0);
        let res = calculate_smart_snapping(moving, &reference, 5.0);

        assert_eq!(res.snapped_x, 100.0);
        assert_eq!(res.snapped_y, 100.0);
        assert_eq!(res.guides.len(), 2);
    }

    #[test]
    fn presenter_session_navigation_and_timer() {
        let mut session = PresenterSession::new(5);
        assert_eq!(session.current_slide_index, 0);
        assert_eq!(session.total_slides, 5);

        assert!(session.advance_slide());
        assert_eq!(session.current_slide_index, 1);

        session.tick(1.5);
        assert_eq!(session.elapsed_seconds, 1.5);

        session.toggle_pause();
        session.tick(2.0);
        assert_eq!(session.elapsed_seconds, 1.5); // paused

        session.toggle_blank();
        assert!(session.is_blanked);
    }

    #[test]
    fn speaker_notes_search_query() {
        let mut doc = PresentationDocument::new("deck-1", "Q3 Review");
        doc.slides[0].speaker_notes = "Welcome investors and stakeholders.".into();

        let mut slide2 = Slide::new("s2", "Financials", "TitleAndContent");
        slide2.speaker_notes = "Revenue grew by 25% year-over-year.".into();
        doc.slides.push(slide2);

        let matches = doc.search_speaker_notes("investors", false);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].0, 0); // Slide 1 (index 0)

        let rev_matches = doc.search_speaker_notes("revenue", false);
        assert_eq!(rev_matches.len(), 1);
        assert_eq!(rev_matches[0].0, 1); // Slide 2 (index 1)
    }

    #[test]
    fn rehearsal_timing_and_report() {
        let mut session = PresenterSession::new(3);

        // Slide 0 for 10 seconds
        session.tick(10.0);
        assert!(session.advance_slide());

        // Slide 1 for 25 seconds
        session.tick(25.0);
        assert!(session.advance_slide());

        // Slide 2 for 15 seconds
        session.tick(15.0);

        let report = session.finish_rehearsal();
        assert_eq!(report.total_duration_seconds, 50.0);
        assert_eq!(report.slide_timings.len(), 3);
        assert!((report.average_seconds_per_slide() - (50.0 / 3.0)).abs() < 1e-4);

        let longest = report.longest_slide().unwrap();
        assert_eq!(longest, (1, 25.0)); // Slide 1 was longest
    }

    #[test]
    fn slide_action_trigger_execution() {
        let mut doc = PresentationDocument::new("deck-actions", "Interactive Deck");
        doc.add_slide("Slide 2", "blank");
        doc.add_slide("Slide 3", "blank");

        // Next slide from 0 -> 1
        assert_eq!(
            doc.execute_action(&SlideActionTrigger::NextSlide, 0),
            Some(1)
        );
        // Next slide from 2 (last) -> None
        assert_eq!(doc.execute_action(&SlideActionTrigger::NextSlide, 2), None);

        // Previous slide from 1 -> 0
        assert_eq!(
            doc.execute_action(&SlideActionTrigger::PreviousSlide, 1),
            Some(0)
        );
        // Previous slide from 0 (first) -> None
        assert_eq!(
            doc.execute_action(&SlideActionTrigger::PreviousSlide, 0),
            None
        );

        // Jump to slide 2
        assert_eq!(
            doc.execute_action(&SlideActionTrigger::JumpToSlide(2), 0),
            Some(2)
        );
        // Invalid jump out of bounds -> None
        assert_eq!(
            doc.execute_action(&SlideActionTrigger::JumpToSlide(10), 0),
            None
        );

        // First / Last slide
        assert_eq!(
            doc.execute_action(&SlideActionTrigger::FirstSlide, 2),
            Some(0)
        );
        assert_eq!(
            doc.execute_action(&SlideActionTrigger::LastSlide, 0),
            Some(2)
        );
    }

    #[test]
    fn presenter_drawing_annotations() {
        let mut session = PresenterSession::new(3);
        assert_eq!(session.annotations.len(), 0);

        let stroke1 = AnnotationStroke {
            tool: AnnotationDrawingTool::Pen,
            color: "#ff0000".into(),
            width: 3.0,
            points: vec![(0.1, 0.1), (0.2, 0.2), (0.3, 0.3)],
        };
        session.add_annotation_stroke(stroke1);

        assert_eq!(session.annotations.len(), 1);
        let overlay = session.annotations.get(&0).unwrap();
        assert_eq!(overlay.strokes.len(), 1);
        assert_eq!(overlay.total_points(), 3);

        // Advance to slide 1 and draw highlighter stroke
        assert!(session.advance_slide());
        let stroke2 = AnnotationStroke {
            tool: AnnotationDrawingTool::Highlighter,
            color: "#ffff00".into(),
            width: 12.0,
            points: vec![(0.5, 0.5), (0.6, 0.5)],
        };
        session.add_annotation_stroke(stroke2);

        assert_eq!(session.annotations.len(), 2);

        // Clear slide 0 annotations
        session.clear_annotations(0);
        assert_eq!(session.annotations.get(&0).unwrap().strokes.len(), 0);
        // Slide 1 remains untouched
        assert_eq!(session.annotations.get(&1).unwrap().strokes.len(), 1);
    }

    #[test]
    fn animation_build_order_and_progress() {
        let mut entries = vec![
            AnimationEntry {
                element_id: "title".into(),
                effect: AnimationEffect::FadeIn,
                build_order: 2,
                duration_ms: 400,
                delay_ms: 100,
            },
            AnimationEntry {
                element_id: "chart".into(),
                effect: AnimationEffect::ZoomIn,
                build_order: 1,
                duration_ms: 200,
                delay_ms: 0,
            },
            AnimationEntry {
                element_id: "subtitle".into(),
                effect: AnimationEffect::FlyFromLeft,
                build_order: 1,
                duration_ms: 300,
                delay_ms: 50,
            },
            AnimationEntry {
                element_id: "stat".into(),
                effect: AnimationEffect::Appear,
                build_order: 0,
                duration_ms: 500,
                delay_ms: 250,
            },
        ];
        entries.reverse();
        sort_animation_builds(&mut entries);
        assert_eq!(
            entries.iter().map(|e| e.build_order).collect::<Vec<_>>(),
            vec![0, 1, 1, 2]
        );
        // Stable: equal build_order keeps original relative order.
        assert_eq!(entries[1].element_id, "subtitle");
        assert_eq!(entries[2].element_id, "chart");

        // Progress == 0 during the delay window (inclusive of its end).
        let fade = AnimationEntry {
            element_id: "fade-item".into(),
            effect: AnimationEffect::FadeIn,
            build_order: 0,
            duration_ms: 400,
            delay_ms: 100,
        };
        assert_eq!(animation_progress(&fade, 0), 0.0);
        assert_eq!(animation_progress(&fade, 99), 0.0);
        assert_eq!(animation_progress(&fade, 100), 0.0);

        // Mid-progress is monotonic between two sample points and strictly inside (0, 1).
        let early = animation_progress(&fade, 200);
        let late = animation_progress(&fade, 300);
        assert!(early > 0.0 && early < late && late < 1.0);

        // Past completion => exactly 1.0.
        assert_eq!(animation_progress(&fade, 500), 1.0);
        assert_eq!(animation_progress(&fade, 10_000), 1.0);

        // Duration 0 means instantly complete once the delay passes.
        let instant = AnimationEntry {
            element_id: "instant".into(),
            effect: AnimationEffect::Appear,
            build_order: 0,
            duration_ms: 0,
            delay_ms: 50,
        };
        assert_eq!(animation_progress(&instant, 49), 0.0);
        assert_eq!(animation_progress(&instant, 50), 1.0);
        assert_eq!(animation_progress(&instant, 51), 1.0);
    }

    #[test]
    fn reading_order_manipulation_and_validation() {
        let mut order = ReadingOrder::new(2);
        assert_eq!(order.slide_index, 2);
        assert!(order.is_valid());

        order.append_missing(&[
            "title-1".to_string(),
            "body-1".to_string(),
            "chart-1".to_string(),
        ]);
        assert_eq!(
            order.ordered_element_ids,
            vec!["title-1", "body-1", "chart-1"]
        );

        // append_missing skips ids already present and appends only new ones.
        order.append_missing(&["body-1".to_string(), "stat-1".to_string()]);
        assert_eq!(
            order.ordered_element_ids,
            vec!["title-1", "body-1", "chart-1", "stat-1"]
        );

        // Move chart to the front of the reading sequence.
        assert!(order.move_element("chart-1", 0));
        assert_eq!(
            order.ordered_element_ids,
            vec!["chart-1", "title-1", "body-1", "stat-1"]
        );
        // Absent id is a no-op; moving to the current position is a no-op.
        assert!(!order.move_element("missing-1", 0));
        assert!(!order.move_element("chart-1", 0));
        assert_eq!(
            order.ordered_element_ids,
            vec!["chart-1", "title-1", "body-1", "stat-1"]
        );

        assert!(order.is_valid());

        // A duplicate id invalidates the order.
        order.ordered_element_ids.push("body-1".into());
        assert!(!order.is_valid());
        // Removing the duplicate fixes it again.
        order.ordered_element_ids.pop();
        assert!(order.is_valid());

        // An empty id invalidates the order even without duplicates.
        let mut broken = ReadingOrder::new(3);
        broken.append_missing(&["a".to_string(), String::new()]);
        assert!(!broken.is_valid());
        // Replacing the empty entry restores validity.
        let empty_pos = broken
            .ordered_element_ids
            .iter()
            .position(|id| id.is_empty())
            .unwrap();
        broken.ordered_element_ids[empty_pos] = "b".to_string();
        assert!(broken.is_valid());
    }

    #[test]
    fn fit_preserves_aspect_ratio_centered() {
        // 2:1 source into a square box letterboxes top and bottom, filling width.
        assert_eq!(
            fit_rect_into_box(200.0, 100.0, 100.0, 100.0, 400.0, 400.0),
            Ok((100.0, 200.0, 400.0, 200.0))
        );

        // 1:4 source into a wide box pillarboxes left and right, filling height.
        assert_eq!(
            fit_rect_into_box(50.0, 200.0, 0.0, 0.0, 800.0, 200.0),
            Ok((375.0, 0.0, 50.0, 200.0))
        );

        // Matching aspect ratio fills the box exactly at the box origin.
        assert_eq!(
            fit_rect_into_box(160.0, 90.0, 10.0, 20.0, 320.0, 180.0),
            Ok((10.0, 20.0, 320.0, 180.0))
        );

        // Zero-height source names the problem.
        assert_eq!(
            fit_rect_into_box(100.0, 0.0, 0.0, 0.0, 400.0, 400.0),
            Err("source height must be positive".to_string())
        );

        // Element geometry is mutated in place with the same fit semantics.
        let mut elem = SlideElement {
            id: "img-1".into(),
            element_type: ElementType::BodyText,
            content: "photo".into(),
            x: 500.0,
            y: 500.0,
            width: 300.0,
            height: 150.0,
            action: None,
        };
        fit_element_to_box(&mut elem, 0.0, 0.0, 600.0, 600.0).expect("fit failed");
        assert_eq!(elem.x, 0.0);
        assert_eq!(elem.y, 150.0);
        assert_eq!(elem.width, 600.0);
        assert_eq!(elem.height, 300.0);
    }

    #[test]
    fn grid_arrangement_layout() {
        let make = |id: &str| SlideElement {
            id: id.into(),
            element_type: ElementType::ShapeRectangle,
            content: "Box".into(),
            x: -100.0,
            y: -100.0,
            width: 10.0,
            height: 10.0,
            action: None,
        };
        let mut slide = Slide::new("slide-grid", "Grid", "blank");
        for name in ["a", "b", "c", "d"] {
            slide.add_element(make(name));
        }

        // 2x2 grid inside a 1000x800 area at (50,60) with 20px gutters.
        // cell_w = (1000-20)/2 = 490, cell_h = (800-20)/2 = 390
        let arranged =
            slide.arrange_grid(&["a", "b", "c", "d"], 50.0, 60.0, 1000.0, 800.0, 2, 20.0);
        assert_eq!(arranged, 4);

        let get =
            |slide: &Slide, id: &str| slide.elements.iter().find(|e| e.id == id).unwrap().clone();
        let a = get(&slide, "a");
        let b = get(&slide, "b");
        let c = get(&slide, "c");
        let d = get(&slide, "d");

        assert_eq!(a.x, 50.0);
        assert_eq!(a.y, 60.0);
        assert_eq!(a.width, 490.0);
        assert_eq!(a.height, 390.0);

        assert_eq!(b.x, 560.0); // 50 + 490 + 20
        assert_eq!(b.y, 60.0);

        assert_eq!(c.x, 50.0);
        assert_eq!(c.y, 470.0); // 60 + 390 + 20

        assert_eq!(d.x, 560.0);
        assert_eq!(d.y, 470.0);

        // Unknown ids are skipped and do not consume cells
        let arranged_partial =
            slide.arrange_grid(&["a", "missing", "d"], 0.0, 0.0, 100.0, 100.0, 1, 0.0);
        assert_eq!(arranged_partial, 2);
        let d2 = get(&slide, "d");
        assert_eq!(d2.x, 0.0);
        assert_eq!(d2.y, 50.0); // second row of the single-column grid

        // Zero columns is a no-op
        let noop = slide.arrange_grid(&["a"], 0.0, 0.0, 100.0, 100.0, 0, 5.0);
        assert_eq!(noop, 0);

        // A lone element fills the entire area exactly
        let mut solo_slide = Slide::new("slide-solo", "Solo", "blank");
        solo_slide.add_element(make("solo"));
        let n = solo_slide.arrange_grid(&["solo"], 0.0, 0.0, 640.0, 480.0, 1, 99.0);
        assert_eq!(n, 1);
        let s2 = &solo_slide.elements[0];
        assert_eq!(s2.width, 640.0);
        assert_eq!(s2.height, 480.0);
    }

    #[test]
    fn linked_asset_relink_lifecycle() {
        let original_hash = LinkedAsset::hash_bytes(b"original image bytes");
        let mut asset = LinkedAsset::new("asset-1", "/assets/hero.png", original_hash);
        assert_eq!(asset.state, LinkedAssetState::Linked);

        // Missing path => Missing.
        asset.refresh_state(|_| false, |_| original_hash);
        assert_eq!(asset.state, LinkedAssetState::Missing);

        // Path resolves with the same hash => back to Linked.
        asset.refresh_state(|_| true, |_| original_hash);
        assert_eq!(asset.state, LinkedAssetState::Linked);

        // Path resolves but content changed => Modified.
        asset.refresh_state(|_| true, |_| original_hash ^ 0xff);
        assert_eq!(asset.state, LinkedAssetState::Modified);

        // Refresh never sets Relinked on its own.
        asset.refresh_state(|_| true, |_| original_hash ^ 0xff);
        assert_eq!(asset.state, LinkedAssetState::Modified);

        // User relinks to a replacement path: path updated, rehashed, Relinked.
        let replacement_hash = LinkedAsset::hash_bytes(b"replacement image bytes");
        asset.relink("/assets/hero-v2.png", |_| replacement_hash);
        assert_eq!(asset.path, "/assets/hero-v2.png");
        assert_eq!(asset.content_hash, replacement_hash);
        assert_eq!(asset.state, LinkedAssetState::Relinked);

        // Refreshing after relink against matching disk state stays consistent.
        asset.refresh_state(|p| p == "/assets/hero-v2.png", |_| replacement_hash);
        assert_eq!(asset.state, LinkedAssetState::Linked);

        // hash_bytes is deterministic and input-sensitive.
        assert_eq!(
            LinkedAsset::hash_bytes(b"lorem ipsum"),
            LinkedAsset::hash_bytes(b"lorem ipsum")
        );
        assert_ne!(
            LinkedAsset::hash_bytes(b"lorem ipsum"),
            LinkedAsset::hash_bytes(b"lorem ipsum!")
        );
        assert_ne!(
            LinkedAsset::hash_bytes(b""),
            LinkedAsset::hash_bytes(b"\x00")
        );
    }

    #[test]
    fn deck_outline_round_trip() {
        let source = "\nTitle One\n  First point\n  Second point\n\nTitle Two\n";
        let deck = deck_from_outline(source).unwrap();
        assert_eq!(deck.len(), 2);

        // Export reproduces the outline structure
        let exported = deck_to_text_outline(&deck).unwrap();
        let lines: Vec<&str> = exported.lines().collect();
        assert_eq!(
            lines,
            vec!["Title One", "  First point", "  Second point", "Title Two"]
        );

        // Re-importing the export yields identical titles and bullet contents
        let reimported = deck_from_outline(&exported).unwrap();
        assert_eq!(reimported.len(), deck.len());
        for (original, round) in deck.iter().zip(reimported.iter()) {
            assert_eq!(original.title, round.title);
            let original_bullets: Vec<&str> = original
                .elements
                .iter()
                .filter(|e| e.element_type == ElementType::BodyText)
                .map(|e| e.content.as_str())
                .collect();
            let round_bullets: Vec<&str> = round
                .elements
                .iter()
                .filter(|e| e.element_type == ElementType::BodyText)
                .map(|e| e.content.as_str())
                .collect();
            assert_eq!(original_bullets, round_bullets);
        }

        // Empty input is rejected
        assert!(deck_to_text_outline(&[]).is_err());
    }

    #[test]
    fn accessibility_audit_flags_empty_visuals() {
        let text_elem = |id: &str, content: &str| SlideElement {
            id: id.into(),
            element_type: ElementType::BodyText,
            content: content.into(),
            x: 0.0,
            y: 0.0,
            width: 200.0,
            height: 40.0,
            action: None,
        };
        let shape = |id: &str, ty: ElementType, content: &str| SlideElement {
            id: id.into(),
            element_type: ty,
            content: content.into(),
            x: 10.0,
            y: 10.0,
            width: 100.0,
            height: 100.0,
            action: None,
        };

        let mut deck = Vec::new();

        // Text-only slide: never flagged.
        let mut slide = Slide::new("slide-intro", "Overview", "title_body");
        slide.add_element(text_elem("intro-body", "Welcome to the deck"));
        deck.push(slide);

        // Mixed slide: empty rectangle flagged, filled circle and empty StatCard clean.
        let mut slide = Slide::new("slide-mixed", "Results", "blank");
        slide.add_element(shape("rect-blank", ElementType::ShapeRectangle, ""));
        slide.add_element(shape("circle-ok", ElementType::ShapeCircle, "42%"));
        slide.add_element(SlideElement {
            element_type: ElementType::StatCard,
            content: String::new(),
            ..shape("stat-blank", ElementType::ShapeRectangle, "")
        });
        deck.push(slide);

        // Second offending slide: whitespace-only circle counts as empty.
        let mut slide = Slide::new("slide-final", "Closing", "blank");
        slide.add_element(shape("circle-blank", ElementType::ShapeCircle, "   "));
        slide.add_element(shape(
            "rect-ok",
            ElementType::ShapeRectangle,
            "Diagram of flow",
        ));
        deck.push(slide);

        let findings = audit_accessibility(&deck);
        assert_eq!(
            findings,
            vec![
                AccessibilityFinding {
                    slide_index: 1,
                    element_id: "rect-blank".to_string(),
                    issue: "empty-content".to_string(),
                },
                AccessibilityFinding {
                    slide_index: 2,
                    element_id: "circle-blank".to_string(),
                    issue: "empty-content".to_string(),
                },
            ]
        );

        // Summary math over all 6 elements across the deck.
        let total: usize = deck.iter().map(|s| s.elements.len()).sum();
        assert_eq!(total, 6);
        let summary = summarize_findings(total, &findings);
        assert_eq!(summary.total_elements, 6);
        assert_eq!(summary.flagged_elements, 2);
        assert!((summary.clean_fraction() - 4.0 / 6.0).abs() < f64::EPSILON);

        // Empty deck audits clean.
        assert!(audit_accessibility(&[]).is_empty());
        assert_eq!(summarize_findings(0, &[]), AccessibilitySummary::default());
        assert_eq!(summarize_findings(0, &[]).clean_fraction(), 1.0);
    }

    #[test]
    fn deck_integrity_digest_stability() {
        let mut doc = PresentationDocument::new("deck-digest", "Digest Deck");
        doc.add_slide("Market Overview", "content");
        doc.slides[0].add_element(SlideElement {
            id: "elem-title".into(),
            element_type: ElementType::Title,
            content: "Quarterly Results".into(),
            x: 40.0,
            y: 60.0,
            width: 320.0,
            height: 80.0,
            action: None,
        });
        doc.slides[1].add_element(SlideElement {
            id: "elem-body".into(),
            element_type: ElementType::BodyText,
            content: "Revenue grew 12%.".into(),
            x: 10.0,
            y: 20.0,
            width: 300.0,
            height: 120.0,
            action: None,
        });
        doc.slides[1].add_element(SlideElement {
            id: "elem-stat".into(),
            element_type: ElementType::StatCard,
            content: "+12% YoY".into(),
            x: 340.0,
            y: 20.0,
            width: 120.0,
            height: 90.0,
            action: None,
        });
        doc.slides[1].speaker_notes = "Walk through the chart slowly.".into();

        // Digest is stable across repeated evaluation.
        let baseline = doc.integrity_digest();
        assert_eq!(baseline, doc.integrity_digest());

        // Moving an element changes the digest.
        let mut moved = doc.clone();
        moved.slides[1].elements[0].x += 15.0;
        assert_ne!(moved.integrity_digest(), baseline);

        // Adding a slide changes the digest.
        let mut added = doc.clone();
        added.add_slide("Closing Remarks", "blank");
        assert_ne!(added.integrity_digest(), baseline);

        // Editing speaker notes changes the digest (notes participate by design).
        let mut noted = doc.clone();
        noted.slides[1].speaker_notes = "Different delivery notes.".into();
        assert_ne!(noted.integrity_digest(), baseline);

        // Reordering elements changes the digest.
        let mut reordered = doc.clone();
        assert!(reordered.slides[1].bring_to_front("elem-body"));
        assert_ne!(reordered.integrity_digest(), baseline);
    }

    #[test]
    fn pptx_title_extraction_orders_numerically_and_joins_runs() {
        fn run(text: &str) -> String {
            format!("<a:r><a:rPr lang=\"en-US\"/><a:t>{text}</a:t></a:r>")
        }
        fn paragraph(runs: &[String]) -> String {
            format!("<a:p>{}</a:p>", runs.concat())
        }
        fn slide_xml(paragraphs: &[String]) -> Vec<u8> {
            format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
                 <p:sld xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" \
                 xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" \
                 xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\">\
                 <p:cSld><p:spTree><p:sp><p:txBody><a:bodyPr/><a:lstStyle/>{}</p:txBody>\
                 </p:sp></p:spTree></p:cSld></p:sld>",
                paragraphs.concat()
            )
            .into_bytes()
        }

        let mut pptx = PackageArchive::new();
        pptx.add("[Content_Types].xml", b"<Types/>".to_vec())
            .unwrap();
        pptx.add(
            "ppt/slides/slide1.xml",
            slide_xml(&[paragraph(&[run("Title One")])]),
        )
        .unwrap();
        // Lexicographic path order would place slide10 before slide2; numeric order wins.
        pptx.add(
            "ppt/slides/slide10.xml",
            slide_xml(&[paragraph(&[run("Deck Ten")])]),
        )
        .unwrap();
        pptx.add(
            "ppt/slides/slide2.xml",
            slide_xml(&[
                paragraph(&[run("Split "), run("Title")]),
                paragraph(&[run("Second paragraph must be ignored")]),
            ]),
        )
        .unwrap();

        let bytes = pptx.to_bytes().unwrap();
        let titles = extract_pptx_titles(&bytes).unwrap();
        assert_eq!(titles, vec!["Title One", "Split Title", "Deck Ten"]);

        let slides = slides_from_pptx(&bytes).unwrap();
        assert_eq!(slides.len(), 3);
        assert_eq!(
            slides.iter().map(|s| s.title.as_str()).collect::<Vec<_>>(),
            vec!["Title One", "Split Title", "Deck Ten"]
        );
        for (index, slide) in slides.iter().enumerate() {
            assert_eq!(slide.id, format!("pptx-slide-{}", index + 1));
            assert_eq!(slide.layout, "imported-pptx");
        }

        // An archive without any ppt/slides/slide<N>.xml parts must error.
        let mut slideless = PackageArchive::new();
        slideless
            .add("[Content_Types].xml", b"<Types/>".to_vec())
            .unwrap();
        slideless
            .add("ppt/presentation.xml", b"<p:presentation/>".to_vec())
            .unwrap();
        assert!(extract_pptx_titles(&slideless.to_bytes().unwrap()).is_err());
        assert!(slides_from_pptx(&slideless.to_bytes().unwrap()).is_err());

        // Garbage bytes and a truncated valid archive must error, not panic or guess.
        assert!(extract_pptx_titles(b"this is not a zip archive").is_err());
        let truncated = &bytes[..bytes.len() / 2];
        assert!(extract_pptx_titles(truncated).is_err());
    }

    #[test]
    fn pptx_title_unescapes_entities_and_handles_empty_first_paragraphs() {
        fn one_slide_archive(slide_body: &str) -> Vec<u8> {
            let mut pptx = PackageArchive::new();
            pptx.add(
                "ppt/slides/slide1.xml",
                format!("<p:sld><p:cSld><p:spTree>{slide_body}</p:spTree></p:cSld></p:sld>")
                    .into_bytes(),
            )
            .unwrap();
            pptx.to_bytes().unwrap()
        }

        // All five entities decode once, across runs, including an attribute-bearing <a:t>.
        let entities = one_slide_archive(
            "<a:txBody><a:p>\
             <a:r><a:t>&amp;&lt;&gt;</a:t></a:r>\
             <a:r><a:t xml:space=\"preserve\"> &quot;&apos;</a:t></a:r>\
             </a:p>\
             <a:p><a:r><a:t>body ignored</a:t></a:r></a:p></a:txBody>",
        );
        assert_eq!(extract_pptx_titles(&entities).unwrap(), vec!["&<> \"'"]);

        // First paragraph with only formatting yields an empty title, not the body text.
        let empty_first = one_slide_archive(
            "<a:txBody><a:p><a:endParaRPr lang=\"en-US\"/></a:p>\
             <a:p><a:r><a:t>body ignored</a:t></a:r></a:p></a:txBody>",
        );
        assert_eq!(extract_pptx_titles(&empty_first).unwrap(), vec![""]);
        let slides = slides_from_pptx(&empty_first).unwrap();
        assert_eq!(slides.len(), 1);
        assert_eq!(slides[0].title, "");
        assert_eq!(slides[0].layout, "imported-pptx");

        // A self-closing first paragraph behaves identically.
        let self_closing = one_slide_archive(
            "<a:txBody><a:p/>\
             <a:p><a:r><a:t>body ignored</a:t></a:r></a:p></a:txBody>",
        );
        assert_eq!(extract_pptx_titles(&self_closing).unwrap(), vec![""]);

        // Escaped ampersand sequences are not double-decoded.
        let double_escape =
            one_slide_archive("<a:p><a:r><a:t>&amp;amp; stays literal</a:t></a:r></a:p>");
        assert_eq!(
            extract_pptx_titles(&double_escape).unwrap(),
            vec!["&amp; stays literal"]
        );
    }

    #[test]
    fn pptx_export_round_trips_through_import() {
        let titles = vec![
            "Opening".to_string(),
            "Split & <Escaped>".to_string(),
            String::new(),
            "Closing".to_string(),
        ];

        let pptx = export_pptx_from_titles(&titles).expect("export succeeds");
        let extracted = extract_pptx_titles(&pptx).expect("re-import succeeds");

        // Titles round-trip in order, including empty slides and entities.
        assert_eq!(extracted, titles);

        // Numeric slide ordering holds beyond nine parts.
        let many: Vec<String> = (0..12).map(|i| format!("Slide {i}")).collect();
        let big = export_pptx_from_titles(&many).unwrap();
        assert_eq!(extract_pptx_titles(&big).unwrap(), many);

        // Deck skeletons build from the export.
        let deck = slides_from_pptx(&pptx).unwrap();
        assert_eq!(deck.len(), 4);
        assert_eq!(deck[2].title, "");

        // Empty decks are rejected by the importer's no-parts rule.
        let none = export_pptx_from_titles(&[]).unwrap();
        assert!(extract_pptx_titles(&none).is_err());
    }
}
