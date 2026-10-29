//! Word document (`.docx`) import into [`WriterDocument`].
//!
//! Imports paragraphs, headings (1-6), lists (bulleted and numbered),
//! inline character formatting (bold, italic, underline, strikethrough, fonts),
//! tables (`<w:tbl>`), and document core properties (title).

use loom_objects::table::TextRun;
use loom_package::zip::PackageArchive;
use loom_text::{CharacterStyle, FontWeight, StyleRun};
use roxmltree::Node;

use super::header_footer;
use super::inline_style::{attribute_value, property_tag_with_start, writer_color};
use super::paragraph_style_import::LocalParagraphFormatChange;
use super::xml::W_NS;
use crate::revisions::{RevisionBlockFormat, RevisionBlockFormatChange};
use crate::{
    PageAppearance, PageOrientation, PageSetup, RevisionAnchor, RevisionBoundaryKind, RevisionKind,
    RevisionPayload, RevisionRecord, RichBlock, TrackedChanges, WriterDocument,
};

mod table;
use table::parse_table;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct FootnoteReference {
    pub ooxml_id: i32,
    pub block_id: u64,
    pub offset: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LocalFootnoteReference {
    ooxml_id: i32,
    offset: usize,
}

#[derive(Clone)]
struct RevisionMarkup {
    id: u64,
    author: String,
    kind: RevisionKind,
}

struct LocalRevision {
    id: u64,
    author: String,
    kind: RevisionKind,
    boundary_kind: RevisionBoundaryKind,
    start: usize,
    end: usize,
    before: Option<RevisionPayload>,
    after: Option<RevisionPayload>,
}

/// Unescapes predefined and numeric XML entities without splitting UTF-8 bytes.
pub(crate) fn xml_unescape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut cursor = 0usize;
    while let Some(relative_ampersand) = value[cursor..].find('&') {
        let start = cursor + relative_ampersand;
        out.push_str(&value[cursor..start]);
        let Some(relative_semicolon) = value[start..].find(';') else {
            out.push('&');
            cursor = start + 1;
            continue;
        };
        let end = start + relative_semicolon;
        let entity = &value[start + 1..end];
        let decoded = match entity {
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "amp" => Some('&'),
            value if value.starts_with("#x") || value.starts_with("#X") => {
                u32::from_str_radix(&value[2..], 16)
                    .ok()
                    .and_then(char::from_u32)
            }
            value if value.starts_with('#') => {
                value[1..].parse::<u32>().ok().and_then(char::from_u32)
            }
            _ => None,
        };
        if let Some(character) = decoded {
            out.push(character);
        } else {
            out.push_str(&value[start..=end]);
        }
        cursor = end + 1;
    }
    out.push_str(&value[cursor..]);
    out
}

/// The `w:val` of the paragraph's `w:ilvl`, the numbering level it sits at.
fn num_pr_level(p_xml: &str) -> Option<String> {
    let start = p_xml.find("<w:numPr")?;
    let rest = &p_xml[start..];
    let snippet = &rest[..rest.find("</w:numPr>").unwrap_or(rest.len())];
    let at = snippet.find("<w:ilvl")?;
    extract_attribute_value(&snippet[at..], "w:val")
}

/// Inspects a `.docx` package archive for unsupported constructs.
pub fn inspect_docx_unsupported(docx_bytes: &[u8]) -> Result<Vec<String>, String> {
    let archive = PackageArchive::from_bytes(docx_bytes)
        .map_err(|err| format!("unreadable docx archive: {err}"))?;
    let mut warnings = Vec::new();

    for path in archive.paths() {
        if (path.starts_with("word/diagrams/") || path.contains("diagram"))
            && !warnings.iter().any(|w: &String| w.contains("SmartArt"))
        {
            warnings.push("SmartArt diagrams are not supported and will be omitted.".to_string());
        }
        if path.starts_with("word/embeddings/")
            && !warnings.iter().any(|w: &String| w.contains("OLE"))
        {
            warnings.push("Embedded OLE objects are not supported.".to_string());
        }
        if (path.contains("vbaProject.bin") || path.contains("vbaData.xml"))
            && !warnings.iter().any(|w: &String| w.contains("VBA"))
        {
            warnings.push("Macros and VBA code are not supported.".to_string());
        }
        if path.ends_with("endnotes.xml")
            && !warnings
                .iter()
                .any(|warning: &String| warning.contains("Endnotes"))
        {
            warnings.push("Endnotes are not supported and will be omitted.".to_string());
        }
        let lower = path.to_ascii_lowercase();
        if (path.starts_with("word/media/") || path.starts_with("media/"))
            && ![".png", ".jpg", ".jpeg"]
                .iter()
                .any(|ext| lower.ends_with(ext))
            && !warnings.iter().any(|w: &String| w.contains("PNG and JPEG"))
        {
            warnings.push(
                "Pictures other than PNG and JPEG are not supported and will be omitted."
                    .to_string(),
            );
        }
    }

    if let Some(doc_bytes) = archive.get("word/document.xml") {
        let xml = String::from_utf8_lossy(doc_bytes);
        let unsupported_background =
            roxmltree::Document::parse(&xml)
                .ok()
                .is_some_and(|document| {
                    document
                        .root_element()
                        .children()
                        .find(|node| node.has_tag_name((W_NS, "background")))
                        .is_some_and(|background| {
                            supported_background_appearance(background).is_none()
                        })
                });
        if unsupported_background
            && !warnings
                .iter()
                .any(|warning: &String| warning.contains("document background"))
        {
            warnings.push(
                "Non-black/white, theme, or drawing document backgrounds are not supported and will be omitted."
                    .to_string(),
            );
        }
        if (xml.contains("<m:oMath") || xml.contains("<m:oMathPara"))
            && !warnings.iter().any(|w: &String| w.contains("Equation"))
        {
            warnings.push("Mathematical equations (OMML) are not supported.".to_string());
        }
        if xml.contains("<w:endnoteReference")
            && !warnings
                .iter()
                .any(|warning: &String| warning.contains("Endnotes"))
        {
            warnings.push("Endnotes are not supported and will be omitted.".to_string());
        }
        if (xml.contains("<w:pict") || xml.contains("<v:imagedata"))
            && !warnings.iter().any(|w: &String| w.contains("older"))
        {
            warnings.push(
                "Pictures in the older Word format are not supported and will be omitted."
                    .to_string(),
            );
        }
        if xml.contains("<wp:anchor") && !warnings.iter().any(|w: &String| w.contains("Floating")) {
            warnings.push("Floating pictures are placed in the text, one to a line.".to_string());
        }
    }

    Ok(warnings)
}

