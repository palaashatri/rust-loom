//! Complex-script run properties that Writer's single run style cannot carry.
//!
//! Import still reads the Latin axes (`w:b`, `w:i`, `w:sz`, `w:rFonts
//! w:ascii`). A script-specific property that disagrees with them is
//! unrepresentable, so the importer reports the exact dropped axes instead of
//! applying them to Latin text or discarding them silently.

use loom_package::zip::PackageArchive;

use super::{xml_property_is_enabled, W_NS};

/// Maximum XML size per DOCX part inspected for complex-script formatting.
const MAX_PART_XML_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct ArchiveLosses {
    pub axes: Vec<&'static str>,
    pub locations: Vec<&'static str>,
}

/// Scans `word/document.xml`, any `word/header*.xml` / `word/footer*.xml`, and
/// `word/footnotes.xml` in `archive` for complex-script run properties that
/// differ from Latin formatting.
pub(super) fn archive_losses(
    archive: &PackageArchive,
    body_document: Option<&roxmltree::Document<'_>>,
) -> ArchiveLosses {
    let mut losses = ArchiveLosses::default();

    let body_hit = match body_document {
        Some(doc) => collect_losses(doc, &mut losses.axes),
        None => scan_archive_part(archive, "word/document.xml", &mut losses.axes),
    };
    if body_hit {
        losses.locations.push("body");
    }

    let mut header_hit = false;
    let mut footer_hit = false;
    for path in archive.paths() {
        if is_word_part_matching(path, "header") {
            header_hit |= scan_archive_part(archive, path, &mut losses.axes);
        } else if is_word_part_matching(path, "footer") {
            footer_hit |= scan_archive_part(archive, path, &mut losses.axes);
        }
    }
    if header_hit {
        losses.locations.push("headers");
    }
    if footer_hit {
        losses.locations.push("footers");
    }
    if scan_archive_part(archive, "word/footnotes.xml", &mut losses.axes) {
        losses.locations.push("footnotes");
    }

    losses
}

/// Builds the user-visible fidelity warning when any part drops complex-script
/// formatting.
pub(super) fn unsupported_warning(
    archive: &PackageArchive,
    body_document: Option<&roxmltree::Document<'_>>,
) -> Option<String> {
    let losses = archive_losses(archive, body_document);
    if losses.axes.is_empty() {
        return None;
    }
    Some(format!(
        "Complex-script (Arabic/Hebrew) run formatting in {} is not supported: {} \
         cannot be represented and is ignored; the affected text itself is imported.",
        losses.locations.join(", "),
        losses.axes.join(", ")
    ))
}

fn is_word_part_matching(path: &str, prefix: &str) -> bool {
    path.strip_prefix("word/").is_some_and(|name| {
        !name.contains('/') && name.starts_with(prefix) && name.ends_with(".xml")
    })
}

fn scan_archive_part(archive: &PackageArchive, path: &str, axes: &mut Vec<&'static str>) -> bool {
    let Some(bytes) = archive.get(path) else {
        return false;
    };
    if bytes.len() > MAX_PART_XML_BYTES {
        return false;
    }
    let Ok(xml) = std::str::from_utf8(bytes) else {
        return false;
    };
    let Ok(document) = roxmltree::Document::parse(xml) else {
        return false;
    };
    collect_losses(&document, axes)
}

/// Records complex-script run properties in `document` that differ from the
/// run's Latin formatting. Returns `true` if at least one loss was observed.
fn collect_losses(document: &roxmltree::Document<'_>, axes: &mut Vec<&'static str>) -> bool {
    const BOLD: &str = "bold (w:bCs)";
    const ITALIC: &str = "italic (w:iCs)";
    const SIZE: &str = "size (w:szCs)";
    const FONT: &str = "font (w:rFonts w:cs)";
    let mut found = false;
    let mut record = |loss: &'static str| {
        found = true;
        if !axes.contains(&loss) {
            axes.push(loss);
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
            record(BOLD);
        }
        if toggled("iCs").is_some_and(|cs| cs != toggled("i").unwrap_or(false)) {
            record(ITALIC);
        }
        if half_points("szCs").is_some_and(|cs| Some(cs) != half_points("sz")) {
            record(SIZE);
        }
        let script_font = fonts.and_then(|fonts| fonts.attribute((W_NS, "cs")));
        let latin_font = fonts.and_then(|fonts| fonts.attribute((W_NS, "ascii")));
        if script_font.is_some_and(|font| Some(font) != latin_font) {
            record(FONT);
        }
    }
    found
}
