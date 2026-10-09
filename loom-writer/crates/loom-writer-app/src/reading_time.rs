//! The reading-time caption in the inspector's status card.

/// Words read per minute that the caption assumes (a typical silent reading
/// speed for prose).
const WORDS_PER_MINUTE: usize = 200;

/// The minutes the caption shows for `word_count` words. An empty document has
/// no reading time (`0`, so the caption is left blank); any text takes at least
/// one minute.
pub(crate) fn minutes_for_words(word_count: usize) -> i32 {
    if word_count == 0 {
        return 0;
    }
    word_count
        .div_ceil(WORDS_PER_MINUTE)
        .clamp(1, i32::MAX as usize) as i32
}

#[cfg(test)]
mod tests {
    use super::minutes_for_words;

    #[test]
    fn empty_documents_have_no_reading_time_and_short_text_takes_a_minute() {
        assert_eq!(minutes_for_words(0), 0);
        assert_eq!(minutes_for_words(1), 1);
        assert_eq!(minutes_for_words(200), 1);
        assert_eq!(minutes_for_words(201), 2);
    }
}
