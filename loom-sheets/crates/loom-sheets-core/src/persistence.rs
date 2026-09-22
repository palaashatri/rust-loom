//! Sheet JSON persistence for `.loomtable` content.
//!
//! The wire shape stays stable while serde_json provides complete escaping and
//! validation: older packages without `alignments`/`styles` still load as
//! unstyled sheets.

use std::collections::BTreeMap;

use crate::style::CellStyle;
use crate::{CellAlignment, CellRef, NumberFormat, Sheet};
use serde::{Deserialize, Serialize};

fn is_false(value: &bool) -> bool {
    !*value
}

fn is_zero_u32(value: &u32) -> bool {
    *value == 0
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedCell {
    #[serde(rename = "ref")]
    reference: String,
    raw: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedAlignment {
    #[serde(rename = "ref")]
    reference: String,
    align: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedStyle {
    #[serde(rename = "ref")]
    reference: String,
    #[serde(default)]
    bold: bool,
    #[serde(default)]
    italic: bool,
    #[serde(default)]
    underline: bool,
    #[serde(default)]
    format: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    decimals: Option<u8>,
    #[serde(default, skip_serializing_if = "is_false")]
    border: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    fill: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    font: Option<u8>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedChart {
    kind: String,
    title: String,
    cat_col: u32,
    val_col: u32,
    #[serde(default = "default_chart_start_row")]
    start_row: u32,
    #[serde(default)]
    end_row: Option<u32>,
}

fn default_chart_start_row() -> u32 {
    1
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedObject {
    kind: String,
    row: u32,
    col: u32,
    width: u32,
    height: u32,
    label: String,
    path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    asset: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    fill: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedSheet {
    name: String,
    cells: Vec<PersistedCell>,
    #[serde(default)]
    alignments: Vec<PersistedAlignment>,
    #[serde(default)]
    styles: Vec<PersistedStyle>,
    #[serde(default)]
    col_widths: BTreeMap<String, f32>,
    #[serde(default)]
    row_heights: BTreeMap<String, f32>,
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    freeze_rows: u32,
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    freeze_cols: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    chart: Option<PersistedChart>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    objects: Vec<PersistedObject>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedWorkbook {
    #[serde(default)]
    version: Option<u32>,
    #[serde(default)]
    active: usize,
    sheets: Vec<PersistedSheet>,
}

fn alignment_as_str(align: CellAlignment) -> &'static str {
    match align {
        CellAlignment::General => "general",
        CellAlignment::Left => "left",
        CellAlignment::Center => "center",
        CellAlignment::Right => "right",
    }
}

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

impl From<&Sheet> for PersistedSheet {
    fn from(sheet: &Sheet) -> Self {
        Self {
            name: sheet.name.clone(),
            cells: sheet
                .cells
                .iter()
                .map(|(reference, cell)| PersistedCell {
                    reference: reference.to_a1(),
                    raw: cell.raw.clone(),
                })
                .collect(),
            alignments: sheet
                .alignments
                .iter()
                .filter(|(_, alignment)| **alignment != CellAlignment::General)
                .map(|(reference, alignment)| PersistedAlignment {
                    reference: reference.to_a1(),
                    align: alignment_as_str(*alignment).to_string(),
                })
                .collect(),
            styles: sheet
                .styles
                .iter()
                .filter(|(_, style)| !style.is_default())
                .map(|(reference, style)| PersistedStyle {
                    reference: reference.to_a1(),
                    bold: style.bold,
                    italic: style.italic,
                    underline: style.underline,
                    format: Some(number_format_as_str(style.number_format).to_string()),
                    decimals: style.decimal_places,
                    border: style.border,
                    fill: (style.fill != crate::style::FillColor::None)
                        .then(|| style.fill.as_str().to_string()),
                    font: style.font_size,
                })
                .collect(),
            col_widths: sheet
                .col_widths
                .iter()
                .map(|(index, width)| (index.to_string(), *width))
                .collect(),
            row_heights: sheet
                .row_heights
                .iter()
                .map(|(index, height)| (index.to_string(), *height))
                .collect(),
            freeze_rows: sheet.freeze_rows,
            freeze_cols: sheet.freeze_cols,
            chart: sheet.chart.as_ref().map(|chart| PersistedChart {
                kind: chart.kind.as_str().to_string(),
                title: chart.title.clone(),
                cat_col: chart.cat_col,
                val_col: chart.val_col,
                start_row: chart.start_row,
                end_row: chart.end_row,
            }),
            objects: sheet
                .objects
                .iter()
                .map(|object| PersistedObject {
                    kind: object.kind.as_str().to_string(),
                    row: object.anchor.row,
                    col: object.anchor.col,
                    width: object.width,
                    height: object.height,
                    label: object.label.clone(),
                    path: object.path.clone(),
                    asset: object.asset.clone(),
                    fill: Some(object.fill.as_str().to_string()),
                })
                .collect(),
        }
    }
}

/// Serialize a sheet to the `.loomtable` content JSON with a standards-compliant encoder.
pub fn sheet_to_json(sheet: &Sheet) -> String {
    serde_json::to_string(&PersistedSheet::from(sheet)).expect("sheet model is JSON serializable")
}

fn parse_alignment(raw: &str) -> Result<CellAlignment, String> {
    match raw {
        "general" => Ok(CellAlignment::General),
        "left" => Ok(CellAlignment::Left),
        "center" => Ok(CellAlignment::Center),
        "right" => Ok(CellAlignment::Right),
        other => Err(format!("unknown cell alignment {other:?}")),
    }
}

fn parse_number_format(raw: Option<&str>) -> Result<NumberFormat, String> {
    match raw.unwrap_or("general") {
        "general" => Ok(NumberFormat::General),
        "currency" => Ok(NumberFormat::Currency),
        "percentage" => Ok(NumberFormat::Percentage),
        "number" => Ok(NumberFormat::Number),
        "scientific" => Ok(NumberFormat::Scientific),
        "date-iso" => Ok(NumberFormat::DateIso),
        "plain-text" => Ok(NumberFormat::PlainText),
        other => Err(format!("unknown number format {other:?}")),
    }
}

fn parse_fill(raw: Option<&str>) -> Result<crate::style::FillColor, String> {
    match raw.unwrap_or("none") {
        "none" => Ok(crate::style::FillColor::None),
        "red" => Ok(crate::style::FillColor::Red),
        "orange" => Ok(crate::style::FillColor::Orange),
        "yellow" => Ok(crate::style::FillColor::Yellow),
        "green" => Ok(crate::style::FillColor::Green),
        "blue" => Ok(crate::style::FillColor::Blue),
        "purple" => Ok(crate::style::FillColor::Purple),
        "gray" | "grey" => Ok(crate::style::FillColor::Gray),
        other => Err(format!("unknown fill color {other:?}")),
    }
}

fn parse_chart_kind(raw: &str) -> Result<crate::ChartKind, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "bar" => Ok(crate::ChartKind::Bar),
        "line" => Ok(crate::ChartKind::Line),
        "pie" => Ok(crate::ChartKind::Pie),
        "scatter" => Ok(crate::ChartKind::Scatter),
        other => Err(format!("unknown chart kind {other:?}")),
    }
}

fn decode_dimension_map(
    values: BTreeMap<String, f32>,
    label: &str,
) -> Result<BTreeMap<u32, f32>, String> {
    values
        .into_iter()
        .map(|(raw_index, value)| {
            let index = raw_index
                .parse::<u32>()
                .map_err(|_| format!("{label} index {raw_index:?} is not a u32"))?;
            if !value.is_finite() || value <= 0.0 {
                return Err(format!("{label} {raw_index} must be finite and positive"));
            }
            Ok((index, value))
        })
        .collect()
}

fn decode_sheet(raw: PersistedSheet) -> Result<Sheet, String> {
    let mut sheet = Sheet::new(&raw.name);
    for cell in raw.cells {
        let reference = CellRef::parse(&cell.reference)
            .ok_or_else(|| format!("invalid cell reference {:?}", cell.reference))?;
        if sheet
            .cells
            .insert(reference, crate::Cell { raw: cell.raw })
            .is_some()
        {
            return Err(format!("duplicate cell reference {:?}", cell.reference));
        }
    }
    for alignment in raw.alignments {
        let reference = CellRef::parse(&alignment.reference)
            .ok_or_else(|| format!("invalid alignment reference {:?}", alignment.reference))?;
        let value = parse_alignment(&alignment.align)?;
        if sheet.alignments.insert(reference, value).is_some() {
            return Err(format!(
                "duplicate alignment reference {:?}",
                alignment.reference
            ));
        }
    }
    for style in raw.styles {
        let reference = CellRef::parse(&style.reference)
            .ok_or_else(|| format!("invalid style reference {:?}", style.reference))?;
        let value = CellStyle {
            bold: style.bold,
            italic: style.italic,
            underline: style.underline,
            number_format: parse_number_format(style.format.as_deref())?,
            decimal_places: style.decimals,
            border: style.border,
            fill: parse_fill(style.fill.as_deref())?,
            font_size: style.font,
        };
        if sheet.styles.insert(reference, value).is_some() {
            return Err(format!("duplicate style reference {:?}", style.reference));
        }
    }
    sheet.col_widths = decode_dimension_map(raw.col_widths, "column width")?;
    sheet.row_heights = decode_dimension_map(raw.row_heights, "row height")?;
    sheet.freeze_rows = raw.freeze_rows;
    sheet.freeze_cols = raw.freeze_cols;
    sheet.chart = raw
        .chart
        .map(|chart| -> Result<crate::SheetChart, String> {
            Ok(crate::SheetChart {
                kind: parse_chart_kind(&chart.kind)?,
                title: chart.title,
                cat_col: chart.cat_col,
                val_col: chart.val_col,
                start_row: chart.start_row,
                end_row: chart.end_row,
            })
        })
        .transpose()?;
    sheet.objects = raw
        .objects
        .into_iter()
        .map(|object| {
            let kind = crate::SheetObjectKind::parse(&object.kind)
                .ok_or_else(|| format!("unknown sheet object kind {:?}", object.kind))?;
            if object.width == 0 || object.height == 0 {
                return Err(format!(
                    "sheet object {:?} has zero dimensions",
                    object.label
                ));
            }
            Ok(crate::SheetObject {
                kind,
                anchor: CellRef {
                    row: object.row,
                    col: object.col,
                },
                width: object.width,
                height: object.height,
                label: object.label,
                path: object.path,
                embedded: None,
                asset: object.asset,
                fill: parse_fill(object.fill.as_deref())?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(sheet)
}

/// Parse sheet JSON back using a complete JSON decoder.
pub fn sheet_from_json(s: &str) -> Result<Sheet, String> {
    let raw: PersistedSheet = serde_json::from_str(s).map_err(|error| error.to_string())?;
    decode_sheet(raw)
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
    let payload = PersistedWorkbook {
        version: Some(1),
        active: active.min(sheets.len().saturating_sub(1)),
        sheets: sheets.iter().map(PersistedSheet::from).collect(),
    };
    serde_json::to_string(&payload).expect("workbook model is JSON serializable")
}

/// Parse workbook JSON back; unknown trailing keys are ignored.
pub fn workbook_from_json(s: &str) -> Result<WorkbookFile, String> {
    let payload: PersistedWorkbook = serde_json::from_str(s).map_err(|error| error.to_string())?;
    if let Some(version) = payload.version {
        if version != 1 {
            return Err(format!("unsupported workbook JSON version {version}"));
        }
    }
    let sheets = payload
        .sheets
        .into_iter()
        .map(decode_sheet)
        .collect::<Result<Vec<_>, String>>()?;
    if sheets.is_empty() {
        return Err("workbook must contain at least one sheet".to_string());
    }
    let active = payload.active.min(sheets.len() - 1);
    Ok(WorkbookFile { sheets, active })
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
            ..Default::default()
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

    #[test]
    fn sheet_objects_roundtrip_with_legacy_payload_compatibility() {
        let mut sheet = Sheet::new("Objects");
        sheet.objects.push(crate::SheetObject::shape(
            CellRef { row: 1, col: 2 },
            "Review",
        ));
        sheet
            .objects
            .push(crate::SheetObject::image(CellRef { row: 5, col: 0 }, "/tmp/hero.png").unwrap());

        let json = sheet_to_json(&sheet);
        let back = sheet_from_json(&json).expect("object payload loads");
        assert_eq!(back.objects, sheet.objects);

        let legacy = r#"{"name":"old","cells":[],"col_widths":{},"row_heights":{}}"#;
        assert!(sheet_from_json(legacy).unwrap().objects.is_empty());
    }

    #[test]
    fn json_roundtrip_escapes_control_chars_backslashes_quotes_and_unicode() {
        let mut sheet = Sheet::new("My \"Sheet\"");
        let values = [
            ("A1", "one\ttwo"),
            ("A2", "one\rtwo"),
            ("A3", "one\ntwo"),
            ("A4", r#"C:\new\notes"#),
            ("A5", "before\"}after"),
            ("A6", "नमस्ते 🌱"),
        ];
        for (reference, value) in values {
            sheet.set_str(reference, value);
        }

        let json = sheet_to_json(&sheet);
        assert!(serde_json::from_str::<serde_json::Value>(&json).is_ok());
        let back = sheet_from_json(&json).expect("escaped sheet JSON must load");
        assert_eq!(back.name, "My \"Sheet\"");
        for (reference, value) in values {
            let cell = CellRef::parse(reference).unwrap();
            assert_eq!(back.raw(cell), Some(value));
        }

        let workbook_json = workbook_to_json(&[sheet], 0);
        let workbook = workbook_from_json(&workbook_json).expect("escaped workbook JSON must load");
        assert_eq!(workbook.sheets[0].name, "My \"Sheet\"");
        assert_eq!(
            workbook.sheets[0].raw(CellRef::parse("A5").unwrap()),
            Some("before\"}after")
        );
    }

    #[test]
    fn json_load_rejects_duplicate_refs_unknown_enums_and_unsupported_versions() {
        let duplicate =
            r#"{"name":"x","cells":[{"ref":"A1","raw":"one"},{"ref":"A1","raw":"two"}]}"#;
        assert!(sheet_from_json(duplicate).is_err());

        let unknown_style =
            r#"{"name":"x","cells":[],"styles":[{"ref":"A1","format":"not-a-format"}]}"#;
        assert!(sheet_from_json(unknown_style).is_err());

        let unknown_chart = r#"{"name":"x","cells":[],"chart":{"kind":"not-a-chart","title":"x","cat_col":0,"val_col":1}}"#;
        assert!(sheet_from_json(unknown_chart).is_err());

        let unsupported_version = r#"{"version":2,"active":0,"sheets":[{"name":"x","cells":[]}] }"#;
        assert!(workbook_from_json(unsupported_version).is_err());
    }
    #[test]
    fn legacy_chart_without_row_bounds_keeps_all_rows() {
        let mut sheet = Sheet::new("Legacy");
        sheet.chart = Some(crate::SheetChart::default());
        let mut json: serde_json::Value = serde_json::from_str(&sheet_to_json(&sheet)).unwrap();
        let chart = json["chart"].as_object_mut().unwrap();
        chart.remove("start_row");
        chart.remove("end_row");
        let restored = sheet_from_json(&json.to_string()).unwrap().chart.unwrap();
        assert_eq!(restored.start_row, 1);
        assert_eq!(restored.end_row, None);
    }
}
