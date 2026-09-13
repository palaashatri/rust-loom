//! Sheet JSON persistence for `.loomtable` content.
//!
//! Hand-rolled serializer keeps the file format stable and backwards
//! compatible: older packages without `alignments`/`styles` still load as
//! unstyled sheets.

use std::collections::BTreeMap;

use crate::style::CellStyle;
use crate::{CellAlignment, CellRef, NumberFormat, Sheet};

/// Canonical string name for a cell alignment used in sheet JSON.
fn alignment_as_str(align: CellAlignment) -> &'static str {
    match align {
        CellAlignment::General => "general",
        CellAlignment::Left => "left",
        CellAlignment::Center => "center",
        CellAlignment::Right => "right",
    }
}

/// Parse a cell alignment from sheet JSON; unknown values map to General.
fn alignment_from_str(raw: &str) -> CellAlignment {
    match raw {
        "left" => CellAlignment::Left,
        "center" => CellAlignment::Center,
        "right" => CellAlignment::Right,
        _ => CellAlignment::General,
    }
}

/// Canonical string name for a number format used in sheet JSON.
fn number_format_as_str(format: NumberFormat) -> &'static str {
    match format {
        NumberFormat::General => "general",
        NumberFormat::Currency => "currency",
        NumberFormat::Percentage => "percentage",
        NumberFormat::Number => "number",
        NumberFormat::Scientific => "scientific",
        NumberFormat::DateIso => "date-iso",
        NumberFormat::PlainText => "plain-text",
    }
}

/// Parse a number format from sheet JSON; unknown values map to General.
fn number_format_from_str(raw: &str) -> NumberFormat {
    match raw {
        "currency" => NumberFormat::Currency,
        "percentage" => NumberFormat::Percentage,
        "number" => NumberFormat::Number,
        "scientific" => NumberFormat::Scientific,
        "date-iso" => NumberFormat::DateIso,
        "plain-text" => NumberFormat::PlainText,
        _ => NumberFormat::General,
    }
}

/// Serialize a sheet to the `.loomtable` content JSON.
pub fn sheet_to_json(sheet: &Sheet) -> String {
    let mut s = String::with_capacity(256);
    s.push('{');
    s.push_str("\"name\":\"");
    s.push_str(&sheet.name.replace('"', "\\\""));
    s.push_str("\",\"cells\":[");
    let mut first = true;
    for (r, c) in &sheet.cells {
        if !first {
            s.push(',');
        }
        first = false;
        s.push('{');
        s.push_str("\"ref\":\"");
        s.push_str(&r.to_a1());
        s.push_str("\",\"raw\":\"");
        s.push_str(
            &c.raw
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('\n', "\\n"),
        );
        s.push_str("\"}");
    }
    s.push(']');
    s.push_str(",\"alignments\":[");
    let mut first_align = true;
    for (cell, align) in &sheet.alignments {
        if *align == CellAlignment::General {
            continue;
        }
        if !first_align {
            s.push(',');
        }
        first_align = false;
        s.push_str("{\"ref\":\"");
        s.push_str(&cell.to_a1());
        s.push_str("\",\"align\":\"");
        s.push_str(alignment_as_str(*align));
        s.push_str("\"}");
    }
    s.push(']');
    s.push_str(",\"styles\":[");
    let mut first_style = true;
    for (cell, style) in &sheet.styles {
        if style.is_default() {
            continue;
        }
        if !first_style {
            s.push(',');
        }
        first_style = false;
        s.push_str("{\"ref\":\"");
        s.push_str(&cell.to_a1());
        s.push_str("\",\"bold\":");
        s.push_str(if style.bold { "true" } else { "false" });
        s.push_str(",\"italic\":");
        s.push_str(if style.italic { "true" } else { "false" });
        s.push_str(",\"underline\":");
        s.push_str(if style.underline { "true" } else { "false" });
        s.push_str(",\"format\":\"");
        s.push_str(number_format_as_str(style.number_format));
        s.push('"');
        if let Some(decimals) = style.decimal_places {
            s.push_str(",\"decimals\":");
            s.push_str(&decimals.to_string());
        }
        if style.border {
            s.push_str(",\"border\":true");
        }
        if style.fill != crate::style::FillColor::None {
            s.push_str(",\"fill\":\"");
            s.push_str(style.fill.as_str());
            s.push('"');
        }
        if let Some(size) = style.font_size {
            s.push_str(",\"font\":");
            s.push_str(&size.to_string());
        }
        s.push('}');
    }
    s.push(']');
    s.push_str(",\"col_widths\":{");
    let mut first_width = true;
    for (col, width) in &sheet.col_widths {
        if !first_width {
            s.push(',');
        }
        first_width = false;
        s.push('"');
        s.push_str(&col.to_string());
        s.push_str("\":");
        s.push_str(&width.to_string());
    }
    s.push_str("},\"row_heights\":{");
    let mut first_height = true;
    for (row, height) in &sheet.row_heights {
        if !first_height {
            s.push(',');
        }
        first_height = false;
        s.push('"');
        s.push_str(&row.to_string());
        s.push_str("\":");
        s.push_str(&height.to_string());
    }
    s.push('}');
    if sheet.freeze_rows > 0 || sheet.freeze_cols > 0 {
        s.push_str(",\"freeze_rows\":");
        s.push_str(&sheet.freeze_rows.to_string());
        s.push_str(",\"freeze_cols\":");
        s.push_str(&sheet.freeze_cols.to_string());
    }
    if let Some(chart) = &sheet.chart {
        s.push_str(",\"chart\":{\"kind\":\"");
        s.push_str(chart.kind.as_str());
        s.push_str("\",\"title\":\"");
        s.push_str(&chart.title.replace('\\', "\\\\").replace('"', "\\\""));
        s.push_str("\",\"cat_col\":");
        s.push_str(&chart.cat_col.to_string());
        s.push_str(",\"val_col\":");
        s.push_str(&chart.val_col.to_string());
        s.push('}');
    }
    s.push('}');
    s
}

