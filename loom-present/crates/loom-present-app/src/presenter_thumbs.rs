//! What the presenter view draws for the current and next slide.
//!
//! The presenter window reuses the editor's slide canvas at thumbnail size, so
//! it needs the same per-element rows the editor feeds that canvas. Keeping the
//! conversion here means the presenter and the editor read a slide one way.

use loom_present_core::PresentationDocument;

use crate::ThumbRows;

/// Index of the slide after `active`, or `None` on the last slide.
pub(crate) fn next_index(active: usize, len: usize) -> Option<usize> {
    (active + 1 < len).then_some(active + 1)
}

/// Rows for the slide after the active one; empty at the end of the deck.
pub(crate) fn next_rows(document: &PresentationDocument) -> ThumbRows {
    crate::picture_view::rows_for(
        document,
        next_index(document.active_index, document.len()).and_then(|i| document.slides.get(i)),
    )
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
