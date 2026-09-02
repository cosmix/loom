//! Tests for retiring a disputing stage's agents before a verdict applies.
//!
//! The kill is observed through a real process, the same way the budget
//! backstop and stall-recovery tests do it: every proxy for "the agent was
//! taken down" lies in at least one lane.

use super::governor_tests::assign_stage_session;
use super::tests::{
    executing_stage, handoff_work_dir, orchestrator_for, recorded_session, spawn_orphan_process,
};
use super::*;
use crate::fs::session_files::save_session;
use crate::models::session::{Session, SessionStatus};
use crate::orchestrator::terminal::native::write_test_pid_identity;
use crate::verify::transitions::{load_stage, update_stage};

fn disputing_stage(work: &std::path::Path) -> (Session, u32) {
    executing_stage(work);
    update_stage("test-stage", work, |stage| {
        stage.try_request_adjudication(None)
    })
    .unwrap();

    let session = recorded_session(work);
    let agent_pid = spawn_orphan_process();
    write_test_pid_identity(work, &session, agent_pid).unwrap();
    assert!(crate::process::is_process_alive(agent_pid));
    assign_stage_session(work, &session.id);
    (session, agent_pid)
}

/// The agent that filed the dispute is idle and must be taken down before the
/// verdict applies, so a successor spawns against the amended criteria
/// instead of the daemon adopting the same idle process.
#[test]
fn retiring_kills_the_disputing_agent_and_clears_the_stage_session() {
    let temp = handoff_work_dir();
    let work = temp.path().join(".loom").join("work");
    let (session, agent_pid) = disputing_stage(&work);

    let mut orchestrator = orchestrator_for(&work, temp.path());
    orchestrator
        .active_sessions
        .insert("test-stage".to_string(), session.clone());

    let survivors = orchestrator.retire_disputing_agents("test-stage").unwrap();

    assert!(survivors.is_empty());
    assert!(
        !crate::process::is_process_alive(agent_pid),
        "the disputing agent must be killed before the verdict applies"
    );
    let stage = load_stage("test-stage", &work).unwrap();
    assert_eq!(stage.session, None);
    assert_eq!(stage.status, StageStatus::NeedsAdjudication);
    assert!(orchestrator.active_sessions.is_empty());
    assert!(
        work.join("handoffs")
            .join("test-stage-handoff-001.md")
            .exists(),
        "the successor needs the retired agent's state, written before the kill"
    );
}

/// The session judging the dispute shares the stage's `stage_id` but must
/// never be touched by the retirement that runs before its own verdict is
/// applied.
#[test]
fn retiring_leaves_the_adjudication_session_alone() {
    let temp = handoff_work_dir();
    let work = temp.path().join(".loom").join("work");
    let (session, agent_pid) = disputing_stage(&work);

    let mut adjudication_session = Session::new_adjudication("test-stage");
    adjudication_session.status = SessionStatus::Running;
    save_session(&adjudication_session, &work).unwrap();
    let adjudicator_pid = spawn_orphan_process();
    write_test_pid_identity(&work, &adjudication_session, adjudicator_pid).unwrap();

    let mut orchestrator = orchestrator_for(&work, temp.path());
    orchestrator
        .active_sessions
        .insert("test-stage".to_string(), session.clone());

    let survivors = orchestrator.retire_disputing_agents("test-stage").unwrap();

    assert!(survivors.is_empty());
    assert!(!crate::process::is_process_alive(agent_pid));
    assert!(
        crate::process::is_process_alive(adjudicator_pid),
        "the live adjudication session judging this stage must not be touched",
    );

    let _ = crate::process::terminate(adjudicator_pid);
}

/// A stage that isn't under adjudication has nothing to retire — this path
/// runs only ahead of applying a verdict.
#[test]
fn a_stage_not_under_adjudication_is_not_retired() {
    let temp = handoff_work_dir();
    let work = temp.path().join(".loom").join("work");
    executing_stage(&work);

    let session = recorded_session(&work);
    let agent_pid = spawn_orphan_process();
    write_test_pid_identity(&work, &session, agent_pid).unwrap();
    assign_stage_session(&work, &session.id);

    let mut orchestrator = orchestrator_for(&work, temp.path());
    orchestrator
        .active_sessions
        .insert("test-stage".to_string(), session.clone());

    let survivors = orchestrator.retire_disputing_agents("test-stage").unwrap();

    assert!(survivors.is_empty());
    assert!(
        crate::process::is_process_alive(agent_pid),
        "a stage not under adjudication must not have its agent retired"
    );

    let _ = crate::process::terminate(agent_pid);
}
