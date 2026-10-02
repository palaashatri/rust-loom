//! Turn the supported parts of an XLSX workbook into worksheet models.

use std::collections::BTreeSet;

use loom_package::zip::PackageArchive;

use crate::style::FillColor;
use crate::{Cell, CellRef, ChartKind, Sheet, SheetChart, SheetObject};

use super::cell_refs::{column_index, parse_cell_ref};
use super::formula_import::{loom_formula, DefinedNames};
use super::import_audit::formula_results_differ;
use super::package_parts::{parent_path, relationship_map, relationship_part_path, resolve_target};
use super::sheet_reader::{
    parse_shared_strings, parse_worksheet, Cached, FormulaKind, ParsedSheet, SharedStrings,
};
use super::style_import::{parse_styles, StyleTable};
use super::styles::fill_from_rgb;
use super::warnings::{chart_plot_groups, imported_chart_group_index, XlsxImportWarning};
use super::xml::{attr, elements, xml_unescape};
use super::EMU_PER_PIXEL;

/// Excel stores column widths as a count of the default font's digits (7 px
/// for Calibri 11, padding included) and row heights in points.
const PIXELS_PER_CHARACTER: f32 = 7.0;
const POINTS_PER_PIXEL: f32 = 0.75;
/// Excel's default row height in points, used when the sheet names none.
const EXCEL_DEFAULT_ROW_POINTS: f32 = 15.0;
/// A `<col>` range covering the whole sheet is limited to the first columns.
const MAX_COLUMNS_PER_SIZE_RANGE: u32 = 256;
/// Cross-checking formula results costs one extra evaluation of the
/// workbook, so very large formula sets are not compared.
const MAX_FORMULAS_TO_COMPARE: usize = 500_000;
/// Functions Loom spills over neighbouring cells, like Excel's dynamic arrays.
const SPILL_FUNCTIONS: &[&str] = &["SEQUENCE", "TRANSPOSE", "SORT", "UNIQUE", "FILTER"];

fn is_spill_formula(body: &str) -> bool {
    let upper = body.trim().to_ascii_uppercase();
    let name = upper
        .strip_prefix("_XLFN._XLWS.")
        .or_else(|| upper.strip_prefix("_XLFN."))
        .unwrap_or(&upper);
    SPILL_FUNCTIONS
        .iter()
        .any(|function| name.starts_with(&format!("{function}(")))
}

/// Text that Loom, which has no separate text type for cells, reads as a
/// number, a boolean or a formula.
fn looks_like_value(text: &str) -> bool {
    let text = text.trim();
    text.parse::<f64>().is_ok()
        || text.eq_ignore_ascii_case("true")
        || text.eq_ignore_ascii_case("false")
        || text.starts_with('=')
}

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
    /// `state="hidden"` or `"veryHidden"` in `workbook.xml`.
    pub(super) hidden: bool,
}

#[derive(Debug, Clone, Copy)]
struct DrawingAnchor {
    cell: CellRef,
    width: u32,
    height: u32,
}

struct Extracted {
    sheets: Vec<Sheet>,
    /// Features found while reading cells, formats and layout.
    warnings: BTreeSet<XlsxImportWarning>,
}

/// Import an `.xlsx` workbook into the full worksheet model used by the app.
///
/// The importer keeps formulas and cell values, then adds the OOXML features
/// represented by the Loom model: cell styles, alignments, column widths, row
/// heights, frozen panes, basic charts, shapes, and embedded images. Excel
/// pivot caches remain usable as their cached worksheet cells, because Loom's
/// native pivot model is formula-backed rather than an OOXML cache object.
pub fn extract_xlsx_sheets(xlsx_bytes: &[u8]) -> Result<Vec<Sheet>, String> {
    let archive = PackageArchive::from_bytes(xlsx_bytes)
        .map_err(|e| format!("unreadable xlsx archive: {e}"))?;
    Ok(extract_from_archive(&archive)?.sheets)
}

