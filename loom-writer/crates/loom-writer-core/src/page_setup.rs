//! Document page setup: paper size, orientation, and margin preset or exact
//! user-entered page geometry.
//!
//! Owns the persisted page-setup value and its projection onto
//! [`PageStyle`] for layout and export.

use super::{ContentParser, JsonValue, PageMarginsPreset, PageOrientation, PageStyle, PaperSize};

/// Page background stored with the document and shared by screen and exports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PageAppearance {
    /// The traditional white document page.
    #[default]
    White,
    /// A solid black page with automatic ink resolved to white.
    Black,
}

impl PageAppearance {
    /// Stable content-JSON value.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::White => "white",
            Self::Black => "black",
        }
    }

    /// Ink for runs whose character style has no explicit RGB colour.
    pub const fn auto_ink_rgba(self) -> [u8; 4] {
        match self {
            Self::White => [0, 0, 0, 255],
            Self::Black => [255, 255, 255, 255],
        }
    }

    /// Solid page fill used by PDF output.
    pub const fn pdf_rgb(self) -> (f32, f32, f32) {
        match self {
            Self::White => (1.0, 1.0, 1.0),
            Self::Black => (0.0, 0.0, 0.0),
        }
    }

    /// Stable value used by the page canvas.
    pub const fn is_black(self) -> bool {
        matches!(self, Self::Black)
    }
}

/// Document-level page setup: paper size, orientation, margin preset, and appearance.
///
/// Composes the paper/orientation/margin primitives into the single value
/// persisted with a document; [`Self::page_style`] projects it onto the
/// concrete [`PageStyle`] consumed by layout and export.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PageSetup {
    /// Physical paper size.
    pub paper: PaperSize,
    /// Page orientation.
    pub orientation: PageOrientation,
    /// Margin preset.
    pub margins: PageMarginsPreset,
    /// Solid page background; white remains the default for old documents.
    pub appearance: PageAppearance,
    /// Exact page dimensions and margins, retained when either field is custom.
    pub custom_geometry: Option<CustomPageGeometry>,
}

/// Exact custom page geometry, stored as thousandths of a point.
///
/// Integer storage keeps document equality and history deterministic while
/// retaining substantially more precision than the page layout can display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CustomPageGeometry {
    width_milli_pt: u32,
    height_milli_pt: u32,
    top_milli_pt: u32,
    bottom_milli_pt: u32,
    left_milli_pt: u32,
    right_milli_pt: u32,
}

const MIN_CUSTOM_PAGE_PT: f64 = 72.0;
const MAX_CUSTOM_PAGE_PT: f64 = 2_000.0;
const MIN_PRINTABLE_PT: f64 = 36.0;
const MAX_CUSTOM_MARGIN_PT: f64 = 2_000.0;
const MILLI_POINTS_PER_POINT: f64 = 1_000.0;

