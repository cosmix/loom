//! The watchdog for an adjudication session that is alive and not working.
//!
//! A judge is the one agent with no worktree and no claim on the stage it
//! works for, so it writes its heartbeat to a key of its own and is measured
//! on a path of its own. Both halves are checked here: that the judge's
//! heartbeat never reaches the stage agent's, and that a judge which stops
//! making tool calls is reported exactly once.

use std::path::Path;

use crate::models::session::{Session, SessionStatus};
use crate::models::stage::Stage;
use crate::orchestrator::liveness::LivenessService;
use crate::orchestrator::monitor::detection::Detection;
use crate::orchestrator::monitor::handlers::Handlers;
use crate::orchestrator::monitor::heartbeat::{
    cleanup_judge_heartbeat, judge_heartbeat_path, Heartbeat, HeartbeatWatcher,
};
use crate::orchestrator::monitor::{MonitorConfig, MonitorEvent};

/// The disputed stage's response budget in these tests. Long enough that the
/// built-in default could never stand in for it by accident.
const BUDGET_SECS: u64 = 900;

/// Write a judge heartbeat for `stage_id`, `age_secs` old.
fn write_judge_heartbeat(work_dir: &Path, stage_id: &str, session_id: &str, age_secs: i64) {
    let mut heartbeat = Heartbeat::new(stage_id.to_string(), session_id.to_string());
    heartbeat.timestamp = chrono::Utc::now() - chrono::Duration::seconds(age_secs);
    let path = judge_heartbeat_path(work_dir, stage_id);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, serde_json::to_string_pretty(&heartbeat).unwrap()).unwrap();
}

/// A live judge for `stage_id`, spawned `age_secs` ago.
fn judge(stage_id: &str, session_id: &str, age_secs: i64) -> Session {
    let mut session = Session::new_adjudication(stage_id);
    session.id = session_id.to_string();
    session.status = SessionStatus::Running;
    session.created_at = chrono::Utc::now() - chrono::Duration::seconds(age_secs);
    session
}

fn disputed_stage(stage_id: &str) -> Stage {
    Stage {
        id: stage_id.to_string(),
        subagent_timeout_secs: Some(BUDGET_SECS),
        ..Stage::default()
    }
}

/// `Handlers` whose liveness probe always answers "alive", which is the
/// precondition for every stall report: a dead judge is a crash, and crash
/// detection owns that.
fn handlers_for(work_dir: &Path) -> (MonitorConfig, Handlers) {
    let config = MonitorConfig {
        work_dir: work_dir.to_path_buf(),
        ..Default::default()
    };
    let handlers = Handlers::new(config.clone(), Some(LivenessService::fixed_for_tests(true)));
    (config, handlers)
}

fn stall_events(events: &[MonitorEvent]) -> Vec<&MonitorEvent> {
    events
        .iter()
        .filter(|event| matches!(event, MonitorEvent::AdjudicatorStalled { .. }))
        .collect()
}

/// A judge heartbeat is keyed by stage but is NOT the stage's heartbeat. If it
/// landed in the ordinary map it would answer for the stage agent's silence,
/// and its `HeartbeatUpdate` would write the judge's context reading onto the
/// stage agent's session record.
#[test]
fn a_judge_heartbeat_never_reaches_the_stage_agents_key() {
    let temp = tempfile::TempDir::new().unwrap();
    let work_dir = temp.path();
    write_judge_heartbeat(work_dir, "disputed-stage", "judge-1", 0);

    let mut watcher = HeartbeatWatcher::new();
    let updates = watcher.poll(work_dir).unwrap();

    assert!(
        updates.is_empty(),
        "a judge heartbeat must not be published as a stage heartbeat update, got: {updates:?}"
    );
    assert!(
        watcher.get_heartbeat("disputed-stage").is_none(),
        "a judge heartbeat must not occupy the stage agent's key"
    );
    assert!(
        watcher
            .get_heartbeat("disputed-stage.adjudication")
            .is_none(),
        "the file stem must not be mistaken for a stage id"
    );
    let cached = watcher
        .judge_heartbeat("disputed-stage")
        .expect("the judge heartbeat must be cached under the real stage id");
    assert_eq!(cached.session_id, "judge-1");

    watcher.remove_judge("disputed-stage");
    assert!(watcher.judge_heartbeat("disputed-stage").is_none());
}

/// Closing a judge removes its heartbeat, and a judge closed before its first
/// tool call never wrote one — so a missing file is success, not an error.
#[test]
fn cleaning_up_a_judge_heartbeat_tolerates_a_missing_file() {
    let temp = tempfile::TempDir::new().unwrap();
    let work_dir = temp.path();
    write_judge_heartbeat(work_dir, "disputed-stage", "judge-1", 0);
    let path = judge_heartbeat_path(work_dir, "disputed-stage");
    assert!(
        path.exists(),
        "fixture heartbeat must exist to be deletable"
    );

    cleanup_judge_heartbeat(work_dir, "disputed-stage");
    assert!(!path.exists());

    cleanup_judge_heartbeat(work_dir, "disputed-stage");
    cleanup_judge_heartbeat(work_dir, "never-existed");
}

