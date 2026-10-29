//! The facade's bridge-event routing, the claims-stability rule and the tick's liveness check.
//!
//! The bridge here knows no session (its host is never called), so `kind_of` answers `None`;
//! the kind of a turn's session is passed to `turn_started` directly. `tests_alive` builds on
//! this rig with a bridge that runs sessions.

use std::path::PathBuf;
use std::time::Duration;

use tempfile::TempDir;

use super::tests::{owned, update, Probe, Row, Sent};
use super::*;
use crate::plan_server::wire::summaries::FailedSummary;
use crate::plan_session::bridge::host::SessionHost;
use crate::plan_session::bridge::{BridgeConfig, BridgeEvent, EndOutcome};
use crate::plan_session::kind::SessionKind;
use crate::plan_session::model::SessionState;
use crate::plan_session::tmux_host::TmuxHost;
use crate::planning::runtime::{create_runtime_dir, runtime_paths_under};
use crate::planning::summary::claims::{create_claim_as, delete_claim};
use crate::planning::summary::evaluate::SummaryState::Missing;

pub(super) const ID: &str = "plans/a";

pub(super) struct Rig {
    pub(super) probe: Arc<Probe>,
    pub(super) rebuilds: Arc<Rebuilds>,
    pub(super) bridge: Arc<Bridge>,
    pub(super) paths: RuntimePaths,
    pub(super) tmp: TempDir,
    /// Every message the facade broadcast.
    pub(super) sent: Sent,
}

impl Rig {
    /// A facade over a temp runtime directory; `env_bridge` lets its tick see the bridge, which
    /// knows no session.
    pub(super) fn new(env_bridge: bool) -> Rig {
        Rig::with_host(Arc::new(TmuxHost), env_bridge)
    }

    /// [`Rig::new`] over `host`, the program the bridge launches sessions with.
    pub(super) fn with_host(host: Arc<dyn SessionHost>, env_bridge: bool) -> Rig {
        Rig::build(host, env_bridge, SchedulerConfig::standard(None), false)
    }

    /// [`Rig::with_host`] with the scheduler's `config` and, with `attributed`, attribution
    /// over the rig's runtime directory.
    pub(super) fn build(
        host: Arc<dyn SessionHost>,
        env_bridge: bool,
        scheduler: SchedulerConfig,
        attributed: bool,
    ) -> Rig {
        let tmp = TempDir::new().expect("temp dir");
        let root: PathBuf = tmp.path().join("project");
        std::fs::create_dir_all(&root).expect("project root");
        let paths = runtime_paths_under(&tmp.path().join("loom-home"), &root).expect("paths");
        create_runtime_dir(&paths).expect("runtime dir");
        let config = BridgeConfig {
            project_root: root.clone(),
            runtime: paths.clone(),
            loom_bin: root.join("loom"),
            claude_bin: root.join("claude"),
            home: root.join("home"),
            closing_message: Arc::new(String::new),
            delivery_timeout: Duration::from_secs(10),
            final_write_bound: Duration::from_secs(600),
            permission_deny_after: Duration::from_secs(3590),
            identity_interval: Duration::from_secs(2),
            tick: Duration::from_millis(20),
        };
        let bridge = Bridge::new(config, host, None);
        let probe = Arc::new(Probe::default());
        let held = env_bridge.then(|| Arc::clone(&bridge));
        let sent: Sent = Arc::default();
        let sink = Arc::clone(&sent);
        let broadcast: Broadcast =
            Arc::new(move |message: &PlanMessage| sink.lock().unwrap().push(message.clone()));
        let attribution = attributed.then(|| Attribution::new(paths.clone()));
        let (spawner, runtime) = (probe.clone(), paths.clone());
        let rebuilds =
            Rebuilds::new_for_test_in(spawner, scheduler, broadcast, runtime, held, attribution);
        probe.rebuilds.set(Arc::downgrade(&rebuilds)).ok();
        Rig {
            probe,
            rebuilds,
            bridge,
            paths,
            tmp,
            sent,
        }
    }

    pub(super) fn event(&self, event: BridgeEvent) {
        self.rebuilds.on_bridge_event(&self.bridge, &event);
    }

