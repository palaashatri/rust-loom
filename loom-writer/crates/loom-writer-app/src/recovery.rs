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
                // exact bytes before the slot is cleared for this launch, unless
                // the draft is clean against its source.
                if !found
                    .as_ref()
                    .is_some_and(RecoveredDraft::is_clean_against_source)
                {
                    recovery_draft::keep(&kept_root, &payload)?;
                    notice = Some(kept_notice(found.as_ref()));
                }
                reset_live_slot()?;
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

#[cfg(test)]
std::thread_local! {
    pub(super) static INJECT_FAILURE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Record the current complete document after a user edit. `source` is the
/// file the document is saved to, if any, so the draft can return to it.
pub(super) fn record_document(
    document: &WriterDocument,
    source: Option<&Path>,
) -> Result<(), String> {
    #[cfg(test)]
    if INJECT_FAILURE.with(std::cell::Cell::get) {
        return Err("injected recovery failure".into());
    }
    DEFERRED.with(DeferredWrite::settled);
    #[cfg(test)]
    WRITES.with(|writes| writes.set(writes.get() + 1));
    let package = loom_writer_core::save_document(document)
        .map_err(|error| format!("could not prepare recovery data: {error}"))?;
    record_snapshot_recovery("writer state", recovery_draft::encode(source, &package))
}

/// Write the draft from the timer without blocking the window: the document is
/// copied now, serialized on one worker thread at a time, and recorded on the UI
/// thread (where the store lives) unless a newer edit, synchronous write, or a
/// clear invalidated it. While a worker is active, only the newest snapshot waits.
pub(super) fn record_document_deferred(
    app: &crate::WriterApp,
    document: &WriterDocument,
    source: Option<PathBuf>,
) -> Result<DeferredDispatch, String> {
    #[cfg(test)]
    WRITES.with(|writes| writes.set(writes.get() + 1));
    let epoch = DEFERRED.with(DeferredWrite::epoch);
    let snapshot = DeferredSnapshot {
        epoch,
        document: document.clone(),
        source,
        app: app.as_weak(),
    };
    if let Some(snapshot) = DEFERRED.with(|state| state.submit(snapshot)) {
        start_deferred_snapshot(snapshot)?;
        Ok(DeferredDispatch::Started)
    } else {
        Ok(DeferredDispatch::Queued)
    }
}

#[derive(Clone)]
struct DeferredSnapshot {
    epoch: u64,
    document: WriterDocument,
    source: Option<PathBuf>,
    app: slint::Weak<crate::WriterApp>,
}

fn start_deferred_snapshot(snapshot: DeferredSnapshot) -> Result<(), String> {
    let retry = snapshot.clone();
    #[cfg(test)]
    let test_worker = DEFERRED_TEST_WORKER.with(|worker| worker.borrow().clone());
    let result = std::thread::Builder::new()
        .name("writer-recovery".into())
        .spawn(move || {
            #[cfg(test)]
            if let Some(worker) = test_worker.as_ref() {
                worker.before_serialize();
            }
            let payload = loom_writer_core::save_document(&snapshot.document)
                .map(|package| recovery_draft::encode(snapshot.source.as_deref(), &package))
                .map_err(|error| format!("could not prepare recovery data: {error}"));
            #[cfg(test)]
            if let Some(worker) = test_worker.as_ref() {
                worker.after_serialize();
            }
            let complete = move || deliver_deferred_snapshot(snapshot, payload);
            #[cfg(test)]
            match test_worker {
                Some(worker) => {
                    let _ = worker.completed.send(Box::new(complete));
                }
                None => {
                    // Without a running event loop there is nowhere to record it.
                    let _ = slint::invoke_from_event_loop(complete);
                }
            }
            #[cfg(not(test))]
            {
                // Without a running event loop there is nowhere to record it.
                let _ = slint::invoke_from_event_loop(complete);
            }
        });
    match result {
        Ok(_) => Ok(()),
        Err(error) => {
            DEFERRED.with(|state| state.start_failed(retry));
            Err(format!("could not start the recovery writer: {error}"))
        }
    }
}

fn deliver_deferred_snapshot(snapshot: DeferredSnapshot, payload: Result<Vec<u8>, String>) {
    if DEFERRED.with(DeferredWrite::epoch) == snapshot.epoch {
        let result = payload.and_then(|payload| {
            #[cfg(test)]
            if INJECT_FAILURE.with(std::cell::Cell::get) {
                return Err("injected recovery failure".into());
            }
            record_snapshot_recovery("writer state", payload)
        });
        match result {
            Ok(()) => {
                DEFERRED.with(|state| state.delivered(snapshot.epoch));
                if let Some(app) = snapshot.app.upgrade() {
                    crate::clear_recovery_failed(&app);
                }
            }
            Err(error) => {
                DEFERRED.with(|state| state.retryable(snapshot.epoch));
                if let Some(app) = snapshot.app.upgrade() {
                    crate::warn_recovery_failed(&app, &error);
                }
            }
        }
    }

    if let Some(next) = DEFERRED.with(DeferredWrite::finished) {
        let weak = next.app.clone();
        if let Err(error) = start_deferred_snapshot(next) {
            if let Some(app) = weak.upgrade() {
                crate::warn_recovery_failed(&app, &error);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DeferredDispatch {
    Started,
    Queued,
}

#[cfg(test)]
pub(super) type DeferredCompletion = Box<dyn FnOnce() + Send + 'static>;

#[cfg(test)]
#[derive(Clone)]
struct DeferredTestWorker {
    started: std::sync::mpsc::Sender<usize>,
    completed: std::sync::mpsc::Sender<DeferredCompletion>,
    release_first: std::sync::Arc<std::sync::Mutex<std::sync::mpsc::Receiver<()>>>,
    start_count: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    in_flight: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    max_in_flight: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

#[cfg(test)]
impl DeferredTestWorker {
    fn before_serialize(&self) {
        use std::sync::atomic::Ordering;

        let index = self.start_count.fetch_add(1, Ordering::SeqCst);
        let active = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        self.max_in_flight.fetch_max(active, Ordering::SeqCst);
        let _ = self.started.send(index);
        if index == 0 {
            if let Ok(receiver) = self.release_first.lock() {
                let _ = receiver.recv();
            }
        }
    }

    fn after_serialize(&self) {
        use std::sync::atomic::Ordering;

        self.in_flight.fetch_sub(1, Ordering::SeqCst);
    }
}

#[cfg(test)]
pub(super) struct DeferredWriteTestHarness {
    pub(super) started: std::sync::mpsc::Receiver<usize>,
    pub(super) completed: std::sync::mpsc::Receiver<DeferredCompletion>,
    release_first: std::sync::mpsc::Sender<()>,
    max_in_flight: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

#[cfg(test)]
impl DeferredWriteTestHarness {
    pub(super) fn install() -> Self {
        let (started_tx, started) = std::sync::mpsc::channel();
        let (completed_tx, completed) = std::sync::mpsc::channel();
        let (release_first, release_first_rx) = std::sync::mpsc::channel();
        let max_in_flight = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let worker = DeferredTestWorker {
            started: started_tx,
            completed: completed_tx,
            release_first: std::sync::Arc::new(std::sync::Mutex::new(release_first_rx)),
            start_count: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            in_flight: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            max_in_flight: max_in_flight.clone(),
        };
        DEFERRED_TEST_WORKER.with(|slot| *slot.borrow_mut() = Some(worker));
        Self {
            started,
            completed,
            release_first,
            max_in_flight,
        }
    }

    pub(super) fn release_first(&self) {
        let _ = self.release_first.send(());
    }

    pub(super) fn max_in_flight(&self) -> usize {
        self.max_in_flight.load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[cfg(test)]
impl Drop for DeferredWriteTestHarness {
    fn drop(&mut self) {
        DEFERRED_TEST_WORKER.with(|slot| *slot.borrow_mut() = None);
        self.release_first();
    }
}

#[cfg(test)]
std::thread_local! {
    static DEFERRED_TEST_WORKER: std::cell::RefCell<Option<DeferredTestWorker>> =
        const { std::cell::RefCell::new(None) };
}

/// Checkpoint the recovery store after the document has been saved to `source`.
pub(super) fn checkpoint_document(
    document: &WriterDocument,
    source: Option<&Path>,
) -> Result<(), String> {
    #[cfg(test)]
    if INJECT_FAILURE.with(std::cell::Cell::get) {
        return Err("injected recovery failure".into());
    }
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

/// How long after an unrecorded keystroke before recovery serialization is
/// attempted. This debounce interval is not a crash-loss bound: serialization
/// and the recovery-store write can still be in progress after it fires.
pub(super) const WRITE_DELAY: std::time::Duration = std::time::Duration::from_millis(600);

/// Bookkeeping for writing the recovery draft off the keystroke path: typing
/// marks the draft stale and arms one timer, and the timer (or any synchronous
/// write that covers the same state) settles it.
#[derive(Default)]
pub(super) struct DeferredWrite {
    stale: std::cell::Cell<bool>,
    armed: std::cell::Cell<bool>,
    /// Bumped by edits and synchronous writes so older serialized drafts are
    /// discarded before they can reach the recovery store.
    epoch: std::cell::Cell<u64>,
    in_flight: std::cell::Cell<bool>,
    pending: std::cell::RefCell<Option<DeferredSnapshot>>,
}

impl DeferredWrite {
    /// A keystroke changed the document. Returns whether the caller must start
    /// the timer, which is only once per delay window.
    pub(super) fn note_edit(&self) -> bool {
        self.stale.set(true);
        self.epoch.set(self.epoch.get().wrapping_add(1));
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
        self.armed.set(false);
        self.pending.borrow_mut().take();
        self.epoch.set(self.epoch.get().wrapping_add(1));
    }

    fn submit(&self, snapshot: DeferredSnapshot) -> Option<DeferredSnapshot> {
        if self.in_flight.get() {
            self.pending.replace(Some(snapshot));
            None
        } else {
            self.pending.borrow_mut().take();
            self.in_flight.set(true);
            Some(snapshot)
        }
    }

    fn start_failed(&self, snapshot: DeferredSnapshot) {
        self.in_flight.set(false);
        if snapshot.epoch == self.epoch.get() {
            self.pending.replace(Some(snapshot));
            self.stale.set(true);
        }
    }

    fn finished(&self) -> Option<DeferredSnapshot> {
        self.in_flight.set(false);
        let pending = self.pending.borrow_mut().take();
        pending
            .filter(|snapshot| snapshot.epoch == self.epoch.get())
            .inspect(|_| self.in_flight.set(true))
    }

    fn delivered(&self, epoch: u64) {
        if self.epoch.get() == epoch {
            self.stale.set(false);
        }
    }

    fn retryable(&self, epoch: u64) {
        if self.epoch.get() == epoch {
            self.stale.set(true);
        }
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
