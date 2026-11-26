//! `word/numbering.xml` generation and import resolution for all Writer
//! `ListMarkerStyle` variants.

use loom_package::zip::PackageArchive;
use loom_text::ListMarkerStyle;

use super::package::SANS_FAMILY;
use super::xml::{W_NS, XML_DECLARATION};

/// First `w:numId` allocated to a numbered list run. `1..=5` are reserved for
/// the shared bullet definitions (`Default`, `Disc`, `Circle`, `Square`, `Dash`).
pub(super) const FIRST_NUMBERED_NUM_ID: usize = 6;

/// Definition of a single exported numbered list run, including any nested
/// levels whose marker style differs from the level-0 base style.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct NumberedListDef {
    pub(super) base: ListMarkerStyle,
    pub(super) level_overrides: Vec<(u8, ListMarkerStyle, u32)>,
    pub(super) start_override: Option<(u8, u32)>,
}

fn ordered_num_fmt(marker: ListMarkerStyle) -> Option<&'static str> {
    match marker {
        ListMarkerStyle::Decimal => Some("decimal"),
        ListMarkerStyle::LowerAlpha => Some("lowerLetter"),
        ListMarkerStyle::UpperAlpha => Some("upperLetter"),
        ListMarkerStyle::LowerRoman => Some("lowerRoman"),
        ListMarkerStyle::UpperRoman => Some("upperRoman"),
        _ => None,
    }
}

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
pub(super) fn numbering_xml(numbered_lists: &[NumberedListDef]) -> String {
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

    for (index, def) in numbered_lists.iter().enumerate() {
        let num_id = FIRST_NUMBERED_NUM_ID + index;
        let abs_id = numbered_abstract_num_id(def.base);
        out.push_str(&format!(
            "<w:num w:numId=\"{num_id}\"><w:abstractNumId w:val=\"{abs_id}\"/>\
             <w:lvlOverride w:ilvl=\"0\"><w:startOverride w:val=\"1\"/></w:lvlOverride>"
        ));
        for &(lvl, marker, start) in &def.level_overrides {
            if let Some(fmt) = ordered_num_fmt(marker) {
                let start_override = if start > 1 {
                    format!("<w:startOverride w:val=\"{start}\"/>")
                } else {
                    String::new()
                };
                out.push_str(&format!(
                    "<w:lvlOverride w:ilvl=\"{lvl}\">{start_override}\
                     <w:lvl w:ilvl=\"{lvl}\"><w:start w:val=\"{start}\"/>\
                     <w:numFmt w:val=\"{fmt}\"/><w:lvlText w:val=\"%{}.\"/><w:lvlJc w:val=\"left\"/>\
                     <w:pPr><w:ind w:left=\"{}\" w:hanging=\"360\"/></w:pPr></w:lvl></w:lvlOverride>",
                    u32::from(lvl) + 1,
                    720 * (u32::from(lvl) + 1)
                ));
            }
        }
        if let Some((lvl, start)) = def.start_override {
            if start > 1 && !def.level_overrides.iter().any(|(l, _, _)| *l == lvl) {
                out.push_str(&format!(
                    "<w:lvlOverride w:ilvl=\"{lvl}\"><w:startOverride w:val=\"{start}\"/></w:lvlOverride>"
                ));
            }
        }
        out.push_str("</w:num>");
    }

    out.push_str("</w:numbering>");
    out
}

fn find_lvl_block(xml: &str, ilvl: u8) -> Option<&str> {
    let tag = format!("<w:lvl w:ilvl=\"{ilvl}\"");
    let start = xml.find(&tag)?;
    let rest = &xml[start..];
    let end = rest.find("</w:lvl>").unwrap_or(rest.len());
    Some(&rest[..end])
}

fn find_lvl_override_block(num_block: &str, ilvl: u8) -> Option<&str> {
    let tag = format!("<w:lvlOverride w:ilvl=\"{ilvl}\"");
    let start = num_block.find(&tag)?;
    let rest = &num_block[start..];
    let end = rest.find("</w:lvlOverride>").unwrap_or(rest.len());
    Some(&rest[..end])
}

fn is_canonical_default_bullet(abs_block: &str) -> bool {
    let lvl0 = find_lvl_block(abs_block, 0).and_then(|b| {
        b.find("<w:lvlText")
            .and_then(|p| super::import::extract_attribute_value(&b[p..], "w:val"))
    });
    let lvl1 = find_lvl_block(abs_block, 1).and_then(|b| {
        b.find("<w:lvlText")
            .and_then(|p| super::import::extract_attribute_value(&b[p..], "w:val"))
    });
    lvl0.as_deref() == Some("\u{2022}") && lvl1.as_deref() == Some("\u{2013}")
}

