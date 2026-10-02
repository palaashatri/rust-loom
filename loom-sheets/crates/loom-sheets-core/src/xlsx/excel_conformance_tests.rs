//! Package conformance checks and the real-Excel fixture for the rich XLSX
//! exporter.
//!
//! The deterministic tests need no spreadsheet application: they parse every
//! exported part, follow every relationship, and check the structural rules
//! that Microsoft Excel enforces when it opens a workbook (an out-of-order row
//! or an unknown content type is silently "repaired" by Excel, which drops
//! content).
//!
//! `export_rich_workbook_for_excel` is the opt-in executable evidence. Set
//! `LOOM_INTEROP_OUT=<dir>` to write the workbook, and additionally
//! `LOOM_INTEROP_EXCEL=1` (Windows with Microsoft Excel installed) to open it
//! invisibly through COM with `tests/excel/dump_excel.ps1` and compare what
//! Excel reports against the Loom model, cell by cell.

use std::collections::{BTreeMap, BTreeSet};

use loom_package::zip::PackageArchive;
use quick_xml::events::Event;
use quick_xml::name::ResolveResult;
use quick_xml::NsReader;

use super::export_xlsx_sheets;

mod excel_driver;
mod fixture;
mod regressions;

pub(super) use fixture::rich_interop_workbook;

/// One XML element with its nesting depth, attributes and
/// the character data directly inside it.
#[derive(Debug, Clone)]
pub(super) struct Node {
    pub depth: usize,
    pub name: String,
    pub attrs: BTreeMap<String, String>,
    pub text: String,
}

/// Parse a whole part; fails on malformed XML, mismatched tags, duplicate
/// attributes or an undeclared namespace prefix.
pub(super) fn walk(path: &str, xml: &str) -> Result<Vec<Node>, String> {
    let mut reader = NsReader::from_str(xml);
    let mut nodes: Vec<Node> = Vec::new();
    let mut stack: Vec<usize> = Vec::new();
    loop {
        let (resolved, event) = reader
            .read_resolved_event()
            .map_err(|error| format!("{path}: {error}"))?;
        let (element, opens) = match event {
            Event::Start(element) => (element, true),
            Event::Empty(element) => (element, false),
            Event::End(_) => {
                stack.pop();
                continue;
            }
            Event::Text(text) => {
                if let Some(&open) = stack.last() {
                    let text = text.decode().map_err(|error| format!("{path}: {error}"))?;
                    nodes[open].text.push_str(&text);
                }
                continue;
            }
            Event::GeneralRef(reference) => {
                if let Some(&open) = stack.last() {
                    let name = reference
                        .decode()
                        .map_err(|error| format!("{path}: {error}"))?;
                    let resolved = match name.as_ref() {
                        "amp" => '&',
                        "lt" => '<',
                        "gt" => '>',
                        "quot" => '"',
                        "apos" => '\'',
                        _ => reference
                            .resolve_char_ref()
                            .map_err(|error| format!("{path}: {error}"))?
                            .ok_or_else(|| format!("{path}: unknown entity &{name};"))?,
                    };
                    nodes[open].text.push(resolved);
                }
                continue;
            }
            Event::Eof => break,
            _ => continue,
        };
        if let ResolveResult::Unknown(prefix) = resolved {
            return Err(format!(
                "{path}: undeclared namespace prefix {}",
                String::from_utf8_lossy(&prefix)
            ));
        }
        let mut attrs = BTreeMap::new();
        for attribute in element.attributes().with_checks(true) {
            let attribute = attribute.map_err(|error| format!("{path}: {error}"))?;
            let key = String::from_utf8_lossy(attribute.key.as_ref()).into_owned();
            let value = attribute
                .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                .map_err(|error| format!("{path}: {error}"))?
                .into_owned();
            if attrs.insert(key.clone(), value).is_some() {
                return Err(format!("{path}: duplicate attribute {key}"));
            }
        }
        nodes.push(Node {
            depth: stack.len(),
            name: String::from_utf8_lossy(element.local_name().as_ref()).into_owned(),
            attrs,
            text: String::new(),
        });
        if opens {
            stack.push(nodes.len() - 1);
        }
    }
    if !stack.is_empty() {
        return Err(format!("{path}: unclosed element"));
    }
    if nodes.is_empty() {
        return Err(format!("{path}: no root element"));
    }
    Ok(nodes)
}

