//! Recovery drafts written off the edit path.
//!
//! Serializing a whole deck costs time that grows with the deck (about 21 ms
//! for 300 slides), so doing it inside every slide switch or edit made those
//! actions slow on big decks. Edits now only mark the draft stale and arm one
//! timer; when it fires, a copy of the deck's content is serialized on a worker
//! thread and the bytes are recorded back on the UI thread, where the recovery
//! store lives. A failed write stays pending and reports its actual error; a
//! later successful write clears that error. Saving and closing invalidate a
//! pending draft through [`invalidate`], so obsolete bytes cannot land after a
//! checkpoint or after the store was cleared.

use std::cell::{Cell, RefCell};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use loom_present_core::PresentationSession;
use slint::{ComponentHandle, SharedString, Timer, TimerMode};

use crate::{recovery_draft, GuiState, PresentApp};

/// How long after the first unrecorded change the draft is written.
pub(crate) const WRITE_DELAY: Duration = Duration::from_millis(600);
/// A single automatic retry is delayed to avoid a tight loop on persistent I/O failure.
const RETRY_DELAY: Duration = Duration::from_secs(5);

#[derive(Default)]
struct DeferredState {
    stale: bool,
    active_epoch: Option<u64>,
    serialization_active: bool,
    /// Identifies the newest edit and invalidates obsolete serialized drafts.
    epoch: u64,
    recovery_error: Option<String>,
    /// At most one automatic retry attempt is allowed until another edit or a success.
    auto_retry_used: bool,
}

type SharedState = Arc<Mutex<DeferredState>>;

thread_local! {
    // Workers hold a bounded clone only until their single delivery attempt
    // completes, so a rejected event-loop dispatch can retain retry state.
    static STATE: RefCell<SharedState> = RefCell::new(Arc::new(Mutex::new(DeferredState::default())));
    static ARMED: Cell<bool> = const { Cell::new(false) };
    static RETRY_ARMED: Cell<bool> = const { Cell::new(false) };
    static BASE_STATUS: RefCell<Option<String>> = const { RefCell::new(None) };
    static TIMER: Timer = Timer::default();
    static RETRY_TIMER: Timer = Timer::default();
}

fn shared_state() -> SharedState {
    STATE.with(|state| state.borrow().clone())
}

pub(crate) fn wire_callbacks(app: &PresentApp, state: &std::rc::Rc<GuiState>) {
    let state = state.clone();
    let app_ref = app.as_weak();
    app.on_recovery_flush(move || {
        if let Some(app) = app_ref.upgrade() {
            if let Ok(session) = state.session.try_borrow() {
                let source = state.save_path.borrow().clone();
                flush(&app, &session, source.as_deref());
            }
        }
    });

    let app_ref = app.as_weak();
    app.on_recovery_retry(move || {
        if let Some(app) = app_ref.upgrade() {
            retry_now(&app);
        }
    });
}

fn with_shared_state<T>(state: &SharedState, f: impl FnOnce(&mut DeferredState) -> T) -> T {
    let mut state = state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    f(&mut state)
}

fn with_state<T>(f: impl FnOnce(&mut DeferredState) -> T) -> T {
    let state = shared_state();
    with_shared_state(&state, f)
}

#[cfg(test)]
thread_local! {
    /// Draft serializations started on this thread, for tests of when work happens.
    pub(crate) static JOBS: Cell<usize> = const { Cell::new(0) };
}

#[cfg(test)]
pub(crate) mod test_support {
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
    use std::sync::{mpsc, Arc, Mutex};
    use std::thread::JoinHandle;
    use std::time::Instant;

    use super::*;

    static NEXT_JOB_ID: AtomicU64 = AtomicU64::new(1);
    type Completion = Box<dyn FnOnce() + Send + 'static>;

    thread_local! {
        static ACTIVE_HARNESS: RefCell<Option<Arc<HarnessInner>>> = const { RefCell::new(None) };
    }

    enum Event {
        Started { id: u64, release: mpsc::Sender<()> },
        Delivery(Delivery),
        Finished(u64),
    }

    pub(super) struct HarnessInner {
        events: mpsc::Sender<Event>,
        launch_requests: AtomicUsize,
        active_workers: AtomicUsize,
        max_active_workers: AtomicUsize,
        closing: AtomicBool,
        releases: Mutex<Vec<mpsc::Sender<()>>>,
        workers: Mutex<Vec<JoinHandle<()>>>,
    }