/// Read `<definedName>` entries as `(name, refersTo, has_sheet_scope)`.
fn defined_name_entries(archive: &PackageArchive) -> Vec<(String, String, bool)> {
    let Some(workbook) = archive
        .get("xl/workbook.xml")
        .and_then(|bytes| std::str::from_utf8(bytes).ok())
    else {
        return Vec::new();
    };
    elements(workbook, "definedName")
        .filter_map(|tag| {
            Some((
                xml_unescape(&attr(tag.attrs, "name")?),
                xml_unescape(tag.body.trim()),
                attr(tag.attrs, "localSheetId").is_some(),
            ))
        })
        .collect()
}

fn cached_text(value: &Cached) -> Option<String> {
    match value {
        Cached::Empty => None,
        Cached::Text(text) | Cached::Error(text) => Some(text.clone()),
        Cached::Number(number) => Some(format!("{number}")),
        Cached::Bool(flag) => Some(if *flag { "TRUE" } else { "FALSE" }.to_string()),
    }
}

/// Fill `sheet` from the parsed part. Returns the formula cells with the
/// value Excel last calculated for them.
fn fill_sheet(
    sheet: &mut Sheet,
    parsed: &ParsedSheet,
    styles: &StyleTable,
    names: &DefinedNames,
    warnings: &mut BTreeSet<XlsxImportWarning>,
) -> Vec<(CellRef, Cached)> {
    let mut shared_formulas = crate::interop::SharedFormulaIndex::default();
    for cell in &parsed.cells {
        if let Some(formula) = &cell.formula {
            if let (Some(id), Some(body)) = (formula.shared_id, formula.body.as_deref()) {
                if formula.kind == FormulaKind::Shared {
                    shared_formulas.insert_master(id, cell.at, &loom_formula(body, names));
                }
            }
        }
    }
    // A dynamic-array formula (`=SEQUENCE(2,2)`) is stored with the values it
    // spilled. Loom spills such formulas itself, and stored values in the
    // spill range would block it, so the spilled cells are not imported.
    let mut spilled = std::collections::HashSet::new();
    for cell in &parsed.cells {
        let Some(formula) = cell
            .formula
            .as_ref()
            .filter(|f| f.kind == FormulaKind::Array)
        else {
            continue;
        };
        let (Some(body), Some((first, last))) = (
            formula.body.as_deref(),
            formula.array_ref.as_deref().and_then(|r| r.split_once(':')),
        ) else {
            continue;
        };
        let spills = is_spill_formula(body);
        if let (true, Some(a), Some(b)) = (spills, parse_cell_ref(first), parse_cell_ref(last)) {
            for row in a.row..=b.row {
                for col in a.col..=b.col {
                    spilled.insert(CellRef { row, col });
                }
            }
            spilled.remove(&cell.at);
        }
    }
    let mut calculated = Vec::new();
    for cell in &parsed.cells {
        let formula_text = cell.formula.as_ref().and_then(|formula| {
            if formula.kind == FormulaKind::Array
                && formula
                    .array_ref
                    .as_deref()
                    .is_some_and(|reference| reference.split_once(':').is_some_and(|(a, b)| a != b))
                && !spilled.contains(&cell.at)
                && formula
                    .body
                    .as_deref()
                    .is_some_and(|body| !is_spill_formula(body))
            {
                warnings.insert(XlsxImportWarning::ArrayFormulas);
            }
            match (formula.kind, formula.shared_id, formula.body.as_deref()) {
                (FormulaKind::Shared, Some(id), _) => shared_formulas
                    .resolve(id, cell.at)
                    .map(|text| text.trim_start_matches('=').to_string()),
                (_, _, Some(body)) => Some(loom_formula(body, names)),
                _ => None,
            }
        });
        let raw = match formula_text {
            Some(text) => {
                calculated.push((cell.at, cell.value.clone()));
                Some(format!("={}", text.trim()))
            }
            None if cell.formula.is_none() && spilled.contains(&cell.at) => None,
            None => {
                if matches!(&cell.value, Cached::Text(text) if looks_like_value(text)) {
                    warnings.insert(XlsxImportWarning::TextReadAsValue);
                }
                cached_text(&cell.value)
            }
        };
        if let Some(raw) = raw {
            sheet.cells.insert(cell.at, Cell { raw });
        }
        let xf = styles.xfs.get(cell.style).copied().unwrap_or_default();
        if !xf.style.is_default() {
            sheet.set_cell_style(cell.at, xf.style);
        }
        if xf.alignment != crate::CellAlignment::General {
            sheet.set_cell_alignment(cell.at, xf.alignment);
        }
        if xf.number_lossy {
            warnings.insert(XlsxImportWarning::UnsupportedNumberFormats);
        }
        if xf.approximated {
            warnings.insert(XlsxImportWarning::ApproximatedFormatting);
        }
    }

    for column in &parsed.columns {
        if column.hidden {
            warnings.insert(XlsxImportWarning::HiddenContent);
            continue;
        }
        if let (true, Some(width)) = (column.custom, column.width) {
            let last = column
                .last
                .min(column.first.saturating_add(MAX_COLUMNS_PER_SIZE_RANGE - 1));
            for col in column.first..=last {
                sheet.set_col_width(col, width * PIXELS_PER_CHARACTER);
            }
        }
    }
    let default_points = parsed
        .default_row_height
        .unwrap_or(EXCEL_DEFAULT_ROW_POINTS);
    for row in &parsed.rows {
        if row.hidden {
            warnings.insert(XlsxImportWarning::HiddenContent);
            continue;
        }
        if let Some(height) = row.height {
            if row.custom || (height - default_points).abs() > 0.01 {
                sheet.set_row_height(row.row, height / POINTS_PER_PIXEL);
            }
        }
    }
    sheet.freeze_panes(parsed.frozen_rows, parsed.frozen_columns);
    if parsed.merged_ranges > 0 {
        warnings.insert(XlsxImportWarning::MergedCells);
    }
    calculated
}

