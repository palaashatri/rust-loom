//! Canonical rich-text capability matrix and cross-app loss classification (GOV-09).

/// Target Loom application for rich-text capability checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RichTextApp {
    Writer,
    Sheets,
    Present,
}

/// How an inline or paragraph attribute behaves when pasted into a target app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttributeSupport {
    Preserved,
    Flattened,
    Dropped,
}

/// Canonical rich-text capability summary for each Loom office application.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppRichTextCapabilities {
    pub app: RichTextApp,
    pub bold_italic_underline_strike: AttributeSupport,
    pub foreground_color: AttributeSupport,
    pub font_family_and_size: AttributeSupport,
    pub superscript_subscript: AttributeSupport,
    pub hyperlinks: AttributeSupport,
    pub paragraph_alignment: AttributeSupport,
    pub list_levels: AttributeSupport,
    pub bidi_direction: AttributeSupport,
}

impl AppRichTextCapabilities {
    /// Returns the canonical capability matrix for `app`.
    pub const fn for_app(app: RichTextApp) -> Self {
        match app {
            RichTextApp::Writer => Self {
                app,
                bold_italic_underline_strike: AttributeSupport::Preserved,
                foreground_color: AttributeSupport::Preserved,
                font_family_and_size: AttributeSupport::Preserved,
                superscript_subscript: AttributeSupport::Preserved,
                hyperlinks: AttributeSupport::Preserved,
                paragraph_alignment: AttributeSupport::Preserved,
                list_levels: AttributeSupport::Preserved,
                bidi_direction: AttributeSupport::Preserved,
            },
            RichTextApp::Sheets => Self {
                app,
                bold_italic_underline_strike: AttributeSupport::Preserved,
                foreground_color: AttributeSupport::Preserved,
                font_family_and_size: AttributeSupport::Preserved,
                superscript_subscript: AttributeSupport::Dropped,
                hyperlinks: AttributeSupport::Preserved,
                paragraph_alignment: AttributeSupport::Preserved,
                list_levels: AttributeSupport::Flattened,
                bidi_direction: AttributeSupport::Preserved,
            },
            RichTextApp::Present => Self {
                app,
                bold_italic_underline_strike: AttributeSupport::Preserved,
                foreground_color: AttributeSupport::Preserved,
                font_family_and_size: AttributeSupport::Preserved,
                superscript_subscript: AttributeSupport::Dropped,
                hyperlinks: AttributeSupport::Flattened,
                paragraph_alignment: AttributeSupport::Preserved,
                list_levels: AttributeSupport::Preserved,
                bidi_direction: AttributeSupport::Preserved,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writer_preserves_all_canonical_attributes() {
        let caps = AppRichTextCapabilities::for_app(RichTextApp::Writer);
        assert_eq!(caps.superscript_subscript, AttributeSupport::Preserved);
        assert_eq!(caps.hyperlinks, AttributeSupport::Preserved);
    }

    #[test]
    fn sheets_and_present_classify_explicit_losses() {
        let sheets = AppRichTextCapabilities::for_app(RichTextApp::Sheets);
        assert_eq!(sheets.list_levels, AttributeSupport::Flattened);
        assert_eq!(sheets.superscript_subscript, AttributeSupport::Dropped);
        let present = AppRichTextCapabilities::for_app(RichTextApp::Present);
        assert_eq!(present.list_levels, AttributeSupport::Preserved);
        assert_eq!(present.hyperlinks, AttributeSupport::Flattened);
    }
}
