//! Document-owned header and footer settings.

use super::{pkg_json, ContentParser, JsonValue, WriterError};

/// Maximum UTF-8 bytes stored in either a header or a footer.
pub const MAX_HEADER_FOOTER_TEXT_BYTES: usize = 2_048;
pub(crate) const MIN_HEADER_FOOTER_MARGIN_PT: f32 = 24.0;
/// Font size used by both the editor and PDF for header/footer lines.
pub const HEADER_FOOTER_FONT_SIZE_PT: f32 = 9.0;
/// Minimum line-box height in ems; mixed-script ascent/descent may raise it.
pub const HEADER_FOOTER_LINE_HEIGHT_EM: f32 = 1.5;

/// Maximum serialized bytes accepted for the complete settings object.
const MAX_HEADER_FOOTER_CONFIG_BYTES: usize = 32 * 1_024;

fn is_line_break(ch: char) -> bool {
    matches!(
        ch,
        '\r' | '\n' | '\u{000B}' | '\u{000C}' | '\u{0085}' | '\u{2028}' | '\u{2029}'
    )
}

/// Page number numbering format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PageNumberFormat {
    #[default]
    Arabic,
    RomanUpper,
    RomanLower,
    Alphabetical,
}

impl PageNumberFormat {
    /// Formats a 1-based page number into a string according to this format.
    pub fn format(&self, page_num: usize) -> String {
        match self {
            Self::Arabic => format!("{page_num}"),
            Self::RomanUpper => roman_number(page_num).unwrap_or_else(|| page_num.to_string()),
            Self::RomanLower => roman_number(page_num)
                .map(|number| number.to_ascii_lowercase())
                .unwrap_or_else(|| page_num.to_string()),
            Self::Alphabetical => alphabetical_number(page_num),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Arabic => "arabic",
            Self::RomanUpper => "roman_upper",
            Self::RomanLower => "roman_lower",
            Self::Alphabetical => "alphabetical",
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "arabic" => Self::Arabic,
            "roman_upper" => Self::RomanUpper,
            "roman_lower" => Self::RomanLower,
            "alphabetical" => Self::Alphabetical,
            _ => return None,
        })
    }
}

fn roman_number(mut value: usize) -> Option<String> {
    if !(1..=3_999).contains(&value) {
        return None;
    }
    const DIGITS: &[(usize, &str)] = &[
        (1_000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut output = String::with_capacity(15);
    for (amount, glyph) in DIGITS {
        while value >= *amount {
            output.push_str(glyph);
            value -= amount;
        }
    }
    Some(output)
}

fn alphabetical_number(mut value: usize) -> String {
    if value == 0 {
        return value.to_string();
    }
    let mut output = Vec::with_capacity(8);
    while value > 0 {
        value -= 1;
        output.push(b'A' + (value % 26) as u8);
        value /= 26;
    }
    output.reverse();
    String::from_utf8(output).expect("ASCII letters are valid UTF-8")
}

/// Header and footer layout configuration for paginated documents.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct HeaderFooterConfig {
    /// Header text (supports `{page}` and `{total}` placeholders).
    pub header_text: String,
    /// Footer text (supports `{page}` and `{total}` placeholders).
    pub footer_text: String,
    /// Header/footer text alignment: empty (left), `left`, `center`, or `right`.
    pub alignment: String,
    /// Page number formatting style.
    pub page_number_format: PageNumberFormat,
    /// Whether the first page omits headers and footers (e.g. cover page).
    pub different_first_page: bool,
}

/// Which repeated page region a header/footer line occupies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderFooterLineKind {
    Header,
    Footer,
}

/// Physical text alignment for repeated page lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderFooterAlignment {
    Left,
    Center,
    Right,
}

/// Canonical page-local geometry shared by the editor projection and PDF.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HeaderFooterLineGeometry {
    /// Left edge of the text column, in points from the page's left edge.
    pub x_pt: f32,
    /// Top edge of the line box, in points from the page's top edge.
    pub top_pt: f32,
    /// Width of the text column between the horizontal margins.
    pub width_pt: f32,
    /// Measured width of the formatted text, used for exact PDF alignment.
    pub text_width_pt: f32,
    /// Left edge of the formatted text, after physical alignment.
    pub text_x_pt: f32,
    /// Line-box height including the largest shaped script extents.
    pub height_pt: f32,
    /// PDF text baseline, in points from the page's bottom edge.
    pub pdf_baseline_y_pt: f32,
    /// Font size shared with the editor text component.
    pub font_size_pt: f32,
    /// Physical text alignment.
    pub alignment: HeaderFooterAlignment,
}

impl HeaderFooterConfig {
    /// Formats header text for a given page index (1-based) and total page count.
    pub fn format_header(&self, page_num: usize, total_pages: usize) -> Option<String> {
        self.format_text(&self.header_text, page_num, total_pages)
    }

    /// Formats footer text for a given page index (1-based) and total page count.
    pub fn format_footer(&self, page_num: usize, total_pages: usize) -> Option<String> {
        self.format_text(&self.footer_text, page_num, total_pages)
    }