pub fn import_docx(docx_bytes: &[u8]) -> Result<WriterDocument, String> {
    let archive = PackageArchive::from_bytes(docx_bytes)
        .map_err(|e| format!("unreadable docx archive: {e}"))?;
    let document_xml = archive
        .get("word/document.xml")
        .ok_or_else(|| "docx archive missing word/document.xml".to_string())?;
    let xml = std::str::from_utf8(document_xml)
        .map_err(|_| "document.xml is not valid UTF-8".to_string())?;

    // Document title from docProps/core.xml when available.
    let title = archive
        .get("docProps/core.xml")
        .and_then(|bytes| std::str::from_utf8(bytes).ok())
        .and_then(extract_core_title)
        .unwrap_or_else(|| "Untitled Document".to_string());

    let (blocks, references, revisions, block_formats, comment_anchors) =
        parse_document_body_and_references(xml, Some(&archive))?;
    let document = roxmltree::Document::parse(xml)
        .map_err(|error| format!("invalid DOCX document.xml: {error}"))?;
    let reference_count = document
        .descendants()
        .filter(|node| node.has_tag_name((W_NS, "footnoteReference")))
        .count();
    if reference_count != references.len() {
        return Err(
            "DOCX footnote reference is outside a supported paragraph or table cell".into(),
        );
    }
    let footnotes = super::footnotes::import_document(&archive, &references, &blocks)?;
    let comments = super::comments_import::import_document(&archive, &comment_anchors, &blocks)?;
    let recording = import_track_revisions(&archive)?;
    let tracked_changes_author = super::package::import_tracked_changes_author(&archive)?;

    let mut doc = WriterDocument::new("imported-doc", title);
    doc.page = import_page_setup(xml)?;
    doc.header_footer = header_footer::import_config(xml, &archive)?;
    doc.blocks = blocks;
    doc.footnotes = footnotes;
    doc.comments = comments;
    doc.header_footer
        .validate_for_page_style(&doc.page.page_style())
        .map_err(str::to_owned)?;
    doc.import_docx_revisions(revisions, block_formats, recording, tracked_changes_author)?;

    Ok(doc)
}

fn supported_background_appearance(background: roxmltree::Node<'_, '_>) -> Option<PageAppearance> {
    if background.children().any(|child| child.is_element())
        || background
            .attributes()
            .any(|attribute| attribute.name() != "color" || attribute.namespace() != Some(W_NS))
    {
        return None;
    }
    match background.attribute((W_NS, "color"))? {
        color if color.eq_ignore_ascii_case("000000") => Some(PageAppearance::Black),
        color if color.eq_ignore_ascii_case("FFFFFF") => Some(PageAppearance::White),
        _ => None,
    }
}

fn import_page_setup(xml: &str) -> Result<PageSetup, String> {
    let document = roxmltree::Document::parse(xml)
        .map_err(|error| format!("invalid DOCX document.xml: {error}"))?;
    let appearance = document
        .root_element()
        .children()
        .find(|node| node.has_tag_name((W_NS, "background")))
        .and_then(supported_background_appearance)
        .unwrap_or_default();
    let mut style = PageSetup::default().page_style();
    let mut orientation = PageOrientation::Portrait;
    let mut imported = None;

    for section in document
        .descendants()
        .filter(|node| node.has_tag_name((W_NS, "sectPr")))
    {
        let size = section
            .children()
            .find(|node| node.has_tag_name((W_NS, "pgSz")));
        let margins = section
            .children()
            .find(|node| node.has_tag_name((W_NS, "pgMar")));
        if let Some(size) = size {
            style.width_pt = twips(size, "w")?;
            style.height_pt = twips(size, "h")?;
            orientation = match size.attribute((W_NS, "orient")) {
                Some("landscape") if style.width_pt >= style.height_pt => {
                    PageOrientation::Landscape
                }
                Some("portrait") if style.width_pt <= style.height_pt => PageOrientation::Portrait,
                Some("landscape" | "portrait") => {
                    return Err("DOCX page orientation does not match its dimensions".into());
                }
                Some(_) => return Err("DOCX page orientation is invalid".into()),
                None if style.width_pt > style.height_pt => PageOrientation::Landscape,
                None if style.width_pt < style.height_pt => PageOrientation::Portrait,
                None => orientation,
            };
        }
        if let Some(margins) = margins {
            if let Some(value) = optional_twips(margins, "top")? {
                style.margin_top_pt = value;
            }
            if let Some(value) = optional_twips(margins, "bottom")? {
                style.margin_bottom_pt = value;
            }
            if let Some(value) = optional_twips(margins, "left")? {
                style.margin_left_pt = value;
            }
            if let Some(value) = optional_twips(margins, "right")? {
                style.margin_right_pt = value;
            }
        }
        if size.is_some() || margins.is_some() {
            let mut setup = PageSetup::default();
            setup
                .set_custom_geometry_points(
                    style.width_pt,
                    style.height_pt,
                    style.margin_top_pt,
                    style.margin_bottom_pt,
                    style.margin_left_pt,
                    style.margin_right_pt,
                )
                .map_err(|error| format!("invalid DOCX page geometry: {error}"))?;
            setup.select_orientation(orientation);
            if imported.is_some_and(|previous| previous != setup) {
                return Err("DOCX sections with different page geometry are not supported".into());
            }
            imported = Some(setup);
        }
    }

    let mut imported = imported.unwrap_or_default();
    imported.appearance = appearance;
    Ok(imported)
}

fn twips(node: roxmltree::Node<'_, '_>, name: &str) -> Result<f32, String> {
    node.attribute((W_NS, name))
        .ok_or_else(|| format!("DOCX page geometry is missing {name}"))?
        .parse::<u32>()
        .map(|value| value as f32 / 20.0)
        .map_err(|_| format!("DOCX page geometry has invalid {name}"))
}

fn optional_twips(node: roxmltree::Node<'_, '_>, name: &str) -> Result<Option<f32>, String> {
    node.attribute((W_NS, name))
        .map(|value| {
            value
                .parse::<u32>()
                .map(|value| value as f32 / 20.0)
                .map_err(|_| format!("DOCX page geometry has invalid {name}"))
        })
        .transpose()
}

/// Parses the contents of `<w:body>` into a list of [`RichBlock`]s.
pub fn parse_document_body(xml: &str) -> Result<Vec<RichBlock>, String> {
    parse_document_body_with_archive(xml, None)
}

pub fn parse_document_body_with_archive(
    xml: &str,
    archive: Option<&PackageArchive>,
) -> Result<Vec<RichBlock>, String> {
    parse_document_body_and_references(xml, archive).map(|(blocks, _, _, _, _)| blocks)
}

fn parse_document_body_and_references(
    xml: &str,
    archive: Option<&PackageArchive>,
) -> Result<
    (
        Vec<RichBlock>,
        Vec<FootnoteReference>,
        Vec<RevisionRecord>,
        std::collections::BTreeMap<u64, RevisionBlockFormatChange>,
        Vec<super::comments_import::CommentAnchor>,
    ),
    String,
