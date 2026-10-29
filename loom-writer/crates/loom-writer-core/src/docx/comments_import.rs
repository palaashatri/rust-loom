//! Import Word comment bodies, resolved state, and paragraph-range anchors.

use std::collections::{HashMap, HashSet};

use loom_package::zip::PackageArchive;
use roxmltree::{Document, Node};

use super::xml::{W14_NS, W15_NS, W_NS};
use crate::{CommentThread, RichBlock};

const COMMENTS_REL: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments";
const COMMENTS_EXTENDED_REL: &str =
    "http://schemas.microsoft.com/office/2011/relationships/commentsExtended";

#[derive(Debug, Clone, Copy)]
pub(super) struct CommentAnchor {
    pub id: u32,
    pub block_id: u64,
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Copy)]
enum MarkerKind {
    Start,
    End,
    Reference,
}

#[derive(Default)]
struct Markers {
    start: Option<usize>,
    end: Option<usize>,
    reference: Option<usize>,
}

struct CommentRecord {
    id: u32,
    author: String,
    body: String,
    last_para_id: Option<String>,
}

/// Maps comment range markers in a paragraph to UTF-8 offsets in its visible text.
pub(super) fn paragraph_anchors(
    paragraph_xml: &str,
    root_tag: &str,
    block_id: u64,
) -> Result<Vec<CommentAnchor>, String> {
    let wrapped = format!("{root_tag}<w:body>{paragraph_xml}</w:body></w:document>");
    let document = Document::parse(&wrapped)
        .map_err(|error| format!("invalid DOCX paragraph XML: {error}"))?;
    let paragraph = document
        .root_element()
        .descendants()
        .find(|node| node.has_tag_name((W_NS, "p")))
        .ok_or_else(|| "DOCX paragraph element is missing".to_string())?;
    let mut markers: HashMap<u32, Markers> = HashMap::new();
    let mut visible_offset = 0usize;

    for node in paragraph.descendants().filter(Node::is_element) {
        if node.has_tag_name((W_NS, "r")) {
            let (run_len, has_comment_reference) = run_visible_length(node);
            if has_comment_reference && run_len != 0 {
                return Err("DOCX comment reference run also contains text".into());
            }
            if !node
                .ancestors()
                .any(|ancestor| ancestor.has_tag_name((W_NS, "del")))
            {
                visible_offset = visible_offset
                    .checked_add(run_len)
                    .ok_or_else(|| "DOCX comment anchor offset overflow".to_string())?;
            }
            continue;
        }

        let kind = if node.has_tag_name((W_NS, "commentRangeStart")) {
            Some(MarkerKind::Start)
        } else if node.has_tag_name((W_NS, "commentRangeEnd")) {
            Some(MarkerKind::End)
        } else if node.has_tag_name((W_NS, "commentReference")) {
            Some(MarkerKind::Reference)
        } else {
            None
        };
        let Some(kind) = kind else { continue };
        let inside_run = node
            .ancestors()
            .any(|ancestor| ancestor.has_tag_name((W_NS, "r")));
        if matches!(kind, MarkerKind::Reference) != inside_run {
            return Err("DOCX comment marker is outside its supported run position".into());
        }
        if node
            .ancestors()
            .any(|ancestor| ancestor.has_tag_name((W_NS, "pPr")))
        {
            return Err("DOCX comment marker is inside paragraph properties".into());
        }
        let raw_id = node
            .attribute((W_NS, "id"))
            .ok_or_else(|| "DOCX comment marker is missing w:id".to_string())?;
        let id = raw_id
            .parse::<u32>()
            .map_err(|_| format!("DOCX comment marker has invalid w:id {raw_id:?}"))?;
        let entry = markers.entry(id).or_default();
        let target = match kind {
            MarkerKind::Start => &mut entry.start,
            MarkerKind::End => &mut entry.end,
            MarkerKind::Reference => &mut entry.reference,
        };
        if target.replace(visible_offset).is_some() {
            return Err(format!("DOCX comment {id} has a duplicate range marker"));
        }
    }

    let mut anchors = Vec::with_capacity(markers.len());
    for (id, marker) in markers {
        let (Some(start), Some(end), Some(reference)) =
            (marker.start, marker.end, marker.reference)
        else {
            return Err(format!("DOCX comment {id} has an incomplete range anchor"));
        };
        if start > end || reference < end {
            return Err(format!(
                "DOCX comment {id} has an inverted or dangling range anchor"
            ));
        }
        anchors.push(CommentAnchor {
            id,
            block_id,
            start,
            end,
        });
    }
    anchors.sort_by_key(|anchor| anchor.id);
    Ok(anchors)
}

