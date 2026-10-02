//! Write worksheet cell styles and map the Loom fill swatches to colours.

use std::collections::BTreeMap;

use crate::style::{CellAlignment, CellStyle, FillColor};
use crate::{CellRef, Sheet};

use super::xml::xml_escape_attr;
use super::MAIN_NS;

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
        if style.fill != FillColor::None && !fills.contains(&style.fill) {
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
            format!(
                "<font>{flags}<sz val=\"{points:.2}\"/><color theme=\"1\"/><name val=\"Calibri\"/><family val=\"2\"/></font>"
            )
        })
        .collect::<String>();
    // Excel reserves the first two fills: "none", then the "gray125" hatch.
    // A solid fill placed second would be replaced by that hatch on load.
    let fill_xml = fills
        .iter()
        .map(|fill| {
            format!(
                "<fill><patternFill patternType=\"solid\"><fgColor rgb=\"FF{}\"/><bgColor indexed=\"64\"/></patternFill></fill>",
                rgb_for_fill(*fill)
            )
        })
        .fold(
            "<fill><patternFill patternType=\"none\"/></fill><fill><patternFill patternType=\"gray125\"/></fill>"
                .to_string(),
            |all, fill| all + &fill,
        );
    let border_xml = borders
        .iter()
        .map(|border| {
            if *border {
                "<border><left style=\"thin\"><color auto=\"1\"/></left><right style=\"thin\"><color auto=\"1\"/></right><top style=\"thin\"><color auto=\"1\"/></top><bottom style=\"thin\"><color auto=\"1\"/></bottom><diagonal/></border>".to_string()
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
            let fill_id = fills
                .iter()
                .position(|fill| *fill == key.style.fill)
                .map_or(0, |position| position + 2);
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
            let mut apply = String::new();
            for (flag, used) in [
                ("applyNumberFormat", num_fmt_id != 0),
                ("applyFont", font_id != 0),
                ("applyFill", fill_id != 0),
                ("applyBorder", border_id != 0),
                ("applyAlignment", !alignment.is_empty()),
            ] {
                if used {
                    apply.push_str(&format!(" {flag}=\"1\""));
                }
            }
            format!(
                "<xf numFmtId=\"{num_fmt_id}\" fontId=\"{font_id}\" fillId=\"{fill_id}\" borderId=\"{border_id}\" xfId=\"0\"{apply}>{alignment}</xf>"
            )
        })
        .collect::<String>();
    let num_fmts = if formats.is_empty() {
        String::new()
    } else {
        format!(
            "<numFmts count=\"{}\">{num_fmt_xml}</numFmts>",
            formats.len()
        )
    };
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><styleSheet xmlns=\"{MAIN_NS}\">{num_fmts}<fonts count=\"{}\">{font_xml}</fonts><fills count=\"{}\">{fill_xml}</fills><borders count=\"{}\">{border_xml}</borders><cellStyleXfs count=\"1\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\"/></cellStyleXfs><cellXfs count=\"{}\">{xf_xml}</cellXfs><cellStyles count=\"1\"><cellStyle name=\"Normal\" xfId=\"0\" builtinId=\"0\"/></cellStyles><dxfs count=\"0\"/><tableStyles count=\"0\" defaultTableStyle=\"TableStyleMedium2\" defaultPivotStyle=\"PivotStyleLight16\"/></styleSheet>",
        fonts.len(),
        fills.len() + 2,
        borders.len(),
        keys.len()
    )
}

pub(super) fn number_format_code(style: CellStyle) -> String {
    let decimals = style.decimal_places.unwrap_or(match style.number_format {
        crate::NumberFormat::Currency => 2,
        crate::NumberFormat::Percentage => 1,
        crate::NumberFormat::Number | crate::NumberFormat::Scientific => 2,
        _ => 0,
    });
    // Excel rejects a trailing decimal point, so zero decimals drop the dot.
    let fraction = if decimals == 0 {
        String::new()
    } else {
        format!(".{}", "0".repeat(decimals as usize))
    };
    match style.number_format {
        crate::NumberFormat::General => "General".to_string(),
        crate::NumberFormat::Currency => format!("$#,##0{fraction}"),
        crate::NumberFormat::Percentage => format!("0{fraction}%"),
        crate::NumberFormat::Number => format!("#,##0{fraction}"),
        crate::NumberFormat::Scientific => format!("0{fraction}E+00"),
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
