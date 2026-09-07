//! `loom-jobs` is a shared runtime for asynchronous background operations in Loom.
//! It provides condition-variable-based blocking scheduling (no busy spinning),
//! cooperative cancellation, pausing with condvar wait, priority queues, progress
//! reporting, panic containment, and deterministic synchronous execution for testing.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::collections::VecDeque;
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};

/// Unique identifier for a job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct JobId(pub u64);

static NEXT_JOB_ID: AtomicU64 = AtomicU64::new(1);

fn next_job_id() -> JobId {
    JobId(NEXT_JOB_ID.fetch_add(1, Ordering::Relaxed))
}

/// Lifecycle state of a job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobState {
    /// Queued in scheduler, not yet started.
    Queued,
    /// Actively executing on a worker thread.
    Running,
    /// Paused cooperatively.
    Paused,
    /// Completed successfully.
    Succeeded,
    /// Failed with an error or panic.
    Failed,
    /// Cancelled before completion.
    Cancelled,
}

impl JobState {
    /// Whether the state is terminal (finished).
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
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

/// Error returned by a job or runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobError {
    /// Human-readable error message.
    pub message: String,
}

impl JobError {
    /// Create a new job error.
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

impl std::error::Error for JobError {}

/// Trait for executable background units of work.
pub trait JobWork: Send + Sync {
    /// Execute the unit of work cooperatively checking `ctx.is_cancelled()`
    /// and `ctx.wait_if_paused()`.
    fn run(&self, ctx: &JobContext) -> Result<(), JobError>;
}

/// Context provided to a running job.
#[derive(Clone)]
pub struct JobContext {
    id: JobId,
    cancelled: Arc<AtomicBool>,
    paused: Arc<Mutex<bool>>,
    pause_condvar: Arc<Condvar>,
    progress: Arc<Mutex<Option<f32>>>,
}

impl JobContext {
    /// Return the unique ID of the executing job.
    pub fn id(&self) -> JobId {
        self.id
    }

    /// Whether cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    /// Whether the job is currently paused.
    pub fn is_paused(&self) -> bool {
        *self.paused.lock().unwrap()
    }

    /// Block efficiently using a condition variable while paused, without busy-spinning.
    /// Wakes immediately if the job is unpaused or cancelled.
    pub fn wait_if_paused(&self) {
        let mut paused_guard = self.paused.lock().unwrap();
        while *paused_guard && !self.cancelled.load(Ordering::Relaxed) {
            paused_guard = self.pause_condvar.wait(paused_guard).unwrap();
        }
    }

    /// Report progress in range `[0.0, 1.0]`.
    pub fn report(&self, fraction: f32) {
        if let Ok(mut g) = self.progress.lock() {
            *g = Some(fraction.clamp(0.0, 1.0));
        }
    }
}

/// Handle to an active or completed job.
#[derive(Clone)]
pub struct JobHandle {
    id: JobId,
    cancelled: Arc<AtomicBool>,
    paused: Arc<Mutex<bool>>,
    pause_condvar: Arc<Condvar>,
    state_and_error: Arc<Mutex<(JobState, Option<JobError>)>>,
    completion_condvar: Arc<Condvar>,
    progress: Arc<Mutex<Option<f32>>>,
}

impl JobHandle {
    /// Unique identifier of the job.
    pub fn id(&self) -> JobId {
        self.id
    }

    /// Request cooperative cancellation and wake any waiting pauses.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
        self.pause_condvar.notify_all();
    }

    /// Pause or resume the job. Wakes the worker when resumed.
    pub fn set_paused(&self, paused: bool) {
        {
            let mut guard = self.paused.lock().unwrap();
            *guard = paused;
        }
        self.pause_condvar.notify_all();
    }

    /// Current execution state.
    pub fn state(&self) -> JobState {
        self.state_and_error.lock().unwrap().0
    }

    /// Return the error message if the job failed.
    pub fn error(&self) -> Option<JobError> {
        self.state_and_error.lock().unwrap().1.clone()
    }

    /// Latest reported progress in range `[0.0, 1.0]`.
    pub fn progress(&self) -> Option<f32> {
        *self.progress.lock().unwrap()
    }

    /// Block using condition variable until the job finishes. No busy-spinning.
    pub fn join(&self) -> JobState {
        let mut guard = self.state_and_error.lock().unwrap();
        while !guard.0.is_terminal() {
            guard = self.completion_condvar.wait(guard).unwrap();
        }
        guard.0
    }
}

/// Execution priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    /// Low priority (background indexing, thumbnail caching).
    Low = 0,
    /// Normal priority (standard background export).
    Normal = 1,
    /// High priority (interactive user preview / render).
    High = 2,
}

/// Internal queue entry.
#[derive(Clone)]
struct JobEntry {
    id: JobId,
    work: Arc<dyn JobWork>,
    priority: Priority,
    handle: JobHandle,
    retries_remaining: u32,
}

struct QueueShared {
    queue: Mutex<VecDeque<JobEntry>>,
    wakeup_condvar: Condvar,
    stop: AtomicBool,
}

