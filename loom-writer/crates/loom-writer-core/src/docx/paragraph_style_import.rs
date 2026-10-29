//! Paragraph properties represented by Writer's paragraph style model.

use loom_text::{Alignment, ParagraphDirection, ParagraphStyle};

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
