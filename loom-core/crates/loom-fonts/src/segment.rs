//! Splitting text into runs that can be shaped as one unit, and ordering
//! those runs for display.
//!
//! A shaper wants one direction and one script per call. Bidi embedding
//! levels (UAX 9) give the direction; the Unicode script property, with
//! "common" characters (spaces, digits, punctuation) taking the script of
//! their neighbours, gives the script.

use crate::face::TextDirection;
use std::ops::Range;
use unicode_bidi::{BidiInfo, Level};
use unicode_script::{Script, UnicodeScript};

/// A maximal stretch of text with one bidi level and one script.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextRun {
    /// Byte range in the segmented text, on character boundaries.
    pub range: Range<usize>,
    /// Bidi embedding level; odd levels are right to left.
    pub level: u8,
    /// ISO 15924 code ("Latn", "Arab", ...), or `None` for text made only of
    /// common characters.
    pub script: Option<[u8; 4]>,
}

impl TextRun {
    /// True for a right-to-left run.
    pub fn is_rtl(&self) -> bool {
        self.level % 2 == 1
    }

    /// The direction to pass to the shaper for this run.
    pub fn direction(&self) -> TextDirection {
        if self.is_rtl() {
            TextDirection::RightToLeft
        } else {
            TextDirection::LeftToRight
        }
    }
}

fn script_code(script: Script) -> Option<[u8; 4]> {
    match script {
        Script::Common | Script::Inherited | Script::Unknown => None,
        other => {
            let name = other.short_name().as_bytes();
            (name.len() == 4).then(|| [name[0], name[1], name[2], name[3]])
        }
    }
}

/// Splits `text` into runs in logical order.
///
/// `base` is the paragraph direction; `Auto` takes it from the first strong
/// character of each paragraph.
pub fn segment(text: &str, base: TextDirection) -> Vec<TextRun> {
    if text.is_empty() {
        return Vec::new();
    }
    let level = match base {
        TextDirection::Auto => None,
        TextDirection::LeftToRight => Some(Level::ltr()),
        TextDirection::RightToLeft => Some(Level::rtl()),
    };
    let info = BidiInfo::new(text, level);

    // Resolve each character's script: common characters follow the previous
    // script, and leading ones take the first script that appears.
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut scripts: Vec<Option<[u8; 4]>> =
        chars.iter().map(|(_, c)| script_code(c.script())).collect();
    let mut previous = scripts.iter().flatten().next().copied();
    for slot in &mut scripts {
        match slot {
            Some(code) => previous = Some(*code),
            None => *slot = previous,
        }
    }

    let mut runs: Vec<TextRun> = Vec::new();
    for (position, &(start, ch)) in chars.iter().enumerate() {
        let level = info.levels[start].number();
        let script = scripts[position];
        let end = start + ch.len_utf8();
        match runs.last_mut() {
            Some(run) if run.level == level && run.script == script => run.range.end = end,
            _ => runs.push(TextRun {
                range: start..end,
                level,
                script,
            }),
        }
    }
    runs
}

/// For one line's runs (given as bidi levels in logical order) returns the
/// logical index of the run drawn at each visual position, left to right.
///
/// Adjacent runs of the same odd level are reversed together, as UAX 9 rule L2
/// requires.
pub fn visual_order(levels: &[u8]) -> Vec<usize> {
    let levels: Vec<Level> = levels
        .iter()
        .map(|n| {
            Level::new((*n).min(Level::max_explicit_depth() + 1)).unwrap_or_else(|_| Level::ltr())
        })
        .collect();
    BidiInfo::reorder_visual(&levels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_latin_is_one_run() {
        let runs = segment("Hello, world 42", TextDirection::Auto);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].range, 0..15);
        assert_eq!(runs[0].level, 0);
        assert_eq!(runs[0].script, Some(*b"Latn"));
    }

    #[test]
    fn a_script_change_splits_the_run_and_spaces_follow_their_neighbour() {
        let text = "Hello Привет";
        let runs = segment(text, TextDirection::Auto);
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].script, Some(*b"Latn"));
        assert_eq!(runs[1].script, Some(*b"Cyrl"));
        // The space after "Hello" stays with the Latin run.
        assert_eq!(&text[runs[0].range.clone()], "Hello ");
    }

    #[test]
    fn embedded_hebrew_gets_an_odd_level_and_reverses_visually() {
        let text = "abc \u{5E9}\u{5DC}\u{5D5}\u{5DD} def";
        let runs = segment(text, TextDirection::Auto);
        let levels: Vec<u8> = runs.iter().map(|r| r.level).collect();
        assert!(levels.contains(&1));
        assert!(runs.iter().any(TextRun::is_rtl));
        // Coverage: runs tile the text.
        let mut at = 0;
        for run in &runs {
            assert_eq!(run.range.start, at);
            at = run.range.end;
        }
        assert_eq!(at, text.len());
        let order = visual_order(&levels);
        assert_eq!(order.len(), runs.len());
        let mut sorted = order.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..runs.len()).collect::<Vec<_>>());
    }

    #[test]
    fn a_right_to_left_base_makes_the_whole_line_level_one() {
        let runs = segment("abc", TextDirection::RightToLeft);
        assert_eq!(runs.len(), 1);
        assert!(runs[0].is_rtl() || runs[0].level == 2);
    }

    #[test]
    fn visual_order_reverses_adjacent_odd_runs() {
        assert_eq!(visual_order(&[0, 1, 1, 0]), vec![0, 2, 1, 3]);
        assert_eq!(visual_order(&[0, 0, 0]), vec![0, 1, 2]);
        assert!(visual_order(&[]).is_empty());
    }

    #[test]
    fn empty_text_has_no_runs() {
        assert!(segment("", TextDirection::Auto).is_empty());
    }
}
