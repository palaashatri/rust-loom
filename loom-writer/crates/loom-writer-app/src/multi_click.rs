//! Double-click selects a word and triple-click selects a paragraph.
//!
//! The ranges are pure functions of the editor text so they can be tested
//! without a window; the click counter takes the current instant for the same
//! reason.

use std::cell::Cell;
use std::time::{Duration, Instant};

/// Consecutive presses closer together than this count as one multi-click.
const MULTI_CLICK_WINDOW: Duration = Duration::from_millis(500);
/// ...and must land within this many logical pixels of the previous press.
const MULTI_CLICK_SLOP: f32 = 4.0;

#[derive(Debug, Clone, Copy)]
struct LastPress {
    at: Instant,
    x: f32,
    y: f32,
    count: u8,
}

/// Counts consecutive presses: 1, 2, 3, then back to 1.
#[derive(Debug, Default)]
pub(crate) struct ClickCounter {
    last: Cell<Option<LastPress>>,
}

impl ClickCounter {
    pub(crate) fn register(&self, now: Instant, x: f32, y: f32) -> u8 {
        let count = match self.last.get() {
            Some(last)
                if now.saturating_duration_since(last.at) <= MULTI_CLICK_WINDOW
                    && (x - last.x).abs() <= MULTI_CLICK_SLOP
                    && (y - last.y).abs() <= MULTI_CLICK_SLOP =>
            {
                last.count % 3 + 1
            }
            _ => 1,
        };
        self.last.set(Some(LastPress {
            at: now,
            x,
            y,
            count,
        }));
        count
    }
}

thread_local! {
    static COUNTER: ClickCounter = ClickCounter::default();
}

/// Count a press of the page at `(x, y)` now: 1, 2 or 3.
pub(crate) fn register_press(x: f32, y: f32) -> u8 {
    COUNTER.with(|counter| counter.register(Instant::now(), x, y))
}

#[derive(PartialEq, Eq)]
enum CharClass {
    Word,
    Space,
    Other,
}

fn class_of(ch: char) -> CharClass {
    if ch.is_alphanumeric() || ch == '_' {
        CharClass::Word
    } else if ch.is_whitespace() {
        CharClass::Space
    } else {
        CharClass::Other
    }
}