    /// Treats the old empty alignment default as left aligned.
    pub fn effective_alignment(&self) -> &str {
        if self.alignment.is_empty() {
            "left"
        } else {
            &self.alignment
        }
    }

    fn maximum_width_expansion(&self, template: &str) -> String {
        let widest_number = match self.page_number_format {
            PageNumberFormat::Arabic => "8".repeat((i32::MAX as usize).to_string().len()),
            PageNumberFormat::RomanUpper => "MMMDCCCLXXXVIII".into(),
            PageNumberFormat::RomanLower => "mmmdccclxxxviii".into(),
            PageNumberFormat::Alphabetical => {
                "W".repeat(self.page_number_format.format(i32::MAX as usize).len())
            }
        };
        template
            .replace("{page}", &widest_number)
            .replace("{total}", &widest_number)
    }

    /// Returns the canonical page-local line box and PDF baseline for `text`.
    /// The supplied text must already have its page fields expanded.
    pub fn line_geometry(
        &self,
        text: &str,
        page: &crate::PageStyle,
        kind: HeaderFooterLineKind,
    ) -> Result<HeaderFooterLineGeometry, &'static str> {
        self.validate()?;
        if text.is_empty() {
            return Err("header/footer line text must not be empty");
        }
        if text.chars().any(is_line_break) {
            return Err("header/footer text must be a single line");
        }
        if !page.width_pt.is_finite()
            || !page.height_pt.is_finite()
            || !page.margin_top_pt.is_finite()
            || !page.margin_bottom_pt.is_finite()
            || !page.margin_left_pt.is_finite()
            || !page.margin_right_pt.is_finite()
        {
            return Err("page geometry must be finite for headers and footers");
        }
        let width_pt = page.width_pt - page.margin_left_pt - page.margin_right_pt;
        if !width_pt.is_finite() || width_pt <= 0.0 {
            return Err("page margins leave no room for header/footer text");
        }
        let margin = match kind {
            HeaderFooterLineKind::Header => page.margin_top_pt,
            HeaderFooterLineKind::Footer => page.margin_bottom_pt,
        };
        if margin < MIN_HEADER_FOOTER_MARGIN_PT {
            return Err("header/footer margin must be at least 24 pt");
        }

