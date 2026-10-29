//! Installs pressure runs on the planning server: the run registry, the bridge listener that
//! forwards a run's session events to its thread, grace expiry, the shutdown hook, the view a
//! new tab opens with, and the report of runs the last server left unfinished.

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};
use std::time::Duration;

use super::driver::{teardown, PressureSessions, Run};
use super::options::PressureRunning;
use super::recover::interrupted_runs;
use super::view::StopCause;
use crate::plan_server::authoring::{AuthoringEvent, FreeReason};
use crate::plan_server::context::PlanContext;
use crate::plan_server::lifecycle::line;
use crate::plan_server::shutdown::ShutdownHook;
use crate::plan_server::wire::message::PlanMessage;
use crate::plan_server::wire::pressure::PressureRunView;
use crate::plan_session::bridge::{Bridge, BridgeEvent};
use crate::plan_session::kind::SessionKind;

/// How long the shutdown drain waits for a stopped run to finish.
const DRAIN_WAIT: Duration = Duration::from_secs(30);

/// The programs and bounds a run uses that tests replace.
#[derive(Clone)]
pub(crate) struct Tools {
    /// The tmux program the teammate pane check runs.
    pub(crate) tmux: PathBuf,
    /// How often a running Claude step's tmux server is checked for a second pane.
    pub(crate) pane_check: Duration,
    /// How long the open chat's closing write may take: plan 7's 600 s bound and its slack.
    pub(crate) chat_close: Duration,
    /// How long a terminated Codex may take to exit before the run stops waiting for it.
    pub(crate) codex_exit_grace: Duration,
}

impl Tools {
    pub(crate) fn standard() -> Tools {
        Tools {
            tmux: PathBuf::from("tmux"),
            pane_check: Duration::from_secs(5),
            chat_close: Duration::from_secs(605),
            codex_exit_grace: Duration::from_secs(10),
        }
    }
}

/// The project's pressure runs: at most one runs; the last view stays for new tabs.
pub struct PressureRuns {
    ctx: Weak<PlanContext>,
    pub(super) sessions: Arc<dyn PressureSessions>,
    pub(super) tools: Tools,
    running: Arc<PressureRunning>,
    current: Mutex<Option<Arc<Run>>>,
    events: Mutex<Option<Sender<BridgeEvent>>>,
    last: Mutex<Option<PressureRunView>>,
}

impl PressureRuns {
    pub(super) fn ctx(&self) -> Option<Arc<PlanContext>> {
        self.ctx.upgrade()
    }

    /// From an accepted start through its cleanup.
    pub(super) fn running(&self) -> bool {
        self.running.0.load(Ordering::SeqCst)
    }

    pub(super) fn current(&self) -> Option<Arc<Run>> {
        lock(&self.current).clone()
    }

    /// Claim the one run slot; false when a run holds it.
    pub(super) fn claim(&self) -> bool {
        let flag = &self.running.0;
        flag.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }

    pub(super) fn unclaim(&self) {
        self.running.0.store(false, Ordering::SeqCst);
    }

    /// Make `run` the current run; its thread hears its sessions' ends on the returned channel.
    pub(super) fn begin(&self, run: Arc<Run>) -> Receiver<BridgeEvent> {
        let (sender, receiver) = mpsc::channel();
        *lock(&self.events) = Some(sender);
        *lock(&self.current) = Some(run);
        receiver
    }

    /// The run ended: the slot is free again (after authoring was released, so a new run's
    /// hold cannot be the one the old run releases).
    pub(super) fn end(&self, run: &Arc<Run>) {
        {
            let mut current = lock(&self.current);
            if current.as_ref().is_some_and(|held| Arc::ptr_eq(held, run)) {
                *current = None;
            }
        }
        *lock(&self.events) = None;
        self.unclaim();
    }

    /// Remember the view for new tabs and send it to every tab.
    pub(super) fn publish(&self, view: &PressureRunView) {
        *lock(&self.last) = Some(view.clone());
        if let Some(ctx) = self.ctx() {
            ctx.hub().broadcast(&PlanMessage::PressureRun(view.clone()));
        }
    }

    /// Stop the current run as the Stop button does; `None` when no run is running.
    pub(super) fn stop(&self, cause: StopCause) -> Option<Arc<Run>> {
        let run = self.current()?;
        run.request_stop(cause);
        for message in teardown(&*self.sessions, &run) {
            line(&message);
        }
        Some(run)
    }

