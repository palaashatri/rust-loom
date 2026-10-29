//! DOCX footnote parts, identifiers, and import validation.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use loom_package::zip::PackageArchive;
use roxmltree::Node;

use super::xml::{self, W_NS, XML_DECLARATION};
use crate::{FootnoteEntry, RichBlock, WriterDocument};

pub(super) const FOOTNOTES_REL: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/footnotes";
pub(super) const FOOTNOTES_CONTENT_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml";
const PACKAGE_REL_NS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const M_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/math";
const MAX_FOOTNOTES_XML_BYTES: usize = crate::footnotes::MAX_FOOTNOTE_TOTAL_BYTES * 6
    + crate::footnotes::MAX_FOOTNOTES_PER_DOCUMENT * 1_024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct InlineReference {
    pub offset: usize,
    pub ooxml_id: i32,
}

/// The note part and the reference IDs used by the document body.
pub(super) struct ExportPlan {
    pub xml: Option<String>,
    by_block: BTreeMap<u64, Vec<InlineReference>>,
}

impl ExportPlan {
    pub(super) fn new(doc: &WriterDocument) -> Result<Self, String> {
        doc.validate_footnotes()
            .map_err(|error| format!("invalid Writer footnotes: {error}"))?;
        for note in &doc.footnotes {
            let block = doc
                .blocks
                .iter()
                .find(|block| block.id == note.block_id)
                .expect("footnote validation found its block");
            if let Some(table) = block.typed_table() {
                let map = crate::tables::TableTextMap::new(table);
                let (row, column) = map
                    .cell_at(note.offset)
                    .ok_or_else(|| format!("footnote {} is outside its table", note.id))?;
                if table.cell(row, column).is_some_and(|cell| cell.is_covered) {
                    return Err(format!(
                        "footnote {} is anchored in a covered table cell",
                        note.id
                    ));
                }
            }
        }
        if doc.footnotes.is_empty() {
            return Ok(Self {
                xml: None,
                by_block: BTreeMap::new(),
            });
        }

        let mut used = BTreeSet::new();
        let mut ids = BTreeMap::new();
        for note in &doc.footnotes {
            if let Ok(id) = i32::try_from(note.id) {
                if id > 0 {
                    used.insert(id);
                    ids.insert(note.id, id);
                }
            }
        }
        let mut next = 1i32;
        let mut fallback_notes: Vec<_> = doc
            .footnotes
            .iter()
            .filter(|note| !ids.contains_key(&note.id))
            .collect();
        fallback_notes.sort_by_key(|note| note.id);
        for note in fallback_notes {
            while used.contains(&next) {
                next = next
                    .checked_add(1)
                    .ok_or_else(|| "no valid OOXML footnote IDs are available".to_string())?;
            }
            ids.insert(note.id, next);
            used.insert(next);
        }

        let mut by_block: BTreeMap<u64, Vec<InlineReference>> = BTreeMap::new();
        for note in &doc.footnotes {
            by_block
                .entry(note.block_id)
                .or_default()
                .push(InlineReference {
                    offset: note.offset,
                    ooxml_id: ids[&note.id],
                });
        }
        for references in by_block.values_mut() {
            references.sort_by_key(|reference| reference.offset);
        }

        let mut xml_part = format!(
            "{XML_DECLARATION}<w:footnotes xmlns:w=\"{W_NS}\">\
             <w:footnote w:type=\"separator\" w:id=\"-1\"><w:p><w:r><w:separator/></w:r></w:p></w:footnote>\
             <w:footnote w:type=\"continuationSeparator\" w:id=\"0\"><w:p><w:r><w:continuationSeparator/></w:r></w:p></w:footnote>"
        );
        for (_, note) in doc.numbered_footnotes() {
            let id = ids[&note.id];
            xml_part.push_str(&format!(
                "<w:footnote w:id=\"{id}\"><w:p><w:r><w:footnoteRef/></w:r>"
            ));
            push_note_body_xml(&mut xml_part, note)?;
            xml_part.push_str("</w:p></w:footnote>");
        }
        xml_part.push_str("</w:footnotes>");
        if xml_part.len() > MAX_FOOTNOTES_XML_BYTES {
            return Err(format!(
                "footnotes.xml exceeds the {MAX_FOOTNOTES_XML_BYTES}-byte package limit"
            ));
        }

        Ok(Self {
            xml: Some(xml_part),
            by_block,
        })
    }

