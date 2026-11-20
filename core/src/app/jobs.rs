//! Job runner: one worker thread per long action (ADR 0004).
//!
//! A job is persisted in `JobDb`, reports through the `EventBus`, can be cancelled between
//! steps, and never stays "Running" forever: a panic is caught, and a job cut off by closing
//! the app is marked `Failed` the next time the services open.

use super::views::{
    JobFinished, JobLog, JobProgress, JobStarted, JobView, ProgressView, SessionView,
};
use super::{
    AppError, AppServices, EVENT_JOB_FINISHED, EVENT_JOB_LOG, EVENT_JOB_PROGRESS, EVENT_JOB_STARTED,
};
use crate::events::{EventBus, EventHandler};
use crate::jobs::{Job, JobState};
use crate::persistence::JobDb;
use std::any::Any;
use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use uuid::Uuid;

/// At most this many `job.log` events per second per job; the full log stays in `JobDb`.
const LOG_EVENTS_PER_SECOND: u32 = 20;
/// `JobView.log` carries this many of the latest lines.
const LOG_TAIL: usize = 200;
/// Written on jobs that were still `Running` when the app closed.
pub const INTERRUPTED: &str = "interrupted (app closed)";

/// Called with the job id and step name at every step boundary, before the cancel check.
/// Tests use it to pause a job deterministically (no sleeps) or to make it panic.
pub type StepHook = Arc<dyn Fn(Uuid, &str) + Send + Sync>;

/// Set by [`AppServices::cancel`], checked by the job at step boundaries.
#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// State every worker thread shares with the services.
pub struct Shared {
    pub bus: Arc<EventBus>,
    pub db: Mutex<JobDb>,
    pub running: Mutex<HashMap<Uuid, Arc<JobCtx>>>,
    pub hook: Mutex<Option<StepHook>>,
    /// Open `job.approval` questions, answered through `AppServices::decide`.
    pub approvals: Mutex<HashMap<Uuid, Sender<bool>>>,
}

impl Shared {
    #[must_use]
    pub fn new(db: JobDb) -> Self {
        Self {
            bus: EventBus::new(),
            db: Mutex::new(db),
            running: Mutex::new(HashMap::new()),
            hook: Mutex::new(None),
            approvals: Mutex::new(HashMap::new()),
        }
    }

    fn save(&self, job: &Job) -> Result<(), AppError> {
        self.db.lock().expect("job db lock").insert(job)?;
        Ok(())
    }

    /// Marks every job left `Running` by a previous run as `Failed`.
    ///
    /// # Errors
    /// Returns [`AppError::Db`] when the job database cannot be read or written.
    pub fn fail_stale_jobs(&self) -> Result<(), AppError> {
        let db = self.db.lock().expect("job db lock");
        for mut job in db.by_state(JobState::Running)? {
            job.log_line(INTERRUPTED);
            job.output = Some(serde_json::json!({ "error": INTERRUPTED }));
            job.mark_failed();
            db.insert(&job)?;
        }
        drop(db);
        Ok(())
    }
}

/// Keeps `job.log` events under [`LOG_EVENTS_PER_SECOND`].
struct LogLimiter {
    window: Instant,
    sent: u32,
}

impl LogLimiter {
    fn new() -> Self {
        Self {
            window: Instant::now(),
            sent: 0,
        }
    }

    fn allow(&mut self) -> bool {
        let now = Instant::now();
        if now.duration_since(self.window) >= Duration::from_secs(1) {
            self.window = now;
            self.sent = 0;
        }
        if self.sent < LOG_EVENTS_PER_SECOND {
            self.sent += 1;
            true
        } else {
            false
        }
    }
}

struct CtxState {
    job: Job,
    progress: Option<ProgressView>,
    limiter: LogLimiter,
}

/// What a job body sees: progress, log, and the cancel check.
pub struct JobCtx {
    id: Uuid,
    bus: Arc<EventBus>,
    hook: Option<StepHook>,
    cancel: CancelToken,
    state: Mutex<CtxState>,
}

impl JobCtx {
    #[must_use]
    pub const fn id(&self) -> Uuid {
        self.id
    }

    #[must_use]
    pub fn cancel_token(&self) -> CancelToken {
        self.cancel.clone()
    }

