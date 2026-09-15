//! Rich XLSX interop for the worksheet model.

use std::collections::{BTreeMap, BTreeSet};

use loom_package::zip::PackageArchive;

use crate::style::{CellAlignment, CellStyle, FillColor};
use crate::{CellRef, ChartKind, Sheet, SheetChart, SheetObject, SheetObjectKind};

const MAIN_NS: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const REL_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const PACKAGE_REL_NS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const DRAWING_NS: &str = "http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing";
const CHART_NS: &str = "http://schemas.openxmlformats.org/drawingml/2006/chart";
const DRAWINGML_NS: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const EMU_PER_PIXEL: f32 = 9_525.0;

#[derive(Debug, Clone)]
struct SheetPart {
    name: String,
    path: String,
    xml: String,
}

#[derive(Debug, Clone, Copy)]
struct XfDef {
    style: CellStyle,
    alignment: CellAlignment,
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
struct StyleTable {
    xfs: Vec<XfDef>,
    num_formats: BTreeMap<u32, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct XfKey {
    style: CellStyle,
    alignment: CellAlignment,
}

impl XfKey {
    fn from_sheet(sheet: &Sheet, cell: CellRef) -> Self {
        Self {
            style: sheet.cell_style(cell),
            alignment: sheet.cell_alignment(cell),
        }
    }

