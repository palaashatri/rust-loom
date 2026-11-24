#![allow(dead_code)]

//! Format Painter state machine backed by `loom_clipboard::StyleClipboard`.

use loom_clipboard::{FormatPainterMode, StyleClipboard};
use loom_text::{Alignment, CharacterStyle, ParagraphDirection};

/// Captured character + paragraph formatting snapshot for Format Painter.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CapturedWriterStyle {
    pub character: CharacterStyle,
    pub alignment: Option<Alignment>,
    pub direction: ParagraphDirection,
}

/// Manages single-shot (`Cmd/Ctrl+Alt+C`) and locked Format Painter state.
#[derive(Debug, Clone, Default)]
pub(crate) struct WriterFormatPainter {
    clipboard: StyleClipboard<CapturedWriterStyle>,
}

impl WriterFormatPainter {
    /// Captures `style` for a single application (`SingleShot`).
    pub(crate) fn copy_style(&mut self, style: CapturedWriterStyle) {
        self.clipboard.pick(style);
    }

    /// Captures `style` for repeated applications (`Locked`).
    pub(crate) fn copy_style_locked(&mut self, style: CapturedWriterStyle) {
        self.clipboard.pick_locked(style);
    }

    /// Consumes (if `SingleShot`) or clones (if `Locked`) the captured style.
    pub(crate) fn apply_style(&mut self) -> Option<CapturedWriterStyle> {
        self.clipboard.apply_take()
    }

    /// Cancels Format Painter and returns to `Inactive`.
    pub(crate) fn cancel(&mut self) {
        self.clipboard.cancel();
    }

    /// Returns `true` when Format Painter is armed (`SingleShot` or `Locked`).
    pub(crate) fn is_active(&self) -> bool {
        self.clipboard.is_active()
    }

    /// Returns `true` when Format Painter is in `Locked` mode.
    pub(crate) fn is_locked(&self) -> bool {
        self.clipboard.is_locked()
    }

    /// Returns the current mode.
    pub(crate) fn mode(&self) -> FormatPainterMode {
        self.clipboard.mode()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_style() -> CapturedWriterStyle {
        let character = CharacterStyle {
            italic: true,
            ..CharacterStyle::default()
        };
        CapturedWriterStyle {
            character,
            alignment: Some(Alignment::Center),
            direction: ParagraphDirection::default(),
        }
    }

    #[test]
    fn single_shot_and_locked_lifecycle() {
        let mut fp = WriterFormatPainter::default();
        assert!(!fp.is_active());

        fp.copy_style(sample_style());
        assert!(fp.is_active());
        assert!(!fp.is_locked());
        assert_eq!(fp.mode(), FormatPainterMode::SingleShot);
        assert!(fp.apply_style().is_some());
        assert!(!fp.is_active());
        assert!(fp.apply_style().is_none());

        fp.copy_style_locked(sample_style());
        assert!(fp.is_locked());
        assert!(fp.apply_style().is_some());
        assert!(fp.apply_style().is_some());
        fp.cancel();
        assert!(!fp.is_active());
    }
}
