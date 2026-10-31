//! Complex-script run properties that Writer's single run style cannot carry.
//!
//! Import still reads the Latin axes (`w:b`, `w:i`, `w:sz`, `w:rFonts
//! w:ascii`). A script-specific property that disagrees with them is
//! unrepresentable, so the importer reports the exact dropped axes instead of
//! applying them to Latin text or discarding them silently.

use super::{xml_property_is_enabled, W_NS};

/// Complex-script run properties that differ from the run's Latin formatting.
/// Writer stores one character style per run, so it reads `w:b`, `w:i`, `w:sz`
/// and `w:rFonts w:ascii` and cannot apply a script-specific alternative.
pub(super) fn only_losses(document: &roxmltree::Document<'_>) -> Vec<&'static str> {
    const BOLD: &str = "bold (w:bCs)";
    const ITALIC: &str = "italic (w:iCs)";
    const SIZE: &str = "size (w:szCs)";
    const FONT: &str = "font (w:rFonts w:cs)";
    let mut losses = Vec::new();
    let record = |loss: &'static str, losses: &mut Vec<&'static str>| {
        if !losses.contains(&loss) {
            losses.push(loss);
        }
    };
    for run in document
        .descendants()
        .filter(|node| node.has_tag_name((W_NS, "r")))
    {
        let Some(properties) = run.children().find(|node| node.has_tag_name((W_NS, "rPr"))) else {
            continue;
        };
        let toggled = |name: &str| {
            properties
                .children()
                .find(|node| node.has_tag_name((W_NS, name)))
                .map(xml_property_is_enabled)
        };
        let half_points = |name: &str| {
            properties
                .children()
                .find(|node| node.has_tag_name((W_NS, name)))
                .and_then(|node| node.attribute((W_NS, "val")))
                .and_then(|value| value.parse::<f32>().ok())
        };
        let fonts = properties
            .children()
            .find(|node| node.has_tag_name((W_NS, "rFonts")));

        if toggled("bCs").is_some_and(|cs| cs != toggled("b").unwrap_or(false)) {
            record(BOLD, &mut losses);
        }
        if toggled("iCs").is_some_and(|cs| cs != toggled("i").unwrap_or(false)) {
            record(ITALIC, &mut losses);
        }
        if half_points("szCs").is_some_and(|cs| Some(cs) != half_points("sz")) {
            record(SIZE, &mut losses);
        }
        let script_font = fonts.and_then(|fonts| fonts.attribute((W_NS, "cs")));
        let latin_font = fonts.and_then(|fonts| fonts.attribute((W_NS, "ascii")));
        if script_font.is_some_and(|font| Some(font) != latin_font) {
            record(FONT, &mut losses);
        }
    }
    losses
}
