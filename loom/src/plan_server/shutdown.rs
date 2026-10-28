//! Two-signal shutdown: the first signal drains every registered component
//! concurrently and names the ones still running; the second aborts them.

use std::io;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};

type Task = Box<dyn FnOnce() + Send>;
type Report = Arc<dyn Fn(&str) + Send + Sync>;

/// A component's part in shutdown. `drain` runs on its own thread and may
/// block until the component has finished its work; `abort` makes a running
/// `drain` return and must return within a bound it states: the session
/// bridge's waits up to 30 s (`LAUNCH_WAIT`) for a launch in flight.
pub struct ShutdownHook {
    pub name: &'static str,
    pub drain: Box<dyn FnOnce() + Send>,
    pub abort: Box<dyn FnOnce() + Send>,
}

/// A registered hook. `drain` is taken when it starts; `abort` is taken
/// when it runs or when the drain returns, so each runs at most once.
struct Slot {
    name: &'static str,
    drain: Option<Task>,
    abort: Option<Task>,
}

#[derive(Default)]
struct State {
    hooks: Vec<Slot>,
    /// Set by the first `begin`; later registrations drain at once.
    report: Option<Report>,
    aborted: bool,
    draining: usize,
}

#[derive(Default)]
struct Inner {
    state: Mutex<State>,
    idle: Condvar,
    stopping: AtomicBool,
}

impl Inner {
    /// Every mutation of `State` is a single field update or push, so a
    /// poisoned lock still holds consistent state.
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn wait_idle(&self) {
        let mut state = self.lock();
        while state.draining > 0 {
            state = self
                .idle
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }
}

/// The registry of [`ShutdownHook`]s and the stopping flag.
#[derive(Default)]
pub struct Shutdown {
    inner: Arc<Inner>,
}

impl Shutdown {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a hook. After [`Shutdown::begin`] its drain starts at once; after
    /// [`Shutdown::abort_all`] its abort runs at once instead.
    pub fn register(&self, hook: ShutdownHook) {
        let mut state = self.inner.lock();
        if state.aborted {
            drop(state);
            (hook.abort)();
            return;
        }
        let index = state.hooks.len();
        let Some(report) = state.report.clone() else {
            state.hooks.push(Slot {
                name: hook.name,
                drain: Some(hook.drain),
                abort: Some(hook.abort),
            });
            return;
        };
        state.hooks.push(Slot {
            name: hook.name,
            drain: None,
            abort: Some(hook.abort),
        });
        state.draining += 1;
        drop(state);
        report(&format!("waiting on: {}", hook.name));
        spawn_drain(&self.inner, index, hook.name, hook.drain, report);
    }

    pub fn is_stopping(&self) -> bool {
        self.inner.stopping.load(Ordering::SeqCst)
    }

    /// First signal: mark stopping and run every registered drain on its own
    /// thread. `report` gets `waiting on: a, b` once, then `finished: a` as
    /// each drain returns. The handle joins once no drain is running. Later
    /// calls start nothing and return a handle that waits for the same.
    pub fn begin(&self, report: impl Fn(&str) + Send + Sync + 'static) -> JoinHandle<()> {
        self.inner.stopping.store(true, Ordering::SeqCst);
        let report: Report = Arc::new(report);
        let started = self.take_drains(&report);
        if !started.is_empty() {
            let names: Vec<&str> = started.iter().map(|(_, name, _)| *name).collect();
            report(&format!("waiting on: {}", names.join(", ")));
        }
        for (index, name, drain) in started {
            spawn_drain(&self.inner, index, name, drain, Arc::clone(&report));
        }
        let inner = Arc::clone(&self.inner);
        thread::Builder::new()
            .name("plan-shutdown".into())
            .spawn(move || inner.wait_idle())
            .expect("spawn the shutdown join thread")
    }

