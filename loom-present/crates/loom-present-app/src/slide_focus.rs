//! Keyboard focus follows the slide. When another slide becomes the active one (it
//! was chosen, added, deleted or opened), the slide editor takes focus, so Tab
//! starts at that slide's first object and the next key does not reach a control
//! left behind by the change. Moving a slide keeps it active, so it is not a change.

use std::cell::RefCell;

thread_local! {
    /// The slide the window showed at the last refresh.
    static SHOWN: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Whether `active` differs from the slide shown at the previous refresh, and
/// remembers it. The first refresh of a window is never a change.
pub(crate) fn active_slide_changed(active: Option<&str>) -> bool {
    SHOWN.with(|shown| {
        let mut shown = shown.borrow_mut();
        let changed = shown
            .as_deref()
            .is_some_and(|previous| Some(previous) != active);
        *shown = active.map(str::to_owned);
        changed
    })
}

#[cfg(test)]
pub(crate) fn forget() {
    SHOWN.with(|shown| *shown.borrow_mut() = None);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_different_active_slide_counts_as_a_change() {
        forget();
        assert!(
            !active_slide_changed(Some("a")),
            "the first refresh is no change"
        );
        assert!(
            !active_slide_changed(Some("a")),
            "the same slide is no change"
        );
        assert!(active_slide_changed(Some("b")), "another slide is a change");
        assert!(!active_slide_changed(Some("b")), "and it is remembered");
    }
}
