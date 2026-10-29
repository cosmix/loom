//! Summary rebuilds: the scheduler, its facade, the real session spawner, claim attribution and routes.

pub mod attribution;
pub mod rebuilds;
pub mod routes;
pub mod scheduler;
pub mod spawner;

pub use routes::routes;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};
use std::thread;
use std::time::{Duration, Instant};

use self::rebuilds::Rebuilds;
use super::concept::ConceptState;
use super::context::PlanContext;
use super::lifecycle::{line, safe};
use super::shutdown::ShutdownHook;
use crate::plan_session::bridge::Bridge;
use crate::plan_session::model::SessionState;
use crate::quota::poller::spawn_quota_poller;

/// How often the running session's claims and its stall are checked.
const TICK: Duration = Duration::from_secs(5);
/// How often the tick thread looks for the server stopping.
const TICK_SLICE: Duration = Duration::from_millis(250);

/// Ends the quota poller when the context, which owns this through its extensions, drops.
struct PollerStop(Arc<AtomicBool>);

impl Drop for PollerStop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

/// Put [`Rebuilds`] into `ctx.extensions()` and feed it: the start-up sweep after the bridge's
/// recovery, every summary engine update, the bridge's events, a 5 s tick, the quota poller
/// and the server's stop. Needs concept mode's `ConceptState` and the chat's `Bridge`, so it
/// is installed after both.
pub fn install(ctx: &Arc<PlanContext>) {
    let rebuilds = Rebuilds::new(ctx);
    ctx.extensions().insert(Arc::clone(&rebuilds));
    let concept = ctx.extensions().get::<ConceptState>();
    let bridge = ctx.extensions().get::<Bridge>();
    match (&concept, &bridge) {
        (Some(concept), Some(bridge)) => {
            // Subscribed first: the recovery callback below runs at once when recovery is
            // already done, and a session it starts must have its events seen.
            follow_bridge(bridge, &rebuilds);
            after_recovery(bridge, concept, &rebuilds);
        }
        _ => line("summaries: no rebuilds: concept mode or the session bridge is missing"),
    }
    if let Some(concept) = &concept {
        follow_engine(concept, &rebuilds);
    }
    spawn_tick(ctx, &rebuilds);
    let stop = Arc::new(AtomicBool::new(false));
    // Detached: the shutdown hook and `PollerStop` end it within one poll slice.
    drop(spawn_quota_poller(
        ctx.runtime().dir.clone(),
        Arc::clone(&stop),
    ));
    ctx.extensions()
        .insert(Arc::new(PollerStop(Arc::clone(&stop))));
    register_shutdown(ctx, &rebuilds, stop);
}

/// Plan 7's recovery ends the sessions of the last server before this callback runs (at once
/// when recovery is already done): sweep their claims, then queue what the engine's first
/// evaluation found out of date.
fn after_recovery(bridge: &Arc<Bridge>, concept: &Arc<ConceptState>, rebuilds: &Arc<Rebuilds>) {
    let (events, weak) = (Arc::downgrade(bridge), Arc::downgrade(rebuilds));
    let concept = Arc::clone(concept);
    bridge.on_recovered(Box::new(move |report| {
        let (Some(bridge), Some(rebuilds)) = (events.upgrade(), weak.upgrade()) else {
            return;
        };
        let ended =
            |state: &SessionState| matches!(state, SessionState::Ended | SessionState::Failed);
        let sessions = bridge.sessions().into_iter();
        let live: Vec<String> = sessions
            .filter(|(_, _, state)| !ended(state))
            .map(|(id, ..)| id)
            .collect();
        let left_alone: Vec<String> = report
            .left_alone
            .iter()
            .map(|(id, ..)| id.clone())
            .collect();
        rebuilds.after_recovery(&live, &left_alone, concept.latest());
    }));
}

fn follow_bridge(bridge: &Arc<Bridge>, rebuilds: &Arc<Rebuilds>) {
    // Weak: the bridge owns this listener.
    let (events, weak) = (Arc::downgrade(bridge), Arc::downgrade(rebuilds));
    bridge.subscribe(Box::new(move |event| {
        if let (Some(bridge), Some(rebuilds)) = (events.upgrade(), weak.upgrade()) {
            rebuilds.on_bridge_event(&bridge, event);
        }
    }));
}

/// Plan 5 owns the summary engine and hands every update to its subscribers.
fn follow_engine(concept: &ConceptState, rebuilds: &Arc<Rebuilds>) {
    let weak = Arc::downgrade(rebuilds);
    concept.subscribe(Box::new(move |_, update| {
        if let Some(rebuilds) = weak.upgrade() {
            rebuilds.on_update(update);
        }
    }));
}

fn spawn_tick(ctx: &Arc<PlanContext>, rebuilds: &Arc<Rebuilds>) {
    let (ctx, rebuilds) = (Arc::downgrade(ctx), Arc::downgrade(rebuilds));
    let spawned = thread::Builder::new()
        .name("loom-plan-summaries".into())
        .spawn(move || run_ticks(&ctx, &rebuilds));
    if let Err(error) = spawned {
        let why = safe(&error.to_string());
        line(&format!(
            "summaries: no tick thread, stalls go unnoticed: {why}"
        ));
    }
}

/// Tick every [`TICK`] until the server stops or its context is gone.
fn run_ticks(ctx: &Weak<PlanContext>, rebuilds: &Weak<Rebuilds>) {
    let mut next = Instant::now() + TICK;
    loop {
        thread::sleep(TICK_SLICE);
        let (Some(ctx), Some(rebuilds)) = (ctx.upgrade(), rebuilds.upgrade()) else {
            return;
        };
        if ctx.is_stopping() {
            return;
        }
        if Instant::now() >= next {
            rebuilds.tick();
            next = Instant::now() + TICK;
        }
    }
}

/// The first Ctrl-C drops the queue and the question and stops the quota poller; plan 7's
/// drain ends a running summary session, so this drain returns at once.
fn register_shutdown(ctx: &Arc<PlanContext>, rebuilds: &Arc<Rebuilds>, stop: Arc<AtomicBool>) {
    let (weak, abort_stop) = (Arc::downgrade(rebuilds), Arc::clone(&stop));
    ctx.shutdown().register(ShutdownHook {
        name: "summaries",
        drain: Box::new(move || {
            stop.store(true, Ordering::SeqCst);
            if let Some(rebuilds) = weak.upgrade() {
                rebuilds.server_stopping();
            }
        }),
        abort: Box::new(move || abort_stop.store(true, Ordering::SeqCst)),
    });
}
