use std::ops::Range;

use super::edits::runs_in_range;
use super::*;

pub(super) fn unique_block_index(
    blocks: &[RichBlock],
    block_id: u64,
) -> Result<usize, WriterError> {
    let mut found = None;
    for (index, block) in blocks.iter().enumerate() {
        if block.id == block_id {
            if found.is_some() {
                return Err(WriterError::Invalid(INVALID_REVISION.into()));
            }
            found = Some(index);
        }
    }
    found.ok_or_else(|| WriterError::Invalid(INVALID_REVISION.into()))
}

pub(super) fn block_start_offset(blocks: &[RichBlock], index: usize) -> usize {
    blocks[..index].iter().fold(0usize, |offset, block| {
        offset.saturating_add(block.text.len_bytes() + 1)
    })
}

pub(super) fn block_location_at(blocks: &[RichBlock], offset: usize) -> Option<(usize, usize)> {
    let mut start = 0usize;
    for (index, block) in blocks.iter().enumerate() {
        let end = start + block.text.len_bytes();
        if offset <= end || index + 1 == blocks.len() {
            return Some((
                index,
                offset.saturating_sub(start).min(block.text.len_bytes()),
            ));
        }
        start = end + 1;
    }
    None
}

pub(super) fn anchor_global_range(
    blocks: &[RichBlock],
    anchor: &RevisionAnchor,
) -> Result<(usize, usize), WriterError> {
    let start_index = unique_block_index(blocks, anchor.block_id)?;
    let end_index = anchor
        .end_block_id
        .map(|id| unique_block_index(blocks, id))
        .transpose()?
        .unwrap_or(start_index);
    let start_block = &blocks[start_index];
    let end_block = &blocks[end_index];
    if end_index < start_index
        || (anchor.end_block_id.is_some() && end_index == start_index)
        || anchor.start > start_block.text.len_bytes()
        || anchor.end > end_block.text.len_bytes()
        || !start_block.text.as_str().is_char_boundary(anchor.start)
        || !end_block.text.as_str().is_char_boundary(anchor.end)
        || (start_index == end_index && anchor.start > anchor.end)
    {
        return Err(WriterError::Invalid(INVALID_REVISION.into()));
    }
    let start = block_start_offset(blocks, start_index) + anchor.start;
    let end = block_start_offset(blocks, end_index) + anchor.end;
    if end < start {
        return Err(WriterError::Invalid(INVALID_REVISION.into()));
    }
    Ok((start, end))
}

pub(super) fn anchor_from_global_range(
    blocks: &[RichBlock],
    start: usize,
    end: usize,
) -> Result<RevisionAnchor, WriterError> {
    let Some((start_index, start_local)) = block_location_at(blocks, start) else {
        return Err(WriterError::Invalid(INVALID_REVISION.into()));
    };
    let Some((end_index, end_local)) = block_location_at(blocks, end) else {
        return Err(WriterError::Invalid(INVALID_REVISION.into()));
    };
    if end_index < start_index {
        return Err(WriterError::Invalid(INVALID_REVISION.into()));
    }
    let start_block = &blocks[start_index];
    let end_block = &blocks[end_index];
    let prefix = start_block
        .text
        .as_str()
        .get(..start_local)
        .ok_or_else(|| WriterError::Invalid(INVALID_REVISION.into()))?;
    Ok(RevisionAnchor {
        block_id: start_block.id,
        start: start_local,
        end: end_local,
        end_block_id: (end_index != start_index).then_some(end_block.id),
        prefix_sha256: crate::zip::sha256(prefix.as_bytes()),
    })
}

pub(super) fn table_range_is_one_cell(block: &RichBlock, range: Range<usize>) -> bool {
    let Some(table) = block.typed_table() else {
        return true;
    };
    let map = crate::tables::TableTextMap::new(table);
    (0..table.row_count()).any(|row| {
        (0..table.col_count()).any(|column| {
            map.cell_range(row, column)
                .is_some_and(|cell| range.start >= cell.start && range.end <= cell.end)
        })
    })
}

pub(super) fn block_revision_runs(block: &RichBlock) -> Vec<StyleRun> {
    let Some(table) = block.typed_table() else {
        return block.runs.clone();
    };
    let map = crate::tables::TableTextMap::new(table);
    let mut runs = Vec::new();
    for row in 0..table.row_count() {
        for column in 0..table.col_count() {
            let Some(cell_range) = map.cell_range(row, column) else {
                continue;
            };
            let Some(cell_block) = crate::table_text::table_cell_rich_block(
                block.id, table, row, column, 11.0, false, None,
            ) else {
                continue;
            };
            runs.extend(cell_block.runs.into_iter().map(|mut run| {
                run.start += cell_range.start;
                run.end += cell_range.start;
                run
            }));
        }
    }
    runs.sort_by_key(|run| run.start);
    runs
}

pub(super) fn runs_for_global_range(
    blocks: &[RichBlock],
    range: Range<usize>,
) -> Result<Vec<StyleRun>, WriterError> {
    if range.start > range.end {
        return Err(WriterError::Invalid(INVALID_REVISION.into()));
    }
    let mut out = Vec::new();
    for (index, block) in blocks.iter().enumerate() {
        let block_start = block_start_offset(blocks, index);
        let block_end = block_start + block.text.len_bytes();
        let start = range.start.max(block_start);
        let end = range.end.min(block_end);
        if start >= end {
            continue;
        }
        let local = start - block_start..end - block_start;
        if !table_range_is_one_cell(block, local.clone()) {
            return Err(WriterError::Invalid(INVALID_REVISION.into()));
        }
        for mut run in runs_in_range(&block_revision_runs(block), local) {
            run.start += start - range.start;
            run.end += start - range.start;
            out.push(run);
        }
    }
    Ok(out)
}