/// Schema order of the children of `CT_Worksheet` (ECMA-376 18.3.1.99).
const WORKSHEET_ORDER: &[&str] = &[
    "sheetPr",
    "dimension",
    "sheetViews",
    "sheetFormatPr",
    "cols",
    "sheetData",
    "sheetCalcPr",
    "sheetProtection",
    "protectedRanges",
    "scenarios",
    "autoFilter",
    "sortState",
    "dataConsolidate",
    "customSheetViews",
    "mergeCells",
    "phoneticPr",
    "conditionalFormatting",
    "dataValidations",
    "hyperlinks",
    "printOptions",
    "pageMargins",
    "pageSetup",
    "headerFooter",
    "rowBreaks",
    "colBreaks",
    "customProperties",
    "cellWatches",
    "ignoredErrors",
    "smartTags",
    "drawing",
    "legacyDrawing",
    "legacyDrawingHF",
    "picture",
    "oleObjects",
    "controls",
    "webPublishItems",
    "tableParts",
    "extLst",
];

/// Schema order of the children of `CT_Workbook`.
const WORKBOOK_ORDER: &[&str] = &[
    "fileVersion",
    "fileSharing",
    "workbookPr",
    "workbookProtection",
    "bookViews",
    "sheets",
    "functionGroups",
    "externalReferences",
    "definedNames",
    "calcPr",
    "oleSize",
    "customWorkbookViews",
    "pivotCaches",
    "smartTagPr",
    "smartTagTypes",
    "webPublishing",
    "fileRecoveryPr",
    "webPublishObjects",
    "extLst",
];

/// Schema order of the children of `CT_Stylesheet`.
const STYLESHEET_ORDER: &[&str] = &[
    "numFmts",
    "fonts",
    "fills",
    "borders",
    "cellStyleXfs",
    "cellXfs",
    "cellStyles",
    "dxfs",
    "tableStyles",
    "colors",
    "extLst",
];

/// The error literals a `t="e"` cell may carry.
const ERROR_LITERALS: &[&str] = &[
    "#NULL!",
    "#DIV/0!",
    "#VALUE!",
    "#REF!",
    "#NAME?",
    "#NUM!",
    "#N/A",
    "#GETTING_DATA",
];

fn check_child_order(path: &str, nodes: &[Node], order: &[&str], problems: &mut Vec<String>) {
    let mut last = None::<usize>;
    for node in nodes.iter().filter(|node| node.depth == 1) {
        let Some(position) = order.iter().position(|name| *name == node.name) else {
            problems.push(format!("{path}: unexpected child element <{}>", node.name));
            continue;
        };
        if let Some(previous) = last {
            if position < previous {
                problems.push(format!(
                    "{path}: <{}> appears after <{}>, against the schema order",
                    node.name, order[previous]
                ));
            }
        }
        last = Some(position);
    }
}

pub(super) fn column_number(letters: &str) -> u32 {
    letters.chars().fold(0u32, |value, c| {
        value * 26 + (c.to_ascii_uppercase() as u32 - 'A' as u32 + 1)
    })
}

/// `(column, row)`, both 1-based, of an A1 reference.
pub(super) fn split_reference(reference: &str) -> Option<(u32, u32)> {
    let digits_at = reference.find(|c: char| c.is_ascii_digit())?;
    let (letters, digits) = reference.split_at(digits_at);
    if letters.is_empty() || !letters.chars().all(|c| c.is_ascii_uppercase()) {
        return None;
    }
    Some((column_number(letters), digits.parse().ok()?))
}

