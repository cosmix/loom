//! The `Rebuilds` facade held in `ctx.extensions()`: the scheduler and attribution behind one
//! lock each, fed by the summary engine, the session bridge and a 5 s tick.
//!
//! Concurrency: every scheduler call runs under the state lock against a recording spawner,
//! which notes the start and stops the call asks for. After the lock is released the facade
//! makes the real calls (a start flushes bridge events into this facade's own listener, so it
//! can never run under the lock), re-locks to settle the start, then broadcasts and prints what
//! changed.
//! Attribution (claim files, the cost log) has its own lock; the two are never held together.

mod api;
mod evaluation;
mod panel;
mod publish;
mod session;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_alive;
#[cfg(test)]
mod tests_bridge;
#[cfg(test)]
mod tests_codex;
#[cfg(test)]
mod tests_ordering;
#[cfg(test)]
mod tests_press;

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};
use std::time::Instant;

use chrono::{SecondsFormat, Utc};

use super::attribution::{Attributed, Attribution};
use super::scheduler::{Scheduler, SchedulerConfig, SchedulerEvent, SessionSpawner};
use crate::plan_server::context::PlanContext;
use crate::plan_server::lifecycle;
use crate::plan_server::wire::message::PlanMessage;
use crate::plan_server::wire::summaries::SummaryPanel;
use crate::plan_session::bridge::{Bridge, EndOutcome};
use crate::planning::runtime::RuntimePaths;
use crate::planning::summary::cost::{self, CostLine};
use crate::planning::summary::evaluate::Evaluation;
use crate::planning::summary::format::SummaryFile;

pub use self::api::{Building, ReleaseRefused, SuspendGuard};
pub use super::scheduler::RebuildReason;

/// Sends one message to every connected browser.
pub type Broadcast = Arc<dyn Fn(&PlanMessage) + Send + Sync>;

/// The session id the [`Recorder`] answers until the real start settles it.
const PENDING: &str = "(starting)";

/// Stands in for the real spawner while the state lock is held.
#[derive(Default)]
struct Recorder {
    start: Mutex<Option<Vec<String>>>,
    stops: Mutex<Vec<(String, bool)>>,
}

impl SessionSpawner for Recorder {
    fn start_summary(&self, ids: &[String]) -> anyhow::Result<String> {
        *lock(&self.start) = Some(ids.to_vec());
        Ok(PENDING.to_owned())
    }

    fn stop(&self, session: &str, release_claims: bool) {
        lock(&self.stops).push((session.to_owned(), release_claims));
    }
}

/// What the facade keeps beside the scheduler.
struct State {
    scheduler: Scheduler,
    /// Events of the calls that drain the scheduler's outbox themselves.
    events: Vec<SchedulerEvent>,
    latest: Option<Arc<Evaluation>>,
    /// What each summary was built from at the last evaluation applied with a project
    /// (`hashes::input_hashes`); a change starts the summary's quiet period.
    inputs: Option<BTreeMap<String, String>>,
    /// Source bytes by summary id, for estimates.
    sizes: BTreeMap<String, u64>,
    /// The file of each running id at the start or its last seal.
    baselines: BTreeMap<String, Option<SummaryFile>>,
    /// The authoring turn holding the scheduler's turn suspension: `(session, turn)`.
    open_turn: Option<(String, String)>,
    /// The suspension token of each live pressure session.
    pressure: HashMap<String, u64>,
    started_at: Option<String>,
    /// The running session's claims at the last tick or batch.
    tick_claims: Option<Vec<String>>,
    /// The running session the last tick found missing from the bridge or ended there.
    tick_gone: Option<String>,
    /// Summary session ends that arrived while a start was settling, kept for that start.
    ended_early: Vec<(String, EndOutcome)>,
}

impl State {
    /// Every id the latest evaluation finds stale or missing ([`evaluation::to_rebuild`]).
    fn to_rebuild(&self) -> Vec<String> {
        self.latest
            .as_deref()
            .map(evaluation::to_rebuild)
            .unwrap_or_default()
    }

    /// The running session once its start settled.
    fn running_session(&self) -> Option<String> {
        let session = self.scheduler.running_session();
        session
            .filter(|session| *session != PENDING)
            .map(str::to_owned)
    }

    /// A start settled: take the files of its ids as the seal baselines.
    fn begin_session(&mut self) {
        self.started_at = Some(Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true));
        self.tick_claims = None;
        self.baselines.clear();
        let (Some(latest), Some((_, ids))) = (&self.latest, self.scheduler.panel_state().running)
        else {
            return;
        };
        for id in ids {
            let file = latest.summaries.get(&id).and_then(|eval| eval.file.clone());
            self.baselines.insert(id, file);
        }
    }
}

