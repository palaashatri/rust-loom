//! Positioned fragments that belong to one logical visual line.

use std::fmt::Write as _;

use crate::{clip01, finite, fmt3, PageIndex, PdfDocument, TextStyle};

/// A styled text fragment at an absolute baseline position.
///
/// Fragments passed to [`PdfDocument::draw_text_line`] share one PDF text
/// object, so extractors keep adjacent styled pieces on the same logical line.
#[derive(Debug, Clone, Copy)]
pub struct PositionedTextRun<'a> {
    /// Baseline x position in PDF points.
    pub x: f32,
    /// Baseline y position in PDF points.
    pub y: f32,
    /// Logical UTF-8 text for this fragment.
    pub text: &'a str,
    /// The fragment's font size, face, and fill color.
    pub style: &'a TextStyle,
}

impl PdfDocument {
    /// Draw positioned styled fragments as one logical line.
    ///
    /// Each fragment is encoded through the same font fallback, ActualText,
    /// and optional run-shaping path as [`Self::draw_text`]. Absolute text
    /// matrices let the caller distribute spaces or apply mixed styles while
    /// keeping a single `BT`/`ET` pair for extraction.
    pub fn draw_text_line(&mut self, page: PageIndex, runs: &[PositionedTextRun<'_>]) {
        let mut operators = String::new();
        for run in runs {
            let Some((font, shown)) = self.show_text(run.text, run.style) else {
                continue;
            };
            let (r, g, b) = (
                clip01(run.style.fill_rgb.0),
                clip01(run.style.fill_rgb.1),
                clip01(run.style.fill_rgb.2),
            );
            let _ = write!(
                operators,
                "{} {} {} rg {font} 1 0 0 1 {:.2} {:.2} Tm {shown} ",
                fmt3(r),
                fmt3(g),
                fmt3(b),
                finite(run.x),
                finite(run.y),
            );
        }
        if operators.is_empty() {
            return;
        }

        let mut text_object = String::with_capacity(operators.len() + 6);
        text_object.push_str("BT ");
        text_object.push_str(&operators);
        text_object.push_str("ET");
        self.page_mut(page).ops.push(text_object);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{inspect, ClusterAdvance, RunShaper};

    #[derive(Debug)]
    struct Narrow;

    impl RunShaper for Narrow {
        fn clusters(&self, text: &str, _bold: bool, _italic: bool) -> Option<Vec<ClusterAdvance>> {
            Some(
                text.chars()
                    .map(|_| ClusterAdvance {
                        chars: 1,
                        advance: Some(300.0),
                    })
                    .collect(),
            )
        }
    }

    #[test]
    fn positioned_fragments_extract_as_one_shaped_logical_line() {
        let regular = TextStyle {
            fill_rgb: (0.1, 0.2, 0.3),
            ..Default::default()
        };
        let bold = TextStyle {
            bold: true,
            fill_rgb: (0.8, 0.1, 0.2),
            ..Default::default()
        };
        let runs = [
            PositionedTextRun {
                x: 10.0,
                y: 100.0,
                text: "alpha ",
                style: &regular,
            },
            PositionedTextRun {
                x: 40.0,
                y: 100.0,
                text: "beta",
                style: &bold,
            },
        ];
        let mut pdf = PdfDocument::new();
        pdf.set_run_shaper(Arc::new(Narrow));
        let page = pdf.add_page(200.0, 120.0);
        pdf.draw_text_line(page, &runs);
        let bytes = pdf.serialize();

        assert_eq!(
            inspect::page_text(&bytes).expect("extract text"),
            ["alpha beta"]
        );
        assert!(
            inspect::spacing_adjustments(&bytes)
                .expect("shaped adjustments")
                .len()
                >= 2
        );
        let content = inspect::readable_content(&bytes).expect("read content");
        assert_eq!(content.matches("BT").count(), 1, "{content}");
        assert!(content.contains("0.100 0.200 0.300 rg"), "{content}");
        assert!(content.contains("0.800 0.100 0.200 rg"), "{content}");
        inspect::verify(&bytes).expect("valid positioned text line");
    }
}