impl CustomPageGeometry {
    /// Construct validated geometry from points: width, height, top, bottom,
    /// left, and right.
    pub fn from_points(
        width_pt: f32,
        height_pt: f32,
        top_margin_pt: f32,
        bottom_margin_pt: f32,
        left_margin_pt: f32,
        right_margin_pt: f32,
    ) -> Result<Self, &'static str> {
        let values = [
            width_pt,
            height_pt,
            top_margin_pt,
            bottom_margin_pt,
            left_margin_pt,
            right_margin_pt,
        ];
        if values.iter().any(|value| !value.is_finite()) {
            return Err("page geometry values must be finite");
        }
        let [width, height, top, bottom, left, right] = values.map(f64::from);
        if !(MIN_CUSTOM_PAGE_PT..=MAX_CUSTOM_PAGE_PT).contains(&width)
            || !(MIN_CUSTOM_PAGE_PT..=MAX_CUSTOM_PAGE_PT).contains(&height)
        {
            return Err("custom page dimensions must be between 72 and 2000 points");
        }
        if [top, bottom, left, right]
            .iter()
            .any(|margin| !(0.0..=MAX_CUSTOM_MARGIN_PT).contains(margin))
        {
            return Err("custom margins must be between 0 and 2000 points");
        }
        if width - left - right < MIN_PRINTABLE_PT || height - top - bottom < MIN_PRINTABLE_PT {
            return Err("page margins must leave at least 36 points for text");
        }
        Ok(Self {
            width_milli_pt: to_milli_points(width),
            height_milli_pt: to_milli_points(height),
            top_milli_pt: to_milli_points(top),
            bottom_milli_pt: to_milli_points(bottom),
            left_milli_pt: to_milli_points(left),
            right_milli_pt: to_milli_points(right),
        })
    }

    /// Return width and height in points.
    pub fn dimensions_pt(self) -> (f32, f32) {
        (
            from_milli_points(self.width_milli_pt),
            from_milli_points(self.height_milli_pt),
        )
    }

    /// Return top, bottom, left, and right margins in points.
    pub fn margins_pt(self) -> (f32, f32, f32, f32) {
        (
            from_milli_points(self.top_milli_pt),
            from_milli_points(self.bottom_milli_pt),
            from_milli_points(self.left_milli_pt),
            from_milli_points(self.right_milli_pt),
        )
    }

    fn swapped_dimensions(self) -> Self {
        Self {
            width_milli_pt: self.height_milli_pt,
            height_milli_pt: self.width_milli_pt,
            ..self
        }
    }

    fn write_content_json(self) -> String {
        format!(
            "{{\"width_mpt\":{},\"height_mpt\":{},\"top_mpt\":{},\"bottom_mpt\":{},\"left_mpt\":{},\"right_mpt\":{}}}",
            self.width_milli_pt,
            self.height_milli_pt,
            self.top_milli_pt,
            self.bottom_milli_pt,
            self.left_milli_pt,
            self.right_milli_pt,
        )
    }

    fn from_content_json_value(value: &str) -> Result<Self, String> {
        let fields = ContentParser::new(value)
            .parse()
            .map_err(|error| error.to_string())?;
        let mut raw = [None; 6];
        for (key, value) in &fields {
            let index = match key.as_str() {
                "width_mpt" => 0,
                "height_mpt" => 1,
                "top_mpt" => 2,
                "bottom_mpt" => 3,
                "left_mpt" => 4,
                "right_mpt" => 5,
                _ => continue,
            };
            if raw[index].is_some() {
                return Err(format!("duplicate custom page geometry field {key}"));
            }
            let JsonValue::Raw(value) = value else {
                return Err(format!(
                    "custom page geometry field {key} must be an integer"
                ));
            };
            raw[index] = Some(
                value
                    .parse::<u32>()
                    .map_err(|_| format!("invalid custom page geometry field {key}"))?,
            );
        }
        let [Some(width), Some(height), Some(top), Some(bottom), Some(left), Some(right)] = raw
        else {
            return Err("custom page geometry is missing a required value".into());
        };
        let geometry = Self {
            width_milli_pt: width,
            height_milli_pt: height,
            top_milli_pt: top,
            bottom_milli_pt: bottom,
            left_milli_pt: left,
            right_milli_pt: right,
        };
        let (width_pt, height_pt) = geometry.dimensions_pt();
        let (top_pt, bottom_pt, left_pt, right_pt) = geometry.margins_pt();
        Self::from_points(width_pt, height_pt, top_pt, bottom_pt, left_pt, right_pt)
            .map_err(str::to_string)
    }
}

fn to_milli_points(points: f64) -> u32 {
    (points * MILLI_POINTS_PER_POINT).round() as u32
}

fn from_milli_points(milli_points: u32) -> f32 {
    milli_points as f32 / MILLI_POINTS_PER_POINT as f32
}

