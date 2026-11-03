//! The facade over a bridge that runs sessions: turn routing by the session's kind, the seal
//! error a stall reports, a session the bridge lists as failed and the cost of an ended one.
//!
//! A host that spawns nothing stands in for tmux; the bridge's driver thread launches the
//! sessions, so these tests wait (bounded) for what the driver does.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::tests_bridge::{ended_with, pressure, turn_ended, Rig, ID};
use super::*;
use crate::plan_server::summaries::scheduler::SchedulerConfig;
use crate::plan_server::wire::summaries::FailedSummary;
use crate::plan_session::bridge::host::{LaunchSpec, SessionHost, Spawned};
use crate::plan_session::bridge::{BridgeEvent, EndOutcome, LaunchGuidance, StartRequest};
use crate::plan_session::kind::{SessionDir, SessionKind};
use crate::plan_session::model::SessionState;
use crate::plan_session::spool::append_event;
use crate::planning::summary::claims::list_claims;
use crate::planning::summary::cost::Trigger;
use crate::process::{IdentityStatus, ProcessIdentity};

/// Launches nothing: a spawn succeeds, or fails when told to; every process stays alive.
#[derive(Default)]
pub(super) struct QuietHost {
    fail_spawn: AtomicBool,
    pub(super) spawns: AtomicUsize,
}

impl SessionHost for QuietHost {
    fn spawn(&self, _spec: &LaunchSpec) -> anyhow::Result<Spawned> {
        anyhow::ensure!(!self.fail_spawn.load(Ordering::SeqCst), "scripted failure");
        self.spawns.fetch_add(1, Ordering::SeqCst);
        let identity = ProcessIdentity {
            pid: 3_900_000,
            start_time: Some(42),
        };
        Ok(Spawned { identity })
    }

    fn deliver(&self, _socket: &str, _target: &str, _text: &str) -> anyhow::Result<()> {
        Ok(())
    }

    fn interrupt(&self, _socket: &str, _target: &str) -> anyhow::Result<()> {
        Ok(())
    }

    fn teardown(&self, _socket: &str) -> anyhow::Result<()> {
        Ok(())
    }

    fn identity(&self, _identity: ProcessIdentity) -> IdentityStatus {
        IdentityStatus::VerifiedAlive
    }
}

pub(super) fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        thread::sleep(Duration::from_millis(10));
    }
}

/// A start request of `kind` with fixed guidance and an opening pasted after `SessionStart`.
pub(super) fn start_request(kind: SessionKind) -> StartRequest {
    let guidance = Box::new(|dir: &SessionDir| {
        Ok::<_, anyhow::Error>(LaunchGuidance {
            brief_path: dir.brief(),
            plugin_dir: dir.plugin(),
            opening_message: "OPEN".to_owned(),
            model: "opus".to_owned(),
            effort: "medium".to_owned(),
            display_name: "loom plan: tests".to_owned(),
            opening_on_argv: false,
        })
    });
    StartRequest {
        kind,
        guidance,
        opening_origin: None,
        offer: None,
        feature: None,
        plan: None,
        extra_env: Vec::new(),
    }
}

/// A rig whose bridge has recovered and runs its driver.
pub(super) struct Live {
    pub(super) rig: Rig,
    pub(super) host: Arc<QuietHost>,
    stop: Arc<AtomicBool>,
    driver: Option<JoinHandle<()>>,
}

impl Live {
    pub(super) fn new() -> Live {
        Live::build(SchedulerConfig::standard(None), false)
    }

    /// A live rig whose scheduler runs on `config`, with attribution when `attributed`.
    pub(super) fn build(config: SchedulerConfig, attributed: bool) -> Live {
        let host = Arc::new(QuietHost::default());
        let rig = Rig::build(host.clone(), true, config, attributed);
        rig.bridge.recover();
        let stop = Arc::new(AtomicBool::new(false));
        let driver = Some(rig.bridge.run(stop.clone()));
        Live {
            rig,
            host,
            stop,
            driver,
        }
    }

