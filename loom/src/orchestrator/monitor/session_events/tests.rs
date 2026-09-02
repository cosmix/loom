//! Regression tests for session-status detection.

use crate::models::dispute::{verdict_file, Citation, DisputeVerdict, DisputeVerdictRecord};
use crate::models::session::{Session, SessionStatus, SessionType};
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::liveness::LivenessService;
use crate::orchestrator::monitor::detection::Detection;
use crate::orchestrator::monitor::handlers::Handlers;
use crate::orchestrator::monitor::{MonitorConfig, MonitorEvent};

/// Build a `(stage, live_session, stale_session)` triple where the stage is
/// executing under `live_session` and `stale_session` is a corpse from an
/// earlier attempt at the SAME stage — the shape every retried stage leaves on
/// disk.
fn stage_with_stale_and_live_sessions() -> (Stage, Session, Session) {
    let mut stale = Session::new();
    stale.id = "session-old".to_string();
    stale.stage_id = Some("weather-cache".to_string());
    stale.status = SessionStatus::Crashed;

    let mut live = Session::new();
    live.id = "session-new".to_string();
    live.stage_id = Some("weather-cache".to_string());
    live.status = SessionStatus::Running;

    let mut stage = Stage::new("weather-cache".to_string(), None);
    stage.id = "weather-cache".to_string();
    stage.status = StageStatus::Executing;
    stage.session = Some(live.id.clone());

    (stage, live, stale)
}

fn detection_harness() -> (tempfile::TempDir, Handlers) {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let config = MonitorConfig {
        work_dir: temp_dir.path().to_path_buf(),
        ..Default::default()
    };
    let handlers = Handlers::new(config, None);
    (temp_dir, handlers)
}

/// A daemon restart must not replay an already-handled crash.
///
/// `last_session_states` is in-memory, so on startup EVERY session file on disk
/// is a first observation — including corpses persisted as Crashed hours ago.
/// Replaying one charges the stage's retry budget and can auto-retry a stage
/// whose real session is alive, putting two agents in one worktree. Observed in
/// a live run on 2026-08-10.
#[test]
fn restart_does_not_replay_a_stale_crashed_session() {
    let (_temp, handlers) = detection_harness();
    let mut detection = Detection::new();
    let (stage, live, stale) = stage_with_stale_and_live_sessions();

    // First poll after a restart sees both session files at once.
    let events = detection.detect_session_changes(&[stale, live], &[stage], &handlers);

    assert!(
        !events
            .iter()
            .any(|e| matches!(e, MonitorEvent::SessionCrashed { .. })),
        "a corpse that is not the stage's active session must not emit a crash: {events:?}"
    );
}

/// The mirror of the above: a crash of the stage's ACTIVE session must still be
/// reported on a first observation, or a stage stranded by a daemon that died
/// between the crash and handling it would sit Executing forever.
#[test]
fn restart_still_reports_a_crash_of_the_stages_active_session() {
    let (_temp, handlers) = detection_harness();
    let mut detection = Detection::new();
    let (mut stage, mut live, _stale) = stage_with_stale_and_live_sessions();
    live.status = SessionStatus::Crashed;
    stage.session = Some(live.id.clone());

    let events = detection.detect_session_changes(&[live], &[stage], &handlers);

    assert!(
        events.iter().any(|e| matches!(
            e,
            MonitorEvent::SessionCrashed { session_id, .. } if session_id == "session-new"
        )),
        "the stage's own crashed session must still be reported: {events:?}"
    );
}

