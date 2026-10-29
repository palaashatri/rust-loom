//! Names and JSON encodings for paragraph/character styles and selections.
//!
//! Kept apart from the document model so `lib.rs` stays a module boundary.

use crate::TextSelection;
use loom_package::manifest::json as pkg_json;
use loom_text::ParagraphDirection;

use super::{
    json_value_text, parse_f32_value, ContentParser, JsonValue, WriterDocument, WriterError,
};

pub(crate) fn alignment_name(alignment: loom_text::Alignment) -> &'static str {
    match alignment {
        loom_text::Alignment::Start => "start",
        loom_text::Alignment::Left => "left",
        loom_text::Alignment::End => "end",
        loom_text::Alignment::Center => "center",
        loom_text::Alignment::Right => "right",
        loom_text::Alignment::Justify => "justify",
    }
}

pub(crate) fn parse_alignment(value: &str) -> loom_text::Alignment {
    match value {
        "start" => loom_text::Alignment::Start,
        "end" => loom_text::Alignment::End,
        "left" => loom_text::Alignment::Left,
        "center" => loom_text::Alignment::Center,
        "right" => loom_text::Alignment::Right,
        "justify" => loom_text::Alignment::Justify,
        _ => loom_text::Alignment::Left,
    }
}

fn direction_name(direction: ParagraphDirection) -> &'static str {
    match direction {
        ParagraphDirection::Auto => "auto",
        ParagraphDirection::LeftToRight => "ltr",
        ParagraphDirection::RightToLeft => "rtl",
    }
}

fn parse_direction(value: &str) -> Option<ParagraphDirection> {
    match value {
        "auto" => Some(ParagraphDirection::Auto),
        "ltr" => Some(ParagraphDirection::LeftToRight),
        "rtl" => Some(ParagraphDirection::RightToLeft),
        _ => None,
    }
}

fn line_break_name(rule: loom_text::LineBreakRule) -> &'static str {
    match rule {
        loom_text::LineBreakRule::NoBreak => "no-break",
        loom_text::LineBreakRule::Word => "word",
        loom_text::LineBreakRule::Char => "char",
    }
}

pub(crate) fn parse_line_break(value: &str) -> loom_text::LineBreakRule {
    match value {
        "no-break" => loom_text::LineBreakRule::NoBreak,
        "char" => loom_text::LineBreakRule::Char,
        _ => loom_text::LineBreakRule::Word,
    }
}

fn font_weight_name(weight: loom_text::FontWeight) -> &'static str {
    match weight {
        loom_text::FontWeight::Thin => "thin",
        loom_text::FontWeight::Light => "light",
        loom_text::FontWeight::Regular => "regular",
        loom_text::FontWeight::Medium => "medium",
        loom_text::FontWeight::Semibold => "semibold",
        loom_text::FontWeight::Bold => "bold",
        loom_text::FontWeight::Black => "black",
    }
}

pub(crate) fn parse_font_weight(value: &str) -> loom_text::FontWeight {
    match value {
        "thin" => loom_text::FontWeight::Thin,
        "light" => loom_text::FontWeight::Light,
        "medium" => loom_text::FontWeight::Medium,
        "semibold" => loom_text::FontWeight::Semibold,
        "bold" => loom_text::FontWeight::Bold,
        "black" => loom_text::FontWeight::Black,
        _ => loom_text::FontWeight::Regular,
    }
}

fn json_f32(value: f32) -> String {
    if value.is_finite() {
        value.to_string()
    } else {
        // JSON has no NaN or infinity. Styles normally contain finite values,
        // but keeping the content writer valid is safer than emitting invalid
        // package data if a caller supplies a non-finite value.
        "0.0".into()
    }
}

fn json_bool(value: bool) -> &'static str {
    if value {
        "true"
    } else {
        "false"
    }
}

fn push_optional_json_string(out: &mut String, value: Option<&str>) {
    match value {
        Some(value) => out.push_str(&pkg_json::escape(value)),
        None => out.push_str("null"),
    }
}