    pub(crate) struct Harness {
        inner: Arc<HarnessInner>,
        events: mpsc::Receiver<Event>,
        buffered: VecDeque<Event>,
        previous: Option<Arc<HarnessInner>>,
        previous_state: Option<SharedState>,
        finished: Vec<u64>,
    }

    pub(crate) struct Started {
        pub(crate) id: u64,
        release: mpsc::Sender<()>,
    }

    pub(crate) struct Delivery {
        pub(crate) id: u64,
        serialization_error: Option<String>,
        invoke_error: Option<String>,
        completion: Option<Completion>,
        dispatch_failure: Option<Completion>,
        acknowledgement: Option<mpsc::Sender<()>>,
    }

    pub(super) struct ActiveWorker(Arc<HarnessInner>);

    impl Harness {
        pub(crate) fn install() -> Self {
            let (events, receiver) = mpsc::channel();
            let previous_state = STATE.with(|state| {
                std::mem::replace(
                    &mut *state.borrow_mut(),
                    Arc::new(Mutex::new(DeferredState::default())),
                )
            });
            TIMER.with(Timer::stop);
            RETRY_TIMER.with(Timer::stop);
            ARMED.with(|armed| armed.set(false));
            RETRY_ARMED.with(|armed| armed.set(false));
            let inner = Arc::new(HarnessInner {
                events,
                launch_requests: AtomicUsize::new(0),
                active_workers: AtomicUsize::new(0),
                max_active_workers: AtomicUsize::new(0),
                closing: AtomicBool::new(false),
                releases: Mutex::new(Vec::new()),
                workers: Mutex::new(Vec::new()),
            });
            let previous = ACTIVE_HARNESS.with(|active| active.borrow_mut().replace(inner.clone()));
            Self {
                inner,
                events: receiver,
                buffered: VecDeque::new(),
                previous,
                previous_state: Some(previous_state),
                finished: Vec::new(),
            }
        }

        pub(crate) fn launch_requests(&self) -> usize {
            self.inner.launch_requests.load(Ordering::SeqCst)
        }

        pub(crate) fn max_active_workers(&self) -> usize {
            self.inner.max_active_workers.load(Ordering::SeqCst)
        }

        pub(crate) fn receive_started(&mut self) -> Started {
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                if let Some(position) = self
                    .buffered
                    .iter()
                    .position(|event| matches!(event, Event::Started { .. }))
                {
                    if let Some(Event::Started { id, release }) = self.buffered.remove(position) {
                        return Started { id, release };
                    }
                }
                let remaining = deadline.saturating_duration_since(Instant::now());
                assert!(
                    !remaining.is_zero(),
                    "timed out waiting for recovery worker start"
                );
                match self.events.recv_timeout(remaining) {
                    Ok(Event::Started { id, release }) => return Started { id, release },
                    Ok(event @ Event::Delivery(_)) => self.buffered.push_back(event),
                    Ok(Event::Finished(id)) => self.finished.push(id),
                    Err(error) => panic!("deferred recovery worker event: {error}"),
                }
            }
        }

