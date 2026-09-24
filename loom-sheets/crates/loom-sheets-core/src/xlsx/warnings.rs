//! Detect XLSX features Loom cannot keep when importing a workbook.

use std::collections::{BTreeMap, BTreeSet};

use loom_package::zip::PackageArchive;

use super::import::workbook_sheet_parts;
use super::package_parts::{
    parent_path, parse_relationships, relationship_part_path, resolve_target, OoxmlRelationship,
};
use super::xml::parse_xml_nodes;
use super::{CHART_NS, DRAWINGML_NS};

/// A known XLSX feature that the Loom workbook model cannot preserve.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum XlsxImportWarning {
    DefinedNames,
    ExternalLinks,
    ConditionalFormatting,
    DataValidation,
    FrozenPanes,
    CustomRowColumnSizes,
    MultipleChartsOnSheet,
    MissingDrawingParts,
    PivotTables,
}

impl XlsxImportWarning {
    /// Text suitable for a user-facing import confirmation.
    pub fn label(self) -> &'static str {
        match self {
            Self::DefinedNames => "defined names and named ranges",
            Self::ExternalLinks => "links to other workbooks",
            Self::ConditionalFormatting => "conditional formatting rules",
            Self::DataValidation => "data validation rules",
            Self::FrozenPanes => "frozen rows or columns",
            Self::CustomRowColumnSizes => "custom row heights or column widths",
            Self::MultipleChartsOnSheet => "additional charts on the same sheet",
            Self::MissingDrawingParts => "drawing objects with missing linked parts",
            Self::PivotTables => "Excel PivotTables (only their cached cell values are imported)",
        }
    }
}

