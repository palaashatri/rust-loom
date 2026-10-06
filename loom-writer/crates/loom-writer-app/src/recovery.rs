use loom_production::define_snapshot_recovery;
use loom_writer_core::WriterDocument;
use slint::ComponentHandle;

define_snapshot_recovery!(
    application_id: "org.loom.writer",
    schema: "loom.writer.package/1"
);

/// Start recovery for every interactive Writer session. An explicitly opened
/// file wins over an older draft, but the recovery store remains active so new
/// edits to that file are still protected.
pub(super) fn initialize_editing_session(
    command_line_open: bool,
) -> Result<Option<WriterDocument>, String> {
    let recovered_payload = initialize_snapshot_recovery()?;
    if command_line_open {
        return Ok(None);
    }
    Ok(recovered_payload.and_then(|payload| loom_writer_core::load_document(&payload).ok()))
}

/// Record the current complete document after a user edit.
pub(super) fn record_document(document: &WriterDocument) -> Result<(), String> {
    DEFERRED.with(DeferredWrite::settled);
    #[cfg(test)]
    WRITES.with(|writes| writes.set(writes.get() + 1));
    let payload = loom_writer_core::save_document(document)
        .map_err(|error| format!("could not prepare recovery data: {error}"))?;
    record_snapshot_recovery("writer state", payload)
}

/// Write the draft from the timer without blocking the window: the document is
/// copied now, serialized on a worker thread, and recorded on the UI thread
/// (where the store lives) unless a synchronous write or a clear happened
/// meanwhile. Failures are reported through `app` when the result arrives.
pub(super) fn record_document_deferred(
    app: &crate::WriterApp,
    document: &WriterDocument,
) -> Result<(), String> {
    #[cfg(test)]
    WRITES.with(|writes| writes.set(writes.get() + 1));
    let epoch = DEFERRED.with(DeferredWrite::epoch);
    let copy = document.clone();
    let weak = app.as_weak();
    std::thread::Builder::new()
        .name("writer-recovery".into())
        .spawn(move || {
            let payload = loom_writer_core::save_document(&copy)
                .map_err(|error| format!("could not prepare recovery data: {error}"));
            // Without a running event loop there is nowhere to record it.
            let _ = slint::invoke_from_event_loop(move || {
                if DEFERRED.with(DeferredWrite::epoch) != epoch {
                    return;
                }
                let result =
                    payload.and_then(|payload| record_snapshot_recovery("writer state", payload));
                if let (Err(error), Some(app)) = (result, weak.upgrade()) {
                    crate::warn_recovery_failed(&app, &error);
                }
            });
        })
        .map(|_| ())
        .map_err(|error| format!("could not start the recovery writer: {error}"))
}

/// Checkpoint the recovery store after the document has been saved to disk.
pub(super) fn checkpoint_document(document: &WriterDocument) -> Result<(), String> {
    DEFERRED.with(DeferredWrite::settled);
    let payload = loom_writer_core::save_document(document)
        .map_err(|error| format!("could not prepare recovery checkpoint: {error}"))?;
    checkpoint_snapshot_recovery(payload)
}

/// Forget the recovery data when the user deliberately closes the document
/// (clean, saved or discarded). A crash leaves it in place for the next launch;
/// an intentional close must not, or a discarded draft comes back every time.
pub(super) fn discard_document_recovery() -> Result<(), String> {
    DEFERRED.with(DeferredWrite::settled);
    LOOM_SNAPSHOT_RECOVERY.with(|slot| match slot.borrow_mut().take() {
        Some(recovery) => recovery.clear().map_err(|error| error.to_string()),
        None => Ok(()),
    })
}

/// How long after the first unrecorded keystroke the draft is written. A
/// crash loses at most this much typing; a pause or any other action (save,
/// open, formatting, undo, close) writes at once.
pub(super) const WRITE_DELAY: std::time::Duration = std::time::Duration::from_millis(600);

/// Bookkeeping for writing the recovery draft off the keystroke path: typing
/// marks the draft stale and arms one timer, and the timer (or any synchronous
/// write that covers the same state) settles it.
#[derive(Default)]
pub(super) struct DeferredWrite {
    stale: std::cell::Cell<bool>,
    armed: std::cell::Cell<bool>,
    /// Bumped by every synchronous write or clear, so a draft still being
    /// serialized when one happens is dropped instead of landing after it.
    epoch: std::cell::Cell<u64>,
}

impl DeferredWrite {
    /// A keystroke changed the document. Returns whether the caller must start
    /// the timer, which is only once per delay window.
    pub(super) fn note_edit(&self) -> bool {
        self.stale.set(true);
        !self.armed.replace(true)
    }

    /// The timer fired. Returns whether the draft needs writing.
    pub(super) fn fire(&self) -> bool {
        self.armed.set(false);
        self.stale.replace(false)
    }

    /// A synchronous write already recorded the current document.
    pub(super) fn settled(&self) {
        self.stale.set(false);
        self.epoch.set(self.epoch.get() + 1);
    }

    pub(super) fn epoch(&self) -> u64 {
        self.epoch.get()
    }

    #[cfg(test)]
    pub(super) fn is_stale(&self) -> bool {
        self.stale.get()
    }
}

std::thread_local! {
    pub(super) static DEFERRED: DeferredWrite = DeferredWrite::default();
}

#[cfg(test)]
std::thread_local! {
    /// Drafts written on this thread, for tests of when writes happen.
    pub(super) static WRITES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
