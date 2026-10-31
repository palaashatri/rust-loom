//! Formatting for text typed at a collapsed caret.
//!
//! Ctrl+B, Ctrl+I and Ctrl+U with no selection, and a font family chosen from
//! the font list, set the style that the next typed text takes. The setting
//! belongs to one caret position: moving the caret, or an edit made elsewhere,
//! drops it.

use std::cell::RefCell;

/// The character style that typed text takes at one caret. `None` keeps the
/// style the text already has.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PendingStyle {
    /// UTF-8 byte offset of the caret the style was set at.
    pub(crate) caret: usize,
    pub(crate) bold: Option<bool>,
    pub(crate) italic: Option<bool>,
    pub(crate) underline: Option<bool>,
    pub(crate) strikethrough: Option<bool>,
    pub(crate) superscript: Option<bool>,
    pub(crate) subscript: Option<bool>,
    pub(crate) color: Option<String>,
    /// The font family as the document stores it (empty: the document font).
    pub(crate) family: Option<String>,
}

impl PendingStyle {
    /// True when nothing is pending.
    pub(crate) fn is_empty(&self) -> bool {
        self.bold.is_none()
            && self.italic.is_none()
            && self.underline.is_none()
            && self.strikethrough.is_none()
            && self.superscript.is_none()
            && self.subscript.is_none()
            && self.color.is_none()
            && self.family.is_none()
    }
}

/// Complete pending typing-style state for a document edit transaction.
pub(crate) type Snapshot = Option<PendingStyle>;

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
    PENDING.with(|pending| match &*pending.borrow() {
        Some(style) if style.caret == caret => style.clone(),
        _ => PendingStyle {
            caret,
            ..PendingStyle::default()
        },
    })
}

/// Make the text typed next at `caret` take the font family `family`.
pub(crate) fn set_family(caret: usize, family: &str) {
    let mut pending = at(caret);
    pending.family = Some(family.to_owned());
    store(pending);
}

pub(crate) fn set_foreground_color(caret: usize, color: &str) {
    let mut pending = at(caret);
    pending.color = Some(color.to_owned());
    store(pending);
}

pub(crate) fn toggle_superscript(
    caret: usize,
    enabled_at_caret: bool,
    subscript_at_caret: bool,
) -> PendingStyle {
    let mut pending = at(caret);
    let enabled = !pending.superscript.unwrap_or(enabled_at_caret);
    pending.superscript = Some(enabled);
    if enabled {
        pending.subscript = Some(false);
    } else if pending.subscript.is_none() && subscript_at_caret {
        pending.subscript = Some(true);
    }
    store(pending.clone());
    pending
}

pub(crate) fn toggle_subscript(
    caret: usize,
    enabled_at_caret: bool,
    superscript_at_caret: bool,
) -> PendingStyle {
    let mut pending = at(caret);
    let enabled = !pending.subscript.unwrap_or(enabled_at_caret);
    pending.subscript = Some(enabled);
    if enabled {
        pending.superscript = Some(false);
    } else if pending.superscript.is_none() && superscript_at_caret {
        pending.superscript = Some(true);
    }
    store(pending.clone());
    pending
}

/// Store the pending style. It applies to text typed at its caret.
pub(crate) fn store(style: PendingStyle) {
    PENDING.with(|pending| *pending.borrow_mut() = Some(style));
}

/// Capture pending typing style for a document edit transaction.
pub(crate) fn snapshot() -> Snapshot {
    PENDING.with(|pending| pending.borrow().clone())
}

/// Restore pending typing style after a rejected document transaction.
pub(crate) fn restore(snapshot: Snapshot) {
    PENDING.with(|pending| *pending.borrow_mut() = snapshot);
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
        if pending.as_ref().is_some_and(|style| style.caret != caret) {
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