fn extract_from_archive(archive: &PackageArchive) -> Result<Extracted, String> {
    let parts = workbook_sheet_parts(archive)?;
    if parts.is_empty() {
        return Err("xlsx workbook has no worksheets".to_string());
    }
    let shared = match archive.get("xl/sharedStrings.xml") {
        Some(bytes) => parse_shared_strings(
            std::str::from_utf8(bytes)
                .map_err(|_| "xl/sharedStrings.xml is not valid UTF-8".to_string())?,
        )?,
        None => SharedStrings::default(),
    };
    let styles = archive
        .get("xl/styles.xml")
        .and_then(|bytes| std::str::from_utf8(bytes).ok())
        .map(parse_styles)
        .unwrap_or_default();
    let names = DefinedNames::from_entries(defined_name_entries(archive));

    let mut warnings = BTreeSet::new();
    if shared.rich {
        warnings.insert(XlsxImportWarning::RichTextRuns);
    }
    let mut sheets = Vec::with_capacity(parts.len());
    let mut calculated = Vec::with_capacity(parts.len());
    for part in &parts {
        let parsed = parse_worksheet(&part.path, &part.xml, &shared)?;
        let mut sheet = Sheet::new(&part.name);
        if part.hidden {
            warnings.insert(XlsxImportWarning::HiddenContent);
        }
        calculated.push(fill_sheet(
            &mut sheet,
            &parsed,
            &styles,
            &names,
            &mut warnings,
        ));
        import_drawings(&mut sheet, &part.xml, &part.path, archive)?;
        sheets.push(sheet);
    }
    let formula_count: usize = calculated.iter().map(Vec::len).sum();
    if formula_count <= MAX_FORMULAS_TO_COMPARE && formula_results_differ(&sheets, &calculated) {
        warnings.insert(XlsxImportWarning::FormulaResultsDiffer);
    }
    Ok(Extracted { sheets, warnings })
}