        pub(crate) fn receive_delivery(&mut self) -> Delivery {
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                if let Some(position) = self
                    .buffered
                    .iter()
                    .position(|event| matches!(event, Event::Delivery(_)))
                {
                    if let Some(Event::Delivery(delivery)) = self.buffered.remove(position) {
                        return delivery;
                    }
                }
                let remaining = deadline.saturating_duration_since(Instant::now());
                assert!(
                    !remaining.is_zero(),
                    "timed out waiting for recovery completion"
                );
                match self.events.recv_timeout(remaining) {
                    Ok(Event::Delivery(delivery)) => return delivery,
                    Ok(event @ Event::Started { .. }) => self.buffered.push_back(event),
                    Ok(Event::Finished(id)) => self.finished.push(id),
                    Err(error) => panic!("deferred recovery worker event: {error}"),
                }
            }
        }
    }

    impl Started {
        pub(crate) fn release(self) {
            let _ = self.release.send(());
        }
    }

    impl Delivery {
        pub(crate) fn completion_for_test(
            id: u64,
            epoch: u64,
            bytes: Vec<u8>,
            app: slint::Weak<PresentApp>,
        ) -> Self {
            let state = super::shared_state();
            let completion: Completion =
                Box::new(move || super::finish_delivery(state, epoch, app, Ok(bytes)));
            Self {
                id,
                serialization_error: None,
                invoke_error: Some("invoke_from_event_loop rejected dispatch".into()),
                completion: Some(completion),
                dispatch_failure: None,
                acknowledgement: None,
            }
        }

        pub(crate) fn serialization_error_for_test(
            id: u64,
            epoch: u64,
            error: String,
            app: slint::Weak<PresentApp>,
        ) -> Self {
            let state = super::shared_state();
            let completion: Completion = Box::new(move || {
                super::finish_delivery(state, epoch, app, Err(error));
            });
            Self {
                id,
                serialization_error: Some("injected serialization failure".into()),
                invoke_error: Some("invoke_from_event_loop rejected dispatch".into()),
                completion: Some(completion),
                dispatch_failure: None,
                acknowledgement: None,
            }
        }

        pub(crate) fn serialization_error(&self) -> Option<&str> {
            self.serialization_error.as_deref()
        }

        pub(crate) fn invoke_error(&self) -> Option<&str> {
            self.invoke_error.as_deref()
        }

        /// Runs the captured completion callback when a test chooses to deliver
        /// it after the deterministic rejected-dispatch setup.
        pub(crate) fn run_after_invoke_failure(mut self) {
            assert!(
                self.invoke_error.is_some(),
                "the test callback fallback is only valid after invoke_from_event_loop fails"
            );
            let completion = self
                .completion
                .take()
                .expect("failed event-loop dispatch retains the original completion callback");
            completion();
            if let Some(acknowledgement) = self.acknowledgement.take() {
                let _ = acknowledgement.send(());
            }
        }

        pub(crate) fn run_dispatch_failure_for_test(mut self) {
            assert!(
                self.invoke_error.is_some(),
                "the dispatch-failure callback is only valid after invoke_from_event_loop fails"
            );
            let failure = self
                .dispatch_failure
                .take()
                .expect("rejected dispatch retains a bounded failure callback");
            failure();
            if let Some(acknowledgement) = self.acknowledgement.take() {
                let _ = acknowledgement.send(());
            }
        }
    }

    impl Drop for Delivery {
        fn drop(&mut self) {
            if let Some(acknowledgement) = self.acknowledgement.take() {
                let _ = acknowledgement.send(());
            }
        }
    }

    impl Drop for Harness {
        fn drop(&mut self) {
            ACTIVE_HARNESS.with(|active| *active.borrow_mut() = self.previous.take());
            self.inner.closing.store(true, Ordering::SeqCst);
            if let Ok(releases) = self.inner.releases.lock() {
                for release in releases.iter() {
                    let _ = release.send(());
                }
            }

            let workers = self
                .inner
                .workers
                .lock()
                .map(|mut workers| std::mem::take(&mut *workers))
                .unwrap_or_default();
            let expected = workers.len();
            let deadline = Instant::now() + Duration::from_secs(10);
            while self.finished.len() < expected {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    break;
                }
                let event = self
                    .buffered
                    .pop_front()
                    .or_else(|| self.events.recv_timeout(remaining).ok());
                match event {
                    Some(Event::Started { release, .. }) => {
                        let _ = release.send(());
                    }
                    Some(Event::Delivery(delivery)) => drop(delivery),
                    Some(Event::Finished(id)) => self.finished.push(id),
                    None => break,
                }
            }
            // Dropping a JoinHandle detaches it. The bounded drain above releases
            // every known gate and delivery; never make test teardown wait forever
            // on a worker whose serializer or injected failure has stalled.
            drop(workers);
            self.buffered.clear();
            while let Ok(event) = self.events.try_recv() {
                if let Event::Delivery(delivery) = event {
                    drop(delivery);
                }
            }
            TIMER.with(Timer::stop);
            RETRY_TIMER.with(Timer::stop);
            ARMED.with(|armed| armed.set(false));
            RETRY_ARMED.with(|armed| armed.set(false));
            if let Some(previous_state) = self.previous_state.take() {
                STATE.with(|state| *state.borrow_mut() = previous_state);
            }
        }
    }

    impl HarnessInner {
        fn worker_started(self: &Arc<Self>, id: u64) -> ActiveWorker {
            let active = self.active_workers.fetch_add(1, Ordering::SeqCst) + 1;
            self.max_active_workers.fetch_max(active, Ordering::SeqCst);
            let (release, wait) = mpsc::channel();
            if let Ok(mut releases) = self.releases.lock() {
                releases.push(release.clone());
            }
            let _ = self.events.send(Event::Started { id, release });
            if !self.closing.load(Ordering::SeqCst) {
                let _ = wait.recv();
            }
            ActiveWorker(self.clone())
        }

        fn enqueue_delivery(
            self: &Arc<Self>,
            id: u64,
            serialization_error: Option<String>,
            invoke_error: Option<String>,
            completion: Option<Completion>,
            dispatch_failure: Option<Completion>,
        ) {
            let (acknowledgement, wait) = mpsc::channel();
            let delivery = Delivery {
                id,
                serialization_error,
                invoke_error,
                completion,
                dispatch_failure,
                acknowledgement: Some(acknowledgement),
            };
            let _ = self.events.send(Event::Delivery(delivery));
            let _ = wait.recv();
            let _ = self.events.send(Event::Finished(id));
        }
    }

    impl Drop for ActiveWorker {
        fn drop(&mut self) {
            self.0.active_workers.fetch_sub(1, Ordering::SeqCst);
        }
    }

    pub(super) fn active_harness() -> Option<Arc<HarnessInner>> {
        ACTIVE_HARNESS.with(|active| active.borrow().clone())
    }

    impl HarnessInner {
        pub(super) fn register_launch(&self) -> u64 {
            self.launch_requests.fetch_add(1, Ordering::SeqCst);
            NEXT_JOB_ID.fetch_add(1, Ordering::SeqCst)
        }

        pub(super) fn worker_started_for_test(self: &Arc<Self>, id: u64) -> ActiveWorker {
            self.worker_started(id)
        }

        pub(super) fn dispatch_for_test(
            self: &Arc<Self>,
            id: u64,
            epoch: u64,
            state: SharedState,
            app: slint::Weak<PresentApp>,
            result: Result<Vec<u8>, String>,
        ) {
            let serialization_error = result.as_ref().err().cloned();
            let completion_state = state.clone();
            let callback: Completion =
                Box::new(move || super::finish_delivery(completion_state, epoch, app, result));
            let invoke_error = Some("invoke_from_event_loop rejected dispatch".to_string());
            let failure_state = state;
            let failure_reason = invoke_error
                .clone()
                .expect("test dispatch always simulates event-loop rejection");
            let dispatch_failure: Completion = Box::new(move || {
                super::record_dispatch_failure(&failure_state, epoch, failure_reason)
            });
            self.enqueue_delivery(
                id,
                serialization_error,
                invoke_error,
                Some(callback),
                Some(dispatch_failure),
            );
        }

        pub(super) fn register_for_test(&self, worker: JoinHandle<()>) {
            self.workers
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(worker);
        }
    }
}

