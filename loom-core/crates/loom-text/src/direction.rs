/// Base direction requested for a paragraph.
///
/// `Auto` leaves the direction to first-strong text detection. Explicit values
/// let document formats retain a paragraph direction independently of its
/// current contents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ParagraphDirection {
    /// Use the first strong character, defaulting to left-to-right.
    #[default]
    Auto,
    /// Lay out the paragraph as left-to-right even when it starts with RTL text.
    LeftToRight,
    /// Lay out the paragraph as right-to-left even when it starts with Latin text.
    RightToLeft,
}