impl PageSetup {
    /// Projects this setup onto the concrete [`PageStyle`] used by layout,
    /// rendering, and export.
    pub fn page_style(&self) -> PageStyle {
        let (width, height) = if self.paper == PaperSize::Custom {
            self.custom_geometry
                .map(CustomPageGeometry::dimensions_pt)
                .unwrap_or_else(|| self.paper.dimensions_pt())
        } else {
            match self.orientation {
                PageOrientation::Portrait => self.paper.dimensions_pt(),
                PageOrientation::Landscape => {
                    let (w, h) = self.paper.dimensions_pt();
                    (h, w)
                }
            }
        };
        let (top, bottom, left, right) = if self.margins == PageMarginsPreset::Custom {
            self.custom_geometry
                .map(CustomPageGeometry::margins_pt)
                .unwrap_or_else(|| self.margins.margins_pt())
        } else {
            self.margins.margins_pt()
        };
        PageStyle {
            width_pt: width,
            height_pt: height,
            margin_top_pt: top,
            margin_bottom_pt: bottom,
            margin_left_pt: left,
            margin_right_pt: right,
            ..PageStyle::default()
        }
    }

    /// Return custom width, height, and margins in points when present.
    pub fn custom_geometry_points(self) -> Option<(f32, f32, f32, f32, f32, f32)> {
        self.custom_geometry.map(|geometry| {
            let (width, height) = geometry.dimensions_pt();
            let (top, bottom, left, right) = geometry.margins_pt();
            (width, height, top, bottom, left, right)
        })
    }

