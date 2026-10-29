//! Paragraph properties represented by Writer's paragraph style model.

use loom_package::zip::PackageArchive;
use loom_text::{Alignment, ParagraphDirection, ParagraphStyle};
use roxmltree::Node;

use super::inline_style::{attribute_value, property_tag};

pub(super) fn paragraph_style_from_docx(p_xml: &str) -> Result<ParagraphStyle, String> {
    let mut style = ParagraphStyle::default();
    if let Some(tag) = property_tag(p_xml, "w:jc") {
        if let Some(value) = attribute_value(tag, "w:val") {
            style.alignment = match value.to_ascii_lowercase().as_str() {
                "start" => Alignment::Start,
                "end" => Alignment::End,
                "left" => Alignment::Left,
                "center" => Alignment::Center,
                "right" => Alignment::Right,
                "both" => Alignment::Justify,
                _ => style.alignment,
            };
        }
    }
    if let Some(tag) = property_tag(p_xml, "w:bidi") {
        style.direction = match attribute_value(tag, "w:val")
            .as_deref()
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            None | Some("true" | "1" | "on") => ParagraphDirection::RightToLeft,
            Some("false" | "0" | "off") => ParagraphDirection::LeftToRight,
            Some(_) => ParagraphDirection::Auto,
        };
    }

    if let Some(tag) = property_tag(p_xml, "w:spacing") {
        for name in ["beforeLines", "afterLines"] {
            if attribute_value(tag, &format!("w:{name}")).is_some() {
                return Err(format!("DOCX paragraph spacing w:{name} is not supported"));
            }
        }
        for name in ["beforeAutospacing", "afterAutospacing"] {
            if let Some(value) = attribute_value(tag, &format!("w:{name}")) {
                match value.to_ascii_lowercase().as_str() {
                    "0" | "false" | "off" => {}
                    "1" | "true" | "on" => {
                        return Err(format!(
                            "automatic DOCX paragraph spacing w:{name} is not supported"
                        ));
                    }
                    _ => return Err(format!("DOCX paragraph w:{name} must be a boolean")),
                }
            }
        }
        style.space_before = twips_points(tag, "before")?.unwrap_or(style.space_before);
        style.space_after = twips_points(tag, "after")?.unwrap_or(style.space_after);
        let line_rule = attribute_value(tag, "w:lineRule");
        if line_rule
            .as_deref()
            .is_some_and(|rule| !rule.eq_ignore_ascii_case("auto"))
        {
            return Err("DOCX exact or minimum line spacing is not supported".into());
        }
        if let Some(line) = twips_value(tag, "line")? {
            if line == 0 {
                return Err("DOCX line spacing must be positive".into());
            }
            style.line_spacing = line as f32 / 240.0;
        }
    }

    if let Some(tag) = property_tag(p_xml, "w:ind") {
        for name in ["start", "end"] {
            if attribute_value(tag, &format!("w:{name}")).is_some() {
                return Err(format!(
                    "logical DOCX paragraph indent w:{name} is not supported"
                ));
            }
        }
        let left = twips_value(tag, "left")?;
        let right = twips_points(tag, "right")?;
        let first_line = twips_points(tag, "firstLine")?;
        let hanging = twips_points(tag, "hanging")?;
        if first_line.is_some() && hanging.is_some() {
            return Err("DOCX paragraph indent cannot set both firstLine and hanging".into());
        }
        let list_style = property_tag(p_xml, "w:pStyle")
            .and_then(|tag| attribute_value(tag, "w:val"))
            .map(|value| value.to_ascii_lowercase())
            .is_some_and(|value| {
                value == "listparagraph" || value == "bullet" || value.contains("list")
            });
        let is_list = property_tag(p_xml, "w:numPr").is_some() || list_style;
        if is_list {
            let level = property_tag(p_xml, "w:ilvl")
                .and_then(|tag| attribute_value(tag, "w:val"))
                .and_then(|value| value.parse::<u8>().ok())
                .unwrap_or(0)
                .min(crate::list_levels::MAX_LIST_LEVEL);
            if first_line.is_some() || hanging.is_some_and(|value| value != 18.0) {
                return Err("DOCX custom list marker indents are not supported".into());
            }
            let base_left = 720 + i32::from(level) * 360;
            if let Some(left) = left {
                if left < base_left {
                    return Err("DOCX list indent is below the supported marker indent".into());
                }
                style.left_indent = (left - base_left) as f32 / 20.0;
            } else if hanging.is_some() {
                return Err("DOCX hanging list indent is missing its left indent".into());
            }
        } else {
            style.left_indent = left.unwrap_or(0) as f32 / 20.0;
            style.first_line_indent = first_line.unwrap_or(0.0) - hanging.unwrap_or(0.0);
        }
        style.right_indent = right.unwrap_or(0.0);
    }
    Ok(style)
}

