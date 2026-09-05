//! Document page setup: paper size, orientation, and margin preset.
//!
//! Owns the persisted page-setup value and its projection onto
//! [`PageStyle`] for layout and export.

use super::{ContentParser, JsonValue, PageMarginsPreset, PageOrientation, PageStyle, PaperSize};

/// Document-level page setup: paper size, orientation, and margin preset.
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
}

impl PageSetup {
    /// Projects this setup onto the concrete [`PageStyle`] used by layout,
    /// rendering, and export.
    pub fn page_style(&self) -> PageStyle {
        let (width, height) = match self.orientation {
            PageOrientation::Portrait => self.paper.dimensions_pt(),
            PageOrientation::Landscape => {
                let (w, h) = self.paper.dimensions_pt();
                (h, w)
            }
        };
        let (top, bottom, left, right) = self.margins.margins_pt();
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

    /// Stable JSON name for the paper size.
    fn paper_name(&self) -> &'static str {
        match self.paper {
            PaperSize::A4 => "a4",
            PaperSize::Letter => "letter",
            PaperSize::Legal => "legal",
            PaperSize::Executive => "executive",
            PaperSize::A3 => "a3",
            PaperSize::A5 => "a5",
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
        }
    }

    /// Parses a margin preset from its JSON name.
    fn margins_from_name(name: &str) -> Option<PageMarginsPreset> {
        Some(match name {
            "normal" => PageMarginsPreset::Normal,
            "narrow" => PageMarginsPreset::Narrow,
            "moderate" => PageMarginsPreset::Moderate,
            "wide" => PageMarginsPreset::Wide,
            _ => return None,
        })
    }

    /// Serializes to the compact JSON object embedded in document content.
    pub(crate) fn write_content_json(self) -> String {
        format!(
            "{{\"paper\":\"{}\",\"orientation\":\"{}\",\"margins\":\"{}\"}}",
            self.paper_name(),
            self.orientation_name(),
            self.margins_name()
        )
    }

    /// Parses the JSON object written by [`Self::to_content_json`].
    pub(crate) fn from_content_json_value(value: &str) -> Option<PageSetup> {
        let parser = ContentParser::new(value);
        let fields = parser.parse().ok()?;
        let mut paper = PaperSize::default();
        let mut orientation = PageOrientation::default();
        let mut margins = PageMarginsPreset::default();
        for (k, v) in &fields {
            let text = match v {
                JsonValue::String(s) => s.clone(),
                JsonValue::Raw(raw) => raw.trim_matches('"').to_string(),
            };
            match k.as_str() {
                "paper" => paper = Self::paper_from_name(&text)?,
                "orientation" => orientation = Self::orientation_from_name(&text)?,
                "margins" => margins = Self::margins_from_name(&text)?,
                _ => {}
            }
        }
        Some(PageSetup {
            paper,
            orientation,
            margins,
        })
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
        };
        let json = setup.write_content_json();
        assert_eq!(
            json,
            r#"{"paper":"legal","orientation":"landscape","margins":"moderate"}"#
        );
        assert_eq!(PageSetup::from_content_json_value(&json), Some(setup));
    }

    #[test]
    fn content_json_rejects_unknown_names() {
        assert_eq!(
            PageSetup::from_content_json_value(r#"{"paper":"tabloid"}"#),
            None
        );
        assert_eq!(
            PageSetup::from_content_json_value(r#"{"orientation":"diagonal"}"#),
            None
        );
    }
}
