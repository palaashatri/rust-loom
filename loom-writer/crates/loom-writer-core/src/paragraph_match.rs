//! Matching an edited document's paragraphs to the old blocks.

use crate::{RichBlock, Text};

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

/// Add conservative metadata matches for changed paragraphs in an unmatched
/// gap. Equal-sized gaps retain positional matching. For unequal gaps, a
/// small sequence alignment pairs only sufficiently similar text, leaving
/// unrelated insertions with fresh metadata instead of borrowing a deleted
/// block's id, kind, or style.
pub(crate) fn metadata_matches(
    old: &[RichBlock],
    new: &[String],
    exact_matches: &[Option<usize>],
) -> Vec<Option<usize>> {
    let anchors = exact_matches
        .iter()
        .enumerate()
        .filter_map(|(new_index, old_index)| old_index.map(|old_index| (new_index, old_index)));
    let mut matches = exact_matches.to_vec();
    let mut old_start = 0;
    let mut new_start = 0;

    for (new_end, old_end) in anchors.chain(std::iter::once((new.len(), old.len()))) {
        let old_gap_len = old_end - old_start;
        let new_gap_len = new_end - new_start;
        if old_gap_len == new_gap_len {
            for offset in 0..new_gap_len {
                if matches[new_start + offset].is_none() {
                    matches[new_start + offset] = Some(old_start + offset);
                }
            }
        } else if old_gap_len > 0 && new_gap_len > 0 {
            for (new_offset, old_offset) in
                align_metadata_gap(&old[old_start..old_end], &new[new_start..new_end])
            {
                matches[new_start + new_offset] = Some(old_start + old_offset);
            }
        }

        if new_end < new.len() {
            old_start = old_end + 1;
            new_start = new_end + 1;
        }
    }

    matches
}

fn align_metadata_gap(old: &[RichBlock], new: &[String]) -> Vec<(usize, usize)> {
    let mut scores = vec![vec![0usize; new.len() + 1]; old.len() + 1];
    for old_index in (0..old.len()).rev() {
        for new_index in (0..new.len()).rev() {
            let skip_old = scores[old_index + 1][new_index];
            let skip_new = scores[old_index][new_index + 1];
            let pair = paragraph_similarity(&old[old_index].text, &new[new_index])
                .map(|score| score + scores[old_index + 1][new_index + 1])
                .unwrap_or(0);
            scores[old_index][new_index] = skip_old.max(skip_new).max(pair);
        }
    }

    let mut pairs = Vec::new();
    let (mut old_index, mut new_index) = (0, 0);
    while old_index < old.len() && new_index < new.len() {
        let score = paragraph_similarity(&old[old_index].text, &new[new_index]);
        let pair = score
            .map(|score| score + scores[old_index + 1][new_index + 1])
            .unwrap_or(0);
        let skip_old = scores[old_index + 1][new_index];
        let skip_new = scores[old_index][new_index + 1];
        if score.is_some() && pair >= skip_old.max(skip_new) {
            pairs.push((new_index, old_index));
            old_index += 1;
            new_index += 1;
        } else if skip_old >= skip_new {
            old_index += 1;
        } else {
            new_index += 1;
        }
    }
    pairs
}

/// Return a normalized similarity score when the unchanged prefix/suffix is
/// substantial enough to identify an edited paragraph. The score is scaled so
/// sequence alignment can compare multiple candidate pairs without floats.
fn paragraph_similarity(old: &Text, new: &str) -> Option<usize> {
    let old_chars: Vec<char> = old.as_str().chars().collect();
    let new_chars: Vec<char> = new.chars().collect();
    let prefix = old_chars
        .iter()
        .zip(&new_chars)
        .take_while(|(old, new)| old == new)
        .count();
    let max_suffix = (old_chars.len() - prefix).min(new_chars.len() - prefix);
    let suffix = (0..max_suffix)
        .take_while(|offset| {
            old_chars[old_chars.len() - 1 - offset] == new_chars[new_chars.len() - 1 - offset]
        })
        .count();
    let shared = prefix + suffix;
    let shorter = old_chars.len().min(new_chars.len());
    if shared == 0 || shorter == 0 || shared * 2 < shorter {
        return None;
    }
    Some(shared * 1_000 / old_chars.len().max(new_chars.len()))
}