    /// Fails with [`AppError::Cancelled`] when the person pressed cancel.
    ///
    /// # Errors
    /// Returns [`AppError::Cancelled`] once cancel was requested.
    pub fn check_cancelled(&self) -> Result<(), AppError> {
        if self.cancel.is_cancelled() {
            Err(AppError::Cancelled)
        } else {
            Ok(())
        }
    }

    /// A step boundary: runs the test hook, honours cancel, then reports progress.
    ///
    /// # Errors
    /// Returns [`AppError::Cancelled`] once cancel was requested.
    pub fn step(&self, name: &str, done: u32, total: u32) -> Result<(), AppError> {
        if let Some(hook) = &self.hook {
            hook(self.id, name);
        }
        self.check_cancelled()?;
        self.progress(name, done, total);
        Ok(())
    }

    /// Reports progress without a cancel check (for the last step, after the work is done).
    pub fn progress(&self, name: &str, done: u32, total: u32) {
        {
            let mut st = self.state.lock().expect("job state lock");
            name.clone_into(&mut st.job.step);
            st.progress = Some(ProgressView { done, total });
        }
        self.publish(
            EVENT_JOB_PROGRESS,
            &JobProgress {
                job: self.id,
                step: name.to_owned(),
                done,
                total,
            },
        );
    }

    /// Appends a line to the job log; the event is skipped when the job logs too fast.
    pub fn log(&self, line: &str) {
        let send = {
            let mut st = self.state.lock().expect("job state lock");
            st.job.log_line(line);
            st.limiter.allow()
        };
        if send {
            self.publish(
                EVENT_JOB_LOG,
                &JobLog {
                    job: self.id,
                    line: line.to_owned(),
                },
            );
        }
    }

    fn publish<T: serde::Serialize>(&self, kind: &str, payload: &T) {
        self.bus.publish(
            kind,
            serde_json::to_value(payload).unwrap_or(serde_json::Value::Null),
        );
    }

    fn view(&self) -> JobView {
        let st = self.state.lock().expect("job state lock");
        job_view(&st.job, st.progress.clone())
    }
}

