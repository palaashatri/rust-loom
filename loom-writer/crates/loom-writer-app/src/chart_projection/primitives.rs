//! Shared, escaped SVG primitives and bounded chart labels.

use std::fmt::Write as _;

use super::{MAX_LABEL_CHARS, PALETTE};

pub(super) fn series_color(color: Option<&str>, index: usize) -> String {
    color
        .filter(|color| valid_color(color))
        .map(str::to_string)
        .unwrap_or_else(|| PALETTE[index % PALETTE.len()].to_string())
}

fn valid_color(color: &str) -> bool {
    color.len() == 7
        && color.starts_with('#')
        && color.as_bytes()[1..]
            .iter()
            .all(|byte| byte.is_ascii_hexdigit())
}

pub(super) fn short_label(label: &str, max_chars: usize) -> String {
    let mut chars = label.chars();
    let mut shortened: String = chars
        .by_ref()
        .take(max_chars.min(MAX_LABEL_CHARS))
        .collect();
    if chars.next().is_some() {
        shortened.push('…');
    }
    shortened
}

fn escape_xml(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len().min(MAX_LABEL_CHARS * 6));
    let mut chars = value.chars();
    for ch in chars.by_ref().take(MAX_LABEL_CHARS) {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            _ => escaped.push(ch),
        }
    }
    if chars.next().is_some() {
        escaped.push('…');
    }
    escaped
}

pub(super) fn text(
    out: &mut String,
    position: (f64, f64),
    size: f64,
    fill: &str,
    anchor: &str,
    value: &str,
    rotate: bool,
) {
    let (x, y) = position;
    let escaped = escape_xml(value);
    if rotate {
        write!(out, r#"<text x="{x:.2}" y="{y:.2}" transform="rotate(-35 {x:.2} {y:.2})" text-anchor="end" font-family="sans-serif" font-size="{size:.1}" fill="{fill}">{escaped}</text>"#)
            .expect("writing to a string cannot fail");
    } else {
        write!(out, r#"<text x="{x:.2}" y="{y:.2}" text-anchor="{anchor}" font-family="sans-serif" font-size="{size:.1}" fill="{fill}">{escaped}</text>"#)
            .expect("writing to a string cannot fail");
    }
}

pub(super) fn chart_title(out: &mut String, x: f64, y: f64, value: &str) {
    let escaped = escape_xml(value);
    write!(out, r##"<text x="{x:.2}" y="{y:.2}" text-anchor="middle" font-family="sans-serif" font-size="11.0" font-weight="600" fill="#202124">{escaped}</text>"##)
        .expect("writing to a string cannot fail");
}

pub(super) fn vertical_text(out: &mut String, x: f64, y: f64, size: f64, fill: &str, value: &str) {
    let escaped = escape_xml(value);
    write!(out, r#"<text x="{x:.2}" y="{y:.2}" transform="rotate(-90 {x:.2} {y:.2})" text-anchor="middle" font-family="sans-serif" font-size="{size:.1}" fill="{fill}">{escaped}</text>"#)
        .expect("writing to a string cannot fail");
}

pub(super) fn line(out: &mut String, x1: f64, y1: f64, x2: f64, y2: f64, color: &str, width: f64) {
    write!(out, r#"<path d="M {x1:.2} {y1:.2} L {x2:.2} {y2:.2}" fill="none" stroke="{color}" stroke-width="{width:.2}"/>"#)
        .expect("writing to a string cannot fail");
}

pub(super) fn rect(out: &mut String, x: f64, y: f64, width: f64, height: f64, color: &str) {
    write!(
        out,
        r#"<rect x="{x:.2}" y="{y:.2}" width="{width:.2}" height="{height:.2}" fill="{color}"/>"#
    )
    .expect("writing to a string cannot fail");
}

pub(super) fn circle(out: &mut String, cx: f64, cy: f64, radius: f64, color: &str) {
    write!(
        out,
        r#"<circle cx="{cx:.2}" cy="{cy:.2}" r="{radius:.2}" fill="{color}"/>"#
    )
    .expect("writing to a string cannot fail");
}

pub(super) fn path_element(
    out: &mut String,
    path: &str,
    fill: &str,
    stroke: &str,
    stroke_width: f64,
    opacity: f64,
) {
    write!(out, r#"<path d="{path}" fill="{fill}" stroke="{stroke}" stroke-width="{stroke_width:.2}" opacity="{opacity:.2}"/>"#)
        .expect("writing to a string cannot fail");
}

pub(super) fn format_axis(value: f64) -> String {
    if value.abs() >= 1_000_000.0 || (value != 0.0 && value.abs() < 0.001) {
        format!("{value:.2e}")
    } else if value.fract().abs() < 0.01 {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    }
}

pub(super) fn format_value(value: f64) -> String {
    if value.abs() >= 1_000_000.0 || (value != 0.0 && value.abs() < 0.001) {
        format!("{value:.2e}")
    } else if value.fract().abs() < 0.01 {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    }
}
