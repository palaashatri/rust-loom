//! Text measurement matched to the bundled Inter face.
//!
//! Line breaking, caret placement, selection and comment rectangles and
//! pointer hit-testing must all agree with the glyphs the page actually draws.
//! They used to assume every glyph is `0.52 em` wide, which is wider than Inter
//! body text and narrower than its headings, so highlights drifted away from
//! the text and clicks landed several characters off on long lines.
//!
//! Advance widths below were read from `Inter-Regular.ttf` and `Inter-Bold.ttf`
//! (the faces `loom-ui` bundles) in 1/1000 em. Weight is bold only where a
//! character run asks for it, exactly as the page markup renders it. Kerning is
//! not applied.

use loom_text::{FontWeight, StyleRun};
use unicode_segmentation::UnicodeSegmentation;

/// Inter Regular advance widths for ASCII 32..=126.
const INTER_REGULAR: [u16; 95] = [
    281, 288, 466, 633, 642, 982, 644, 300, 365, 365, 501, 662, 288, 460, 288, 360, 631, 407, 610,
    618, 646, 608, 620, 566, 619, 620, 288, 302, 662, 662, 662, 511, 966, 690, 654, 730, 722, 601,
    590, 746, 743, 269, 571, 672, 565, 903, 753, 765, 639, 765, 644, 642, 646, 744, 690, 985, 682,
    679, 629, 365, 360, 365, 471, 456, 323, 562, 612, 571, 612, 583, 370, 613, 591, 242, 242, 549,
    242, 876, 591, 600, 612, 612, 376, 528, 327, 591, 562, 818, 546, 562, 552, 426, 333, 426, 662,
];

/// Inter Bold advance widths for ASCII 32..=126.
const INTER_BOLD: [u16; 95] = [
    237, 338, 552, 649, 655, 1016, 672, 339, 377, 377, 559, 679, 334, 468, 334, 388, 674, 431, 630,
    646, 676, 639, 649, 582, 651, 649, 334, 343, 679, 679, 679, 560, 1016, 747, 662, 740, 722, 607,
    587, 750, 747, 281, 584, 719, 565, 932, 762, 771, 648, 777, 657, 655, 667, 732, 747, 1038, 738,
    731, 664, 377, 388, 377, 487, 476, 365, 581, 630, 588, 630, 596, 398, 632, 623, 271, 271, 580,
    271, 913, 623, 613, 630, 630, 407, 560, 366, 623, 600, 850, 580, 602, 573, 469, 372, 469, 679,
];

/// Advance of one character in 1/1000 em.
fn glyph_units(ch: char, bold: bool) -> u32 {
    let table = if bold { &INTER_BOLD } else { &INTER_REGULAR };
    match ch {
        ' '..='~' => u32::from(table[ch as usize - 32]),
        '\n' | '\r' | '\u{200B}'..='\u{200D}' | '\u{0300}'..='\u{036F}' => 0,
        '\u{2013}' => 600,
        '\u{2014}' | '\u{2026}' => 1000,
        '\u{2018}' | '\u{2019}' => 300,
        '\u{201C}' | '\u{201D}' => 480,
        '\u{2022}' => 480,
        // CJK and other full-width scripts.
        c if (c as u32) >= 0x2E80 => 1000,
        // Accented Latin and other letters: a typical lower-case advance.
        c if c.is_alphabetic() => {
            if bold {
                610
            } else {
                590
            }
        }
        _ => {
            if bold {
                600
            } else {
                570
            }
        }
    }
}

fn is_bold_at(runs: &[StyleRun], position: usize) -> bool {
    runs.iter()
        .find(|run| run.start <= position && position < run.end)
        .is_some_and(|run| matches!(run.style.weight, FontWeight::Bold | FontWeight::Black))
}

/// A grapheme's width: its base character, since marks that follow add none.
fn grapheme_units(grapheme: &str, position: usize, runs: &[StyleRun]) -> u32 {
    grapheme
        .chars()
        .next()
        .map_or(0, |ch| glyph_units(ch, is_bold_at(runs, position)))
}