fn run_visible_length(run: Node<'_, '_>) -> (usize, bool) {
    let mut length = 0usize;
    let mut has_comment_reference = false;
    for node in run.descendants().filter(Node::is_element) {
        if node.tag_name().namespace() != Some(W_NS) {
            continue;
        }
        match node.tag_name().name() {
            "t" => length = length.saturating_add(node.text().unwrap_or_default().len()),
            "tab" | "br" | "cr" => length = length.saturating_add(1),
            "commentReference" => has_comment_reference = true,
            _ => {}
        }
    }
    (length, has_comment_reference)
}

/// Rejects marker elements skipped by the ordinary body/table paragraph walkers.
pub(super) fn validate_document_markers(
    xml: &str,
    anchors: &[CommentAnchor],
) -> Result<(), String> {
    let document =
        Document::parse(xml).map_err(|error| format!("invalid DOCX document.xml: {error}"))?;
    let marker_count = document
        .descendants()
        .filter(|node| {
            node.is_element()
                && node.tag_name().namespace() == Some(W_NS)
                && matches!(
                    node.tag_name().name(),
                    "commentRangeStart" | "commentRangeEnd" | "commentReference"
                )
        })
        .count();
    if marker_count != anchors.len().saturating_mul(3) {
        return Err("DOCX comment markers are outside supported paragraph anchors".into());
    }
    let mut ids = HashSet::with_capacity(anchors.len());
    if anchors.iter().any(|anchor| !ids.insert(anchor.id)) {
        return Err("DOCX comment range ID is reused across paragraphs".into());
    }
    Ok(())
}

pub(super) fn import_document(
    archive: &PackageArchive,
    anchors: &[CommentAnchor],
    blocks: &[RichBlock],
) -> Result<Vec<CommentThread>, String> {
    let comments_xml = related_part_text(archive, COMMENTS_REL, "word/comments.xml")?;
    let extended_xml =
        related_part_text(archive, COMMENTS_EXTENDED_REL, "word/commentsExtended.xml")?;
    let Some(comments_xml) = comments_xml else {
        if !anchors.is_empty() || extended_xml.is_some() {
            return Err("DOCX comment anchor has no related comments part".into());
        }
        return Ok(Vec::new());
    };

    let comments = parse_comments(&comments_xml)?;
    let mut anchor_by_id = HashMap::with_capacity(anchors.len());
    for anchor in anchors {
        if anchor_by_id.insert(anchor.id, *anchor).is_some() {
            return Err(format!(
                "DOCX comment {} is anchored more than once",
                anchor.id
            ));
        }
        let block = blocks
            .iter()
            .find(|block| block.id == anchor.block_id)
            .ok_or_else(|| format!("DOCX comment {} points to a missing paragraph", anchor.id))?;
        if block.kind == crate::TABLE_BLOCK_KIND
            || anchor.start > anchor.end
            || anchor.end > block.text.len_bytes()
            || !block.text.as_str().is_char_boundary(anchor.start)
            || !block.text.as_str().is_char_boundary(anchor.end)
        {
            return Err(format!(
                "DOCX comment {} has a dangling paragraph anchor",
                anchor.id
            ));
        }
    }
    let resolved = extended_xml
        .as_deref()
        .map(|xml| parse_comments_extended(xml, &comments))
        .transpose()?
        .unwrap_or_default();
    if comments.len() != anchor_by_id.len()
        || comments
            .iter()
            .any(|comment| !anchor_by_id.contains_key(&comment.id))
    {
        return Err("DOCX comment part and document anchors do not match".into());
    }

    comments
        .into_iter()
        .map(|comment| {
            let anchor = anchor_by_id[&comment.id];
            Ok(CommentThread {
                id: comment.id.to_string(),
                author: comment.author,
                block_id: anchor.block_id,
                start: anchor.start,
                end: anchor.end,
                body: comment.body,
                resolved: resolved.get(&comment.id).copied().unwrap_or(false),
                orphaned: false,
            })
        })
        .collect()
}

