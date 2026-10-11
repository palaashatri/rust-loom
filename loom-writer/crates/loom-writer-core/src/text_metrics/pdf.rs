//! Text as the PDF export draws it: the same shaping the editor measures
//! with, so a line that fits on the page fits in the file and breaks where it
//! breaks on screen.

use loom_pdf::{ClusterAdvance, RunShaper, TextStyle};

use super::faces::Style;
use super::measure::{advances, LineMeasure, Uncovered};

fn face_of(style: &TextStyle) -> Style {
    Style::new(style.bold, style.italic)
}

/// Width in points of one run drawn in `style`: Inter shaped with kerning, as
/// [`InterShaper`] spaces it in the file, and characters Inter lacks at the
/// advance the PDF writes them with.
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

/// Spaces every run of a PDF as [`pdf_text_width`] measures it. Characters
/// Inter does not have are left at the advance the PDF gives them.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct InterShaper;

impl RunShaper for InterShaper {
    fn clusters(&self, text: &str, bold: bool, italic: bool) -> Option<Vec<ClusterAdvance>> {
        let found = advances(text, 0, &[], Style::new(bold, italic), Uncovered::Drawn);
        let clusters = (0..found.starts.len())
            .map(|index| {
                let from = found.starts[index] as usize;
                let to = found.ends[index] as usize;
                ClusterAdvance {
                    chars: text[from..to].chars().count(),
                    advance: found.shaped[index].then(|| found.em[index] * 1000.0),
                }
            })
            .collect();
        Some(clusters)
    }
}