pub(super) fn detect_import_warnings(
    archive: &PackageArchive,
) -> Result<Vec<XlsxImportWarning>, String> {
    let mut warnings = BTreeSet::new();
    if let Some(workbook) = archive.get("xl/workbook.xml") {
        let workbook_xml = std::str::from_utf8(workbook)
            .map_err(|_| "xl/workbook.xml is not valid UTF-8".to_string())?;
        let nodes = parse_xml_nodes("xl/workbook.xml", workbook_xml)?;
        if nodes
            .iter()
            .any(|node| is_spreadsheet_namespace(&node.namespace) && node.name == "definedName")
        {
            warnings.insert(XlsxImportWarning::DefinedNames);
        }
        if nodes.iter().any(|node| {
            is_spreadsheet_namespace(&node.namespace)
                && (node.name == "pivotTableParts" || node.name == "pivotSource")
        }) {
            warnings.insert(XlsxImportWarning::PivotTables);
        }
    }

    let workbook_relationships = archive
        .get("xl/_rels/workbook.xml.rels")
        .map(|bytes| parse_relationships("xl/_rels/workbook.xml.rels", bytes))
        .transpose()?
        .unwrap_or_default();
    if workbook_relationships
        .iter()
        .any(|relationship| relationship.kind.ends_with("/externalLink"))
        || archive
            .paths()
            .iter()
            .any(|path| path.starts_with("xl/externalLinks/"))
    {
        warnings.insert(XlsxImportWarning::ExternalLinks);
    }
    if workbook_relationships.iter().any(|relationship| {
        relationship.kind.ends_with("/pivotTable")
            || relationship.kind.ends_with("/pivotCacheDefinition")
            || relationship.kind.ends_with("/pivotCacheRecords")
    }) || archive
        .paths()
        .iter()
        .any(|path| path.starts_with("xl/pivotTables/") || path.starts_with("xl/pivotCache/"))
    {
        warnings.insert(XlsxImportWarning::PivotTables);
    }

    for sheet in workbook_sheet_parts(archive)? {
        let nodes = parse_xml_nodes(&sheet.path, &sheet.xml)?;
        for node in &nodes {
            let is_main = is_spreadsheet_namespace(&node.namespace);
            let is_x14 = node.namespace == X14_SPREADSHEET_NS;
            if is_main && node.name == "conditionalFormatting"
                || is_x14 && node.name == "conditionalFormatting"
            {
                warnings.insert(XlsxImportWarning::ConditionalFormatting);
            }
            if (is_main || is_x14) && node.name == "dataValidation" {
                warnings.insert(XlsxImportWarning::DataValidation);
            }
            if is_main && node.name == "pane" {
                let split = ["xSplit", "ySplit"]
                    .iter()
                    .filter_map(|attribute| node.attribute(attribute))
                    .filter_map(|value| value.parse::<f64>().ok())
                    .any(|value| value != 0.0);
                let frozen = node
                    .attribute("state")
                    .is_some_and(|state| state == "frozen" || state == "frozenSplit");
                if split || frozen {
                    warnings.insert(XlsxImportWarning::FrozenPanes);
                }
            }
            if is_main
                && ((node.name == "col" && node.attribute("width").is_some())
                    || (node.name == "row" && node.attribute("ht").is_some()))
            {
                warnings.insert(XlsxImportWarning::CustomRowColumnSizes);
            }
        }

        let Some(drawing) = nodes
            .iter()
            .find(|node| is_spreadsheet_namespace(&node.namespace) && node.name == "drawing")
        else {
            continue;
        };
        let Some(drawing_id) = drawing
            .attribute("id")
            .or_else(|| drawing.attributes.get("r:id").map(String::as_str))
        else {
            warnings.insert(XlsxImportWarning::MissingDrawingParts);
            continue;
        };
        let sheet_rels_path = relationship_part_path(&sheet.path);
        let Some(sheet_rels_bytes) = archive.get(&sheet_rels_path) else {
            warnings.insert(XlsxImportWarning::MissingDrawingParts);
            continue;
        };
        let sheet_relationships = parse_relationships(&sheet_rels_path, sheet_rels_bytes)?;
        let Some(drawing_rel) = sheet_relationships.iter().find(|rel| rel.id == drawing_id) else {
            warnings.insert(XlsxImportWarning::MissingDrawingParts);
            continue;
        };
        if !drawing_rel.kind.ends_with("/drawing") {
            warnings.insert(XlsxImportWarning::MissingDrawingParts);
            continue;
        }
        let drawing_path = resolve_target(parent_path(&sheet.path), &drawing_rel.target);
        let Some(drawing_bytes) = archive.get(&drawing_path) else {
            warnings.insert(XlsxImportWarning::MissingDrawingParts);
            continue;
        };
        let drawing_xml = std::str::from_utf8(drawing_bytes)
            .map_err(|_| format!("{drawing_path} is not valid UTF-8"))?;
        let drawing_nodes = parse_xml_nodes(&drawing_path, drawing_xml)?;
        let drawing_rels_path = relationship_part_path(&drawing_path);
        let drawing_relationships = archive
            .get(&drawing_rels_path)
            .map(|bytes| parse_relationships(&drawing_rels_path, bytes))
            .transpose()?
            .unwrap_or_default();
        let mut charts = 0usize;
        for node in &drawing_nodes {
            if node.namespace == CHART_NS && node.name == "chart" {
                charts += 1;
                if !drawing_target_exists(
                    &drawing_relationships,
                    &node.attributes,
                    "chart",
                    &drawing_path,
                    archive,
                ) {
                    warnings.insert(XlsxImportWarning::MissingDrawingParts);
                }
            } else if node.namespace == DRAWINGML_NS
                && node.name == "blip"
                && !drawing_target_exists(
                    &drawing_relationships,
                    &node.attributes,
                    "image",
                    &drawing_path,
                    archive,
                )
            {
                warnings.insert(XlsxImportWarning::MissingDrawingParts);
            }
        }
        if charts > 1 {
            warnings.insert(XlsxImportWarning::MultipleChartsOnSheet);
        }
    }

    Ok(warnings.into_iter().collect())
}

const SPREADSHEET_NS: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const STRICT_SPREADSHEET_NS: &str = "http://purl.oclc.org/ooxml/spreadsheetml/main";
const X14_SPREADSHEET_NS: &str = "http://schemas.microsoft.com/office/spreadsheetml/2009/9/main";
fn is_spreadsheet_namespace(namespace: &str) -> bool {
    namespace == SPREADSHEET_NS || namespace == STRICT_SPREADSHEET_NS
}

fn drawing_target_exists(
    relationships: &[OoxmlRelationship],
    attributes: &BTreeMap<String, String>,
    expected_kind: &str,
    drawing_path: &str,
    archive: &PackageArchive,
) -> bool {
    let relationship_id = attributes
        .get("r:id")
        .or_else(|| attributes.get("r:embed"))
        .or_else(|| attributes.get("id"))
        .or_else(|| attributes.get("embed"));
    let Some(relationship_id) = relationship_id else {
        return false;
    };
    relationships
        .iter()
        .find(|relationship| relationship.id == *relationship_id)
        .filter(|relationship| relationship.kind.ends_with(&format!("/{expected_kind}")))
        .is_some_and(|relationship| {
            let target = resolve_target(parent_path(drawing_path), &relationship.target);
            archive.get(&target).is_some()
        })
}