fn related_part_text(
    archive: &PackageArchive,
    relationship_type: &str,
    conventional_path: &str,
) -> Result<Option<String>, String> {
    let Some(rels_bytes) = archive.get("word/_rels/document.xml.rels") else {
        if archive.get(conventional_path).is_some() {
            return Err(format!("DOCX part {conventional_path} has no relationship"));
        }
        return Ok(None);
    };
    let rels_xml = std::str::from_utf8(rels_bytes)
        .map_err(|_| "DOCX document relationships are not valid UTF-8".to_string())?;
    let rels = Document::parse(rels_xml)
        .map_err(|error| format!("invalid DOCX document relationships: {error}"))?;
    let matches: Vec<_> = rels
        .root_element()
        .children()
        .filter(|node| {
            node.is_element()
                && node.tag_name().name() == "Relationship"
                && node.attribute("Type") == Some(relationship_type)
        })
        .collect();
    if matches.len() > 1 {
        return Err(format!(
            "DOCX has duplicate relationships for {conventional_path}"
        ));
    }
    let Some(relation) = matches.first() else {
        if archive.get(conventional_path).is_some() {
            return Err(format!("DOCX part {conventional_path} is not related"));
        }
        return Ok(None);
    };
    if relation.attribute("TargetMode") == Some("External") {
        return Err(format!("DOCX {conventional_path} relationship is external"));
    }
    let target = relation
        .attribute("Target")
        .ok_or_else(|| format!("DOCX relationship for {conventional_path} is missing Target"))?;
    let path = resolve_target(target)?;
    let bytes = archive
        .get(&path)
        .ok_or_else(|| format!("DOCX relationship target {path} is missing"))?;
    let text =
        std::str::from_utf8(bytes).map_err(|_| format!("DOCX part {path} is not valid UTF-8"))?;
    Ok(Some(text.to_owned()))
}

fn resolve_target(target: &str) -> Result<String, String> {
    if target.contains('%') || target.contains('?') || target.contains('#') {
        return Err("escaped or parameterized DOCX comment part paths are unsupported".into());
    }
    let mut parts: Vec<&str> = if target.starts_with('/') {
        Vec::new()
    } else {
        vec!["word"]
    };
    for part in target.trim_start_matches('/').split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if parts.pop().is_none() {
                    return Err("DOCX comment relationship escapes the package root".into());
                }
            }
            value => parts.push(value),
        }
    }
    if parts.is_empty() {
        return Err("DOCX comment relationship has an empty part path".into());
    }
    Ok(parts.join("/"))
}