        let shaped = crate::shape_text_line(
            text,
            &[],
            HEADER_FOOTER_FONT_SIZE_PT,
            "paragraph",
            loom_text::ParagraphDirection::Auto,
            loom_text::Alignment::Left,
            0..text.len(),
            width_pt,
        )
        .ok_or("header/footer text could not be measured")?;
        let ascent = shaped.ascent_pt();
        let descent = shaped.descent_pt();
        let line_gap = shaped.line_gap_pt().max(0.0);
        let shaped_height = ascent + descent + line_gap;
        if !ascent.is_finite()
            || !descent.is_finite()
            || !line_gap.is_finite()
            || ascent < 0.0
            || descent < 0.0
            || !shaped_height.is_finite()
        {
            return Err("header/footer text has invalid font geometry");
        }
        let height_pt =
            (HEADER_FOOTER_FONT_SIZE_PT * HEADER_FOOTER_LINE_HEIGHT_EM).max(shaped_height);
        if !height_pt.is_finite() || height_pt > margin {
            return Err("header/footer text does not fit inside the page margin");
        }
        let baseline_from_top = (height_pt - shaped_height) / 2.0 + ascent;
        let top_pt = match kind {
            HeaderFooterLineKind::Header => (page.margin_top_pt - height_pt) / 2.0,
            HeaderFooterLineKind::Footer => {
                page.height_pt - page.margin_bottom_pt + (page.margin_bottom_pt - height_pt) / 2.0
            }
        };
        let font = loom_pdf::TextStyle {
            size_pt: HEADER_FOOTER_FONT_SIZE_PT,
            ..Default::default()
        };
        let text_width_pt = crate::text_metrics::pdf_text_width(text, &font);
        if !text_width_pt.is_finite() || text_width_pt > width_pt {
            return Err("header/footer text does not fit between the page margins");
        }
        let alignment = match self.effective_alignment() {
            "left" => HeaderFooterAlignment::Left,
            "center" => HeaderFooterAlignment::Center,
            "right" => HeaderFooterAlignment::Right,
            _ => return Err("header/footer alignment must be left, center, or right"),
        };
        let text_x_pt = match alignment {
            HeaderFooterAlignment::Left => page.margin_left_pt,
            HeaderFooterAlignment::Center => page.margin_left_pt + (width_pt - text_width_pt) / 2.0,
            HeaderFooterAlignment::Right => page.width_pt - page.margin_right_pt - text_width_pt,
        };
        let pdf_baseline_y_pt = page.height_pt - top_pt - baseline_from_top;
        if !top_pt.is_finite()
            || !text_x_pt.is_finite()
            || !pdf_baseline_y_pt.is_finite()
            || text_x_pt < page.margin_left_pt - 0.01
            || text_x_pt + text_width_pt > page.width_pt - page.margin_right_pt + 0.01
        {
            return Err("header/footer geometry falls outside the page margins");
        }
        Ok(HeaderFooterLineGeometry {
            x_pt: page.margin_left_pt,
            top_pt,
            width_pt,
            text_width_pt,
            text_x_pt,
            height_pt,
            pdf_baseline_y_pt,
            font_size_pt: HEADER_FOOTER_FONT_SIZE_PT,
            alignment,
        })
    }

    fn format_text(&self, text: &str, page_num: usize, total_pages: usize) -> Option<String> {
        if (self.different_first_page && page_num == 1) || text.is_empty() {
            return None;
        }
        Some(
            text.replace("{page}", &self.page_number_format.format(page_num))
                .replace("{total}", &total_pages.to_string()),
        )
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.header_text.len() > MAX_HEADER_FOOTER_TEXT_BYTES {
            return Err("header text exceeds the 2048-byte limit");
        }
        if self.footer_text.len() > MAX_HEADER_FOOTER_TEXT_BYTES {
            return Err("footer text exceeds the 2048-byte limit");
        }
        if self.header_text.chars().any(is_line_break) {
            return Err("header text must be a single line");
        }
        if self.footer_text.chars().any(is_line_break) {
            return Err("footer text must be a single line");
        }
        if !matches!(self.alignment.as_str(), "" | "left" | "center" | "right") {
            return Err("header/footer alignment must be left, center, or right");
        }
        Ok(())
    }

    pub fn validate_for_page_style(&self, page: &crate::PageStyle) -> Result<(), &'static str> {
        self.validate()?;
        if !self.header_text.is_empty()
            && (!page.margin_top_pt.is_finite() || page.margin_top_pt < MIN_HEADER_FOOTER_MARGIN_PT)
        {
            return Err("top margin must be at least 24 pt when a header is set");
        }
        if !self.footer_text.is_empty()
            && (!page.margin_bottom_pt.is_finite()
                || page.margin_bottom_pt < MIN_HEADER_FOOTER_MARGIN_PT)
        {
            return Err("bottom margin must be at least 24 pt when a footer is set");
        }

        // Bound every page field using the widest value this format can emit.
        if !self.header_text.is_empty() {
            let text = self.maximum_width_expansion(&self.header_text);
            self.line_geometry(&text, page, HeaderFooterLineKind::Header)?;
        }
        if !self.footer_text.is_empty() {
            let text = self.maximum_width_expansion(&self.footer_text);
            self.line_geometry(&text, page, HeaderFooterLineKind::Footer)?;
        }
        Ok(())
    }

    pub(crate) fn write_content_json(&self) -> String {
        format!(
            "{{\"header_text\":{},\"footer_text\":{},\"alignment\":{},\"page_number_format\":{},\"different_first_page\":{}}}",
            pkg_json::escape(&self.header_text),
            pkg_json::escape(&self.footer_text),
            pkg_json::escape(&self.alignment),
            pkg_json::escape(self.page_number_format.name()),
            self.different_first_page,
        )
    }

    pub(crate) fn from_content_json_value(value: &str) -> Result<Self, WriterError> {
        if value.len() > MAX_HEADER_FOOTER_CONFIG_BYTES {
            return Err(WriterError::Invalid(
                "header/footer settings exceed the 32768-byte limit".into(),
            ));
        }
        let fields = ContentParser::new(value).parse()?;
        let mut config = Self::default();
        let mut seen = Vec::with_capacity(fields.len());
        for (key, value) in fields {
            if seen.iter().any(|previous| previous == &key) {
                return Err(WriterError::Invalid(format!(
                    "duplicate header/footer setting {key}"
                )));
            }
            seen.push(key.clone());
            match key.as_str() {
                "header_text" => config.header_text = string_value(value, "header_text")?,
                "footer_text" => config.footer_text = string_value(value, "footer_text")?,
                "alignment" => config.alignment = string_value(value, "alignment")?,
                "page_number_format" => {
                    let name = string_value(value, "page_number_format")?;
                    config.page_number_format =
                        PageNumberFormat::from_name(&name).ok_or_else(|| {
                            WriterError::Invalid("unknown header/footer page number format".into())
                        })?;
                }
                "different_first_page" => {
                    config.different_first_page = bool_value(value, "different_first_page")?;
                }
                _ => {
                    return Err(WriterError::Invalid(format!(
                        "unsupported header/footer setting {key}"
                    )));
                }
            }
        }
        config
            .validate()
            .map_err(|error| WriterError::Invalid(error.into()))?;
        Ok(config)
    }
}

fn string_value(value: JsonValue, field: &str) -> Result<String, WriterError> {
    match value {
        JsonValue::String(value) => Ok(value),
        JsonValue::Raw(_) => Err(WriterError::Invalid(format!(
            "header/footer setting {field} must be a string"
        ))),
    }
}

fn bool_value(value: JsonValue, field: &str) -> Result<bool, WriterError> {
    match value {
        JsonValue::Raw(value) if value == "true" => Ok(true),
        JsonValue::Raw(value) if value == "false" => Ok(false),
        _ => Err(WriterError::Invalid(format!(
            "header/footer setting {field} must be a boolean"
        ))),
    }
}