    /// The latest run's latest view.
    pub(super) fn last_view(&self) -> Option<PressureRunView> {
        lock(&self.last).clone()
    }

    /// Forward a session's end to the running run's thread, which keeps the ends of its own
    /// sessions (only a run starts pressure sessions, and it ends the open chat itself). A new
    /// authoring chat replaces an ended run's view.
    fn forward(&self, event: &BridgeEvent) {
        match event {
            BridgeEvent::Ended { .. } => {
                if let Some(sender) = lock(&self.events).as_ref() {
                    let _ = sender.send(event.clone());
                }
            }
            BridgeEvent::Started {
                kind: SessionKind::Authoring,
                ..
            } if self.current().is_none() => *lock(&self.last) = None,
            _ => {}
        }
    }
}

/// Install pressure runs over the session bridge.
pub fn run(ctx: &Arc<PlanContext>) {
    let Some(bridge) = ctx.extensions().get::<Bridge>() else {
        ctx.extensions()
            .insert(Arc::new(PressureRunning::default()));
        line("pressure runs are unavailable: the session bridge is missing");
        return;
    };
    install_with(ctx, bridge, Tools::standard());
}

/// [`run`] over `sessions`, with `tools`.
pub(crate) fn install_with(
    ctx: &Arc<PlanContext>,
    sessions: Arc<dyn PressureSessions>,
    tools: Tools,
) {
    let running = Arc::new(PressureRunning::default());
    ctx.extensions().insert(Arc::clone(&running));
    let runs = Arc::new(PressureRuns {
        ctx: Arc::downgrade(ctx),
        sessions: Arc::clone(&sessions),
        tools,
        running,
        current: Mutex::new(None),
        events: Mutex::new(None),
        last: Mutex::new(None),
    });
    ctx.extensions().insert(Arc::clone(&runs));
    let weak = Arc::downgrade(&runs);
    sessions.subscribe(Box::new(move |event| {
        if let Some(runs) = weak.upgrade() {
            runs.forward(event);
        }
    }));
    follow_authoring(ctx, &runs);
    send_on_open(ctx, &runs);
    register_shutdown(ctx, &runs);
    let root = ctx.root().to_path_buf();
    sessions.on_recovered(Box::new(move |report| {
        for message in interrupted_runs(&root, report) {
            line(&message);
        }
    }));
}

/// The holder away for the grace period: nobody is left to answer the run's prompts, so the
/// run stops as Stop does.
fn follow_authoring(ctx: &PlanContext, runs: &Arc<PressureRuns>) {
    let weak = Arc::downgrade(runs);
    ctx.authoring().subscribe(Box::new(move |event| {
        let expired = AuthoringEvent::Freed {
            reason: FreeReason::GraceExpired,
        };
        if *event == expired {
            if let Some(runs) = weak.upgrade() {
                runs.stop(StopCause::GraceExpired);
            }
        }
    }));
}

/// A tab that opens (or reconnects) gets the run's latest view.
fn send_on_open(ctx: &PlanContext, runs: &Arc<PressureRuns>) {
    let weak = Arc::downgrade(runs);
    ctx.hub().on_open(Box::new(move |_| {
        let view = weak.upgrade().and_then(|runs| runs.last_view());
        view.map(|view| vec![PlanMessage::PressureRun(view)])
            .unwrap_or_default()
    }));
}

/// Ctrl-C stops the run as Stop does; the drain waits for the run's thread (it finishes
/// quickly: the steps are torn down and Codex is terminated), the abort waits for nothing.
fn register_shutdown(ctx: &PlanContext, runs: &Arc<PressureRuns>) {
    let (drain, abort) = (Arc::downgrade(runs), Arc::downgrade(runs));
    ctx.shutdown().register(ShutdownHook {
        name: "pressure run",
        drain: Box::new(move || {
            let stopped = drain
                .upgrade()
                .and_then(|runs| runs.stop(StopCause::Server));
            if let Some(run) = stopped {
                run.wait_done(DRAIN_WAIT);
            }
        }),
        abort: Box::new(move || {
            if let Some(runs) = abort.upgrade() {
                runs.stop(StopCause::Server);
            }
        }),
    });
}

/// Every critical section of the module leaves its value consistent (one store, clone or
/// view change), so a poisoned lock is still usable.
pub(super) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
