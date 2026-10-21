//! A picture keeps its own line. An edit that would change a picture's
//! characters, or merge its line with a neighbouring paragraph, is refused
//! rather than written into the picture block.

use crate::RichBlock;

/// Whether replacing bytes `old_start..old_end` of the editor text would
/// change a picture other than by removing it whole. Typing beside a picture
/// is an insertion at one of its edges, so that is covered too; so is
/// removing the newline that separates a picture from its neighbour.
pub(crate) fn edit_touches_picture(blocks: &[RichBlock], old_start: usize, old_end: usize) -> bool {
    let inserting = old_start == old_end;
    let mut start = 0usize;
    for block in blocks {
        let end = start + block.text.len_bytes();
        // Removing the picture with the newline before or after it is allowed.
        let removed_whole = old_start <= start && old_end >= end;
        if block.is_image() && !removed_whole {
            let touches_inside = if inserting {
                // Before the object character, between its characters, or after the marker.
                old_start >= start && old_start <= end
            } else {
                old_start < end && old_end > start
            };
            // The newline before the picture (at start - 1) and the one after it (at end).
            let before_newline = start > 0 && old_start < start && old_end >= start;
            let after_newline = old_start <= end && old_end > end;
            if touches_inside || (!inserting && (before_newline || after_newline)) {
                return true;
            }
        }
        start = end + 1;
    }
    false
}