fn job_view(job: &Job, progress: Option<ProgressView>) -> JobView {
    let skip = job.log.len().saturating_sub(LOG_TAIL);
    let error = if job.state == JobState::Failed {
        job.output
            .as_ref()
            .and_then(|o| o.get("error"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
    } else {
        None
    };
    let result = if job.state == JobState::Succeeded {
        job.output.clone()
    } else {
        None
    };
    JobView {
        id: job.id,
        session: job.session_id,
        step: job.step.clone(),
        state: job.state,
        progress,
        log: job.log[skip..].to_vec(),
        result,
        error,
    }
}

fn panic_message(p: &(dyn Any + Send)) -> String {
    p.downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| p.downcast_ref::<String>().cloned())
        .map_or_else(|| "the job panicked".to_owned(), |m| format!("panic: {m}"))
}

type Outcome = Result<Result<serde_json::Value, AppError>, Box<dyn Any + Send>>;

/// Saves the final state, then tells the window. The order matters: a listener that sees
/// `job.finished` can already read the stored job.
fn finish(shared: &Shared, ctx: &JobCtx, outcome: Outcome) {
    let (state, result, error) = match outcome {
        Ok(Ok(v)) => (JobState::Succeeded, Some(v), None),
        Ok(Err(AppError::Cancelled)) => (JobState::Cancelled, None, None),
        Ok(Err(e)) => (JobState::Failed, None, Some(e.to_string())),
        Err(p) => (JobState::Failed, None, Some(panic_message(&*p))),
    };
    let job = {
        let mut st = ctx.state.lock().expect("job state lock");
        st.job.state = state;
        if let Some(e) = &error {
            st.job.log_line(&format!("error: {e}"));
            st.job.output = Some(serde_json::json!({ "error": e }));
        } else if state == JobState::Succeeded {
            st.job.output.clone_from(&result);
        }
        st.job.clone()
    };
    // The event is the primary channel; if this write fails the row stays `Running` and the
    // next `AppServices::open` repairs it as interrupted.
    let _ = shared.save(&job);
    shared.running.lock().expect("running lock").remove(&ctx.id);
    ctx.publish(
        EVENT_JOB_FINISHED,
        &JobFinished {
            job: ctx.id,
            state,
            result,
            error,
        },
    );
    // Handlers already saw the events; nothing reads the queue, so do not let it grow.
    drop(shared.bus.drain());
}

impl AppServices {
    /// Listens to every job event (the desktop shell forwards them to the window).
    pub fn subscribe(&self, handler: Arc<dyn EventHandler>) {
        self.shared.bus.subscribe(handler);
    }

    /// Installs a [`StepHook`] for jobs started afterwards. Meant for tests.
    #[doc(hidden)]
    pub fn set_step_hook(&self, hook: StepHook) {
        *self.shared.hook.lock().expect("hook lock") = Some(hook);
    }

    /// Runs `body` on a worker thread as a persisted, cancellable job and returns its id at once.
    pub(super) fn start_job<F>(&self, step: &str, body: F) -> Result<Uuid, AppError>
    where
        F: FnOnce(&JobCtx) -> Result<serde_json::Value, AppError> + Send + 'static,
    {
        let session = Uuid::new_v4();
        let mut job = Job::new(session, step);
        job.mark_running();
        let ctx = Arc::new(JobCtx {
            id: job.id,
            bus: Arc::clone(&self.shared.bus),
            hook: self.shared.hook.lock().expect("hook lock").clone(),
            cancel: CancelToken::new(),
            state: Mutex::new(CtxState {
                job: job.clone(),
                progress: None,
                limiter: LogLimiter::new(),
            }),
        });
        self.shared.save(&job)?;
        self.shared
            .running
            .lock()
            .expect("running lock")
            .insert(ctx.id, Arc::clone(&ctx));
        ctx.publish(
            EVENT_JOB_STARTED,
            &JobStarted {
                job: ctx.id,
                session,
                step: step.to_owned(),
            },
        );
        let shared = Arc::clone(&self.shared);
        let worker = Arc::clone(&ctx);
        let spawned = std::thread::Builder::new()
            .name("ummp-job".into())
            .spawn(move || {
                let outcome = catch_unwind(AssertUnwindSafe(|| body(&worker)));
                finish(&shared, &worker, outcome);
            });
        if let Err(e) = spawned {
            self.shared
                .running
                .lock()
                .expect("running lock")
                .remove(&ctx.id);
            return Err(AppError::Io(e));
        }
        Ok(ctx.id)
    }

    /// The job's state, step, progress, last log lines and outcome.
    ///
    /// # Errors
    /// Returns [`AppError::NotFound`] for an unknown id.
    pub fn job(&self, id: Uuid) -> Result<JobView, AppError> {
        let live = self
            .shared
            .running
            .lock()
            .expect("running lock")
            .get(&id)
            .cloned();
        if let Some(ctx) = live {
            return Ok(ctx.view());
        }
        self.shared
            .db
            .lock()
            .expect("job db lock")
            .by_id(id)?
            .map(|j| job_view(&j, None))
            .ok_or_else(|| AppError::NotFound(format!("job {id}")))
    }

    /// The latest `limit` jobs, newest first, for the Home screen.
    ///
    /// # Errors
    /// Returns [`AppError::Db`] when the job database cannot be read.
    pub fn sessions(&self, limit: usize) -> Result<Vec<SessionView>, AppError> {
        let jobs = self.shared.db.lock().expect("job db lock").recent(limit)?;
        Ok(jobs
            .iter()
            .map(|j| {
                let v = job_view(j, None);
                SessionView {
                    id: v.id,
                    state: v.state,
                    step: v.step,
                    output: v
                        .result
                        .as_ref()
                        .and_then(|r| r.get("output"))
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_owned),
                    error: v.error,
                }
            })
            .collect())
    }

    /// Asks a running job to stop at its next step boundary. A finished job is left alone.
    ///
    /// # Errors
    /// Returns [`AppError::NotFound`] for an unknown id.
    pub fn cancel(&self, id: Uuid) -> Result<(), AppError> {
        let live = self
            .shared
            .running
            .lock()
            .expect("running lock")
            .get(&id)
            .cloned();
        if let Some(ctx) = live {
            ctx.cancel.cancel();
            ctx.log("cancel requested; stopping after the current step");
            return Ok(());
        }
        self.job(id).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limiter_lets_at_most_twenty_through_per_second() {
        let mut l = LogLimiter::new();
        let allowed = (0..10_000).filter(|_| l.allow()).count();
        // Ten thousand lines in well under a second: only the first window's worth is sent.
        assert!(allowed <= 2 * LOG_EVENTS_PER_SECOND as usize, "{allowed}");
        assert!(allowed >= 1);
    }
}