pub(super) fn payload_for_range(
    blocks: &[RichBlock],
    range: Range<usize>,
) -> Result<RevisionPayload, WriterError> {
    let text = blocks_to_editor_text(blocks)
        .get(range.clone())
        .ok_or_else(|| WriterError::Invalid(INVALID_REVISION.into()))?
        .to_owned();
    Ok(RevisionPayload {
        runs: runs_for_global_range(blocks, range)?,
        text,
    })
}

pub(super) fn blocks_to_editor_text(blocks: &[RichBlock]) -> String {
    blocks
        .iter()
        .map(|block| block.text.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

#[derive(Clone, Copy)]
pub(super) enum RevisionResolutionOrder {
    AcceptOldest,
    RejectNewest,
}

pub(super) fn ensure_revision_resolution_order(
    blocks: &[RichBlock],
    revisions: &[RevisionRecord],
    id: u64,
    revision: &RevisionRecord,
    order: RevisionResolutionOrder,
) -> Result<(), WriterError> {
    let range = |revision: &RevisionRecord| {
        if blocks.is_empty()
            && revision.kind == RevisionKind::Deletion
            && revision.anchor.end_block_id.is_none()
            && revision.anchor.start == 0
            && revision.anchor.end == 0
        {
            Ok((0, 0))
        } else {
            anchor_global_range(blocks, &revision.anchor)
        }
    };
    let (start, end) = range(revision)?;
    for other in revisions {
        let is_dependent = match order {
            RevisionResolutionOrder::AcceptOldest => other.id < id,
            RevisionResolutionOrder::RejectNewest => other.id > id,
        };
        if !is_dependent {
            continue;
        }
        let (other_start, other_end) = range(other)?;
        let overlaps = if start == end {
            if other_start == other_end {
                other_start == start
            } else {
                other_start < start && start < other_end
            }
        } else if other_start == other_end {
            start < other_start && other_start < end
        } else {
            other_start < end && start < other_end
        };
        if overlaps {
            return Err(WriterError::Invalid(REVISION_DEPENDENCY_CONFLICT.into()));
        }
    }
    Ok(())
}

pub(super) fn validate_record(
    revision: &RevisionRecord,
    blocks: &[RichBlock],
) -> Result<(), WriterError> {
    if revision.id == 0
        || revision.author.trim().is_empty()
        || revision.author.len() > MAX_REVISION_AUTHOR_BYTES
    {
        return Err(WriterError::Invalid(INVALID_REVISION.into()));
    }
    if blocks.is_empty() {
        if revision.kind != RevisionKind::Deletion
            || revision.anchor.end_block_id.is_some()
            || revision.anchor.start != 0
            || revision.anchor.end != 0
            || revision.anchor.prefix_sha256 != crate::zip::sha256(&[])
        {
            return Err(WriterError::Invalid(INVALID_REVISION.into()));
        }
    }
    let (global_start, global_end) = if blocks.is_empty() {
        (0, 0)
    } else {
        let (start, end) = anchor_global_range(blocks, &revision.anchor)?;
        let start_index = unique_block_index(blocks, revision.anchor.block_id)?;
        let start_text = blocks[start_index].text.as_str();
        if crate::zip::sha256(start_text[..revision.anchor.start].as_bytes())
            != revision.anchor.prefix_sha256
        {
            return Err(WriterError::Invalid(INVALID_REVISION.into()));
        }
        if revision.anchor.end_block_id.is_none() {
            let block = &blocks[start_index];
            if !table_range_is_one_cell(block, revision.anchor.start..revision.anchor.end) {
                return Err(WriterError::Invalid(INVALID_REVISION.into()));
            }
        }
        (start, end)
    };
    let text = blocks_to_editor_text(blocks);
    let range_text = text
        .get(global_start..global_end)
        .ok_or_else(|| WriterError::Invalid(INVALID_REVISION.into()))?;
    let range_runs = runs_for_global_range(blocks, global_start..global_end)?;
    for payload in [&revision.before, &revision.after].into_iter().flatten() {
        payload.validate()?;
    }
    match revision.kind {
        RevisionKind::Insertion => {
            let after = revision
                .after
                .as_ref()
                .ok_or_else(|| WriterError::Invalid(INVALID_REVISION.into()))?;
            if revision.before.is_some()
                || global_start == global_end
                || after.text != range_text
                || range_runs != after.runs
            {
                return Err(WriterError::Invalid(INVALID_REVISION.into()));
            }
        }
        RevisionKind::Deletion => {
            let before = revision
                .before
                .as_ref()
                .ok_or_else(|| WriterError::Invalid(INVALID_REVISION.into()))?;
            if revision.after.is_some() || global_start != global_end || before.text.is_empty() {
                return Err(WriterError::Invalid(INVALID_REVISION.into()));
            }
        }
        RevisionKind::Formatting => {
            let before = revision
                .before
                .as_ref()
                .ok_or_else(|| WriterError::Invalid(INVALID_REVISION.into()))?;
            let after = revision
                .after
                .as_ref()
                .ok_or_else(|| WriterError::Invalid(INVALID_REVISION.into()))?;
            let empty_block_target = global_start == global_end
                && revision.anchor.end_block_id.is_none()
                && revision.anchor.start == 0
                && revision.anchor.end == 0
                && before.text.is_empty()
                && before.runs.is_empty()
                && after.text.is_empty()
                && after.runs.is_empty();
            if (global_start == global_end && !empty_block_target)
                || before.text != after.text
                || after.text != range_text
                || range_runs != after.runs
            {
                return Err(WriterError::Invalid(INVALID_REVISION.into()));
            }
        }
    }
    Ok(())
}
