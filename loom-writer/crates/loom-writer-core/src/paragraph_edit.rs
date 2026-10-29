//! Paragraph-structure edits: what Enter, Backspace and Delete do to the
//! blocks around the edit.
//!
//! The editor reports plain text, so a split or a join arrives as an edit of
//! the text. These rules give it the behaviour of a word processor: a split
//! keeps the paragraph's kind, style and alignment on both halves, a join keeps
//! the first paragraph's, and inline runs stay on the characters they cover.

use crate::charts::CHART_BLOCK_KIND;
use crate::comments::TextEdit;
use crate::images::IMAGE_BLOCK_KIND;
use crate::tables::TABLE_BLOCK_KIND;
use crate::{RichBlock, Text, WriterDocument};
use loom_text::{ParagraphStyle, StyleRun};

#[cfg(test)]
mod tests;

fn is_list(kind: &str) -> bool {
    matches!(kind, "list-bulleted" | "list-numbered")
}

fn is_text_kind(kind: &str) -> bool {
    !matches!(kind, TABLE_BLOCK_KIND | IMAGE_BLOCK_KIND | CHART_BLOCK_KIND)
}

/// One step out of a list for a list item that is at its own start (or empty):
/// a nested item moves up a level, a top-level item becomes a paragraph.
fn step_out_of_list(block: &mut RichBlock) {
    let level = block.effective_list_level();
    if level > 0 {
        block.style.list_level = level - 1;
    } else {
        block.kind = "paragraph".to_string();
    }
}

/// Rebuild the blocks of an edit that splits or joins paragraphs. `None` means
/// the edit is not of that kind (or touches a table, picture or chart), and
/// the caller falls back to matching paragraphs by their text.
pub(crate) fn rebuild(
    old: &[RichBlock],
    paragraphs: &[String],
    edit: TextEdit,
    new_text: &str,
    next_id: &mut u64,
) -> Option<Vec<RichBlock>> {
    let mut starts = Vec::with_capacity(old.len());
    let mut at = 0usize;
    for (index, block) in old.iter().enumerate() {
        if index > 0 {
            at += 1;
        }
        starts.push(at);
        at += block.text.len_bytes();
    }
    let old_len = at;
    if edit.old_start > edit.old_end || edit.old_end > old_len || edit.new_start > edit.new_end {
        return None;
    }
    let block_end = |i: usize| starts[i] + old[i].text.len_bytes();
    let first = (0..old.len()).find(|&i| block_end(i) >= edit.old_start)?;
    let last = (first..old.len()).find(|&i| block_end(i) >= edit.old_end)?;
    let inserted = new_text.get(edit.new_start..edit.new_end)?;
    let inserted_breaks = inserted.matches('\n').count();
    if inserted_breaks == 0 && first == last {
        return None;
    }
    if !old[first..=last].iter().all(|b| is_text_kind(&b.kind)) {
        return None;
    }
    let delta = inserted.len() as isize - (edit.old_end - edit.old_start) as isize;
    let region_end = (block_end(last) as isize + delta).try_into().ok()?;
    let region_text = new_text.get(starts[first]..region_end)?;
    let count = region_text.matches('\n').count() + 1;
    let after = old.len() - 1 - last;
    if paragraphs.len() != first + count + after {
        return None;
    }
    let same =
        |old_index: usize, new_index: usize| old[old_index].text.as_str() == paragraphs[new_index];
    if !(0..first).all(|i| same(i, i)) || !(0..after).all(|k| same(last + 1 + k, first + count + k))
    {
        return None;
    }

    let pure_insert = edit.old_start == edit.old_end;
    let at_block_start = pure_insert && edit.old_start == starts[first];
    let at_block_end = pure_insert && edit.old_start == block_end(first);
    let end_of_heading =
        at_block_end && old[first].kind.starts_with("heading") && inserted.starts_with('\n');

    let mut runs: Vec<StyleRun> = Vec::new();
    for i in first..=last {
        for run in &old[i].runs {
            let (s, e) = (starts[i] + run.start, starts[i] + run.end);
            let mut push = |s: usize, e: usize| {
                if s < e {
                    runs.push(StyleRun {
                        start: s,
                        end: e,
                        style: run.style.clone(),
                    });
                }
            };
            push(s, e.min(edit.old_start));
            let shifted = s.max(edit.old_end);
            if shifted < e {
                let to = |o: usize| (o as isize + delta) as usize;
                runs.push(StyleRun {
                    start: to(shifted),
                    end: to(e),
                    style: run.style.clone(),
                });
            }
        }
    }

    let mut blocks: Vec<RichBlock> = old[..first].to_vec();
    let mut start = starts[first];
    for j in 0..count {
        let text = &paragraphs[first + j];
        let source = if j + 1 == count && last > first && count > 1 {
            &old[last]
        } else {
            &old[first]
        };
        let keeps_identity = if at_block_start && count > 1 {
            j + 1 == count
        } else {
            j == 0
        };
        let id = if keeps_identity {
            old[first].id
        } else {
            let id = *next_id;
            *next_id += 1;
            id
        };
        let mut block = source.clone();
        block.id = id;
        block.text = Text::from_str(text);
        if end_of_heading && j > 0 {
            block.kind = "paragraph".to_string();
            block.style = ParagraphStyle {
                alignment: source.style.alignment,
                direction: source.style.direction,
                ..ParagraphStyle::default()
            };
        }
        let end = start + text.len();
        let mut local: Vec<StyleRun> = Vec::new();
        for run in &runs {
            let (s, e) = (run.start.max(start), run.end.min(end));
            if s >= e {
                continue;
            }
            let piece = StyleRun {
                start: s - start,
                end: e - start,
                style: run.style.clone(),
            };
            match local.last_mut() {
                Some(prev) if prev.end == piece.start && prev.style == piece.style => {
                    prev.end = piece.end
                }
                _ => local.push(piece),
            }
        }
        block.runs = local;
        blocks.push(block);
        start = end + 1;
    }
    blocks.extend(old[last + 1..].iter().cloned());
    Some(blocks)
}