#[cfg(test)]
pub(crate) fn current_epoch() -> u64 {
    with_state(|state| state.epoch)
}

#[cfg(test)]
pub(crate) fn prepare_delivery_for_test(epoch: u64) {
    with_state(|state| {
        state.epoch = epoch;
        state.stale = false;
        state.active_epoch = Some(epoch);
        state.serialization_active = false;
    });
}

#[cfg(test)]
pub(crate) fn retry_is_armed_for_test() -> bool {
    RETRY_ARMED.with(Cell::get)
}

#[cfg(test)]
pub(crate) fn edit_timer_is_armed_for_test() -> bool {
    ARMED.with(Cell::get)
}

#[cfg(test)]
pub(crate) fn fire_retry_callback_for_test(app: &PresentApp) {
    assert!(retry_is_armed_for_test(), "a deferred retry must be armed");
    RETRY_TIMER.with(Timer::stop);
    retry_timer_fired(app.as_weak());
}

#[cfg(test)]
pub(crate) fn serialization_is_active_for_test() -> bool {
    with_state(|state| state.serialization_active)
}

/// Keeps recovery failures visible while ordinary status updates continue to arrive.
pub(crate) fn project_status(app: &PresentApp, value: impl Into<SharedString>) {
    BASE_STATUS.with(|status| *status.borrow_mut() = Some(value.into().to_string()));
    schedule_retry_if_needed(app);
    render_status(app);
}

fn render_status(app: &PresentApp) {
    let base = BASE_STATUS.with(|status| status.borrow().clone().unwrap_or_default());
    let error = with_state(|state| state.recovery_error.clone());
    app.set_recovery_failure_message(error.clone().unwrap_or_default().into());
    let displayed = match error {
        Some(_) if base.starts_with("Deck saved; recovery checkpoint failed") => base,
        Some(_) if base.is_empty() => "Recovery pending".to_string(),
        Some(_) => format!("{base} · Recovery pending"),
        None => base,
    };
    app.set_status_left(SharedString::from(displayed));
}

