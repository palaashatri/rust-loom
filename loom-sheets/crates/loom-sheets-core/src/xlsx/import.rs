//! Turn the supported parts of an XLSX workbook into worksheet models.

use loom_package::zip::PackageArchive;

use crate::style::FillColor;
use crate::{CellRef, ChartKind, Sheet, SheetChart, SheetObject};

use super::cell_refs::column_index;
use super::package_parts::{parent_path, relationship_map, relationship_part_path, resolve_target};
use super::styles::{apply_imported_styles, fill_from_rgb, parse_styles};
use super::warnings::XlsxImportWarning;
use super::xml::{attr, contains_element, elements, xml_unescape};
use super::EMU_PER_PIXEL;

/// Parsed worksheet model plus known features that will be omitted by import.
#[derive(Debug, Clone)]
pub struct XlsxImport {
    pub sheets: Vec<Sheet>,
    pub warnings: Vec<XlsxImportWarning>,
}

#[derive(Debug, Clone)]
pub(super) struct SheetPart {
    pub(super) name: String,
    pub(super) path: String,
    pub(super) xml: String,
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
    extract_xlsx_sheets_from_archive(xlsx_bytes, &archive)
}

fn extract_xlsx_sheets_from_archive(
    xlsx_bytes: &[u8],
    archive: &PackageArchive,
) -> Result<Vec<Sheet>, String> {
    let basic = crate::extract_xlsx_workbook(xlsx_bytes)?;
    let parts = workbook_sheet_parts(archive)?;
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
        import_drawings(&mut sheet, &part.xml, &part.path, archive)?;
        sheets.push(sheet);
    }
    Ok(sheets)
}

/// Parse an XLSX workbook and list model features that cannot be preserved.
pub fn import_xlsx_sheets(xlsx_bytes: &[u8]) -> Result<XlsxImport, String> {
    let archive = PackageArchive::from_bytes(xlsx_bytes)
        .map_err(|e| format!("unreadable xlsx archive: {e}"))?;
    let warnings = super::warnings::detect_import_warnings(&archive)?;
    Ok(XlsxImport {
        sheets: extract_xlsx_sheets_from_archive(xlsx_bytes, &archive)?,
        warnings,
    })
}

pub(super) fn workbook_sheet_parts(archive: &PackageArchive) -> Result<Vec<SheetPart>, String> {
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
            width = ((end.col.saturating_sub(cell.col) + 1) as f32 * crate::DEFAULT_COL_WIDTH)
                .round() as u32;
            height = ((end.row.saturating_sub(cell.row) + 1) as f32 * crate::DEFAULT_ROW_HEIGHT)
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
    let rows = formulas.first().and_then(|formula| {
        let range = formula.rsplit('!').next()?.replace('$', "");
        let (first, last) = range.split_once(':')?;
        Some((CellRef::parse(first)?.row, CellRef::parse(last)?.row))
    });
    Some(SheetChart {
        start_row: rows.map_or(1, |rows| rows.0),
        end_row: rows.map(|rows| rows.1),
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
