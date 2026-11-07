use std::ops::Range;

use crate::WriterDocument;

use super::anchors::{
    block_location_at, block_revision_runs, block_start_offset, table_range_is_one_cell,
};
use super::*;

pub(super) fn rebase_revision_offset(
    offset: usize,
    edit: crate::comments::TextEdit,
    start_bias: bool,
) -> usize {
    if offset < edit.old_start {
        offset
    } else if offset > edit.old_end {
        edit.new_end + (offset - edit.old_end)
    } else if edit.old_start == edit.old_end {
        if start_bias {
            edit.new_start
        } else {
            edit.new_end
        }
    } else if offset == edit.old_start {
        edit.new_start
    } else if offset == edit.old_end {
        edit.new_end
    } else if start_bias {
        edit.new_start
    } else {
        edit.new_end
    }
}

pub(super) fn rebase_revision_offset_through_hunks(
    offset: usize,
    edits: &[RevisionTextEdit],
    start_bias: bool,
) -> usize {
    let mut delta = 0i128;
    for edit in edits {
        if offset < edit.old_start {
            return apply_revision_delta(offset, delta);
        }
        if edit.old_start == edit.old_end && offset == edit.old_start {
            return if start_bias {
                edit.new_start
            } else {
                edit.new_end
            };
        }
        if offset == edit.old_start {
            return edit.new_start;
        }
        if offset < edit.old_end {
            return if start_bias {
                edit.new_start
            } else {
                edit.new_end
            };
        }
        if offset == edit.old_end {
            return edit.new_end;
        }
        delta = edit.new_end as i128 - edit.old_end as i128;
    }
    apply_revision_delta(offset, delta)
}

pub(super) fn apply_revision_delta(offset: usize, delta: i128) -> usize {
    if delta >= 0 {
        offset.saturating_add(delta as usize)
    } else {
        offset.saturating_sub(delta.unsigned_abs() as usize)
    }
}

pub(super) fn validate_supported_block_format(
    before: &WriterDocument,
    after: &WriterDocument,
) -> Result<(), WriterError> {
    let text_is_unchanged = before.editor_text() == after.editor_text();
    if text_is_unchanged
        && (before.blocks.len() != after.blocks.len()
            || before
                .blocks
                .iter()
                .zip(&after.blocks)
                .any(|(old, new)| old.id != new.id || old.text != new.text))
    {
        return Err(WriterError::Invalid(UNRECORDABLE_BLOCK_FORMAT.into()));
    }

    for old_block in &before.blocks {
        let Some(new_block) = after.blocks.iter().find(|block| block.id == old_block.id) else {
            continue;
        };
        let block_format_changed =
            old_block.kind != new_block.kind || old_block.style != new_block.style;
        let ordinary_text_blocks =
            is_ordinary_text_block(old_block) && is_ordinary_text_block(new_block);
        if (block_format_changed && !ordinary_text_blocks)
            || old_block.image != new_block.image
            || old_block.chart != new_block.chart
            || !table_format_without_cell_runs_is_equal(old_block, new_block)
        {
            return Err(WriterError::Invalid(UNRECORDABLE_BLOCK_FORMAT.into()));
        }
    }
    Ok(())
}

fn is_ordinary_text_block(block: &crate::RichBlock) -> bool {
    let supported_kind = block.kind == "paragraph"
        || (1..=6).any(|level| block.kind == format!("heading{level}"))
        || crate::list_levels::is_list_kind(&block.kind);
    supported_kind
        && block.typed_table().is_none()
        && block.image.is_none()
        && block.chart.is_none()
}

