//! Generic Format Painter / style clipboard state machine for Writer, Sheets, and Present.

use serde::{Deserialize, Serialize};

/// Operating mode of the Format Painter tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormatPainterMode {
    /// Format Painter is off.
    #[default]
    Inactive,
    /// Applies the captured style once, then returns to `Inactive`.
    SingleShot,
    /// Applies the captured style repeatedly until explicitly cancelled.
    Locked,
}

/// Generic Format Painter state machine holding a cloned style snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StyleClipboard<T: Clone> {
    mode: FormatPainterMode,
    style: Option<T>,
}

impl<T: Clone> Default for StyleClipboard<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Clone> StyleClipboard<T> {
    /// Creates an inactive style clipboard.
    pub fn new() -> Self {
        Self {
            mode: FormatPainterMode::Inactive,
            style: None,
        }
    }

    /// Captures `style` for a single application.
    pub fn pick(&mut self, style: T) {
        self.mode = FormatPainterMode::SingleShot;
        self.style = Some(style);
    }

    /// Captures `style` for repeated applications until cancelled.
    pub fn pick_locked(&mut self, style: T) {
        self.mode = FormatPainterMode::Locked;
        self.style = Some(style);
    }

    /// Applies the captured style.
    ///
    /// In `SingleShot` mode, consumes the style and resets to `Inactive`.
    /// In `Locked` mode, returns a clone and stays `Locked`.
    /// In `Inactive` mode, returns `None`.
    pub fn apply_take(&mut self) -> Option<T> {
        match self.mode {
            FormatPainterMode::Inactive => None,
            FormatPainterMode::SingleShot => {
                self.mode = FormatPainterMode::Inactive;
                self.style.take()
            }
            FormatPainterMode::Locked => self.style.clone(),
        }
    }

    /// Inspects the captured style without changing state.
    pub fn peek(&self) -> Option<&T> {
        self.style.as_ref()
    }

    /// Clears the captured style and returns to `Inactive`.
    pub fn cancel(&mut self) {
        self.mode = FormatPainterMode::Inactive;
        self.style = None;
    }

    /// Returns the current Format Painter mode.
    pub fn mode(&self) -> FormatPainterMode {
        self.mode
    }

    /// Returns `true` when a style is armed (`SingleShot` or `Locked`).
    pub fn is_active(&self) -> bool {
        self.mode != FormatPainterMode::Inactive && self.style.is_some()
    }

    /// Returns `true` when in `Locked` mode.
    pub fn is_locked(&self) -> bool {
        self.mode == FormatPainterMode::Locked && self.style.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    struct DummyStyle {
        bold: bool,
        font_size: u16,
    }

    #[test]
    fn single_shot_consumes_on_apply() {
        let mut cb = StyleClipboard::new();
        assert!(!cb.is_active());
        assert!(!cb.is_locked());
        assert_eq!(cb.apply_take(), None);

        let style = DummyStyle {
            bold: true,
            font_size: 14,
        };
        cb.pick(style.clone());
        assert!(cb.is_active());
        assert!(!cb.is_locked());
        assert_eq!(cb.mode(), FormatPainterMode::SingleShot);
        assert_eq!(cb.peek(), Some(&style));

        assert_eq!(cb.apply_take(), Some(style));
        assert!(!cb.is_active());
        assert_eq!(cb.mode(), FormatPainterMode::Inactive);
        assert_eq!(cb.peek(), None);
        assert_eq!(cb.apply_take(), None);
    }

    #[test]
    fn locked_persists_until_cancelled() {
        let mut cb = StyleClipboard::default();
        let style = DummyStyle {
            bold: false,
            font_size: 11,
        };

        cb.pick_locked(style.clone());
        assert!(cb.is_active());
        assert!(cb.is_locked());
        assert_eq!(cb.mode(), FormatPainterMode::Locked);

        assert_eq!(cb.apply_take(), Some(style.clone()));
        assert_eq!(cb.apply_take(), Some(style.clone()));
        assert!(cb.is_active());
        assert!(cb.is_locked());

        cb.cancel();
        assert!(!cb.is_active());
        assert!(!cb.is_locked());
        assert_eq!(cb.mode(), FormatPainterMode::Inactive);
        assert_eq!(cb.apply_take(), None);
    }
}