pub(super) fn has_unsupported_table_layout(style: &ParagraphStyle) -> bool {
    let defaults = ParagraphStyle::default();
    style.space_before != defaults.space_before
        || style.space_after != defaults.space_after
        || style.line_spacing != defaults.line_spacing
        || style.left_indent != defaults.left_indent
        || style.right_indent != defaults.right_indent
        || style.first_line_indent != defaults.first_line_indent
}

fn twips_value(tag: &str, name: &str) -> Result<Option<i32>, String> {
    let Some(raw) = attribute_value(tag, &format!("w:{name}")) else {
        return Ok(None);
    };
    let value = raw
        .parse::<i32>()
        .map_err(|_| format!("DOCX paragraph {name} must be an integer number of twips"))?;
    if !(0..=31_680).contains(&value) {
        return Err(format!(
            "DOCX paragraph {name} is outside the supported twip range"
        ));
    }
    Ok(Some(value))
}

fn twips_points(tag: &str, name: &str) -> Result<Option<f32>, String> {
    twips_value(tag, name).map(|value| value.map(|twips| twips as f32 / 20.0))
}

pub(super) struct ParagraphProperties {
    pub current_xml: String,
    pub previous_xml: Option<String>,
    pub revision_id: Option<u64>,
    pub revision_author: Option<String>,
}

pub(super) fn extract_paragraph_properties(
    paragraph_xml: &str,
    root_tag: &str,
) -> Result<ParagraphProperties, String> {
    let wrapped = format!("{root_tag}<w:body>{paragraph_xml}</w:body></w:document>");
    let document = roxmltree::Document::parse(&wrapped)
        .map_err(|error| format!("invalid DOCX paragraph XML: {error}"))?;
    let paragraph = document
        .root_element()
        .descendants()
        .find(|node| node.has_tag_name((super::xml::W_NS, "p")))
        .ok_or_else(|| "DOCX paragraph element is missing".to_string())?;
    let mut properties = paragraph
        .children()
        .filter(|node| node.has_tag_name((super::xml::W_NS, "pPr")));
    let Some(ppr) = properties.next() else {
        return Ok(ParagraphProperties {
            current_xml: "<w:pPr/>".into(),
            previous_xml: None,
            revision_id: None,
            revision_author: None,
        });
    };
    if properties.next().is_some() {
        return Err("DOCX paragraph has duplicate w:pPr elements".into());
    }

    let mut changes = ppr
        .children()
        .filter(|node| node.has_tag_name((super::xml::W_NS, "pPrChange")));
    let change = changes.next();
    if changes.next().is_some() {
        return Err("DOCX paragraph has multiple w:pPrChange revisions".into());
    }
    if change.is_some() && ppr.attributes().next().is_some() {
        return Err("DOCX paragraph properties have unsupported attributes".into());
    }
    let prefix_len = root_tag.len() + "<w:body>".len();
    let current_properties = ppr
        .children()
        .filter(Node::is_element)
        .filter(|node| !node.has_tag_name((super::xml::W_NS, "pPrChange")))
        .map(|node| {
            let range = node.range();
            let start = range
                .start
                .checked_sub(prefix_len)
                .ok_or_else(|| "invalid DOCX paragraph-property location".to_string())?;
            let end = range
                .end
                .checked_sub(prefix_len)
                .ok_or_else(|| "invalid DOCX paragraph-property location".to_string())?;
            paragraph_xml
                .get(start..end)
                .map(str::to_owned)
                .ok_or_else(|| "invalid DOCX paragraph-property boundary".to_string())
        })
        .collect::<Result<String, String>>()?;

    let Some(change) = change else {
        return Ok(ParagraphProperties {
            current_xml: format!("<w:pPr>{current_properties}</w:pPr>"),
            previous_xml: None,
            revision_id: None,
            revision_author: None,
        });
    };
    validate_property_attributes(change, "pPrChange", &["id", "author"])?;
    if change
        .children()
        .filter(Node::is_text)
        .any(|node| node.text().is_some_and(|text| !text.trim().is_empty()))
    {
        return Err("DOCX w:pPrChange contains unsupported text".into());
    }
    let mut previous_properties = change
        .children()
        .filter(|node| node.has_tag_name((super::xml::W_NS, "pPr")));
    let Some(previous_ppr) = previous_properties.next() else {
        return Err("DOCX w:pPrChange is missing its previous w:pPr".into());
    };
    if previous_properties.next().is_some()
        || previous_ppr
            .descendants()
            .any(|node| node.has_tag_name((super::xml::W_NS, "pPrChange")))
        || change
            .children()
            .filter(Node::is_element)
            .any(|node| !node.has_tag_name((super::xml::W_NS, "pPr")))
    {
        return Err("DOCX nested or duplicate paragraph-property revisions are unsupported".into());
    }
    let id = change
        .attribute((super::xml::W_NS, "id"))
        .ok_or_else(|| "DOCX pPrChange is missing w:id".to_string())?
        .parse::<u64>()
        .map_err(|_| "DOCX pPrChange has an invalid w:id".to_string())?;
    let author = change
        .attribute((super::xml::W_NS, "author"))
        .ok_or_else(|| "DOCX pPrChange is missing w:author".to_string())?
        .to_owned();
    let range = previous_ppr.range();
    let start = range
        .start
        .checked_sub(prefix_len)
        .ok_or_else(|| "invalid DOCX previous paragraph-property location".to_string())?;
    let end = range
        .end
        .checked_sub(prefix_len)
        .ok_or_else(|| "invalid DOCX previous paragraph-property location".to_string())?;
    let previous_xml = paragraph_xml
        .get(start..end)
        .ok_or_else(|| "invalid DOCX previous paragraph-property boundary".to_string())?
        .to_owned();

    Ok(ParagraphProperties {
        current_xml: format!("<w:pPr>{current_properties}</w:pPr>"),
        previous_xml: Some(previous_xml),
        revision_id: Some(id),
        revision_author: Some(author),
    })
}

