//! Line breaking for the PDF export. Lines are wrapped with the widths the PDF
//! draws with (the embedded Inter faces, bold and italic per run, measured from
//! the font program itself), so a line and its appended runs never pass the
//! right margin. The editor measures with the same advances and breaks at the
//! same places (see `text_metrics`), so a page of the PDF reads like the page
//! on screen.

use loom_pdf::{text_width_pt, TextStyle};
use loom_text::StyleRun;
use unicode_segmentation::UnicodeSegmentation;

use crate::text_metrics::is_bold_weight;

/// The PDF style of the character at byte `index`: the block's base style with
/// the bold and italic of the run that covers it. Bold follows the editor's
/// rule: weight 700 and up (Semibold draws as regular in both).
pub(crate) fn run_style(base: &TextStyle, runs: &[StyleRun], index: usize) -> TextStyle {
    let mut style = base.clone();
    if let Some(run) = runs
        .iter()
        .find(|run| run.start <= index && index < run.end)
    {
        style.bold |= is_bold_weight(run.style.weight);
        style.italic |= run.style.italic;
    }
    style
}

/// The advance of one grapheme (a character and the marks that follow it)
/// when set in `style`.
fn advance(grapheme: &str, style: &TextStyle) -> f32 {
    text_width_pt(grapheme, style)
}

/// The width of `marker` followed by `text`, with each grapheme in its own
/// run style.
pub(crate) fn line_width(
    marker: &str,
    text: &str,
    runs: &[StyleRun],
    base: &TextStyle,
    text_start: usize,
) -> f32 {
    let mut width = if marker.is_empty() {
        0.0
    } else {
        text_width_pt(marker, base)
    };
    for (offset, grapheme) in text.grapheme_indices(true) {
        width += advance(grapheme, &run_style(base, runs, text_start + offset));
    }
    width
}

/// Breaks `text` into lines whose drawn width stays within `width` points; the
/// first line may be narrower (`first_width`) to leave room for a list marker.
/// Lines break after spaces. A word wider than its line is broken between its
/// graphemes. A hard line break (a newline) ends its line; the break itself is not
/// part of either line. Returns the byte range of each line; empty text has
/// one line, and text that ends in a hard break has no empty line after it
/// (as in the editor).
pub(crate) fn wrap(
    text: &str,
    runs: &[StyleRun],
    base: &TextStyle,
    first_width: f32,
    width: f32,
) -> Vec<(usize, usize)> {
    let mut segments: Vec<&str> = text.split('\n').collect();
    if segments.len() > 1 && segments.last() == Some(&"") {
        segments.pop();
    }
    let mut lines = Vec::new();
    let mut offset = 0usize;
    for (index, segment) in segments.into_iter().enumerate() {
        let first = if index == 0 { first_width } else { width };
        for (start, end) in wrap_segment(segment, offset, runs, base, first, width) {
            lines.push((offset + start, offset + end));
        }
        offset += segment.len() + 1;
    }
    lines
}