/// The production half: the context and its runtime directory.
struct Env {
    ctx: Weak<PlanContext>,
    runtime: RuntimePaths,
    /// A bridge fixed at construction; only tests set it, production reads the context's.
    bridge: Option<Arc<Bridge>>,
}

impl Env {
    fn bridge(&self) -> Option<Arc<Bridge>> {
        let fixed = self.bridge.clone();
        fixed.or_else(|| self.ctx.upgrade()?.extensions().get::<Bridge>())
    }
}

/// Summary rebuilds: the scheduler, attribution, the cost log and their broadcasts.
pub struct Rebuilds {
    inner: Mutex<State>,
    attribution: Option<Mutex<Attribution>>,
    log: Mutex<Vec<CostLine>>,
    spawner: Arc<dyn SessionSpawner>,
    broadcast: Broadcast,
    env: Option<Env>,
    /// The panel last broadcast, so an unchanged panel is not sent again.
    panel_sent: Mutex<Option<SummaryPanel>>,
}

impl Rebuilds {
    /// The server's facade over `spawner`, the hub and the runtime directory's cost log.
    pub fn with_spawner(ctx: &Arc<PlanContext>, spawner: Arc<dyn SessionSpawner>) -> Arc<Rebuilds> {
        let problems = ctx.read_only().problems();
        let read_only = problems.first().map(|problem| problem.message.clone());
        let runtime = ctx.runtime().clone();
        let log = cost::read_log(&runtime).unwrap_or_else(|error| {
            let why = lifecycle::safe(&format!("{error:#}"));
            lifecycle::line(&format!("summaries: cannot read the cost log: {why}"));
            Vec::new()
        });
        let mut attribution = Attribution::new(runtime.clone());
        if let Some(bridge) = ctx.extensions().get::<Bridge>() {
            attribution = attribution.with_sink(bridge);
        }
        let hub = Arc::downgrade(ctx);
        let broadcast: Broadcast = Arc::new(move |message: &PlanMessage| {
            if let Some(ctx) = hub.upgrade() {
                ctx.hub().broadcast(message);
            }
        });
        let env = Env {
            ctx: Arc::downgrade(ctx),
            runtime,
            bridge: None,
        };
        let config = SchedulerConfig::standard(read_only);
        let mut rebuilds = Rebuilds::build(spawner, config, broadcast);
        rebuilds.attribution = Some(Mutex::new(attribution));
        rebuilds.log = Mutex::new(log);
        rebuilds.env = Some(env);
        Arc::new(rebuilds)
    }

    /// A facade without a server: no attribution, cost log, quota or claims.
    pub fn new_for_test(
        spawner: Arc<dyn SessionSpawner>,
        read_only: Option<String>,
        broadcast: Broadcast,
    ) -> Arc<Rebuilds> {
        let config = SchedulerConfig::standard(read_only);
        Arc::new(Rebuilds::build(spawner, config, broadcast))
    }

    /// [`Rebuilds::new_for_test`] with the scheduler's `config`, a runtime directory (so the
    /// claims tick reads claim files), optionally a bridge for the tick's liveness check and
    /// attribution over that directory; no context, so no sizes.
    #[cfg(test)]
    fn new_for_test_in(
        spawner: Arc<dyn SessionSpawner>,
        config: SchedulerConfig,
        broadcast: Broadcast,
        runtime: RuntimePaths,
        bridge: Option<Arc<Bridge>>,
        attribution: Option<Attribution>,
    ) -> Arc<Rebuilds> {
        let mut rebuilds = Rebuilds::build(spawner, config, broadcast);
        rebuilds.attribution = attribution.map(Mutex::new);
        rebuilds.env = Some(Env {
            ctx: Weak::new(),
            runtime,
            bridge,
        });
        Arc::new(rebuilds)
    }

    fn build(
        spawner: Arc<dyn SessionSpawner>,
        config: SchedulerConfig,
        broadcast: Broadcast,
    ) -> Rebuilds {
        let state = State {
            scheduler: Scheduler::new(config),
            events: Vec::new(),
            latest: None,
            inputs: None,
            sizes: BTreeMap::new(),
            baselines: BTreeMap::new(),
            open_turn: None,
            pressure: HashMap::new(),
            started_at: None,
            tick_claims: None,
            tick_gone: None,
            ended_early: Vec::new(),
        };
        Rebuilds {
            inner: Mutex::new(state),
            attribution: None,
            log: Mutex::new(Vec::new()),
            spawner,
            broadcast,
            env: None,
            panel_sent: Mutex::new(None),
        }
    }