pub(super) fn record_block_format_edits(
    document: &mut WriterDocument,
    before: &WriterDocument,
) -> Result<(), WriterError> {
    for old_block in &before.blocks {
        let Some(index) = document
            .blocks
            .iter()
            .position(|block| block.id == old_block.id)
        else {
            continue;
        };
        let block = &document.blocks[index];
        if old_block.kind == block.kind && old_block.style == block.style {
            continue;
        }
        let block_id = block.id;
        let text = block.text.as_str().to_owned();
        let runs = block_revision_runs(block);
        let payload = RevisionPayload {
            text: text.clone(),
            runs,
        };
        let after_format = RevisionBlockFormat::from_block(block);
        let before_format = RevisionBlockFormat::from_block(old_block);
        let anchor = RevisionAnchor {
            block_id,
            start: 0,
            end: text.len(),
            end_block_id: None,
            prefix_sha256: crate::zip::sha256(&[]),
        };
        document.record_revision_at_anchor(
            RevisionKind::Formatting,
            anchor,
            RevisionBoundaryKind::Unknown,
            Some(payload.clone()),
            Some(payload),
            Some(RevisionBlockFormatChange {
                before: before_format,
                after: after_format,
            }),
        )?;
    }
    Ok(())
}

pub(super) fn table_format_without_cell_runs_is_equal(old: &RichBlock, new: &RichBlock) -> bool {
    match (old.typed_table(), new.typed_table()) {
        (Some(old_table), Some(new_table)) => {
            let mut old_table = old_table.clone();
            let mut new_table = new_table.clone();
            for table in [&mut old_table, &mut new_table] {
                for row in &mut table.rows {
                    for cell in &mut row.cells {
                        cell.content.runs.clear();
                    }
                }
            }
            old_table == new_table
        }
        (None, None) => true,
        _ => false,
    }
}

pub(super) fn validate_revision_text_edits(
    before: &WriterDocument,
    after: &WriterDocument,
    edits: &[RevisionTextEdit],
) -> Result<(), WriterError> {
    let old_text = before.editor_text();
    let new_text = after.editor_text();
    let mut old_cursor = 0;
    let mut new_cursor = 0;
    for edit in edits {
        if edit.old_start < old_cursor
            || edit.new_start < new_cursor
            || edit.old_start > edit.old_end
            || edit.new_start > edit.new_end
            || old_text.get(edit.old_start..edit.old_end).is_none()
            || new_text.get(edit.new_start..edit.new_end).is_none()
        {
            return Err(WriterError::Invalid(INVALID_REVISION.into()));
        }
        if old_text.get(edit.old_start..edit.old_end) == new_text.get(edit.new_start..edit.new_end)
        {
            return Err(WriterError::Invalid(
                "revision text hunks must describe an actual text change".into(),
            ));
        }
        if old_text.get(old_cursor..edit.old_start) != new_text.get(new_cursor..edit.new_start) {
            return Err(WriterError::Invalid(INVALID_REVISION.into()));
        }
        old_cursor = edit.old_end;
        new_cursor = edit.new_end;
    }
    if old_text.get(old_cursor..) != new_text.get(new_cursor..) {
        return Err(WriterError::Invalid(INVALID_REVISION.into()));
    }
    Ok(())
}

pub(super) fn remap_revision_runs_through_hunks(
    old_revision_start: usize,
    new_revision_start: usize,
    edits: &[RevisionTextEdit],
    runs: &[StyleRun],
    new_text_len: usize,
) -> Vec<StyleRun> {
    let mut remapped = Vec::with_capacity(runs.len());
    for run in runs {
        let old_start = old_revision_start + run.start;
        let old_end = old_revision_start + run.end;
        let starts_at_insertion = edits
            .iter()
            .any(|edit| edit.old_start == old_start && edit.old_end == old_start);
        let mapped_start =
            rebase_revision_offset_through_hunks(old_start, edits, !starts_at_insertion);
        let mapped_end = rebase_revision_offset_through_hunks(old_end, edits, false);
        let start = mapped_start
            .saturating_sub(new_revision_start)
            .min(new_text_len);
        let end = mapped_end
            .saturating_sub(new_revision_start)
            .min(new_text_len);
        if start < end {
            remapped.push(StyleRun {
                start,
                end,
                style: run.style.clone(),
            });
        }
    }
    coalesce_runs(remapped)
}

pub(super) fn range_touches_table(blocks: &[RichBlock], range: Range<usize>) -> bool {
    blocks.iter().enumerate().any(|(index, block)| {
        if block.typed_table().is_none() {
            return false;
        }
        let start = block_start_offset(blocks, index);
        let end = start + block.text.len_bytes();
        range.start <= end && range.end >= start
    })
}