/// Width in points of `text`, which starts at byte `text_start` of its block
/// (so character runs, expressed in block offsets, line up).
pub fn text_advance(text: &str, text_start: usize, runs: &[StyleRun], font_size: f32) -> f32 {
    let units: u32 = text
        .grapheme_indices(true)
        .map(|(index, grapheme)| grapheme_units(grapheme, text_start + index, runs))
        .sum();
    units as f32 * font_size / 1000.0
}

/// The byte offset in `text` nearest to horizontal position `x` (points from
/// the line's left edge). A trailing hard line break is never a valid caret
/// position for a click past the end of the line.
pub fn offset_at_x(
    text: &str,
    text_start: usize,
    runs: &[StyleRun],
    font_size: f32,
    x: f32,
) -> usize {
    let visible = text.trim_end_matches('\n');
    let mut left = 0.0_f32;
    for (index, grapheme) in visible.grapheme_indices(true) {
        let width = grapheme_units(grapheme, text_start + index, runs) as f32 * font_size / 1000.0;
        if x < left + width / 2.0 {
            return index;
        }
        left += width;
    }
    visible.len()
}

/// Split a block's text into the byte ranges of its lines so that each line
/// fits `max_width` points, breaking after whitespace where possible and inside
/// a word only when one word is wider than a line. A hard line break ends its
/// line and is part of it. Spaces may hang past the edge, as in any editor.
pub(crate) fn wrap_by_width(
    text: &str,
    runs: &[StyleRun],
    font_size: f32,
    max_width: f32,
) -> Vec<(usize, usize)> {
    if text.is_empty() {
        return vec![(0, 0)];
    }
    let em = font_size / 1000.0;
    let mut ranges = Vec::new();
    let mut line_start = 0usize;
    let mut width = 0.0_f32;
    let mut last_break: Option<usize> = None;
    for (index, grapheme) in text.grapheme_indices(true) {
        if grapheme == "\n" {
            ranges.push((line_start, index + 1));
            line_start = index + 1;
            width = 0.0;
            last_break = None;
            continue;
        }
        let glyph = grapheme_units(grapheme, index, runs) as f32 * em;
        let is_space = grapheme.chars().any(char::is_whitespace);
        if !is_space && index > line_start && width + glyph > max_width {
            let end = last_break
                .filter(|break_at| *break_at > line_start)
                .unwrap_or(index);
            ranges.push((line_start, end));
            line_start = end;
            width = text_advance(&text[line_start..index], line_start, runs, font_size);
            last_break = None;
        }
        width += glyph;
        if is_space {
            last_break = Some(index + grapheme.len());
        }
    }
    if line_start < text.len() {
        ranges.push((line_start, text.len()));
    }
    ranges
}

#[cfg(test)]
mod tests {
    use super::*;
    use loom_text::CharacterStyle;

    fn bold_run(start: usize, end: usize) -> StyleRun {
        StyleRun {
            start,
            end,
            style: CharacterStyle {
                weight: FontWeight::Bold,
                ..Default::default()
            },
        }
    }

    #[test]
    fn widths_follow_inter_and_scale_with_size() {
        // H 744 + e 583... read from the table, not recomputed here.
        let hello = text_advance("Hello", 0, &[], 10.0);
        let by_hand = (INTER_REGULAR[usize::from(b'H') - 32]
            + INTER_REGULAR[usize::from(b'e') - 32]
            + 2 * INTER_REGULAR[usize::from(b'l') - 32]
            + INTER_REGULAR[usize::from(b'o') - 32]) as f32
            / 100.0;
        assert!((hello - by_hand).abs() < 1e-3);
        assert!((text_advance("Hello", 0, &[], 20.0) - 2.0 * hello).abs() < 1e-3);
        assert_eq!(text_advance("", 0, &[], 12.0), 0.0);
        assert_eq!(
            text_advance("a\nb", 0, &[], 12.0),
            text_advance("ab", 0, &[], 12.0)
        );
    }