fn resolve_part(base_dir: &str, target: &str) -> String {
    let joined: Vec<&str> = if let Some(absolute) = target.strip_prefix('/') {
        absolute.split('/').collect()
    } else {
        base_dir
            .split('/')
            .filter(|part| !part.is_empty())
            .chain(target.split('/'))
            .collect()
    };
    let mut normalized: Vec<&str> = Vec::new();
    for part in joined {
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

type RelationshipTable = BTreeMap<String, BTreeMap<String, (String, String, bool)>>;

/// Everything Excel would trip over, as plain-language problems. An empty
/// list means the package is structurally sound.
pub(super) fn package_problems(bytes: &[u8]) -> Vec<String> {
    let mut problems = Vec::new();
    let archive = match PackageArchive::from_bytes(bytes) {
        Ok(archive) => archive,
        Err(error) => return vec![format!("not a readable package: {error}")],
    };
    let paths: BTreeSet<String> = archive.paths().into_iter().map(str::to_string).collect();
    check_content_types(&archive, &paths, &mut problems);

    let mut relationships: RelationshipTable = BTreeMap::new();
    let mut parsed: BTreeMap<String, Vec<Node>> = BTreeMap::new();
    for path in &paths {
        if !(path.ends_with(".xml") || path.ends_with(".rels")) || path == "[Content_Types].xml" {
            continue;
        }
        let Ok(xml) = std::str::from_utf8(archive.get(path).unwrap_or_default()) else {
            problems.push(format!("{path}: not UTF-8"));
            continue;
        };
        let nodes = match walk(path, xml) {
            Ok(nodes) => nodes,
            Err(error) => {
                problems.push(error);
                continue;
            }
        };
        if path.ends_with(".rels") {
            relationships.insert(
                path.clone(),
                check_relationships(path, &nodes, &paths, &mut problems),
            );
        }
        parsed.insert(path.clone(), nodes);
    }

    let cell_xf_count = parsed.get("xl/styles.xml").map(|nodes| {
        let mut inside = false;
        let mut total = 0usize;
        for node in nodes {
            if node.depth == 1 {
                inside = node.name == "cellXfs";
            } else if inside && node.depth == 2 && node.name == "xf" {
                total += 1;
            }
        }
        total
    });
    let shared_string_count = parsed.get("xl/sharedStrings.xml").map(|nodes| {
        let declared = nodes
            .first()
            .and_then(|root| root.attrs.get("uniqueCount"))
            .and_then(|value| value.parse::<usize>().ok());
        let actual = nodes
            .iter()
            .filter(|node| node.depth == 1 && node.name == "si")
            .count();
        if declared.is_some_and(|declared| declared != actual) {
            problems.push(format!(
                "xl/sharedStrings.xml: uniqueCount {declared:?} but {actual} entries"
            ));
        }
        actual
    });

    for (path, nodes) in &parsed {
        match nodes.first().map(|node| node.name.as_str()) {
            Some("worksheet") => check_worksheet(
                path,
                nodes,
                cell_xf_count,
                shared_string_count,
                &mut problems,
            ),
            Some("workbook") => check_child_order(path, nodes, WORKBOOK_ORDER, &mut problems),
            Some("styleSheet") => check_stylesheet(path, nodes, &mut problems),
            _ => {}
        }
        if path.ends_with(".rels") {
            continue;
        }
        let (dir, file) = path.rsplit_once('/').unwrap_or(("", path.as_str()));
        let rels_path = if dir.is_empty() {
            format!("_rels/{file}.rels")
        } else {
            format!("{dir}/_rels/{file}.rels")
        };
        for node in nodes {
            for (key, value) in &node.attrs {
                if matches!(key.as_str(), "r:id" | "r:embed" | "r:link") {
                    let known = relationships
                        .get(&rels_path)
                        .is_some_and(|ids| ids.contains_key(value));
                    if !known {
                        problems.push(format!(
                            "{path}: <{}> {key}=\"{value}\" has no relationship in {rels_path}",
                            node.name
                        ));
                    }
                }
            }
        }
    }

    match relationships.get("xl/_rels/workbook.xml.rels") {
        Some(rels) => {
            if !rels
                .values()
                .any(|(kind, _, _)| kind.ends_with("/worksheet"))
            {
                problems.push("workbook has no worksheet relationship".to_string());
            }
            if !rels.values().any(|(kind, _, _)| kind.ends_with("/styles")) {
                problems.push("workbook does not link a styles part".to_string());
            }
        }
        None => problems.push("xl/_rels/workbook.xml.rels is missing".to_string()),
    }
    problems
}

fn check_content_types(
    archive: &PackageArchive,
    paths: &BTreeSet<String>,
    problems: &mut Vec<String>,
) {
    let content_types = archive
        .get("[Content_Types].xml")
        .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
        .unwrap_or_default();
    let mut defaults = BTreeSet::new();
    let mut overrides = BTreeSet::new();
    match walk("[Content_Types].xml", &content_types) {
        Ok(nodes) => {
            for node in &nodes {
                match node.name.as_str() {
                    "Default" => {
                        if let Some(extension) = node.attrs.get("Extension") {
                            if !defaults.insert(extension.to_ascii_lowercase()) {
                                problems.push(format!(
                                    "[Content_Types].xml: duplicate Default for .{extension}"
                                ));
                            }
                        }
                    }
                    "Override" => {
                        if let Some(part) = node.attrs.get("PartName") {
                            if !overrides.insert(part.clone()) {
                                problems.push(format!(
                                    "[Content_Types].xml: duplicate Override for {part}"
                                ));
                            }
                            if !paths.contains(part.trim_start_matches('/')) {
                                problems.push(format!(
                                    "[Content_Types].xml: Override names missing part {part}"
                                ));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        Err(error) => problems.push(error),
    }
    for path in paths {
        if path == "[Content_Types].xml" {
            continue;
        }
        let extension = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
        if !overrides.contains(&format!("/{path}")) && !defaults.contains(&extension) {
            problems.push(format!("{path}: no content type"));
        }
    }
}

fn check_relationships(
    path: &str,
    nodes: &[Node],
    paths: &BTreeSet<String>,
    problems: &mut Vec<String>,
) -> BTreeMap<String, (String, String, bool)> {
    let source_dir = path.rsplit_once("/_rels/").map_or("", |(dir, _)| dir);
    let mut ids = BTreeMap::new();
    for node in nodes.iter().filter(|node| node.name == "Relationship") {
        let (Some(id), Some(target)) = (node.attrs.get("Id"), node.attrs.get("Target")) else {
            problems.push(format!("{path}: relationship without Id or Target"));
            continue;
        };
        let external = node
            .attrs
            .get("TargetMode")
            .is_some_and(|mode| mode == "External");
        let resolved = resolve_part(source_dir, target);
        if !external && !paths.contains(&resolved) {
            problems.push(format!("{path}: {id} targets missing part {resolved}"));
        }
        let kind = node.attrs.get("Type").cloned().unwrap_or_default();
        if ids.insert(id.clone(), (kind, resolved, external)).is_some() {
            problems.push(format!("{path}: duplicate relationship id {id}"));
        }
    }
    ids
}

fn check_worksheet(
    path: &str,
    nodes: &[Node],
    cell_xf_count: Option<usize>,
    shared_string_count: Option<usize>,
    problems: &mut Vec<String>,
) {
    check_child_order(path, nodes, WORKSHEET_ORDER, problems);
    let mut last_row = 0u32;
    let mut current_row = None::<u32>;
    let mut last_col = 0u32;
    for (index, node) in nodes.iter().enumerate() {
        match (node.depth, node.name.as_str()) {
            (2, "row") => {
                let Some(row) = node.attrs.get("r").and_then(|r| r.parse::<u32>().ok()) else {
                    problems.push(format!("{path}: row without r"));
                    continue;
                };
                if row <= last_row {
                    problems.push(format!(
                        "{path}: row {row} follows row {last_row}; rows must be unique and ascending"
                    ));
                }
                last_row = row;
                current_row = Some(row);
                last_col = 0;
            }
            (3, "c") => check_cell(
                path,
                nodes,
                index,
                current_row,
                &mut last_col,
                cell_xf_count,
                shared_string_count,
                problems,
            ),
            _ => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn check_cell(
    path: &str,
    nodes: &[Node],
    index: usize,
    current_row: Option<u32>,
    last_col: &mut u32,
    cell_xf_count: Option<usize>,
    shared_string_count: Option<usize>,
    problems: &mut Vec<String>,
) {
    let node = &nodes[index];
    let reference = node.attrs.get("r").map_or("?", String::as_str);
    let Some((col, row)) = split_reference(reference) else {
        problems.push(format!("{path}: cell with a bad reference {reference:?}"));
        return;
    };
    if Some(row) != current_row {
        problems.push(format!(
            "{path}: cell {reference} sits in row {current_row:?}"
        ));
    }
    if col <= *last_col {
        problems.push(format!(
            "{path}: cell {reference} is not after the previous cell of its row"
        ));
    }
    *last_col = col;
    if let (Some(style), Some(count)) = (node.attrs.get("s"), cell_xf_count) {
        if style.parse::<usize>().map_or(true, |style| style >= count) {
            problems.push(format!(
                "{path}: cell {reference} uses missing style {style}"
            ));
        }
    }
    let children = nodes[index + 1..]
        .iter()
        .take_while(|child| child.depth > node.depth)
        .filter(|child| child.depth == node.depth + 1);
    let mut value = None::<&str>;
    let mut has_formula = false;
    for child in children {
        match child.name.as_str() {
            "v" => value = Some(child.text.as_str()),
            "f" => has_formula = true,
            _ => {}
        }
    }
    let kind = node.attrs.get("t").map_or("n", String::as_str);
    match (kind, value) {
        ("e", Some(literal)) if !ERROR_LITERALS.contains(&literal) => problems.push(format!(
            "{path}: cell {reference} is typed as an error but holds {literal:?}"
        )),
        ("e", None) => problems.push(format!("{path}: error cell {reference} has no value")),
        ("b", Some(literal)) if !matches!(literal, "0" | "1") => problems.push(format!(
            "{path}: boolean cell {reference} holds {literal:?}"
        )),
        ("s", Some(literal)) => {
            match (literal.parse::<usize>(), shared_string_count) {
                (Ok(position), Some(count)) if position < count => {}
                _ => problems.push(format!(
                    "{path}: cell {reference} points at missing shared string {literal:?}"
                )),
            }
            if has_formula {
                problems.push(format!(
                    "{path}: formula cell {reference} caches its text as a shared string; \
                     formula text results are t=\"str\""
                ));
            }
        }
        ("n", Some(literal)) if literal.parse::<f64>().map_or(true, |n| !n.is_finite()) => problems
            .push(format!(
                "{path}: numeric cell {reference} holds {literal:?}"
            )),
        _ => {}
    }
}

fn check_stylesheet(path: &str, nodes: &[Node], problems: &mut Vec<String>) {
    check_child_order(path, nodes, STYLESHEET_ORDER, problems);
    let section_items = |container: &str, item: &str| {
        let mut inside = false;
        let mut total = 0usize;
        for node in nodes {
            if node.depth == 1 {
                inside = node.name == container;
            } else if inside && node.depth == 2 && node.name == item {
                total += 1;
            }
        }
        total
    };
    for (container, item) in [
        ("numFmts", "numFmt"),
        ("fonts", "font"),
        ("fills", "fill"),
        ("borders", "border"),
        ("cellXfs", "xf"),
    ] {
        let Some(declared) = nodes
            .iter()
            .find(|n| n.depth == 1 && n.name == container)
            .map(|n| n.attrs.get("count").and_then(|c| c.parse::<usize>().ok()))
        else {
            continue;
        };
        let actual = section_items(container, item);
        if declared != Some(actual) {
            problems.push(format!(
                "{path}: <{container}> declares count {declared:?} but holds {actual}"
            ));
        }
        if container == "numFmts" && actual == 0 {
            problems.push(format!("{path}: <numFmts> is present but empty"));
        }
    }
    let fonts = section_items("fonts", "font");
    let fills = section_items("fills", "fill");
    let borders = section_items("borders", "border");
    let mut inside = false;
    for node in nodes {
        if node.depth == 1 {
            inside = node.name == "cellXfs";
        } else if inside && node.depth == 2 && node.name == "xf" {
            for (key, limit) in [("fontId", fonts), ("fillId", fills), ("borderId", borders)] {
                let value = node
                    .attrs
                    .get(key)
                    .and_then(|v| v.parse::<usize>().ok())
                    .unwrap_or(0);
                if value >= limit {
                    problems.push(format!("{path}: xf {key}={value} is out of range"));
                }
            }
        }
    }
    // Excel reserves the first two fills: none, then gray125.
    let fill_patterns: Vec<&str> = {
        let mut inside = false;
        let mut patterns = Vec::new();
        for node in nodes {
            if node.depth == 1 {
                inside = node.name == "fills";
            } else if inside && node.name == "patternFill" {
                patterns.push(node.attrs.get("patternType").map_or("", String::as_str));
            }
        }
        patterns
    };
    if fill_patterns.get(..2) != Some(&["none", "gray125"][..]) {
        problems.push(format!(
            "{path}: the first two fills must be none and gray125, found {:?}",
            &fill_patterns[..fill_patterns.len().min(2)]
        ));
    }
}

#[test]
fn rich_workbook_package_is_structurally_sound() {
    let bytes = export_xlsx_sheets(&rich_interop_workbook()).expect("export rich workbook");
    let problems = package_problems(&bytes);
    assert!(
        problems.is_empty(),
        "package problems:\n{}",
        problems.join("\n")
    );
}

/// Opt-in evidence run against real Microsoft Excel; see the module docs.
#[test]
fn export_rich_workbook_for_excel() {
    let Ok(out_dir) = std::env::var("LOOM_INTEROP_OUT") else {
        eprintln!("LOOM_INTEROP_OUT is not set; skipping the Excel interoperability run");
        return;
    };
    let sheets = rich_interop_workbook();
    let bytes = export_xlsx_sheets(&sheets).expect("export rich workbook");
    let dir = std::path::PathBuf::from(out_dir);
    std::fs::create_dir_all(&dir).expect("create output directory");
    let path = dir.join("loom-excel-interop.xlsx");
    std::fs::write(&path, &bytes).expect("write workbook");
    eprintln!("wrote {}", path.display());
    let problems = package_problems(&bytes);
    assert!(
        problems.is_empty(),
        "package problems:\n{}",
        problems.join("\n")
    );
    if std::env::var("LOOM_INTEROP_EXCEL").as_deref() == Ok("1") {
        excel_driver::verify_with_excel(&sheets, &path, &dir);
    }
}