    pub(super) fn references_for(&self, block_id: u64) -> &[InlineReference] {
        self.by_block
            .get(&block_id)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }
}

fn push_note_body_xml(out: &mut String, note: &FootnoteEntry) -> Result<(), String> {
    let mut plain = String::new();
    for character in note.body.chars() {
        if character == '\r' {
            if !plain.is_empty() {
                push_plain_note_run(out, &plain);
                plain.clear();
            }
            out.push_str("<w:r><w:t xml:space=\"preserve\">&#13;</w:t></w:r>");
        } else if xml::is_xml_char(character) {
            plain.push(character);
        } else {
            return Err(format!(
                "footnote {} contains a character that DOCX cannot preserve",
                note.id
            ));
        }
    }
    if !plain.is_empty() {
        push_plain_note_run(out, &plain);
    }
    Ok(())
}

fn push_plain_note_run(out: &mut String, text: &str) {
    let mut children = String::new();
    xml::push_run_children(&mut children, text);
    out.push_str("<w:r>");
    out.push_str(&children);
    out.push_str("</w:r>");
}

/// Resolves body references only after the complete document body and note part
/// have been parsed, so malformed packages cannot produce a partial document.
pub(super) fn import_document(
    archive: &PackageArchive,
    references: &[super::import::FootnoteReference],
    blocks: &[RichBlock],
) -> Result<Vec<FootnoteEntry>, String> {
    let part_path = footnotes_part_path(archive)?;
    let Some(part_path) = part_path else {
        if references.is_empty() {
            return Ok(Vec::new());
        }
        return Err("DOCX document references footnotes but has no footnotes relationship".into());
    };
    let bytes = archive
        .get(&part_path)
        .ok_or_else(|| format!("DOCX footnotes relationship points to missing {part_path}"))?;
    if bytes.len() > MAX_FOOTNOTES_XML_BYTES {
        return Err(format!(
            "DOCX footnotes.xml exceeds the {MAX_FOOTNOTES_XML_BYTES}-byte package limit"
        ));
    }
    let xml = std::str::from_utf8(bytes)
        .map_err(|_| "DOCX footnotes.xml is not valid UTF-8".to_string())?;
    let document = roxmltree::Document::parse(xml)
        .map_err(|error| format!("invalid DOCX footnotes.xml: {error}"))?;
    let root = document.root_element();
    if !root.has_tag_name((W_NS, "footnotes")) {
        return Err("DOCX footnotes.xml has an invalid root element".into());
    }

    let mut definitions = BTreeMap::new();
    let mut special_ids = HashSet::new();
    let mut total_body_bytes = 0usize;
    for note in root.children().filter(|node| node.is_element()) {
        if !note.has_tag_name((W_NS, "footnote")) {
            return Err("DOCX footnotes.xml contains an unexpected element".into());
        }
        let raw_id = note
            .attribute((W_NS, "id"))
            .ok_or_else(|| "DOCX footnote is missing w:id".to_string())?;
        let id = raw_id
            .parse::<i32>()
            .map_err(|_| format!("DOCX footnote has invalid w:id {raw_id:?}"))?;
        let note_type = note.attribute((W_NS, "type")).unwrap_or("normal");
        match (note_type, id) {
            ("separator", -1) => {
                require_special_marker(note, "separator", &mut special_ids)?;
                continue;
            }
            ("continuationSeparator", 0) => {
                require_special_marker(note, "continuationSeparator", &mut special_ids)?;
                continue;
            }
            ("separator" | "continuationSeparator", _) => {
                return Err(format!("DOCX {note_type} footnote has invalid w:id {id}"));
            }
            ("normal", id) if id > 0 => {}
            ("normal", _) => {
                return Err(format!("DOCX normal footnote has invalid w:id {id}"));
            }
            (other, _) => return Err(format!("unsupported DOCX footnote type {other:?}")),
        }
        if definitions.contains_key(&id) {
            return Err(format!("DOCX footnote ID {id} is duplicated"));
        }
        if definitions.len() >= crate::footnotes::MAX_FOOTNOTES_PER_DOCUMENT {
            return Err(format!(
                "DOCX has more than {} footnotes",
                crate::footnotes::MAX_FOOTNOTES_PER_DOCUMENT
            ));
        }
        let body = footnote_body(note)?;
        if body.len() > crate::footnotes::MAX_FOOTNOTE_BODY_BYTES {
            return Err(format!(
                "DOCX footnote {id} exceeds the {}-byte body limit",
                crate::footnotes::MAX_FOOTNOTE_BODY_BYTES
            ));
        }
        total_body_bytes = total_body_bytes.saturating_add(body.len());
        if total_body_bytes > crate::footnotes::MAX_FOOTNOTE_TOTAL_BYTES {
            return Err(format!(
                "DOCX footnote bodies exceed the {}-byte document limit",
                crate::footnotes::MAX_FOOTNOTE_TOTAL_BYTES
            ));
        }
        definitions.insert(id, body);
    }

    let mut seen_ids = HashSet::new();
    let mut seen_anchors = HashSet::new();
    let mut imported = Vec::with_capacity(references.len());
    for reference in references {
        if reference.ooxml_id <= 0 {
            return Err(format!(
                "DOCX footnote reference has invalid w:id {}",
                reference.ooxml_id
            ));
        }
        if !seen_ids.insert(reference.ooxml_id) {
            return Err(format!(
                "DOCX footnote ID {} is referenced more than once",
                reference.ooxml_id
            ));
        }
        let body = definitions.get(&reference.ooxml_id).ok_or_else(|| {
            format!(
                "DOCX footnote reference {} has no matching footnote",
                reference.ooxml_id
            )
        })?;
        if !seen_anchors.insert((reference.block_id, reference.offset)) {
            return Err("DOCX footnote references share an anchor position".into());
        }
        let block = blocks
            .iter()
            .find(|block| block.id == reference.block_id)
            .ok_or_else(|| {
                format!(
                    "DOCX footnote reference {} points to a missing block",
                    reference.ooxml_id
                )
            })?;
        if reference.offset > block.text.len_bytes()
            || !block.text.as_str().is_char_boundary(reference.offset)
        {
            return Err(format!(
                "DOCX footnote reference {} has an invalid UTF-8 anchor",
                reference.ooxml_id
            ));
        }
        imported.push(FootnoteEntry {
            id: reference.ooxml_id as u64,
            block_id: reference.block_id,
            offset: reference.offset,
            body: body.clone(),
        });
    }
    if let Some(id) = definitions.keys().find(|id| !seen_ids.contains(id)) {
        return Err(format!("DOCX footnote {id} has no inline reference"));
    }
    Ok(imported)
}

