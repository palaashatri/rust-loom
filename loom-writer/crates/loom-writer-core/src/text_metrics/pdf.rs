//! Text as the PDF export draws it: the same shaping the editor measures
//! with, so a line that fits on the page fits in the file and breaks where it
//! breaks on screen.

use loom_pdf::TextStyle;

use super::faces::Style;
use super::measure::{LineMeasure, Uncovered};

fn face_of(style: &TextStyle) -> Style {
    Style::new(style.bold, style.italic)
}

/// Width in points of one run drawn in `style`, including the PDF fallback
/// advance for characters outside the measured face.
pub(crate) fn pdf_text_width(text: &str, style: &TextStyle) -> f32 {
    LineMeasure::new(
        text,
        0,
        &[],
        drawn_size(style),
        face_of(style),
        Uncovered::Drawn,
    )
    .width()
}

/// The size the PDF writer sets text at: an unusable size draws at the
/// default.
pub(crate) fn drawn_size(style: &TextStyle) -> f32 {
    if style.size_pt.is_finite() && style.size_pt > 0.0 {
        style.size_pt
    } else {
        TextStyle::default().size_pt
    }
}
