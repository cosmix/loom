//! A stage session that never wrote a heartbeat of its own is measured from
//! its spawn, so one that never reaches its first tool call is reported once
//! its budget runs out instead of staying silent forever.

use crate::models::session::{Session, SessionStatus, SessionType};
use crate::models::stage::Stage;
use crate::orchestrator::liveness::LivenessService;
use crate::orchestrator::monitor::detection::Detection;
use crate::orchestrator::monitor::handlers::Handlers;
use crate::orchestrator::monitor::heartbeat::{write_heartbeat, Heartbeat, HeartbeatWatcher};
use crate::orchestrator::monitor::{MonitorConfig, MonitorEvent};

fn fixed_harness(
    work_dir: &std::path::Path,
    now: chrono::DateTime<chrono::Utc>,
) -> (MonitorConfig, Handlers, HeartbeatWatcher) {
    let config = MonitorConfig {
        work_dir: work_dir.to_path_buf(),
        ..Default::default()
    };
    let handlers = Handlers::new(config.clone(), Some(LivenessService::fixed_for_tests(true)));
    (config, handlers, HeartbeatWatcher::with_now(now))
}

fn running_pair(stage_id: &str, session_id: &str) -> (Session, Stage) {
    let mut session = Session::new();
    session.id = session_id.to_string();
    session.status = SessionStatus::Running;
    session.stage_id = Some(stage_id.to_string());
    let stage = Stage {
        id: stage_id.to_string(),
        session: Some(session_id.to_string()),
        subagent_timeout_secs: Some(60),
        ..Stage::default()
    };
    (session, stage)
}

/// `session-1` on `stage-1`, spawned `age_secs` before `now`.
fn spawned(now: chrono::DateTime<chrono::Utc>, age_secs: i64) -> (Session, Stage) {
    let (mut session, stage) = running_pair("stage-1", "session-1");
    session.created_at = now - chrono::Duration::seconds(age_secs);
    (session, stage)
}

fn detect(
    work_dir: &std::path::Path,
    now: chrono::DateTime<chrono::Utc>,
    session: Session,
    stage: Stage,
) -> Vec<MonitorEvent> {
    let (config, handlers, mut watcher) = fixed_harness(work_dir, now);
    Detection::new().detect_heartbeat_events(&[session], &[stage], &mut watcher, &config, &handlers)
}

fn hung_events(events: Vec<MonitorEvent>) -> Vec<MonitorEvent> {
    events
        .into_iter()
        .filter(|event| matches!(event, MonitorEvent::SessionHung { .. }))
        .collect()
}

fn never_started_report() -> MonitorEvent {
    MonitorEvent::SessionHung {
        session_id: "session-1".to_string(),
        stage_id: Some("stage-1".to_string()),
        stale_duration_secs: 120,
        timeout_secs: 60,
        last_activity: None,
        finished_without_completing: false,
    }
}

#[test]
fn a_session_with_no_heartbeat_past_its_budget_is_reported_hung() {
    let temp = tempfile::TempDir::new().unwrap();
    let now = chrono::Utc::now();
    let (session, stage) = spawned(now, 120);

    let events = detect(temp.path(), now, session, stage);

    assert_eq!(events, vec![never_started_report()]);
}

#[test]
fn a_session_with_no_heartbeat_inside_its_budget_is_not_reported() {
    let temp = tempfile::TempDir::new().unwrap();
    let now = chrono::Utc::now();
    let (session, stage) = spawned(now, 30);

    let events = detect(temp.path(), now, session, stage);

    assert!(events.is_empty(), "inside its budget, got: {events:?}");
}

#[test]
fn a_previous_sessions_heartbeat_does_not_describe_a_never_started_session() {
    let temp = tempfile::TempDir::new().unwrap();
    let now = chrono::Utc::now();
    let (session, stage) = spawned(now, 120);
    let previous = Heartbeat::new("stage-1".to_string(), "old-session".to_string())
        .with_last_tool("Bash".to_string())
        .with_activity("Running cargo test".to_string());
    write_heartbeat(temp.path(), &previous).unwrap();

    let events = detect(temp.path(), now, session, stage);

    assert_eq!(hung_events(events), vec![never_started_report()]);
}

#[test]
fn a_zero_budget_never_reports_a_session_without_a_heartbeat() {
    let temp = tempfile::TempDir::new().unwrap();
    let now = chrono::Utc::now();
    let (session, mut stage) = spawned(now, 5_000);
    stage.subagent_timeout_secs = Some(0);

    let events = detect(temp.path(), now, session, stage);

    assert!(
        events.is_empty(),
        "a zero budget reports nothing, got: {events:?}"
    );
}

#[test]
fn a_merge_session_without_a_heartbeat_is_not_reported() {
    let temp = tempfile::TempDir::new().unwrap();
    let now = chrono::Utc::now();
    let (mut session, stage) = spawned(now, 120);
    session.session_type = SessionType::Merge;

    let events = detect(temp.path(), now, session, stage);

    assert!(
        events.is_empty(),
        "a merge session keeps its own handling, got: {events:?}"
    );
}