/// The case the watchdog exists for: a judge that never reached a tool call,
/// so it has no heartbeat at all and is measured from its spawn. Reported
/// once — the report closes it, so a repeat every poll would be noise about a
/// session that is already gone.
#[test]
fn a_judge_that_never_worked_is_reported_once() {
    let temp = tempfile::TempDir::new().unwrap();
    let (config, handlers) = handlers_for(temp.path());
    let mut watcher = HeartbeatWatcher::new();
    let mut detection = Detection::new();

    let sessions = [judge("disputed-stage", "judge-1", BUDGET_SECS as i64 + 60)];
    let stages = [disputed_stage("disputed-stage")];

    let events =
        detection.detect_heartbeat_events(&sessions, &stages, &mut watcher, &config, &handlers);
    let stalled = stall_events(&events);
    assert_eq!(
        stalled.len(),
        1,
        "expected one stall report, got: {events:?}"
    );
    let MonitorEvent::AdjudicatorStalled {
        session_id,
        stage_id,
        stale_duration_secs,
        timeout_secs,
    } = stalled[0]
    else {
        unreachable!("filtered above")
    };
    assert_eq!(session_id, "judge-1");
    assert_eq!(stage_id, "disputed-stage");
    assert_eq!(*timeout_secs, BUDGET_SECS);
    assert!(*stale_duration_secs > BUDGET_SECS);

    let repeat =
        detection.detect_heartbeat_events(&sessions, &stages, &mut watcher, &config, &handlers);
    assert!(
        stall_events(&repeat).is_empty(),
        "the stall latch must suppress the second report, got: {repeat:?}"
    );
}

/// A judge inside its budget is working, however long it has been running.
#[test]
fn a_judge_that_answered_recently_is_left_alone() {
    let temp = tempfile::TempDir::new().unwrap();
    let (config, handlers) = handlers_for(temp.path());
    write_judge_heartbeat(temp.path(), "disputed-stage", "judge-1", 30);

    let mut watcher = HeartbeatWatcher::new();
    let mut detection = Detection::new();
    let sessions = [judge("disputed-stage", "judge-1", BUDGET_SECS as i64 + 60)];
    let stages = [disputed_stage("disputed-stage")];

    let events =
        detection.detect_heartbeat_events(&sessions, &stages, &mut watcher, &config, &handlers);
    assert!(
        stall_events(&events).is_empty(),
        "a judge that made a tool call 30s ago is working, got: {events:?}"
    );
}

/// A heartbeat left on the stage's adjudication key by a PREVIOUS judge says
/// nothing about the one running now. Without the identity check its fresh
/// timestamp would vouch for a successor that has never done anything.
#[test]
fn a_previous_judges_heartbeat_does_not_vouch_for_the_current_one() {
    let temp = tempfile::TempDir::new().unwrap();
    let (config, handlers) = handlers_for(temp.path());
    write_judge_heartbeat(temp.path(), "disputed-stage", "judge-1", 0);

    let mut watcher = HeartbeatWatcher::new();
    let mut detection = Detection::new();
    let sessions = [judge("disputed-stage", "judge-2", BUDGET_SECS as i64 + 60)];
    let stages = [disputed_stage("disputed-stage")];

    let events =
        detection.detect_heartbeat_events(&sessions, &stages, &mut watcher, &config, &handlers);
    assert_eq!(
        stall_events(&events).len(),
        1,
        "a heartbeat naming judge-1 must not excuse judge-2, got: {events:?}"
    );
}

/// A stage that declares a zero budget would otherwise have every judge closed
/// on its first poll. Warn-only is the same rule `is_stall_escalation` applies
/// to stage agents.
#[test]
fn a_zero_budget_never_closes_a_judge() {
    let temp = tempfile::TempDir::new().unwrap();
    let (config, handlers) = handlers_for(temp.path());
    let mut watcher = HeartbeatWatcher::new();
    let mut detection = Detection::new();

    let sessions = [judge("disputed-stage", "judge-1", 5_000)];
    let stages = [Stage {
        id: "disputed-stage".to_string(),
        subagent_timeout_secs: Some(0),
        ..Stage::default()
    }];

    let events =
        detection.detect_heartbeat_events(&sessions, &stages, &mut watcher, &config, &handlers);
    assert!(
        stall_events(&events).is_empty(),
        "a stage declaring no real budget gets no judge watchdog, got: {events:?}"
    );
}

/// The stage-agent path is untouched: a session with no heartbeat is a session
/// that has not reached its first tool call, and stays silent as before.
#[test]
fn a_stage_agent_without_a_heartbeat_still_emits_nothing() {
    let temp = tempfile::TempDir::new().unwrap();
    let (config, handlers) = handlers_for(temp.path());
    let mut watcher = HeartbeatWatcher::new();
    let mut detection = Detection::new();

    let mut agent = Session::new();
    agent.id = "session-1".to_string();
    agent.status = SessionStatus::Running;
    agent.stage_id = Some("disputed-stage".to_string());
    agent.created_at = chrono::Utc::now() - chrono::Duration::seconds(BUDGET_SECS as i64 + 60);

    let events = detection.detect_heartbeat_events(
        &[agent],
        &[disputed_stage("disputed-stage")],
        &mut watcher,
        &config,
        &handlers,
    );
    assert!(
        events.is_empty(),
        "a stage agent is judged on its heartbeat file alone, got: {events:?}"
    );
}
