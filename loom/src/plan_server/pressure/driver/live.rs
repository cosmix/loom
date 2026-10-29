//! A run's shared state: the stop request, what is running now (the Claude step's session and
//! the Codex process), and the view. The run thread, the Stop route, grace expiry and the
//! shutdown hook all reach it; each lock is held for one short step.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use super::super::install::lock;
use super::super::recover::terminate_codex;
use super::super::view::{RunView, StopCause};
use super::PressureSessions;
use crate::plan_server::ids::{BrowserId, PublicId};
use crate::plan_server::wire::pressure::PressureRunView;
use crate::plan_session::bridge::{EndCause, EndMode};
use crate::process::ProcessIdentity;

/// Where the running Claude step's session is in its life.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Running,
    /// A teardown was asked for.
    TearingDown,
    /// Ended by its marker: plan 7 keeps it (and Codex) until `release_session`.
    AwaitRelease,
    /// Released, or ended any other way.
    Done,
}

/// What is running now.
#[derive(Default)]
pub(in crate::plan_server::pressure) struct Live {
    session: Option<(String, Phase)>,
    codex: Option<ProcessIdentity>,
}

impl Live {
    pub(in crate::plan_server::pressure) fn started(&mut self, id: &str) {
        self.session = Some((id.to_owned(), Phase::Running));
    }

    pub(in crate::plan_server::pressure) fn marker_ended(&mut self) {
        self.set_phase(Phase::AwaitRelease);
    }

    pub(in crate::plan_server::pressure) fn ended(&mut self) {
        self.set_phase(Phase::Done);
    }

    pub(in crate::plan_server::pressure) fn set_codex(&mut self, codex: Option<ProcessIdentity>) {
        self.codex = codex;
    }

    /// The marker-ended session to release, once.
    pub(in crate::plan_server::pressure) fn claim_release(&mut self) -> Option<String> {
        match &mut self.session {
            Some((id, phase)) if *phase == Phase::AwaitRelease => {
                *phase = Phase::Done;
                Some(id.clone())
            }
            _ => None,
        }
    }

    fn set_phase(&mut self, next: Phase) {
        if let Some((_, phase)) = &mut self.session {
            *phase = next;
        }
    }

    /// What a teardown does to the session: end a running one, release a marker-ended one.
    fn claim_teardown(&mut self) -> Option<(String, bool)> {
        let (id, phase) = self.session.as_mut()?;
        let release = match *phase {
            Phase::Running => false,
            Phase::AwaitRelease => true,
            Phase::TearingDown | Phase::Done => return None,
        };
        *phase = if release {
            Phase::Done
        } else {
            Phase::TearingDown
        };
        Some((id.clone(), release))
    }
}

/// One pressure run.
pub(in crate::plan_server::pressure) struct Run {
    /// The public id authoring was taken with; the run releases only that hold.
    pub(in crate::plan_server::pressure) starter: PublicId,
    /// The starting browser, whose tabs get the closing chat's queued messages.
    pub(in crate::plan_server::pressure) browser: BrowserId,
    stop: AtomicBool,
    cause: Mutex<Option<StopCause>>,
    live: Mutex<Live>,
    view: Mutex<RunView>,
    done: Mutex<bool>,
    finished: Condvar,
}

impl Run {
    pub(in crate::plan_server::pressure) fn new(
        starter: PublicId,
        browser: BrowserId,
        view: RunView,
    ) -> Self {
        Run {
            starter,
            browser,
            stop: AtomicBool::new(false),
            cause: Mutex::new(None),
            live: Mutex::default(),
            view: Mutex::new(view),
            done: Mutex::new(false),
            finished: Condvar::new(),
        }
    }

    /// Ask the run to stop; the first cause is the one shown.
    pub(in crate::plan_server::pressure) fn request_stop(&self, cause: StopCause) {
        let mut first = lock(&self.cause);
        if first.is_none() {
            *first = Some(cause);
        }
        drop(first);
        self.stop.store(true, Ordering::SeqCst);
    }

    pub(in crate::plan_server::pressure) fn stopped(&self) -> bool {
        self.stop.load(Ordering::SeqCst)
    }

    pub(in crate::plan_server::pressure) fn cause(&self) -> Option<StopCause> {
        lock(&self.cause).clone()
    }

    pub(in crate::plan_server::pressure) fn live(&self) -> MutexGuard<'_, Live> {
        lock(&self.live)
    }

    /// Change the view; returns the changed view to broadcast.
    pub(in crate::plan_server::pressure) fn update(
        &self,
        change: impl FnOnce(&mut RunView),
    ) -> PressureRunView {
        let mut view = lock(&self.view);
        change(&mut view);
        view.view()
    }

    pub(in crate::plan_server::pressure) fn view(&self) -> PressureRunView {
        lock(&self.view).view()
    }

    pub(in crate::plan_server::pressure) fn mark_done(&self) {
        *lock(&self.done) = true;
        self.finished.notify_all();
    }

    /// Wait up to `bound` for the run's thread to finish; true when it has.
    pub(in crate::plan_server::pressure) fn wait_done(&self, bound: Duration) -> bool {
        let done = lock(&self.done);
        let (done, _) = self
            .finished
            .wait_timeout_while(done, bound, |done| !*done)
            .unwrap_or_else(PoisonError::into_inner);
        *done
    }
}

/// Stop's direct teardown: deny the pending permission request and interrupt the running
/// Claude step, then tear its session down (or release it when its marker already ended it),
/// and terminate Codex through its verified identity. Returns lines for the terminal.
pub(in crate::plan_server::pressure) fn teardown(
    sessions: &dyn PressureSessions,
    run: &Run,
) -> Vec<String> {
    let (session, codex) = {
        let mut live = run.live();
        (live.claim_teardown(), live.codex)
    };
    match session {
        Some((id, true)) => {
            if let Err(error) = sessions.release_session(&id) {
                tracing::debug!(session = %id, %error, "release at teardown");
            }
        }
        Some((id, false)) => {
            // `interrupt` denies a pending permission request first; it refuses a step that
            // runs no turn, which the end below still tears down.
            let _ = sessions.interrupt(&id);
            if let Err(error) = sessions.end(&id, EndMode::Teardown, EndCause::Caller) {
                tracing::debug!(session = %id, %error, "end at teardown");
            }
        }
        None => {}
    }
    codex.and_then(terminate_codex).into_iter().collect()
}
