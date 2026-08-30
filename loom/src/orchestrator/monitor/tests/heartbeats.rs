//! Hung detection and heartbeat-file ownership.

use crate::models::session::{Session, SessionStatus};
use crate::models::stage::Stage;
use crate::orchestrator::monitor::detection::Detection;
use crate::orchestrator::monitor::handlers::Handlers;
use crate::orchestrator::monitor::{MonitorConfig, MonitorEvent};

#[test]
fn same_timestamp_with_changed_context_is_a_new_heartbeat() {
    use crate::orchestrator::monitor::heartbeat::{write_heartbeat, Heartbeat, HeartbeatWatcher};

    let temp = tempfile::TempDir::new().unwrap();
    let mut watcher = HeartbeatWatcher::new();
    let mut heartbeat =
        Heartbeat::new("stage-1".to_string(), "session-1".to_string()).with_context_tokens(130_000);
    heartbeat.timestamp = chrono::Utc::now();
    write_heartbeat(temp.path(), &heartbeat).unwrap();
    assert_eq!(watcher.poll(temp.path()).unwrap().len(), 1);

    heartbeat.context_tokens = Some(80_000);
    write_heartbeat(temp.path(), &heartbeat).unwrap();
    let updates = watcher.poll(temp.path()).unwrap();

    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0].heartbeat.context_tokens, Some(80_000));
}

#[test]
fn test_hung_detection_honors_per_stage_subagent_timeout() {
    use tempfile::TempDir;

    use crate::orchestrator::liveness::LivenessService;
    use crate::orchestrator::monitor::heartbeat::{
        write_heartbeat, Heartbeat, HeartbeatWatcher, DEFAULT_HUNG_TIMEOUT_SECS,
    };

    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path().to_path_buf();

    let config = MonitorConfig {
        work_dir: work_dir.clone(),
        ..Default::default()
    };
    // Hung detection only fires for a session whose process is still alive;
    // without a liveness source the probe returns None and the arm is skipped.
    let handlers = Handlers::new(config.clone(), Some(LivenessService::fixed_for_tests(true)));

    // A heartbeat 400s old: past the 300s built-in default, well inside a 900s
    // budget. The same on-disk state must read differently per stage.
    let mut heartbeat = Heartbeat::new("slow-stage".to_string(), "session-1".to_string());
    heartbeat.timestamp = chrono::Utc::now() - chrono::Duration::seconds(400);
    write_heartbeat(&work_dir, &heartbeat).unwrap();

    let mut session = Session::new();
    session.id = "session-1".to_string();
    session.status = SessionStatus::Running;
    session.stage_id = Some("slow-stage".to_string());

    let mut stage = Stage {
        id: "slow-stage".to_string(),
        subagent_timeout_secs: Some(900),
        ..Default::default()
    };

    let mut watcher = HeartbeatWatcher::new();
    let mut detection = Detection::new();

    // A stage that declared a 900s budget is not flagged at 400s of silence.
    let events = detection.detect_heartbeat_events(
        &[session.clone()],
        &[stage.clone()],
        &mut watcher,
        &config,
        &handlers,
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, MonitorEvent::SessionHung { .. })),
        "an explicit 900s budget must suppress the 400s-silence warning, got: {events:?}"
    );

    // The identical heartbeat under the built-in default IS flagged, and the
    // event reports the budget it was measured against.
    stage.subagent_timeout_secs = None;
    let events =
        detection.detect_heartbeat_events(&[session], &[stage], &mut watcher, &config, &handlers);
    let reported = events
        .iter()
        .find_map(|e| match e {
            MonitorEvent::SessionHung { timeout_secs, .. } => Some(*timeout_secs),
            _ => None,
        })
        .expect("a stage on the built-in default must be flagged after 400s of silence");
    assert_eq!(reported, DEFAULT_HUNG_TIMEOUT_SECS);
}

/// Build a stage whose active session is `active_session_id`.
fn stage_owned_by(stage_id: &str, active_session_id: &str) -> Stage {
    Stage {
        id: stage_id.to_string(),
        session: Some(active_session_id.to_string()),
        ..Stage::default()
    }
}

/// Build a session that names `stage_id` without necessarily owning it.
fn session_naming_stage(session_id: &str, stage_id: &str) -> Session {
    let mut session = Session::new();
    session.id = session_id.to_string();
    session.stage_id = Some(stage_id.to_string());
    session
}

#[test]
fn a_dead_sessions_cleanup_cannot_delete_the_live_sessions_heartbeat() {
    // Heartbeat files are keyed by STAGE, so every session a stage has ever
    // had shares one path while only one owns it. A stage that crashed and
    // retried leaves each corpse on disk with `stage_id` still set, so
    // without the ownership guard the terminal handling of an OLD session
    // deletes the CURRENT session's heartbeat — freezing its `last_active` at
    // spawn and making a healthy long-running session look like it died
    // instantly, precisely on the repeat-failing stages worth debugging.
    let work = tempfile::TempDir::new().unwrap();
    let work_dir = work.path();

    let heartbeat = crate::orchestrator::monitor::heartbeat::Heartbeat::new(
        "flaky-stage".to_string(),
        "session-live".to_string(),
    );
    crate::orchestrator::monitor::heartbeat::write_heartbeat(work_dir, &heartbeat).unwrap();
    let path = crate::orchestrator::monitor::heartbeat::heartbeat_path(work_dir, "flaky-stage");
    assert!(
        path.exists(),
        "fixture heartbeat must exist to be deletable"
    );

    // The stage has moved on: `session-live` owns it, `session-dead` does not.
    let stages = vec![stage_owned_by("flaky-stage", "session-live")];
    let dead = session_naming_stage("session-dead", "flaky-stage");

    crate::orchestrator::monitor::session_events::cleanup_heartbeat_for_session(
        work_dir, &dead, &stages,
    );
    assert!(
        path.exists(),
        "a session the stage no longer points at must not delete the live heartbeat"
    );

    // POSITIVE CONTROL. Without this, the assertion above would also pass if
    // cleanup were broken outright and never deleted anything.
    let live = session_naming_stage("session-live", "flaky-stage");
    crate::orchestrator::monitor::session_events::cleanup_heartbeat_for_session(
        work_dir, &live, &stages,
    );
    assert!(
        !path.exists(),
        "the stage's own current session must still clean its heartbeat up"
    );
}
