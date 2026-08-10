//! Regression tests for session-status detection.

use crate::models::session::{Session, SessionStatus};
use crate::models::stage::{Stage, StageStatus};
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
