//! Follow links between the files inside an XLSX package.

use std::collections::BTreeMap;

use super::xml::{attr, elements, parse_xml_nodes, xml_unescape};
use super::PACKAGE_REL_NS;

pub(super) struct OoxmlRelationship {
    pub(super) id: String,
    pub(super) kind: String,
    pub(super) target: String,
}

pub(super) fn parse_relationships(
    path: &str,
    bytes: &[u8],
) -> Result<Vec<OoxmlRelationship>, String> {
    let xml = std::str::from_utf8(bytes).map_err(|_| format!("{path} is not valid UTF-8"))?;
    let nodes = parse_xml_nodes(path, xml)?;
    Ok(nodes
        .into_iter()
        .filter(|node| node.namespace == PACKAGE_REL_NS && node.name == "Relationship")
        .filter_map(|node| {
            Some(OoxmlRelationship {
                id: node.attribute("Id")?.to_string(),
                kind: node.attribute("Type")?.to_string(),
                target: node.attribute("Target")?.to_string(),
            })
        })
        .collect())
}

pub(super) fn relationship_map(xml: &str) -> BTreeMap<String, String> {
    elements(xml, "Relationship")
        .filter_map(|tag| {
            Some((
                attr(tag.attrs, "Id")?,
                xml_unescape(&attr(tag.attrs, "Target")?),
            ))
        })
        .collect()
}

pub(super) fn relationship_part_path(part: &str) -> String {
    let parent = parent_path(part);
    let filename = part.rsplit('/').next().unwrap_or(part);
    if parent.is_empty() {
        format!("_rels/{filename}.rels")
    } else {
        format!("{parent}/_rels/{filename}.rels")
    }
}

pub(super) fn parent_path(path: &str) -> &str {
    path.rsplit_once('/')
        .map(|(parent, _)| parent)
        .unwrap_or("")
}

pub(super) fn resolve_target(base_dir: &str, target: &str) -> String {
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