    fn is_default(self) -> bool {
        self.style.is_default() && self.alignment == CellAlignment::General
    }
}

#[derive(Debug, Clone, Copy)]
struct DrawingAnchor {
    cell: CellRef,
    width: u32,
    height: u32,
}

/// Import an `.xlsx` workbook into the full worksheet model used by the app.
///
/// The importer keeps formulas and cached values from the existing worksheet
/// reader, then adds the OOXML features represented by the Loom model:
/// cell styles, alignments, basic charts, shapes, and embedded images. Excel
/// pivot caches remain usable as their cached worksheet cells, because Loom's
/// native pivot model is formula-backed rather than an OOXML cache object.
pub fn extract_xlsx_sheets(xlsx_bytes: &[u8]) -> Result<Vec<Sheet>, String> {
    let archive = PackageArchive::from_bytes(xlsx_bytes)
        .map_err(|e| format!("unreadable xlsx archive: {e}"))?;
    let basic = super::extract_xlsx_workbook(xlsx_bytes)?;
    let parts = workbook_sheet_parts(&archive)?;
    if parts.is_empty() {
        return Err("xlsx workbook has no worksheets".to_string());
    }
    let styles = archive
        .get("xl/styles.xml")
        .and_then(|bytes| std::str::from_utf8(bytes).ok())
        .map(parse_styles)
        .unwrap_or_default();

    let mut sheets = Vec::with_capacity(parts.len());
    for (index, part) in parts.iter().enumerate() {
        let (basic_name, grid) = basic
            .get(index)
            .cloned()
            .unwrap_or_else(|| (part.name.clone(), Vec::new()));
        let name = if part.name.trim().is_empty() {
            basic_name
        } else {
            part.name.clone()
        };
        let mut sheet = Sheet::new(&name);
        for (row, values) in grid.iter().enumerate() {
            for (col, value) in values.iter().enumerate() {
                if !value.is_empty() {
                    sheet.set_raw(
                        CellRef {
                            row: row as u32,
                            col: col as u32,
                        },
                        value,
                    );
                }
            }
        }
        apply_imported_styles(&mut sheet, &part.xml, &styles);
        import_drawings(&mut sheet, &part.xml, &part.path, &archive)?;
        sheets.push(sheet);
    }
    Ok(sheets)
}

fn workbook_sheet_parts(archive: &PackageArchive) -> Result<Vec<SheetPart>, String> {
    let Some(workbook_bytes) = archive.get("xl/workbook.xml") else {
        let sheet_path = "xl/worksheets/sheet1.xml";
        let sheet_bytes = archive
            .get(sheet_path)
            .ok_or_else(|| "missing worksheet part xl/worksheets/sheet1.xml".to_string())?;
        let xml = std::str::from_utf8(sheet_bytes)
            .map_err(|_| "xl/worksheets/sheet1.xml is not valid UTF-8".to_string())?;
        return Ok(vec![SheetPart {
            name: "Sheet1".to_string(),
            path: sheet_path.to_string(),
            xml: xml.to_string(),
        }]);
    };
    let workbook_xml = std::str::from_utf8(workbook_bytes)
        .map_err(|_| "xl/workbook.xml is not valid UTF-8".to_string())?;
    let rels_bytes = archive
        .get("xl/_rels/workbook.xml.rels")
        .ok_or_else(|| "missing workbook relationships".to_string())?;
    let rels_xml = std::str::from_utf8(rels_bytes)
        .map_err(|_| "xl/_rels/workbook.xml.rels is not valid UTF-8".to_string())?;
    let relationships = relationship_map(rels_xml);
    let mut parts = Vec::new();
    for tag in elements(workbook_xml, "sheet") {
        let Some(name) = attr(tag.attrs, "name").map(|value| xml_unescape(&value)) else {
            continue;
        };
        let Some(rel_id) = attr(tag.attrs, "r:id").or_else(|| attr(tag.attrs, "id")) else {
            continue;
        };
        let Some(target) = relationships.get(&rel_id) else {
            continue;
        };
        let path = resolve_target("xl", target);
        let Some(sheet_bytes) = archive.get(&path) else {
            return Err(format!("missing worksheet part {path}"));
        };
        let xml =
            std::str::from_utf8(sheet_bytes).map_err(|_| format!("{path} is not valid UTF-8"))?;
        parts.push(SheetPart {
            name,
            path,
            xml: xml.to_string(),
        });
    }
    Ok(parts)
}

fn apply_imported_styles(sheet: &mut Sheet, sheet_xml: &str, styles: &StyleTable) {
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

fn parse_styles(xml: &str) -> StyleTable {
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

fn import_drawings(
    sheet: &mut Sheet,
    sheet_xml: &str,
    sheet_path: &str,
    archive: &PackageArchive,
) -> Result<(), String> {
    let Some(drawing_tag) = elements(sheet_xml, "drawing").next() else {
        return Ok(());
    };
    let Some(drawing_rel_id) =
        attr(drawing_tag.attrs, "r:id").or_else(|| attr(drawing_tag.attrs, "id"))
    else {
        return Ok(());
    };
    let rels_path = relationship_part_path(sheet_path);
    let Some(rels_bytes) = archive.get(&rels_path) else {
        return Ok(());
    };
    let rels_xml =
        std::str::from_utf8(rels_bytes).map_err(|_| format!("{rels_path} is not valid UTF-8"))?;
    let sheet_relationships = relationship_map(rels_xml);
    let Some(drawing_target) = sheet_relationships.get(&drawing_rel_id) else {
        return Ok(());
    };
    let drawing_path = resolve_target(parent_path(sheet_path), drawing_target);
    let Some(drawing_bytes) = archive.get(&drawing_path) else {
        return Ok(());
    };
    let drawing_xml = std::str::from_utf8(drawing_bytes)
        .map_err(|_| format!("{drawing_path} is not valid UTF-8"))?;
    let drawing_rels_path = relationship_part_path(&drawing_path);
    let drawing_relationships = archive
        .get(&drawing_rels_path)
        .and_then(|bytes| std::str::from_utf8(bytes).ok())
        .map(relationship_map)
        .unwrap_or_default();

    for anchor_tag in
        elements(drawing_xml, "oneCellAnchor").chain(elements(drawing_xml, "twoCellAnchor"))
    {
        let anchor = parse_drawing_anchor(anchor_tag.body);
        if let Some(graphic) = elements(anchor_tag.body, "graphicFrame").next() {
            if let Some(chart_rel_id) = elements(graphic.body, "chart")
                .next()
                .and_then(|tag| attr(tag.attrs, "r:id").or_else(|| attr(tag.attrs, "id")))
            {
                if let Some(target) = drawing_relationships.get(&chart_rel_id) {
                    let chart_path = resolve_target(parent_path(&drawing_path), target);
                    if let Some(chart_bytes) = archive.get(&chart_path) {
                        if let Ok(chart_xml) = std::str::from_utf8(chart_bytes) {
                            if let Some(chart) = parse_chart(chart_xml) {
                                sheet.chart = Some(chart);
                            }
                        }
                    }
                }
            }
            continue;
        }
        if let Some(pic) = elements(anchor_tag.body, "pic").next() {
            let Some(embed) = elements(pic.body, "blip")
                .next()
                .and_then(|tag| attr(tag.attrs, "r:embed").or_else(|| attr(tag.attrs, "embed")))
            else {
                continue;
            };
            let Some(target) = drawing_relationships.get(&embed) else {
                continue;
            };
            let image_path = resolve_target(parent_path(&drawing_path), target);
            let Some(bytes) = archive.get(&image_path) else {
                continue;
            };
            let label = elements(pic.body, "cNvPr")
                .next()
                .and_then(|tag| attr(tag.attrs, "descr").or_else(|| attr(tag.attrs, "name")))
                .map(|value| xml_unescape(&value))
                .unwrap_or_else(|| image_path.rsplit('/').next().unwrap_or("Image").to_string());
            let mut object = SheetObject::image(anchor.cell, image_path.clone())?;
            object.width = anchor.width;
            object.height = anchor.height;
            object.label = label;
            object.embedded = Some(bytes.to_vec());
            sheet.objects.push(object);
            continue;
        }
        if let Some(shape) = elements(anchor_tag.body, "sp").next() {
            let label = elements(shape.body, "t")
                .next()
                .map(|tag| xml_unescape(tag.body.trim()))
                .unwrap_or_default();
            let fill = elements(shape.body, "srgbClr")
                .next()
                .and_then(|tag| attr(tag.attrs, "val"))
                .map(|value| fill_from_rgb(&value))
                .unwrap_or(FillColor::Blue);
            let mut object = SheetObject::shape(anchor.cell, label);
            object.width = anchor.width;
            object.height = anchor.height;
            object.fill = fill;
            sheet.objects.push(object);
        }
    }
    Ok(())
}

fn parse_drawing_anchor(body: &str) -> DrawingAnchor {
    let cell = elements(body, "from")
        .next()
        .map(|tag| parse_marker(tag.body))
        .unwrap_or(CellRef { row: 0, col: 0 });
    let (mut width, mut height) = elements(body, "ext")
        .next()
        .map(|tag| {
            (
                attr(tag.attrs, "cx")
                    .and_then(|value| value.parse::<f32>().ok())
                    .map(|value| (value / EMU_PER_PIXEL).round().max(1.0) as u32)
                    .unwrap_or(240),
                attr(tag.attrs, "cy")
                    .and_then(|value| value.parse::<f32>().ok())
                    .map(|value| (value / EMU_PER_PIXEL).round().max(1.0) as u32)
                    .unwrap_or(112),
            )
        })
        .unwrap_or((240, 112));
    if let Some(to) = elements(body, "to").next() {
        let end = parse_marker(to.body);
        if !elements(body, "ext").next().is_some() {
            width = ((end.col.saturating_sub(cell.col) + 1) as f32 * super::DEFAULT_COL_WIDTH)
                .round() as u32;
            height = ((end.row.saturating_sub(cell.row) + 1) as f32 * super::DEFAULT_ROW_HEIGHT)
                .round() as u32;
        }
    }
    DrawingAnchor {
        cell,
        width: width.max(1),
        height: height.max(1),
    }
}

fn parse_marker(body: &str) -> CellRef {
    let col = elements(body, "col")
        .next()
        .and_then(|tag| tag.body.trim().parse::<u32>().ok())
        .unwrap_or(0);
    let row = elements(body, "row")
        .next()
        .and_then(|tag| tag.body.trim().parse::<u32>().ok())
        .unwrap_or(0);
    CellRef { row, col }
}

fn parse_chart(xml: &str) -> Option<SheetChart> {
    let kind = if contains_element(xml, "pieChart") {
        ChartKind::Pie
    } else if contains_element(xml, "scatterChart") {
        ChartKind::Scatter
    } else if contains_element(xml, "lineChart") {
        ChartKind::Line
    } else if contains_element(xml, "barChart") {
        ChartKind::Bar
    } else {
        return None;
    };
    let title = elements(xml, "t")
        .next()
        .map(|tag| xml_unescape(tag.body.trim()))
        .unwrap_or_else(|| "Chart".to_string());
    let formulas = elements(xml, "f")
        .map(|tag| xml_unescape(tag.body.trim()))
        .collect::<Vec<_>>();
    let cat_col = formulas
        .first()
        .and_then(|formula| formula_column(formula))
        .unwrap_or(0);
    let val_col = formulas
        .get(1)
        .and_then(|formula| formula_column(formula))
        .unwrap_or(cat_col.saturating_add(1));
    Some(SheetChart {
        kind,
        title,
        cat_col,
        val_col,
    })
}

fn formula_column(formula: &str) -> Option<u32> {
    let range = formula.rsplit('!').next().unwrap_or(formula);
    let first = range.split(':').next()?.trim_matches('$');
    let letters = first
        .chars()
        .take_while(|c| c.is_ascii_alphabetic())
        .collect::<String>();
    if letters.is_empty() {
        return None;
    }
    Some(column_index(&letters))
}

/// Export every worksheet with formulas, cell formatting, and supported
/// drawing objects represented in OOXML parts. The legacy exporter supplies
/// the well-tested cell/value package; this layer adds the richer parts and
/// rewrites the package without losing the base workbook metadata.
pub fn export_xlsx_sheets(sheets: &[Sheet]) -> Result<Vec<u8>, String> {
    if sheets.is_empty() {
        return Err("xlsx export needs at least one sheet".to_string());
    }
    let evaluated = super::workbook::evaluate_workbook(sheets);
    let data = sheets
        .iter()
        .zip(evaluated.iter())
        .map(|(sheet, values)| super::sheet_to_xlsx_data(sheet, values))
        .collect::<Vec<_>>();
    let base = super::export_xlsx_workbook(&data)?;
    let base_archive = PackageArchive::from_bytes(&base)
        .map_err(|e| format!("xlsx export base archive failed: {e}"))?;

    let mut style_keys = vec![XfKey {
        style: CellStyle::default(),
        alignment: CellAlignment::General,
    }];
    for sheet in sheets {
        let mut coordinates = BTreeSet::new();
        coordinates.extend(sheet.cells.keys().copied());
        coordinates.extend(sheet.styles.keys().copied());
        coordinates.extend(sheet.alignments.keys().copied());
        for cell in coordinates {
            let key = XfKey::from_sheet(sheet, cell);
            if !style_keys.contains(&key) {
                style_keys.push(key);
            }
        }
    }
    let style_ids = style_keys
        .iter()
        .enumerate()
        .map(|(id, key)| (*key, id))
        .collect::<Vec<_>>();
    let styles_xml = render_styles_xml(&style_keys);
    let mut replacements = BTreeMap::<String, Vec<u8>>::new();
    let mut extras = BTreeMap::<String, Vec<u8>>::new();
    let mut image_extensions = BTreeSet::new();

    for (index, sheet) in sheets.iter().enumerate() {
        let sheet_path = format!("xl/worksheets/sheet{}.xml", index + 1);
        let sheet_xml = base_archive
            .get(&sheet_path)
            .ok_or_else(|| format!("missing generated worksheet part {sheet_path}"))?;
        let sheet_xml = std::str::from_utf8(sheet_xml)
            .map_err(|_| format!("{sheet_path} is not valid UTF-8"))?;
        let styled_xml = apply_exported_styles(sheet_xml, sheet, &style_ids);
        let has_drawing = sheet.chart.is_some() || !sheet.objects.is_empty();
        if has_drawing {
            let drawing_number = index + 1;
            let (drawing_xml, drawing_rels, chart_parts, image_parts, extensions) =
                render_drawing_parts(sheet, index + 1)?;
            image_extensions.extend(extensions);
            extras.insert(
                format!("xl/drawings/drawing{drawing_number}.xml"),
                drawing_xml.into_bytes(),
            );
            extras.insert(
                format!("xl/drawings/_rels/drawing{drawing_number}.xml.rels"),
                drawing_rels.into_bytes(),
            );
            for (path, xml) in chart_parts {
                extras.insert(path, xml.into_bytes());
            }
            for (path, bytes) in image_parts {
                extras.insert(path, bytes);
            }
            // `worksheet` is generated by the legacy exporter without the
            // office-document relationship namespace. Declare it on the
            // element that owns `r:id` so independent OOXML parsers can
            // resolve the prefix.
            let drawing_relationship = format!("<drawing xmlns:r=\"{REL_NS}\" r:id=\"rId1\"/>");
            let with_drawing = styled_xml.replace(
                "</worksheet>",
                &format!("{drawing_relationship}</worksheet>"),
            );
            extras.insert(
                format!("xl/worksheets/_rels/sheet{}.xml.rels", index + 1),
                worksheet_drawing_rels(drawing_number).into_bytes(),
            );
            replacements.insert(sheet_path, with_drawing.into_bytes());
        } else {
            replacements.insert(sheet_path, styled_xml.into_bytes());
        }
    }

    let content_types = base_archive
        .get("[Content_Types].xml")
        .ok_or_else(|| "generated xlsx is missing [Content_Types].xml".to_string())?;
    let content_types = std::str::from_utf8(content_types)
        .map_err(|_| "[Content_Types].xml is not valid UTF-8".to_string())?;
    replacements.insert(
        "[Content_Types].xml".to_string(),
        render_content_types(content_types, sheets, &image_extensions).into_bytes(),
    );
    extras.insert("xl/styles.xml".to_string(), styles_xml.into_bytes());

    let mut archive = PackageArchive::new();
    for path in base_archive.paths() {
        if let Some(bytes) = replacements.remove(path) {
            archive
                .add(path, bytes)
                .map_err(|e| format!("xlsx export failed: {e}"))?;
        } else if path != "xl/styles.xml" {
            archive
                .add(path, base_archive.get(path).unwrap_or_default().to_vec())
                .map_err(|e| format!("xlsx export failed: {e}"))?;
        }
    }
    for (path, bytes) in replacements {
        archive
            .add(&path, bytes)
            .map_err(|e| format!("xlsx export failed: {e}"))?;
    }
    for (path, bytes) in extras {
        archive
            .add(&path, bytes)
            .map_err(|e| format!("xlsx export failed: {e}"))?;
    }
    archive
        .to_bytes()
        .map_err(|e| format!("xlsx export failed: {e}"))
}

fn apply_exported_styles(xml: &str, sheet: &Sheet, style_ids: &[(XfKey, usize)]) -> String {
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

type DrawingParts = (
    String,
    String,
    Vec<(String, String)>,
    Vec<(String, Vec<u8>)>,
    BTreeSet<String>,
);

fn render_drawing_parts(sheet: &Sheet, sheet_number: usize) -> Result<DrawingParts, String> {
    let mut drawing = String::new();
    let mut drawing_rels = String::new();
    let mut chart_parts = Vec::new();
    let mut image_parts = Vec::new();
    let mut image_extensions = BTreeSet::new();
    let mut rel_id = 1usize;
    let mut object_id = 1usize;

    if let Some(chart) = &sheet.chart {
        let chart_rel = format!("rId{rel_id}");
        rel_id += 1;
        drawing_rels.push_str(&format!(
            "<Relationship Id=\"{chart_rel}\" Type=\"{CHART_REL_TYPE}\" Target=\"../charts/chart{sheet_number}.xml\"/>"
        ));
        chart_parts.push((
            format!("xl/charts/chart{sheet_number}.xml"),
            render_chart_xml(sheet, chart),
        ));
        drawing.push_str(&one_cell_anchor(
            CellRef { row: 0, col: 3 },
            560,
            320,
            &format!(
                "<xdr:graphicFrame macro=\"\"><xdr:nvGraphicFramePr><xdr:cNvPr id=\"{object_id}\" name=\"Chart {object_id}\"/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr><xdr:xfrm/><a:graphic><a:graphicData uri=\"{CHART_GRAPHIC_URI}\"><c:chart r:id=\"{chart_rel}\"/></a:graphicData></a:graphic></xdr:graphicFrame>"
            ),
        ));
        object_id += 1;
    }

    for (object_index, object) in sheet.objects.iter().enumerate() {
        match object.kind {
            SheetObjectKind::Shape => {
                drawing.push_str(&one_cell_anchor(
                    object.anchor,
                    object.width,
                    object.height,
                    &format!(
                        "<xdr:sp><xdr:nvSpPr><xdr:cNvPr id=\"{object_id}\" name=\"Shape {object_id}\" descr=\"{}\"/><xdr:cNvSpPr txBox=\"0\"/></xdr:nvSpPr><xdr:spPr><a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill><a:ln/></xdr:spPr><xdr:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang=\"en-US\"/><a:t>{}</a:t></a:r></a:p></xdr:txBody></xdr:sp>",
                        xml_escape_attr(&object.label),
                        rgb_for_fill(object.fill),
                        xml_escape_text(&object.label),
                    ),
                ));
                object_id += 1;
            }
            SheetObjectKind::Image => {
                let payload = object
                    .embedded
                    .clone()
                    .or_else(|| std::fs::read(&object.path).ok())
                    .ok_or_else(|| {
                        if object.path.trim().is_empty() {
                            "image object has no embedded payload or source path".to_string()
                        } else {
                            format!("unable to read image {}", object.path)
                        }
                    })?;
                let extension = image_extension_for_object(object);
                image_extensions.insert(extension.to_string());
                let filename = format!("sheet{sheet_number}-object{object_index}.{extension}");
                let image_rel = format!("rId{rel_id}");
                rel_id += 1;
                drawing_rels.push_str(&format!(
                    "<Relationship Id=\"{image_rel}\" Type=\"{IMAGE_REL_TYPE}\" Target=\"../media/{filename}\"/>"
                ));
                image_parts.push((format!("xl/media/{filename}"), payload));
                drawing.push_str(&one_cell_anchor(
                    object.anchor,
                    object.width,
                    object.height,
                    &format!(
                        "<xdr:pic><xdr:nvPicPr><xdr:cNvPr id=\"{object_id}\" name=\"Image {object_id}\" descr=\"{}\"/><xdr:cNvPicPr/></xdr:nvPicPr><xdr:blipFill><a:blip r:embed=\"{image_rel}\"/><a:stretch><a:fillRect/></a:stretch></xdr:blipFill><xdr:spPr><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></xdr:spPr></xdr:pic>",
                        xml_escape_attr(&object.label),
                    ),
                ));
                object_id += 1;
            }
        }
    }
    Ok((
        format!(
            "<?xml version=\"1.0\"?><xdr:wsDr xmlns:xdr=\"{DRAWING_NS}\" xmlns:a=\"{DRAWINGML_NS}\" xmlns:c=\"{CHART_NS}\" xmlns:r=\"{REL_NS}\">{drawing}</xdr:wsDr>"
        ),
        format!(
            "<?xml version=\"1.0\"?><Relationships xmlns=\"{PACKAGE_REL_NS}\">{drawing_rels}</Relationships>"
        ),
        chart_parts,
        image_parts,
        image_extensions,
    ))
}

fn one_cell_anchor(cell: CellRef, width: u32, height: u32, content: &str) -> String {
    let cx = (width.max(1) as f32 * EMU_PER_PIXEL).round() as u64;
    let cy = (height.max(1) as f32 * EMU_PER_PIXEL).round() as u64;
    format!(
        "<xdr:oneCellAnchor><xdr:from><xdr:col>{}</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>{}</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:ext cx=\"{cx}\" cy=\"{cy}\"/>{content}<xdr:clientData/></xdr:oneCellAnchor>",
        cell.col, cell.row
    )
}

fn render_chart_xml(sheet: &Sheet, chart: &SheetChart) -> String {
    let end_row = sheet
        .used_range()
        .map(|(_, _, _, max_row)| max_row + 1)
        .unwrap_or(2)
        .max(2);
    let name = chart_sheet_reference(&sheet.name);
    let cat_column = column_letters(chart.cat_col as usize);
    let val_column = column_letters(chart.val_col as usize);
    // Spreadsheet quoting and XML escaping are separate operations. The
    // sheet name is quoted for the formula grammar above; escape the complete
    // generated formula before putting it in element text.
    let cat_range = xml_escape_text(&format!("{name}!${cat_column}$2:${cat_column}${end_row}"));
    let val_range = xml_escape_text(&format!("{name}!${val_column}$2:${val_column}${end_row}"));
    let title = xml_escape_text(&chart.title);
    let series = match chart.kind {
        ChartKind::Scatter => format!(
            "<c:scatterChart><c:scatterStyle val=\"lineMarker\"/><c:varyColors val=\"0\"/><c:ser><c:idx val=\"0\"/><c:order val=\"0\"/><c:xVal><c:numRef><c:f>{cat_range}</c:f></c:numRef></c:xVal><c:yVal><c:numRef><c:f>{val_range}</c:f></c:numRef></c:yVal></c:ser><c:axId val=\"-2128941756\"/><c:axId val=\"-2128941755\"/></c:scatterChart>"
        ),
        ChartKind::Pie => format!(
            "<c:pieChart><c:varyColors val=\"1\"/><c:ser><c:idx val=\"0\"/><c:order val=\"0\"/><c:cat><c:strRef><c:f>{cat_range}</c:f></c:strRef></c:cat><c:val><c:numRef><c:f>{val_range}</c:f></c:numRef></c:val></c:ser></c:pieChart>"
        ),
        ChartKind::Line => format!(
            "<c:lineChart><c:grouping val=\"standard\"/><c:varyColors val=\"0\"/><c:ser><c:idx val=\"0\"/><c:order val=\"0\"/><c:cat><c:strRef><c:f>{cat_range}</c:f></c:strRef></c:cat><c:val><c:numRef><c:f>{val_range}</c:f></c:numRef></c:val></c:ser><c:axId val=\"-2128941756\"/><c:axId val=\"-2128941755\"/></c:lineChart>"
        ),
        ChartKind::Bar => format!(
            "<c:barChart><c:barDir val=\"col\"/><c:grouping val=\"clustered\"/><c:varyColors val=\"0\"/><c:ser><c:idx val=\"0\"/><c:order val=\"0\"/><c:cat><c:strRef><c:f>{cat_range}</c:f></c:strRef></c:cat><c:val><c:numRef><c:f>{val_range}</c:f></c:numRef></c:val></c:ser><c:axId val=\"-2128941756\"/><c:axId val=\"-2128941755\"/></c:barChart>"
        ),
    };
    let axes = match chart.kind {
        ChartKind::Bar | ChartKind::Line => "<c:catAx><c:axId val=\"-2128941756\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"b\"/><c:crossAx val=\"-2128941755\"/><c:crosses val=\"autoZero\"/><c:auto val=\"1\"/><c:lblAlgn val=\"ctr\"/><c:lblOffset val=\"100\"/></c:catAx><c:valAx><c:axId val=\"-2128941755\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"l\"/><c:crossAx val=\"-2128941756\"/><c:crosses val=\"autoZero\"/><c:crossBetween val=\"midCat\"/></c:valAx>",
        ChartKind::Scatter => "<c:valAx><c:axId val=\"-2128941756\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"b\"/><c:crossAx val=\"-2128941755\"/><c:crosses val=\"autoZero\"/></c:valAx><c:valAx><c:axId val=\"-2128941755\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"l\"/><c:crossAx val=\"-2128941756\"/><c:crosses val=\"autoZero\"/></c:valAx>",
        ChartKind::Pie => "",
    };
    format!(
        "<?xml version=\"1.0\"?><c:chartSpace xmlns:c=\"{CHART_NS}\" xmlns:a=\"{DRAWINGML_NS}\"><c:chart><c:title><c:tx><c:rich><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang=\"en-US\"/><a:t>{title}</a:t></a:r></a:p></c:rich></c:tx><c:layout/></c:title><c:plotArea><c:layout/>{series}{axes}</c:plotArea><c:plotVisOnly val=\"1\"/></c:chart></c:chartSpace>"
    )
}

fn worksheet_drawing_rels(drawing_number: usize) -> String {
    format!(
        "<?xml version=\"1.0\"?><Relationships xmlns=\"{PACKAGE_REL_NS}\"><Relationship Id=\"rId1\" Type=\"{DRAWING_REL_TYPE}\" Target=\"../drawings/drawing{drawing_number}.xml\"/></Relationships>"
    )
}

fn render_content_types(
    base: &str,
    sheets: &[Sheet],
    image_extensions: &BTreeSet<String>,
) -> String {
    let mut additions = String::new();
    additions.push_str(
        "<Override PartName=\"/xl/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml\"/>",
    );
    for (index, sheet) in sheets.iter().enumerate() {
        if sheet.chart.is_some() || !sheet.objects.is_empty() {
            additions.push_str(&format!(
                "<Override PartName=\"/xl/drawings/drawing{}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.drawing+xml\"/>",
                index + 1
            ));
        }
        if sheet.chart.is_some() {
            additions.push_str(&format!(
                "<Override PartName=\"/xl/charts/chart{}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.drawingml.chart+xml\"/>",
                index + 1
            ));
        }
    }
    for extension in image_extensions {
        let mime = match extension.as_str() {
            "jpg" | "jpeg" => "image/jpeg",
            "gif" => "image/gif",
            "webp" => "image/webp",
            "svg" => "image/svg+xml",
            _ => "image/png",
        };
        additions.push_str(&format!(
            "<Default Extension=\"{extension}\" ContentType=\"{mime}\"/>"
        ));
    }
    base.replace("</Types>", &format!("{additions}</Types>"))
}

const DRAWING_REL_TYPE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing";
const CHART_REL_TYPE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart";
const IMAGE_REL_TYPE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image";
const CHART_GRAPHIC_URI: &str = "http://schemas.openxmlformats.org/drawingml/2006/chart";

fn render_styles_xml(keys: &[XfKey]) -> String {
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

fn rgb_for_fill(fill: FillColor) -> &'static str {
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

fn fill_from_rgb(raw: &str) -> FillColor {
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

fn image_extension_for_object(object: &SheetObject) -> &'static str {
    let source = if object.path.trim().is_empty() {
        object.asset.as_deref().unwrap_or("image.png")
    } else {
        &object.path
    };
    match source
        .rsplit('.')
        .next()
        .unwrap_or("png")
        .to_ascii_lowercase()
        .as_str()
    {
        "jpg" | "jpeg" => "jpg",
        "gif" => "gif",
        "webp" => "webp",
        "svg" | "svgz" => "svg",
        _ => "png",
    }
}

#[derive(Debug, Clone, Copy)]
struct XmlElement<'a> {
    attrs: &'a str,
    body: &'a str,
}

#[derive(Debug, Clone, Copy)]
struct XmlSpan<'a> {
    open_start: usize,
    open_end: usize,
    attrs: &'a str,
}

fn elements<'a>(xml: &'a str, wanted: &str) -> std::vec::IntoIter<XmlElement<'a>> {
    let mut found = Vec::new();
    let mut cursor = 0;
    while let Some(relative) = xml[cursor..].find('<') {
        let start = cursor + relative;
        let Some(end_relative) = xml[start..].find('>') else {
            break;
        };
        let end = start + end_relative;
        let raw = &xml[start + 1..end];
        if raw.starts_with('/') || raw.starts_with('!') || raw.starts_with('?') {
            cursor = end + 1;
            continue;
        }
        let name_end = raw
            .find(|character: char| character.is_whitespace() || character == '/')
            .unwrap_or(raw.len());
        let raw_name = &raw[..name_end];
        if local_name(raw_name) != wanted {
            cursor = end + 1;
            continue;
        }
        let attrs = raw[name_end..].trim_end_matches('/').trim();
        if raw.trim_end().ends_with('/') {
            found.push(XmlElement { attrs, body: "" });
            cursor = end + 1;
            continue;
        }
        let body_start = end + 1;
        if let Some((close_start, close_end)) = matching_close(xml, body_start, wanted) {
            found.push(XmlElement {
                attrs,
                body: &xml[body_start..close_start],
            });
            cursor = close_end;
        } else {
            found.push(XmlElement {
                attrs,
                body: &xml[body_start..],
            });
            cursor = xml.len();
        }
    }
    found.into_iter()
}

fn elements_with_offsets<'a>(xml: &'a str, wanted: &str) -> Vec<XmlSpan<'a>> {
    let mut found = Vec::new();
    let mut cursor = 0;
    while let Some(relative) = xml[cursor..].find('<') {
        let start = cursor + relative;
        let Some(end_relative) = xml[start..].find('>') else {
            break;
        };
        let end = start + end_relative + 1;
        let raw = &xml[start + 1..end - 1];
        if raw.starts_with('/') || raw.starts_with('!') || raw.starts_with('?') {
            cursor = end;
            continue;
        }
        let name_end = raw
            .find(|character: char| character.is_whitespace() || character == '/')
            .unwrap_or(raw.len());
        if local_name(&raw[..name_end]) == wanted {
            found.push(XmlSpan {
                open_start: start,
                open_end: end,
                attrs: raw[name_end..].trim_end_matches('/').trim(),
            });
        }
        cursor = end;
    }
    found
}