/// Parse sheet JSON back.
pub fn sheet_from_json(s: &str) -> Result<Sheet, String> {
    let mut name = String::new();
    if let Some(prefix) = s.split("\"cells\"").next() {
        if let Some(n) = prefix.split("\"name\":\"").nth(1) {
            let end = n.find('"').unwrap_or(n.len());
            name = n[..end].replace("\\\"", "\"").replace("\\\\", "\\");
        }
    }
    let mut sheet = Sheet::new(&name);
    let body = s.split("\"cells\":").nth(1).unwrap_or("[]");
    for frag in body.split("{\"ref\":\"") {
        if frag.is_empty() {
            continue;
        }
        let Some(end) = frag.find("\",\"raw\":\"") else {
            continue;
        };
        let a1 = &frag[..end];
        let rest = &frag[end + "\",\"raw\":\"".len()..];
        let Some(end2) = rest.find("\"}") else {
            continue;
        };
        let raw = rest[..end2]
            .replace("\\\"", "\"")
            .replace("\\\\", "\\")
            .replace("\\n", "\n");
        sheet.set_str(a1, &raw);
    }
    for (index, width) in parse_dimension_map(s, "col_widths") {
        sheet.set_col_width(index, width);
    }
    for (index, height) in parse_dimension_map(s, "row_heights") {
        sheet.set_row_height(index, height);
    }
    for (cell, align) in parse_alignment_list(s) {
        sheet.set_cell_alignment(cell, align);
    }
    for (cell, style) in parse_style_list(s) {
        sheet.set_cell_style(cell, style);
    }
    sheet.freeze_rows = parse_u32_field(s, "freeze_rows");
    sheet.freeze_cols = parse_u32_field(s, "freeze_cols");
    if let Some(chart) = parse_chart(s) {
        sheet.chart = Some(chart);
    }
    Ok(sheet)
}

/// Parse an optional `"chart":{...}` object; malformed entries load as none.
fn parse_chart(s: &str) -> Option<crate::SheetChart> {
    let marker = "\"chart\":{";
    let start = s.find(marker)? + marker.len();
    let rest = &s[start..];
    // Balance braces outside strings to find the object end.
    let mut depth = 1usize;
    let mut end = None;
    let mut in_string = false;
    let mut escaped = false;
    for (i, ch) in rest.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
        } else if ch == '"' {
            in_string = true;
        } else if ch == '{' {
            depth += 1;
        } else if ch == '}' {
            depth -= 1;
            if depth == 0 {
                end = Some(i);
                break;
            }
        }
    }
    let body = &rest[..end?];
    let kind = body
        .split("\"kind\":\"")
        .nth(1)
        .and_then(|tail| tail.split('"').next())
        .map(crate::ChartKind::parse_kind)
        .unwrap_or(crate::ChartKind::Bar);
    let title = body
        .split("\"title\":\"")
        .nth(1)
        .and_then(|tail| {
            let mut out = String::new();
            let mut chars = tail.chars();
            loop {
                let c = chars.next()?;
                if c == '\\' {
                    out.push(chars.next()?);
                } else if c == '"' {
                    break;
                } else {
                    out.push(c);
                }
            }
            Some(out)
        })
        .unwrap_or_default();
    let number_after = |key: &str| {
        body.split(key)
            .nth(1)
            .and_then(|tail| {
                tail.chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect::<String>()
                    .parse::<u32>()
                    .ok()
            })
            .unwrap_or(0)
    };
    Some(crate::SheetChart {
        kind,
        title,
        cat_col: number_after("\"cat_col\":"),
        val_col: number_after("\"val_col\":"),
    })
}

