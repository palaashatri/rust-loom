//! `loom-jobs` is a shared framework for asynchronous work that supports
//! progress, cancellation, priority, and observation without any UI coupling.
//!
//! The framework is deliberately dependency-free and deterministic so it can
//! run in headless CI and Docker. Long-running Loom work (media decoding,
//! export, model inference, thumbnail generation) runs through jobs.

use std::collections::VecDeque;
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Unique job id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct JobId(pub u64);

static NEXT_JOB_ID: AtomicU64 = AtomicU64::new(1);

fn next_job_id() -> JobId {
    JobId(NEXT_JOB_ID.fetch_add(1, Ordering::Relaxed))
}

/// Job state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobState {
    /// Queued, not started.
    Queued,
    /// Running.
    Running,
    /// Paused.
    Paused,
    /// Completed successfully.
    Succeeded,
    /// Failed with an error.
    Failed,
    /// Cancelled before completion.
    Cancelled,
}

impl fmt::Display for JobState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Paused => "paused",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        };
        f.write_str(s)
    }
}

/// A cooperatively-cancellable unit of work.
pub trait JobWork: Send + Sync {
    /// Progress must call `[ProgressSink::report]` repeatedly and check
    /// `[ProgressSink::is_cancelled]`. Must respect pause where supported.
    fn run(&self, ctx: &JobContext) -> Result<(), JobError>;
}

/// Error returned by a job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobError {
    /// Human-readable message.
    pub message: String,
}

impl JobError {
    /// Create a job error.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for JobError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl core::error::Error for JobError {}

/// Context passed to a running job.
#[derive(Clone)]
pub struct JobContext {
    id: JobId,
    cancelled: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    progress: Arc<Mutex<Option<f32>>>,
}

impl JobContext {
    /// Whether cancellation was requested.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    /// Whether paused.
    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }

    /// Block while paused, returning early if cancelled.
    ///
    /// This is a best-effort cooperative pause; a job that iterates quickly
    /// calls this each iteration.
    pub fn wait_if_paused(&self) {
        while self.paused.load(Ordering::Relaxed) && !self.cancelled.load(Ordering::Relaxed) {
            std::thread::yield_now();
        }
    }

    /// Report progress in `[0,1]`.
    pub fn report(&self, f: f32) {
        if let Ok(mut g) = self.progress.lock() {
            *g = Some(f.clamp(0.0, 1.0));
        }
    }

    /// Job id.
    pub fn id(&self) -> JobId {
        self.id
    }
}

/// A submitted job handle.
#[derive(Clone)]
pub struct JobHandle {
    id: JobId,
    cancelled: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    state: Arc<Mutex<JobState>>,
    progress: Arc<Mutex<Option<f32>>>,
    joined: Arc<AtomicU64>, // 0 not done, 1 done
}

impl JobHandle {
    /// Request cancellation.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    /// Toggle pause.
    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Relaxed);
    }

    /// Current state.
    pub fn state(&self) -> JobState {
        *self.state.lock().unwrap()
    }

    /// Latest progress in `[0,1]`.
    pub fn progress(&self) -> Option<f32> {
        *self.progress.lock().unwrap()
    }

    /// Block until finished (for use in tests / CLI). Returns final state.
    pub fn join(&self) -> JobState {
        while self.joined.load(Ordering::Relaxed) == 0 {
            std::thread::yield_now();
        }
        self.state()
    }

    /// Id.
    pub fn id(&self) -> JobId {
        self.id
    }
}

/// Priority for scheduling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    /// Low priority (background).
    Low = 0,
    /// Normal.
    Normal = 1,
    /// High (user-interactive).
    High = 2,
}

/// A job in the queue.
#[derive(Clone)]
pub struct JobEntry {
    id: JobId,
    work: Arc<dyn JobWork>,
    priority: Priority,
    handle: JobHandle,
}

/// A simple work queue that executes jobs on a worker thread pool.
///
/// The queue materializes its own worker threads so it can operate without a
/// global async runtime and is deterministic testable.
pub struct JobQueue {
    inner: Arc<QueueInner>,
    workers: Vec<std::thread::JoinHandle<()>>,
}

