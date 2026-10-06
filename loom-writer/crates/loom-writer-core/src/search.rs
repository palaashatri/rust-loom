//! Fast literal search for the common case of ASCII text.

/// Every position where `text` equals the lowercase ASCII `needle` ignoring
/// case, as byte ranges. Overlapping matches are all reported, like the general
/// character-window search this replaces for plain ASCII blocks.
pub(crate) fn ascii_case_insensitive<'a>(
    text: &'a str,
    needle: &'a [u8],
) -> impl Iterator<Item = (usize, usize)> + 'a {
    let bytes = text.as_bytes();
    let length = needle.len();
    let last_start = if length == 0 || bytes.len() < length {
        0
    } else {
        bytes.len() - length + 1
    };
    (0..last_start).filter_map(move |start| {
        let window = &bytes[start..start + length];
        window
            .iter()
            .zip(needle)
            .all(|(a, b)| a.to_ascii_lowercase() == *b)
            .then_some((start, start + length))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_ignore_case_and_overlap() {
        let hits: Vec<_> = ascii_case_insensitive("aAa Aa", b"aa").collect();
        assert_eq!(hits, vec![(0, 2), (1, 3), (4, 6)]);
        assert_eq!(ascii_case_insensitive("abc", b"").count(), 0);
        assert_eq!(ascii_case_insensitive("ab", b"abc").count(), 0);
    }
}

#[cfg(test)]
mod document_tests {
    use crate::WriterDocument;

    /// The byte-by-byte ASCII path must agree with the general Unicode path,
    /// which a non-ASCII block in the same document still uses.
    #[test]
    fn ascii_search_agrees_with_the_general_path() {
        let ascii = "Dolor sit AMET dolor DOLOR\nnothing here\ndoloRdolor";
        let mut plain = WriterDocument::new("a", "A");
        plain.replace_paragraphs(ascii);
        // A trailing non-ASCII character forces the general path on each block.
        let mut general = WriterDocument::new("b", "B");
        general.replace_paragraphs(
            &ascii
                .lines()
                .map(|line| format!("{line}\u{e9}"))
                .collect::<Vec<_>>()
                .join("\n"),
        );
        let spans = |document: &WriterDocument| -> Vec<(usize, usize)> {
            document
                .find_all("dolor", false)
                .into_iter()
                .map(|hit| (hit.start, hit.end))
                .collect()
        };
        assert_eq!(spans(&plain), spans(&general));
        assert_eq!(spans(&plain).len(), 5);
    }
}
