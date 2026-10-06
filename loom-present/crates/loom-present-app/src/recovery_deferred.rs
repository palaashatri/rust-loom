//! Recovery drafts written off the edit path.
//!
//! Serializing a whole deck costs time that grows with the deck (about 21 ms
//! for 300 slides), so doing it inside every slide switch or edit made those
//! actions slow on big decks. Edits now only mark the draft stale and arm one
//! timer; when it fires, a copy of the deck's content is serialized on a worker
//! thread and the bytes are recorded back on the UI thread, where the recovery
//! store lives. A crash loses at most [`WRITE_DELAY`] plus the serialization
//! time of typing. Saving and closing settle or cancel a pending draft through
//! [`invalidate`], so a stale draft can never land after a checkpoint or after
//! the store was cleared.

use std::cell::Cell;
use std::time::Duration;

use loom_present_core::{save_presentation_session, PresentationSession};
use slint::{ComponentHandle, Timer, TimerMode};

use crate::PresentApp;

/// How long after the first unrecorded change the draft is written.
pub(crate) const WRITE_DELAY: Duration = Duration::from_millis(600);

thread_local! {
    static STALE: Cell<bool> = const { Cell::new(false) };
    static ARMED: Cell<bool> = const { Cell::new(false) };
    /// Bumped whenever a draft in flight must be dropped.
    static EPOCH: Cell<u64> = const { Cell::new(0) };
    static TIMER: Timer = Timer::default();
}

#[cfg(test)]
thread_local! {
    /// Draft serializations started on this thread, for tests of when work happens.
    pub(crate) static JOBS: Cell<usize> = const { Cell::new(0) };
}

/// The deck changed. Marks the draft stale and arms the timer, once per window.
pub(crate) fn note_edit(app: &PresentApp) {
    STALE.with(|stale| stale.set(true));
    if ARMED.with(|armed| armed.replace(true)) {
        return;
    }
    let weak = app.as_weak();
    TIMER.with(|timer| {
        timer.start(TimerMode::SingleShot, WRITE_DELAY, move || {
            ARMED.with(|armed| armed.set(false));
            if let Some(app) = weak.upgrade() {
                app.invoke_recovery_flush();
            }
        });
    });
}

/// Drop any draft that is stale or in flight: a checkpoint or a cleared store
/// already covers (or supersedes) it.
pub(crate) fn invalidate() {
    STALE.with(|stale| stale.set(false));
    EPOCH.with(|epoch| epoch.set(epoch.get() + 1));
}

/// Whether a change is waiting to be written.
#[cfg(test)]
pub(crate) fn is_stale() -> bool {
    STALE.with(Cell::get)
}

/// The content a draft needs, without the undo history.
fn content_of(session: &PresentationSession) -> PresentationSession {
    let mut content = PresentationSession::new(session.document.clone());
    content.theme = session.theme.clone();
    content.transitions = session.transitions.clone();
    content
}

/// Write the draft if it is stale: copy the content now, serialize it on a
/// worker thread and record the result when it comes back.
pub(crate) fn flush(session: &PresentationSession) {
    if !STALE.with(|stale| stale.replace(false)) {
        return;
    }
    #[cfg(test)]
    JOBS.with(|jobs| jobs.set(jobs.get() + 1));
    let content = content_of(session);
    let epoch = EPOCH.with(Cell::get);
    let spawned = std::thread::Builder::new()
        .name("present-recovery".into())
        .spawn(move || {
            let Ok(bytes) = save_presentation_session(&content) else {
                return;
            };
            // Without a running event loop there is nowhere to record it.
            let _ = slint::invoke_from_event_loop(move || deliver(epoch, bytes));
        });
    if spawned.is_err() {
        // No thread: leave the draft stale so the next change tries again.
        STALE.with(|stale| stale.set(true));
    }
}

/// Record a serialized draft unless something superseded it while it was built.
fn deliver(epoch: u64, bytes: Vec<u8>) {
    if EPOCH.with(Cell::get) != epoch {
        return;
    }
    let _ = crate::record_snapshot_recovery("presentation state", bytes);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_draft_built_before_an_invalidation_is_dropped() {
        // `deliver` is the only way a draft reaches the store; an invalidated
        // epoch must not record. With no recovery store open recording is a
        // no-op either way, so the epoch check is asserted directly.
        let epoch = EPOCH.with(Cell::get);
        invalidate();
        assert_ne!(EPOCH.with(Cell::get), epoch);
        STALE.with(|stale| stale.set(true));
        invalidate();
        assert!(!is_stale(), "an invalidation also clears a pending draft");
    }
}
