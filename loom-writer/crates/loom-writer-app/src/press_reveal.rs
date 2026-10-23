//! Keeps a press from scrolling the page under the pointer.
//!
//! A selection change normally scrolls the caret into view. A press must not
//! do that, because the page would move before the press lands on its cell.
//! Drag moves still reveal, so a held drag can scroll toward its focus.

use std::cell::Cell;

thread_local! {
    static SUPPRESSED: Cell<bool> = const { Cell::new(false) };
}

/// Restores the suppression state that was in force before a press, even when
/// the press unwinds.
struct Restore(bool);

impl Drop for Restore {
    fn drop(&mut self) {
        SUPPRESSED.with(|flag| flag.set(self.0));
    }
}

/// Applies a press's selection change with the caret reveal suppressed.
pub(crate) fn during_press<T>(apply: impl FnOnce() -> T) -> T {
    let _restore = Restore(SUPPRESSED.with(|flag| flag.replace(true)));
    apply()
}

/// `true` while a press's selection change is being applied.
pub(crate) fn is_suppressed() -> bool {
    SUPPRESSED.with(Cell::get)
}
