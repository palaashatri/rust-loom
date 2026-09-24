//! Read and write worksheet cell styles.

use std::collections::{BTreeMap, BTreeSet};

use crate::style::{CellAlignment, CellStyle, FillColor};
use crate::{CellRef, Sheet};

use super::cell_refs::parse_cell_ref;
use super::xml::{
    attr, contains_element, elements, elements_with_offsets, xml_escape_attr, xml_unescape,
};
use super::MAIN_NS;

#[derive(Debug, Clone, Copy)]
pub(super) struct XfDef {
    pub(super) style: CellStyle,
    pub(super) alignment: CellAlignment,
}

impl Default for XfDef {
    fn default() -> Self {
        Self {
            style: CellStyle::default(),
            alignment: CellAlignment::General,
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct StyleTable {
    pub(super) xfs: Vec<XfDef>,
    pub(super) num_formats: BTreeMap<u32, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct XfKey {
    pub(super) style: CellStyle,
    pub(super) alignment: CellAlignment,
}

impl XfKey {
    pub(super) fn from_sheet(sheet: &Sheet, cell: CellRef) -> Self {
        Self {
            style: sheet.cell_style(cell),
            alignment: sheet.cell_alignment(cell),
        }
    }

    pub(super) fn is_default(self) -> bool {
        self.style.is_default() && self.alignment == CellAlignment::General
    }
}

pub(super) fn apply_imported_styles(sheet: &mut Sheet, sheet_xml: &str, styles: &StyleTable) {
    if styles.xfs.is_empty() {
        return;
    }
    for cell in elements(sheet_xml, "c") {
        let Some(reference) = attr(cell.attrs, "r") else {
            continue;
        };
        let Some(cell_ref) = parse_cell_ref(&reference) else {
            continue;
        };
        let style_id = attr(cell.attrs, "s")
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(0);
        let xf = styles.xfs.get(style_id).copied().unwrap_or_default();
        sheet.set_cell_style(cell_ref, xf.style);
        sheet.set_cell_alignment(cell_ref, xf.alignment);
    }
}

pub(super) fn parse_styles(xml: &str) -> StyleTable {
    let mut table = StyleTable::default();
    let custom_formats = elements(xml, "numFmt")
        .filter_map(|tag| {
            Some((
                attr(tag.attrs, "numFmtId")?.parse::<u32>().ok()?,
                xml_unescape(&attr(tag.attrs, "formatCode")?),
            ))
        })
        .collect::<BTreeMap<_, _>>();
    table.num_formats = custom_formats;

    let fonts = elements(xml, "font")
        .map(|tag| {
            let size = elements(tag.body, "sz")
                .next()
                .and_then(|value| attr(value.attrs, "val"))
                .and_then(|value| value.parse::<f32>().ok())
                .and_then(|points| {
                    if (points - 11.0).abs() < 0.01 {
                        None
                    } else {
                        Some((points * 4.0 / 3.0).round().clamp(1.0, 255.0) as u8)
                    }
                });
            (
                contains_element(tag.body, "b"),
                contains_element(tag.body, "i"),
                contains_element(tag.body, "u"),
                size,
            )
        })
        .collect::<Vec<_>>();
    let fills = elements(xml, "fill")
        .map(|tag| {
            elements(tag.body, "fgColor")
                .next()
                .and_then(|color| attr(color.attrs, "rgb"))
                .map(|value| fill_from_rgb(&value))
                .unwrap_or(FillColor::None)
        })
        .collect::<Vec<_>>();
    let borders = elements(xml, "border")
        .map(|tag| {
            ["left", "right", "top", "bottom"].iter().any(|side| {
                elements(tag.body, side)
                    .next()
                    .and_then(|value| attr(value.attrs, "style"))
                    .is_some_and(|value| !value.eq_ignore_ascii_case("none"))
            })
        })
        .collect::<Vec<_>>();
    let Some(cell_xfs) = elements(xml, "cellXfs").next() else {
        return table;
    };
    for xf in elements(cell_xfs.body, "xf") {
        let font_id = attr(xf.attrs, "fontId")
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(0);
        let fill_id = attr(xf.attrs, "fillId")
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(0);
        let border_id = attr(xf.attrs, "borderId")
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(0);
        let num_fmt_id = attr(xf.attrs, "numFmtId")
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(0);
        let (bold, italic, underline, font_size) = fonts
            .get(font_id)
            .copied()
            .unwrap_or((false, false, false, None));
        let format_code = table
            .num_formats
            .get(&num_fmt_id)
            .map(String::as_str)
            .or_else(|| builtin_format_code(num_fmt_id));
        let (number_format, decimal_places) = parse_number_format(format_code);
        let style = CellStyle {
            bold,
            italic,
            underline,
            number_format,
            decimal_places,
            border: borders.get(border_id).copied().unwrap_or(false),
            fill: fills.get(fill_id).copied().unwrap_or(FillColor::None),
            font_size,
        };
        let alignment = elements(xf.body, "alignment")
            .next()
            .and_then(|alignment| attr(alignment.attrs, "horizontal"))
            .map(|value| match value.to_ascii_lowercase().as_str() {
                "left" => CellAlignment::Left,
                "center" | "centercontinuous" => CellAlignment::Center,
                "right" => CellAlignment::Right,
                _ => CellAlignment::General,
            })
            .unwrap_or(CellAlignment::General);
        table.xfs.push(XfDef { style, alignment });
    }
    table
}

fn parse_number_format(code: Option<&str>) -> (crate::NumberFormat, Option<u8>) {
    let Some(code) = code else {
        return (crate::NumberFormat::General, None);
    };
    let lower = code.to_ascii_lowercase();
    let decimals = lower
        .split(';')
        .next()
        .and_then(|section| section.split('.').nth(1))
        .map(|fraction| {
            fraction
                .chars()
                .take_while(|c| matches!(c, '0' | '#'))
                .count()
        })
        .filter(|count| *count > 0)
        .map(|count| count.min(u8::MAX as usize) as u8);
    if lower.contains('%') {
        (crate::NumberFormat::Percentage, decimals)
    } else if lower.contains('e') && lower.contains('0') {
        (crate::NumberFormat::Scientific, decimals)
    } else if lower.contains('y') || lower.contains('d') && lower.contains("mm") {
        (crate::NumberFormat::DateIso, decimals)
    } else if lower.contains('$') || lower.contains('€') || lower.contains('£') {
        (crate::NumberFormat::Currency, decimals)
    } else if lower == "@" {
        (crate::NumberFormat::PlainText, None)
    } else if lower == "general" {
        (crate::NumberFormat::General, None)
    } else {
        (crate::NumberFormat::Number, decimals)
    }
}

fn builtin_format_code(id: u32) -> Option<&'static str> {
    match id {
        0 => Some("General"),
        1 => Some("0"),
        2 => Some("0.00"),
        3 => Some("#,##0"),
        4 => Some("#,##0.00"),
        9 => Some("0%"),
        10 => Some("0.00%"),
        11 => Some("0.00E+00"),
        14 => Some("m/d/yy"),
        49 => Some("@"),
        _ => None,
    }
}

pub(super) fn apply_exported_styles(
    xml: &str,
    sheet: &Sheet,
    style_ids: &[(XfKey, usize)],
) -> String {
    let mut styled_cells = BTreeMap::<CellRef, usize>::new();
    for cell in sheet
        .styles
        .keys()
        .chain(sheet.alignments.keys())
        .copied()
        .collect::<BTreeSet<_>>()
    {
        let key = XfKey::from_sheet(sheet, cell);
        if !key.is_default() {
            if let Some((_, id)) = style_ids.iter().find(|(candidate, _)| *candidate == key) {
                styled_cells.insert(cell, *id);
            }
        }
    }
    if styled_cells.is_empty() {
        return xml.to_string();
    }
    let mut output = String::with_capacity(xml.len() + styled_cells.len() * 16);
    let mut cursor = 0;
    let mut present = BTreeSet::new();
    for tag in elements_with_offsets(xml, "c") {
        output.push_str(&xml[cursor..tag.open_start]);
        let Some(reference) = attr(tag.attrs, "r") else {
            output.push_str(&xml[tag.open_start..tag.open_end]);
            cursor = tag.open_end;
            continue;
        };
        let Some(cell_ref) = parse_cell_ref(&reference) else {
            output.push_str(&xml[tag.open_start..tag.open_end]);
            cursor = tag.open_end;
            continue;
        };
        if let Some(style_id) = styled_cells.get(&cell_ref) {
            present.insert(cell_ref);
            let open = &xml[tag.open_start..tag.open_end];
            if open.contains(" s=") || open.contains(" s=\"") {
                output.push_str(open);
            } else if let Some(close) = open.rfind('>') {
                output.push_str(&open[..close]);
                output.push_str(&format!(" s=\"{style_id}\">"));
            } else {
                output.push_str(open);
            }
        } else {
            output.push_str(&xml[tag.open_start..tag.open_end]);
        }
        cursor = tag.open_end;
    }
    output.push_str(&xml[cursor..]);

    let missing = styled_cells
        .iter()
        .filter(|(cell, _)| !present.contains(cell))
        .map(|(cell, style_id)| {
            (
                cell.row,
                format!(
                    "<row r=\"{}\"><c r=\"{}\" s=\"{}\"/></row>",
                    cell.row + 1,
                    cell.to_a1(),
                    style_id
                ),
            )
        })
        .collect::<Vec<_>>();
    if missing.is_empty() {
        return output;
    }
    let rows = missing.into_iter().map(|(_, row)| row).collect::<String>();
    output.replace("</sheetData>", &format!("{rows}</sheetData>"))
}

pub(super) fn render_styles_xml(keys: &[XfKey]) -> String {
    let mut fonts = Vec::<(bool, bool, bool, Option<u8>)>::new();
    let mut fills = Vec::<FillColor>::new();
    let mut borders = Vec::<bool>::new();
    let mut formats = BTreeMap::<String, u32>::new();
    for key in keys {
        let style = key.style;
        let font = (style.bold, style.italic, style.underline, style.font_size);
        if !fonts.contains(&font) {
            fonts.push(font);
        }
        if !fills.contains(&style.fill) {
            fills.push(style.fill);
        }
        if !borders.contains(&style.border) {
            borders.push(style.border);
        }
        let code = number_format_code(style);
        if code != "General" && code != "@" {
            formats.entry(code).or_insert(0);
        }
    }
    for (format_id, id) in (164u32..).zip(formats.values_mut()) {
        *id = format_id;
    }
    let font_xml = fonts
        .iter()
        .map(|(bold, italic, underline, size)| {
            let mut flags = String::new();
            if *bold {
                flags.push_str("<b/>");
            }
            if *italic {
                flags.push_str("<i/>");
            }
            if *underline {
                flags.push_str("<u/>");
            }
            let points = size.map(|px| (px as f32 * 0.75).max(1.0)).unwrap_or(11.0);
            format!("<font>{flags}<sz val=\"{points:.2}\"/><name val=\"Aptos\"/></font>")
        })
        .collect::<String>();
    let fill_xml = fills
        .iter()
        .map(|fill| {
            if *fill == FillColor::None {
                "<fill><patternFill patternType=\"none\"/></fill>".to_string()
            } else {
                format!(
                    "<fill><patternFill patternType=\"solid\"><fgColor rgb=\"FF{}\"/><bgColor indexed=\"64\"/></patternFill></fill>",
                    rgb_for_fill(*fill)
                )
            }
        })
        .collect::<String>();
    let border_xml = borders
        .iter()
        .map(|border| {
            if *border {
                "<border><left style=\"thin\"/><right style=\"thin\"/><top style=\"thin\"/><bottom style=\"thin\"/><diagonal/></border>".to_string()
            } else {
                "<border><left/><right/><top/><bottom/><diagonal/></border>".to_string()
            }
        })
        .collect::<String>();
    let num_fmt_xml = formats
        .iter()
        .map(|(code, id)| {
            format!(
                "<numFmt numFmtId=\"{id}\" formatCode=\"{}\"/>",
                xml_escape_attr(code)
            )
        })
        .collect::<String>();
    let xf_xml = keys
        .iter()
        .map(|key| {
            let font_id = fonts
                .iter()
                .position(|font| {
                    *font
                        == (
                            key.style.bold,
                            key.style.italic,
                            key.style.underline,
                            key.style.font_size,
                        )
                })
                .unwrap_or(0);
            let fill_id = fills.iter().position(|fill| *fill == key.style.fill).unwrap_or(0);
            let border_id = borders
                .iter()
                .position(|border| *border == key.style.border)
                .unwrap_or(0);
            let code = number_format_code(key.style);
            let num_fmt_id = if code == "General" {
                0
            } else if code == "@" {
                49
            } else {
                *formats.get(&code).unwrap_or(&0)
            };
            let alignment = match key.alignment {
                CellAlignment::Left => "<alignment horizontal=\"left\"/>",
                CellAlignment::Center => "<alignment horizontal=\"center\"/>",
                CellAlignment::Right => "<alignment horizontal=\"right\"/>",
                CellAlignment::General => "",
            };
            let apply_alignment = if alignment.is_empty() {
                ""
            } else {
                " applyAlignment=\"1\""
            };
            format!(
                "<xf numFmtId=\"{num_fmt_id}\" fontId=\"{font_id}\" fillId=\"{fill_id}\" borderId=\"{border_id}\" xfId=\"0\"{apply_alignment}>{alignment}</xf>"
            )
        })
        .collect::<String>();
    format!(
        "<?xml version=\"1.0\"?><styleSheet xmlns=\"{MAIN_NS}\"><numFmts count=\"{}\">{num_fmt_xml}</numFmts><fonts count=\"{}\">{font_xml}</fonts><fills count=\"{}\">{fill_xml}</fills><borders count=\"{}\">{border_xml}</borders><cellStyleXfs count=\"1\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\"/></cellStyleXfs><cellXfs count=\"{}\">{xf_xml}</cellXfs><cellStyles count=\"1\"><cellStyle name=\"Normal\" xfId=\"0\" builtinId=\"0\"/></cellStyles></styleSheet>",
        formats.len(),
        fonts.len(),
        fills.len(),
        borders.len(),
        keys.len()
    )
}

fn number_format_code(style: CellStyle) -> String {
    let decimals = style.decimal_places.unwrap_or(match style.number_format {
        crate::NumberFormat::Currency => 2,
        crate::NumberFormat::Percentage => 1,
        crate::NumberFormat::Number | crate::NumberFormat::Scientific => 2,
        _ => 0,
    });
    match style.number_format {
        crate::NumberFormat::General => "General".to_string(),
        crate::NumberFormat::Currency => format!("$#,##0.{}", "0".repeat(decimals as usize)),
        crate::NumberFormat::Percentage => format!("0.{}%", "0".repeat(decimals as usize)),
        crate::NumberFormat::Number => format!("#,##0.{}", "0".repeat(decimals as usize)),
        crate::NumberFormat::Scientific => format!("0.{}E+00", "0".repeat(decimals as usize)),
        crate::NumberFormat::DateIso => "yyyy-mm-dd".to_string(),
        crate::NumberFormat::PlainText => "@".to_string(),
    }
}

pub(super) fn rgb_for_fill(fill: FillColor) -> &'static str {
    match fill {
        FillColor::None => "FFFFFF",
        FillColor::Red => "FECACA",
        FillColor::Orange => "FED7AA",
        FillColor::Yellow => "FEF08A",
        FillColor::Green => "BBF7D0",
        FillColor::Blue => "BFDBFE",
        FillColor::Purple => "DDD6FE",
        FillColor::Gray => "E5E7EB",
    }
}

pub(super) fn fill_from_rgb(raw: &str) -> FillColor {
    let value = raw.trim().trim_start_matches('#');
    let value = if value.len() == 8 { &value[2..] } else { value };
    [
        FillColor::Red,
        FillColor::Orange,
        FillColor::Yellow,
        FillColor::Green,
        FillColor::Blue,
        FillColor::Purple,
        FillColor::Gray,
    ]
    .into_iter()
    .min_by_key(|fill| color_distance(value, rgb_for_fill(*fill)))
    .filter(|fill| color_distance(value, rgb_for_fill(*fill)) < 20_000)
    .unwrap_or(FillColor::None)
}

fn color_distance(left: &str, right: &str) -> u32 {
    let parse = |value: &str| {
        if value.len() != 6 {
            return [0u8; 3];
        }
        [
            u8::from_str_radix(&value[0..2], 16).unwrap_or(0),
            u8::from_str_radix(&value[2..4], 16).unwrap_or(0),
            u8::from_str_radix(&value[4..6], 16).unwrap_or(0),
        ]
    };
    let left = parse(left);
    let right = parse(right);
    left.into_iter()
        .zip(right)
        .map(|(a, b)| (i32::from(a) - i32::from(b)).unsigned_abs().pow(2))
        .sum()
}
