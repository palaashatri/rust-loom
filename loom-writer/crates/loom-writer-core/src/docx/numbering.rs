//! `word/numbering.xml` generation and import resolution for all Writer
//! `ListMarkerStyle` variants.

use loom_package::zip::PackageArchive;
use loom_text::ListMarkerStyle;

use super::package::SANS_FAMILY;
use super::xml::{W_NS, XML_DECLARATION};

/// First `w:numId` allocated to a numbered list run. `1..=5` are reserved for
/// the shared bullet definitions (`Default`, `Disc`, `Circle`, `Square`, `Dash`).
pub(super) const FIRST_NUMBERED_NUM_ID: usize = 6;

pub(super) fn bullet_num_id(marker: ListMarkerStyle) -> usize {
    match marker {
        ListMarkerStyle::Disc => 2,
        ListMarkerStyle::Circle => 3,
        ListMarkerStyle::Square => 4,
        ListMarkerStyle::Dash => 5,
        _ => 1,
    }
}

fn numbered_abstract_num_id(marker: ListMarkerStyle) -> usize {
    match marker {
        ListMarkerStyle::Decimal => 6,
        ListMarkerStyle::LowerAlpha => 7,
        ListMarkerStyle::UpperAlpha => 8,
        ListMarkerStyle::LowerRoman => 9,
        ListMarkerStyle::UpperRoman => 10,
        _ => 1,
    }
}

fn write_bullet_abstract_num(out: &mut String, abs_id: usize, glyph: Option<&str>) {
    let family = SANS_FAMILY;
    out.push_str(&format!(
        "<w:abstractNum w:abstractNumId=\"{abs_id}\"><w:multiLevelType w:val=\"hybridMultilevel\"/>"
    ));
    for level in 0..9u32 {
        let level_glyph = glyph.unwrap_or(if level % 2 == 0 {
            "\u{2022}"
        } else {
            "\u{2013}"
        });
        out.push_str(&format!(
            "<w:lvl w:ilvl=\"{level}\"><w:start w:val=\"1\"/><w:numFmt w:val=\"bullet\"/>\
             <w:lvlText w:val=\"{level_glyph}\"/><w:lvlJc w:val=\"left\"/>\
             <w:pPr><w:ind w:left=\"{}\" w:hanging=\"360\"/></w:pPr>\
             <w:rPr><w:rFonts w:ascii=\"{family}\" w:hAnsi=\"{family}\" w:cs=\"{family}\" w:hint=\"default\"/></w:rPr></w:lvl>",
            720 * (level + 1)
        ));
    }
    out.push_str("</w:abstractNum>");
}

fn write_ordered_abstract_num(out: &mut String, abs_id: usize, fixed_format: Option<&str>) {
    out.push_str(&format!(
        "<w:abstractNum w:abstractNumId=\"{abs_id}\"><w:multiLevelType w:val=\"hybridMultilevel\"/>"
    ));
    for level in 0..9u32 {
        let format = fixed_format
            .unwrap_or_else(|| ["decimal", "lowerLetter", "lowerRoman"][(level % 3) as usize]);
        out.push_str(&format!(
            "<w:lvl w:ilvl=\"{level}\"><w:start w:val=\"1\"/><w:numFmt w:val=\"{format}\"/>\
             <w:lvlText w:val=\"%{}.\"/><w:lvlJc w:val=\"left\"/>\
             <w:pPr><w:ind w:left=\"{}\" w:hanging=\"360\"/></w:pPr></w:lvl>",
            level + 1,
            720 * (level + 1)
        ));
    }
    out.push_str("</w:abstractNum>");
}