fn floor_boundary(text: &str, mut offset: usize) -> usize {
    offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

/// The run of same-kind characters (word, spaces or punctuation) around the
/// byte `offset`.
///
/// A pointer hit lands on the nearest caret position, so clicking the right half
/// of a word's last letter gives the offset just after it. A word on either side
/// of the offset therefore wins over spaces and punctuation, and a click past
/// the end of a line still selects the last word.
pub(crate) fn word_range(text: &str, offset: usize) -> (usize, usize) {
    let offset = floor_boundary(text, offset);
    let at = text[offset..].chars().next().filter(|ch| *ch != '\n');
    let before = text[..offset].chars().next_back().filter(|ch| *ch != '\n');
    let is_word = |ch: &char| class_of(*ch) == CharClass::Word;
    let anchor = match (at, before) {
        (Some(a), _) if is_word(&a) => offset,
        (_, Some(b)) if is_word(&b) => offset - b.len_utf8(),
        (Some(_), _) => offset,
        (None, Some(b)) => offset - b.len_utf8(),
        (None, None) => return (offset, offset),
    };
    let target = class_of(
        text[anchor..]
            .chars()
            .next()
            .expect("anchor is on a character"),
    );
    let mut start = anchor;
    for (index, ch) in text[..anchor].char_indices().rev() {
        if ch == '\n' || class_of(ch) != target {
            break;
        }
        start = index;
    }
    let mut end = anchor;
    for (index, ch) in text[anchor..].char_indices() {
        if ch == '\n' || class_of(ch) != target {
            break;
        }
        end = anchor + index + ch.len_utf8();
    }
    (start, end)
}

/// The whole paragraph (up to, not including, its line break) around `offset`.
pub(crate) fn paragraph_range(text: &str, offset: usize) -> (usize, usize) {
    let offset = floor_boundary(text, offset);
    let start = text[..offset].rfind('\n').map_or(0, |index| index + 1);
    let end = text[offset..]
        .find('\n')
        .map_or(text.len(), |index| offset + index);
    (start, end)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = "Welcome to Loom, Writer!\nSecond  paragraph here";

    fn picked(range: (usize, usize)) -> &'static str {
        &TEXT[range.0..range.1]
    }

    #[test]
    fn a_double_click_selects_the_word_under_the_pointer() {
        assert_eq!(picked(word_range(TEXT, 0)), "Welcome");
        assert_eq!(picked(word_range(TEXT, 3)), "Welcome");
        assert_eq!(
            picked(word_range(TEXT, 7)),
            "Welcome",
            "the edge belongs to the next char"
        );
        let loom = TEXT.find("Loom").unwrap();
        assert_eq!(picked(word_range(TEXT, loom + 2)), "Loom");
    }

    #[test]
    fn spaces_and_punctuation_select_their_own_run() {
        let text = "wow!! ok  go";
        let bangs = text.find("!!").unwrap();
        assert_eq!(
            &text[word_range(text, bangs + 1).0..word_range(text, bangs + 1).1],
            "!!"
        );
        let gap = text.find("  ").unwrap();
        assert_eq!(
            &text[word_range(text, gap + 1).0..word_range(text, gap + 1).1],
            "  "
        );
    }

    #[test]
    fn a_click_past_the_end_of_a_line_selects_the_last_word_and_stays_in_the_line() {
        let bang = TEXT.find('!').unwrap();
        assert_eq!(
            picked(word_range(TEXT, bang)),
            "Writer",
            "between a word and a mark the word wins"
        );
        assert_eq!(
            picked(word_range(TEXT, bang + 1)),
            "!",
            "past the mark at the end of a line, only the mark is adjacent"
        );
        assert_eq!(picked(word_range(TEXT, TEXT.len())), "here");
        assert_eq!(word_range("", 0), (0, 0));
        assert_eq!(
            word_range("a\n\nb", 2),
            (2, 2),
            "an empty line selects nothing"
        );
    }

    #[test]
    fn selection_never_crosses_a_paragraph_break_or_splits_a_character() {
        let text = "naïve café\nüber";
        let accent = text.find('ï').unwrap();
        assert_eq!(
            &text[word_range(text, accent + 1).0..word_range(text, accent + 1).1],
            "naïve"
        );
        let (start, end) = word_range(text, text.find('\n').unwrap() + 1);
        assert_eq!(&text[start..end], "über");
    }

    #[test]
    fn a_triple_click_selects_the_whole_paragraph() {
        assert_eq!(picked(paragraph_range(TEXT, 5)), "Welcome to Loom, Writer!");
        let second = TEXT.find("Second").unwrap();
        assert_eq!(
            picked(paragraph_range(TEXT, second + 4)),
            "Second  paragraph here"
        );
        assert_eq!(
            picked(paragraph_range(TEXT, TEXT.len())),
            "Second  paragraph here"
        );
    }

    #[test]
    fn presses_count_up_to_three_then_start_over_and_reset_when_far_or_slow() {
        let counter = ClickCounter::default();
        let t0 = Instant::now();
        let at = |ms: u64| t0 + Duration::from_millis(ms);
        assert_eq!(counter.register(at(0), 100.0, 100.0), 1);
        assert_eq!(counter.register(at(200), 101.0, 99.0), 2);
        assert_eq!(counter.register(at(400), 100.0, 100.0), 3);
        assert_eq!(
            counter.register(at(600), 100.0, 100.0),
            1,
            "a fourth press starts over"
        );
        assert_eq!(counter.register(at(700), 100.0, 100.0), 2);
        assert_eq!(counter.register(at(1500), 100.0, 100.0), 1, "too slow");
        assert_eq!(counter.register(at(1600), 140.0, 100.0), 1, "moved too far");
    }
}
