//! A crash of a `WaitingForInput` stage's session, and handle eviction for a
//! crash that no longer applies.

use super::orchestrator_for;
use crate::models::session::{Session, SessionStatus};
use crate::models::stage::{Stage, StageStatus};
use crate::verify::transitions::{load_stage, save_stage};
use serial_test::serial;
use std::path::Path;

fn stage_under(work_dir: &Path, status: StageStatus, session: &str) -> Stage {
    let mut stage = Stage::new("weather-cache".to_string(), None);
    stage.id = "weather-cache".to_string();
    stage.status = status;
    stage.session = Some(session.to_string());
    save_stage(&stage, work_dir).unwrap();
    stage
}

fn handle(id: &str, stage_id: &str) -> Session {
    let mut session = Session::new();
    session.id = id.to_string();
    session.stage_id = Some(stage_id.to_string());
    session.status = SessionStatus::Running;
    session.created_at = chrono::Utc::now() - chrono::Duration::minutes(10);
    session
}

#[test]
#[serial]
fn a_dead_waiter_is_blocked_and_its_handle_dropped() {
    let temp = tempfile::tempdir().unwrap();
    let work_dir = temp.path().join(".loom").join("work");
    let stage = stage_under(&work_dir, StageStatus::WaitingForInput, "session-dead");
    let mut orchestrator = orchestrator_for(&work_dir, temp.path());
    let dead = handle("session-dead", &stage.id);
    orchestrator.active_sessions.insert(stage.id.clone(), dead);

    orchestrator
        .handle_session_crashed("session-dead", Some(stage.id.clone()), None)
        .unwrap();

    let after = load_stage(&stage.id, &work_dir).unwrap();
    assert_eq!(after.status, StageStatus::Blocked);
    assert_eq!(after.retry_count, 1);
    assert!(!orchestrator.active_sessions.contains_key(&stage.id));
}

#[test]
#[serial]
fn a_stale_crash_leaves_a_live_waiter_alone() {
    let temp = tempfile::tempdir().unwrap();
    let work_dir = temp.path().join(".loom").join("work");
    let stage = stage_under(&work_dir, StageStatus::WaitingForInput, "session-live");
    let mut orchestrator = orchestrator_for(&work_dir, temp.path());
    let live = handle("session-live", &stage.id);
    orchestrator.active_sessions.insert(stage.id.clone(), live);

    orchestrator
        .handle_session_crashed("session-old", Some(stage.id.clone()), None)
        .unwrap();

    let after = load_stage(&stage.id, &work_dir).unwrap();
    assert_eq!(after.status, StageStatus::WaitingForInput);
    assert_eq!(after.session.as_deref(), Some("session-live"));
    assert_eq!(
        orchestrator
            .active_sessions
            .get(&stage.id)
            .map(|session| session.id.as_str()),
        Some("session-live")
    );
}

#[test]
#[serial]
fn a_stale_crash_drops_only_its_own_handle() {
    let temp = tempfile::tempdir().unwrap();
    let work_dir = temp.path().join(".loom").join("work");
    let stage = stage_under(&work_dir, StageStatus::Completed, "session-x");
    let mut orchestrator = orchestrator_for(&work_dir, temp.path());
    let finished = handle("session-x", &stage.id);
    orchestrator
        .active_sessions
        .insert(stage.id.clone(), finished);

    orchestrator
        .handle_session_crashed("session-x", Some(stage.id.clone()), None)
        .unwrap();

    let after = load_stage(&stage.id, &work_dir).unwrap();
    assert_eq!(after.status, StageStatus::Completed);
    assert!(!orchestrator.active_sessions.contains_key(&stage.id));
}
