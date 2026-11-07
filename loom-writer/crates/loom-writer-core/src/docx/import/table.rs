use super::*;

type ParsedTable = (
    RichBlock,
    usize,
    Vec<FootnoteReference>,
    Vec<RevisionRecord>,
);

/// Parses a table w:tbl element.
pub(super) fn parse_table(
    xml: &str,
    start: usize,
    id: u64,
    root_tag: &str,
) -> Result<Option<ParsedTable>, String> {
    let close_tag = "</w:tbl>";
    let Some(end_rel) = xml[start..].find(close_tag) else {
        return Ok(None);
    };
    let tbl_xml = &xml[start..start + end_rel + close_tag.len()];
    let next_cursor = start + end_rel + close_tag.len();
    let mut rows: Vec<Vec<Vec<TextRun>>> = Vec::new();
    let mut row_references: Vec<Vec<Vec<LocalFootnoteReference>>> = Vec::new();
    let mut row_revisions: Vec<Vec<Vec<RevisionRecord>>> = Vec::new();
    let mut header_row = false;
    let mut tr_cursor = 0usize;

    while let Some(tr_rel) = find_tag_start(&tbl_xml[tr_cursor..], "w:tr") {
        let tr_start = tr_cursor + tr_rel;
        let tr_close_rel = match tbl_xml[tr_start..].find("</w:tr>") {
            Some(end) => end,
            None => break,
        };
        let tr_xml = &tbl_xml[tr_start..tr_start + tr_close_rel + "</w:tr>".len()];
        tr_cursor = tr_start + tr_close_rel + "</w:tr>".len();

        if tr_xml.contains("<w:tblHeader") {
            header_row = true;
        }

        let mut row_cells: Vec<Vec<TextRun>> = Vec::new();
        let mut row_cell_references: Vec<Vec<LocalFootnoteReference>> = Vec::new();
        let mut row_cell_revisions: Vec<Vec<RevisionRecord>> = Vec::new();
        let mut tc_cursor = 0usize;
        while let Some(tc_rel) = find_tag_start(&tr_xml[tc_cursor..], "w:tc") {
            let tc_start = tc_cursor + tc_rel;
            let tc_close_rel = match tr_xml[tc_start..].find("</w:tc>") {
                Some(end) => end,
                None => break,
            };
            let tc_xml = &tr_xml[tc_start..tc_start + tc_close_rel + "</w:tc>".len()];
            tc_cursor = tc_start + tc_close_rel + "</w:tc>".len();

            let mut cell_runs = Vec::new();
            let mut cell_references = Vec::new();
            let mut cell_revisions = Vec::new();
            let mut p_cursor = 0usize;
            while let Some(p_rel) = find_tag_start(&tc_xml[p_cursor..], "w:p") {
                let p_start = p_cursor + p_rel;
                let mut paragraph_references = Vec::new();
                let mut paragraph_revisions = Vec::new();
                let mut paragraph_formats = Vec::new();
                let mut paragraph_comments = Vec::new();
                if let Some((block_opt, next_p)) = parse_paragraph(
                    tc_xml,
                    p_start,
                    0,
                    None,
                    root_tag,
                    super::ParagraphOutput {
                        footnotes: &mut paragraph_references,
                        revisions: &mut paragraph_revisions,
                        paragraph_formats: &mut paragraph_formats,
                        comment_anchors: &mut paragraph_comments,
                    },
                )? {
                    if !paragraph_comments.is_empty() {
                        return Err(
                            "DOCX comments anchored to table cells are not supported".into()
                        );
                    }
                    if !paragraph_formats.is_empty() {
                        return Err(
                            "DOCX paragraph-format revisions in table cells are not supported"
                                .into(),
                        );
                    }
                    if paragraph_revisions.iter().any(|revision| {
                        revision.boundary_kind == crate::RevisionBoundaryKind::ParagraphMark
                    }) {
                        return Err(
                            "DOCX paragraph-mark revisions in table cells are not supported".into(),
                        );
                    }
                    if let Some(block) = block_opt {
                        if super::super::paragraph_style_import::has_unsupported_table_layout(
                            &block.style,
                        ) {
                            return Err("paragraph spacing and indents in DOCX table cells are not supported".into());
                        }
                        let mut offset: usize =
                            cell_runs.iter().map(|run: &TextRun| run.text.len()).sum();
                        if !cell_runs.is_empty() {
                            cell_runs.push(TextRun::plain(" "));
                            offset += 1;
                        }
                        for mut revision in paragraph_revisions {
                            revision.anchor.start += offset;
                            revision.anchor.end += offset;
                            cell_revisions.push(revision);
                        }
                        cell_references.extend(paragraph_references.into_iter().map(|reference| {
                            LocalFootnoteReference {
                                ooxml_id: reference.ooxml_id,
                                offset: offset + reference.offset,
                            }
                        }));
                        cell_runs.extend(paragraph_text_runs(&block));
                    }
                    p_cursor = next_p;
                } else {
                    break;
                }
            }

            if cell_references.is_empty() {
                row_cells.push(trim_text_runs(cell_runs));
            } else {
                row_cells.push(cell_runs);
            }
            row_cell_references.push(cell_references);
            row_cell_revisions.push(cell_revisions);
        }

        if !row_cells.is_empty() {
            rows.push(row_cells);
            row_references.push(row_cell_references);
            row_revisions.push(row_cell_revisions);
        }
    }

    if rows.is_empty() {
        return Ok(None);
    }

    let columns = rows.iter().map(Vec::len).max().unwrap_or(0).max(1);
    let mut table = loom_objects::Table::new(rows.len(), columns);
    table.id = format!("tbl-{id}");
    table.header_row = header_row;
    for (row, cells) in rows.iter().enumerate() {
        for (column, runs) in cells.iter().enumerate() {
            if runs.is_empty() {
                table.set_cell_text(row, column, "");
            } else {
                table.set_cell_rich_text(row, column, runs.clone());
            }
        }
    }

    let block = RichBlock::table_block(id, table);
    let table = block
        .typed_table()
        .ok_or_else(|| "invalid imported DOCX table block".to_string())?;
    let map = crate::tables::TableTextMap::new(table);
    let mut references = Vec::new();
    let mut revisions = Vec::new();
    let mut revision_ids = std::collections::HashSet::new();
    for (row, cells) in row_references.iter().enumerate() {
        for (column, cell_references) in cells.iter().enumerate() {
            let Some(range) = map.cell_range(row, column) else {
                continue;
            };
            references.extend(cell_references.iter().map(|reference| FootnoteReference {
                ooxml_id: reference.ooxml_id,
                block_id: id,
                offset: range.start + reference.offset,
            }));
        }
    }
    for (row, cells) in row_revisions.iter().enumerate() {
        for (column, cell_revisions) in cells.iter().enumerate() {
            let Some(range) = map.cell_range(row, column) else {
                continue;
            };
            for mut revision in cell_revisions.iter().cloned() {
                if !revision_ids.insert(revision.id) {
                    return Err(
                        "DOCX revision ID is reused within table cells or paragraphs".into(),
                    );
                }
                revision.anchor.block_id = id;
                revision.anchor.start += range.start;
                revision.anchor.end += range.start;
                revision.anchor.end_block_id = None;
                let prefix = block
                    .text
                    .as_str()
                    .get(..revision.anchor.start)
                    .ok_or_else(|| "DOCX table revision anchor is outside its table".to_string())?;
                revision.anchor.prefix_sha256 = crate::zip::sha256(prefix.as_bytes());
                revisions.push(revision);
            }
        }
    }
    Ok(Some((block, next_cursor, references, revisions)))
}