impl WriterDocument {
    fn list_block_starting_at(&mut self, offset: usize) -> Option<&mut RichBlock> {
        let mut start = 0usize;
        for (index, block) in self.blocks.iter_mut().enumerate() {
            if index > 0 {
                start += 1;
            }
            if start == offset {
                return is_list(&block.kind).then_some(block);
            }
            start += block.text.len_bytes();
            if start >= offset {
                return None;
            }
        }
        None
    }

    /// Backspace at the start of a list item: a nested item first moves up one
    /// level and keeps its marker; a top-level item loses its marker, so the
    /// next Backspace can join it to the paragraph above. Returns whether the
    /// item at `offset` changed.
    pub fn unlist_block_at(&mut self, offset: usize) -> bool {
        self.list_block_starting_at(offset)
            .map(step_out_of_list)
            .is_some()
    }

    /// Enter on an empty list item first moves a nested item up one level; a
    /// top-level item leaves the list instead of adding another empty item.
    /// Returns whether the item at `offset` was empty and listed.
    pub fn exit_empty_list_item_at(&mut self, offset: usize) -> bool {
        self.list_block_starting_at(offset)
            .filter(|block| block.text.is_empty())
            .map(step_out_of_list)
            .is_some()
    }

    /// Enter pressed on an empty list item reaches the document as an inserted
    /// line break; the item leaves the list instead and no paragraph is added.
    pub(crate) fn exit_list_on_enter(&mut self, edit: TextEdit, inserted: &str) -> bool {
        inserted
            == "
" && edit.old_start == edit.old_end
            && self.exit_empty_list_item_at(edit.old_start)
    }