struct QueueInner {
    queue: Mutex<VecDeque<JobEntry>>,
    stop: AtomicBool,
}

impl JobQueue {
    /// Create a queue with `workers` threads.
    pub fn new(workers: usize) -> Self {
        let inner = Arc::new(QueueInner {
            queue: Mutex::new(VecDeque::new()),
            stop: AtomicBool::new(false),
        });
        let mut handles = Vec::with_capacity(workers);
        for _ in 0..workers.max(1) {
            let inner = Arc::clone(&inner);
            handles.push(std::thread::spawn(move || worker_loop(inner)));
        }
        Self {
            inner,
            workers: handles,
        }
    }

    /// Submit a job and return its handle.
    pub fn submit(&self, work: Arc<dyn JobWork>, priority: Priority) -> JobHandle {
        let id = next_job_id();
        let cancelled = Arc::new(AtomicBool::new(false));
        let paused = Arc::new(AtomicBool::new(false));
        let state = Arc::new(Mutex::new(JobState::Queued));
        let progress = Arc::new(Mutex::new(None));
        let joined = Arc::new(AtomicU64::new(0));
        let handle = JobHandle {
            id,
            cancelled,
            paused,
            state,
            progress,
            joined,
        };
        let entry = JobEntry {
            id,
            work,
            priority,
            handle: handle.clone(),
        };
        let mut q = self.inner.queue.lock().unwrap();
        q.push_back(entry);
        // Re-sort by priority (stable: keep arrival order within same priority).
        // Simple insertion: pop and reinsert in order.
        let mut items: Vec<JobEntry> = q.drain(..).collect();
        items.sort_by_key(|e| std::cmp::Reverse(e.priority));
        q.extend(items);
        handle
    }

    /// Request graceful stop (waits for running job to finish within a timeout).
    pub fn stop(self) {
        self.inner.stop.store(true, Ordering::Relaxed);
        for h in self.workers {
            let _ = h.join();
        }
    }
}

fn worker_loop(inner: Arc<QueueInner>) {
    loop {
        if inner.stop.load(Ordering::Relaxed) && inner.queue.lock().unwrap().is_empty() {
            break;
        }
        let next = {
            let mut q = inner.queue.lock().unwrap();
            q.pop_front()
        };
        match next {
            Some(entry) => run_job(entry),
            None => {
                if inner.stop.load(Ordering::Relaxed) {
                    break;
                }
                std::thread::yield_now();
            }
        }
    }
}

fn run_job(entry: JobEntry) {
    {
        let mut s = entry.handle.state.lock().unwrap();
        *s = JobState::Running;
    }
    let ctx = JobContext {
        id: entry.id,
        cancelled: entry.handle.cancelled.clone(),
        paused: entry.handle.paused.clone(),
        progress: entry.handle.progress.clone(),
    };
    let result = if ctx.is_cancelled() {
        Err(JobError::new("cancelled before start"))
    } else if entry.work.run(&ctx).is_ok() {
        Ok(())
    } else {
        Err(JobError::new("job failed"))
    };
    match result {
        Ok(()) => {
            let mut s = entry.handle.state.lock().unwrap();
            if entry.handle.cancelled.load(Ordering::Relaxed) {
                *s = JobState::Cancelled;
            } else {
                *s = JobState::Succeeded;
            }
        }
        Err(e) => {
            let mut s = entry.handle.state.lock().unwrap();
            if entry.handle.cancelled.load(Ordering::Relaxed) {
                *s = JobState::Cancelled;
            } else {
                *s = JobState::Failed;
            }
            let _ = e;
        }
    }
    entry.handle.joined.store(1, Ordering::Relaxed);
}

/// A `JobWork` wrapper for a function.
pub struct FnJob {
    f: JobFn,
}

/// Boxed job function stored by [`FnJob`].
type JobFn = Box<dyn Fn(&JobContext) -> Result<(), JobError> + Send + Sync>;

impl FnJob {
    /// Create a job from a closure.
    pub fn new(f: impl Fn(&JobContext) -> Result<(), JobError> + Send + Sync + 'static) -> Self {
        Self { f: Box::new(f) }
    }
}

