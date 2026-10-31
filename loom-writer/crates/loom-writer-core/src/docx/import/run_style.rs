//! Normalization of imported run styling that Writer treats as inherited.

use loom_text::CharacterStyle;

/// Drop an explicit size that only restates the block's inherited size, so a
/// plain Word run stays an unstyled Writer run after import. Run fonts are
/// never cleared: the exporter omits `w:rFonts` for inherited text, so any
/// `w:rFonts` an external document declares is the author's choice.
pub(super) fn normalize(style: &mut CharacterStyle, kind: &str) {
    let default_size = CharacterStyle::default().font_size;
    let kind_size = if kind.starts_with("heading") {
        crate::PageStyle::default().font_size_for_kind(kind)
    } else {
        default_size
    };
    if (style.font_size - kind_size).abs() < 0.01 {
        style.font_size = default_size;
        style.explicit_size = false;
    }
}