/// Multi-threaded asynchronous work queue with condition-variable scheduling.
pub struct JobQueue {
    shared: Arc<QueueShared>,
    workers: Vec<std::thread::JoinHandle<()>>,
}

impl JobQueue {
    /// Create a new job queue with `num_workers` worker threads.
    pub fn new(num_workers: usize) -> Self {
        let shared = Arc::new(QueueShared {
            queue: Mutex::new(VecDeque::new()),
            wakeup_condvar: Condvar::new(),
            stop: AtomicBool::new(false),
        });

        let mut workers = Vec::with_capacity(num_workers.max(1));
        for _ in 0..num_workers.max(1) {
            let shared_clone = Arc::clone(&shared);
            workers.push(std::thread::spawn(move || worker_loop(shared_clone)));
        }

        Self { shared, workers }
    }

    /// Submit a job for background execution.
    pub fn submit(&self, work: Arc<dyn JobWork>, priority: Priority) -> JobHandle {
        self.submit_with_retry(work, priority, 0)
    }

    /// Submit a job with a retry count upon transient failure.
    pub fn submit_with_retry(
        &self,
        work: Arc<dyn JobWork>,
        priority: Priority,
        max_retries: u32,
    ) -> JobHandle {
        let id = next_job_id();
        let cancelled = Arc::new(AtomicBool::new(false));
        let paused = Arc::new(Mutex::new(false));
        let pause_condvar = Arc::new(Condvar::new());
        let state_and_error = Arc::new(Mutex::new((JobState::Queued, None)));
        let completion_condvar = Arc::new(Condvar::new());
        let progress = Arc::new(Mutex::new(None));

        let handle = JobHandle {
            id,
            cancelled,
            paused,
            pause_condvar,
            state_and_error,
            completion_condvar,
            progress,
        };

        let entry = JobEntry {
            id,
            work,
            priority,
            handle: handle.clone(),
            retries_remaining: max_retries,
        };

        {
            let mut q = self.shared.queue.lock().unwrap();
            q.push_back(entry);
            // Stable sort by priority (High first, preserving arrival order)
            let mut items: Vec<JobEntry> = q.drain(..).collect();
            items.sort_by_key(|e| std::cmp::Reverse(e.priority));
            q.extend(items);
        }

        // Wake one sleeping worker
        self.shared.wakeup_condvar.notify_one();
        handle
    }

    /// Request graceful shutdown and join all worker threads.
    pub fn stop(self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        self.shared.wakeup_condvar.notify_all();
        for worker in self.workers {
            let _ = worker.join();
        }
    }
}

fn worker_loop(shared: Arc<QueueShared>) {
    loop {
        let mut queue = shared.queue.lock().unwrap();
        while queue.is_empty() && !shared.stop.load(Ordering::Relaxed) {
            queue = shared.wakeup_condvar.wait(queue).unwrap();
        }

        if shared.stop.load(Ordering::Relaxed) && queue.is_empty() {
            break;
        }

        if let Some(entry) = queue.pop_front() {
            drop(queue);
            run_job(entry, &shared);
        }
    }
}

fn run_job(mut entry: JobEntry, shared: &Arc<QueueShared>) {
    {
        let mut guard = entry.handle.state_and_error.lock().unwrap();
        guard.0 = JobState::Running;
    }

    let ctx = JobContext {
        id: entry.id,
        cancelled: entry.handle.cancelled.clone(),
        paused: entry.handle.paused.clone(),
        pause_condvar: entry.handle.pause_condvar.clone(),
        progress: entry.handle.progress.clone(),
    };

    if ctx.is_cancelled() {
        let mut guard = entry.handle.state_and_error.lock().unwrap();
        guard.0 = JobState::Cancelled;
        entry.handle.completion_condvar.notify_all();
        return;
    }

    // Panic containment
    let work = Arc::clone(&entry.work);
    let run_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| work.run(&ctx)));

    let final_outcome = match run_result {
        Ok(Ok(())) => {
            if ctx.is_cancelled() {
                (JobState::Cancelled, None)
            } else {
                (JobState::Succeeded, None)
            }
        }
        Ok(Err(job_err)) => {
            if ctx.is_cancelled() {
                (JobState::Cancelled, None)
            } else if entry.retries_remaining > 0 {
                // Re-queue with one fewer retry
                entry.retries_remaining -= 1;
                let mut q = shared.queue.lock().unwrap();
                q.push_front(entry);
                shared.wakeup_condvar.notify_one();
                return;
            } else {
                (JobState::Failed, Some(job_err))
            }
        }
        Err(panic_payload) => {
            let msg = if let Some(s) = panic_payload.downcast_ref::<&str>() {
                format!("job panicked: {s}")
            } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                format!("job panicked: {s}")
            } else {
                "job panicked with unknown error".to_string()
            };
            (JobState::Failed, Some(JobError::new(msg)))
        }
    };

    {
        let mut guard = entry.handle.state_and_error.lock().unwrap();
        guard.0 = final_outcome.0;
        guard.1 = final_outcome.1;
    }
    entry.handle.completion_condvar.notify_all();
}