/// Generates `word/numbering.xml` with distinct `<w:abstractNum>` definitions for
/// every supported bullet and ordered `ListMarkerStyle`, plus `<w:num>` entries
/// for each bullet style and each numbered list run.
pub(super) fn numbering_xml(numbered_lists: &[ListMarkerStyle]) -> String {
    let mut out = String::from(XML_DECLARATION);
    out.push_str(&format!("<w:numbering xmlns:w=\"{W_NS}\">"));

    write_bullet_abstract_num(&mut out, 0, None);
    write_ordered_abstract_num(&mut out, 1, None);
    write_bullet_abstract_num(&mut out, 2, Some("\u{2022}"));
    write_bullet_abstract_num(&mut out, 3, Some("\u{25e6}"));
    write_bullet_abstract_num(&mut out, 4, Some("\u{25aa}"));
    write_bullet_abstract_num(&mut out, 5, Some("\u{2013}"));
    write_ordered_abstract_num(&mut out, 6, Some("decimal"));
    write_ordered_abstract_num(&mut out, 7, Some("lowerLetter"));
    write_ordered_abstract_num(&mut out, 8, Some("upperLetter"));
    write_ordered_abstract_num(&mut out, 9, Some("lowerRoman"));
    write_ordered_abstract_num(&mut out, 10, Some("upperRoman"));

    out.push_str("<w:num w:numId=\"1\"><w:abstractNumId w:val=\"0\"/></w:num>");
    out.push_str("<w:num w:numId=\"2\"><w:abstractNumId w:val=\"2\"/></w:num>");
    out.push_str("<w:num w:numId=\"3\"><w:abstractNumId w:val=\"3\"/></w:num>");
    out.push_str("<w:num w:numId=\"4\"><w:abstractNumId w:val=\"4\"/></w:num>");
    out.push_str("<w:num w:numId=\"5\"><w:abstractNumId w:val=\"5\"/></w:num>");

    for (index, marker) in numbered_lists.iter().enumerate() {
        let num_id = FIRST_NUMBERED_NUM_ID + index;
        let abs_id = numbered_abstract_num_id(*marker);
        out.push_str(&format!(
            "<w:num w:numId=\"{num_id}\"><w:abstractNumId w:val=\"{abs_id}\"/>\
             <w:lvlOverride w:ilvl=\"0\"><w:startOverride w:val=\"1\"/></w:lvlOverride></w:num>"
        ));
    }

    out.push_str("</w:numbering>");
    out
}

/// Resolves a `w:numId` from `word/numbering.xml` to `(block_kind, ListMarkerStyle)`.
pub(super) fn resolve_num_id_list_style(
    archive: Option<&PackageArchive>,
    num_id: &str,
) -> Option<(&'static str, ListMarkerStyle)> {
    if let Some(num_xml) = archive
        .and_then(|a| a.get("word/numbering.xml"))
        .and_then(|b| std::str::from_utf8(b).ok())
    {
        let num_tag = format!("<w:num w:numId=\"{num_id}\"");
        if let Some(num_pos) = num_xml.find(&num_tag) {
            let num_rest = &num_xml[num_pos..];
            let num_end = num_rest.find("</w:num>").unwrap_or(num_rest.len());
            let num_block = &num_rest[..num_end];
            if let Some(abs_pos) = num_block.find("<w:abstractNumId") {
                if let Some(abs_id) =
                    super::import::extract_attribute_value(&num_block[abs_pos..], "w:val")
                {
                    let abs_tag = format!("<w:abstractNum w:abstractNumId=\"{abs_id}\"");
                    if let Some(abs_start) = num_xml.find(&abs_tag) {
                        let abs_rest = &num_xml[abs_start..];
                        let abs_end = abs_rest.find("</w:abstractNum>").unwrap_or(abs_rest.len());
                        let abs_block = &abs_rest[..abs_end];
                        if let Some(fmt_pos) = abs_block.find("<w:numFmt") {
                            if let Some(fmt_val) = super::import::extract_attribute_value(
                                &abs_block[fmt_pos..],
                                "w:val",
                            ) {
                                return match fmt_val.as_str() {
                                    "none" => None,
                                    "bullet" => {
                                        let marker = abs_block
                                            .find("<w:lvlText")
                                            .and_then(|p| {
                                                super::import::extract_attribute_value(
                                                    &abs_block[p..],
                                                    "w:val",
                                                )
                                            })
                                            .map(|t| match t.as_str() {
                                                "\u{2022}" => ListMarkerStyle::Disc,
                                                "\u{25e6}" => ListMarkerStyle::Circle,
                                                "\u{25aa}" => ListMarkerStyle::Square,
                                                "\u{2013}" | "-" => ListMarkerStyle::Dash,
                                                _ => ListMarkerStyle::Default,
                                            })
                                            .unwrap_or(ListMarkerStyle::Default);
                                        Some(("list-bulleted", marker))
                                    }
                                    "decimal" => Some(("list-numbered", ListMarkerStyle::Decimal)),
                                    "lowerLetter" => {
                                        Some(("list-numbered", ListMarkerStyle::LowerAlpha))
                                    }
                                    "upperLetter" => {
                                        Some(("list-numbered", ListMarkerStyle::UpperAlpha))
                                    }
                                    "lowerRoman" => {
                                        Some(("list-numbered", ListMarkerStyle::LowerRoman))
                                    }
                                    "upperRoman" => {
                                        Some(("list-numbered", ListMarkerStyle::UpperRoman))
                                    }
                                    _ => Some(("list-numbered", ListMarkerStyle::Default)),
                                };
                            }
                        }
                    }
                }
            }
        }
    }
    if num_id == "1" {
        Some(("list-bulleted", ListMarkerStyle::Default))
    } else {
        Some(("list-numbered", ListMarkerStyle::Default))
    }
}
