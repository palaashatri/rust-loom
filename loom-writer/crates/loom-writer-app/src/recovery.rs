use crate::recovery_draft::{self, RecoveredDraft};
use loom_production::define_snapshot_recovery;
use loom_writer_core::WriterDocument;
use slint::ComponentHandle;
use std::path::{Path, PathBuf};

define_snapshot_recovery!(
    application_id: "org.loom.writer",
    schema: "loom.writer.package/1"
);

/// What startup found in the recovery store for this launch.
pub(super) struct StartupRecovery {
    /// The draft to show instead of the requested or template document.
    pub(super) draft: Option<RecoveredDraft>,
    /// Status text for the window, when this launch has something to say.
    pub(super) notice: Option<String>,
}

/// Start recovery for an interactive Writer session.
///
/// A draft of the file being opened is restored as unsaved work that saves back
/// to that file. A draft of any other file is never overwritten: it is kept
/// beside the live slot and offered on a later launch. Plain launches restore
/// the live draft, or the newest kept draft when the live slot is empty.
pub(super) fn initialize_editing_session(open: Option<&Path>) -> Result<StartupRecovery, String> {
    let startup = start(open);
    if startup.is_err() {
        // The live slot must stop recording now: a new edit appended to it would
        // become the newest record and displace the draft still stored there.
        let _ = LOOM_SNAPSHOT_RECOVERY.with(|slot| slot.borrow_mut().take());
    }
    startup
}

fn start(open: Option<&Path>) -> Result<StartupRecovery, String> {
    let kept_root = recovery_draft::quarantine_root()?;
    let live = initialize_snapshot_recovery()?;
    let mut draft = None;
    let mut notice = None;
    if let Some(payload) = live {
        match RecoveredDraft::from_payload(&payload) {
            Some(found) if found.belongs_to(open) => {
                // The live slot continues this draft, so an older kept copy of it
                // would only resurrect it after a later discard.
                prune_kept_copies(&kept_root, &payload);
                notice = Some(restored_notice(&found));
                draft = Some(found);
            }
            found => {
                // Another file was opened, or the data cannot be read. Keep the
                // exact bytes before the slot is cleared for this launch.
                recovery_draft::keep(&kept_root, &payload)?;
                reset_live_slot()?;
                notice = Some(kept_notice(found.as_ref()));
            }
        }
    }
    if draft.is_none() {
        if let Some(found) = restore_kept(&kept_root, open)? {
            notice = Some(restored_notice(&found));
            draft = Some(found);
        }
    }
    Ok(StartupRecovery { draft, notice })
}

/// Bring back the newest kept draft that belongs to this launch. The live slot
/// is empty here, so the kept copy is recorded there first and only then removed.
fn restore_kept(root: &Path, open: Option<&Path>) -> Result<Option<RecoveredDraft>, String> {
    for entry in recovery_draft::kept_entries(root) {
        let Ok(Some(payload)) = recovery_draft::read_kept(&entry) else {
            continue;
        };
        let Some(found) = RecoveredDraft::from_payload(&payload) else {
            continue;
        };
        if !found.belongs_to(open) {
            continue;
        }
        record_snapshot_recovery("writer state", payload)?;
        if let Err(error) = recovery_draft::remove_kept(&entry) {
            // The draft is live already; a leftover copy is pruned on the next launch.
            eprintln!("Writer could not remove a restored kept draft: {error}");
        }
        return Ok(Some(found));
    }
    Ok(None)
}

/// Remove kept copies identical to a payload that the live slot now holds.
fn prune_kept_copies(root: &Path, payload: &[u8]) {
    for entry in recovery_draft::kept_entries(root) {
        if matches!(recovery_draft::read_kept(&entry), Ok(Some(kept)) if kept == payload) {
            if let Err(error) = recovery_draft::remove_kept(&entry) {
                eprintln!("Writer could not remove a duplicate kept draft: {error}");
            }
        }
    }
}

/// Clear the live slot's data and start an empty slot. Callers keep a copy first.
fn reset_live_slot() -> Result<(), String> {
    if let Some(live) = LOOM_SNAPSHOT_RECOVERY.with(|slot| slot.borrow_mut().take()) {
        live.clear().map_err(|error| error.to_string())?;
    }
    initialize_snapshot_recovery()?;
    Ok(())
}

fn restored_notice(draft: &RecoveredDraft) -> String {
    format!("Restored unsaved changes to {}", draft.name())
}

fn kept_notice(draft: Option<&RecoveredDraft>) -> String {
    match draft {
        Some(draft) => format!(
            "Unsaved changes to {} were kept and will be offered on a later launch.",
            draft.name()
        ),
        None => "Recovery data that could not be read was kept on disk.".to_owned(),
    }
}

/// Record the current complete document after a user edit. `source` is the
/// file the document is saved to, if any, so the draft can return to it.
pub(super) fn record_document(
    document: &WriterDocument,
    source: Option<&Path>,
) -> Result<(), String> {
    DEFERRED.with(DeferredWrite::settled);
    #[cfg(test)]
    WRITES.with(|writes| writes.set(writes.get() + 1));
    let package = loom_writer_core::save_document(document)
        .map_err(|error| format!("could not prepare recovery data: {error}"))?;
    record_snapshot_recovery("writer state", recovery_draft::encode(source, &package))
}

/// Write the draft from the timer without blocking the window: the document is
/// copied now, serialized on a worker thread, and recorded on the UI thread
/// (where the store lives) unless a synchronous write or a clear happened
/// meanwhile. Failures are reported through `app` when the result arrives.
pub(super) fn record_document_deferred(
    app: &crate::WriterApp,
    document: &WriterDocument,
    source: Option<PathBuf>,
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
                .map(|package| recovery_draft::encode(source.as_deref(), &package))
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

/// Checkpoint the recovery store after the document has been saved to `source`.
pub(super) fn checkpoint_document(
    document: &WriterDocument,
    source: Option<&Path>,
) -> Result<(), String> {
    DEFERRED.with(DeferredWrite::settled);
    let package = loom_writer_core::save_document(document)
        .map_err(|error| format!("could not prepare recovery checkpoint: {error}"))?;
    checkpoint_snapshot_recovery(recovery_draft::encode(source, &package))
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