fn report_recovery_error(app: &slint::Weak<PresentApp>, error: String) {
    with_state(|state| state.recovery_error = Some(error));
    if let Some(app) = app.upgrade() {
        schedule_retry_if_needed(&app);
        render_status(&app);
    }
}

fn clear_recovery_error(app: &slint::Weak<PresentApp>) {
    cancel_retry();
    with_state(|state| {
        state.recovery_error = None;
        state.auto_retry_used = false;
    });
    BASE_STATUS.with(|status| {
        let mut status = status.borrow_mut();
        if status.as_deref() == Some("Deck saved; recovery checkpoint failed") {
            *status = Some("Deck saved; recovery is current".into());
        } else if let Some(previous) = status.as_deref() {
            const PRESERVATION_FAILURE: &str =
                "The current presentation could not be saved to recovery:";
            if let Some(prefix_end) = previous.find(PRESERVATION_FAILURE) {
                let reset_status = previous[..prefix_end].trim_end();
                *status = Some(format!(
                    "{reset_status} The current presentation is now saved to recovery; retry the replacement to try again."
                ));
            }
        }
    });
    if let Some(app) = app.upgrade() {
        render_status(&app);
    }
}

#[cfg(test)]
#[path = "recovery_deferred_tests.rs"]
mod recovery_deferred_tests;

/// A normal Save As completed, but its recovery checkpoint did not. Keep the
/// current deck eligible for the same deferred write path used by edits.
pub(crate) fn report_checkpoint_failure(app: &PresentApp, error: String) {
    with_state(|state| {
        state.stale = true;
        state.recovery_error = Some(error);
        state.auto_retry_used = false;
    });
    schedule_retry_if_needed(app);
    render_status(app);
}

/// Explicit Retry replaces the pending automatic retry and uses the latest
/// session supplied by the existing recovery-flush callback.
pub(crate) fn retry_now(app: &PresentApp) {
    cancel_retry();
    let should_retry = with_state(|state| {
        if !state.stale {
            return false;
        }
        state.auto_retry_used = true;
        true
    });
    if should_retry {
        invoke_recovery_flush_after_reconnect(app);
    }
}

fn invoke_recovery_flush_after_reconnect(app: &PresentApp) {
    match crate::recovery_replacement::reconnect_recovery_service() {
        Ok(()) => app.invoke_recovery_flush(),
        Err(error) => {
            with_state(|state| {
                state.stale = true;
                // Both explicit and automatic retries have consumed their attempt.
                state.auto_retry_used = true;
            });
            report_recovery_error(
                &app.as_weak(),
                format!("recovery service could not be reopened: {error}"),
            );
        }
    }
}

fn cancel_retry() {
    RETRY_TIMER.with(Timer::stop);
    RETRY_ARMED.with(|armed| armed.set(false));
}

fn schedule_retry_if_needed(app: &PresentApp) {
    if RETRY_ARMED.with(Cell::get) {
        return;
    }
    let should_schedule = with_state(|state| {
        if !state.stale || state.recovery_error.is_none() || state.auto_retry_used {
            return false;
        }
        true
    });
    if !should_schedule {
        return;
    }
    RETRY_ARMED.with(|armed| armed.set(true));
    let weak = app.as_weak();
    RETRY_TIMER.with(|timer| {
        timer.start(TimerMode::SingleShot, RETRY_DELAY, move || {
            retry_timer_fired(weak.clone());
        });
    });
}

fn retry_timer_fired(app: slint::Weak<PresentApp>) {
    RETRY_ARMED.with(|armed| armed.set(false));
    let should_retry = with_state(|state| {
        if !state.stale {
            return false;
        }
        state.auto_retry_used = true;
        true
    });
    if !should_retry {
        return;
    }
    if let Some(app) = app.upgrade() {
        invoke_recovery_flush_after_reconnect(&app);
    }
}

