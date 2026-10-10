//! Text measurement matched to the bundled Inter face.
//!
//! Line breaking, caret placement, selection and comment rectangles and
//! pointer hit-testing must all agree with the glyphs the page actually draws,
//! and with the PDF export, which wraps with the same advances. Characters the
//! bundled Inter faces cover are measured by `loom_pdf::text_width_pt`, which
//! reads the advances from the font programs the PDF embeds, so the editor and
//! the exported file break lines at the same places. Weight is bold only where
//! a character run asks for it (weight 700 and up, as the page markup renders
//! it). Kerning is not applied.
//!
//! Characters Inter lacks (CJK, emoji) are drawn by whatever font the window
//! falls back to; their widths are estimates.

use loom_pdf::{is_measured_exactly, text_width_pt, TextStyle};
use loom_text::{FontWeight, StyleRun};
use unicode_segmentation::UnicodeSegmentation;

/// Whether a character weight draws in the bold face: the editor's page markup
/// renders weights from Bold (700) up in bold and the rest in regular, and the
/// PDF export follows the same rule so both break lines identically.
pub(crate) fn is_bold_weight(weight: FontWeight) -> bool {
    weight.numeric() >= FontWeight::Bold.numeric()
}

/// Advance of a character Inter lacks, in 1/1000 em: full-width scripts and
/// emoji take one em, letters and everything else a typical advance.
fn estimated_units(ch: char, bold: bool) -> f32 {
    match ch {
        c if (c as u32) >= 0x2E80 => 1000.0,
        c if c.is_alphabetic() => {
            if bold {
                610.0
            } else {
                590.0
            }
        }
        _ => {
            if bold {
                600.0
            } else {
                570.0
            }
        }
    }
}

fn is_bold_at(runs: &[StyleRun], position: usize) -> bool {
    runs.iter()
        .find(|run| run.start <= position && position < run.end)
        .is_some_and(|run| is_bold_weight(run.style.weight))
}

/// A grapheme's width in points. A grapheme Inter covers is measured as the
/// PDF export measures it (accent sequences compose into one glyph); any other
/// takes the estimate for its base character, since marks that follow add none.
fn grapheme_width(grapheme: &str, position: usize, runs: &[StyleRun], font_size: f32) -> f32 {
    let bold = is_bold_at(runs, position);
    if grapheme.chars().all(is_measured_exactly) {
        let style = TextStyle {
            size_pt: font_size,
            bold,
            ..TextStyle::default()
        };
        return text_width_pt(grapheme, &style);
    }
    grapheme
        .chars()
        .next()
        .map_or(0.0, |ch| estimated_units(ch, bold) * font_size / 1000.0)
}

/// Width in points of `text`, which starts at byte `text_start` of its block
/// (so character runs, expressed in block offsets, line up).
pub fn text_advance(text: &str, text_start: usize, runs: &[StyleRun], font_size: f32) -> f32 {
    text.grapheme_indices(true)
        .map(|(index, grapheme)| grapheme_width(grapheme, text_start + index, runs, font_size))
        .sum()
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
        let width = grapheme_width(grapheme, text_start + index, runs, font_size);
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
        let glyph = grapheme_width(grapheme, index, runs, font_size);
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
    fn widths_are_the_advances_the_pdf_embeds_and_scale_with_size() {
        let at = |text: &str, size: f32, bold: bool| {
            text_width_pt(
                text,
                &TextStyle {
                    size_pt: size,
                    bold,
                    ..TextStyle::default()
                },
            )
        };
        let hello = text_advance("Hello", 0, &[], 10.0);
        // Per-character sums of the PDF's own measurement.
        let by_hand: f32 = "Hello"
            .chars()
            .map(|c| at(&c.to_string(), 10.0, false))
            .sum();
        assert!((hello - by_hand).abs() < 1e-4, "{hello} vs {by_hand}");
        assert!((hello - at("Hello", 10.0, false)).abs() < 1e-4);
        assert!((text_advance("Hello", 0, &[], 20.0) - 2.0 * hello).abs() < 1e-3);
        assert_eq!(text_advance("", 0, &[], 12.0), 0.0);
        assert_eq!(
            text_advance(
                "a
b",
                0,
                &[],
                12.0
            ),
            text_advance("ab", 0, &[], 12.0)
        );
        // Inter Regular 'H' is 743/1000 em: the old hand table said 744.
        assert!((at("H", 1000.0, false) - 743.0).abs() < 1.0);
    }

    #[test]
    fn semibold_draws_as_regular_like_the_page_markup() {
        let semibold = StyleRun {
            start: 0,
            end: 3,
            style: CharacterStyle {
                weight: FontWeight::Semibold,
                ..Default::default()
            },
        };
        assert_eq!(
            text_advance("one", 0, &[semibold], 12.0),
            text_advance("one", 0, &[], 12.0)
        );
        assert!(is_bold_weight(FontWeight::Bold));
        assert!(is_bold_weight(FontWeight::Black));
        assert!(!is_bold_weight(FontWeight::Semibold));
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