fn is_canonical_default_ordered(abs_block: &str) -> bool {
    let fmt = |lvl: u8| {
        find_lvl_block(abs_block, lvl).and_then(|b| {
            b.find("<w:numFmt")
                .and_then(|p| super::import::extract_attribute_value(&b[p..], "w:val"))
        })
    };
    fmt(0).as_deref() == Some("decimal")
        && fmt(1).as_deref() == Some("lowerLetter")
        && fmt(2).as_deref() == Some("lowerRoman")
}

/// Resolves a `w:numId` (and paragraph `w:ilvl`) from `word/numbering.xml` to
/// `(block_kind, ListMarkerStyle)`.
pub(super) fn resolve_num_id_list_style(
    archive: Option<&PackageArchive>,
    num_id: &str,
    ilvl: u8,
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
            let override_block = find_lvl_override_block(num_block, ilvl);
            if let Some(abs_pos) = num_block.find("<w:abstractNumId") {
                if let Some(abs_id) =
                    super::import::extract_attribute_value(&num_block[abs_pos..], "w:val")
                {
                    let abs_tag = format!("<w:abstractNum w:abstractNumId=\"{abs_id}\"");
                    if let Some(abs_start) = num_xml.find(&abs_tag) {
                        let abs_rest = &num_xml[abs_start..];
                        let abs_end = abs_rest.find("</w:abstractNum>").unwrap_or(abs_rest.len());
                        let abs_block = &abs_rest[..abs_end];
                        let override_lvl = override_block.and_then(|ov| find_lvl_block(ov, ilvl));
                        let target_block = override_lvl
                            .or_else(|| find_lvl_block(abs_block, ilvl))
                            .unwrap_or(abs_block);
                        if let Some(fmt_pos) = target_block.find("<w:numFmt") {
                            if let Some(fmt_val) = super::import::extract_attribute_value(
                                &target_block[fmt_pos..],
                                "w:val",
                            ) {
                                return match fmt_val.as_str() {
                                    "none" => None,
                                    "bullet" => {
                                        let marker = if override_lvl.is_none()
                                            && is_canonical_default_bullet(abs_block)
                                        {
                                            ListMarkerStyle::Default
                                        } else {
                                            target_block
                                                .find("<w:lvlText")
                                                .and_then(|p| {
                                                    super::import::extract_attribute_value(
                                                        &target_block[p..],
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
                                                .unwrap_or(ListMarkerStyle::Default)
                                        };
                                        Some(("list-bulleted", marker))
                                    }
                                    _ if override_lvl.is_none()
                                        && is_canonical_default_ordered(abs_block) =>
                                    {
                                        Some(("list-numbered", ListMarkerStyle::Default))
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

/// Resolves `<w:numPr>` inherited from a paragraph style (`word/styles.xml`),
/// following `<w:basedOn>` chains up to 16 hops.
pub(super) fn resolve_style_num_pr(
    archive: &PackageArchive,
    style_id: &str,
) -> Option<(String, u8)> {
    let styles_bytes = archive.get("word/styles.xml")?;
    let styles_xml = std::str::from_utf8(styles_bytes).ok()?;
    let mut current_id = style_id.to_string();
    for _ in 0..16 {
        let mut cursor = 0usize;
        let mut matched_block: Option<&str> = None;
        while let Some(rel) = styles_xml[cursor..].find("<w:style ") {
            let start = cursor + rel;
            let end = styles_xml[start..]
                .find("</w:style>")
                .map_or(styles_xml.len(), |e| start + e + "</w:style>".len());
            let block = &styles_xml[start..end];
            if super::import::extract_attribute_value(block, "w:styleId").as_deref()
                == Some(current_id.as_str())
            {
                matched_block = Some(block);
                break;
            }
            cursor = end;
        }
        let block = matched_block?;
        if let Some(num_start) = block.find("<w:numPr") {
            let num_end = block[num_start..]
                .find("</w:numPr>")
                .map_or(block.len(), |e| num_start + e);
            let snippet = &block[num_start..num_end];
            if let Some(id_pos) = snippet.find("<w:numId") {
                if let Some(num_id) =
                    super::import::extract_attribute_value(&snippet[id_pos..], "w:val")
                {
                    if num_id == "0" {
                        return None;
                    }
                    let ilvl = snippet
                        .find("<w:ilvl")
                        .and_then(|p| {
                            super::import::extract_attribute_value(&snippet[p..], "w:val")
                        })
                        .map_or(0, |v| crate::list_levels::parse_list_level(&v));
                    return Some((num_id, ilvl));
                }
            }
        }
        let based_pos = block.find("<w:basedOn")?;
        current_id = super::import::extract_attribute_value(&block[based_pos..], "w:val")?;
    }
    None
}