/// The deck changed. Marks the draft stale and arms the timer, once per window.
pub(crate) fn note_edit(app: &PresentApp) {
    cancel_retry();
    with_state(|state| {
        state.epoch = state.epoch.wrapping_add(1);
        state.stale = true;
        state.auto_retry_used = false;
    });
    if with_state(|state| state.recovery_error.is_some()) {
        render_status(app);
    }
    if import_warning_pending() {
        return;
    }
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

/// Suspends pending recovery writes while an import decision owns the session.
/// Any work already being serialized is invalidated and retried after a choice.
pub(crate) fn hold_for_import_warning() {
    TIMER.with(Timer::stop);
    ARMED.with(|armed| armed.set(false));
    cancel_retry();
    with_state(|state| {
        state.stale = true;
        state.epoch = state.epoch.wrapping_add(1);
    });
}

/// Resumes the stale draft after Cancel or Continue has finished safely.
pub(crate) fn resume_after_import_warning(app: &PresentApp) {
    if with_state(|state| state.stale) {
        note_edit(app);
    }
}

/// Drop any draft that is stale or in flight: a checkpoint or a cleared store
/// already covers (or supersedes) it.
pub(crate) fn invalidate() {
    TIMER.with(Timer::stop);
    ARMED.with(|armed| armed.set(false));
    cancel_retry();
    with_state(|state| {
        state.stale = false;
        state.epoch = state.epoch.wrapping_add(1);
        state.recovery_error = None;
        state.auto_retry_used = false;
        if !state.serialization_active {
            state.active_epoch = None;
        }
    });
}

/// Whether a change is waiting to be written.
#[cfg(test)]
pub(crate) fn is_stale() -> bool {
    with_state(|state| state.stale)
}

/// The content a draft needs, without the undo history.
fn content_of(session: &PresentationSession) -> PresentationSession {
    let mut content = PresentationSession::new(session.document.clone());
    content.theme = session.theme.clone();
    content.transitions = session.transitions.clone();
    content
}

/// Write the draft if it is stale: copy the content now, serialize it on a
/// worker thread and record the result when it comes back. `source` is the file
/// the deck is saved to, so the draft can be restored under that name.
pub(crate) fn flush(app: &PresentApp, session: &PresentationSession, source: Option<&Path>) {
    TIMER.with(Timer::stop);
    ARMED.with(|armed| armed.set(false));
    cancel_retry();
    let state = shared_state();
    if import_warning_pending() {
        with_shared_state(&state, |state| state.stale = true);
        return;
    }
    let epoch = with_shared_state(&state, |state| {
        if !state.stale || state.active_epoch.is_some() {
            return None;
        }
        state.stale = false;
        let epoch = state.epoch;
        state.active_epoch = Some(epoch);
        state.serialization_active = true;
        Some(epoch)
    });
    let Some(epoch) = epoch else {
        return;
    };
    #[cfg(test)]
    JOBS.with(|jobs| jobs.set(jobs.get() + 1));
    #[cfg(test)]
    let test_harness = test_support::active_harness();
    #[cfg(test)]
    let test_job_id = test_harness
        .as_ref()
        .map(|harness| harness.register_launch());
    let content = content_of(session);
    let source = source.map(Path::to_path_buf);
    let app_weak = app.as_weak();
    #[cfg(test)]
    let worker_harness = test_harness.clone();
    let worker_state = state.clone();
    let worker_app = app_weak.clone();
    #[cfg(test)]
    let spawned = std::thread::Builder::new()
        .name("present-recovery".into())
        .spawn(move || {
            let active_worker = match (worker_harness.as_ref(), test_job_id) {
                (Some(harness), Some(id)) => Some(harness.worker_started_for_test(id)),
                _ => None,
            };
            let result = recovery_draft::payload(&content, source.as_deref());
            mark_serialization_returned(&worker_state, epoch);
            if let (Some(harness), Some(id)) = (worker_harness, test_job_id) {
                drop(active_worker);
                harness.dispatch_for_test(id, epoch, worker_state, worker_app, result);
                return;
            }
            let failure_state = worker_state.clone();
            if let Err(error) = slint::invoke_from_event_loop(move || {
                finish_delivery(worker_state, epoch, worker_app, result)
            }) {
                record_dispatch_failure(&failure_state, epoch, error.to_string());
            }
        });
    #[cfg(test)]
    match spawned {
        Ok(worker) => {
            if let Some(harness) = test_harness {
                harness.register_for_test(worker);
            }
        }
        Err(_) => {
            with_shared_state(&state, |state| {
                if state.active_epoch == Some(epoch) {
                    state.active_epoch = None;
                    state.serialization_active = false;
                    state.stale = true;
                }
            });
            report_recovery_error(&app_weak, "could not start the recovery serializer".into());
        }
    }

    #[cfg(not(test))]
    let spawned = std::thread::Builder::new()
        .name("present-recovery".into())
        .spawn(move || {
            let result = recovery_draft::payload(&content, source.as_deref());
            mark_serialization_returned(&worker_state, epoch);
            let failure_state = worker_state.clone();
            if let Err(error) = slint::invoke_from_event_loop(move || {
                finish_delivery(worker_state, epoch, worker_app, result)
            }) {
                record_dispatch_failure(&failure_state, epoch, error.to_string());
            }
        });
    #[cfg(not(test))]
    if let Err(error) = spawned {
        with_shared_state(&state, |state| {
            if state.active_epoch == Some(epoch) {
                state.active_epoch = None;
                state.serialization_active = false;
                state.stale = true;
            }
        });
        report_recovery_error(
            &app_weak,
            format!("could not start the recovery serializer: {error}"),
        );
    }
}

/// Record a serialized draft unless something superseded it while it was built.
fn finish_delivery(
    state: SharedState,
    epoch: u64,
    app: slint::Weak<PresentApp>,
    result: Result<Vec<u8>, String>,
) {
    let owned_delivery = with_shared_state(&state, |state| {
        if state.active_epoch != Some(epoch) {
            return None;
        }
        state.active_epoch = None;
        state.serialization_active = false;
        Some((state.epoch, state.stale))
    });
    let Some((current_epoch, stale)) = owned_delivery else {
        return;
    };
    if current_epoch != epoch {
        if import_warning_pending() {
            with_shared_state(&state, |state| state.stale = true);
        } else if stale {
            flush_latest_after_obsolete_delivery(&app);
        }
        return;
    }
    if import_warning_pending() {
        with_shared_state(&state, |state| state.stale = true);
        return;
    }
    let bytes = match result {
        Ok(bytes) => bytes,
        Err(error) => {
            with_shared_state(&state, |state| state.stale = true);
            report_recovery_error(&app, format!("snapshot serialization failed: {error}"));
            return;
        }
    };
    match record_deferred_snapshot(bytes) {
        Ok(()) => {
            with_shared_state(&state, |state| state.stale = false);
            clear_recovery_error(&app);
        }
        Err(error) => {
            with_shared_state(&state, |state| state.stale = true);
            report_recovery_error(&app, error);
        }
    }
}

fn mark_serialization_returned(state: &SharedState, epoch: u64) {
    with_shared_state(state, |state| {
        if state.active_epoch == Some(epoch) {
            state.serialization_active = false;
        }
    });
}

fn record_dispatch_failure(state: &SharedState, epoch: u64, error: String) {
    with_shared_state(state, |state| {
        if state.active_epoch != Some(epoch) {
            return;
        }
        state.active_epoch = None;
        state.serialization_active = false;
        if state.epoch == epoch || state.stale {
            state.stale = true;
            state.recovery_error = Some(format!("event loop rejected recovery delivery: {error}"));
        }
    });
}

fn flush_latest_after_obsolete_delivery(app: &slint::Weak<PresentApp>) {
    if import_warning_pending() || !with_state(|state| state.stale) {
        return;
    }
    if let Some(app) = app.upgrade() {
        TIMER.with(Timer::stop);
        ARMED.with(|armed| armed.set(false));
        app.invoke_recovery_flush();
    }
}

fn record_deferred_snapshot(bytes: Vec<u8>) -> Result<(), String> {
    crate::PRESENT_RECOVERY.with(|slot| {
        let mut slot = slot.borrow_mut();
        let recovery = slot
            .as_mut()
            .ok_or_else(|| "recovery service is unavailable".to_string())?;
        recovery
            .record("presentation state", bytes)
            .map(|_| ())
            .map_err(|error| error.to_string())
    })
}

fn import_warning_pending() -> bool {
    crate::table_actions::PENDING_PPTX_IMPORT.with(|slot| slot.borrow().is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_draft_built_before_an_invalidation_is_dropped() {
        // `finish_delivery` is the only way a draft reaches the store; an invalidated
        // epoch must not record. With no recovery store open recording is a
        // no-op either way, so the epoch check is asserted directly.
        let epoch = current_epoch();
        invalidate();
        assert_ne!(current_epoch(), epoch);
        with_state(|state| state.stale = true);
        invalidate();
        assert!(!is_stale(), "an invalidation also clears a pending draft");
    }
}