    /// Set exact custom geometry from points. The document stores dimensions
    /// and margins together so editing one field cannot lose the others.
    pub fn set_custom_geometry_points(
        &mut self,
        width_pt: f32,
        height_pt: f32,
        top_margin_pt: f32,
        bottom_margin_pt: f32,
        left_margin_pt: f32,
        right_margin_pt: f32,
    ) -> Result<(), &'static str> {
        let geometry = CustomPageGeometry::from_points(
            width_pt,
            height_pt,
            top_margin_pt,
            bottom_margin_pt,
            left_margin_pt,
            right_margin_pt,
        )?;
        self.paper = PaperSize::Custom;
        self.orientation = if width_pt > height_pt {
            PageOrientation::Landscape
        } else {
            PageOrientation::Portrait
        };
        self.margins = PageMarginsPreset::Custom;
        self.custom_geometry = Some(geometry);
        Ok(())
    }

    /// Set exact custom paper dimensions while retaining the current margins.
    pub fn set_custom_dimensions_points(
        &mut self,
        width_pt: f32,
        height_pt: f32,
    ) -> Result<(), &'static str> {
        let style = self.page_style();
        let geometry = CustomPageGeometry::from_points(
            width_pt,
            height_pt,
            style.margin_top_pt,
            style.margin_bottom_pt,
            style.margin_left_pt,
            style.margin_right_pt,
        )?;
        self.paper = PaperSize::Custom;
        self.orientation = if width_pt > height_pt {
            PageOrientation::Landscape
        } else {
            PageOrientation::Portrait
        };
        self.custom_geometry = Some(geometry);
        Ok(())
    }

    /// Set exact custom margins while retaining the current paper dimensions.
    pub fn set_custom_margins_points(
        &mut self,
        top_margin_pt: f32,
        bottom_margin_pt: f32,
        left_margin_pt: f32,
        right_margin_pt: f32,
    ) -> Result<(), &'static str> {
        let style = self.page_style();
        let geometry = CustomPageGeometry::from_points(
            style.width_pt,
            style.height_pt,
            top_margin_pt,
            bottom_margin_pt,
            left_margin_pt,
            right_margin_pt,
        )?;
        self.margins = PageMarginsPreset::Custom;
        self.custom_geometry = Some(geometry);
        Ok(())
    }

    /// Select a standard paper size or initialize the custom geometry using
    /// the current effective page dimensions.
    pub fn select_paper(&mut self, paper: PaperSize) {
        if paper == PaperSize::Custom && self.custom_geometry.is_none() {
            let style = self.page_style();
            self.custom_geometry = CustomPageGeometry::from_points(
                style.width_pt,
                style.height_pt,
                style.margin_top_pt,
                style.margin_bottom_pt,
                style.margin_left_pt,
                style.margin_right_pt,
            )
            .ok();
        }
        self.paper = paper;
        self.prune_custom_geometry();
    }

    /// Select portrait/landscape; custom geometry swaps its width and height
    /// so the visible orientation control remains truthful.
    pub fn select_orientation(&mut self, orientation: PageOrientation) {
        if self.paper == PaperSize::Custom {
            if let Some(geometry) = self.custom_geometry {
                let (width, height) = geometry.dimensions_pt();
                let swap = (orientation == PageOrientation::Landscape && width < height)
                    || (orientation == PageOrientation::Portrait && width > height);
                if swap {
                    self.custom_geometry = Some(geometry.swapped_dimensions());
                }
            }
        }
        self.orientation = orientation;
    }

    /// Select a standard margin preset or initialize custom margins from the
    /// current effective page style.
    pub fn select_margins(&mut self, margins: PageMarginsPreset) {
        if margins == PageMarginsPreset::Custom && self.custom_geometry.is_none() {
            let style = self.page_style();
            self.custom_geometry = CustomPageGeometry::from_points(
                style.width_pt,
                style.height_pt,
                style.margin_top_pt,
                style.margin_bottom_pt,
                style.margin_left_pt,
                style.margin_right_pt,
            )
            .ok();
        }
        self.margins = margins;
        self.prune_custom_geometry();
    }

    fn prune_custom_geometry(&mut self) {
        if self.paper != PaperSize::Custom && self.margins != PageMarginsPreset::Custom {
            self.custom_geometry = None;
        }
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        let needs_geometry =
            self.paper == PaperSize::Custom || self.margins == PageMarginsPreset::Custom;
        if needs_geometry && self.custom_geometry.is_none() {
            return Err("custom page settings are missing their geometry".into());
        }
        if !needs_geometry && self.custom_geometry.is_some() {
            return Err("custom page geometry has no custom page setting".into());
        }
        if let Some(geometry) = self.custom_geometry {
            let (width, height) = geometry.dimensions_pt();
            let (top, bottom, left, right) = geometry.margins_pt();
            CustomPageGeometry::from_points(width, height, top, bottom, left, right)
                .map_err(str::to_string)?;
            if self.paper == PaperSize::Custom
                && width != height
                && ((self.orientation == PageOrientation::Landscape) != (width > height))
            {
                return Err("custom page orientation does not match its dimensions".into());
            }
        }
        Ok(())
    }

    /// Stable JSON name for the paper size.
    fn paper_name(&self) -> &'static str {
        match self.paper {
            PaperSize::A4 => "a4",
            PaperSize::Letter => "letter",
            PaperSize::Legal => "legal",
            PaperSize::Executive => "executive",
            PaperSize::A3 => "a3",
            PaperSize::A5 => "a5",
            PaperSize::Custom => "custom",
        }
    }

    /// Parses a paper size from its JSON name.
    fn paper_from_name(name: &str) -> Option<PaperSize> {
        Some(match name {
            "a4" => PaperSize::A4,
            "letter" => PaperSize::Letter,
            "legal" => PaperSize::Legal,
            "executive" => PaperSize::Executive,
            "a3" => PaperSize::A3,
            "a5" => PaperSize::A5,
            "custom" => PaperSize::Custom,
            _ => return None,
        })
    }

    /// Stable JSON name for the orientation.
    fn orientation_name(&self) -> &'static str {
        match self.orientation {
            PageOrientation::Portrait => "portrait",
            PageOrientation::Landscape => "landscape",
        }
    }

    /// Parses an orientation from its JSON name.
    fn orientation_from_name(name: &str) -> Option<PageOrientation> {
        Some(match name {
            "portrait" => PageOrientation::Portrait,
            "landscape" => PageOrientation::Landscape,
            _ => return None,
        })
    }

    /// Stable JSON name for the margin preset.
    fn margins_name(&self) -> &'static str {
        match self.margins {
            PageMarginsPreset::Normal => "normal",
            PageMarginsPreset::Narrow => "narrow",
            PageMarginsPreset::Moderate => "moderate",
            PageMarginsPreset::Wide => "wide",
            PageMarginsPreset::Custom => "custom",
        }
    }

    /// Parses a margin preset from its JSON name.
    fn margins_from_name(name: &str) -> Option<PageMarginsPreset> {
        Some(match name {
            "normal" => PageMarginsPreset::Normal,
            "narrow" => PageMarginsPreset::Narrow,
            "moderate" => PageMarginsPreset::Moderate,
            "wide" => PageMarginsPreset::Wide,
            "custom" => PageMarginsPreset::Custom,
            _ => return None,
        })
    }

    /// Resolve the persisted page setup while preserving legacy white-page defaults.
    pub(crate) fn from_document_content_value(
        value: Option<&str>,
        format: u32,
    ) -> Result<PageSetup, String> {
        match value {
            Some(raw) => Self::from_content_json_value_for_format(raw, format),
            None if (1..=7).contains(&format) => Ok(PageSetup::default()),
            None if format == 8 => Err("content format 8 is missing page setup settings".into()),
            None => Err("unsupported page setup content format".into()),
        }
    }

    pub(crate) fn write_content_json(self) -> String {
        let custom = self
            .custom_geometry
            .map(|geometry| format!(",\"custom_geometry\":{}", geometry.write_content_json()))
            .unwrap_or_default();
        format!(
            "{{\"paper\":\"{}\",\"orientation\":\"{}\",\"margins\":\"{}\",\"appearance\":\"{}\"{custom}}}",
            self.paper_name(),
            self.orientation_name(),
            self.margins_name(),
            self.appearance.as_str(),
        )
    }

    #[cfg(test)]
    pub(crate) fn from_content_json_value(value: &str) -> Result<PageSetup, String> {
        Self::parse_content_json_value(value, false, false)
    }

    /// Parses page setup using the document content version's appearance rules.
    pub(crate) fn from_content_json_value_for_format(
        value: &str,
        format: u32,
    ) -> Result<PageSetup, String> {
        match format {
            1..=7 => Self::parse_content_json_value(value, false, true),
            8 => Self::parse_content_json_value(value, true, false),
            _ => Err("unsupported page setup content format".into()),
        }
    }

    fn parse_content_json_value(
        value: &str,
        appearance_required: bool,
        appearance_forbidden: bool,
    ) -> Result<PageSetup, String> {
        let fields = ContentParser::new(value)
            .parse()
            .map_err(|error| error.to_string())?;
        let mut paper = PaperSize::default();
        let mut orientation = PageOrientation::default();
        let mut margins = PageMarginsPreset::default();
        let mut appearance = PageAppearance::default();
        let mut appearance_seen = false;
        let mut custom_geometry = None;
        let mut seen = Vec::with_capacity(fields.len());
        for (key, value) in &fields {
            if seen.contains(&key) {
                return Err(format!("duplicate page setup field {key}"));
            }
            seen.push(key);
            let text = match value {
                JsonValue::String(text) => text.as_str(),
                JsonValue::Raw(_) => "",
            };
            match key.as_str() {
                "paper" => {
                    paper = Self::paper_from_name(text)
                        .ok_or_else(|| "unknown page paper size".to_string())?
                }
                "orientation" => {
                    orientation = Self::orientation_from_name(text)
                        .ok_or_else(|| "unknown page orientation".to_string())?
                }
                "margins" => {
                    margins = Self::margins_from_name(text)
                        .ok_or_else(|| "unknown page margin preset".to_string())?
                }
                "appearance" => {
                    if appearance_forbidden {
                        return Err("page appearance requires content format 8 or later".into());
                    }
                    appearance = match value {
                        JsonValue::String(name) if name == "white" => PageAppearance::White,
                        JsonValue::String(name) if name == "black" => PageAppearance::Black,
                        JsonValue::String(_) => {
                            return Err("unknown page appearance".into());
                        }
                        JsonValue::Raw(_) => {
                            return Err("page appearance must be a string".into());
                        }
                    };
                    appearance_seen = true;
                }
                "custom_geometry" => {
                    let JsonValue::Raw(raw) = value else {
                        return Err("custom page geometry must be an object".into());
                    };
                    custom_geometry = Some(CustomPageGeometry::from_content_json_value(raw)?);
                }
                _ => {}
            }
        }
        if appearance_required && !appearance_seen {
            return Err("content format 8 is missing page appearance".into());
        }
        let setup = PageSetup {
            paper,
            orientation,
            margins,
            appearance,
            custom_geometry,
        };
        setup.validate()?;
        Ok(setup)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_setup_projects_a4_portrait_normal_margins() {
        let style = PageSetup::default().page_style();
        assert_eq!((style.width_pt, style.height_pt), (595.0, 842.0));
        assert_eq!(style.margin_top_pt, 72.0);
        assert_eq!(style.margin_left_pt, 72.0);
    }

    #[test]
    fn landscape_swaps_dimensions_and_wide_margins_apply() {
        let setup = PageSetup {
            paper: PaperSize::Letter,
            orientation: PageOrientation::Landscape,
            margins: PageMarginsPreset::Wide,
            ..PageSetup::default()
        };
        let style = setup.page_style();
        assert_eq!((style.width_pt, style.height_pt), (792.0, 612.0));
        assert_eq!(style.margin_left_pt, 144.0);
    }

    #[test]
    fn content_json_round_trips_all_fields() {
        let setup = PageSetup {
            paper: PaperSize::Legal,
            orientation: PageOrientation::Landscape,
            margins: PageMarginsPreset::Moderate,
            ..PageSetup::default()
        };
        let json = setup.write_content_json();
        assert_eq!(
            json,
            r#"{"paper":"legal","orientation":"landscape","margins":"moderate","appearance":"white"}"#
        );
        assert_eq!(PageSetup::from_content_json_value(&json), Ok(setup));
    }

    #[test]
    fn black_appearance_survives_loom_save_and_open() {
        let mut document = crate::WriterDocument::new("black-page", "Black page");
        document.page.appearance = PageAppearance::Black;

        let saved = crate::save_document(&document).expect("save .loomdoc");
        let opened = crate::load_document(&saved).expect("open .loomdoc");
        assert_eq!(opened.page.appearance, PageAppearance::Black);
    }

    #[test]
    fn content_json_rejects_unknown_names() {
        assert_eq!(
            PageSetup::from_content_json_value(r#"{"paper":"tabloid"}"#),
            Err("unknown page paper size".into())
        );
        assert_eq!(
            PageSetup::from_content_json_value(r#"{"orientation":"diagonal"}"#),
            Err("unknown page orientation".into())
        );
    }

    #[test]
    fn custom_geometry_validates_projects_and_round_trips() {
        let mut setup = PageSetup::default();
        setup
            .set_custom_geometry_points(612.25, 792.5, 24.0, 30.0, 36.0, 42.0)
            .expect("valid exact page setup");
        let style = setup.page_style();
        assert_eq!((style.width_pt, style.height_pt), (612.25, 792.5));
        assert_eq!(
            (
                style.margin_top_pt,
                style.margin_bottom_pt,
                style.margin_left_pt,
                style.margin_right_pt,
            ),
            (24.0, 30.0, 36.0, 42.0)
        );
        let json = setup.write_content_json();
        assert!(json.contains("\"paper\":\"custom\""));
        assert!(json.contains("\"margins\":\"custom\""));
        assert_eq!(PageSetup::from_content_json_value(&json), Ok(setup));
    }

    #[test]
    fn custom_orientation_swaps_dimensions_without_changing_margins() {
        let mut setup = PageSetup::default();
        setup
            .set_custom_geometry_points(600.0, 800.0, 20.0, 25.0, 30.0, 35.0)
            .expect("valid exact page setup");
        setup.select_orientation(PageOrientation::Landscape);
        let style = setup.page_style();
        assert_eq!((style.width_pt, style.height_pt), (800.0, 600.0));
        assert_eq!((style.margin_top_pt, style.margin_left_pt), (20.0, 30.0));
    }

    #[test]
    fn invalid_or_incomplete_custom_geometry_is_rejected() {
        assert_eq!(
            CustomPageGeometry::from_points(612.0, 792.0, 400.0, 400.0, 72.0, 72.0),
            Err("page margins must leave at least 36 points for text")
        );
        assert!(PageSetup::from_content_json_value(
            r#"{"paper":"custom","orientation":"portrait","margins":"normal"}"#
        )
        .is_err());
        assert!(PageSetup::from_content_json_value(
            r#"{"paper":"custom","orientation":"portrait","margins":"custom","custom_geometry":{"width_mpt":1000}}"#
        )
        .is_err());
    }

    #[test]
    fn custom_geometry_requires_a_custom_setting_and_consistent_orientation() {
        let geometry = "\"custom_geometry\":{\"width_mpt\":612000,\"height_mpt\":792000,\"top_mpt\":24000,\"bottom_mpt\":30000,\"left_mpt\":36000,\"right_mpt\":42000}";
        assert!(PageSetup::from_content_json_value(&format!(
            "{{\"paper\":\"a4\",\"margins\":\"normal\",{geometry}}}"
        ))
        .is_err());
        assert!(PageSetup::from_content_json_value(&format!(
            "{{\"paper\":\"custom\",\"orientation\":\"portrait\",\"margins\":\"custom\",{geometry}}}"
        ))
        .is_ok());
        assert!(PageSetup::from_content_json_value(&format!(
            "{{\"paper\":\"custom\",\"orientation\":\"landscape\",\"margins\":\"custom\",{geometry}}}"
        ))
        .is_err());
    }

    #[test]
    fn square_custom_pages_allow_either_orientation() {
        let mut setup = PageSetup::default();
        setup
            .set_custom_geometry_points(600.0, 600.0, 24.0, 24.0, 24.0, 24.0)
            .expect("valid square custom page");
        setup.select_orientation(PageOrientation::Landscape);
        assert!(setup.validate().is_ok());
    }

    #[test]
    fn custom_dimension_and_margin_edits_preserve_unedited_settings() {
        let mut setup = PageSetup::default();
        setup
            .set_custom_dimensions_points(600.0, 800.0)
            .expect("valid custom dimensions");
        assert_eq!(setup.paper, PaperSize::Custom);
        assert_eq!(setup.margins, PageMarginsPreset::Normal);
        assert_eq!(setup.page_style().margin_top_pt, 72.0);

        setup
            .set_custom_margins_points(24.0, 30.0, 36.0, 42.0)
            .expect("valid custom margins");
        assert_eq!(setup.paper, PaperSize::Custom);
        assert_eq!(setup.margins, PageMarginsPreset::Custom);
        assert_eq!(setup.page_style().width_pt, 600.0);
        assert_eq!(setup.page_style().margin_top_pt, 24.0);

        let mut standard = PageSetup::default();
        standard
            .set_custom_margins_points(24.0, 30.0, 36.0, 42.0)
            .expect("custom margins on standard paper");
        assert_eq!(standard.paper, PaperSize::A4);
        assert_eq!(standard.page_style().width_pt, 595.0);
    }
}