impl JobWork for FnJob {
    fn run(&self, ctx: &JobContext) -> Result<(), JobError> {
        (self.f)(ctx)
    }
}

/// Convenience: an always-succeeding job.
pub struct NoopJob;

impl JobWork for NoopJob {
    fn run(&self, _ctx: &JobContext) -> Result<(), JobError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU32;

    #[test]
    fn successful_job() {
        let q = JobQueue::new(1);
        let handle = q.submit(
            Arc::new(FnJob::new(|ctx| {
                ctx.report(0.5);
                Ok(())
            })),
            Priority::Normal,
        );
        assert_eq!(handle.join(), JobState::Succeeded);
        assert!((handle.progress().unwrap() - 0.5).abs() < 1e-6);
        q.stop();
    }

    #[test]
    fn cancelled_before_start() {
        let q = JobQueue::new(1);
        let handle = q.submit(Arc::new(NoopJob), Priority::Low);
        handle.cancel();
        assert!(matches!(
            handle.join(),
            JobState::Cancelled | JobState::Failed
        ));
        q.stop();
    }

    #[test]
    fn priority_order() {
        let q = JobQueue::new(1);
        let order = Arc::new(Mutex::new(Vec::new()));
        // A single worker would otherwise drain the low-priority job before
        // the high-priority one is submitted, making the test racy. A gate
        // job occupies the worker until all submissions are queued, so the
        // scheduler's reorder-on-submit is the only thing under test.
        let release = Arc::new(AtomicBool::new(false));
        let r = release.clone();
        let gate = q.submit(
            Arc::new(FnJob::new(move |_| {
                while !r.load(Ordering::Relaxed) {
                    std::thread::yield_now();
                }
                Ok(())
            })),
            Priority::High,
        );
        let o = order.clone();
        let low = q.submit(
            Arc::new(FnJob::new(move |_| {
                o.lock().unwrap().push("low");
                Ok(())
            })),
            Priority::Low,
        );
        let o2 = order.clone();
        let high = q.submit(
            Arc::new(FnJob::new(move |_| {
                o2.lock().unwrap().push("high");
                Ok(())
            })),
            Priority::High,
        );
        // Push a low first to ensure they're reordered by the queue.
        let o3 = order.clone();
        let mid = q.submit(
            Arc::new(FnJob::new(move |_| {
                o3.lock().unwrap().push("mid");
                Ok(())
            })),
            Priority::Low,
        );
        release.store(true, Ordering::Relaxed);
        let _ = gate.join();
        let _ = low.join();
        let _ = high.join();
        let _ = mid.join();
        let seen = order.lock().unwrap().clone();
        assert_eq!(seen, vec!["high", "low", "mid"]);
        q.stop();
    }

    #[test]
    fn pause_yields() {
        let q = JobQueue::new(1);
        let ran = Arc::new(AtomicU32::new(0));
        let r = ran.clone();
        let handle = q.submit(
            Arc::new(FnJob::new(move |ctx| {
                for _ in 0..100 {
                    ctx.wait_if_paused();
                    if ctx.is_cancelled() {
                        return Err(JobError::new("cancelled"));
                    }
                    r.fetch_add(1, Ordering::Relaxed);
                }
                Ok(())
            })),
            Priority::Normal,
        );
        handle.set_paused(true);
        std::thread::sleep(std::time::Duration::from_millis(20));
        handle.set_paused(false);
        assert_eq!(handle.join(), JobState::Succeeded);
        assert!(ran.load(Ordering::Relaxed) >= 1);
        q.stop();
    }

    #[test]
    fn progress_bounded() {
        let q = JobQueue::new(1);
        let h = q.submit(
            Arc::new(FnJob::new(|ctx| {
                ctx.report(-0.2);
                ctx.report(1.5);
                Ok(())
            })),
            Priority::Normal,
        );
        let _ = h.join();
        let p = h.progress().unwrap();
        assert!((0.0..=1.0).contains(&p));
        q.stop();
    }
}
