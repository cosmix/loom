//! Fail-closed process-identity tests for handoff takedown.

use super::governor_tests::assign_stage_session;
use super::tests::{
    executing_stage, handoff_work_dir, orchestrator_for, recorded_session, write_pid_file,
};
use super::*;
use crate::fs::session_files::{load_session_exact, save_session};
use crate::models::session::{SessionExitReason, SessionStatus};
use crate::orchestrator::terminal::native::{
    session_process_status, write_test_pid_identity, SessionProcessStatus,
};
use crate::verify::transitions::{load_stage, update_stage};
use std::path::Path;
use std::time::Duration;

fn spawn_sigterm_resistant_orphan(ready: &Path) -> u32 {
    let output = std::process::Command::new("sh")
        .args([
            "-c",
            "(trap '' TERM; : > \"$1\"; exec sleep 30) >/dev/null 2>&1 & echo $!",
            "sh",
        ])
        .arg(ready)
        .output()
        .expect("failed to spawn a SIGTERM-resistant stand-in agent");
    let pid = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .expect("stand-in agent printed no pid");
    for _ in 0..100 {
        if ready.exists() {
            return pid;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("stand-in agent never installed its SIGTERM handler");
}

fn force_kill(pid: u32) {
    use nix::sys::signal::{kill, Signal};
    use nix::unistd::Pid;

    let pid = i32::try_from(pid).expect("test pid fits i32");
    let _ = kill(Pid::from_raw(pid), Signal::SIGKILL);
}

#[test]
fn handoff_never_treats_missing_pid_identity_as_confirmed_death() {
    let temp = handoff_work_dir();
    let work = temp.path().join(".loom").join("work");
    executing_stage(&work);
    let mut session = recorded_session(&work);
    session.status = SessionStatus::Completed;
    save_session(&session, &work).unwrap();
    assign_stage_session(&work, &session.id);

    let mut orchestrator = orchestrator_for(&work, temp.path());
    orchestrator.graph.mark_executing("test-stage").unwrap();
    orchestrator
        .active_sessions
        .insert("test-stage".to_string(), session.clone());
    assert!(orchestrator
        .begin_handoff("test-stage", &session.id)
        .unwrap()
        .is_some());
    let survivors = orchestrator
        .take_down_stage_agents("test-stage", &session.id, SessionExitReason::ContextCeiling)
        .unwrap();

    assert_eq!(survivors, vec![session.id.clone()]);
    assert!(orchestrator.active_sessions.contains_key("test-stage"));
    assert_eq!(
        load_stage("test-stage", &work).unwrap().status,
        StageStatus::NeedsHandoff
    );
    assert!(!graph_has_ready_stage(&orchestrator.graph, "test-stage"));
    let persisted = load_session_exact(&work, &session.id).unwrap().unwrap();
    assert_eq!(persisted.status, SessionStatus::Completed);
    assert_eq!(persisted.exit_reason, None);
}

#[test]
fn a_delayed_second_takedown_does_not_relabel_the_first() {
    let temp = handoff_work_dir();
    let work = temp.path().join(".loom").join("work");
    let session = recorded_session(&work);
    write_pid_file(&work, &session, Some(u64::MAX));
    let mut orchestrator = orchestrator_for(&work, temp.path());

    let first = orchestrator
        .take_down_agents(
            "test-stage",
            vec![session.clone()],
            SessionExitReason::ContextCeiling,
        )
        .unwrap();
    let second = orchestrator
        .take_down_agents(
            "test-stage",
            vec![session.clone()],
            SessionExitReason::Stalled,
        )
        .unwrap();

    assert!(first.is_empty());
    assert!(second.is_empty());
    let persisted = load_session_exact(&work, &session.id).unwrap().unwrap();
    assert_eq!(persisted.status, SessionStatus::ContextExhausted);
    assert_eq!(
        persisted.exit_reason,
        Some(SessionExitReason::ContextCeiling)
    );
}

#[test]
fn dispute_retirement_records_replaced() {
    let temp = handoff_work_dir();
    let work = temp.path().join(".loom").join("work");
    executing_stage(&work);
    update_stage("test-stage", &work, |stage| {
        stage.try_request_adjudication(None)
    })
    .unwrap();
    let session = recorded_session(&work);
    write_pid_file(&work, &session, Some(u64::MAX));
    assign_stage_session(&work, &session.id);
    let mut orchestrator = orchestrator_for(&work, temp.path());
    orchestrator
        .active_sessions
        .insert("test-stage".to_string(), session.clone());

    let survivors = orchestrator.retire_disputing_agents("test-stage").unwrap();

    assert!(survivors.is_empty());
    let persisted = load_session_exact(&work, &session.id).unwrap().unwrap();
    assert_eq!(persisted.status, SessionStatus::ContextExhausted);
    assert_eq!(persisted.exit_reason, Some(SessionExitReason::Replaced));
}

#[test]
fn handoff_does_not_requeue_a_verified_process_that_outlives_sigterm() {
    let temp = handoff_work_dir();
    let work = temp.path().join(".loom").join("work");
    executing_stage(&work);
    let session = recorded_session(&work);
    assign_stage_session(&work, &session.id);

    let pid = spawn_sigterm_resistant_orphan(&work.join("slow-exit-ready"));
    write_test_pid_identity(&work, &session, pid).unwrap();
    assert_eq!(
        session_process_status(&work, &session),
        SessionProcessStatus::VerifiedAlive
    );

    let mut orchestrator = orchestrator_for(&work, temp.path());
    orchestrator.graph.mark_executing("test-stage").unwrap();
    orchestrator
        .on_needs_handoff(&session.id, "test-stage")
        .unwrap();

    assert!(crate::process::is_process_alive(pid));
    assert_eq!(
        session_process_status(&work, &session),
        SessionProcessStatus::VerifiedAlive,
        "SIGTERM must not erase the identity needed by confirmation"
    );
    assert_eq!(
        load_stage("test-stage", &work).unwrap().status,
        StageStatus::NeedsHandoff
    );
    assert!(!graph_has_ready_stage(&orchestrator.graph, "test-stage"));

    force_kill(pid);
}