/// Parse an XLSX workbook and list model features that cannot be preserved.
pub fn import_xlsx_sheets(xlsx_bytes: &[u8]) -> Result<XlsxImport, String> {
    let archive = PackageArchive::from_bytes(xlsx_bytes)
        .map_err(|e| format!("unreadable xlsx archive: {e}"))?;
    let mut warnings: BTreeSet<XlsxImportWarning> =
        super::warnings::detect_import_warnings(&archive)?
            .into_iter()
            .collect();
    let extracted = extract_from_archive(&archive)?;
    warnings.extend(extracted.warnings);
    Ok(XlsxImport {
        sheets: extracted.sheets,
        warnings: warnings.into_iter().collect(),
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
            hidden: false,
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
            hidden: matches!(
                attr(tag.attrs, "state").as_deref(),
                Some("hidden") | Some("veryHidden")
            ),
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
                            if let Some(chart) = parse_chart(&chart_path, chart_xml)? {
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
    // Excel's drawing XML holds several `a:ext` elements: the real size is the
    // one with `cx`/`cy`; `a:ext uri=...` extension entries have neither.
    let emu = |tag: &super::xml::XmlElement<'_>, name: &str| {
        attr(tag.attrs, name)
            .and_then(|value| value.parse::<f32>().ok())
            .filter(|value| *value > 0.0)
            .map(|value| (value / EMU_PER_PIXEL).round().max(1.0) as u32)
    };
    let sized = elements(body, "ext").find_map(|tag| Some((emu(&tag, "cx")?, emu(&tag, "cy")?)));
    let (width, height) = match (sized, elements(body, "to").next()) {
        (Some(size), _) => size,
        (None, Some(to)) => {
            let end = parse_marker(to.body);
            (
                ((end.col.saturating_sub(cell.col) + 1) as f32 * crate::DEFAULT_COL_WIDTH).round()
                    as u32,
                ((end.row.saturating_sub(cell.row) + 1) as f32 * crate::DEFAULT_ROW_HEIGHT).round()
                    as u32,
            )
        }
        (None, None) => (240, 112),
    };
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

fn parse_chart(path: &str, xml: &str) -> Result<Option<SheetChart>, String> {
    let chart_groups = chart_plot_groups(path, xml)?;
    let Some(imported_group_index) = imported_chart_group_index(&chart_groups) else {
        return Ok(None);
    };
    let imported_group = &chart_groups[imported_group_index];
    let kind = match imported_group.name.as_str() {
        "pieChart" => ChartKind::Pie,
        "scatterChart" => ChartKind::Scatter,
        "lineChart" => ChartKind::Line,
        "barChart" => ChartKind::Bar,
        _ => unreachable!("the import priority only selects supported chart groups"),
    };
    let title = elements(xml, "t")
        .next()
        .map(|tag| xml_unescape(tag.body.trim()))
        .unwrap_or_else(|| "Chart".to_string());
    let first_series = imported_group.first_series_body.unwrap_or_default();
    let (category_tag, value_tag) = if kind == ChartKind::Scatter {
        ("xVal", "yVal")
    } else {
        ("cat", "val")
    };
    let category_formula = elements(first_series, category_tag)
        .next()
        .and_then(|category| elements(category.body, "f").next())
        .map(|formula| xml_unescape(formula.body.trim()));
    let value_formula = elements(first_series, value_tag)
        .next()
        .and_then(|value| elements(value.body, "f").next())
        .map(|formula| xml_unescape(formula.body.trim()));
    let cat_col = category_formula
        .as_deref()
        .and_then(formula_column)
        .unwrap_or(0);
    let val_col = value_formula
        .as_deref()
        .and_then(formula_column)
        .unwrap_or(cat_col.saturating_add(1));
    let rows = category_formula.as_deref().and_then(|formula| {
        let range = formula.rsplit('!').next()?.replace('$', "");
        let (first, last) = range.split_once(':')?;
        Some((CellRef::parse(first)?.row, CellRef::parse(last)?.row))
    });
    Ok(Some(SheetChart {
        start_row: rows.map_or(1, |rows| rows.0),
        end_row: rows.map(|rows| rows.1),
        kind,
        title,
        cat_col,
        val_col,
    }))
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