fn require_special_marker(
    note: Node<'_, '_>,
    marker: &str,
    special_ids: &mut HashSet<i32>,
) -> Result<(), String> {
    let id = note
        .attribute((W_NS, "id"))
        .and_then(|value| value.parse::<i32>().ok())
        .expect("special note ID was parsed");
    if !special_ids.insert(id) {
        return Err(format!("DOCX special footnote ID {id} is duplicated"));
    }
    if !note
        .descendants()
        .any(|node| node.has_tag_name((W_NS, marker)))
    {
        return Err(format!("DOCX special footnote {id} is missing w:{marker}"));
    }
    if note
        .descendants()
        .any(|node| node.has_tag_name((W_NS, "t")))
    {
        return Err(format!("DOCX special footnote {id} contains ordinary text"));
    }
    Ok(())
}

fn footnote_body(note: Node<'_, '_>) -> Result<String, String> {
    let mut paragraphs = Vec::new();
    for child in note.children().filter(|node| node.is_element()) {
        if !child.has_tag_name((W_NS, "p")) {
            return Err("DOCX footnote contains unsupported block content".into());
        }
        paragraphs.push(child);
    }
    if paragraphs.is_empty() {
        return Err("DOCX footnote is missing a paragraph".into());
    }
    let mut body = String::new();
    for (index, paragraph) in paragraphs.iter().enumerate() {
        if index > 0 {
            append_note_text(&mut body, "\n")?;
        }
        append_note_nodes(*paragraph, &mut body)?;
    }
    Ok(body)
}

