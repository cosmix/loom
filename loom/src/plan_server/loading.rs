//! The background half of startup: everything `loom plan` does after it has bound, so the URL
//! and the browser never wait on the project's size or on the prerequisite checks.
//!
//! The accept loop runs from the start. The page shell, assets and the token bootstrap need
//! nothing loaded and answer at once; `PlanRouter` holds `/api/*` requests and `/ws` upgrades
//! until [`Loading`] leaves [`LoadState::Loading`], so no handler sees a half-installed server and
//! no data route can answer an empty result as if it were final. A full held lane answers 503
//! rather than starve the shell. Answering 503 to every request would strand the page: file
//! mode fetches `/api/tree` once per mount and ignores a failure.
//!
//! The load takes about a second on 864 documents, and the wait is bounded by [`LOAD_WAIT`].
//! Revisit the single gate if a route has to answer before the load ends.

use std::panic::{self, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};

use super::context::PlanContext;
use super::launch::spawn_workers;
use super::prereq::ReadOnly;
use super::watch;
use super::worker_owner::Registration;
use crate::planning::watch::Watcher;

/// How long one request waits for the load before it is told to try again.
pub(super) const LOAD_WAIT: Duration = Duration::from_secs(60);
/// What a stop result says about a failed load; the cause is already on the terminal.
const LOAD_FAILED: &str = "the project could not be loaded; the cause is printed above";
/// Longest single sleep of a waiting request, so a stop is seen promptly.
const STOP_CHECK: Duration = Duration::from_millis(50);

/// The install step of the load: `routes::install_all`, with whatever the caller adds around it.
pub(super) type Install = Box<dyn FnOnce(&Arc<PlanContext>) + Send>;
/// The load's source of the prerequisites' verdict. It runs on the load thread: the Claude login
/// probe in [`ReadOnly::detect`] alone may take its 30 s timeout.
pub(super) type CheckReadOnly = Box<dyn FnOnce() -> ReadOnly + Send>;

/// How far the background load got.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) enum LoadState {
    #[default]
    Loading,
    Ready,
    /// The load failed, with this cause; the server is stopping.
    Failed(String),
}

/// The load's state, which every held request waits on.
#[derive(Default)]
pub(crate) struct Loading {
    state: Mutex<LoadState>,
    changed: Condvar,
}

impl Loading {
    pub(super) fn finish(&self, state: LoadState) {
        *self.lock() = state;
        self.changed.notify_all();
    }

    /// The state once the load has ended, or as it stands when `stopped` turns true or `max`
    /// has passed.
    pub(super) fn wait(&self, max: Duration, stopped: impl Fn() -> bool) -> LoadState {
        let deadline = Instant::now() + max;
        let mut state = self.lock();
        while *state == LoadState::Loading && !stopped() {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break;
            }
            let (next, _) = self
                .changed
                .wait_timeout(state, left.min(STOP_CHECK))
                .unwrap_or_else(PoisonError::into_inner);
            state = next;
        }
        state.clone()
    }

    /// The state as it stands, without waiting.
    pub(super) fn state(&self) -> LoadState {
        self.lock().clone()
    }

    /// Whether the load failed. The message went to the terminal when it was recorded, so a stop
    /// result carries [`LOAD_FAILED`] and the cause is not printed a second time.
    pub(super) fn failure(&self) -> Option<&'static str> {
        matches!(*self.lock(), LoadState::Failed(_)).then_some(LOAD_FAILED)
    }

    /// Every update is one store, so a poisoned lock is still consistent.
    fn lock(&self) -> MutexGuard<'_, LoadState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// What the load thread needs from `start_owned`.
pub(super) struct Load {
    pub(super) read_only: CheckReadOnly,
    pub(super) install: Install,
    pub(super) running: Arc<AtomicBool>,
    pub(super) stop: Arc<AtomicBool>,
}

/// Start the load. The thread keeps its `registration` until it ends, so the cleanup owner
/// neither joins nor disposes anything while the load can still register workers.
pub(super) fn spawn(ctx: &Arc<PlanContext>, load: Load, registration: &Registration) -> Result<()> {
    let (worker_ctx, worker_registration) = (Arc::clone(ctx), registration.clone());
    let handle = thread::Builder::new()
        .name("loom-plan-load".into())
        .spawn(move || run(&worker_ctx, load, &worker_registration))
        .context("cannot start the loom plan load thread")?;
    registration.background(handle);
    Ok(())
}

/// Run the load and publish its outcome. A failure prints its cause and stops the server, rather
/// than leaving a page that can never load.
fn run(ctx: &Arc<PlanContext>, load: Load, registration: &Registration) {
    let (running, stop) = (Arc::clone(&load.running), Arc::clone(&load.stop));
    let loaded = AssertUnwindSafe(|| load_and_install(ctx, load, registration));
    let loaded = panic::catch_unwind(loaded);
    let failure = match loaded {
        Ok(Ok(())) => return,
        Ok(Err(error)) => error,
        Err(_) => anyhow!("the loom plan load panicked"),
    };
    let message = format!("{failure:#}");
    // Where it is recorded: the stop result can be lost to a drain that outlasts its deadline.
    eprintln!("error: the project could not be loaded: {message}");
    ctx.loading().finish(LoadState::Failed(message));
    running.store(false, Ordering::SeqCst);
    stop.store(true, Ordering::SeqCst);
}

/// The load itself. A server told to stop while the project loaded installs nothing: held
/// requests already answer that it is stopping.
fn load_and_install(ctx: &Arc<PlanContext>, load: Load, registration: &Registration) -> Result<()> {
    let watcher = load_project(ctx, (load.read_only)())?;
    if ctx.is_stopping() {
        return Ok(());
    }
    (load.install)(ctx);
    spawn_workers(ctx, watcher, &load.stop, registration)?;
    ctx.loading().finish(LoadState::Ready);
    Ok(())
}

/// Take the watcher's baseline, then load the project. The baseline keeps edits made during the
/// rest of the load in the watcher's first batch.
pub(super) fn load_project(ctx: &PlanContext, read_only: ReadOnly) -> Result<Watcher> {
    let watcher = watch::baseline(ctx.root().to_path_buf(), ctx.own_writes().clone());
    ctx.load_project(read_only)?;
    Ok(watcher)
}
