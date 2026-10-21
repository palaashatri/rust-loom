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

    /// Turn the list item that starts at `offset` into a plain paragraph: the
    /// first Backspace at the start of an item removes its marker before it
    /// can join the paragraph above. Returns whether a marker was removed.
    pub fn unlist_block_at(&mut self, offset: usize) -> bool {
        self.list_block_starting_at(offset)
            .map(|block| block.kind = "paragraph".to_string())
            .is_some()
    }

    /// Enter on an empty list item leaves the list instead of adding another
    /// empty item. Returns whether the item at `offset` was empty and listed.
    pub fn exit_empty_list_item_at(&mut self, offset: usize) -> bool {
        self.list_block_starting_at(offset)
            .filter(|block| block.text.is_empty())
            .map(|block| block.kind = "paragraph".to_string())
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
}