    /// Split a block at byte offset, preserving styles and moving right-side footnotes.
    /// The offset is floored to a UTF-8 character boundary and clamped to the block.
    pub fn split_block(
        &mut self,
        block_id: u64,
        byte_offset: usize,
    ) -> Result<u64, crate::WriterError> {
        let index = self
            .blocks
            .iter()
            .position(|block| block.id == block_id)
            .ok_or_else(|| crate::WriterError::Invalid(format!("block {block_id} not found")))?;
        let block = &self.blocks[index];
        let original_id = block.id;
        let text = block.text.as_str();
        let text_len = text.len();
        let offset = crate::floor_char_boundary(text, byte_offset.min(text_len));
        let first_text = &text[..offset];
        let second_text = &text[offset..];

        let new_id = self.next_id();
        let mut first_block = RichBlock::new(original_id, &block.kind, first_text);
        first_block.style = block.style.clone();
        for run in &block.runs {
            if run.start < offset {
                let start = crate::floor_char_boundary(text, run.start.min(text_len));
                let end = crate::floor_char_boundary(text, run.end.min(offset));
                if start < end {
                    first_block.runs.push(StyleRun {
                        start,
                        end,
                        style: run.style.clone(),
                    });
                }
            }
        }

        let mut second_block = RichBlock::new(new_id, &block.kind, second_text);
        second_block.style = block.style.clone();
        for run in &block.runs {
            if run.end > offset {
                let start = run.start.saturating_sub(offset);
                let end = run.end - offset;
                if start < end {
                    second_block.runs.push(StyleRun {
                        start,
                        end,
                        style: run.style.clone(),
                    });
                }
            }
        }

        self.blocks[index] = first_block;
        self.blocks.insert(index + 1, second_block);
        crate::footnotes::rebase_after_split(&mut self.footnotes, original_id, new_id, offset);
        Ok(new_id)
    }

    /// Merge adjacent blocks, retaining their footnote anchors and stable IDs.
    pub fn merge_blocks(
        &mut self,
        first_id: u64,
        second_id: u64,
    ) -> Result<(), crate::WriterError> {
        let first_index = self
            .blocks
            .iter()
            .position(|block| block.id == first_id)
            .ok_or_else(|| crate::WriterError::Invalid(format!("block {first_id} not found")))?;
        let second_index = self
            .blocks
            .iter()
            .position(|block| block.id == second_id)
            .ok_or_else(|| crate::WriterError::Invalid(format!("block {second_id} not found")))?;
        if second_index != first_index + 1 {
            return Err(crate::WriterError::Invalid(
                "blocks must be adjacent to merge".into(),
            ));
        }

        let first = &self.blocks[first_index];
        let second = &self.blocks[second_index];
        let first_len = first.text.as_str().len();
        let combined_text = format!("{}{}", first.text.as_str(), second.text.as_str());
        let mut combined_block = RichBlock::new(first.id, &first.kind, &combined_text);
        combined_block.style = first.style.clone();
        combined_block.runs.extend(first.runs.clone());
        for run in &second.runs {
            combined_block.runs.push(StyleRun {
                start: run.start + first_len,
                end: run.end + first_len,
                style: run.style.clone(),
            });
        }
        combined_block.runs.sort_by_key(|run| run.start);
        let mut coalesced: Vec<StyleRun> = Vec::with_capacity(combined_block.runs.len());
        for run in combined_block.runs.drain(..) {
            if run.start == run.end {
                continue;
            }
            if let Some(last) = coalesced.last_mut() {
                if last.end == run.start && last.style == run.style {
                    last.end = run.end;
                    continue;
                }
            }
            coalesced.push(run);
        }
        combined_block.runs = coalesced;
        crate::footnotes::rebase_after_merge(&mut self.footnotes, first_id, second_id, first_len)
            .map_err(crate::WriterError::Invalid)?;

        self.blocks[first_index] = combined_block;
        self.blocks.remove(second_index);
        Ok(())
    }
}
