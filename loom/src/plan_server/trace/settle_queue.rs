//! The settle worker: settle commits in the order they were asked for, one at a time, on a thread
//! of their own. `POST /api/settle` answers once the line is written; a commit whose hooks run for
//! minutes holds up only the settles queued behind it, and how each ended reaches the browsers as
//! a `settle_commit` message (git's and the hooks' words to the settling browser alone).
//!
//! The thread starts with the first job and ends when the queue is empty. At shutdown (a drain,
//! while every socket is still up) no further job starts, the jobs still queued end as not
//! committed, their lines written, and the commit in flight runs to its end: a server stop never
//! cuts git or the user's hooks short. Job ids lead with a token of this server, so a browser that
//! reconnects to another server never takes one of its jobs for its own.

use std::collections::VecDeque;
use std::panic::{self, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError, Weak};
use std::thread;

use crate::plan_server::context::PlanContext;
use crate::plan_server::ids::BrowserId;
use crate::plan_server::lifecycle;
use crate::plan_server::shutdown::ShutdownHook;
use crate::plan_server::wire::message::PlanMessage;
use crate::plan_server::wire::trace::SettleCommitStart;

use super::settle_commit::Limits;
use super::settle_job::{self, Outcome, SettleJob};

const STOPPING: &str = "loom plan is stopping";
const STOPPED_FIRST: &str = "loom plan stopped before the commit ran; settle again";
const PANICKED: &str = "the commit failed unexpectedly; settle again";

/// The queue of one server's settle commits.
pub(super) struct SettleQueue {
    ctx: Weak<PlanContext>,
    /// This server's token, which leads every job id.
    token: String,
    state: Mutex<State>,
    /// Signalled when the worker ends, or when shutdown stops waiting for it.
    idle: Condvar,
    stop: Arc<AtomicBool>,
    last_job: AtomicU64,
}

#[derive(Default)]
struct State {
    jobs: VecDeque<(String, SettleJob)>,
    /// Whether a worker thread is draining `jobs`.
    running: bool,
    /// Set by the shutdown abort: the drain no longer waits for the commit in flight.
    released: bool,
}

/// Install the queue the settle route hands its commits to, and its part in shutdown.
pub(super) fn install(ctx: &Arc<PlanContext>) {
    let queue = Arc::new(SettleQueue {
        ctx: Arc::downgrade(ctx),
        token: uuid::Uuid::new_v4().simple().to_string()[..8].to_owned(),
        state: Mutex::default(),
        idle: Condvar::new(),
        stop: Arc::new(AtomicBool::new(false)),
        last_job: AtomicU64::new(0),
    });
    ctx.extensions().insert(Arc::clone(&queue));
    let drain = Arc::clone(&queue);
    ctx.shutdown().register(ShutdownHook {
        name: "settle commits",
        drain: Box::new(move || drain.shut_down()),
        abort: Box::new(move || queue.release()),
    });
}

/// Queue `job` behind every earlier settle; where its commit stands, for the settle's answer.
pub(super) fn push(ctx: &PlanContext, job: SettleJob) -> SettleCommitStart {
    match ctx.extensions().get::<SettleQueue>() {
        Some(queue) => queue.push(job),
        None => SettleCommitStart::NotCommitted {
            reason: "the commit worker is not running".to_owned(),
        },
    }
}

/// `outcome` to the settling `browser`, and its shared form to every other browser.
fn announce(ctx: &PlanContext, browser: &BrowserId, outcome: Outcome) {
    let hub = ctx.hub();
    hub.send_to_browser(browser, &PlanMessage::SettleCommit(outcome.settler));
    hub.send_to_others(browser, &PlanMessage::SettleCommit(outcome.others));
}

impl SettleQueue {
    fn push(self: &Arc<Self>, job: SettleJob) -> SettleCommitStart {
        let number = self.last_job.fetch_add(1, Ordering::Relaxed) + 1;
        let id = format!("{}-{number}", self.token);
        let mut state = self.lock();
        if self.stop.load(Ordering::SeqCst) {
            let reason = STOPPING.to_owned();
            return SettleCommitStart::NotCommitted { reason };
        }
        state.jobs.push_back((id.clone(), job));
        if state.running {
            return SettleCommitStart::Pending { job: id };
        }
        match self.spawn() {
            Ok(()) => {
                state.running = true;
                SettleCommitStart::Pending { job: id }
            }
            Err(reason) => {
                state.jobs.pop_back();
                SettleCommitStart::NotCommitted { reason }
            }
        }
    }

    /// Start the worker. The caller holds the state lock, which the worker takes before its
    /// first job, so it cannot find the queue empty and end before `running` is set.
    fn spawn(self: &Arc<Self>) -> Result<(), String> {
        let Some(ctx) = self.ctx.upgrade() else {
            return Err(STOPPING.to_owned());
        };
        let (queue, worker_ctx) = (Arc::clone(self), Arc::clone(&ctx));
        let spawned = thread::Builder::new()
            .name("loom-plan-settle".into())
            .spawn(move || queue.work(&worker_ctx));
        match spawned {
            Ok(handle) => {
                ctx.own_worker("settle commit", handle, Some(Arc::clone(&self.stop)));
                Ok(())
            }
            Err(error) => {
                tracing::warn!("the settle commit worker could not start: {error}");
                Err("the commit worker could not start".to_owned())
            }
        }
    }

    /// Run jobs until the queue is empty or the server stops, announcing how each ended. A panic,
    /// in the job or its announcement, ends that job alone: the loop goes on to [`Self::next`],
    /// which clears `running`, so the shutdown drain never waits on a worker that is gone.
    fn work(&self, ctx: &PlanContext) {
        let unwound = |step: &dyn Fn()| panic::catch_unwind(AssertUnwindSafe(step)).is_err();
        while let Some((id, job)) = self.next() {
            let ran = || {
                let outcome = settle_job::run(ctx.root(), &id, &job, Limits::STANDARD);
                announce(ctx, &job.browser, outcome);
            };
            if unwound(&ran) {
                tracing::warn!("a settle commit panicked");
                unwound(&|| announce(ctx, &job.browser, settle_job::not_run(&id, &job, PANICKED)));
            }
        }
    }

    /// The next job, or `None` (the worker ends) once the queue is empty or the server stops.
    fn next(&self) -> Option<(String, SettleJob)> {
        let mut state = self.lock();
        let next = if self.stop.load(Ordering::SeqCst) {
            None
        } else {
            state.jobs.pop_front()
        };
        if next.is_none() {
            state.running = false;
            self.idle.notify_all();
        }
        next
    }

    /// Shutdown's drain: no job starts from now on, the queued ones end as not committed, and the
    /// commit in flight runs to its end, its outcome announced while the sockets are still up.
    fn shut_down(&self) {
        let dropped = {
            let mut state = self.lock();
            self.stop.store(true, Ordering::SeqCst);
            std::mem::take(&mut state.jobs)
        };
        if let Some(ctx) = self.ctx.upgrade() {
            for (id, job) in &dropped {
                announce(
                    &ctx,
                    &job.browser,
                    settle_job::not_run(id, job, STOPPED_FIRST),
                );
            }
        }
        let mut state = self.lock();
        if state.running && !state.released {
            lifecycle::line("waiting for a settle commit's hooks to finish");
        }
        while state.running && !state.released {
            state = self
                .idle
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    /// Shutdown's abort: the drain stops waiting; the commit in flight is left to run.
    fn release(&self) {
        self.lock().released = true;
        self.idle.notify_all();
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
