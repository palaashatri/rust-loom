//! Reordering slides: one undoable step per move, selection follows the slide.

use crate::PresentationSession;

impl PresentationSession {
    /// Whether the active slide has a slide above it to swap with.
    pub fn can_move_active_slide_up(&self) -> bool {
        self.document.active_index > 0 && self.document.slides.len() > 1
    }

    /// Whether the active slide has a slide below it to swap with.
    pub fn can_move_active_slide_down(&self) -> bool {
        self.document.active_index + 1 < self.document.slides.len()
    }

    /// Moves the active slide one place earlier (`-1`) or later (`1`) as one
    /// undoable edit. Returns the `(from, to)` positions, or `None` (and no
    /// history entry) when the slide is already at that end of the deck.
    pub fn move_active_slide(&mut self, delta: isize) -> Option<(usize, usize)> {
        let from = self.document.active_index;
        let to = from.checked_add_signed(delta)?;
        self.move_slide(from, to).then_some((from, to))
    }
}
