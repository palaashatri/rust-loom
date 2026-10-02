//! What the presenter view draws for the current and next slide.
//!
//! The presenter window reuses the editor's slide canvas at thumbnail size, so
//! it needs the same per-element rows the editor feeds that canvas. Keeping the
//! conversion here means the presenter and the editor read a slide one way.

use loom_present_core::{PresentationDocument, Slide};
use slint::{ModelRc, SharedString, VecModel};

use crate::ThumbRows;

/// Index of the slide after `active`, or `None` on the last slide.
pub(crate) fn next_index(active: usize, len: usize) -> Option<usize> {
    (active + 1 < len).then_some(active + 1)
}

/// Canvas rows for one slide. Selection is never shown on a thumbnail.
pub(crate) fn rows_for(slide: Option<&Slide>) -> ThumbRows {
    let elements = slide.map(|slide| slide.elements.as_slice()).unwrap_or(&[]);
    let model = |values: Vec<f32>| ModelRc::new(VecModel::from(values));
    ThumbRows {
        labels: ModelRc::new(VecModel::from(
            elements
                .iter()
                .map(|element| SharedString::from(element.content.as_str()))
                .collect::<Vec<_>>(),
        )),
        contents: ModelRc::new(VecModel::from(
            elements
                .iter()
                .map(|element| SharedString::from(element.content.as_str()))
                .collect::<Vec<_>>(),
        )),
        xs: model(elements.iter().map(|e| e.x).collect()),
        ys: model(elements.iter().map(|e| e.y).collect()),
        widths: model(elements.iter().map(|e| e.width).collect()),
        heights: model(elements.iter().map(|e| e.height).collect()),
        rotations: model(elements.iter().map(|e| e.rotation_deg).collect()),
        types: ModelRc::new(VecModel::from(
            elements
                .iter()
                .map(|e| crate::element_type_index(&e.element_type))
                .collect::<Vec<_>>(),
        )),
    }
}

/// Rows for the slide after the active one; empty at the end of the deck.
pub(crate) fn next_rows(document: &PresentationDocument) -> ThumbRows {
    rows_for(next_index(document.active_index, document.len()).and_then(|i| document.slides.get(i)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use slint::Model;

    #[test]
    fn next_index_stops_at_the_last_slide() {
        assert_eq!(next_index(0, 3), Some(1));
        assert_eq!(next_index(1, 3), Some(2));
        assert_eq!(next_index(2, 3), None);
        assert_eq!(next_index(0, 1), None);
        assert_eq!(next_index(0, 0), None);
    }

    #[test]
    fn rows_mirror_the_slide_elements() {
        let mut document = PresentationDocument::new("deck", "Deck");
        document.add_slide("Second", "content");
        let rows = next_rows(&document);
        let first = &document.slides[1];
        assert_eq!(rows.xs.row_count(), first.elements.len());
        assert_eq!(rows.contents.row_count(), first.elements.len());
        assert_eq!(rows.types.row_count(), first.elements.len());
        document.select_slide(1);
        assert_eq!(next_rows(&document).xs.row_count(), 0, "no next slide");
    }
}
