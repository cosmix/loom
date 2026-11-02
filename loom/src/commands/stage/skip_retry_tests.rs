//! Tests for what a retry does to a v2 stage's contract respawn budget.

use tempfile::TempDir;

use super::*;
use crate::verify::contracts::store::{attempts_spent, spend_attempt};
use crate::verify::contracts::test_support::{contract_stage, write_test_freeze};
use crate::verify::transitions::create_stage;

#[test]
fn retry_refuses_unknown_assigned_session_without_force() {
    let temp = TempDir::new().unwrap();
    let sessions = temp.path().join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    std::fs::write(sessions.join("session-1.md"), "malformed").unwrap();

    let error = ensure_retry_session_safe(temp.path(), "s1", "session-1", false).unwrap_err();
    assert!(format!("{error:#}").contains("Cannot establish whether"));
    assert!(ensure_retry_session_safe(temp.path(), "s1", "session-1", true).is_ok());
}

#[test]
fn retry_refuses_a_verified_live_session_without_force() {
    let temp = TempDir::new().unwrap();
    let mut session = crate::models::session::Session::new();
    session.assign_to_stage("s1".to_string());
    session.status = crate::models::session::SessionStatus::Running;
    crate::fs::session_files::save_session(&session, temp.path()).unwrap();
    crate::orchestrator::terminal::native::write_test_pid_identity(
        temp.path(),
        &session,
        std::process::id(),
    )
    .unwrap();

    assert!(ensure_retry_session_safe(temp.path(), "s1", &session.id, false).is_err());
    assert!(ensure_retry_session_safe(temp.path(), "s1", &session.id, true).is_ok());
}

/// The stage `spent_contract_stage` leaves on disk, as `retry` read it.
const WRITER_BLOCKED: (&StageStatus, Option<&str>) = (&StageStatus::Blocked, Some("writer-1"));

/// The admission lock covers every launch from its write-ahead through its
/// backend's answer, so a retry judging the stage's session waits it out.
#[test]
fn retry_waits_for_the_launch_holding_the_stage_admission() {
    let temp = TempDir::new().unwrap();
    let mut stage = crate::models::stage::Stage::new("s1".to_string(), None);
    stage.id = "s1".to_string();
    stage.status = StageStatus::Blocked;
    create_stage(&stage, temp.path()).unwrap();
    let admission = crate::fs::stage_admission::lock(temp.path(), "s1").unwrap();

    let work = temp.path().to_path_buf();
    let retrying = std::thread::spawn(move || retry_in(&work, "s1".to_string(), false, None));
    std::thread::sleep(std::time::Duration::from_millis(300));
    assert_eq!(
        load_stage("s1", temp.path()).unwrap().status,
        StageStatus::Blocked,
        "the retry requeued the stage under a launch in flight"
    );

    drop(admission);
    retrying.join().unwrap().unwrap();
    assert_eq!(
        load_stage("s1", temp.path()).unwrap().status,
        StageStatus::Queued
    );
}

/// The status alone does not show a launch that finished after `retry` read
/// the stage: the session it judged is no longer the stage's.
#[test]
fn retry_refuses_a_stage_whose_session_changed_after_the_probe() {
    let temp = TempDir::new().unwrap();
    let planned = spent_contract_stage(temp.path());

    let read_earlier = (&StageStatus::Blocked, Some("a-dead-session"));
    let error = persist_retry_delta("s1", temp.path(), &planned, read_earlier, false).unwrap_err();

    assert!(format!("{error:#}").contains("session"), "{error:#}");
    let stage = load_stage("s1", temp.path()).unwrap();
    assert_eq!(stage.status, StageStatus::Blocked);
    assert_eq!(stage.session.as_deref(), Some("writer-1"));
}

/// A blocked v2 stage whose contract writers spent the whole budget, and the
/// state `retry` plans for it.
fn spent_contract_stage(work_dir: &Path) -> crate::models::stage::Stage {
    let mut stage = contract_stage("s1", "writer-1");
    stage.status = StageStatus::Blocked;
    create_stage(&stage, work_dir).unwrap();
    for _ in 0..3 {
        spend_attempt(work_dir, "s1").unwrap();
    }
    stage.session = None;
    stage
}

#[test]
fn retry_grants_an_unfrozen_stage_a_fresh_contract_budget() {
    let temp = TempDir::new().unwrap();
    let planned = spent_contract_stage(temp.path());

    persist_retry_delta("s1", temp.path(), &planned, WRITER_BLOCKED, false).unwrap();

    assert_eq!(
        load_stage("s1", temp.path()).unwrap().status,
        StageStatus::Queued
    );
    assert_eq!(attempts_spent(temp.path(), "s1").unwrap(), 0);
}

#[test]
fn retry_leaves_a_frozen_stage_budget_alone() {
    let temp = TempDir::new().unwrap();
    let planned = spent_contract_stage(temp.path());
    write_test_freeze(temp.path(), "s1", "writer-1");

    persist_retry_delta("s1", temp.path(), &planned, WRITER_BLOCKED, false).unwrap();

    assert_eq!(
        load_stage("s1", temp.path()).unwrap().status,
        StageStatus::Queued
    );
    assert_eq!(attempts_spent(temp.path(), "s1").unwrap(), 3);
}

/// A refused transition (the on-disk status no longer matches what `retry`
/// planned against) must not reset the budget: the reset sits inside the
/// same locked closure, after the status re-validation, so its error path
/// is never reached.
#[test]
fn retry_refused_transition_leaves_budget_unspent() {
    let temp = TempDir::new().unwrap();
    let planned = spent_contract_stage(temp.path());

    // The stage is on disk as `Blocked`, but `retry` is told to expect
    // `Executing` — the closure's own re-validation refuses the mismatch.
    let expected = (&StageStatus::Executing, Some("writer-1"));
    let result = persist_retry_delta("s1", temp.path(), &planned, expected, false);

    assert!(result.is_err());
    assert_eq!(
        load_stage("s1", temp.path()).unwrap().status,
        StageStatus::Blocked
    );
    assert_eq!(attempts_spent(temp.path(), "s1").unwrap(), 3);
}