    fn apply_evaluation(&self, eval: Arc<Evaluation>, invalidated: &[String], stale: &[String]) {
        let sizes = self.source_sizes(&eval);
        let inputs = self.source_inputs(&eval);
        if let Some(sizes) = &sizes {
            self.attribute(|attribution| attribution.set_sizes(sizes.clone()));
        }
        self.run(|state, spawner, now| {
            if let Some(sizes) = sizes {
                state.sizes = sizes;
            }
            if let Some(inputs) = inputs {
                state.note_inputs(inputs, now);
            }
            evaluation::keep_terminal_claims(&mut state.scheduler, &eval);
            evaluation::detect_seals(state, &eval, now);
            state
                .scheduler
                .set_known(eval.summaries.keys().cloned().collect());
            // Claims first: the prune in `on_update` keeps the unsealed mark of an id the
            // running session claimed, which this evaluation may be the first to show.
            if let Some(session) = state.running_session() {
                let held = evaluation::held_by(&eval, &session);
                state.tick_claims = Some(held.clone());
                state
                    .scheduler
                    .claims_changed(&session, &held, now, spawner);
            }
            let events = state.scheduler.on_update(invalidated, stale, now, spawner);
            state.events.extend(events);
            state.latest = Some(eval);
        });
    }

    /// Run `op` under the state lock against a [`Recorder`], then carry out and publish; then
    /// end a started session whose end arrived before its start settled.
    fn run<R>(&self, op: impl FnOnce(&mut State, &dyn SessionSpawner, Instant) -> R) -> R {
        let (result, mut early) = self.run_once(op);
        while let Some((session, outcome)) = early {
            let end = |state: &mut State, spawner: &dyn SessionSpawner, now| {
                state.summary_ended(&session, &outcome, now, spawner);
            };
            early = self.run_once(end).1;
        }
        result
    }

    /// One [`Rebuilds::run`] step; returns the early end of the session it started, if any.
    fn run_once<R>(
        &self,
        op: impl FnOnce(&mut State, &dyn SessionSpawner, Instant) -> R,
    ) -> (R, Option<(String, EndOutcome)>) {
        let recorder = Recorder::default();
        let now = Instant::now();
        let (result, before, mut events) = {
            let mut state = self.lock();
            let before = publish::Snapshot::of(&state.scheduler);
            let result = op(&mut state, &recorder, now);
            let mut events = std::mem::take(&mut state.events);
            events.extend(state.scheduler.take_events());
            (result, before, events)
        };
        let early = self.carry_out(recorder, &mut events);
        self.publish(&before, &events);
        (result, early)
    }

    /// Make the stops and the start the scheduler asked for, then settle the start. Returns
    /// the started session's end when it arrived before the start settled (a launch that
    /// failed at once): the caller applies it after publishing the start.
    fn carry_out(
        &self,
        recorder: Recorder,
        events: &mut Vec<SchedulerEvent>,
    ) -> Option<(String, EndOutcome)> {
        let stops = recorder
            .stops
            .into_inner()
            .unwrap_or_else(PoisonError::into_inner);
        for (session, release_claims) in stops {
            self.spawner.stop(&session, release_claims);
        }
        let start = recorder
            .start
            .into_inner()
            .unwrap_or_else(PoisonError::into_inner);
        let ids = start?;
        let result = self.spawner.start_summary(&ids);
        let (started, triggers, early) = {
            let mut state = self.lock();
            let started = state.scheduler.resolve_start(PENDING, result);
            let ended = std::mem::take(&mut state.ended_early);
            let early = ended
                .into_iter()
                .find(|(id, _)| Some(id) == started.as_ref());
            if started.is_some() {
                state.begin_session();
            }
            events.extend(state.scheduler.take_events());
            (started, state.scheduler.running_triggers(), early)
        };
        events.retain_mut(|event| match event {
            SchedulerEvent::Started { session, .. } if *session == PENDING => match &started {
                Some(real) => {
                    session.clone_from(real);
                    true
                }
                None => false,
            },
            _ => true,
        });
        // A session that already ended was cleared from attribution by its end.
        if let (Some(session), None) = (&started, &early) {
            self.attribute(|attribution| attribution.summary_started(session, triggers));
        }
        early
    }

    /// Fold costs into the log; broadcast a turn's rows and the panel.
    fn record_costs(&self, attributed: Attributed) {
        lock(&self.log).extend(attributed.lines);
        if let Some(turn_cost) = attributed.turn_cost {
            (self.broadcast)(&PlanMessage::SummaryTurnCost(turn_cost));
        }
        self.publish_panel();
    }

    fn attribute<R>(&self, f: impl FnOnce(&mut Attribution) -> R) -> Option<R> {
        self.attribution
            .as_ref()
            .map(|attribution| f(&mut lock(attribution)))
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        lock(&self.inner)
    }
}

/// Every critical section leaves its data consistent, so a poisoned lock is still usable.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