pub(super) fn validate_tracked_paragraph_properties(
    ppr_xml: &str,
    root_tag: &str,
    allow_paragraph_mark_revisions: bool,
) -> Result<(), String> {
    let wrapped = format!("{root_tag}<w:body><w:p>{ppr_xml}</w:p></w:body></w:document>");
    let document = roxmltree::Document::parse(&wrapped)
        .map_err(|error| format!("invalid DOCX paragraph properties: {error}"))?;
    let ppr = document
        .root_element()
        .descendants()
        .find(|node| node.has_tag_name((super::xml::W_NS, "pPr")))
        .ok_or_else(|| "DOCX paragraph-property change is missing w:pPr".to_string())?;
    if ppr.attributes().next().is_some() {
        return Err("tracked DOCX paragraph properties have unsupported attributes".into());
    }
    for property in ppr.children().filter(Node::is_element) {
        let (name, allowed_attributes): (&str, &[&str]) = match property.tag_name().name() {
            "pStyle" => ("pStyle", &["val"]),
            "bidi" => ("bidi", &["val"]),
            "spacing" => ("spacing", &["before", "after", "line", "lineRule"]),
            "ind" => ("ind", &["left", "right", "firstLine", "hanging"]),
            "jc" => ("jc", &["val"]),
            "rPr" if allow_paragraph_mark_revisions => {
                validate_paragraph_mark_properties(property)?;
                continue;
            }
            "numPr" => {
                return Err(
                    "tracked paragraph formatting revisions with list mappings are unsupported"
                        .into(),
                );
            }
            other => {
                return Err(format!(
                    "tracked DOCX paragraph property w:{other} is not supported"
                ));
            }
        };
        validate_property_attributes(property, name, allowed_attributes)?;
        if name == "bidi" {
            let value = property.attribute((super::xml::W_NS, "val"));
            if value.is_some_and(|value| {
                !["true", "1", "on", "false", "0", "off"]
                    .iter()
                    .any(|allowed| value.eq_ignore_ascii_case(allowed))
            }) {
                return Err("tracked DOCX paragraph bidi value is not supported".into());
            }
        } else if name == "jc" {
            let value = property.attribute((super::xml::W_NS, "val"));
            if value.is_none_or(|value| {
                !["start", "end", "left", "center", "right", "both"]
                    .iter()
                    .any(|allowed| value.eq_ignore_ascii_case(allowed))
            }) {
                return Err("tracked DOCX paragraph alignment is not supported".into());
            }
        }
    }
    Ok(())
}

fn validate_property_attributes(
    node: Node<'_, '_>,
    property: &str,
    allowed: &[&str],
) -> Result<(), String> {
    for attribute in node.attributes() {
        if attribute.namespace() != Some(super::xml::W_NS) || !allowed.contains(&attribute.name()) {
            return Err(format!(
                "tracked DOCX paragraph property w:{property} has an unsupported attribute"
            ));
        }
    }
    Ok(())
}

fn validate_paragraph_mark_properties(node: Node<'_, '_>) -> Result<(), String> {
    if node.attributes().next().is_some() {
        return Err("tracked DOCX paragraph-mark properties are unsupported".into());
    }
    for revision in node.children().filter(Node::is_element) {
        if !revision.has_tag_name((super::xml::W_NS, "ins"))
            && !revision.has_tag_name((super::xml::W_NS, "del"))
        {
            return Err("tracked DOCX paragraph-mark formatting is unsupported".into());
        }
        validate_property_attributes(revision, revision.tag_name().name(), &["id", "author"])?;
    }
    Ok(())
}