    #[test]
    fn bold_runs_are_wider_and_only_where_they_apply() {
        let plain = text_advance("one two three", 0, &[], 12.0);
        let bolded = text_advance("one two three", 0, &[bold_run(4, 7)], 12.0);
        assert!(bolded > plain, "a bold word is wider");
        let only_first = text_advance("one ", 0, &[bold_run(4, 7)], 12.0);
        assert!((only_first - text_advance("one ", 0, &[], 12.0)).abs() < 1e-4);
        // A slice starting mid-block still finds the run by block offset.
        let slice = text_advance("two", 4, &[bold_run(4, 7)], 12.0);
        assert!(slice > text_advance("two", 4, &[], 12.0));
    }

    #[test]
    fn combining_marks_add_no_width() {
        let composed = text_advance("e\u{301}", 0, &[], 12.0);
        let plain = text_advance("e", 0, &[], 12.0);
        assert!((composed - plain).abs() < 1e-4);
    }

    #[test]
    fn every_wrapped_line_fits_and_the_lines_cover_the_text() {
        let text = "Write, format, and export documents. Your files stay on your computer as open loomdoc packages, and nothing is uploaded.";
        let max = 200.0;
        let ranges = wrap_by_width(text, &[], 11.0, max);
        assert!(ranges.len() > 1);
        let mut expected_start = 0;
        for (start, end) in &ranges {
            assert_eq!(*start, expected_start, "no gaps or overlaps");
            expected_start = *end;
            let visible = text[*start..*end].trim_end();
            assert!(
                text_advance(visible, *start, &[], 11.0) <= max + 0.01,
                "line too wide: {visible:?}"
            );
        }
        assert_eq!(expected_start, text.len());
    }

    #[test]
    fn long_words_break_inside_and_make_progress() {
        let text = "supercalifragilisticexpialidocious";
        let ranges = wrap_by_width(text, &[], 12.0, 40.0);
        assert!(ranges.len() > 2);
        assert!(ranges.iter().all(|(start, end)| end > start));
        assert_eq!(ranges.last().unwrap().1, text.len());
        // A line narrower than one glyph still advances one grapheme at a time.
        let tiny = wrap_by_width("abc", &[], 12.0, 0.1);
        assert_eq!(tiny, vec![(0, 1), (1, 2), (2, 3)]);
    }

    #[test]
    fn hard_line_breaks_close_their_line_and_empty_text_is_one_empty_line() {
        assert_eq!(wrap_by_width("", &[], 12.0, 100.0), vec![(0, 0)]);
        assert_eq!(
            wrap_by_width("ab\ncd", &[], 12.0, 500.0),
            vec![(0, 3), (3, 5)]
        );
    }

    #[test]
    fn bold_text_wraps_earlier_than_plain_text() {
        let text = "aaa bbb ccc ddd eee fff ggg hhh";
        let plain = wrap_by_width(text, &[], 12.0, 120.0).len();
        let bold = wrap_by_width(text, &[bold_run(0, text.len())], 12.0, 120.0).len();
        assert!(bold >= plain);
    }

    #[test]
    fn a_click_lands_on_the_nearest_character_boundary() {
        let text = "Hello world";
        let size = 12.0;
        let after_hello = text_advance("Hello", 0, &[], size);
        assert_eq!(offset_at_x(text, 0, &[], size, 0.0), 0);
        assert_eq!(offset_at_x(text, 0, &[], size, after_hello + 0.5), 5);
        assert_eq!(offset_at_x(text, 0, &[], size, after_hello - 0.5), 5);
        assert_eq!(offset_at_x(text, 0, &[], size, 10_000.0), text.len());
        // Past the end of a line that ends in a hard break stays before the break.
        assert_eq!(offset_at_x("ab\n", 0, &[], size, 10_000.0), 2);
        // A boundary measured by `text_advance` is where the click maps back to.
        for boundary in [1, 4, 7, 10] {
            let x = text_advance(&text[..boundary], 0, &[], size);
            assert_eq!(offset_at_x(text, 0, &[], size, x), boundary);
        }
    }
}