pub(crate) fn paragraph_style_json(style: &loom_text::ParagraphStyle) -> String {
    let mut out = String::from("{\"alignment\":");
    out.push_str(&pkg_json::escape(alignment_name(style.alignment)));
    out.push_str(",\"direction\":");
    out.push_str(&pkg_json::escape(direction_name(style.direction)));
    out.push_str(",\"space_before\":");
    out.push_str(&json_f32(style.space_before));
    out.push_str(",\"space_after\":");
    out.push_str(&json_f32(style.space_after));
    out.push_str(",\"line_spacing\":");
    out.push_str(&json_f32(style.line_spacing));
    out.push_str(",\"left_indent\":");
    out.push_str(&json_f32(style.left_indent));
    out.push_str(",\"right_indent\":");
    out.push_str(&json_f32(style.right_indent));
    out.push_str(",\"first_line_indent\":");
    out.push_str(&json_f32(style.first_line_indent));
    out.push_str(",\"break_rule\":");
    out.push_str(&pkg_json::escape(line_break_name(style.break_rule)));
    // Written only for a nested item, so files without nesting are unchanged.
    if style.list_level > 0 {
        out.push_str(",\"list_level\":");
        out.push_str(&style.list_level.to_string());
    }
    out.push('}');
    out
}

pub(crate) fn parse_paragraph_style(
    raw: &str,
    mut style: loom_text::ParagraphStyle,
) -> Result<loom_text::ParagraphStyle, WriterError> {
    let fields = ContentParser::new(raw).parse()?;
    for (key, value) in &fields {
        match key.as_str() {
            "alignment" | "align" => {
                style.alignment = parse_alignment(&json_value_text(value));
            }
            "direction" => {
                style.direction = parse_direction(&json_value_text(value))
                    .ok_or_else(|| WriterError::Invalid("unknown paragraph direction".into()))?;
            }
            "space_before" => parse_f32_value(value, &mut style.space_before),
            "space_after" => parse_f32_value(value, &mut style.space_after),
            "line_spacing" => parse_f32_value(value, &mut style.line_spacing),
            "left_indent" => parse_f32_value(value, &mut style.left_indent),
            "right_indent" => parse_f32_value(value, &mut style.right_indent),
            "first_line_indent" => parse_f32_value(value, &mut style.first_line_indent),
            "break_rule" => {
                style.break_rule = parse_line_break(&json_value_text(value));
            }
            "list_level" => {
                style.list_level = super::list_levels::parse_list_level(&json_value_text(value));
            }
            _ => {}
        }
    }
    Ok(style)
}

const INVALID_NATIVE_TEXT_COLOR: &str = "invalid native text color; expected #RRGGBB or #RRGGBBAA";

fn optional_color(value: &JsonValue) -> Result<Option<String>, WriterError> {
    match value {
        JsonValue::String(value) => Ok(Some(value.clone())),
        JsonValue::Raw(value) if value == "null" => Ok(None),
        _ => Err(WriterError::Invalid(INVALID_NATIVE_TEXT_COLOR.into())),
    }
}

fn parse_bool_value(value: &JsonValue, target: &mut bool) {
    if let JsonValue::Raw(value) = value {
        match value.as_str() {
            "true" => *target = true,
            "false" => *target = false,
            _ => {}
        }
    }
}

pub(crate) fn parse_character_style(
    raw: &str,
    mut style: loom_text::CharacterStyle,
) -> Result<loom_text::CharacterStyle, WriterError> {
    let fields = ContentParser::new(raw).parse()?;
    for (key, value) in &fields {
        match key.as_str() {
            "font_family" => {
                if let JsonValue::String(value) = value {
                    style.font_family = value.clone();
                }
            }
            "font_size" => parse_f32_value(value, &mut style.font_size),
            "weight" => style.weight = parse_font_weight(&json_value_text(value)),
            "italic" => parse_bool_value(value, &mut style.italic),
            "underline" => parse_bool_value(value, &mut style.underline),
            "strikethrough" => parse_bool_value(value, &mut style.strikethrough),
            "superscript" => parse_bool_value(value, &mut style.superscript),
            "subscript" => parse_bool_value(value, &mut style.subscript),
            "color" => style.color = optional_color(value)?,
            "background" => style.background = optional_color(value)?,
            "explicit_size" => parse_bool_value(value, &mut style.explicit_size),
            _ => {}
        }
    }
    Ok(style)
}

