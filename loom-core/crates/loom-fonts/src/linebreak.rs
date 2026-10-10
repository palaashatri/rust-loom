//! Line-break opportunities (UAX #14).
//!
//! Wrapping needs to know where a line may end, not only how wide the text
//! is. This is a thin, allocation-light wrapper over the `unicode-linebreak`
//! crate (Apache-2.0) so callers get byte offsets and no extra types.

use unicode_linebreak::{linebreaks, BreakOpportunity};

/// A place where a line may end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LineBreak {
    /// Byte offset of the break: the new line starts at `offset`. Always on a
    /// character boundary, and a mandatory break at `text.len()` always ends
    /// the list.
    pub offset: usize,
    /// True when a line must end here (a newline, or the end of the text);
    /// false when it may.
    pub mandatory: bool,
}

/// Every break opportunity in `text`, ascending by byte offset.
///
/// Spaces, hyphens, CJK ideographs and the other UAX #14 classes are
/// honoured; a no-break space or word joiner gives none. The offset at
/// `text.len()` is the mandatory end of text. Empty text has none.
pub fn line_break_opportunities(text: &str) -> Vec<LineBreak> {
    if text.is_empty() {
        return Vec::new();
    }
    linebreaks(text)
        .map(|(offset, kind)| LineBreak {
            offset,
            mandatory: kind == BreakOpportunity::Mandatory,
        })
        .collect()
}
