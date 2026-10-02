//! Detect XLSX features Loom cannot keep when importing a workbook.

use std::collections::{BTreeMap, BTreeSet};

use loom_package::zip::PackageArchive;
use quick_xml::events::{BytesStart, Event};
use quick_xml::name::ResolveResult;
use quick_xml::NsReader;

use super::import::workbook_sheet_parts;
use super::package_parts::{
    parent_path, parse_relationships, relationship_part_path, resolve_target, OoxmlRelationship,
};
use super::xml::{attr, elements, parse_xml_nodes, xml_unescape};
use super::{CHART_NS, DRAWINGML_NS, DRAWING_NS, REL_NS};

const STRICT_CHART_NS: &str = "http://purl.oclc.org/ooxml/drawingml/chart";
const STRICT_DRAWING_NS: &str = "http://purl.oclc.org/ooxml/drawingml/spreadsheetDrawing";
const STRICT_DRAWINGML_NS: &str = "http://purl.oclc.org/ooxml/drawingml/main";
const STRICT_REL_NS: &str = "http://purl.oclc.org/ooxml/officeDocument/relationships";
const MARKUP_COMPATIBILITY_NS: &str = "http://schemas.openxmlformats.org/markup-compatibility/2006";

/// A chart group kind recognized in SpreadsheetML chart parts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum XlsxChartType {
    Area,
    Area3D,
    Bar,
    Bar3D,
    Bubble,
    Doughnut,
    Line,
    Line3D,
    OfPie,
    Pie,
    Pie3D,
    Radar,
    Scatter,
    Stock,
    Surface,
    Surface3D,
    Other,
}

impl XlsxChartType {
    fn from_tag(tag: &str) -> Self {
        match tag.strip_suffix("Chart") {
            Some("area") => Self::Area,
            Some("area3D") => Self::Area3D,
            Some("bar") => Self::Bar,
            Some("bar3D") => Self::Bar3D,
            Some("bubble") => Self::Bubble,
            Some("doughnut") => Self::Doughnut,
            Some("line") => Self::Line,
            Some("line3D") => Self::Line3D,
            Some("ofPie") => Self::OfPie,
            Some("pie") => Self::Pie,
            Some("pie3D") => Self::Pie3D,
            Some("radar") => Self::Radar,
            Some("scatter") => Self::Scatter,
            Some("stock") => Self::Stock,
            Some("surface") => Self::Surface,
            Some("surface3D") => Self::Surface3D,
            _ => Self::Other,
        }
    }

    fn is_supported(self) -> bool {
        matches!(self, Self::Bar | Self::Line | Self::Pie | Self::Scatter)
    }