> {
    validate_revision_markup_scope(xml)?;
    let root_tag = document_root_tag(xml)?;
    let body_start_tag = "<w:body";
    let body_start = match xml.find(body_start_tag) {
        Some(pos) => {
            let after = pos + body_start_tag.len();
            let close = xml[after..]
                .find('>')
                .ok_or_else(|| "malformed <w:body> tag".to_string())?;
            after + close + 1
        }
        None => return Err("docx missing <w:body>".to_string()),
    };

    let body_end = xml[body_start..]
        .find("</w:body>")
        .map(|pos| body_start + pos)
        .unwrap_or(xml.len());

    let body_xml = &xml[body_start..body_end];
    let mut blocks = Vec::new();
    let mut references = Vec::new();
    let mut revisions: Vec<RevisionRecord> = Vec::new();
    let mut block_formats = std::collections::BTreeMap::new();
    let mut comment_anchors = Vec::new();
    let mut cursor = 0usize;
    let mut next_id = 1u64;

    while cursor < body_xml.len() {
        let rest = &body_xml[cursor..];
        let p_pos = find_tag_start(rest, "w:p");
        let tbl_pos = find_tag_start(rest, "w:tbl");

        match (p_pos, tbl_pos) {
            (Some(p_rel), Some(tbl_rel)) if tbl_rel < p_rel => {
                let tbl_abs = cursor + tbl_rel;
                if let Some((block, next_cursor, table_references, table_revisions)) =
                    parse_table(body_xml, tbl_abs, next_id, root_tag)?
                {
                    if table_revisions
                        .iter()
                        .any(|revision| revisions.iter().any(|existing| existing.id == revision.id))
                    {
                        return Err("DOCX revision ID is reused across document blocks".into());
                    }
                    revisions.extend(table_revisions);
                    blocks.push(block);
                    references.extend(table_references);
                    next_id += 1;
                    cursor = next_cursor;
                } else {
                    cursor = tbl_abs + "<w:tbl".len();
                }
            }
            (Some(_), Some(_)) | (Some(_), None) => {
                let p_rel = p_pos.unwrap();
                let p_abs = cursor + p_rel;
                let mut local_references = Vec::new();
                let mut local_revisions = Vec::new();
                let mut local_paragraph_formats = Vec::new();
                if let Some((block_opt, next_cursor)) = parse_paragraph(
                    body_xml,
                    p_abs,
                    next_id,
                    archive,
                    root_tag,
                    &mut local_references,
                    &mut local_revisions,
                    &mut local_paragraph_formats,
                    &mut comment_anchors,
                )? {
                    if let Some(block) = block_opt {
                        references.extend(local_references.into_iter().map(|reference| {
                            FootnoteReference {
                                ooxml_id: reference.ooxml_id,
                                block_id: block.id,
                                offset: reference.offset,
                            }
                        }));
                        if local_revisions.iter().any(|revision| {
                            block_formats.contains_key(&revision.id)
                                || local_paragraph_formats
                                    .iter()
                                    .any(|change| change.id == revision.id)
                        }) {
                            return Err(
                                "DOCX revision ID collides between inline and paragraph formatting"
                                    .into(),
                            );
                        }
                        for revision in local_revisions {
                            if let Some(existing) = revisions
                                .iter_mut()
                                .find(|existing| existing.id == revision.id)
                            {
                                let previous = blocks.last().ok_or_else(|| {
                                    "DOCX revision ID cannot span an empty document".to_string()
                                })?;
                                WriterDocument::merge_docx_paragraph_revision(
                                    existing, revision, previous, &block,
                                )?;
                            } else {
                                revisions.push(revision);
                            }
                        }
                        for change in local_paragraph_formats {
                            if revisions.iter().any(|revision| revision.id == change.id)
                                || block_formats.contains_key(&change.id)
                            {
                                return Err(
                                    "DOCX reuses a revision ID for paragraph formatting".into()
                                );
                            }
                            let payload = RevisionPayload {
                                text: block.text.as_str().to_owned(),
                                runs: block.runs.clone(),
                            };
                            revisions.push(RevisionRecord {
                                id: change.id,
                                author: change.author,
                                anchor: RevisionAnchor {
                                    block_id: block.id,
                                    start: 0,
                                    end: block.text.len_bytes(),
                                    end_block_id: None,
                                    prefix_sha256: crate::zip::sha256(&[]),
                                },
                                kind: RevisionKind::Formatting,
                                boundary_kind: RevisionBoundaryKind::Unknown,
                                before: Some(payload.clone()),
                                after: Some(payload),
                            });
                            block_formats.insert(
                                change.id,
                                RevisionBlockFormatChange {
                                    before: change.before,
                                    after: change.after,
                                },
                            );
                        }
                        blocks.push(block);
                        next_id += 1;
                    } else if !local_references.is_empty() {
                        return Err("DOCX footnote reference has no paragraph block".into());
                    } else if !local_revisions.is_empty() {
                        return Err("DOCX tracked change has no paragraph block".into());
                    }
                    cursor = next_cursor;
                } else {
                    cursor = p_abs + "<w:p".len();
                }
            }
            (None, Some(tbl_rel)) => {
                let tbl_abs = cursor + tbl_rel;
                if let Some((block, next_cursor, table_references, table_revisions)) =
                    parse_table(body_xml, tbl_abs, next_id, root_tag)?
                {
                    if table_revisions
                        .iter()
                        .any(|revision| revisions.iter().any(|existing| existing.id == revision.id))
                    {
                        return Err("DOCX revision ID is reused across document blocks".into());
                    }
                    revisions.extend(table_revisions);
                    blocks.push(block);
                    references.extend(table_references);
                    next_id += 1;
                    cursor = next_cursor;
                } else {
                    cursor = tbl_abs + "<w:tbl".len();
                }
            }
            (None, None) => break,
        }
    }

    table::normalize_paragraph_mark_revisions(
        &mut blocks,
        &references,
        &mut revisions,
        &comment_anchors,
    )?;
    super::comments_import::validate_document_markers(xml, &comment_anchors)?;
    Ok((
        blocks,
        references,
        revisions,
        block_formats,
        comment_anchors,
    ))
}

fn find_tag_start(xml: &str, tag_name: &str) -> Option<usize> {
    let mut offset = 0;
    let needle = format!("<{tag_name}");
    while let Some(pos) = xml[offset..].find(&needle) {
        let candidate = offset + pos;
        let after = candidate + needle.len();
        if after < xml.len() {
            let next_byte = xml.as_bytes()[after];
            if next_byte == b'>'
                || next_byte == b' '
                || next_byte == b'/'
                || next_byte == b'\n'
                || next_byte == b'\r'
                || next_byte == b'\t'
            {
                return Some(candidate);
            }
        }
        offset = candidate + needle.len();
    }
    None
}