/// Parse an optional top-level `"key":123` integer; missing or malformed is 0.
fn parse_u32_field(s: &str, key: &str) -> u32 {
    let marker = format!("\"{key}\":");
    let Some(start) = s.find(&marker).map(|index| index + marker.len()) else {
        return 0;
    };
    let digits: String = s[start..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse::<u32>().unwrap_or(0)
}

/// A multi-sheet workbook file: ordered sheets plus the active tab index.
#[derive(Debug, Clone)]
pub struct WorkbookFile {
    /// Sheets in tab order; always at least one.
    pub sheets: Vec<Sheet>,
    /// Active tab clamped to `sheets` on load.
    pub active: usize,
}

/// Serialize a workbook (all tabs + active index) to `.loomtable` content.
pub fn workbook_to_json(sheets: &[Sheet], active: usize) -> String {
    let mut s = String::with_capacity(512);
    s.push_str("{\"version\":1,\"active\":");
    s.push_str(&active.min(sheets.len().saturating_sub(1)).to_string());
    s.push_str(",\"sheets\":[");
    let mut first = true;
    for sheet in sheets {
        if !first {
            s.push(',');
        }
        first = false;
        s.push_str(&sheet_to_json(sheet));
    }
    s.push_str("]}");
    s
}

/// Parse workbook JSON back; unknown trailing keys are ignored.
pub fn workbook_from_json(s: &str) -> Result<WorkbookFile, String> {
    let Some(body) = extract_json_array(s, "sheets") else {
        return Err("missing sheets array".to_string());
    };
    let mut sheets = Vec::new();
    for obj in split_top_level_objects(body) {
        sheets.push(sheet_from_json(obj)?);
    }
    if sheets.is_empty() {
        return Err("workbook must contain at least one sheet".to_string());
    }
    let active = parse_u32_field(s, "active") as usize;
    let active = active.min(sheets.len() - 1);
    Ok(WorkbookFile { sheets, active })
}

/// Split a `[{...},{...}]` array body into its top-level object slices,
/// respecting string escapes so nested braces inside cell text survive.
fn split_top_level_objects(body: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let bytes = body.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'{' {
            i += 1;
            continue;
        }
        let start = i;
        let mut depth = 0usize;
        let mut in_string = false;
        let mut escaped = false;
        while i < bytes.len() {
            let b = bytes[i];
            if in_string {
                if escaped {
                    escaped = false;
                } else if b == b'\\' {
                    escaped = true;
                } else if b == b'"' {
                    in_string = false;
                }
            } else if b == b'"' {
                in_string = true;
            } else if b == b'{' {
                depth += 1;
            } else if b == b'}' {
                depth -= 1;
                if depth == 0 {
                    i += 1;
                    break;
                }
            }
            i += 1;
        }
        out.push(&body[start..i.min(body.len())]);
    }
    out
}

/// Extract the raw array body for a top-level `"key":[...]` entry.
fn extract_json_array<'a>(s: &'a str, key: &str) -> Option<&'a str> {
    let marker = format!("\"{key}\":[");
    let start = s.find(&marker)? + marker.len();
    let rest = &s[start..];
    let mut depth = 1usize;
    let mut end = None;
    for (i, ch) in rest.char_indices() {
        match ch {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }
    end.map(|i| &rest[..i])
}

/// Parse `{"ref":"A1","align":"left"}` entries; unknown refs are skipped.
fn parse_alignment_list(s: &str) -> Vec<(CellRef, CellAlignment)> {
    let Some(body) = extract_json_array(s, "alignments") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for frag in body.split("{\"ref\":\"") {
        if frag.is_empty() {
            continue;
        }
        let Some(end) = frag.find("\",\"align\":\"") else {
            continue;
        };
        let a1 = &frag[..end];
        let rest = &frag[end + "\",\"align\":\"".len()..];
        let Some(end2) = rest.find('"') else {
            continue;
        };
        let Some(cell) = CellRef::parse(a1) else {
            continue;
        };
        out.push((cell, alignment_from_str(&rest[..end2])));
    }
    out
}

