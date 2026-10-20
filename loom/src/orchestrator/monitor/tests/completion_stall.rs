//! A session running its pinned `loom stage complete` is silent for the whole
//! acceptance run; the monitor must hold its escalating report to the
//! completion's own line.

use crate::models::session::{Session, SessionStatus};
use crate::models::stage::{AcceptanceCriterion, Stage};
use crate::orchestrator::liveness::LivenessService;
use crate::orchestrator::monitor::detection::Detection;
use crate::orchestrator::monitor::handlers::Handlers;
use crate::orchestrator::monitor::heartbeat::{write_heartbeat, Heartbeat, HeartbeatWatcher};
use crate::orchestrator::monitor::stall_line::COMPLETION_ACTIVITY;
use crate::orchestrator::monitor::{MonitorConfig, MonitorEvent};

/// Two criteria at 300 s each plus the 1800 s margin.
const COMPLETION_LINE_SECS: i64 = 2_400;

/// Whether one poll of a session silent for `silence_secs` reports it at the
/// escalation line, the report the stall recovery acts on.
fn escalated(silence_secs: i64, activity: &str) -> bool {
    let temp = tempfile::tempdir().unwrap();
    let now = chrono::Utc::now();
    let config = MonitorConfig {
        work_dir: temp.path().to_path_buf(),
        ..Default::default()
    };
    let handlers = Handlers::new(config.clone(), Some(LivenessService::fixed_for_tests(true)));
    let mut heartbeat = Heartbeat::new("build".to_string(), "session-1".to_string())
        .with_last_tool("Bash".to_string())
        .with_activity(activity.to_string());
    heartbeat.timestamp = now - chrono::Duration::seconds(silence_secs);
    heartbeat.progress_at = Some(heartbeat.timestamp);
    write_heartbeat(temp.path(), &heartbeat).unwrap();
    let mut session = Session::new();
    session.id = "session-1".to_string();
    session.status = SessionStatus::Running;
    session.stage_id = Some("build".to_string());
    let stage = Stage {
        id: "build".to_string(),
        acceptance: vec![AcceptanceCriterion::Simple("cargo test".to_string()); 2],
        ..Default::default()
    };
    let mut watcher = HeartbeatWatcher::with_now(now);
    let mut detection = Detection::new();
    let events =
        detection.detect_heartbeat_events(&[session], &[stage], &mut watcher, &config, &handlers);
    let reported = events
        .iter()
        .any(|event| matches!(event, MonitorEvent::SessionHung { .. }));
    assert!(reported, "a silence past the budget is always reported");
    detection.hung_latches().1.contains("session-1")
}

#[test]
fn a_running_completion_is_not_escalated_inside_its_line() {
    assert!(!escalated(COMPLETION_LINE_SECS - 1, COMPLETION_ACTIVITY));
}

#[test]
fn a_running_completion_is_escalated_at_its_line() {
    assert!(escalated(COMPLETION_LINE_SECS, COMPLETION_ACTIVITY));
}

#[test]
fn any_other_activity_escalates_at_three_budgets() {
    assert!(escalated(COMPLETION_LINE_SECS - 1, "Tool executed: Bash"));
}