/// A paragraph holding one inline picture as a picture block. `None` when
/// the paragraph has no picture, or its picture is not a PNG or JPEG the
/// package carries (the paragraph then imports as text, as before).
fn parse_picture(p_xml: &str, id: u64, archive: &PackageArchive) -> Option<RichBlock> {
    let drawing = &p_xml[p_xml.find("<w:drawing")?..];
    if drawing.contains("<c:chart") {
        return None;
    }
    let blip = &drawing[drawing.find("<a:blip")?..];
    let rid = extract_attribute_value(blip, "r:embed")?;
    let rels_xml = archive
        .get("word/_rels/document.xml.rels")
        .map(|b| String::from_utf8_lossy(b).to_string())
        .unwrap_or_default();
    let target = parse_rels(&rels_xml).get(&rid)?.clone();
    let path = match target.strip_prefix('/') {
        Some(absolute) => absolute.to_string(),
        None => format!("word/{target}"),
    };
    let name = extract_attribute_value(drawing, "descr")
        .filter(|descr| !descr.trim().is_empty())
        .or_else(|| path.rsplit('/').next().map(str::to_string))
        .unwrap_or_else(|| "picture".to_string());
    let mut image = crate::InlineImage::from_bytes(&name, archive.get(&path)?.to_vec()).ok()?;
    // The size Word shows it at, in English Metric Units.
    if let Some(extent) = drawing.find("<wp:extent") {
        let extent = &drawing[extent..];
        let emu = |attr: &str| {
            extract_attribute_value(extent, attr)
                .and_then(|v| v.parse::<f32>().ok())
                .filter(|v| *v > 0.0)
        };
        if let (Some(cx), Some(cy)) = (emu("cx"), emu("cy")) {
            image.width_pt = cx / 12_700.0;
            image.height_pt = cy / 12_700.0;
        }
    }
    let text = crate::images::picture_text(id as u32);
    let mut block = RichBlock::new(id, crate::IMAGE_BLOCK_KIND, &text);
    block.image = Some(image);
    Some(block)
}

fn validate_revision_markup_scope(xml: &str) -> Result<(), String> {
    let document = roxmltree::Document::parse(xml)
        .map_err(|error| format!("invalid DOCX document.xml: {error}"))?;
    let body = document
        .root_element()
        .descendants()
        .find(|node| node.has_tag_name((W_NS, "body")))
        .ok_or_else(|| "docx missing <w:body>".to_string())?;

    for node in body
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().namespace() == Some(W_NS))
    {
        let name = node.tag_name().name();
        let has_ancestor = |tag| {
            node.ancestors()
                .any(|ancestor| ancestor.has_tag_name((W_NS, tag)))
        };
        if name.starts_with("move") || name.starts_with("customXmlMove") {
            return Err(format!(
                "DOCX move revision element w:{name} is not supported"
            ));
        }
        if name.starts_with("customXmlIns")
            || name.starts_with("customXmlDel")
            || matches!(name, "cellIns" | "cellDel")
        {
            return Err(format!("DOCX revision element w:{name} is not supported"));
        }
        if name == "ins" || name == "del" {
            if has_ancestor("pPr") {
                let in_paragraph_mark_properties =
                    has_ancestor("p") && has_ancestor("rPr") && !has_ancestor("r");
                if !in_paragraph_mark_properties {
                    return Err(format!(
                        "DOCX paragraph-mark revision w:{name} is outside w:pPr/w:rPr"
                    ));
                }
                continue;
            }
            if !has_ancestor("p") || has_ancestor("r") {
                return Err(format!(
                    "block-level DOCX revision wrapper w:{name} is not supported"
                ));
            }
            continue;
        }
        if name == "pPrChange" {
            let is_paragraph_property_change = node.parent().is_some_and(|ppr| {
                ppr.has_tag_name((W_NS, "pPr"))
                    && ppr
                        .parent()
                        .is_some_and(|paragraph| paragraph.has_tag_name((W_NS, "p")))
            });
            if !is_paragraph_property_change {
                return Err("DOCX pPrChange is outside a paragraph's direct w:pPr".into());
            }
            if has_ancestor("tbl") {
                return Err(
                    "tracked paragraph formatting inside DOCX tables is unsupported".into(),
                );
            }
            continue;
        }
        if name == "rPrChange" {
            if !has_ancestor("r") || has_ancestor("pPr") {
                return Err("DOCX run-property revision is outside a supported run".into());
            }
            continue;
        }
        if name.ends_with("Change") {
            return Err(format!("DOCX revision element w:{name} is not supported"));
        }
    }
    Ok(())
}

fn document_root_tag(xml: &str) -> Result<&str, String> {
    let start = xml
        .find("<w:document")
        .ok_or_else(|| "DOCX is missing its w:document root".to_string())?;
    let end = xml[start..]
        .find('>')
        .map(|relative| start + relative + 1)
        .ok_or_else(|| "malformed DOCX w:document tag".to_string())?;
    Ok(&xml[start..end])
}

fn import_track_revisions(archive: &PackageArchive) -> Result<bool, String> {
    let Some(bytes) = archive.get("word/settings.xml") else {
        return Ok(false);
    };
    let settings = std::str::from_utf8(bytes)
        .map_err(|_| "DOCX settings.xml is not valid UTF-8".to_string())?;
    let document = roxmltree::Document::parse(settings)
        .map_err(|error| format!("invalid DOCX settings.xml: {error}"))?;
    let mut nodes = document
        .descendants()
        .filter(|node| node.has_tag_name((W_NS, "trackRevisions")));
    let Some(node) = nodes.next() else {
        return Ok(false);
    };
    if nodes.next().is_some() {
        return Err("DOCX settings contain duplicate trackRevisions elements".into());
    }
    match node.attribute((W_NS, "val")) {
        None | Some("1" | "true" | "on") => Ok(true),
        Some("0" | "false" | "off") => Ok(false),
        Some(value) => Err(format!("DOCX trackRevisions has invalid value {value:?}")),
    }
}

fn revision_markup(node: Node<'_, '_>, kind: RevisionKind) -> Result<RevisionMarkup, String> {
    let raw_id = node
        .attribute((W_NS, "id"))
        .ok_or_else(|| "DOCX tracked change is missing w:id".to_string())?;
    let id = raw_id
        .parse::<u64>()
        .map_err(|_| format!("DOCX tracked change has invalid w:id {raw_id:?}"))?;
    let author = node
        .attribute((W_NS, "author"))
        .ok_or_else(|| "DOCX tracked change is missing w:author".to_string())?
        .to_owned();
    let mut settings = TrackedChanges::default();
    settings
        .set_author(author.clone())
        .map_err(|error| format!("invalid DOCX tracked-change author: {error}"))?;
    Ok(RevisionMarkup { id, author, kind })
}

fn collect_revision_runs<'a, 'input>(
    node: Node<'a, 'input>,
    context: Option<RevisionMarkup>,
    runs: &mut Vec<(Node<'a, 'input>, Option<RevisionMarkup>)>,
) -> Result<(), String> {
    if node.is_element() && node.tag_name().namespace() == Some(W_NS) {
        let name = node.tag_name().name();
        if name == "r" {
            runs.push((node, context));
            return Ok(());
        }
        if name == "ins" || name == "del" {
            if has_paragraph_properties_ancestor(node) {
                return Ok(());
            }
            if context.is_some() {
                return Err("nested DOCX insertion/deletion revisions are unsupported".into());
            }
            let kind = if name == "ins" {
                RevisionKind::Insertion
            } else {
                RevisionKind::Deletion
            };
            let markup = revision_markup(node, kind)?;
            let count = runs.len();
            for child in node.children() {
                collect_revision_runs(child, Some(markup.clone()), runs)?;
            }
            if runs.len() == count {
                return Err("DOCX tracked change has no supported text runs".into());
            }
            return Ok(());
        }
        if name == "pPrChange" {
            return Ok(());
        }
        if name == "rPrChange" && has_paragraph_properties_ancestor(node) {
            return Err("paragraph-mark formatting revisions are not supported".into());
        }
        if (name.ends_with("Change") && name != "rPrChange")
            || name.starts_with("move")
            || name.starts_with("customXmlMove")
        {
            return Err(format!("DOCX revision element w:{name} is not supported"));
        }
    }
    for child in node.children() {
        if child.is_element() {
            collect_revision_runs(child, context.clone(), runs)?;
        }
    }
    Ok(())
}