    /// Start a session of `kind` in the bridge and return its id.
    pub(super) fn start(&self, kind: SessionKind) -> String {
        let request = start_request(kind);
        self.rig.bridge.start_session(request).expect("the start")
    }

    fn state_of(&self, id: &str) -> Option<SessionState> {
        let mut listed = self.rig.bridge.sessions().into_iter();
        listed.find(|(found, ..)| found == id).map(|(.., s)| s)
    }

    /// The bridge's main transcript of session `id` holds `records`.
    pub(super) fn give_transcript(&self, id: &str, records: &[Value]) {
        let path = self.rig.tmp.path().join(format!("{id}.jsonl"));
        let body: String = records.iter().map(|record| format!("{record}\n")).collect();
        std::fs::write(&path, body).expect("transcript");
        wait_until("the spawn", || self.host.spawns.load(Ordering::SeqCst) > 0);
        let payload = json!({"session_id": id, "transcript_path": path,
            "hook_event_name": "SessionStart", "source": "startup"});
        let dir = self.rig.paths.sessions.join(id);
        append_event(&dir, "SessionStart", &payload).expect("spool the start");
        wait_until("the transcript", || {
            let found = self.rig.bridge.transcripts(id);
            found.and_then(|found| found.main) == Some(path.clone())
        });
    }
}

impl Drop for Live {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(driver) = self.driver.take() {
            let _ = driver.join();
        }
    }
}

#[test]
fn a_turn_suspends_rebuilds_only_in_a_session_the_bridge_knows_as_authoring() {
    let cases = [
        (SessionKind::Authoring, true),
        (pressure(), false),
        (SessionKind::Summary, false),
    ];
    for (kind, holds) in cases {
        let live = Live::new();
        let id = live.start(kind.clone());
        let turn_started = BridgeEvent::TurnStarted {
            id: id.clone(),
            turn_id: "t1".into(),
        };
        live.rig.event(turn_started);
        live.rig.report_missing();
        assert_eq!(live.rig.starts().is_empty(), holds, "turn of {kind:?}");
        if holds {
            live.rig.event(turn_ended(&id, "t1"));
            assert_eq!(live.rig.starts().len(), 1, "the turn's end releases it");
        }
    }
}

fn failed_seal_records() -> Vec<Value> {
    let call = json!({"type": "assistant", "message": {"role": "assistant", "content": [
        {"type": "tool_use", "id": "t1", "name": "Bash",
         "input": {"command": format!("loom plan summary seal {ID}")}}]}});
    let failure = "drafts/plans/a.md:3: unknown citation s-9";
    let block = json!({"type": "tool_result", "tool_use_id": "t1", "content": failure,
        "is_error": true});
    let result = json!({"type": "user", "message": {"role": "user", "content": [block]}});
    vec![call, result]
}

#[test]
fn a_stall_fails_the_ids_with_the_seal_error_read_from_the_transcript() {
    // A zero timeout makes the first tick a stall; the production timeout is ten minutes.
    let config = SchedulerConfig {
        no_seal_timeout: Duration::ZERO,
        ..SchedulerConfig::standard(None)
    };
    let live = Live::build(config, false);
    let id = live.start(SessionKind::Summary);
    live.give_transcript(&id, &failed_seal_records());
    live.rig.probe.answer_next_start(&id);
    live.rig.report_missing();
    live.rig.rebuilds.tick();
    let failed = FailedSummary {
        id: ID.to_owned(),
        error: "drafts/plans/a.md:3: unknown citation s-9".to_owned(),
    };
    assert_eq!(live.rig.rebuilds.panel().failed, vec![failed]);
    assert_eq!(live.rig.stops(), vec![(id, true)]);
}