    fn unsupported_label(self) -> &'static str {
        match self {
            Self::Area => "unsupported area charts",
            Self::Area3D => "unsupported 3-D area charts",
            Self::Bar => "unsupported bar charts",
            Self::Bar3D => "unsupported 3-D bar charts",
            Self::Bubble => "unsupported bubble charts",
            Self::Doughnut => "unsupported doughnut charts",
            Self::Line => "unsupported line charts",
            Self::Line3D => "unsupported 3-D line charts",
            Self::OfPie => "unsupported of-pie charts",
            Self::Pie => "unsupported pie charts",
            Self::Pie3D => "unsupported 3-D pie charts",
            Self::Radar => "unsupported radar charts",
            Self::Scatter => "unsupported scatter charts",
            Self::Stock => "unsupported stock charts",
            Self::Surface => "unsupported surface charts",
            Self::Surface3D => "unsupported 3-D surface charts",
            Self::Other => "unsupported unrecognized chart types",
        }
    }

    fn dropped_plot_label(self) -> &'static str {
        match self {
            Self::Area => "additional area chart plots",
            Self::Area3D => "additional 3-D area chart plots",
            Self::Bar => "additional bar chart plots",
            Self::Bar3D => "additional 3-D bar chart plots",
            Self::Bubble => "additional bubble chart plots",
            Self::Doughnut => "additional doughnut chart plots",
            Self::Line => "additional line chart plots",
            Self::Line3D => "additional 3-D line chart plots",
            Self::OfPie => "additional of-pie chart plots",
            Self::Pie => "additional pie chart plots",
            Self::Pie3D => "additional 3-D pie chart plots",
            Self::Radar => "additional radar chart plots",
            Self::Scatter => "additional scatter chart plots",
            Self::Stock => "additional stock chart plots",
            Self::Surface => "additional surface chart plots",
            Self::Surface3D => "additional 3-D surface chart plots",
            Self::Other => "additional unrecognized chart plots",
        }
    }

    fn dropped_series_label(self) -> &'static str {
        match self {
            Self::Area => "additional area chart series",
            Self::Area3D => "additional 3-D area chart series",
            Self::Bar => "additional bar chart series",
            Self::Bar3D => "additional 3-D bar chart series",
            Self::Bubble => "additional bubble chart series",
            Self::Doughnut => "additional doughnut chart series",
            Self::Line => "additional line chart series",
            Self::Line3D => "additional 3-D line chart series",
            Self::OfPie => "additional of-pie chart series",
            Self::Pie => "additional pie chart series",
            Self::Pie3D => "additional 3-D pie chart series",
            Self::Radar => "additional radar chart series",
            Self::Scatter => "additional scatter chart series",
            Self::Stock => "additional stock chart series",
            Self::Surface => "additional surface chart series",
            Self::Surface3D => "additional 3-D surface chart series",
            Self::Other => "additional unrecognized chart series",
        }
    }
}

/// A known XLSX feature that the Loom workbook model cannot preserve.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum XlsxImportWarning {
    DefinedNames,
    ExternalLinks,
    ConditionalFormatting,
    DataValidation,
    MergedCells,
    HiddenContent,
    ArrayFormulas,
    UnsupportedNumberFormats,
    ApproximatedFormatting,
    RichTextRuns,
    TextReadAsValue,
    NotesAndLinks,
    TablesAndFilters,
    FormulaResultsDiffer,
    Date1904System,
    MultipleChartsOnSheet,
    MissingDrawingParts,
    AbsoluteDrawingAnchors,
    PivotTables,
    UnsupportedChart(XlsxChartType),
    DroppedChartPlot(XlsxChartType),
    DroppedChartSeries(XlsxChartType),
}