fn has_paragraph_properties_ancestor(node: Node<'_, '_>) -> bool {
    let mut parent = node.parent();
    while let Some(ancestor) = parent {
        if ancestor.has_tag_name((W_NS, "pPr")) {
            return true;
        }
        parent = ancestor.parent();
    }
    false
}

fn parse_run_properties(properties: Node<'_, '_>) -> Result<CharacterStyle, String> {
    let mut style = CharacterStyle::default();
    let elements = properties.children().filter(Node::is_element);
    for property in elements {
        if property.tag_name().namespace() != Some(W_NS) {
            continue;
        }
        match property.tag_name().name() {
            "b" | "bCs" if xml_property_is_enabled(property) => style.weight = FontWeight::Bold,
            "i" | "iCs" if xml_property_is_enabled(property) => style.italic = true,
            "u" if xml_property_is_enabled(property) => style.underline = true,
            "strike" if xml_property_is_enabled(property) => style.strikethrough = true,
            "sz" | "szCs" => {
                if let Some(value) = property.attribute((W_NS, "val")) {
                    if let Ok(half_points) = value.parse::<f32>() {
                        style.font_size = half_points / 2.0;
                        style.explicit_size = true;
                    }
                }
            }
            "color" => {
                if let Some(value) = property.attribute((W_NS, "val")) {
                    style.color = if value == "auto" {
                        None
                    } else {
                        Some(writer_color(value).ok_or_else(|| {
                            "DOCX color value must be six hexadecimal digits".to_string()
                        })?)
                    };
                }
            }
            "rFonts" => {
                if let Some(value) = property.attribute((W_NS, "ascii")) {
                    if !value.is_empty() {
                        style.font_family = value.to_owned();
                    }
                }
            }
            "vertAlign" => {
                if let Some(value) = property.attribute((W_NS, "val")) {
                    style.superscript = value == "superscript";
                    style.subscript = value == "subscript";
                }
            }
            _ => {}
        }
    }
    Ok(style)
}

fn xml_property_is_enabled(property: Node<'_, '_>) -> bool {
    !matches!(
        property.attribute((W_NS, "val")),
        Some("0" | "false" | "off" | "none")
    )
}

fn parse_run_style(run: Node<'_, '_>) -> Result<CharacterStyle, String> {
    let Some(properties) = run.children().find(|node| node.has_tag_name((W_NS, "rPr"))) else {
        return Ok(CharacterStyle::default());
    };
    parse_run_properties(properties)
}

fn validate_tracked_run_properties(
    properties: Node<'_, '_>,
    allow_change: bool,
) -> Result<(), String> {
    for property in properties.children().filter(Node::is_element) {
        if property.tag_name().namespace() != Some(W_NS) {
            continue;
        }
        let name = property.tag_name().name();
        if allow_change && name == "rPrChange" {
            continue;
        }
        if !matches!(
            name,
            "b" | "bCs"
                | "i"
                | "iCs"
                | "u"
                | "strike"
                | "sz"
                | "szCs"
                | "color"
                | "rFonts"
                | "vertAlign"
        ) {
            return Err(format!(
                "DOCX tracked formatting property w:{name} is not supported"
            ));
        }
        if name == "u"
            && !matches!(
                property.attribute((W_NS, "val")),
                None | Some("single" | "none")
            )
        {
            return Err("DOCX tracked underline style is not supported".into());
        }
        if name == "rFonts" {
            let ascii = property.attribute((W_NS, "ascii"));
            if ["hAnsi", "eastAsia", "cs"].into_iter().any(|slot| {
                property
                    .attribute((W_NS, slot))
                    .is_some_and(|value| Some(value) != ascii)
            }) {
                return Err("DOCX tracked script-specific fonts are not supported".into());
            }
        }
    }
    Ok(())
}

fn parse_run_format_revision(
    run: Node<'_, '_>,
) -> Result<Option<(RevisionMarkup, CharacterStyle)>, String> {
    let Some(properties) = run.children().find(|node| node.has_tag_name((W_NS, "rPr"))) else {
        return Ok(None);
    };
    let mut changes = properties
        .children()
        .filter(|node| node.has_tag_name((W_NS, "rPrChange")));
    let Some(change) = changes.next() else {
        return Ok(None);
    };
    if changes.next().is_some() {
        return Err("DOCX run has multiple formatting revisions".into());
    }
    validate_tracked_run_properties(properties, true)?;
    let mut old_properties = change
        .children()
        .filter(|node| node.has_tag_name((W_NS, "rPr")));
    let Some(old) = old_properties.next() else {
        return Err("DOCX rPrChange is missing its previous w:rPr".into());
    };
    if old_properties.next().is_some()
        || old
            .children()
            .any(|node| node.has_tag_name((W_NS, "rPrChange")))
    {
        return Err("DOCX nested or duplicate formatting revisions are unsupported".into());
    }
    validate_tracked_run_properties(old, false)?;
    let markup = revision_markup(change, RevisionKind::Formatting)?;
    Ok(Some((markup, parse_run_properties(old)?)))
}

fn payload_piece(text: &str, style: &CharacterStyle) -> RevisionPayload {
    let runs = if text.is_empty() || *style == CharacterStyle::default() {
        Vec::new()
    } else {
        vec![StyleRun {
            start: 0,
            end: text.len(),
            style: style.clone(),
        }]
    };
    RevisionPayload {
        text: text.to_owned(),
        runs,
    }
}

fn append_payload(target: &mut RevisionPayload, piece: RevisionPayload) {
    let offset = target.text.len();
    target.runs.extend(piece.runs.into_iter().map(|mut run| {
        run.start += offset;
        run.end += offset;
        run
    }));
    target.text.push_str(&piece.text);
}

