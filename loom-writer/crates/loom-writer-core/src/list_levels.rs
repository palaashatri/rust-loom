//! List nesting: each list item's level (0 to 5), the indent a level adds, the
//! numbering that restarts per level, and the bullet glyph a level draws.
//!
//! The level is stored on the paragraph style, so it saves, loads, edits and
//! undoes with the rest of the paragraph. A block that is not a list item draws
//! no level, whatever its style says.

use crate::{RichBlock, WriterDocument};

/// The deepest list level (zero-based), so six levels in all.
pub const MAX_LIST_LEVEL: u8 = 5;

/// The indent each list level adds, in points.
pub const LIST_LEVEL_INDENT_PT: f32 = 18.0;

/// The bullet glyph of each level, cycling from the top level.
const BULLET_GLYPHS: [char; 3] = ['\u{2022}', '\u{25e6}', '\u{25aa}'];

/// What a level change over a selection did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ListLevelShift {
    /// List items the selection touches.
    pub items: usize,
    /// Items whose level moved; zero when every item is already at the limit.
    pub changed: usize,
}

pub(crate) fn is_list_kind(kind: &str) -> bool {
    matches!(kind, "list-bulleted" | "list-numbered")
}

/// A stored level read from a file: clamped to the deepest level, and the top
/// level when the text is not a number.
pub(crate) fn parse_list_level(text: &str) -> u8 {
    text.trim()
        .parse::<u32>()
        .map_or(0, |level| level.min(u32::from(MAX_LIST_LEVEL)) as u8)
}

/// Whether a selection or caret at `low..high` reaches a block spanning
/// `start..=end`. A caret belongs to the block it touches; a range only to the
/// blocks whose text it crosses, so selecting up to the next paragraph's first
/// character leaves that paragraph alone.
fn touches(start: usize, end: usize, low: usize, high: usize) -> bool {
    if low == high {
        (start..=end).contains(&low)
    } else {
        start < high && end > low
    }
}

impl RichBlock {
    /// The level the block is drawn at: its stored level for a list item, 0 otherwise.
    pub fn effective_list_level(&self) -> u8 {
        if is_list_kind(&self.kind) {
            self.style.list_level.min(MAX_LIST_LEVEL)
        } else {
            0
        }
    }

    /// The indent the block's list level adds to its left indent, in points.
    pub fn list_indent_pt(&self) -> f32 {
        f32::from(self.effective_list_level()) * LIST_LEVEL_INDENT_PT
    }

    /// The marker drawn beside the block: a bullet glyph for its level, or
    /// `number.` for a numbered item. Empty for other blocks.
    pub fn list_marker(&self, number: usize) -> String {
        match self.kind.as_str() {
            "list-bulleted" => {
                let level = usize::from(self.effective_list_level());
                BULLET_GLYPHS[level % BULLET_GLYPHS.len()].to_string()
            }
            "list-numbered" => format!("{number}."),
            _ => String::new(),
        }
    }
}

impl WriterDocument {
    /// The number shown beside each numbered list item, 0 elsewhere. Each
    /// level counts its own items. A block that is not a list item restarts
    /// every level; a list item restarts the levels below its own.
    pub fn list_item_numbers(&self) -> Vec<usize> {
        const LEVELS: usize = MAX_LIST_LEVEL as usize + 1;
        let mut counters = [0usize; LEVELS];
        self.blocks
            .iter()
            .map(|block| {
                if !is_list_kind(&block.kind) {
                    counters = [0; LEVELS];
                    return 0;
                }
                let level = usize::from(block.effective_list_level());
                for deeper in counters.iter_mut().skip(level + 1) {
                    *deeper = 0;
                }
                if block.kind == "list-numbered" {
                    counters[level] += 1;
                    counters[level]
                } else {
                    counters[level] = 0;
                    0
                }
            })
            .collect()
    }

    /// The list level of the list item that contains `offset`, if any.
    pub fn list_level_at(&self, offset: usize) -> Option<u8> {
        let mut at = 0usize;
        for (index, block) in self.blocks.iter().enumerate() {
            if index > 0 {
                at += 1;
            }
            let start = at;
            let end = start + block.text.len_bytes();
            if (start..=end).contains(&offset) {
                return is_list_kind(&block.kind).then(|| block.effective_list_level());
            }
            at = end;
        }
        None
    }

    /// Moves every list item the selection `low..high` touches one level
    /// deeper (`deeper`) or shallower, within the list levels. Other blocks are
    /// left as they are.
    pub fn shift_list_levels(&mut self, low: usize, high: usize, deeper: bool) -> ListLevelShift {
        let (low, high) = (low.min(high), low.max(high));
        let mut shift = ListLevelShift::default();
        let mut at = 0usize;
        for (index, block) in self.blocks.iter_mut().enumerate() {
            if index > 0 {
                at += 1;
            }
            let start = at;
            let end = start + block.text.len_bytes();
            at = end;
            if !is_list_kind(&block.kind) || !touches(start, end, low, high) {
                continue;
            }
            shift.items += 1;
            let level = block.effective_list_level();
            let next = if deeper {
                (level + 1).min(MAX_LIST_LEVEL)
            } else {
                level.saturating_sub(1)
            };
            if next != level {
                block.style.list_level = next;
                shift.changed += 1;
            }
        }
        shift
    }
}

#[cfg(test)]
#[path = "list_levels_tests.rs"]
mod tests;
