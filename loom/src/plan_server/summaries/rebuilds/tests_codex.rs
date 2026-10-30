//! A summary job the spawner runs outside the session bridge (a Codex job): the liveness tick
//! leaves it alone, its watcher's end reaches the scheduler like a bridge `Ended`, an end
//! that beats the start's settling is kept, and the server stop reaches the spawner.

use std::sync::OnceLock;

use super::tests::{update, Row};
use super::tests_bridge::{Rig, ID};
use super::*;
use crate::plan_server::summaries::scheduler::SchedulerConfig;
use crate::plan_server::wire::summaries::FailedSummary;
use crate::planning::summary::evaluate::SummaryState::Missing;

const SESSION: &str = "codex-1-0";

/// A spawner whose sessions are the Codex spawner's: `runs_outside_bridge` says whether the
/// session it answers is one.
struct CodexProbe {
    outside: bool,
    starts: Mutex<Vec<Vec<String>>>,
    stops: Mutex<Vec<(String, bool)>>,
    stop_alls: Mutex<usize>,
    /// An end the next start reports for its own session before it returns, as a watcher
    /// that sees the process exit at once does.
    end_first: Mutex<Option<EndOutcome>>,
    rebuilds: OnceLock<Weak<Rebuilds>>,
}

impl CodexProbe {
    fn new(outside: bool) -> Arc<CodexProbe> {
        Arc::new(CodexProbe {
            outside,
            starts: Mutex::default(),
            stops: Mutex::default(),
            stop_alls: Mutex::default(),
            end_first: Mutex::default(),
            rebuilds: OnceLock::new(),
        })
    }

    fn stops(&self) -> Vec<(String, bool)> {
        lock(&self.stops).clone()
    }

    fn stop_alls(&self) -> usize {
        *lock(&self.stop_alls)
    }
}

impl SessionSpawner for CodexProbe {
    fn start_summary(&self, ids: &[String]) -> anyhow::Result<String> {
        lock(&self.starts).push(ids.to_vec());
        let end = lock(&self.end_first).take();
        let rebuilds = self.rebuilds.get().and_then(Weak::upgrade);
        if let (Some(outcome), Some(rebuilds)) = (end, rebuilds) {
            rebuilds.codex_session_ended(SESSION, &outcome);
        }
        Ok(SESSION.to_owned())
    }

    fn stop(&self, session: &str, release_claims: bool) {
        lock(&self.stops).push((session.to_owned(), release_claims));
    }

    fn runs_outside_bridge(&self, session: &str) -> bool {
        self.outside && session == SESSION
    }

    fn stop_all(&self) {
        *lock(&self.stop_alls) += 1;
    }
}

/// A facade over `probe` whose tick sees a bridge that knows no session.
fn facade_over(rig: &Rig, probe: &Arc<CodexProbe>) -> Arc<Rebuilds> {
    let broadcast: Broadcast = Arc::new(|_: &PlanMessage| {});
    let rebuilds = Rebuilds::new_for_test_in(
        probe.clone(),
        SchedulerConfig::standard(None),
        broadcast,
        rig.paths.clone(),
        Some(Arc::clone(&rig.bridge)),
        None,
    );
    probe.rebuilds.set(Arc::downgrade(&rebuilds)).ok();
    rebuilds
}

fn report_missing(rebuilds: &Rebuilds) {
    let rows: [Row<'_>; 1] = [(ID, Missing, None, None)];
    rebuilds.on_update(&update(&rows, &[ID]));
}

fn failed(error: &str) -> Vec<FailedSummary> {
    vec![FailedSummary {
        id: ID.to_owned(),
        error: error.to_owned(),
    }]
}

fn running_session(rebuilds: &Rebuilds) -> Option<String> {
    rebuilds.panel().running.map(|running| running.session)
}

#[test]
fn the_tick_never_ends_a_job_its_spawner_runs() {
    let rig = Rig::new(true);
    let probe = CodexProbe::new(true);
    let rebuilds = facade_over(&rig, &probe);
    report_missing(&rebuilds);
    assert_eq!(running_session(&rebuilds).as_deref(), Some(SESSION));
    for _ in 0..4 {
        rebuilds.tick();
    }
    assert_eq!(running_session(&rebuilds).as_deref(), Some(SESSION));
    assert_eq!(lock(&probe.starts).len(), 1, "not restarted");
    assert_eq!(probe.stops(), vec![], "not stopped");
}

#[test]
fn the_tick_ends_a_session_that_is_neither_in_the_bridge_nor_a_job() {
    let rig = Rig::new(true);
    let probe = CodexProbe::new(false);
    let rebuilds = facade_over(&rig, &probe);
    report_missing(&rebuilds);
    rebuilds.tick();
    rebuilds.tick();
    assert_eq!(
        lock(&probe.starts).len(),
        2,
        "ended, requeued and restarted"
    );
}

#[test]
fn a_failed_job_fails_its_ids_like_a_failed_bridge_session() {
    let rig = Rig::new(true);
    let probe = CodexProbe::new(true);
    let rebuilds = facade_over(&rig, &probe);
    report_missing(&rebuilds);
    rebuilds.codex_session_ended(SESSION, &EndOutcome::Failed("codex exited".to_owned()));
    let panel = rebuilds.panel();
    assert_eq!(
        (panel.failed, panel.running),
        (failed("codex exited"), None)
    );
    assert_eq!(lock(&probe.starts).len(), 1, "not requeued, not restarted");
}

#[test]
fn a_torn_down_job_requeues_its_ids() {
    let rig = Rig::new(true);
    let probe = CodexProbe::new(true);
    let rebuilds = facade_over(&rig, &probe);
    report_missing(&rebuilds);
    rebuilds.codex_session_ended(SESSION, &EndOutcome::TornDown);
    assert_eq!(
        rebuilds.panel().failed,
        vec![],
        "a requested stop is no failure"
    );
    assert_eq!(lock(&probe.starts).len(), 2, "requeued and started again");
}

#[test]
fn a_job_end_that_beats_the_start_settling_is_applied() {
    let rig = Rig::new(true);
    let probe = CodexProbe::new(true);
    let rebuilds = facade_over(&rig, &probe);
    *lock(&probe.end_first) = Some(EndOutcome::Failed("launch failed".to_owned()));
    report_missing(&rebuilds);
    let panel = rebuilds.panel();
    assert_eq!(panel.failed, failed("launch failed"));
    assert_eq!(panel.running, None);
    assert_eq!(lock(&probe.starts).len(), 1, "not restarted");
}

#[test]
fn the_server_stop_stops_a_running_job_and_releases_its_claims() {
    let rig = Rig::new(true);
    let probe = CodexProbe::new(true);
    let rebuilds = facade_over(&rig, &probe);
    report_missing(&rebuilds);
    rebuilds.server_stopping();
    assert_eq!(probe.stops(), vec![(SESSION.to_owned(), true)]);
    assert_eq!(probe.stop_alls(), 1, "also stops a start still settling");
}

#[test]
fn the_server_stop_leaves_a_bridge_session_to_the_bridge_drain() {
    let rig = Rig::new(true);
    let probe = CodexProbe::new(false);
    let rebuilds = facade_over(&rig, &probe);
    report_missing(&rebuilds);
    rebuilds.server_stopping();
    assert_eq!(probe.stops(), vec![]);
    assert_eq!(probe.stop_alls(), 1, "a pending start is stopped too");
}