fn append_local_revision(
    revisions: &mut Vec<LocalRevision>,
    markup: &RevisionMarkup,
    start: usize,
    end: usize,
    text: &str,
    before_style: Option<&CharacterStyle>,
    after_style: Option<&CharacterStyle>,
) -> Result<(), String> {
    if text.is_empty() {
        return Err("DOCX tracked change has no supported text".into());
    }
    let before = before_style.map(|style| payload_piece(text, style));
    let after = after_style.map(|style| payload_piece(text, style));
    let boundary_kind = if text.contains('\u{b}') {
        RevisionBoundaryKind::SoftBreak
    } else {
        RevisionBoundaryKind::Text
    };
    if let Some(existing) = revisions
        .iter_mut()
        .find(|revision| revision.id == markup.id)
    {
        if existing.kind != markup.kind || existing.author != markup.author {
            return Err("DOCX revision ID has conflicting kind or author".into());
        }
        let contiguous = match markup.kind {
            RevisionKind::Deletion => existing.start == start && existing.end == end,
            RevisionKind::Insertion | RevisionKind::Formatting => existing.end == start,
        };
        if !contiguous {
            return Err("DOCX revision ID describes disjoint text ranges".into());
        }
        match (&mut existing.before, before) {
            (Some(target), Some(piece)) => append_payload(target, piece),
            (None, None) => {}
            _ => return Err("DOCX revision payload is inconsistent".into()),
        }
        match (&mut existing.after, after) {
            (Some(target), Some(piece)) => append_payload(target, piece),
            (None, None) => {}
            _ => return Err("DOCX revision payload is inconsistent".into()),
        }
        if existing.boundary_kind != RevisionBoundaryKind::ParagraphMark {
            existing.boundary_kind = if existing.boundary_kind == RevisionBoundaryKind::SoftBreak
                && boundary_kind == RevisionBoundaryKind::SoftBreak
            {
                RevisionBoundaryKind::SoftBreak
            } else if existing.boundary_kind == RevisionBoundaryKind::SoftBreak
                || boundary_kind == RevisionBoundaryKind::SoftBreak
            {
                RevisionBoundaryKind::Mixed
            } else {
                RevisionBoundaryKind::Text
            };
        }
        if markup.kind != RevisionKind::Deletion {
            existing.end = end;
        }
        return Ok(());
    }
    revisions.push(LocalRevision {
        id: markup.id,
        author: markup.author.clone(),
        kind: markup.kind,
        boundary_kind,
        start,
        end,
        before,
        after,
    });
    Ok(())
}

fn parse_paragraph_runs(
    paragraph_xml: &str,
    root_tag: &str,
    text: &mut String,
    runs: &mut Vec<StyleRun>,
    footnotes: &mut Vec<LocalFootnoteReference>,
    revisions: &mut Vec<LocalRevision>,
) -> Result<(), String> {
    let wrapped = format!("{root_tag}<w:body>{paragraph_xml}</w:body></w:document>");
    let document = roxmltree::Document::parse(&wrapped)
        .map_err(|error| format!("invalid DOCX paragraph XML: {error}"))?;
    let paragraph = document
        .root_element()
        .descendants()
        .find(|node| node.has_tag_name((W_NS, "p")))
        .ok_or_else(|| "DOCX paragraph element is missing".to_string())?;
    let mut revision_runs = Vec::new();
    collect_revision_runs(paragraph, None, &mut revision_runs)?;
    let prefix_len = root_tag.len() + "<w:body>".len();
    for (run, context) in revision_runs {
        let range = run.range();
        let start = range
            .start
            .checked_sub(prefix_len)
            .ok_or_else(|| "invalid DOCX run location".to_string())?;
        let end = range
            .end
            .checked_sub(prefix_len)
            .ok_or_else(|| "invalid DOCX run location".to_string())?;
        let run_xml = paragraph_xml
            .get(start..end)
            .ok_or_else(|| "invalid DOCX run boundary".to_string())?;
        let current_style = parse_run_style(run)?;
        let format_revision = parse_run_format_revision(run)?;
        if run_xml.contains("<w:delText")
            && context
                .as_ref()
                .is_none_or(|markup| markup.kind != RevisionKind::Deletion)
        {
            return Err("DOCX delText appears outside a deletion revision".into());
        }
        let (run_text, references) = parse_run_content(run_xml)?;
        if !references.is_empty() && (context.is_some() || format_revision.is_some()) {
            return Err("tracked DOCX footnote references are not supported".into());
        }
        if let Some(markup) = context
            .as_ref()
            .filter(|markup| markup.kind == RevisionKind::Deletion)
        {
            if format_revision.is_some() {
                return Err("formatting changes inside DOCX deletions are unsupported".into());
            }
            let offset = text.len();
            append_local_revision(
                revisions,
                markup,
                offset,
                offset,
                &run_text,
                Some(&current_style),
                None,
            )?;
            continue;
        }
        let start = text.len();
        footnotes.extend(
            references
                .into_iter()
                .map(|reference| LocalFootnoteReference {
                    ooxml_id: reference.ooxml_id,
                    offset: start + reference.offset,
                }),
        );
        text.push_str(&run_text);
        let end = text.len();
        if end > start && current_style != CharacterStyle::default() {
            runs.push(StyleRun {
                start,
                end,
                style: current_style.clone(),
            });
        }
        if let Some(markup) = context.as_ref() {
            append_local_revision(
                revisions,
                markup,
                start,
                end,
                &run_text,
                None,
                Some(&current_style),
            )?;
        }
        if let Some((markup, old_style)) = format_revision {
            append_local_revision(
                revisions,
                &markup,
                start,
                end,
                &run_text,
                Some(&old_style),
                Some(&current_style),
            )?;
        }
    }
    Ok(())
}

fn local_revision_records(
    block_id: u64,
    text: &str,
    revisions: Vec<LocalRevision>,
) -> Result<Vec<RevisionRecord>, String> {
    revisions
        .into_iter()
        .map(|revision| {
            let prefix = text
                .get(..revision.start)
                .ok_or_else(|| "DOCX revision anchor is outside its paragraph".to_string())?;
            Ok(RevisionRecord {
                id: revision.id,
                author: revision.author,
                anchor: RevisionAnchor {
                    block_id,
                    start: revision.start,
                    end: revision.end,
                    end_block_id: None,
                    prefix_sha256: crate::zip::sha256(prefix.as_bytes()),
                },
                kind: revision.kind,
                boundary_kind: revision.boundary_kind,
                before: revision.before,
                after: revision.after,
            })
        })
        .collect()
}

