//! Work counters for the caret index.
//!
//! Wall-clock time is a poor regression signal on shared machines, so the
//! tests assert on counted work instead: how many steps building a line's
//! caret stops took and how many binary-search steps a query took. The
//! counters are per thread, cost a thread-local add, and are not part of the
//! supported API.

use std::cell::Cell;

thread_local! {
    static STEPS: Cell<u64> = const { Cell::new(0) };
    static BUILDS: Cell<u64> = const { Cell::new(0) };
}

/// Adds `n` units of caret work (one per glyph group, grapheme, comparison or
/// binary-search step).
pub(crate) fn add_steps(n: u64) {
    STEPS.with(|steps| steps.set(steps.get() + n));
}

/// Records one construction of a caret-stop table.
pub(crate) fn add_build() {
    BUILDS.with(|builds| builds.set(builds.get() + 1));
}

/// First index of `items` (sorted so that `before` is true for a prefix) for
/// which `before` is false: a binary search that counts its iterations, so
/// the tests can see that every caret query is logarithmic.
pub(crate) fn lower_bound<T>(items: &[T], before: impl Fn(&T) -> bool) -> usize {
    let (mut low, mut high) = (0, items.len());
    while low < high {
        add_steps(1);
        let mid = low + (high - low) / 2;
        if before(&items[mid]) {
            low = mid + 1;
        } else {
            high = mid;
        }
    }
    low
}

/// Clears this thread's counters.
#[doc(hidden)]
pub fn reset() {
    STEPS.with(|steps| steps.set(0));
    BUILDS.with(|builds| builds.set(0));
}

/// Caret work counted on this thread since [`reset`].
#[doc(hidden)]
pub fn caret_steps() -> u64 {
    STEPS.with(Cell::get)
}

/// Caret-stop tables built on this thread since [`reset`].
#[doc(hidden)]
pub fn caret_builds() -> u64 {
    BUILDS.with(Cell::get)
}