pub(super) fn append_paragraph_mark_revisions(
    root_tag: &str,
    paragraph_xml: &str,
    text: &str,
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
    let Some(ppr) = paragraph
        .children()
        .find(|node| node.has_tag_name((W_NS, "pPr")))
    else {
        return Ok(());
    };
    let Some(rpr) = ppr.children().find(|node| node.has_tag_name((W_NS, "rPr"))) else {
        return Ok(());
    };
    for node in rpr.children().filter(|node| node.is_element()) {
        let name = node.tag_name().name();
        let kind = match name {
            "ins" => RevisionKind::Insertion,
            "del" => RevisionKind::Deletion,
            "rPrChange" => {
                return Err("paragraph-mark formatting revisions are unsupported".into());
            }
            _ => continue,
        };
        let markup = revision_markup(node, kind)?;
        if let Some(existing) = revisions
            .iter_mut()
            .find(|revision| revision.id == markup.id)
        {
            if existing.kind != kind || existing.author != markup.author {
                return Err(
                    "DOCX paragraph-mark revision metadata conflicts with its text runs".into(),
                );
            }
            if existing.boundary_kind == RevisionBoundaryKind::ParagraphMark {
                return Err(
                    "multiple paragraph marks for one revision in a paragraph are unsupported"
                        .into(),
                );
            }
            let touches_mark = match kind {
                RevisionKind::Insertion => existing.end == text.len(),
                RevisionKind::Deletion => {
                    existing.start == text.len() && existing.end == text.len()
                }
                RevisionKind::Formatting => false,
            };
            if !touches_mark {
                return Err(
                    "DOCX paragraph-mark revision is not adjacent to its text fragment".into(),
                );
            }
            existing.boundary_kind = RevisionBoundaryKind::ParagraphMark;
            let marker = RevisionPayload {
                text: "\n".into(),
                runs: Vec::new(),
            };
            match kind {
                RevisionKind::Insertion => {
                    let payload = existing.after.as_mut().ok_or_else(|| {
                        "DOCX paragraph-mark insertion has no matching text payload".to_string()
                    })?;
                    append_payload(payload, marker);
                }
                RevisionKind::Deletion => {
                    let payload = existing.before.as_mut().ok_or_else(|| {
                        "DOCX paragraph-mark deletion has no matching text payload".to_string()
                    })?;
                    append_payload(payload, marker);
                }
                RevisionKind::Formatting => unreachable!(),
            }
        } else {
            let marker = RevisionPayload {
                text: "\n".into(),
                runs: Vec::new(),
            };
            revisions.push(LocalRevision {
                id: markup.id,
                author: markup.author,
                kind,
                boundary_kind: RevisionBoundaryKind::ParagraphMark,
                start: text.len(),
                end: text.len(),
                before: (kind == RevisionKind::Deletion).then(|| marker.clone()),
                after: (kind == RevisionKind::Insertion).then_some(marker),
            });
        }
    }
    Ok(())
}

