//! Text measurement matched to the faces the page draws.
//!
//! Line breaking, caret placement, selection and comment rectangles and
//! pointer hit-testing must all agree with the glyphs the page actually draws,
//! and with the PDF export, which wraps with the same advances. The page draws
//! each run in its family (Inter unless the run names another), shaped with
//! kerning, so text is shaped here the same way through `loom-fonts`
//! ([`faces`] loads the faces once and remembers each distinct word's
//! advances; [`measure`] lays pieces end to end). Weight is bold only where a
//! character run asks for it (weight 700 and up, as the page markup renders
//! it) and italic follows the run. A run's font size is not drawn per run, so
//! it is not measured per run.
//!
//! Characters the run's face lacks (CJK in Inter, emoji) are drawn by whatever
//! font the window falls back to; their widths are estimates. The PDF export,
//! which draws every run in Inter, measures them at the advance it writes them
//! with (see [`pdf`]).

mod faces;
mod measure;
mod pdf;

#[cfg(test)]
mod family_tests;
#[cfg(test)]
mod kerning_tests;

use loom_text::{FontWeight, StyleRun};

pub(crate) use faces::Style;
pub(crate) use measure::{LineMeasure, Uncovered};
pub(crate) use pdf::{drawn_size, pdf_text_width, InterShaper};

/// Whether a character weight draws in the bold face: the editor's page markup
/// renders weights from Bold (700) up in bold and the rest in regular, and the
/// PDF export follows the same rule so both break lines identically.
pub(crate) fn is_bold_weight(weight: FontWeight) -> bool {
    weight.numeric() >= FontWeight::Bold.numeric()
}

/// The measure of a whole block's text as the editor draws it.
pub(crate) fn block_measure(text: &str, runs: &[StyleRun], font_size: f32) -> LineMeasure {
    LineMeasure::new(
        text,
        0,
        runs,
        font_size,
        Style::default(),
        Uncovered::Estimate,
    )
}

/// Width in points of `text`, which starts at byte `text_start` of its block
/// (so character runs, expressed in block offsets, line up). The text is
/// shaped on its own: kerning against a neighbour outside it is not applied.
pub fn text_advance(text: &str, text_start: usize, runs: &[StyleRun], font_size: f32) -> f32 {
    LineMeasure::new(
        text,
        text_start,
        runs,
        font_size,
        Style::default(),
        Uncovered::Estimate,
    )
    .width()
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
    let visible = text.trim_end_matches('\n').len();
    LineMeasure::new(
        text,
        text_start,
        runs,
        font_size,
        Style::default(),
        Uncovered::Estimate,
    )
    .offset_at_x(x, visible)
}

/// Split a block's text into the byte ranges of its lines so that each line
/// fits `max_width` points, breaking after whitespace where possible and
/// inside a word only when one word is wider than a line. A hard line break
/// ends its line and is part of it. Spaces may hang past the edge, as in any
/// editor. (The layout wraps through its cached [`LineMeasure`]; this is the
/// same computation from scratch, which the tests compare against.)
#[cfg(test)]
pub(crate) fn wrap_by_width(
    text: &str,
    runs: &[StyleRun],
    font_size: f32,
    max_width: f32,
) -> Vec<(usize, usize)> {
    block_measure(text, runs, font_size).wrap(text, max_width)
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

    /// The width of `text` shaped in one piece straight through `loom-fonts`,
    /// without any of this module's word splitting or caching.
    fn shaped_in_one_piece(text: &str, size: f32, bold: bool, italic: bool) -> f32 {
        use loom_fonts::FontCatalog;
        let catalog = FontCatalog::bundled_only();
        let font = catalog
            .resolve("Inter", if bold { 700 } else { 400 }, italic)
            .expect("Inter");
        catalog
            .load(font.primary())
            .expect("face")
            .text_width(text, size)
    }

    #[test]
    fn widths_are_the_shaped_advances_of_the_embedded_face_and_scale_with_size() {
        let hello = text_advance("Hello", 0, &[], 10.0);
        assert!(
            (hello - shaped_in_one_piece("Hello", 10.0, false, false)).abs() < 1e-3,
            "{hello}"
        );
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
        assert!((text_advance("H", 0, &[], 1000.0) - 743.0).abs() < 1.0);
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
