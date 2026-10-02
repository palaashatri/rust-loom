//! Read `xl/styles.xml` into the cell formats the Loom model keeps.
//!
//! Every format Excel can store that the model cannot show exactly (a font
//! colour, wrapped text, a fill that is not one of Loom's seven swatches, a
//! time format, ...) is mapped to the closest thing Loom has and recorded on
//! the format, so the import can say so instead of looking faithful.

use std::collections::BTreeMap;

use crate::style::{CellAlignment, CellStyle, FillColor};

use super::number_formats::{builtin_format_code, parse_number_format};
use super::styles::{fill_from_rgb, rgb_for_fill};
use super::xml::{attr, contains_element, elements, xml_unescape};

#[derive(Debug, Clone, Copy)]
pub(super) struct XfDef {
    pub(super) style: CellStyle,
    pub(super) alignment: CellAlignment,
    /// The number format needs something Loom cannot display.
    pub(super) number_lossy: bool,
    /// Fonts, colours, wrapping or borders were reduced to Loom's subset.
    pub(super) approximated: bool,
}

impl Default for XfDef {
    fn default() -> Self {
        Self {
            style: CellStyle::default(),
            alignment: CellAlignment::General,
            number_lossy: false,
            approximated: false,
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct StyleTable {
    pub(super) xfs: Vec<XfDef>,
}

#[derive(Clone, Copy)]
struct FontFacts {
    bold: bool,
    italic: bool,
    underline: bool,
    size: Option<u8>,
    approximated: bool,
}

fn truthy_flag(body: &str, tag: &str) -> bool {
    elements(body, tag).next().is_some_and(|element| {
        !matches!(
            attr(element.attrs, "val").as_deref(),
            Some("0") | Some("false")
        )
    })
}

fn font_facts(body: &str) -> FontFacts {
    let size = elements(body, "sz")
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
    let plain_color = elements(body, "color").next().map_or(true, |color| {
        let rgb = attr(color.attrs, "rgb");
        let theme = attr(color.attrs, "theme");
        let indexed = attr(color.attrs, "indexed");
        rgb.as_deref().is_some_and(|rgb| rgb.ends_with("000000"))
            || theme.as_deref() == Some("1")
            || matches!(indexed.as_deref(), Some("8") | Some("64"))
            || attr(color.attrs, "auto").is_some()
    });
    FontFacts {
        bold: truthy_flag(body, "b"),
        italic: truthy_flag(body, "i"),
        underline: contains_element(body, "u")
            && attr(elements(body, "u").next().map_or("", |u| u.attrs), "val").as_deref()
                != Some("none"),
        size,
        approximated: !plain_color
            || contains_element(body, "strike")
            || contains_element(body, "vertAlign"),
    }
}

/// `(fill, approximated)` for one `<fill>` element.
fn fill_facts(body: &str) -> (FillColor, bool) {
    let solid = elements(body, "patternFill")
        .next()
        .is_some_and(|pattern| attr(pattern.attrs, "patternType").as_deref() == Some("solid"));
    if !solid {
        let styled = elements(body, "patternFill").next().is_some_and(|pattern| {
            !matches!(
                attr(pattern.attrs, "patternType").as_deref(),
                None | Some("none") | Some("gray125")
            )
        });
        return (
            FillColor::None,
            styled || contains_element(body, "gradientFill"),
        );
    }
    let color = elements(body, "fgColor").next();
    let Some(rgb) = color.and_then(|color| attr(color.attrs, "rgb")) else {
        // Theme or indexed colours need the workbook theme; not read.
        return (FillColor::None, true);
    };
    let fill = fill_from_rgb(&rgb);
    let digits = rgb.trim().trim_start_matches('#');
    let digits = if digits.len() == 8 {
        &digits[2..]
    } else {
        digits
    };
    let exact = fill != FillColor::None && rgb_for_fill(fill).eq_ignore_ascii_case(digits);
    (fill, !exact)
}

/// `(any border, approximated)` for one `<border>` element.
fn border_facts(body: &str) -> (bool, bool) {
    let sides: Vec<Option<String>> = ["left", "right", "top", "bottom"]
        .iter()
        .map(|side| {
            elements(body, side)
                .next()
                .and_then(|value| attr(value.attrs, "style"))
                .filter(|style| !style.eq_ignore_ascii_case("none"))
        })
        .collect();
    let drawn = sides.iter().filter(|side| side.is_some()).count();
    let all_thin = drawn == 4 && sides.iter().all(|side| side.as_deref() == Some("thin"));
    (drawn > 0, drawn > 0 && !all_thin)
}

pub(super) fn parse_styles(xml: &str) -> StyleTable {
    let mut table = StyleTable::default();
    let custom_formats: BTreeMap<u32, String> = elements(xml, "numFmt")
        .filter_map(|tag| {
            Some((
                attr(tag.attrs, "numFmtId")?.parse::<u32>().ok()?,
                xml_unescape(&attr(tag.attrs, "formatCode")?),
            ))
        })
        .collect();
    let fonts: Vec<FontFacts> = elements(xml, "font")
        .map(|tag| font_facts(tag.body))
        .collect();
    let fills: Vec<(FillColor, bool)> = elements(xml, "fill")
        .map(|tag| fill_facts(tag.body))
        .collect();
    let borders: Vec<(bool, bool)> = elements(xml, "border")
        .map(|tag| border_facts(tag.body))
        .collect();
    let Some(cell_xfs) = elements(xml, "cellXfs").next() else {
        return table;
    };
    let index = |xf: &super::xml::XmlElement<'_>, name: &str| {
        attr(xf.attrs, name)
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(0)
    };
    for xf in elements(cell_xfs.body, "xf") {
        let font = fonts
            .get(index(&xf, "fontId"))
            .copied()
            .unwrap_or(FontFacts {
                bold: false,
                italic: false,
                underline: false,
                size: None,
                approximated: false,
            });
        let (fill, fill_approximated) = fills
            .get(index(&xf, "fillId"))
            .copied()
            .unwrap_or((FillColor::None, false));
        let (border, border_approximated) = borders
            .get(index(&xf, "borderId"))
            .copied()
            .unwrap_or((false, false));
        let number_id = attr(xf.attrs, "numFmtId")
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(0);
        let code = custom_formats
            .get(&number_id)
            .map(String::as_str)
            .or_else(|| builtin_format_code(number_id));
        let number = parse_number_format(code);
        let alignment_element = elements(xf.body, "alignment").next();
        let alignment_attr = |name: &str| alignment_element.and_then(|a| attr(a.attrs, name));
        let horizontal = alignment_attr("horizontal").map(|value| value.to_ascii_lowercase());
        let alignment = match horizontal.as_deref() {
            Some("left") => CellAlignment::Left,
            Some("center") | Some("centercontinuous") => CellAlignment::Center,
            Some("right") => CellAlignment::Right,
            _ => CellAlignment::General,
        };
        let alignment_approximated = matches!(
            horizontal.as_deref(),
            Some("fill") | Some("justify") | Some("distributed")
        ) || matches!(
            alignment_attr("wrapText").as_deref(),
            Some("1") | Some("true")
        ) || alignment_attr("indent")
            .is_some_and(|value| value != "0")
            || alignment_attr("textRotation").is_some_and(|value| value != "0");
        table.xfs.push(XfDef {
            style: CellStyle {
                bold: font.bold,
                italic: font.italic,
                underline: font.underline,
                number_format: number.format,
                decimal_places: number.decimals,
                border,
                fill,
                font_size: font.size,
            },
            alignment,
            number_lossy: number.lossy,
            approximated: font.approximated
                || fill_approximated
                || border_approximated
                || alignment_approximated,
        });
    }
    table
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NumberFormat;

    #[test]
    fn excel_default_cell_format_is_plain_and_exact() {
        let xml = "<styleSheet><fonts><font><sz val=\"11\"/><color theme=\"1\"/><name val=\"Calibri\"/></font></fonts>\
            <fills><fill><patternFill patternType=\"none\"/></fill><fill><patternFill patternType=\"gray125\"/></fill></fills>\
            <borders><border><left/><right/><top/><bottom/><diagonal/></border></borders>\
            <cellXfs count=\"1\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\"/></cellXfs></styleSheet>";
        let table = parse_styles(xml);
        assert!(table.xfs[0].style.is_default());
        assert!(!table.xfs[0].approximated && !table.xfs[0].number_lossy);
    }

    #[test]
    fn unsupported_formatting_is_flagged_on_the_format() {
        let xml = "<styleSheet><numFmts count=\"1\"><numFmt numFmtId=\"164\" formatCode=\"h:mm\"/></numFmts>\
            <fonts><font><sz val=\"11\"/></font>\
            <font><b val=\"0\"/><color rgb=\"FFFF0000\"/></font></fonts>\
            <fills><fill><patternFill patternType=\"none\"/></fill><fill><patternFill patternType=\"gray125\"/></fill>\
            <fill><patternFill patternType=\"solid\"><fgColor rgb=\"FFFFFF00\"/></patternFill></fill></fills>\
            <borders><border><left/><right/><top/><bottom/></border></borders>\
            <cellXfs count=\"3\"><xf fontId=\"0\" fillId=\"0\" borderId=\"0\"/>\
            <xf numFmtId=\"164\" fontId=\"0\" fillId=\"0\" borderId=\"0\"/>\
            <xf fontId=\"1\" fillId=\"2\" borderId=\"0\"><alignment horizontal=\"center\" wrapText=\"1\"/></xf></cellXfs></styleSheet>";
        let table = parse_styles(xml);
        assert!(table.xfs[1].number_lossy);
        assert_eq!(table.xfs[1].style.number_format, NumberFormat::General);
        let styled = table.xfs[2];
        assert!(!styled.style.bold, "<b val=\"0\"/> is not bold");
        assert_eq!(styled.style.fill, FillColor::Yellow);
        assert_eq!(styled.alignment, CellAlignment::Center);
        assert!(styled.approximated, "font colour, nearest swatch and wrap");
    }
}