pub(super) struct LocalParagraphFormatChange {
    pub(super) id: u64,
    pub(super) author: String,
    pub(super) before: crate::revisions::RevisionBlockFormat,
    pub(super) after: crate::revisions::RevisionBlockFormat,
}

pub(super) fn paragraph_kind_from_docx(
    ppr_xml: &str,
    archive: Option<&PackageArchive>,
    strict: bool,
) -> Result<String, String> {
    let mut kind = "paragraph".to_string();
    if let Some(style_start) = ppr_xml.find("<w:pStyle") {
        let Some(value) = super::import::extract_attribute_value(&ppr_xml[style_start..], "w:val")
        else {
            if strict {
                return Err("tracked DOCX paragraph style is missing w:val".into());
            }
            return Ok(kind);
        };
        let value_lower = value.to_ascii_lowercase();
        if strict {
            if value_lower == "normal" {
                kind = "paragraph".into();
            } else if let Some(level) = value_lower.strip_prefix("heading") {
                if matches!(level, "1" | "2" | "3" | "4" | "5" | "6") {
                    kind = format!("heading{level}");
                } else {
                    return Err(
                        "tracked DOCX paragraph style is not an exact heading mapping".into(),
                    );
                }
            } else {
                return Err(
                    "tracked DOCX paragraph style is not an exact paragraph or heading mapping"
                        .into(),
                );
            }
        } else if value_lower.starts_with("heading") {
            let num_str: String = value_lower.chars().filter(|c| c.is_ascii_digit()).collect();
            if let Ok(level @ 1..=6) = num_str.parse::<u8>() {
                kind = format!("heading{level}");
            }
        } else if value_lower == "title" {
            kind = "heading1".to_string();
        } else if value_lower == "subtitle" {
            kind = "heading2".to_string();
        } else if value_lower == "listparagraph"
            || value_lower == "bullet"
            || value_lower.contains("list")
        {
            kind = "list-bulleted".to_string();
        }
    }

    if let Some(num_start) = ppr_xml.find("<w:numPr") {
        if strict {
            return Err(
                "tracked paragraph formatting revisions with list mappings are unsupported".into(),
            );
        }
        let num_end = ppr_xml[num_start..]
            .find("</w:numPr>")
            .unwrap_or(ppr_xml.len() - num_start);
        let num_snippet = &ppr_xml[num_start..num_start + num_end];
        if let Some(num_id_pos) = num_snippet.find("<w:numId") {
            if let Some(num_id_val) =
                super::import::extract_attribute_value(&num_snippet[num_id_pos..], "w:val")
            {
                if let Some(list_kind) =
                    super::import::resolve_num_id_list_kind(archive, &num_id_val)
                {
                    kind = list_kind.to_string();
                }
            } else {
                kind = "list-bulleted".to_string();
            }
        } else {
            kind = "list-bulleted".to_string();
        }
    }
    if strict && crate::list_levels::is_list_kind(&kind) {
        return Err(
            "tracked paragraph formatting revisions with list mappings are unsupported".into(),
        );
    }
    Ok(kind)
}

pub(super) fn parse_tracked_paragraph_format_change(
    properties: &ParagraphProperties,
    root_tag: &str,
    archive: Option<&PackageArchive>,
    current_kind: &str,
    current_style: &ParagraphStyle,
) -> Result<Option<LocalParagraphFormatChange>, String> {
    let Some(previous_xml) = properties.previous_xml.as_deref() else {
        return Ok(None);
    };
    validate_tracked_paragraph_properties(&properties.current_xml, root_tag, true)?;
    validate_tracked_paragraph_properties(previous_xml, root_tag, false)?;
    let parsed_current_kind = paragraph_kind_from_docx(&properties.current_xml, archive, true)?;
    if parsed_current_kind != current_kind {
        return Err("tracked DOCX paragraph style is not represented by the live paragraph".into());
    }
    let before_kind = paragraph_kind_from_docx(previous_xml, archive, true)?;
    let before_style = paragraph_style_from_docx(previous_xml)?;
    let id = properties
        .revision_id
        .ok_or_else(|| "DOCX pPrChange is missing its revision ID".to_string())?;
    let author = properties
        .revision_author
        .clone()
        .ok_or_else(|| "DOCX pPrChange is missing its author".to_string())?;
    Ok(Some(LocalParagraphFormatChange {
        id,
        author,
        before: crate::revisions::RevisionBlockFormat {
            kind: before_kind,
            style: before_style,
        },
        after: crate::revisions::RevisionBlockFormat {
            kind: current_kind.to_owned(),
            style: current_style.clone(),
        },
    }))
}