#[test]
fn a_session_listed_failed_fails_its_ids_instead_of_requeueing_them() {
    let live = Live::new();
    live.host.fail_spawn.store(true, Ordering::SeqCst);
    let id = live.start(SessionKind::Summary);
    wait_until("the failed listing", || {
        live.state_of(&id) == Some(SessionState::Failed)
    });
    live.rig.probe.answer_next_start(&id);
    live.rig.report_missing();
    live.rig.rebuilds.tick();
    assert_eq!(live.rig.rebuilds.panel().failed, vec![], "one sighting");
    live.rig.rebuilds.tick();
    let panel = live.rig.rebuilds.panel();
    let failed = FailedSummary {
        id: ID.to_owned(),
        error: "the session failed".to_owned(),
    };
    assert_eq!((panel.failed, panel.running), (vec![failed], None));
    assert_eq!(live.rig.starts().len(), 1, "not requeued, not restarted");
}

/// Regression: a session the bridge lists as alive must never count as gone, however many
/// ticks see it; otherwise every running rebuild would be ended, requeued and restarted.
#[test]
fn a_session_the_bridge_runs_is_never_ended_by_the_tick() {
    let live = Live::new();
    let id = live.start(SessionKind::Summary);
    wait_until("the spawn", || live.host.spawns.load(Ordering::SeqCst) > 0);
    live.rig.probe.answer_next_start(&id);
    live.rig.report_missing();
    assert_eq!(live.rig.rebuilds.panel().running.expect("runs").session, id);
    for _ in 0..3 {
        live.rig.rebuilds.tick();
    }
    assert!(
        matches!(
            live.state_of(&id),
            Some(SessionState::Starting | SessionState::Working | SessionState::Idle)
        ),
        "the bridge still runs it"
    );
    assert_eq!(live.rig.starts().len(), 1, "not restarted");
    assert_eq!(live.rig.stops(), vec![], "not stopped");
    assert_eq!(
        live.rig
            .rebuilds
            .panel()
            .running
            .expect("still runs")
            .session,
        id
    );
}

/// One assistant record that used `output` output tokens.
pub(super) fn usage_records(output: u64) -> Vec<Value> {
    let usage = json!({"input_tokens": 10, "cache_creation_input_tokens": 200,
        "cache_read_input_tokens": 3000, "output_tokens": output});
    let message = json!({"id": "msg-1", "model": "claude-opus-4-1", "role": "assistant",
        "content": [{"type": "text", "text": "sealed"}], "usage": usage});
    vec![
        json!({"type": "assistant", "uuid": "u-1", "requestId": "req-1",
        "timestamp": "2026-10-29T10:00:01.000Z", "message": message}),
    ]
}

/// The facade's attribution wiring: a summary session's end clears the claims it holds and
/// logs its own transcript's cost, which the panel shows and broadcasts.
#[test]
fn a_summary_session_end_clears_its_claims_and_costs_its_transcript() {
    let live = Live::build(SchedulerConfig::standard(None), true);
    let id = live.start(SessionKind::Summary);
    live.give_transcript(&id, &usage_records(40));
    live.rig.probe.answer_next_start(&id);
    live.rig.report_missing();
    live.rig.claim(&id);
    let claims = list_claims(&live.rig.paths).expect("claims");
    assert_eq!(claims.len(), 1, "the session holds a claim");
    live.rig
        .event(ended_with(&id, SessionKind::Summary, EndOutcome::Closed));
    let claims = list_claims(&live.rig.paths).expect("claims");
    assert!(claims.is_empty(), "the end cleared its claims: {claims:?}");
    let panel = live.rig.rebuilds.panel();
    let last = panel.last.expect("the session's cost line");
    assert_eq!(last.summaries, vec![ID.to_owned()]);
    assert_eq!(last.trigger, Trigger::Background);
    assert_eq!(last.tokens.output, Some(40));
    assert_eq!(panel.week.output, Some(40));
    let costed = |message: &PlanMessage| matches!(message, PlanMessage::SummaryPanel(panel) if panel.last.is_some());
    let sent = live.rig.sent.lock().unwrap();
    assert!(sent.iter().any(costed), "the costed panel was broadcast");
}