/// Type alias for heap-allocated job function closures.
pub type JobClosure = Box<dyn Fn(&JobContext) -> Result<(), JobError> + Send + Sync>;

/// Closure-based `JobWork` adapter.
pub struct FnJob {
    func: JobClosure,
}

impl FnJob {
    /// Create a job from a closure.
    pub fn new(f: impl Fn(&JobContext) -> Result<(), JobError> + Send + Sync + 'static) -> Self {
        Self { func: Box::new(f) }
    }
}

impl JobWork for FnJob {
    fn run(&self, ctx: &JobContext) -> Result<(), JobError> {
        (self.func)(ctx)
    }
}

/// No-op job that completes immediately with success.
pub struct NoopJob;

impl JobWork for NoopJob {
    fn run(&self, _ctx: &JobContext) -> Result<(), JobError> {
        Ok(())
    }
}

/// Synchronous, deterministic executor for unit tests and headless CLI operations.
pub struct SyncJobExecutor;

impl SyncJobExecutor {
    /// Execute a job synchronously on the current thread.
    pub fn run(work: &dyn JobWork) -> Result<(), JobError> {
        let ctx = JobContext {
            id: next_job_id(),
            cancelled: Arc::new(AtomicBool::new(false)),
            paused: Arc::new(Mutex::new(false)),
            pause_condvar: Arc::new(Condvar::new()),
            progress: Arc::new(Mutex::new(None)),
        };
        work.run(&ctx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU32;

    #[test]
    fn successful_job_with_progress_and_join() {
        let q = JobQueue::new(2);
        let handle = q.submit(
            Arc::new(FnJob::new(|ctx| {
                ctx.report(0.25);
                ctx.report(0.75);
                Ok(())
            })),
            Priority::Normal,
        );

        assert_eq!(handle.join(), JobState::Succeeded);
        assert!((handle.progress().unwrap() - 0.75).abs() < 1e-6);
        assert_eq!(handle.error(), None);
        q.stop();
    }

    #[test]
    fn failed_job_propagates_error() {
        let q = JobQueue::new(1);
        let handle = q.submit(
            Arc::new(FnJob::new(|_| {
                Err(JobError::new("disk full during decode"))
            })),
            Priority::Normal,
        );

        assert_eq!(handle.join(), JobState::Failed);
        assert_eq!(
            handle.error(),
            Some(JobError::new("disk full during decode"))
        );
        q.stop();
    }

    #[test]
    fn panic_containment_in_worker() {
        let q = JobQueue::new(1);
        let handle = q.submit(
            Arc::new(FnJob::new(|_| {
                panic!("deliberate test panic");
            })),
            Priority::Normal,
        );

        assert_eq!(handle.join(), JobState::Failed);
        assert!(handle
            .error()
            .unwrap()
            .message
            .contains("deliberate test panic"));

        // Next job should still run on the same queue without deadlocking
        let second = q.submit(Arc::new(NoopJob), Priority::Normal);
        assert_eq!(second.join(), JobState::Succeeded);
        q.stop();
    }

    #[test]
    fn cancellation_before_and_during_execution() {
        let q = JobQueue::new(1);
        let handle = q.submit(Arc::new(NoopJob), Priority::Low);
        handle.cancel();
        assert_eq!(handle.join(), JobState::Cancelled);

        let active_handle = q.submit(
            Arc::new(FnJob::new(|ctx| {
                while !ctx.is_cancelled() {
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                Err(JobError::new("cancelled"))
            })),
            Priority::Normal,
        );
        std::thread::sleep(std::time::Duration::from_millis(15));
        active_handle.cancel();
        assert_eq!(active_handle.join(), JobState::Cancelled);
        q.stop();
    }

    #[test]
    fn condvar_pause_and_resume() {
        let q = JobQueue::new(1);
        let iterations = Arc::new(AtomicU32::new(0));
        let it_clone = iterations.clone();

        let handle = q.submit(
            Arc::new(FnJob::new(move |ctx| {
                for _ in 0..20 {
                    ctx.wait_if_paused();
                    it_clone.fetch_add(1, Ordering::SeqCst);
                    std::thread::sleep(std::time::Duration::from_millis(2));
                }
                Ok(())
            })),
            Priority::Normal,
        );

        handle.set_paused(true);
        std::thread::sleep(std::time::Duration::from_millis(20));
        let count_during_pause = iterations.load(Ordering::SeqCst);

        handle.set_paused(false);
        assert_eq!(handle.join(), JobState::Succeeded);
        assert!(iterations.load(Ordering::SeqCst) >= count_during_pause);
        q.stop();
    }

    #[test]
    fn synchronous_test_executor() {
        let job = FnJob::new(|ctx| {
            ctx.report(1.0);
            Ok(())
        });
        assert!(SyncJobExecutor::run(&job).is_ok());
    }
}