fn validate_color(color: Option<&str>) -> Result<(), &'static str> {
    if color.is_some() && crate::layout::parse_rgba(color).is_none() {
        Err(INVALID_NATIVE_TEXT_COLOR)
    } else {
        Ok(())
    }
}

pub(crate) fn validate_character_style(
    style: &loom_text::CharacterStyle,
) -> Result<(), &'static str> {
    if !style.font_size.is_finite() {
        return Err("invalid character style");
    }
    validate_color(style.color.as_deref())?;
    validate_color(style.background.as_deref())
}

pub(crate) fn validate_table_text_colors(table: &loom_objects::Table) -> Result<(), &'static str> {
    for row in &table.rows {
        for cell in &row.cells {
            for run in &cell.content.runs {
                validate_color(run.color.as_deref())?;
            }
        }
    }
    Ok(())
}

pub(crate) fn validate_native_text_colors(document: &WriterDocument) -> Result<(), &'static str> {
    for block in &document.blocks {
        for run in &block.runs {
            validate_color(run.style.color.as_deref())?;
            validate_color(run.style.background.as_deref())?;
        }
        if let Some(table) = &block.table {
            validate_table_text_colors(table)?;
        }
    }
    Ok(())
}

fn character_style_json(style: &loom_text::CharacterStyle) -> String {
    let mut out = String::from("{\"font_family\":");
    out.push_str(&pkg_json::escape(&style.font_family));
    out.push_str(",\"font_size\":");
    out.push_str(&json_f32(style.font_size));
    out.push_str(",\"weight\":");
    out.push_str(&pkg_json::escape(font_weight_name(style.weight)));
    out.push_str(",\"italic\":");
    out.push_str(json_bool(style.italic));
    out.push_str(",\"underline\":");
    out.push_str(json_bool(style.underline));
    out.push_str(",\"strikethrough\":");
    out.push_str(json_bool(style.strikethrough));
    out.push_str(",\"superscript\":");
    out.push_str(json_bool(style.superscript));
    out.push_str(",\"subscript\":");
    out.push_str(json_bool(style.subscript));
    out.push_str(",\"color\":");
    push_optional_json_string(&mut out, style.color.as_deref());
    out.push_str(",\"background\":");
    push_optional_json_string(&mut out, style.background.as_deref());
    // Written only when set, so a run that inherits its size is saved as before.
    if style.explicit_size {
        out.push_str(",\"explicit_size\":true");
    }
    out.push('}');
    out
}

pub(crate) fn runs_json(runs: &[loom_text::StyleRun]) -> String {
    let mut out = String::from("[");
    for (index, run) in runs.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str("{\"start\":");
        out.push_str(&run.start.to_string());
        out.push_str(",\"end\":");
        out.push_str(&run.end.to_string());
        out.push_str(",\"style\":");
        out.push_str(&character_style_json(&run.style));
        out.push('}');
    }
    out.push(']');
    out
}

pub(crate) fn selection_json(selection: &TextSelection) -> String {
    let affinity = match selection.affinity {
        loom_document::CaretAffinity::Upstream => "upstream",
        loom_document::CaretAffinity::Downstream => "downstream",
    };
    format!(
        "{{\"anchor\":{},\"focus\":{},\"affinity\":{}}}",
        selection.anchor,
        selection.focus,
        pkg_json::escape(affinity)
    )
}

#[cfg(test)]
#[path = "native_style_validation_tests.rs"]
mod native_style_validation_tests;