fn matching_close(xml: &str, body_start: usize, wanted: &str) -> Option<(usize, usize)> {
    let mut cursor = body_start;
    let mut depth = 1usize;
    while let Some(relative) = xml[cursor..].find('<') {
        let start = cursor + relative;
        let end = start + xml[start..].find('>')?;
        let raw = &xml[start + 1..end];
        if let Some(stripped) = raw.strip_prefix('/') {
            let name = stripped.trim();
            if local_name(name) == wanted {
                depth -= 1;
                if depth == 0 {
                    return Some((start, end + 1));
                }
            }
        } else if !raw.starts_with('!') && !raw.starts_with('?') {
            let name_end = raw
                .find(|character: char| character.is_whitespace() || character == '/')
                .unwrap_or(raw.len());
            if local_name(&raw[..name_end]) == wanted && !raw.trim_end().ends_with('/') {
                depth += 1;
            }
        }
        cursor = end + 1;
    }
    None
}

fn local_name(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

fn contains_element(xml: &str, wanted: &str) -> bool {
    elements(xml, wanted).next().is_some()
}

fn attr(attrs: &str, wanted: &str) -> Option<String> {
    let mut cursor = 0;
    let bytes = attrs.as_bytes();
    while cursor < bytes.len() {
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if cursor >= bytes.len() {
            break;
        }
        let key_start = cursor;
        while cursor < bytes.len() && !bytes[cursor].is_ascii_whitespace() && bytes[cursor] != b'='
        {
            cursor += 1;
        }
        let key = &attrs[key_start..cursor];
        while cursor < bytes.len() && (bytes[cursor].is_ascii_whitespace() || bytes[cursor] == b'=')
        {
            cursor += 1;
        }
        if cursor >= bytes.len() {
            break;
        }
        let quote = bytes[cursor];
        if quote != b'"' && quote != b'\'' {
            while cursor < bytes.len() && !bytes[cursor].is_ascii_whitespace() {
                cursor += 1;
            }
            continue;
        }
        cursor += 1;
        let value_start = cursor;
        while cursor < bytes.len() && bytes[cursor] != quote {
            cursor += 1;
        }
        let value = attrs[value_start..cursor].to_string();
        if key == wanted {
            return Some(value);
        }
        cursor = cursor.saturating_add(1);
    }
    None
}

fn relationship_map(xml: &str) -> BTreeMap<String, String> {
    elements(xml, "Relationship")
        .filter_map(|tag| {
            Some((
                attr(tag.attrs, "Id")?,
                xml_unescape(&attr(tag.attrs, "Target")?),
            ))
        })
        .collect()
}

fn relationship_part_path(part: &str) -> String {
    let parent = parent_path(part);
    let filename = part.rsplit('/').next().unwrap_or(part);
    if parent.is_empty() {
        format!("_rels/{filename}.rels")
    } else {
        format!("{parent}/_rels/{filename}.rels")
    }
}

fn parent_path(path: &str) -> &str {
    path.rsplit_once('/')
        .map(|(parent, _)| parent)
        .unwrap_or("")
}

fn resolve_target(base_dir: &str, target: &str) -> String {
    let unescaped_target = xml_unescape(target);
    let target = unescaped_target.trim_start_matches('/');
    let mut parts = Vec::new();
    if target.starts_with("xl/") {
        parts.extend(target.split('/'));
    } else {
        parts.extend(base_dir.split('/'));
        parts.extend(target.split('/'));
    }
    let mut normalized = Vec::new();
    for part in parts {
        match part {
            "" | "." => {}
            ".." => {
                normalized.pop();
            }
            value => normalized.push(value),
        }
    }
    normalized.join("/")
}

fn parse_cell_ref(raw: &str) -> Option<CellRef> {
    CellRef::parse(raw.trim())
}

fn column_index(letters: &str) -> u32 {
    letters
        .chars()
        .fold(0u32, |value, character| {
            value
                .saturating_mul(26)
                .saturating_add(character.to_ascii_uppercase() as u32 - 'A' as u32 + 1)
        })
        .saturating_sub(1)
}

fn column_letters(mut col: usize) -> String {
    let mut letters = String::new();
    loop {
        letters.insert(0, (b'A' + (col % 26) as u8) as char);
        if col < 26 {
            break;
        }
        col = col / 26 - 1;
    }
    letters
}

fn chart_sheet_reference(name: &str) -> String {
    format!("'{}'", name.replace('\'', "''"))
}

fn xml_escape_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn xml_escape_attr(value: &str) -> String {
    xml_escape_text(value)
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn xml_unescape(value: &str) -> String {
    value
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&gt;", ">")
        .replace("&lt;", "<")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::{CellAlignment, FillColor};
    use crate::{CellRef, ChartKind, Sheet, SheetChart, SheetObject};

    #[test]
    fn rich_xlsx_roundtrip_preserves_styles_charts_and_embedded_images() {
        let mut sheet = Sheet::new("Sales");
        sheet.set_str("A1", "Quarter");
        sheet.set_str("B1", "Revenue");
        sheet.set_str("A2", "Q1");
        sheet.set_str("B2", "120");
        sheet.set_str("A3", "Q2");
        sheet.set_str("B3", "180");
        sheet.set_cell_alignment(CellRef { row: 0, col: 1 }, CellAlignment::Right);
        let mut style = sheet.cell_style(CellRef { row: 0, col: 1 });
        style.bold = true;
        style.fill = FillColor::Yellow;
        sheet.set_cell_style(CellRef { row: 0, col: 1 }, style);
        sheet.chart = Some(SheetChart {
            kind: ChartKind::Bar,
            title: "Revenue".to_string(),
            cat_col: 0,
            val_col: 1,
        });
        let mut image =
            SheetObject::image(CellRef { row: 4, col: 0 }, "hero.png").expect("image object");
        image.embedded = Some(vec![137, 80, 78, 71]);
        sheet.objects.push(image);

        let bytes = export_xlsx_sheets(&[sheet]).expect("rich export");
        let archive = PackageArchive::from_bytes(&bytes).expect("zip");
        assert!(archive.get("xl/styles.xml").is_some());
        assert!(archive.get("xl/charts/chart1.xml").is_some());
        assert!(archive.get("xl/media/sheet1-object0.png").is_some());

        let sheets = extract_xlsx_sheets(&bytes).expect("rich import");
        assert_eq!(sheets[0].cell_style(CellRef { row: 0, col: 1 }), style);
        assert_eq!(
            sheets[0].cell_alignment(CellRef { row: 0, col: 1 }),
            CellAlignment::Right
        );
        assert_eq!(
            sheets[0].chart.as_ref().map(|chart| chart.kind),
            Some(ChartKind::Bar)
        );
        assert_eq!(
            sheets[0].objects[0].embedded.as_deref(),
            Some(&[137, 80, 78, 71][..])
        );
    }

    #[test]
    fn rich_xlsx_roundtrip_keeps_drawing_relationships_per_sheet() {
        let mut first = Sheet::new("First");
        first.set_str("A1", "one");
        first.chart = Some(SheetChart {
            kind: ChartKind::Line,
            title: "First chart".to_string(),
            cat_col: 0,
            val_col: 1,
        });
        let mut second = Sheet::new("Second");
        second.set_str("A1", "two");
        second.objects.push(SheetObject::shape(
            CellRef { row: 1, col: 1 },
            "Second shape",
        ));

        let bytes = export_xlsx_sheets(&[first, second]).expect("rich export");
        let archive = PackageArchive::from_bytes(&bytes).expect("zip");
        assert!(archive.get("xl/drawings/drawing1.xml").is_some());
        assert!(archive.get("xl/drawings/drawing2.xml").is_some());
        assert!(archive.get("xl/charts/chart1.xml").is_some());
        assert!(archive.get("xl/worksheets/_rels/sheet2.xml.rels").is_some());

        let sheets = extract_xlsx_sheets(&bytes).expect("rich import");
        assert_eq!(sheets.len(), 2);
        assert_eq!(
            sheets[0].chart.as_ref().map(|chart| chart.title.as_str()),
            Some("First chart")
        );
        assert_eq!(sheets[1].objects[0].label, "Second shape");
    }

    #[test]
    fn rich_xlsx_escapes_chart_sheet_references_and_binds_drawing_namespace() {
        let mut sheet = Sheet::new("R&D");
        sheet.set_str("A2", "North");
        sheet.set_str("B2", "10");
        sheet.chart = Some(SheetChart {
            kind: ChartKind::Line,
            title: "Revenue <Q1>".to_string(),
            cat_col: 0,
            val_col: 1,
        });

        let bytes = export_xlsx_sheets(&[sheet]).expect("rich export");
        let archive = PackageArchive::from_bytes(&bytes).expect("zip");
        let worksheet = String::from_utf8_lossy(archive.get("xl/worksheets/sheet1.xml").unwrap());
        assert!(worksheet.contains("<drawing xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" r:id=\"rId1\"/>")
        );

        let chart = String::from_utf8_lossy(archive.get("xl/charts/chart1.xml").unwrap());
        assert!(chart.contains("'R&amp;D'!$A$2:$A$2"));
        assert!(chart.contains("Revenue &lt;Q1&gt;"));
        assert!(!chart.contains("'R&D'!$A$2:$A$2"));

        let imported = extract_xlsx_sheets(&bytes).expect("rich import");
        assert_eq!(imported[0].name, "R&D");
        assert_eq!(
            imported[0].chart.as_ref().map(|chart| chart.cat_col),
            Some(0)
        );
    }
}