/// Parses a single `<w:p>...</w:p>` block.
fn parse_paragraph(
    xml: &str,
    start: usize,
    id: u64,
    archive: Option<&PackageArchive>,
    root_tag: &str,
    footnotes: &mut Vec<LocalFootnoteReference>,
    revisions: &mut Vec<RevisionRecord>,
    paragraph_formats: &mut Vec<LocalParagraphFormatChange>,
    comment_anchors: &mut Vec<super::comments_import::CommentAnchor>,
) -> Result<Option<(Option<RichBlock>, usize)>, String> {
    let Some(tag_open_end) = xml[start..].find('>') else {
        return Ok(None);
    };
    let tag_open_end = tag_open_end + start + 1;
    let tag_str = &xml[start..tag_open_end];
    if tag_str.ends_with("/>") {
        return Ok(Some((None, tag_open_end)));
    }

    let close_tag = "</w:p>";
    let Some(end_rel) = xml[start..].find(close_tag) else {
        return Ok(None);
    };
    let p_xml = &xml[start..start + end_rel + close_tag.len()];
    let next_cursor = start + end_rel + close_tag.len();
    let has_revision_markup = [
        "<w:ins",
        "<w:del",
        "<w:rPrChange",
        "<w:pPrChange",
        "<w:moveFrom",
        "<w:moveTo",
    ]
    .iter()
    .any(|tag| p_xml.contains(tag));
    let paragraph_properties =
        super::paragraph_style_import::extract_paragraph_properties(p_xml, root_tag)?;
    let paragraph_style = super::paragraph_style_import::paragraph_style_from_docx(
        &paragraph_properties.current_xml,
    )?;
    let local_comment_anchors = super::comments_import::paragraph_anchors(p_xml, root_tag, id)?;

    // A picture: `<a:blip r:embed>` in a drawing, bytes from the package.
    if let Some(archive) = archive {
        if let Some(mut block) = parse_picture(p_xml, id, archive) {
            block.style = paragraph_style.clone();
            if !local_comment_anchors.is_empty() {
                return Err("DOCX comments anchored to pictures are not supported".into());
            }
            if has_revision_markup {
                return Err("tracked changes to DOCX pictures are unsupported".into());
            }
            if find_tag_start(p_xml, "w:footnoteReference").is_some() {
                return Err("DOCX footnote reference on a picture paragraph is unsupported".into());
            }
            return Ok(Some((Some(block), next_cursor)));
        }
    }

    // Check for DrawingML chart in <w:drawing>
    if let Some(archive) = archive {
        if let Some(drawing_pos) = p_xml.find("<w:drawing") {
            let drawing_xml = &p_xml[drawing_pos..];
            if drawing_xml.contains("<c:chart") {
                if !local_comment_anchors.is_empty() {
                    return Err("DOCX comments anchored to charts are not supported".into());
                }
                if has_revision_markup {
                    return Err("tracked changes to DOCX charts are unsupported".into());
                }
                if let Some(rid) = extract_attribute_value(drawing_xml, "r:id") {
                    let rels_xml = archive
                        .get("word/_rels/document.xml.rels")
                        .map(|b| String::from_utf8_lossy(b).to_string())
                        .unwrap_or_default();
                    let rels = parse_rels(&rels_xml);
                    if let Some(target) = rels.get(&rid) {
                        let chart_path = if target.starts_with('/') {
                            target.trim_start_matches('/').to_string()
                        } else {
                            format!("word/{target}")
                        };
                        if let Some(chart_bytes) = archive.get(&chart_path) {
                            let chart_xml = String::from_utf8_lossy(chart_bytes);
                            if let Ok(chart) = loom_objects::chart_ooxml::c_chart_xml_to_chart(
                                &chart_xml,
                                format!("chart-{id}"),
                            ) {
                                if let Ok(json) = chart.to_json() {
                                    let text = crate::images::picture_text(id as u32);
                                    let mut block = RichBlock::chart_block(id, &text, json);
                                    block.style = paragraph_style.clone();
                                    if find_tag_start(p_xml, "w:footnoteReference").is_some() {
                                        return Err("DOCX footnote reference on a chart paragraph is unsupported".into());
                                    }
                                    return Ok(Some((Some(block), next_cursor)));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // The live paragraph properties exclude the nested previous pPr in pPrChange.
    let mut kind = super::paragraph_style_import::paragraph_kind_from_docx(
        &paragraph_properties.current_xml,
        archive,
        false,
    )?;
    let mut paragraph_style = paragraph_style;

    // A list item's nesting is the numbering level Word stores as `w:ilvl`.
    if crate::list_levels::is_list_kind(&kind) {
        paragraph_style.list_level =
            num_pr_level(p_xml).map_or(0, |value| crate::list_levels::parse_list_level(&value));
    }

    let mut text = String::new();
    let mut runs = Vec::new();
    let mut local_revisions = Vec::new();
    parse_paragraph_runs(
        p_xml,
        root_tag,
        &mut text,
        &mut runs,
        footnotes,
        &mut local_revisions,
    )?;
    table::append_paragraph_mark_revisions(root_tag, p_xml, &text, &mut local_revisions)?;

    let local_paragraph_format =
        super::paragraph_style_import::parse_tracked_paragraph_format_change(
            &paragraph_properties,
            root_tag,
            archive,
            &kind,
            &paragraph_style,
        )?;
    if local_paragraph_format.as_ref().is_some_and(|change| {
        local_revisions
            .iter()
            .any(|revision| revision.id == change.id)
    }) {
        return Err("DOCX paragraph-format revision ID collides with an inline revision".into());
    }

    if text.trim().is_empty()
        && footnotes.is_empty()
        && local_revisions.is_empty()
        && local_paragraph_format.is_none()
        && local_comment_anchors.is_empty()
    {
        return Ok(Some((None, next_cursor)));
    }
    if local_comment_anchors
        .iter()
        .any(|anchor| anchor.end > text.len())
    {
        return Err("DOCX comment range exceeds its paragraph text".into());
    }
    comment_anchors.extend(local_comment_anchors);

    let mut block = RichBlock::new(id, &kind, &text);
    block.style = paragraph_style;
    block.runs = runs;
    revisions.extend(local_revision_records(id, &text, local_revisions)?);
    if let Some(change) = local_paragraph_format {
        paragraph_formats.push(change);
    }
    Ok(Some((Some(block), next_cursor)))
}

/// Parses text and inline footnote references from one WordprocessingML run.
fn parse_run_content(r_xml: &str) -> Result<(String, Vec<LocalFootnoteReference>), String> {
    let mut out = String::new();
    let mut footnotes = Vec::new();
    let mut cursor = 0usize;
    while cursor < r_xml.len() {
        let rest = &r_xml[cursor..];
        let t_pos = find_tag_start(rest, "w:t");
        let del_text_pos = find_tag_start(rest, "w:delText");
        let tab_pos = find_tag_start(rest, "w:tab");
        let br_pos = find_tag_start(rest, "w:br");
        let cr_pos = find_tag_start(rest, "w:cr");
        let footnote_pos = find_tag_start(rest, "w:footnoteReference");

        let mut next_action: Option<(&str, usize)> = None;
        for (kind, position) in [
            ("t", t_pos),
            ("delText", del_text_pos),
            ("tab", tab_pos),
            ("br", br_pos),
            ("cr", cr_pos),
            ("footnote", footnote_pos),
        ] {
            if let Some(position) = position {
                if next_action.map_or(true, |(_, current)| position < current) {
                    next_action = Some((kind, position));
                }
            }
        }

        match next_action {
            Some(("t", position)) | Some(("delText", position)) => {
                let tag_name = if next_action.is_some_and(|(kind, _)| kind == "delText") {
                    "w:delText"
                } else {
                    "w:t"
                };
                let tag_start = cursor + position;
                let open_end = r_xml[tag_start..]
                    .find('>')
                    .ok_or_else(|| format!("malformed DOCX {tag_name} tag"))?
                    + tag_start;
                let content_start = open_end + 1;
                let close_tag = format!("</{tag_name}>");
                let close_relative = r_xml[content_start..]
                    .find(&close_tag)
                    .ok_or_else(|| format!("malformed DOCX {tag_name} element"))?;
                let content = &r_xml[content_start..content_start + close_relative];
                out.push_str(&xml_unescape(content));
                cursor = content_start + close_relative + close_tag.len();
            }
            Some(("tab", position)) => {
                out.push('\t');
                let tag_start = cursor + position;
                let open_end = r_xml[tag_start..]
                    .find('>')
                    .ok_or_else(|| "malformed DOCX w:tab tag".to_string())?;
                cursor = tag_start + open_end + 1;
            }
            Some(("br", position)) => {
                let tag_start = cursor + position;
                let open_end = r_xml[tag_start..]
                    .find('>')
                    .ok_or_else(|| "malformed DOCX w:br tag".to_string())?
                    + tag_start;
                let page = r_xml[tag_start..open_end].contains("w:type=\"page\"");
                out.push(if page { '\u{c}' } else { '\u{b}' });
                cursor = open_end + 1;
            }
            Some(("cr", position)) => {
                out.push('\u{b}');
                let tag_start = cursor + position;
                let open_end = r_xml[tag_start..]
                    .find('>')
                    .ok_or_else(|| "malformed DOCX w:cr tag".to_string())?;
                cursor = tag_start + open_end + 1;
            }
            Some(("footnote", position)) => {
                let tag_start = cursor + position;
                let open_end = r_xml[tag_start..]
                    .find('>')
                    .ok_or_else(|| "malformed DOCX w:footnoteReference tag".to_string())?
                    + tag_start;
                let tag = &r_xml[tag_start..=open_end];
                let raw_id = extract_attribute_value(tag, "w:id")
                    .ok_or_else(|| "DOCX footnote reference is missing w:id".to_string())?;
                let ooxml_id = raw_id
                    .parse::<i32>()
                    .map_err(|_| format!("DOCX footnote reference has invalid w:id {raw_id:?}"))?;
                footnotes.push(LocalFootnoteReference {
                    ooxml_id,
                    offset: out.len(),
                });
                if tag.ends_with("/>") {
                    cursor = open_end + 1;
                } else {
                    let close = "</w:footnoteReference>";
                    let content_start = open_end + 1;
                    let close_relative = r_xml[content_start..]
                        .find(close)
                        .ok_or_else(|| "malformed DOCX footnote reference element".to_string())?;
                    if !r_xml[content_start..content_start + close_relative]
                        .trim()
                        .is_empty()
                    {
                        return Err("DOCX footnote reference must be empty".into());
                    }
                    cursor = content_start + close_relative + close.len();
                }
            }
            _ => break,
        }
    }
    Ok((out, footnotes))
}

fn paragraph_text_runs(block: &RichBlock) -> Vec<TextRun> {
    let text = block.text.as_str();
    let mut out = Vec::with_capacity(block.runs.len() * 2 + 1);
    let mut cursor = 0;
    for run in &block.runs {
        let start = run.start.max(cursor).min(text.len());
        let end = run.end.min(text.len());
        if start > cursor {
            out.push(TextRun::plain(&text[cursor..start]));
        }
        if end > start {
            let style = &run.style;
            let mut piece = TextRun::plain(&text[start..end]);
            piece.bold = style.weight.numeric() >= FontWeight::Bold.numeric();
            piece.italic = style.italic;
            piece.underline = style.underline;
            piece.strikethrough = style.strikethrough;
            piece.superscript = style.superscript;
            piece.subscript = style.subscript;
            piece.font_size_pt = style.has_own_size().then_some(style.font_size);
            piece.font_family = (!style.font_family.is_empty()).then(|| style.font_family.clone());
            piece.color = style.color.clone();
            out.push(piece);
            cursor = end;
        }
    }
    if cursor < text.len() {
        out.push(TextRun::plain(&text[cursor..]));
    }
    out
}

fn trim_text_runs(runs: Vec<TextRun>) -> Vec<TextRun> {
    let text: String = runs.iter().map(|run| run.text.as_str()).collect();
    let start = text.len() - text.trim_start().len();
    let end = text.trim_end().len();
    if start >= end {
        return Vec::new();
    }

    let mut out = Vec::with_capacity(runs.len());
    let mut offset = 0;
    for run in runs {
        let run_start = offset;
        let run_end = run_start + run.text.len();
        offset = run_end;
        let from = start.saturating_sub(run_start).min(run.text.len());
        let to = end.saturating_sub(run_start).min(run.text.len());
        if from < to {
            let mut piece = run;
            piece.text = piece.text[from..to].to_owned();
            out.push(piece);
        }
    }
    out
}

pub(super) fn extract_attribute_value(tag_snippet: &str, attr_name: &str) -> Option<String> {
    attribute_value(tag_snippet, attr_name)
}

fn extract_core_title(xml: &str) -> Option<String> {
    let start_tag = "<dc:title";
    let start_idx = xml.find(start_tag)?;
    let after_start = start_idx + start_tag.len();
    let tag_close = xml[after_start..].find('>')?;
    let content_start = after_start + tag_close + 1;
    let end_idx = xml[content_start..].find("</dc:title>")?;
    let raw = &xml[content_start..content_start + end_idx];
    let title = xml_unescape(raw).trim().to_string();
    if title.is_empty() {
        None
    } else {
        Some(title)
    }
}

fn parse_rels(rels_xml: &str) -> std::collections::BTreeMap<String, String> {
    let mut map = std::collections::BTreeMap::new();
    let mut offset = 0;
    while let Some((rel_pos, tag_str)) =
        property_tag_with_start(&rels_xml[offset..], "Relationship")
    {
        if let (Some(id), Some(target)) = (
            extract_attribute_value(tag_str, "Id"),
            extract_attribute_value(tag_str, "Target"),
        ) {
            map.insert(id, target);
        }
        offset += rel_pos + tag_str.len();
    }
    map
}

pub(super) fn resolve_num_id_list_kind(
    archive: Option<&PackageArchive>,
    num_id: &str,
) -> Option<&'static str> {
    if let Some(num_xml) = archive
        .and_then(|a| a.get("word/numbering.xml"))
        .and_then(|b| std::str::from_utf8(b).ok())
    {
        let num_tag = format!("<w:num w:numId=\"{num_id}\"");
        if let Some(num_pos) = num_xml.find(&num_tag) {
            let num_rest = &num_xml[num_pos..];
            let num_end = num_rest.find("</w:num>").unwrap_or(num_rest.len());
            let num_block = &num_rest[..num_end];
            if let Some(abs_pos) = num_block.find("<w:abstractNumId") {
                if let Some(abs_id) = extract_attribute_value(&num_block[abs_pos..], "w:val") {
                    let abs_tag = format!("<w:abstractNum w:abstractNumId=\"{abs_id}\"");
                    if let Some(abs_start) = num_xml.find(&abs_tag) {
                        let abs_rest = &num_xml[abs_start..];
                        let abs_end = abs_rest.find("</w:abstractNum>").unwrap_or(abs_rest.len());
                        let abs_block = &abs_rest[..abs_end];
                        if let Some(fmt_pos) = abs_block.find("<w:numFmt") {
                            if let Some(fmt_val) =
                                extract_attribute_value(&abs_block[fmt_pos..], "w:val")
                            {
                                return match fmt_val.to_ascii_lowercase().as_str() {
                                    "none" => None,
                                    "bullet" => Some("list-bulleted"),
                                    _ => Some("list-numbered"),
                                };
                            }
                        }
                    }
                }
            }
        }
    }
    if num_id == "1" {
        Some("list-bulleted")
    } else {
        Some("list-numbered")
    }
}
