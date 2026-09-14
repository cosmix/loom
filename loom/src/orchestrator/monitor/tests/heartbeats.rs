//! Hung detection and heartbeat-file ownership.

use crate::models::session::{Session, SessionStatus};
use crate::models::stage::Stage;
use crate::orchestrator::liveness::LivenessService;
use crate::orchestrator::monitor::detection::Detection;
use crate::orchestrator::monitor::handlers::Handlers;
use crate::orchestrator::monitor::heartbeat::{
    judge_heartbeat_path, write_heartbeat, ActivityKind, Heartbeat, HeartbeatWatcher,
};
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

fn observation(now: chrono::DateTime<chrono::Utc>, timestamp_age: i64, tokens: u32) -> Heartbeat {
    let mut heartbeat = Heartbeat::new("stage-1".to_string(), "session-1".to_string());
    heartbeat.timestamp = now - chrono::Duration::seconds(timestamp_age);
    heartbeat.progress_at = Some(now - chrono::Duration::seconds(120));
    heartbeat.activity_kind = Some(ActivityKind::Observation);
    heartbeat.context_tokens = Some(tokens);
    heartbeat.last_tool = Some("Read".to_string());
    heartbeat.activity = Some("Observing".to_string());
    heartbeat
}

fn heartbeat_event(now: chrono::DateTime<chrono::Utc>, tokens: u32) -> MonitorEvent {
    MonitorEvent::HeartbeatReceived {
        stage_id: "stage-1".to_string(),
        session_id: "session-1".to_string(),
        progress_at: now - chrono::Duration::seconds(120),
        context_tokens: Some(tokens),
        transcript_path: None,
        last_tool: Some("Read".to_string()),
    }
}

fn write_judge_observation(work_dir: &std::path::Path, now: chrono::DateTime<chrono::Utc>) {
    let mut heartbeat = Heartbeat::new("stage-1".to_string(), "judge-1".to_string());
    heartbeat.timestamp = now - chrono::Duration::seconds(1);
    heartbeat.progress_at = Some(now - chrono::Duration::seconds(120));
    heartbeat.activity_kind = Some(ActivityKind::Observation);
    let path = judge_heartbeat_path(work_dir, "stage-1");
    std::fs::write(path, serde_json::to_string_pretty(&heartbeat).unwrap()).unwrap();
}

fn assert_latch(detection: &Detection, expected: (bool, bool)) {
    let (reported, escalated) = detection.hung_latches();
    assert_eq!(
        (
            reported.contains("session-1"),
            escalated.contains("session-1"),
        ),
        expected
    );
}

#[test]
fn observation_updates_do_not_clear_an_existing_hung_report() {
    let temp = tempfile::TempDir::new().unwrap();
    let now = chrono::Utc::now();
    let (config, handlers, mut watcher) = fixed_harness(temp.path(), now);
    let (session, stage) = running_pair("stage-1", "session-1");
    let mut detection = Detection::new();
    write_heartbeat(temp.path(), &observation(now, 2, 10_000)).unwrap();

    let first = detection.detect_heartbeat_events(
        std::slice::from_ref(&session),
        std::slice::from_ref(&stage),
        &mut watcher,
        &config,
        &handlers,
    );
    assert_eq!(
        first,
        vec![
            heartbeat_event(now, 10_000),
            MonitorEvent::SessionHung {
                session_id: "session-1".to_string(),
                stage_id: Some("stage-1".to_string()),
                stale_duration_secs: 120,
                timeout_secs: 60,
                last_activity: Some("Observing".to_string()),
                finished_without_completing: false,
            }
        ]
    );

    write_heartbeat(temp.path(), &observation(now, 1, 20_000)).unwrap();
    let second =
        detection.detect_heartbeat_events(&[session], &[stage], &mut watcher, &config, &handlers);
    assert_eq!(second, vec![heartbeat_event(now, 20_000)]);
    assert_latch(&detection, (true, false));
}

#[test]
fn real_progress_clears_the_latch_once_and_an_identical_poll_is_quiet() {
    let temp = tempfile::TempDir::new().unwrap();
    let now = chrono::Utc::now();
    let (config, handlers, mut watcher) = fixed_harness(temp.path(), now);
    let (session, stage) = running_pair("stage-1", "session-1");
    let mut detection = Detection::new();
    write_heartbeat(temp.path(), &observation(now, 1, 20_000)).unwrap();
    let _ = detection.detect_heartbeat_events(
        std::slice::from_ref(&session),
        std::slice::from_ref(&stage),
        &mut watcher,
        &config,
        &handlers,
    );
    assert_latch(&detection, (true, false));

    let mut progressed = observation(now, 0, 30_000);
    progressed.progress_at = Some(now);
    progressed.activity_kind = Some(ActivityKind::Progress);
    write_heartbeat(temp.path(), &progressed).unwrap();
    let events = detection.detect_heartbeat_events(
        std::slice::from_ref(&session),
        std::slice::from_ref(&stage),
        &mut watcher,
        &config,
        &handlers,
    );
    assert_eq!(
        events,
        vec![MonitorEvent::HeartbeatReceived {
            stage_id: "stage-1".to_string(),
            session_id: "session-1".to_string(),
            progress_at: now,
            context_tokens: Some(30_000),
            transcript_path: None,
            last_tool: Some("Read".to_string()),
        }]
    );
    assert_latch(&detection, (false, false));

    let repeat =
        detection.detect_heartbeat_events(&[session], &[stage], &mut watcher, &config, &handlers);
    assert_eq!(repeat, Vec::<MonitorEvent>::new());
    assert_latch(&detection, (false, false));
}

#[test]
fn judge_idle_time_comes_from_judge_progress_not_the_stage_heartbeat() {
    let temp = tempfile::TempDir::new().unwrap();
    let now = chrono::Utc::now();
    let (config, handlers, mut watcher) = fixed_harness(temp.path(), now);
    let mut stage_heartbeat = Heartbeat::new("stage-1".to_string(), "stage-agent".to_string());
    stage_heartbeat.timestamp = now;
    stage_heartbeat.progress_at = Some(now);
    write_heartbeat(temp.path(), &stage_heartbeat).unwrap();

    write_judge_observation(temp.path(), now);
    let initial = watcher.poll(temp.path()).unwrap();
    assert_eq!(initial.len(), 1);
    assert_eq!(initial[0].heartbeat.session_id, "stage-agent");

    let mut judge = Session::new_adjudication("stage-1");
    judge.id = "judge-1".to_string();
    judge.status = SessionStatus::Running;
    judge.created_at = now - chrono::Duration::seconds(1);
    let stage = Stage {
        id: "stage-1".to_string(),
        subagent_timeout_secs: Some(60),
        ..Stage::default()
    };
    let events = Detection::new().detect_heartbeat_events(
        &[judge],
        &[stage],
        &mut watcher,
        &config,
        &handlers,
    );
    assert_eq!(
        events,
        vec![MonitorEvent::AdjudicatorStalled {
            session_id: "judge-1".to_string(),
            stage_id: "stage-1".to_string(),
            stale_duration_secs: 120,
            timeout_secs: 60,
        }]
    );
}

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