    /// Second signal: run every abort not yet run whose drain has not
    /// returned (running or never started), in registration order. Drains
    /// not yet started never start.
    pub fn abort_all(&self) {
        self.inner.stopping.store(true, Ordering::SeqCst);
        let aborts: Vec<Task> = {
            let mut state = self.inner.lock();
            state.aborted = true;
            state
                .hooks
                .iter_mut()
                .filter_map(|slot| {
                    slot.drain = None;
                    slot.abort.take()
                })
                .collect()
        };
        for abort in aborts {
            abort();
        }
    }

    /// On the first `begin`, record `report` and take every drain not yet
    /// started; nothing on later calls or after `abort_all`.
    fn take_drains(&self, report: &Report) -> Vec<(usize, &'static str, Task)> {
        let mut state = self.inner.lock();
        if state.report.is_some() || state.aborted {
            return Vec::new();
        }
        state.report = Some(Arc::clone(report));
        let started: Vec<_> = state
            .hooks
            .iter_mut()
            .enumerate()
            .filter_map(|(index, slot)| slot.drain.take().map(|drain| (index, slot.name, drain)))
            .collect();
        state.draining += started.len();
        started
    }
}

/// Run `drain` on a thread named after the hook. Bookkeeping lives in
/// [`DrainDone`], dropped when the drain returns or panics, or with the
/// closure when the thread cannot be spawned.
fn spawn_drain(inner: &Arc<Inner>, index: usize, name: &'static str, drain: Task, report: Report) {
    let done = DrainDone {
        inner: Arc::clone(inner),
        index,
        name,
        report,
    };
    let spawned = thread::Builder::new()
        .name(format!("plan-drain-{name}"))
        .spawn(move || {
            let _done = done;
            drain();
        });
    if let Err(error) = spawned {
        tracing::warn!("could not start the {name} shutdown drain: {error}");
    }
}

struct DrainDone {
    inner: Arc<Inner>,
    index: usize,
    name: &'static str,
    report: Report,
}

impl Drop for DrainDone {
    fn drop(&mut self) {
        {
            let mut state = self.inner.lock();
            state.hooks[self.index].abort = None;
            state.draining -= 1;
        }
        self.inner.idle.notify_all();
        // Last: a panicking reporter must not leave `stop_when_drained` waiting.
        (self.report)(&format!("finished: {}", self.name));
    }
}

/// Route SIGINT, SIGTERM and SIGHUP (the `ctrlc` crate's `termination`
/// feature) to `shutdown`. The first signal calls `on_first`, which starts
/// the drain (typically through [`Shutdown::begin`]) and must return
/// promptly: later signals are handled on the same thread. Every later
/// signal runs [`Shutdown::abort_all`] and then `exit(130)`. Fails when a
/// handler is already installed; the crate allows one per process.
pub fn install_signal_handler(
    shutdown: Arc<Shutdown>,
    on_first: Box<dyn Fn() + Send + Sync>,
    exit: fn(i32) -> !,
) -> io::Result<()> {
    let handler = SignalHandler::new(shutdown, on_first, exit);
    ctrlc::set_handler(move || handler.on_signal()).map_err(io::Error::other)
}

pub(super) struct SignalHandler {
    shutdown: Arc<Shutdown>,
    on_first: Box<dyn Fn() + Send + Sync>,
    exit: fn(i32) -> !,
    signals: AtomicUsize,
}

impl SignalHandler {
    pub(super) fn new(
        shutdown: Arc<Shutdown>,
        on_first: Box<dyn Fn() + Send + Sync>,
        exit: fn(i32) -> !,
    ) -> Self {
        Self {
            shutdown,
            on_first,
            exit,
            signals: AtomicUsize::new(0),
        }
    }

    pub(super) fn on_signal(&self) {
        if self.signals.fetch_add(1, Ordering::SeqCst) == 0 {
            (self.on_first)();
        } else {
            self.shutdown.abort_all();
            (self.exit)(130);
        }
    }
}
