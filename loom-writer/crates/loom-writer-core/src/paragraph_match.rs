//! Matching an edited document's paragraphs to the old blocks.

use crate::RichBlock;

/// Match exact paragraph text in order, retaining the stable metadata of the
/// old block. LCS gives insertions and deletions in the middle the same
/// behavior as edits at the beginning or end, instead of relying on position.
///
/// An edit changes a few paragraphs in a long document, so the paragraphs the
/// old and new text share at both ends are set aside and the table is built
/// only for what lies between. Every table value outside that region has a
/// closed form, so the traceback visits exactly the cells the full table would
/// and picks the same match among equal paragraphs.
pub(crate) fn exact_paragraph_matches(old: &[RichBlock], new: &[String]) -> Vec<Option<usize>> {
    let (old_len, new_len) = (old.len(), new.len());
    let same = |old_index: usize, new_index: usize| old[old_index].text.as_str() == new[new_index];
    let shorter = old_len.min(new_len);
    let prefix = (0..shorter).take_while(|&at| same(at, at)).count();
    let suffix = (0..shorter - prefix)
        .take_while(|&at| same(old_len - 1 - at, new_len - 1 - at))
        .count();
    let (old_end, new_end) = (old_len - suffix, new_len - suffix);

    let columns = new_end - prefix + 1;
    let mut middle = vec![0usize; (old_end - prefix + 1) * columns];
    let cell =
        |old_index: usize, new_index: usize| (old_index - prefix) * columns + new_index - prefix;
    for old_index in (prefix..old_end).rev() {
        for new_index in (prefix..new_end).rev() {
            middle[cell(old_index, new_index)] = if same(old_index, new_index) {
                1 + middle[cell(old_index + 1, new_index + 1)]
            } else {
                middle[cell(old_index + 1, new_index)].max(middle[cell(old_index, new_index + 1)])
            };
        }
    }
    // Length of the longest common subsequence of `old[old_index..]` and
    // `new[new_index..]`. Once either side is inside the shared tail, the
    // shorter remainder is wholly contained in the other.
    let lcs = |old_index: usize, new_index: usize| {
        if old_index >= old_end || new_index >= new_end {
            (old_len - old_index).min(new_len - new_index)
        } else {
            middle[cell(old_index, new_index)] + suffix
        }
    };

    let mut matches = vec![None; new_len];
    for (at, slot) in matches.iter_mut().enumerate().take(prefix) {
        *slot = Some(at);
    }
    let (mut old_index, mut new_index) = (prefix, prefix);
    while old_index < old_len && new_index < new_len {
        if same(old_index, new_index)
            && lcs(old_index, new_index) == 1 + lcs(old_index + 1, new_index + 1)
        {
            matches[new_index] = Some(old_index);
            old_index += 1;
            new_index += 1;
        } else if lcs(old_index + 1, new_index) >= lcs(old_index, new_index + 1) {
            old_index += 1;
        } else {
            new_index += 1;
        }
    }
    matches
}

#[cfg(test)]
mod tests;