pub(super) fn range_is_one_table_cell(blocks: &[RichBlock], range: Range<usize>) -> bool {
    let (Some((start_index, start)), Some((end_index, end))) = (
        block_location_at(blocks, range.start),
        block_location_at(blocks, range.end),
    ) else {
        return false;
    };
    start_index == end_index && table_range_is_one_cell(&blocks[start_index], start..end)
}

pub(super) fn style_change_ranges(
    text: &str,
    before: &[StyleRun],
    after: &[StyleRun],
) -> Vec<Range<usize>> {
    let mut boundaries = vec![0, text.len()];
    boundaries.extend(before.iter().flat_map(|run| [run.start, run.end]));
    boundaries.extend(after.iter().flat_map(|run| [run.start, run.end]));
    boundaries.sort_unstable();
    boundaries.dedup();

    let mut ranges: Vec<Range<usize>> = Vec::new();
    for pair in boundaries.windows(2) {
        let range = pair[0]..pair[1];
        if range.is_empty() {
            continue;
        }
        let old_style = before
            .iter()
            .find(|run| run.start <= range.start && range.start < run.end)
            .map(|run| &run.style);
        let new_style = after
            .iter()
            .find(|run| run.start <= range.start && range.start < run.end)
            .map(|run| &run.style);
        if old_style != new_style {
            if let Some(last) = ranges.last_mut().filter(|last| last.end == range.start) {
                last.end = range.end;
            } else {
                ranges.push(range);
            }
        }
    }
    ranges
}

pub(super) fn reset_table_run_style(run: &mut loom_objects::TextRun) {
    run.bold = false;
    run.italic = false;
    run.underline = false;
    run.strikethrough = false;
    run.superscript = false;
    run.subscript = false;
    run.font_size_pt = None;
    run.font_family = None;
    run.color = None;
}

pub(super) fn apply_table_run_style(
    run: &mut loom_objects::TextRun,
    style: &loom_text::CharacterStyle,
) {
    run.bold = style.weight == loom_text::FontWeight::Bold;
    run.italic = style.italic;
    run.underline = style.underline;
    run.strikethrough = style.strikethrough;
    run.superscript = style.superscript;
    run.subscript = style.subscript;
    run.font_size_pt = style.explicit_size.then_some(style.font_size);
    run.font_family = (!style.font_family.is_empty()).then(|| style.font_family.clone());
    run.color = style.color.clone();
}

pub(super) fn runs_in_range(runs: &[StyleRun], range: Range<usize>) -> Vec<StyleRun> {
    let mut clipped = Vec::new();
    for run in runs {
        let start = run.start.max(range.start);
        let end = run.end.min(range.end);
        if start < end {
            clipped.push(StyleRun {
                start: start - range.start,
                end: end - range.start,
                style: run.style.clone(),
            });
        }
    }
    clipped
}

pub(super) fn replace_runs_in_range(
    current: &[StyleRun],
    range: Range<usize>,
    replacement: &[StyleRun],
) -> Vec<StyleRun> {
    let mut runs = Vec::new();
    for run in current {
        if run.start < range.start {
            let end = run.end.min(range.start);
            if run.start < end {
                runs.push(StyleRun {
                    start: run.start,
                    end,
                    style: run.style.clone(),
                });
            }
        }
        if run.end > range.end {
            let start = run.start.max(range.end);
            if start < run.end {
                runs.push(StyleRun {
                    start,
                    end: run.end,
                    style: run.style.clone(),
                });
            }
        }
    }
    runs.extend(replacement.iter().filter_map(|run| {
        if run.start < run.end {
            Some(StyleRun {
                start: run.start + range.start,
                end: run.end + range.start,
                style: run.style.clone(),
            })
        } else {
            None
        }
    }));
    runs.sort_by_key(|run| run.start);
    coalesce_runs(runs)
}

pub(super) fn coalesce_runs(runs: Vec<StyleRun>) -> Vec<StyleRun> {
    let mut result: Vec<StyleRun> = Vec::with_capacity(runs.len());
    for run in runs {
        if run.start == run.end {
            continue;
        }
        if let Some(last) = result.last_mut() {
            if last.end == run.start && last.style == run.style {
                last.end = run.end;
                continue;
            }
        }
        result.push(run);
    }
    result
}