/// Parse `{"ref":"A1","bold":true,...,"format":"currency","decimals":2}` entries.
fn parse_style_list(s: &str) -> Vec<(CellRef, CellStyle)> {
    let Some(body) = extract_json_array(s, "styles") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for frag in body.split("{\"ref\":\"") {
        if frag.is_empty() {
            continue;
        }
        let Some(end) = frag.find('"') else {
            continue;
        };
        let a1 = &frag[..end];
        let Some(cell) = CellRef::parse(a1) else {
            continue;
        };
        let entry = &frag[end..];
        let bold = entry.contains("\"bold\":true");
        let italic = entry.contains("\"italic\":true");
        let underline = entry.contains("\"underline\":true");
        let format = entry
            .split("\"format\":\"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .map(number_format_from_str)
            .unwrap_or(NumberFormat::General);
        let decimals = entry.split("\"decimals\":").nth(1).and_then(|rest| {
            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            if digits.is_empty() {
                None
            } else {
                digits.parse::<u8>().ok()
            }
        });
        let border = entry.contains("\"border\":true");
        let fill = entry
            .split("\"fill\":\"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .map(crate::style::FillColor::parse_kind)
            .unwrap_or(crate::style::FillColor::None);
        let font_size = entry.split("\"font\":").nth(1).and_then(|rest| {
            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            if digits.is_empty() {
                None
            } else {
                digits.parse::<u8>().ok()
            }
        });
        out.push((
            cell,
            CellStyle {
                bold,
                italic,
                underline,
                number_format: format,
                decimal_places: decimals,
                border,
                fill,
                font_size,
            },
        ));
    }
    out
}

/// Parse the optional numeric dimension maps emitted by [`sheet_to_json`].
fn parse_dimension_map(s: &str, key: &str) -> BTreeMap<u32, f32> {
    let marker = format!("\"{key}\":{{");
    let Some(start) = s.find(&marker).map(|index| index + marker.len()) else {
        return BTreeMap::new();
    };
    let rest = &s[start..];
    let body = rest.split('}').next().unwrap_or_default();
    body.split(',')
        .filter_map(|entry| {
            let (raw_index, raw_value) = entry.split_once(':')?;
            let index = raw_index.trim().trim_matches('"').parse::<u32>().ok()?;
            let value = raw_value.trim().parse::<f32>().ok()?;
            value.is_finite().then_some((index, value))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evaluate;

    #[test]
    fn json_roundtrip_preserves_alignments_and_styles() {
        let mut sheet = Sheet::new("styled");
        sheet.set_str("A1", "1234.5");
        sheet.set_str("B1", "0.125");
        let a1 = CellRef::parse("A1").unwrap();
        let b1 = CellRef::parse("B1").unwrap();
        sheet.set_cell_alignment(a1, crate::CellAlignment::Center);
        sheet.set_cell_style(
            a1,
            CellStyle {
                bold: true,
                italic: false,
                underline: true,
                number_format: NumberFormat::Currency,
                decimal_places: Some(0),
                border: false,
                fill: crate::style::FillColor::None,
                font_size: None,
            },
        );
        sheet.set_cell_style(
            b1,
            CellStyle {
                bold: false,
                italic: true,
                underline: false,
                number_format: NumberFormat::Percentage,
                decimal_places: Some(2),
                border: true,
                fill: crate::style::FillColor::Blue,
                font_size: Some(18),
            },
        );
        let json = sheet_to_json(&sheet);
        let back = sheet_from_json(&json).unwrap();
        assert_eq!(back.cell_alignment(a1), crate::CellAlignment::Center);
        let back_a1 = back.cell_style(a1);
        assert!(back_a1.bold);
        assert!(back_a1.underline);
        assert!(!back_a1.italic);
        assert_eq!(back_a1.number_format, NumberFormat::Currency);
        assert_eq!(back_a1.decimal_places, Some(0));
        let back_b1 = back.cell_style(b1);
        assert!(back_b1.italic);
        assert_eq!(back_b1.number_format, NumberFormat::Percentage);
        assert_eq!(back_b1.decimal_places, Some(2));
        assert!(back_b1.border);
        assert_eq!(back_b1.fill, crate::style::FillColor::Blue);
        assert_eq!(back_b1.font_size, Some(18));
        let legacy = r#"{"name":"old","cells":[],"col_widths":{},"row_heights":{}}"#;
        let legacy_sheet = sheet_from_json(legacy).unwrap();
        assert_eq!(
            legacy_sheet.cell_alignment(a1),
            crate::CellAlignment::General
        );
        assert!(legacy_sheet.cell_style(a1).is_default());
        let vals = evaluate(&back);
        assert!(vals.contains_key(&a1));
    }

    #[test]
    fn json_roundtrip_preserves_freeze_panes() {
        let mut sheet = Sheet::new("frozen");
        sheet.set_str("A1", "H");
        sheet.freeze_panes(2, 1);
        let json = sheet_to_json(&sheet);
        let back = sheet_from_json(&json).unwrap();
        assert_eq!(back.freeze_rows, 2);
        assert_eq!(back.freeze_cols, 1);

        let plain = Sheet::new("plain");
        let plain_json = sheet_to_json(&plain);
        assert!(!plain_json.contains("freeze_rows"));
        let plain_back = sheet_from_json(&plain_json).unwrap();
        assert_eq!(plain_back.freeze_rows, 0);
        assert_eq!(plain_back.freeze_cols, 0);
    }

    #[test]
    fn workbook_roundtrip_preserves_all_tabs_and_active_index() {
        let mut first = Sheet::new("First");
        first.set_str("A1", "1");
        first.set_str("B1", "=A1+1");
        first.freeze_panes(1, 0);
        let mut second = Sheet::new("Second");
        second.set_str("A1", "text with {braces} and \"quotes\"");
        second.set_cell_alignment(CellRef::parse("A1").unwrap(), crate::CellAlignment::Right);
        let third = Sheet::new("Third");

        let json = workbook_to_json(&[first, second, third], 2);
        let back = workbook_from_json(&json).unwrap();
        assert_eq!(back.sheets.len(), 3);
        assert_eq!(back.active, 2);
        assert_eq!(back.sheets[0].name, "First");
        assert_eq!(
            back.sheets[0].raw(CellRef::parse("B1").unwrap()),
            Some("=A1+1")
        );
        assert_eq!(back.sheets[0].freeze_rows, 1);
        assert_eq!(back.sheets[1].name, "Second");
        assert_eq!(
            back.sheets[1].raw(CellRef::parse("A1").unwrap()),
            Some("text with {braces} and \"quotes\"")
        );
        assert_eq!(
            back.sheets[1].cell_alignment(CellRef::parse("A1").unwrap()),
            crate::CellAlignment::Right
        );
        assert_eq!(back.sheets[2].name, "Third");
        let vals = evaluate(&back.sheets[0]);
        assert_eq!(
            vals.get(&CellRef::parse("B1").unwrap()),
            Some(&crate::Value::Number(2.0))
        );
    }

    #[test]
    fn workbook_load_rejects_empty_and_clamps_active() {
        assert!(workbook_from_json("{\"version\":1,\"active\":0,\"sheets\":[]}").is_err());
        assert!(workbook_from_json("{\"version\":1}").is_err());
        let json = workbook_to_json(&[Sheet::new("Only")], 7);
        let back = workbook_from_json(&json).unwrap();
        assert_eq!(back.active, 0);
    }

    #[test]
    fn sheet_chart_roundtrip_and_kind_cycle() {
        assert_eq!(crate::ChartKind::Bar.cycle(), crate::ChartKind::Line);
        assert_eq!(crate::ChartKind::Line.cycle(), crate::ChartKind::Pie);
        assert_eq!(crate::ChartKind::Pie.cycle(), crate::ChartKind::Bar);
        assert_eq!(crate::ChartKind::parse_kind("line"), crate::ChartKind::Line);
        assert_eq!(crate::ChartKind::parse_kind("bogus"), crate::ChartKind::Bar);

        let mut sheet = Sheet::new("Charted \"Q1\"");
        sheet.set_str("A2", "Q1");
        sheet.set_str("B2", "10");
        sheet.chart = Some(crate::SheetChart {
            kind: crate::ChartKind::Pie,
            title: "Share \"A\"".to_string(),
            cat_col: 0,
            val_col: 1,
        });
        let json = sheet_to_json(&sheet);
        let back = sheet_from_json(&json).unwrap();
        let chart = back.chart.expect("chart persists");
        assert_eq!(chart.kind, crate::ChartKind::Pie);
        assert_eq!(chart.title, "Share \"A\"");
        assert_eq!((chart.cat_col, chart.val_col), (0, 1));

        let plain = Sheet::new("plain");
        assert!(!sheet_to_json(&plain).contains("\"chart\""));
        assert!(sheet_from_json(&sheet_to_json(&plain))
            .unwrap()
            .chart
            .is_none());
    }
}