pub(super) fn normalize_paragraph_mark_revisions(
    blocks: &mut Vec<RichBlock>,
    references: &[FootnoteReference],
    revisions: &mut [RevisionRecord],
    comments: &[super::super::comments_import::CommentAnchor],
) -> Result<(), String> {
    let revision_blocks: Vec<(u64, u64)> = revisions
        .iter()
        .map(|revision| (revision.id, revision.anchor.block_id))
        .collect();
    for revision in revisions.iter_mut() {
        if revision.boundary_kind != RevisionBoundaryKind::ParagraphMark {
            continue;
        }
        let index = blocks
            .iter()
            .position(|block| block.id == revision.anchor.block_id)
            .ok_or_else(|| {
                "DOCX paragraph-mark revision refers to a missing paragraph".to_string()
            })?;
        if revision.kind == RevisionKind::Insertion && revision.anchor.end_block_id.is_none() {
            let next = blocks.get(index + 1).ok_or_else(|| {
                "DOCX paragraph-mark insertion has no following paragraph".to_string()
            })?;
            if blocks[index].text.is_empty()
                || next.text.is_empty()
                || blocks[index].typed_table().is_some()
                || next.typed_table().is_some()
                || blocks[index].is_image()
                || next.is_image()
                || blocks[index].kind == crate::CHART_BLOCK_KIND
                || next.kind == crate::CHART_BLOCK_KIND
            {
                return Err("DOCX paragraph-mark insertion spans unsupported blocks".into());
            }
            if revision.anchor.end != blocks[index].text.len_bytes() {
                return Err(
                    "DOCX paragraph-mark insertion is not at its paragraph boundary".into(),
                );
            }
            revision.anchor.end = 0;
            revision.anchor.end_block_id = Some(next.id);
            revision.anchor.prefix_sha256 =
                crate::zip::sha256(blocks[index].text.as_str().as_bytes());
            continue;
        }
        if revision.kind != RevisionKind::Deletion {
            continue;
        }
        let next = blocks
            .get(index + 1)
            .ok_or_else(|| "DOCX paragraph-mark deletion has no following paragraph".to_string())?;
        if blocks[index].kind != "paragraph"
            || next.kind != "paragraph"
            || blocks[index].text.is_empty()
            || next.text.is_empty()
            || blocks[index].typed_table().is_some()
            || next.typed_table().is_some()
            || blocks[index].image.is_some()
            || next.image.is_some()
            || blocks[index].chart.is_some()
            || next.chart.is_some()
            || blocks[index].style != next.style
        {
            return Err(
                "DOCX paragraph-mark deletion cannot merge these paragraph properties".into(),
            );
        }
        let merged_ids = [blocks[index].id, next.id];
        if references
            .iter()
            .any(|reference| merged_ids.contains(&reference.block_id))
            || comments
                .iter()
                .any(|comment| merged_ids.contains(&comment.block_id))
            || revision_blocks
                .iter()
                .any(|(id, block_id)| *id != revision.id && merged_ids.contains(block_id))
        {
            return Err(
                "DOCX paragraph-mark deletion overlaps comments, footnotes or other revisions"
                    .into(),
            );
        }
        if revision.anchor.start != blocks[index].text.len_bytes()
            || (revision.anchor.end != 0 && revision.anchor.end != revision.anchor.start)
            || (revision.anchor.end_block_id.is_some()
                && revision.anchor.end_block_id != Some(next.id))
        {
            return Err("DOCX paragraph-mark deletion fragments are not adjacent".into());
        }
        let first_len = blocks[index].text.len_bytes();
        let merged_text = format!("{}{}", blocks[index].text.as_str(), next.text.as_str());
        let mut merged = RichBlock::new(blocks[index].id, "paragraph", &merged_text);
        merged.style = blocks[index].style.clone();
        merged.runs = blocks[index].runs.clone();
        merged
            .runs
            .extend(next.runs.iter().map(|run| loom_text::StyleRun {
                start: run.start + first_len,
                end: run.end + first_len,
                style: run.style.clone(),
            }));
        revision.anchor.end = revision.anchor.start;
        revision.anchor.end_block_id = None;
        revision.anchor.prefix_sha256 =
            crate::zip::sha256(&merged.text.as_str().as_bytes()[..revision.anchor.start]);
        blocks[index] = merged;
        blocks.remove(index + 1);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::parse_table;

    #[test]
    fn paragraph_format_revisions_in_table_cells_fail_closed() {
        let xml = r#"<w:tbl><w:tr><w:tc><w:p><w:pPr><w:jc w:val="right"/><w:pPrChange w:id="1" w:author="Reviewer"><w:pPr><w:jc w:val="center"/></w:pPr></w:pPrChange></w:pPr><w:r><w:t>cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#;
        let root_tag = r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">"#;
        let error = match parse_table(xml, 0, 1, root_tag) {
            Err(error) => error,
            Ok(_) => panic!("table-cell paragraph-format revisions must fail closed"),
        };
        assert!(error.contains("paragraph-format revisions in table cells are not supported"));
    }
}