/// Write a minimal `verdict.md` naming `session_id` as the judge that wrote
/// it, in the on-disk shape `read_verdict_record` (via
/// `parse_yaml_frontmatter`) expects.
fn write_verdict(work_dir: &std::path::Path, stage_id: &str, dispute_id: u32, session_id: &str) {
    let disputes_root = work_dir.join("disputes");
    std::fs::create_dir_all(disputes_root.join(stage_id).join(dispute_id.to_string())).unwrap();
    let record = DisputeVerdictRecord {
        id: dispute_id,
        stage_id: stage_id.to_string(),
        verdict: DisputeVerdict::Reject {
            citations: vec![Citation {
                file: "f".to_string(),
                line: None,
                excerpt: "e".to_string(),
                claim: "c".to_string(),
            }],
            reasoning: "criterion is correct".to_string(),
        },
        adjudicator_attempt_count: 1,
        created_at: chrono::Utc::now(),
        model: "test".to_string(),
        session_id: Some(session_id.to_string()),
    };
    let yaml = serde_yaml::to_string(&record).unwrap();
    let path = verdict_file(&disputes_root, stage_id, dispute_id);
    std::fs::write(
        &path,
        format!("---\n{yaml}---\n\n# Verdict {stage_id}/{dispute_id}\n"),
    )
    .unwrap();
}

/// A judge whose verdict is already on disk has finished its job: its
/// process exiting afterward is an ordinary completion, not a crash, and
/// must write no crash report.
#[test]
fn adjudication_session_with_written_verdict_is_completed_not_crashed() {
    let (temp, mut handlers) = detection_harness();
    handlers.set_liveness(LivenessService::fixed_for_tests(false));
    let mut detection = Detection::new();

    let mut session = Session::new();
    session.id = "judge-1".to_string();
    session.stage_id = Some("weather-cache".to_string());
    session.status = SessionStatus::Running;
    session.session_type = SessionType::Adjudication;
    session.set_pid(99997);

    let mut stage = Stage::new("weather-cache".to_string(), None);
    stage.id = "weather-cache".to_string();
    stage.status = StageStatus::NeedsAdjudication;

    write_verdict(temp.path(), "weather-cache", 1, "judge-1");

    // First poll establishes Running state in detection tracking.
    detection.detect_session_changes(&[session.clone()], &[stage.clone()], &handlers);
    // Second poll: PID dead but a verdict on disk names this session as the
    // judge that wrote it, so this is an ordinary exit.
    let events = detection.detect_session_changes(&[session.clone()], &[stage], &handlers);

    assert!(
        !events
            .iter()
            .any(|e| matches!(e, MonitorEvent::SessionCrashed { .. })),
        "a judge with a written verdict must not be reported as crashed: {events:?}"
    );
    assert_eq!(
        detection.last_session_states.get("judge-1"),
        Some(&SessionStatus::Completed),
        "judge session should be marked Completed, not Crashed"
    );
    assert!(
        !temp.path().join("crashes").exists(),
        "no crash report should be written for a judge that already recorded its verdict"
    );
}

/// The mirror of the above: a vanished adjudication session with NO verdict
/// on disk (e.g. it crashed mid-judgment) still takes the crash path.
#[test]
fn adjudication_session_without_verdict_is_reported_as_crash() {
    let (_temp, mut handlers) = detection_harness();
    handlers.set_liveness(LivenessService::fixed_for_tests(false));
    let mut detection = Detection::new();

    let mut session = Session::new();
    session.id = "judge-2".to_string();
    session.stage_id = Some("weather-cache".to_string());
    session.status = SessionStatus::Running;
    session.session_type = SessionType::Adjudication;
    session.set_pid(99996);

    let mut stage = Stage::new("weather-cache".to_string(), None);
    stage.id = "weather-cache".to_string();
    stage.status = StageStatus::NeedsAdjudication;

    // First poll establishes Running state in detection tracking.
    detection.detect_session_changes(&[session.clone()], &[stage.clone()], &handlers);
    // Second poll: PID dead, no verdict anywhere for this session.
    let events = detection.detect_session_changes(&[session.clone()], &[stage], &handlers);

    assert!(
        events.iter().any(|e| matches!(
            e,
            MonitorEvent::SessionCrashed { session_id, .. } if session_id == "judge-2"
        )),
        "a judge with no written verdict must still be reported as crashed: {events:?}"
    );
    assert_eq!(
        detection.last_session_states.get("judge-2"),
        Some(&SessionStatus::Crashed)
    );
}
