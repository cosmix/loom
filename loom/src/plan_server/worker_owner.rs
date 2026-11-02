//! Own and reap every server worker without making stop wait on blocked I/O.

use std::net::{SocketAddr, TcpListener};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};

use super::context::PlanContext;
use super::lock::Ownership;
use crate::web_server::ShutdownPending;

const STOP_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug)]
enum Worker {
    Accept(JoinHandle<Result<()>>),
    Background(JoinHandle<()>),
}

/// The cleanup thread retains this state itself until all owned work ends.
/// Its handle consequently cannot be lost while cleanup is still running.
struct State {
    complete: Arc<AtomicBool>,
    accept_done: AtomicBool,
    cleanup: Mutex<Option<JoinHandle<()>>>,
    failure: Arc<Mutex<Option<String>>>,
}

pub(super) struct Workers {
    local: SocketAddr,
    state: Arc<State>,
    result: mpsc::Receiver<Result<()>>,
}

/// A handle for registering workers. The owner joins nothing until every clone has dropped.
#[derive(Clone)]
pub(super) struct Registration(mpsc::Sender<Worker>);

impl Registration {
    pub(super) fn background(&self, handle: JoinHandle<()>) {
        self.0
            .send(Worker::Background(handle))
            .expect("live worker owner");
    }

    pub(super) fn accept(&self, handle: JoinHandle<Result<()>>) {
        self.0
            .send(Worker::Accept(handle))
            .expect("live worker owner");
    }
}

impl Workers {
    /// Register workers as they start. A partial startup failure stops the
    /// registered set while its cleanup owner retains every live resource.
    pub(super) fn launch(
        listener: TcpListener,
        ownership: Arc<Ownership>,
        context: Arc<PlanContext>,
        running: &AtomicBool,
        stop: &AtomicBool,
        start: impl FnOnce(TcpListener, &Registration) -> Result<()>,
    ) -> Result<Self> {
        let (workers, registration) = Self::start(&listener, ownership, context)?;
        let started = start(listener, &registration);
        if started.is_err() {
            running.store(false, Ordering::SeqCst);
            stop.store(true, Ordering::SeqCst);
        }
        drop(registration);
        started?;
        Ok(workers)
    }

    /// Start ownership before spawning any worker. The listener reservation
    /// and singleton outlive both transport requests and background workers.
    fn start(
        listener: &TcpListener,
        ownership: Arc<Ownership>,
        context: Arc<PlanContext>,
    ) -> Result<(Self, Registration)> {
        let local = listener.local_addr()?;
        let reservation = listener
            .try_clone()
            .context("cannot retain server listener")?;
        let state = Arc::new(State {
            complete: Arc::new(AtomicBool::new(false)),
            accept_done: AtomicBool::new(false),
            cleanup: Mutex::new(None),
            failure: Arc::new(Mutex::new(None)),
        });
        let (register, incoming) = mpsc::channel();
        let (finished, result) = mpsc::channel();
        let owner = Arc::clone(&state);
        let handle = thread::Builder::new()
            .name("loom-plan-cleanup".into())
            .spawn(move || cleanup(incoming, owner, reservation, ownership, context, finished))
            .context("cannot start the planning server worker owner")?;
        *state
            .cleanup
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(handle);
        Ok((
            Self {
                local,
                state,
                result,
            },
            Registration(register),
        ))
    }

    pub(super) fn is_serving(&self) -> bool {
        !self.state.accept_done.load(Ordering::Acquire)
    }

    pub(super) fn stop(self) -> Result<()> {
        match self.result.recv_timeout(STOP_TIMEOUT) {
            Ok(result) => result,
            Err(mpsc::RecvTimeoutError::Timeout) => Err(ShutdownPending::new(
                self.local,
                Arc::clone(&self.state.complete),
            )
            .with_failure(Arc::clone(&self.state.failure))
            .into()),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(anyhow!(
                "planning server worker owner stopped without a result"
            )),
        }
    }
}

fn cleanup(
    incoming: mpsc::Receiver<Worker>,
    owner: Arc<State>,
    reservation: TcpListener,
    ownership: Arc<Ownership>,
    context: Arc<PlanContext>,
    finished: mpsc::Sender<Result<()>>,
) {
    let result = reap(incoming, &owner, &context);
    // Context extensions own child processes and may block while disposing
    // them. Keep the address and singleton until that disposal has finished.
    drop(context);
    drop(reservation);
    drop(ownership);
    if let Err(error) = &result {
        *owner
            .failure
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(format!("{error:#}"));
    }
    owner.complete.store(true, Ordering::Release);
    if let Err(mpsc::SendError(Err(error))) = finished.send(result) {
        tracing::error!("planning server cleanup failed: {error:#}");
    }
}

fn reap(incoming: mpsc::Receiver<Worker>, state: &State, context: &PlanContext) -> Result<()> {
    let (mut accept, mut background) = (None, Vec::new());
    for worker in incoming {
        match worker {
            Worker::Accept(handle) => accept = Some(handle),
            Worker::Background(handle) => background.push(handle),
        }
    }
    let mut result = match accept {
        Some(handle) => handle
            .join()
            .unwrap_or_else(|_| Err(anyhow!("the loom plan accept loop panicked"))),
        None => Ok(()),
    };
    state.accept_done.store(true, Ordering::Release);
    let drains = context
        .shutdown()
        .start_drains(|message| tracing::debug!("{message}"));
    if let Some(pending) = result
        .as_ref()
        .err()
        .and_then(|error| error.downcast_ref::<ShutdownPending>())
    {
        pending.wait(Duration::MAX);
        result = Ok(());
    }
    for handle in background {
        if handle.join().is_err() && result.is_ok() {
            result = Err(anyhow!("a planning server background worker panicked"));
        }
    }
    // Bridge drivers must keep progressing while chat drains perform their
    // final writes. Only after hooks finish do we stop and join module workers.
    drains.wait();
    let modules = context.stop_workers();
    if result.is_ok() {
        result = modules;
    }
    match context.loading().failure() {
        Some(message) if result.is_ok() => Err(anyhow!(message)),
        _ => result,
    }
}

#[cfg(test)]
#[path = "worker_owner_tests.rs"]
mod tests;
