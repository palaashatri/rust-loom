//! Names and JSON encodings for paragraph/character styles and selections.
//!
//! Kept apart from the document model so `lib.rs` stays a module boundary.

use crate::TextSelection;
use loom_package::manifest::json as pkg_json;

pub(crate) fn alignment_name(alignment: loom_text::Alignment) -> &'static str {
    match alignment {
        loom_text::Alignment::Left => "left",
        loom_text::Alignment::Center => "center",
        loom_text::Alignment::Right => "right",
        loom_text::Alignment::Justify => "justify",
    }
}

pub(crate) fn parse_alignment(value: &str) -> loom_text::Alignment {
    match value {
        "center" => loom_text::Alignment::Center,
        "right" => loom_text::Alignment::Right,
        "justify" => loom_text::Alignment::Justify,
        _ => loom_text::Alignment::Left,
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
    out.push('}');
    out
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
