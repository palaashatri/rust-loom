use super::*;

/// The original full-table implementation, kept as the reference.
fn reference_matches(old: &[RichBlock], new: &[String]) -> Vec<Option<usize>> {
    let mut lcs = vec![vec![0usize; new.len() + 1]; old.len() + 1];
    for old_index in (0..old.len()).rev() {
        for new_index in (0..new.len()).rev() {
            lcs[old_index][new_index] = if old[old_index].text.as_str() == new[new_index] {
                1 + lcs[old_index + 1][new_index + 1]
            } else {
                lcs[old_index + 1][new_index].max(lcs[old_index][new_index + 1])
            };
        }
    }

    let mut matches = vec![None; new.len()];
    let (mut old_index, mut new_index) = (0, 0);
    while old_index < old.len() && new_index < new.len() {
        if old[old_index].text.as_str() == new[new_index]
            && lcs[old_index][new_index] == 1 + lcs[old_index + 1][new_index + 1]
        {
            matches[new_index] = Some(old_index);
            old_index += 1;
            new_index += 1;
        } else if lcs[old_index + 1][new_index] >= lcs[old_index][new_index + 1] {
            old_index += 1;
        } else {
            new_index += 1;
        }
    }
    matches
}

fn blocks(texts: &[&str]) -> Vec<RichBlock> {
    texts
        .iter()
        .enumerate()
        .map(|(id, text)| RichBlock::new(id as u64 + 1, "paragraph", text))
        .collect()
}

fn owned(texts: &[&str]) -> Vec<String> {
    texts.iter().map(|text| text.to_string()).collect()
}

/// Every sequence over a small alphabet up to `length`, so duplicated
/// paragraphs (the ambiguous cases) are well covered.
fn sequences(length: usize, alphabet: &[&'static str]) -> Vec<Vec<&'static str>> {
    let mut all = vec![Vec::new()];
    let mut layer = vec![Vec::new()];
    for _ in 0..length {
        let mut next = Vec::new();
        for sequence in &layer {
            for letter in alphabet {
                let mut longer = sequence.clone();
                longer.push(*letter);
                next.push(longer);
            }
        }
        all.extend(next.iter().cloned());
        layer = next;
    }
    all
}

#[test]
fn matches_equal_the_full_table_for_every_small_sequence_pair() {
    let all = sequences(5, &["a", "b", "c"]);
    let mut compared = 0usize;
    for old in all.iter().filter(|seq| seq.len() <= 5) {
        for new in all.iter().filter(|seq| seq.len() <= 5) {
            let old_blocks = blocks(old);
            let new_texts = owned(new);
            assert_eq!(
                exact_paragraph_matches(&old_blocks, &new_texts),
                reference_matches(&old_blocks, &new_texts),
                "{old:?} -> {new:?}"
            );
            compared += 1;
        }
    }
    assert!(compared > 100_000);
}

#[test]
fn a_one_paragraph_edit_in_a_long_document_matches_the_reference() {
    let paragraphs: Vec<String> = (0..300).map(|n| format!("paragraph {}", n % 40)).collect();
    let refs: Vec<&str> = paragraphs.iter().map(String::as_str).collect();
    let old = blocks(&refs);
    for edit in [0usize, 1, 150, 298, 299] {
        let mut changed = paragraphs.clone();
        changed[edit].push_str(" edited");
        assert_eq!(
            exact_paragraph_matches(&old, &changed),
            reference_matches(&old, &changed)
        );
        let mut split = paragraphs.clone();
        split.insert(edit, String::new());
        assert_eq!(
            exact_paragraph_matches(&old, &split),
            reference_matches(&old, &split)
        );
        let mut joined = paragraphs.clone();
        joined.remove(edit);
        assert_eq!(
            exact_paragraph_matches(&old, &joined),
            reference_matches(&old, &joined)
        );
    }
}

#[test]
fn identical_and_empty_inputs_match_in_place() {
    let old = blocks(&["x", "y", "z"]);
    assert_eq!(
        exact_paragraph_matches(&old, &owned(&["x", "y", "z"])),
        vec![Some(0), Some(1), Some(2)]
    );
    assert_eq!(
        exact_paragraph_matches(&old, &[]),
        Vec::<Option<usize>>::new()
    );
    assert_eq!(exact_paragraph_matches(&[], &owned(&["x"])), vec![None]);
}