fn parse_comments(xml: &str) -> Result<Vec<CommentRecord>, String> {
    let document =
        Document::parse(xml).map_err(|error| format!("invalid DOCX comments part: {error}"))?;
    let root = document.root_element();
    if !root.has_tag_name((W_NS, "comments")) {
        return Err("DOCX comments part has the wrong root element".into());
    }
    let mut ids = HashSet::new();
    let mut comments = Vec::new();
    for node in root
        .children()
        .filter(|node| node.has_tag_name((W_NS, "comment")))
    {
        let raw_id = node
            .attribute((W_NS, "id"))
            .ok_or_else(|| "DOCX comment is missing w:id".to_string())?;
        let id = raw_id
            .parse::<u32>()
            .map_err(|_| format!("DOCX comment has invalid w:id {raw_id:?}"))?;
        if id > super::comments::MAX_COMMENT_ID {
            return Err(format!(
                "DOCX comment ID {id} exceeds the supported paragraph-ID range"
            ));
        }
        if !ids.insert(id) {
            return Err(format!("DOCX comment ID {id} is duplicated"));
        }
        let author = node
            .attribute((W_NS, "author"))
            .ok_or_else(|| format!("DOCX comment {id} is missing w:author"))?
            .to_owned();
        let paragraphs: Vec<_> = node
            .children()
            .filter(|child| child.has_tag_name((W_NS, "p")))
            .collect();
        let mut lines = Vec::with_capacity(paragraphs.len());
        let mut last_para_id = None;
        for paragraph in paragraphs {
            lines.push(comment_paragraph_text(paragraph, id)?);
            last_para_id = paragraph.attribute((W14_NS, "paraId")).map(str::to_owned);
        }
        comments.push(CommentRecord {
            id,
            author,
            body: lines.join("\n"),
            last_para_id,
        });
    }
    Ok(comments)
}

fn comment_paragraph_text(paragraph: Node<'_, '_>, id: u32) -> Result<String, String> {
    let mut text = String::new();
    for node in paragraph.descendants().filter(Node::is_element) {
        if node.tag_name().namespace() != Some(W_NS) {
            continue;
        }
        match node.tag_name().name() {
            "t" => text.push_str(node.text().unwrap_or_default()),
            "tab" => text.push('\t'),
            "br" | "cr" => text.push('\n'),
            "p" | "pPr" | "pStyle" | "r" | "rPr" | "rStyle" | "annotationRef" | "proofErr"
            | "bookmarkStart" | "bookmarkEnd" => {}
            "commentRangeStart" | "commentRangeEnd" | "commentReference" => {
                return Err(format!("DOCX comment {id} contains nested comment markup"));
            }
            "b" | "bCs" | "i" | "iCs" | "u" | "strike" | "color" | "sz" | "szCs" | "rFonts"
            | "vertAlign" => {
                return Err(format!(
                    "DOCX comment {id} has unsupported rich text formatting"
                ));
            }
            name => {
                return Err(format!("DOCX comment {id} contains unsupported w:{name}"));
            }
        }
    }
    Ok(text)
}

fn parse_comments_extended(
    xml: &str,
    comments: &[CommentRecord],
) -> Result<HashMap<u32, bool>, String> {
    let document = Document::parse(xml)
        .map_err(|error| format!("invalid DOCX extended comments part: {error}"))?;
    if !document.root_element().has_tag_name((W15_NS, "commentsEx")) {
        return Err("DOCX extended comments part has the wrong root element".into());
    }
    let by_para_id: HashMap<&str, u32> = comments
        .iter()
        .filter_map(|comment| comment.last_para_id.as_deref().map(|id| (id, comment.id)))
        .collect();
    let mut resolved = HashMap::new();
    for node in document
        .root_element()
        .children()
        .filter(|node| node.has_tag_name((W15_NS, "commentEx")))
    {
        let para_id = node
            .attribute((W15_NS, "paraId"))
            .ok_or_else(|| "DOCX commentEx is missing w15:paraId".to_string())?;
        let id = *by_para_id
            .get(para_id)
            .ok_or_else(|| format!("DOCX commentEx points to unknown paragraph ID {para_id}"))?;
        let done = node
            .attribute((W15_NS, "done"))
            .ok_or_else(|| format!("DOCX commentEx {para_id} is missing w15:done"))?;
        let value = match done {
            "1" | "true" | "on" => true,
            "0" | "false" | "off" => false,
            _ => return Err(format!("DOCX commentEx {para_id} has invalid w15:done")),
        };
        if resolved.insert(id, value).is_some() {
            return Err(format!(
                "DOCX comment {id} has duplicate resolved-state entries"
            ));
        }
    }
    Ok(resolved)
}