/// [`wrap`] for text with no hard breaks; `origin` is its byte offset in the
/// block, which the character runs are expressed in.
fn wrap_segment(
    text: &str,
    origin: usize,
    runs: &[StyleRun],
    base: &TextStyle,
    first_width: f32,
    width: f32,
) -> Vec<(usize, usize)> {
    // Measure every grapheme once, in the face its run selects.
    let chars: Vec<(usize, &str, f32)> = text
        .grapheme_indices(true)
        .map(|(index, grapheme)| {
            let style = run_style(base, runs, origin + index);
            (index, grapheme, advance(grapheme, &style))
        })
        .collect();

    // The current line runs from `line_start`. `to_word` is its width up to the
    // end of its last word, `last_word_end` that byte, and `spaces` the width of
    // the spaces after that word. Spaces are kept on the line while they fit and
    // are dropped at a break when they would not.
    let mut lines = Vec::new();
    let mut line_start = 0usize;
    let mut to_word = 0.0f32;
    let mut last_word_end = 0usize;
    let mut spaces = 0.0f32;
    let mut limit = first_width;
    let mut has_word = false;
    let mut next = 0usize;
    while next < chars.len() {
        let word_begin = next;
        while next < chars.len() && chars[next].1 != " " {
            next += 1;
        }
        let word_end = next;
        while next < chars.len() && chars[next].1 == " " {
            next += 1;
        }
        let word = &chars[word_begin..word_end];
        let word_width: f32 = word.iter().map(|character| character.2).sum();
        let space_width: f32 = chars[word_end..next].iter().map(|c| c.2).sum();
        if let Some(&(first_byte, _, _)) = word.first() {
            // A word that does not fit moves whole to the next line.
            if has_word && to_word + spaces + word_width > limit {
                if to_word + spaces <= limit {
                    lines.push((line_start, first_byte));
                } else {
                    lines.push((line_start, last_word_end));
                }
                line_start = first_byte;
                to_word = 0.0;
                spaces = 0.0;
                limit = width;
                has_word = false;
            } else {
                // Spaces between two words on one line are inside that line.
                to_word += spaces;
                spaces = 0.0;
            }
            // A word still too wide for a line is broken between graphemes.
            for &(byte, grapheme, character_width) in word {
                if has_word && to_word + character_width > limit && byte > line_start {
                    lines.push((line_start, byte));
                    line_start = byte;
                    to_word = 0.0;
                    spaces = 0.0;
                    limit = width;
                }
                to_word += character_width;
                last_word_end = byte + grapheme.len();
                has_word = true;
            }
        }
        if has_word {
            spaces = space_width;
        } else {
            // Leading spaces of a line are drawn and take room.
            to_word += space_width;
        }
    }
    if has_word && to_word + spaces > limit {
        lines.push((line_start, last_word_end));
    } else {
        lines.push((line_start, text.len()));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use loom_text::FontWeight;

    fn plain() -> TextStyle {
        TextStyle {
            size_pt: 12.0,
            ..Default::default()
        }
    }

    #[test]
    fn every_wrapped_line_fits_its_width() {
        let text = "the quick brown fox jumps over the lazy dog ".repeat(6);
        let lines = wrap(&text, &[], &plain(), 200.0, 200.0);
        assert!(lines.len() > 3);
        for (start, end) in &lines {
            let width = line_width("", &text[*start..*end], &[], &plain(), *start);
            assert!(width <= 200.0 + 0.01, "{width} overflows 200 pt");
        }
        // Only the spaces at a break may be dropped; every letter is kept once.
        let joined: String = lines.iter().map(|(s, e)| &text[*s..*e]).collect();
        assert_eq!(
            joined.replace(' ', ""),
            text.replace(' ', ""),
            "wrapping keeps every letter once"
        );
    }

    #[test]
    fn a_long_word_is_broken_between_characters_inside_the_width() {
        let text = "x".repeat(80);
        let lines = wrap(&text, &[], &plain(), 100.0, 100.0);
        assert!(lines.len() > 1);
        for (start, end) in &lines {
            assert!(line_width("", &text[*start..*end], &[], &plain(), *start) <= 100.01);
        }
    }

    #[test]
    fn a_bold_run_is_measured_in_bold() {
        let text = "plain mmmmmmmmmm";
        let bold = [StyleRun {
            start: 6,
            end: text.len(),
            style: loom_text::CharacterStyle {
                weight: FontWeight::Bold,
                ..Default::default()
            },
        }];
        let regular = line_width("", text, &[], &plain(), 0);
        let mixed = line_width("", text, &bold, &plain(), 0);
        assert!(mixed > regular, "bold m is wider than regular m");
    }

    #[test]
    fn lines_break_at_the_same_bytes_as_the_editor_layout() {
        use crate::text_metrics::wrap_by_width;
        let texts = [
            "Loom Writer keeps your documents on your computer. Files are saved as packages \
             that any copy of the app can read, and nothing is uploaded unless you choose to.",
            "A short paragraph.",
            "Before xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx after.",
            "Leading and  double  spaces   inside, ending with a space ",
        ];
        let bold_runs = |text: &str| {
            let start = text.find(' ').unwrap_or(0) + 1;
            vec![StyleRun {
                start,
                end: text.len().min(start + 24),
                style: loom_text::CharacterStyle {
                    weight: FontWeight::Bold,
                    ..Default::default()
                },
            }]
        };
        let base = TextStyle {
            size_pt: 11.0,
            ..Default::default()
        };
        let trimmed = |text: &str, (start, end): (usize, usize)| {
            (start, start + text[start..end].trim_end().len())
        };
        for text in texts {
            for runs in [Vec::new(), bold_runs(text)] {
                for width in (60..=470).step_by(13) {
                    let width = width as f32;
                    let editor: Vec<_> = wrap_by_width(text, &runs, 11.0, width)
                        .into_iter()
                        .map(|range| trimmed(text, range))
                        .collect();
                    let exported: Vec<_> = wrap(text, &runs, &base, width, width)
                        .into_iter()
                        .map(|range| trimmed(text, range))
                        .collect();
                    assert_eq!(exported, editor, "{text:?} at {width} pt, runs {runs:?}");
                }
            }
        }
    }

    #[test]
    fn a_hard_break_ends_a_line_in_the_export_as_in_the_editor() {
        let text = "ab\ncd\n\nef\n";
        let lines = wrap(text, &[], &plain(), 500.0, 500.0);
        let words: Vec<&str> = lines.iter().map(|(s, e)| &text[*s..*e]).collect();
        assert_eq!(words, ["ab", "cd", "", "ef"]);
    }

    #[test]
    fn a_base_letter_and_its_mark_are_never_split_across_lines() {
        // 40 accented letters written as base + combining acute, in a column
        // far narrower than the run: every line must end on a whole grapheme.
        let text = "e\u{301}".repeat(40);
        let lines = wrap(&text, &[], &plain(), 30.0, 30.0);
        assert!(lines.len() > 1);
        for (start, end) in lines {
            assert!(text.is_char_boundary(start) && text.is_char_boundary(end));
            assert!(
                !text[start..].starts_with('\u{301}'),
                "a line starts on a mark"
            );
        }
    }

    #[test]
    fn empty_text_is_one_empty_line() {
        assert_eq!(wrap("", &[], &plain(), 100.0, 100.0), vec![(0, 0)]);
    }
}