fn append_note_nodes(node: Node<'_, '_>, out: &mut String) -> Result<(), String> {
    for child in node.children() {
        if child.has_tag_name((W_NS, "t")) {
            append_note_text(out, child.text().unwrap_or_default())?;
        } else if child.has_tag_name((W_NS, "tab")) {
            append_note_text(out, "\t")?;
        } else if child.has_tag_name((W_NS, "br")) || child.has_tag_name((W_NS, "cr")) {
            append_note_text(out, "\n")?;
        } else if child.has_tag_name((W_NS, "drawing"))
            || child.has_tag_name((W_NS, "pict"))
            || child.has_tag_name((W_NS, "object"))
            || child.has_tag_name((W_NS, "tbl"))
            || child.tag_name().namespace() == Some(M_NS)
        {
            return Err("DOCX footnote contains embedded content Writer cannot preserve".into());
        } else if child.is_element() {
            append_note_nodes(child, out)?;
        }
    }
    Ok(())
}

fn append_note_text(out: &mut String, text: &str) -> Result<(), String> {
    if out.len().saturating_add(text.len()) > crate::footnotes::MAX_FOOTNOTE_BODY_BYTES {
        return Err(format!(
            "DOCX footnote exceeds the {}-byte body limit",
            crate::footnotes::MAX_FOOTNOTE_BODY_BYTES
        ));
    }
    out.push_str(text);
    Ok(())
}

fn footnotes_part_path(archive: &PackageArchive) -> Result<Option<String>, String> {
    let relationship_bytes = archive.get("word/_rels/document.xml.rels");
    let part_exists = archive.get("word/footnotes.xml").is_some();
    let Some(bytes) = relationship_bytes else {
        return if part_exists {
            Err("DOCX footnotes.xml has no document relationship".into())
        } else {
            Ok(None)
        };
    };
    let xml = std::str::from_utf8(bytes)
        .map_err(|_| "DOCX document relationships are not valid UTF-8".to_string())?;
    let document = roxmltree::Document::parse(xml)
        .map_err(|error| format!("invalid DOCX document relationships: {error}"))?;
    let root = document.root_element();
    if !root.has_tag_name((PACKAGE_REL_NS, "Relationships")) {
        return Err("DOCX document relationships have an invalid root element".into());
    }
    let matches: Vec<_> = root
        .children()
        .filter(|node| node.has_tag_name((PACKAGE_REL_NS, "Relationship")))
        .filter(|node| node.attribute("Type") == Some(FOOTNOTES_REL))
        .collect();
    if matches.len() > 1 {
        return Err("DOCX has duplicate footnotes relationships".into());
    }
    let Some(relationship) = matches.first() else {
        return if part_exists {
            Err("DOCX footnotes.xml has no document relationship".into())
        } else {
            Ok(None)
        };
    };
    if relationship.attribute("TargetMode") == Some("External") {
        return Err("DOCX footnotes relationship cannot be external".into());
    }
    let target = relationship
        .attribute("Target")
        .ok_or_else(|| "DOCX footnotes relationship is missing Target".to_string())?;
    let path = resolve_part_target(target)?;
    if archive.get(&path).is_none() {
        return Err(format!(
            "DOCX footnotes relationship points to missing {path}"
        ));
    }
    Ok(Some(path))
}

fn resolve_part_target(target: &str) -> Result<String, String> {
    if target.is_empty() || target.contains(['?', '#', '\\']) {
        return Err("DOCX footnotes relationship has an invalid Target".into());
    }
    let mut components = Vec::new();
    let relative = target.strip_prefix('/').unwrap_or(target);
    let source = if target.starts_with('/') {
        relative.to_string()
    } else {
        format!("word/{relative}")
    };
    for component in source.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                if components.pop().is_none() {
                    return Err("DOCX footnotes relationship escapes the package".into());
                }
            }
            value => components.push(value),
        }
    }
    if components.is_empty() {
        return Err("DOCX footnotes relationship has an invalid Target".into());
    }
    Ok(components.join("/"))
}