impl XlsxImportWarning {
    /// Text suitable for a user-facing import confirmation.
    pub fn label(self) -> &'static str {
        match self {
            Self::DefinedNames => "defined names and named ranges",
            Self::ExternalLinks => "links to other workbooks",
            Self::ConditionalFormatting => "conditional formatting rules",
            Self::DataValidation => "data validation rules",
            Self::MergedCells => "merged cells (they import as separate cells)",
            Self::HiddenContent => "hidden sheets, rows or columns (they import visible)",
            Self::ArrayFormulas => {
                "array formulas over several cells (only the first cell keeps the formula)"
            }
            Self::UnsupportedNumberFormats => {
                "time, fraction or non-dollar currency number formats (shown as plain numbers or dates)"
            }
            Self::ApproximatedFormatting => {
                "fonts, text colours, wrapped text, exact fill colours and border styles"
            }
            Self::RichTextRuns => "formatting inside part of a cell's text",
            Self::NotesAndLinks => "cell comments, notes and hyperlinks",
            Self::TablesAndFilters => "Excel tables and filters",
            Self::TextReadAsValue => {
                "text cells that look like numbers, TRUE/FALSE or formulas (Loom reads them as those values)"
            }
            Self::FormulaResultsDiffer => {
                "formulas Loom cannot calculate yet or calculates differently from Excel"
            }
            Self::Date1904System => "the 1904 date system (dates may be off by four years)",
            Self::MultipleChartsOnSheet => "additional charts on the same sheet",
            Self::MissingDrawingParts => "drawing objects with missing linked parts",
            Self::AbsoluteDrawingAnchors => "objects positioned with absolute anchors",
            Self::PivotTables => "Excel PivotTables (only their cached cell values are imported)",
            Self::UnsupportedChart(chart_type) => chart_type.unsupported_label(),
            Self::DroppedChartPlot(chart_type) => chart_type.dropped_plot_label(),
            Self::DroppedChartSeries(chart_type) => chart_type.dropped_series_label(),
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
                && node.name == "workbookPr"
                && matches!(node.attribute("date1904"), Some("1") | Some("true"))
        }) {
            warnings.insert(XlsxImportWarning::Date1904System);
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
    let paths = archive.paths();
    if paths
        .iter()
        .any(|path| path.starts_with("xl/comments") || path.starts_with("xl/threadedComments/"))
    {
        warnings.insert(XlsxImportWarning::NotesAndLinks);
    }
    if paths.iter().any(|path| path.starts_with("xl/tables/")) {
        warnings.insert(XlsxImportWarning::TablesAndFilters);
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
            if is_main && node.name == "hyperlink" {
                warnings.insert(XlsxImportWarning::NotesAndLinks);
            }
            if is_main && node.name == "autoFilter" {
                warnings.insert(XlsxImportWarning::TablesAndFilters);
            }
            if (is_main || is_x14) && node.name == "dataValidation" {
                warnings.insert(XlsxImportWarning::DataValidation);
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
        if drawing_has_unpreserved_absolute_anchor(&drawing_path, drawing_xml)? {
            warnings.insert(XlsxImportWarning::AbsoluteDrawingAnchors);
        }
        let drawing_rels_path = relationship_part_path(&drawing_path);
        let drawing_relationships = archive
            .get(&drawing_rels_path)
            .map(|bytes| parse_relationships(&drawing_rels_path, bytes))
            .transpose()?
            .unwrap_or_default();
        let mut charts = 0usize;
        for node in &drawing_nodes {
            if is_chart_namespace(&node.namespace) && node.name == "chart" {
                charts += 1;
                let chart_path = drawing_target_path(
                    &drawing_relationships,
                    &node.attributes,
                    "chart",
                    &drawing_path,
                );
                let Some(chart_path) = chart_path.filter(|path| archive.get(path).is_some()) else {
                    warnings.insert(XlsxImportWarning::MissingDrawingParts);
                    continue;
                };
                let Some(chart_bytes) = archive.get(&chart_path) else {
                    warnings.insert(XlsxImportWarning::MissingDrawingParts);
                    continue;
                };
                let chart_xml = std::str::from_utf8(chart_bytes)
                    .map_err(|_| format!("{chart_path} is not valid UTF-8"))?;
                let chart_groups = chart_plot_groups(&chart_path, chart_xml)?;
                if chart_groups.is_empty() {
                    warnings.insert(XlsxImportWarning::UnsupportedChart(XlsxChartType::Other));
                }
                let imported_index = imported_chart_group_index(&chart_groups);
                if let Some(group) = imported_index.and_then(|index| chart_groups.get(index)) {
                    if group.series_count > 1 {
                        warnings.insert(XlsxImportWarning::DroppedChartSeries(
                            XlsxChartType::from_tag(&group.name),
                        ));
                    }
                }
                if chart_groups.len() == 1 {
                    if let Some(chart_type) = unsupported_chart_type(&chart_groups[0].name) {
                        warnings.insert(XlsxImportWarning::UnsupportedChart(chart_type));
                    }
                } else if chart_groups.len() > 1 {
                    for (index, group) in chart_groups.iter().enumerate() {
                        if Some(index) == imported_index {
                            continue;
                        }
                        let chart_type = XlsxChartType::from_tag(&group.name);
                        warnings.insert(if chart_type.is_supported() {
                            XlsxImportWarning::DroppedChartPlot(chart_type)
                        } else {
                            XlsxImportWarning::UnsupportedChart(chart_type)
                        });
                    }
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

fn is_chart_namespace(namespace: &str) -> bool {
    namespace == CHART_NS || namespace == STRICT_CHART_NS
}

fn is_spreadsheet_drawing_namespace(namespace: &str) -> bool {
    namespace == DRAWING_NS || namespace == STRICT_DRAWING_NS
}

fn drawing_has_unpreserved_absolute_anchor(path: &str, xml: &str) -> Result<bool, String> {
    #[derive(Clone, PartialEq, Eq)]
    struct DrawingObjectIdentity {
        kind: String,
        id: String,
        name: String,
        description: Option<String>,
        width: Option<String>,
        height: Option<String>,
        imported_payload: Vec<String>,
    }

    #[derive(Default)]
    struct AlternateContent {
        choices_supported: Vec<bool>,
        fallback_objects: Vec<DrawingObjectIdentity>,
    }

    #[derive(Clone, Copy)]
    enum AnchorKind {
        Absolute,
        Supported,
    }

    struct AnchorCapture {
        kind: AnchorKind,
        choices: Vec<(usize, usize)>,
        fallbacks: Vec<usize>,
        start_byte: usize,
    }

    struct AbsoluteCandidate {
        choices: Vec<(usize, usize)>,
        identity: Option<DrawingObjectIdentity>,
    }

    #[derive(Clone, Copy)]
    enum ElementFrame {
        AlternateContent(usize),
        Choice { index: usize, choice_index: usize },
        Fallback(usize),
        Anchor(usize),
        Other,
    }

    fn attribute_value(
        path: &str,
        element: &BytesStart<'_>,
        wanted: &[u8],
    ) -> Result<Option<String>, String> {
        for attribute in element.attributes().with_checks(false) {
            let attribute = attribute.map_err(|error| format!("parse {path}: {error}"))?;
            if attribute.key.as_ref() == wanted {
                return attribute
                    .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                    .map(|value| Some(value.into_owned()))
                    .map_err(|error| format!("parse {path}: {error}"));
            }
        }
        Ok(None)
    }

    fn choice_is_supported(
        path: &str,
        reader: &NsReader<&[u8]>,
        requires: Option<&str>,
    ) -> Result<bool, String> {
        let Some(requires) = requires else {
            return Ok(false);
        };
        let prefixes = requires.split_whitespace().collect::<Vec<_>>();
        if prefixes.is_empty() {
            return Ok(false);
        }
        for prefix in prefixes {
            let qualified_name = format!("{prefix}:required");
            let (namespace, _) = reader
                .resolver()
                .resolve_element(quick_xml::name::QName(qualified_name.as_bytes()));
            let namespace = resolved_chart_namespace(path, namespace)?;
            if !matches!(
                namespace.as_str(),
                DRAWING_NS
                    | STRICT_DRAWING_NS
                    | DRAWINGML_NS
                    | STRICT_DRAWINGML_NS
                    | CHART_NS
                    | STRICT_CHART_NS
                    | SPREADSHEET_NS
                    | STRICT_SPREADSHEET_NS
                    | REL_NS
                    | STRICT_REL_NS
            ) {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn object_identity(anchor_xml: &str) -> Option<DrawingObjectIdentity> {
        let (kind, object_body) = ["graphicFrame", "pic", "sp"].into_iter().find_map(|kind| {
            elements(anchor_xml, kind)
                .next()
                .map(|object| (kind, object.body))
        })?;
        let properties = elements(object_body, "cNvPr").next()?;
        let id = attr(properties.attrs, "id")?;
        let name = attr(properties.attrs, "name")?;
        let description = attr(properties.attrs, "descr");
        let extent = elements(anchor_xml, "ext").next();
        let width = extent.and_then(|item| attr(item.attrs, "cx"));
        let height = extent.and_then(|item| attr(item.attrs, "cy"));
        let imported_payload = match kind {
            "graphicFrame" => vec![elements(object_body, "chart")
                .next()
                .and_then(|chart| attr(chart.attrs, "r:id").or_else(|| attr(chart.attrs, "id")))?],
            "pic" => vec![
                description.clone().unwrap_or_else(|| name.clone()),
                elements(object_body, "blip").next().and_then(|blip| {
                    attr(blip.attrs, "r:embed").or_else(|| attr(blip.attrs, "embed"))
                })?,
            ],
            "sp" => vec![
                elements(object_body, "t")
                    .next()
                    .map(|text| xml_unescape(text.body.trim()))
                    .unwrap_or_default(),
                elements(object_body, "srgbClr")
                    .next()
                    .and_then(|color| attr(color.attrs, "val"))
                    .unwrap_or_else(|| "default-blue".to_string()),
            ],
            _ => return None,
        };
        Some(DrawingObjectIdentity {
            kind: kind.to_string(),
            id,
            name,
            description,
            width,
            height,
            imported_payload,
        })
    }

    let mut reader = NsReader::from_str(xml);
    let mut elements = Vec::<ElementFrame>::new();
    let mut alternate_contents = Vec::<AlternateContent>::new();
    let mut anchors = Vec::<AnchorCapture>::new();
    let mut absolute_anchors = Vec::<AbsoluteCandidate>::new();

    loop {
        let event_start = reader.buffer_position() as usize;
        let (resolved_namespace, event) = reader
            .read_resolved_event()
            .map_err(|error| format!("parse {path}: {error}"))?;
        match event {
            Event::Start(element) => {
                let namespace = resolved_chart_namespace(path, resolved_namespace)?;
                let name = String::from_utf8_lossy(element.local_name().as_ref()).into_owned();
                let parent = elements.last().copied();
                if namespace == MARKUP_COMPATIBILITY_NS && name == "AlternateContent" {
                    let index = alternate_contents.len();
                    alternate_contents.push(AlternateContent::default());
                    elements.push(ElementFrame::AlternateContent(index));
                } else if namespace == MARKUP_COMPATIBILITY_NS && name == "Choice" {
                    if let Some(ElementFrame::AlternateContent(index)) = parent {
                        let requires = attribute_value(path, &element, b"Requires")?;
                        let supported = choice_is_supported(path, &reader, requires.as_deref())?;
                        let choice_index = alternate_contents[index].choices_supported.len();
                        alternate_contents[index].choices_supported.push(supported);
                        elements.push(ElementFrame::Choice {
                            index,
                            choice_index,
                        });
                    } else {
                        elements.push(ElementFrame::Other);
                    }
                } else if namespace == MARKUP_COMPATIBILITY_NS && name == "Fallback" {
                    if let Some(ElementFrame::AlternateContent(index)) = parent {
                        elements.push(ElementFrame::Fallback(index));
                    } else {
                        elements.push(ElementFrame::Other);
                    }
                } else if is_spreadsheet_drawing_namespace(&namespace)
                    && matches!(
                        name.as_str(),
                        "absoluteAnchor" | "oneCellAnchor" | "twoCellAnchor"
                    )
                {
                    let kind = if name == "absoluteAnchor" {
                        AnchorKind::Absolute
                    } else {
                        AnchorKind::Supported
                    };
                    let choices = elements
                        .iter()
                        .filter_map(|frame| match frame {
                            ElementFrame::Choice {
                                index,
                                choice_index,
                            } => Some((*index, *choice_index)),
                            _ => None,
                        })
                        .collect();
                    let fallbacks = elements
                        .iter()
                        .filter_map(|frame| match frame {
                            ElementFrame::Fallback(index) => Some(*index),
                            _ => None,
                        })
                        .collect();
                    let index = anchors.len();
                    anchors.push(AnchorCapture {
                        kind,
                        choices,
                        fallbacks,
                        start_byte: event_start,
                    });
                    elements.push(ElementFrame::Anchor(index));
                } else {
                    elements.push(ElementFrame::Other);
                }
            }
            Event::Empty(element) => {
                let namespace = resolved_chart_namespace(path, resolved_namespace)?;
                let name = String::from_utf8_lossy(element.local_name().as_ref()).into_owned();
                if is_spreadsheet_drawing_namespace(&namespace) && name == "absoluteAnchor" {
                    let choices = elements
                        .iter()
                        .filter_map(|frame| match frame {
                            ElementFrame::Choice {
                                index,
                                choice_index,
                            } => Some((*index, *choice_index)),
                            _ => None,
                        })
                        .collect();
                    absolute_anchors.push(AbsoluteCandidate {
                        choices,
                        identity: None,
                    });
                }
            }
            Event::End(_) => match elements.pop() {
                Some(ElementFrame::Anchor(index)) => {
                    let anchor = &anchors[index];
                    let anchor_xml = xml
                        .get(anchor.start_byte..reader.buffer_position() as usize)
                        .ok_or_else(|| {
                            format!("parse {path}: invalid drawing anchor byte range")
                        })?;
                    let identity = object_identity(anchor_xml);
                    match anchor.kind {
                        AnchorKind::Absolute => absolute_anchors.push(AbsoluteCandidate {
                            choices: anchor.choices.clone(),
                            identity,
                        }),
                        AnchorKind::Supported => {
                            if let Some(identity) = identity {
                                for alternate_index in &anchor.fallbacks {
                                    alternate_contents[*alternate_index]
                                        .fallback_objects
                                        .push(identity.clone());
                                }
                            }
                        }
                    }
                }
                Some(
                    ElementFrame::AlternateContent(_)
                    | ElementFrame::Choice { .. }
                    | ElementFrame::Fallback(_)
                    | ElementFrame::Other,
                )
                | None => {}
            },
            Event::Eof => break,
            _ => {}
        }
    }

    if !elements.is_empty() {
        return Err(format!("parse {path}: unclosed drawing XML element"));
    }

    Ok(absolute_anchors.iter().any(|anchor| {
        if anchor.choices.is_empty() {
            return true;
        }

        for (index, choice_index) in &anchor.choices {
            let alternate_content = &alternate_contents[*index];
            match alternate_content
                .choices_supported
                .iter()
                .position(|supported| *supported)
            {
                Some(selected_index) if selected_index == *choice_index => {}
                Some(_) => return false,
                None => {
                    return anchor.identity.as_ref().map_or(true, |identity| {
                        !alternate_content.fallback_objects.contains(identity)
                    });
                }
            }
        }

        true
    }))
}

pub(super) struct ChartPlotGroup<'a> {
    pub(super) name: String,
    pub(super) first_series_body: Option<&'a str>,
    series_count: usize,
}

struct ChartXmlFrame {
    namespace: String,
    name: String,
    is_plot_area: bool,
    is_plot_group: bool,
    plot_group_series_count: usize,
    plot_group_first_series_range: Option<(usize, usize)>,
    plot_series_body_start: Option<usize>,
}

pub(super) fn chart_plot_groups<'a>(
    path: &str,
    chart_xml: &'a str,
) -> Result<Vec<ChartPlotGroup<'a>>, String> {
    let mut reader = NsReader::from_str(chart_xml);
    reader.config_mut().trim_text(false);
    let mut stack = Vec::<ChartXmlFrame>::new();
    let mut groups = Vec::new();

    loop {
        let event_start = reader.buffer_position() as usize;
        let (resolved_namespace, event) = reader
            .read_resolved_event()
            .map_err(|error| format!("parse {path}: {error}"))?;
        match event {
            Event::Start(element) => {
                let namespace = resolved_chart_namespace(path, resolved_namespace)?;
                let name = String::from_utf8_lossy(element.local_name().as_ref()).into_owned();
                let mut plot_series_body_start = None;
                if name == "ser" && is_chart_namespace(&namespace) {
                    if let Some(parent) = stack.last_mut().filter(|parent| parent.is_plot_group) {
                        parent.plot_group_series_count += 1;
                        if parent.plot_group_first_series_range.is_none() {
                            plot_series_body_start = Some(reader.buffer_position() as usize);
                        }
                    }
                }
                let is_plot_area = name == "plotArea"
                    && is_chart_namespace(&namespace)
                    && stack.len() >= 2
                    && stack[stack.len() - 1].name == "chart"
                    && is_chart_namespace(&stack[stack.len() - 1].namespace)
                    && stack[stack.len() - 2].name == "chartSpace"
                    && is_chart_namespace(&stack[stack.len() - 2].namespace);
                let is_plot_group = stack.last().is_some_and(|parent| parent.is_plot_area)
                    && is_chart_namespace(&namespace)
                    && name.ends_with("Chart");
                stack.push(ChartXmlFrame {
                    namespace,
                    name,
                    is_plot_area,
                    is_plot_group,
                    plot_group_series_count: 0,
                    plot_group_first_series_range: None,
                    plot_series_body_start,
                });
            }
            Event::Empty(element) => {
                let namespace = resolved_chart_namespace(path, resolved_namespace)?;
                let name = String::from_utf8_lossy(element.local_name().as_ref()).into_owned();
                if name == "ser" && is_chart_namespace(&namespace) {
                    if let Some(parent) = stack.last_mut().filter(|parent| parent.is_plot_group) {
                        parent.plot_group_series_count += 1;
                        if parent.plot_group_first_series_range.is_none() {
                            parent.plot_group_first_series_range = Some((event_start, event_start));
                        }
                    }
                }
                if stack.last().is_some_and(|parent| parent.is_plot_area)
                    && is_chart_namespace(&namespace)
                    && name.ends_with("Chart")
                {
                    groups.push(ChartPlotGroup {
                        name,
                        first_series_body: None,
                        series_count: 0,
                    });
                }
            }
            Event::End(_) => {
                if let Some(frame) = stack.pop() {
                    if let Some(body_start) = frame.plot_series_body_start {
                        if let Some(parent) = stack.last_mut().filter(|parent| parent.is_plot_group)
                        {
                            if parent.plot_group_first_series_range.is_none() {
                                parent.plot_group_first_series_range =
                                    Some((body_start, event_start));
                            }
                        }
                    }
                    if frame.is_plot_group {
                        let first_series_body = frame
                            .plot_group_first_series_range
                            .map(|(start, end)| {
                                chart_xml.get(start..end).ok_or_else(|| {
                                    format!("parse {path}: invalid chart series byte range")
                                })
                            })
                            .transpose()?;
                        groups.push(ChartPlotGroup {
                            name: frame.name,
                            first_series_body,
                            series_count: frame.plot_group_series_count,
                        });
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }

    if !stack.is_empty() {
        return Err(format!("parse {path}: unclosed chart XML element"));
    }
    Ok(groups)
}

fn resolved_chart_namespace(path: &str, namespace: ResolveResult<'_>) -> Result<String, String> {
    match namespace {
        ResolveResult::Bound(namespace) => {
            Ok(String::from_utf8_lossy(namespace.as_ref()).into_owned())
        }
        ResolveResult::Unbound => Ok(String::new()),
        ResolveResult::Unknown(prefix) => Err(format!(
            "parse {path}: XML element uses undeclared namespace prefix {}",
            String::from_utf8_lossy(&prefix)
        )),
    }
}

pub(super) fn imported_chart_group_index(chart_groups: &[ChartPlotGroup<'_>]) -> Option<usize> {
    ["pieChart", "scatterChart", "lineChart", "barChart"]
        .iter()
        .find_map(|name| chart_groups.iter().position(|group| &group.name == name))
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
    drawing_target_path(relationships, attributes, expected_kind, drawing_path)
        .is_some_and(|target| archive.get(&target).is_some())
}

fn drawing_target_path(
    relationships: &[OoxmlRelationship],
    attributes: &BTreeMap<String, String>,
    expected_kind: &str,
    drawing_path: &str,
) -> Option<String> {
    let relationship_id = attributes
        .get("r:id")
        .or_else(|| attributes.get("r:embed"))
        .or_else(|| attributes.get("id"))
        .or_else(|| attributes.get("embed"));
    let relationship_id = relationship_id?;
    relationships
        .iter()
        .find(|relationship| relationship.id == *relationship_id)
        .filter(|relationship| relationship.kind.ends_with(&format!("/{expected_kind}")))
        .map(|relationship| resolve_target(parent_path(drawing_path), &relationship.target))
}

fn unsupported_chart_type(chart_tag: &str) -> Option<XlsxChartType> {
    let chart_type = XlsxChartType::from_tag(chart_tag);
    (!chart_type.is_supported()).then_some(chart_type)
}
