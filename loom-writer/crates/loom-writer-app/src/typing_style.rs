//! Formatting for text typed at a collapsed caret.
//!
//! Ctrl+B, Ctrl+I and Ctrl+U with no selection set the style that the next typed
//! text takes. The setting belongs to one caret position: moving the caret, or an
//! edit made elsewhere, drops it.

use std::cell::RefCell;

/// The character style that typed text takes at one caret. `None` keeps the
/// style the text already has.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PendingStyle {
    /// UTF-8 byte offset of the caret the style was set at.
    pub(crate) caret: usize,
    pub(crate) bold: Option<bool>,
    pub(crate) italic: Option<bool>,
    pub(crate) underline: Option<bool>,
}

/// The character styles that Ctrl+B, Ctrl+I and Ctrl+U toggle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InlineStyle {
    Bold,
    Italic,
    Underline,
}

impl InlineStyle {
    fn name(self) -> &'static str {
        match self {
            Self::Bold => "Bold",
            Self::Italic => "Italic",
            Self::Underline => "Underline",
        }
    }
}

std::thread_local! {
    static PENDING: RefCell<Option<PendingStyle>> = const { RefCell::new(None) };
}

/// The pending style for `caret`. Nothing set there gives an empty style.
pub(crate) fn at(caret: usize) -> PendingStyle {
    PENDING.with(|pending| match *pending.borrow() {
        Some(style) if style.caret == caret => style,
        _ => PendingStyle {
            caret,
            ..PendingStyle::default()
        },
    })
}

/// Store the pending style. It applies to text typed at its caret.
pub(crate) fn store(style: PendingStyle) {
    PENDING.with(|pending| *pending.borrow_mut() = Some(style));
}

/// Forget the pending style, whatever its caret. Tests start from a clean slate.
#[cfg(test)]
pub(crate) fn clear() {
    PENDING.with(|pending| *pending.borrow_mut() = None);
}

/// Forget the pending style unless it belongs to `caret`.
pub(crate) fn retain_at(caret: usize) {
    PENDING.with(|pending| {
        let mut pending = pending.borrow_mut();
        if pending.is_some_and(|style| style.caret != caret) {
            *pending = None;
        }
    });
}

/// Toggle one style for the text typed next at `caret`. `enabled_at_caret` is
/// the style of the text under the caret, used until the user has toggled it.
/// Returns whether the style is now on, for the status line.
pub(crate) fn toggle(caret: usize, style: InlineStyle, enabled_at_caret: bool) -> bool {
    let mut pending = at(caret);
    let slot = match style {
        InlineStyle::Bold => &mut pending.bold,
        InlineStyle::Italic => &mut pending.italic,
        InlineStyle::Underline => &mut pending.underline,
    };
    let now_on = !slot.unwrap_or(enabled_at_caret);
    *slot = Some(now_on);
    store(pending);
    now_on
}

/// The status line text after a toggle at a collapsed caret.
pub(crate) fn announcement(style: InlineStyle, now_on: bool) -> String {
    format!(
        "{} {} for new text",
        style.name(),
        if now_on { "on" } else { "off" }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_toggle_starts_from_the_style_at_the_caret_and_flips_each_time() {
        clear();
        assert!(toggle(3, InlineStyle::Italic, false));
        assert!(!toggle(3, InlineStyle::Italic, false));
        assert!(at(3).italic == Some(false));
        assert!(at(3).bold.is_none(), "other styles are untouched");
    }

    #[test]
    fn a_pending_style_belongs_to_its_caret_only() {
        clear();
        toggle(4, InlineStyle::Bold, false);
        retain_at(4);
        assert_eq!(at(4).bold, Some(true), "the same caret keeps it");
        retain_at(5);
        assert_eq!(at(4).bold, None, "moving the caret drops it");
    }
}