    /// The engine reports `ID` missing: a rebuild starts unless something suspends rebuilds.
    pub(super) fn report_missing(&self) {
        let rows: [Row<'_>; 1] = [(ID, Missing, None, None)];
        self.rebuilds.on_update(&update(&rows, &[ID]));
    }

    pub(super) fn starts(&self) -> Vec<Vec<String>> {
        self.probe.starts()
    }

    pub(super) fn stops(&self) -> Vec<(String, bool)> {
        self.probe.stops.lock().unwrap().clone()
    }

    pub(super) fn claim(&self, session: &str) {
        let made = create_claim_as(&self.paths, ID, Some(session.to_owned())).expect("claim");
        made.expect("no earlier claim");
    }

    fn release(&self) {
        delete_claim(&self.paths, ID).expect("delete the claim");
    }
}

fn started(id: &str, kind: SessionKind) -> BridgeEvent {
    BridgeEvent::Started {
        id: id.into(),
        kind,
    }
}

fn state(id: &str, state: SessionState) -> BridgeEvent {
    BridgeEvent::State {
        id: id.into(),
        state,
    }
}

pub(super) fn turn_ended(id: &str, turn: &str) -> BridgeEvent {
    BridgeEvent::TurnEnded {
        id: id.into(),
        turn_id: turn.into(),
        changed: Vec::new(),
    }
}

fn ended(id: &str, kind: SessionKind) -> BridgeEvent {
    ended_with(id, kind, EndOutcome::Closed)
}

pub(super) fn ended_with(id: &str, kind: SessionKind, outcome: EndOutcome) -> BridgeEvent {
    BridgeEvent::Ended {
        id: id.into(),
        kind,
        outcome,
    }
}

pub(super) fn pressure() -> SessionKind {
    SessionKind::Pressure {
        plan: PathBuf::from("doc/plans/PLAN-x.md"),
    }
}

/// An authoring session `chat` with turn `t1` open and a rebuild held back by it.
fn held_by_authoring_turn() -> Rig {
    let rig = Rig::new(false);
    rig.event(started("chat", SessionKind::Authoring));
    rig.rebuilds
        .turn_started(Some(SessionKind::Authoring), "chat", "t1");
    rig.report_missing();
    assert!(rig.starts().is_empty(), "the open turn holds rebuilds");
    rig
}

#[test]
fn idle_or_ended_releases_the_turn_only_of_the_session_that_holds_it() {
    for end in [SessionState::Idle, SessionState::Ended] {
        let rig = held_by_authoring_turn();
        rig.event(state("another", end));
        assert!(rig.starts().is_empty(), "{end:?} of another session");
        rig.event(state("chat", SessionState::Working));
        assert!(rig.starts().is_empty(), "Working releases nothing");
        rig.event(state("chat", end));
        assert_eq!(rig.starts(), vec![owned(&[ID])], "{end:?} of the holder");
    }
}

#[test]
fn only_an_authoring_turn_suspends_rebuilds() {
    let held = [(Some(SessionKind::Authoring), true)];
    let free = [
        (Some(SessionKind::Summary), false),
        (Some(pressure()), false),
        (None, false),
    ];
    for (kind, holds) in held.into_iter().chain(free) {
        let rig = Rig::new(false);
        rig.rebuilds.turn_started(kind.clone(), "s", "t1");
        rig.report_missing();
        assert_eq!(rig.starts().is_empty(), holds, "turn of {kind:?}");
    }
}

#[test]
fn a_summary_session_start_suspends_nothing() {
    let rig = Rig::new(false);
    rig.event(started("sum", SessionKind::Summary));
    rig.report_missing();
    assert_eq!(rig.starts(), vec![owned(&[ID])]);
}

#[test]
fn a_pressure_run_holds_rebuilds_from_its_start_to_its_end() {
    let rig = Rig::new(false);
    rig.event(started("p1", pressure()));
    rig.event(started("p2", pressure()));
    rig.report_missing();
    assert!(rig.starts().is_empty(), "a pressure run holds");
    rig.event(ended("p1", pressure()));
    assert!(rig.starts().is_empty(), "p2 still holds");
    rig.event(ended("unknown", pressure()));
    assert!(
        rig.starts().is_empty(),
        "an unknown session releases nothing"
    );
    rig.event(ended("p2", pressure()));
    assert_eq!(rig.starts(), vec![owned(&[ID])]);
}

#[test]
fn a_pressure_run_and_an_authoring_turn_are_counted_separately() {
    let rig = Rig::new(false);
    rig.event(started("p1", pressure()));
    rig.event(started("chat", SessionKind::Authoring));
    rig.rebuilds
        .turn_started(Some(SessionKind::Authoring), "chat", "t1");
    rig.report_missing();
    rig.event(turn_ended("chat", "t1"));
    assert!(rig.starts().is_empty(), "the pressure run still holds");
    rig.event(ended("p1", pressure()));
    assert_eq!(rig.starts(), vec![owned(&[ID])]);

    let rig = Rig::new(false);
    rig.event(started("p1", pressure()));
    rig.event(started("chat", SessionKind::Authoring));
    rig.rebuilds
        .turn_started(Some(SessionKind::Authoring), "chat", "t1");
    rig.report_missing();
    rig.event(ended("p1", pressure()));
    assert!(rig.starts().is_empty(), "the authoring turn still holds");
    rig.event(turn_ended("chat", "t1"));
    assert_eq!(rig.starts(), vec![owned(&[ID])]);
}

/// A rebuild of `ID` running as session `s1` that has claimed `ID`, seen by two claims ticks.
fn claimed_run() -> Rig {
    let rig = Rig::new(false);
    rig.report_missing();
    assert_eq!(rig.starts(), vec![owned(&[ID])]);
    rig.claim("s1");
    rig.rebuilds.tick();
    rig.rebuilds.tick();
    assert!(rig.stops().is_empty(), "a held claim keeps the session");
    rig
}

#[test]
fn one_differing_claims_listing_stops_nothing() {
    let rig = claimed_run();
    rig.release();
    rig.rebuilds.tick();
    assert!(
        rig.stops().is_empty(),
        "a seal's empty listing needs a second"
    );
    rig.claim("s1");
    rig.rebuilds.tick();
    rig.rebuilds.tick();
    assert!(rig.stops().is_empty(), "the claim came back");
    assert_eq!(rig.starts().len(), 1, "nothing was requeued");
}

#[test]
fn two_agreeing_empty_listings_stop_a_session_with_no_claims_left() {
    let rig = claimed_run();
    rig.release();
    rig.rebuilds.tick();
    assert!(rig.stops().is_empty());
    rig.rebuilds.tick();
    assert_eq!(rig.stops(), vec![("s1".to_owned(), false)]);
}

#[test]
fn the_tick_ends_a_summary_session_the_bridge_no_longer_runs() {
    let rig = Rig::new(true);
    rig.report_missing();
    assert_eq!(rig.rebuilds.panel().running.expect("s1").session, "s1");
    rig.rebuilds.tick();
    assert_eq!(rig.starts().len(), 1, "one sighting ends nothing");
    rig.rebuilds.tick();
    assert_eq!(
        rig.starts(),
        vec![owned(&[ID]), owned(&[ID])],
        "requeued, restarted"
    );
    assert_eq!(rig.rebuilds.panel().running.expect("s2").session, "s2");
}

#[test]
fn an_end_delivered_between_two_ticks_fails_the_ids_with_its_reason() {
    let rig = Rig::new(true);
    rig.report_missing();
    rig.rebuilds.tick();
    let why = "the tmux server died";
    rig.event(ended_with(
        "s1",
        SessionKind::Summary,
        EndOutcome::Failed(why.into()),
    ));
    rig.rebuilds.tick();
    let panel = rig.rebuilds.panel();
    let failed = FailedSummary {
        id: ID.to_owned(),
        error: why.to_owned(),
    };
    assert_eq!(panel.failed, vec![failed]);
    assert_eq!(panel.running, None);
    assert_eq!(rig.starts().len(), 1, "failed ids are not requeued");
}

#[test]
fn a_sighting_belongs_to_the_session_it_saw() {
    let rig = Rig::new(true);
    rig.report_missing();
    rig.rebuilds.tick();
    rig.event(ended("s1", SessionKind::Summary));
    assert_eq!(rig.starts().len(), 2, "the end restarted the ids as s2");
    rig.rebuilds.tick();
    assert_eq!(rig.starts().len(), 2, "s2's first sighting ends nothing");
    rig.rebuilds.tick();
    assert_eq!(rig.starts().len(), 3, "s2's second sighting does");
}

#[test]
fn the_tick_leaves_a_session_alone_without_a_bridge() {
    let rig = Rig::new(false);
    rig.report_missing();
    rig.rebuilds.tick();
    assert_eq!(rig.starts().len(), 1);
    assert_eq!(rig.rebuilds.panel().running.expect("s1").session, "s1");
}
