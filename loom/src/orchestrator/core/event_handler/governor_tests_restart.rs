//! Restart-safety tests for strict session discovery during handoff.

use super::*;

/// Session discovery is part of the proof that no writer survives. A corrupt
/// record may belong to this stage, so takedown must fail closed.
#[test]
fn handoff_does_not_requeue_when_persisted_session_discovery_is_uncertain() {
    let temp = handoff_work_dir();
    let work = temp.path().join(".loom").join("work");
    executing_stage(&work);

    let session = recorded_session(&work);
    let agent_pid = spawn_orphan_process();
    write_test_pid_identity(&work, &session, agent_pid).unwrap();
    assign_stage_session(&work, &session.id);
    std::fs::write(work.join("sessions/corrupt.md"), "not session frontmatter").unwrap();

    let mut orchestrator = orchestrator_for(&work, temp.path());
    orchestrator.graph.mark_executing("test-stage").unwrap();

    let error = orchestrator
        .on_needs_handoff(&session.id, "test-stage")
        .expect_err("uncertain session discovery must block re-queue");

    assert!(format!("{error:#}").contains("parsing persisted session"));
    assert_eq!(
        load_stage("test-stage", &work).unwrap().status,
        StageStatus::NeedsHandoff
    );
    assert!(
        crate::process::is_process_alive(agent_pid),
        "discovery must finish before any candidate session is killed"
    );

    let _ = crate::process::terminate(agent_pid);
}

/// A restarted backstop must recover the same record for its handoff as
/// takedown does for its kill.
#[test]
fn a_restarted_backstop_writes_handoff_then_retires_the_persisted_session() {
    let temp = handoff_work_dir();
    let work = temp.path().join(".loom").join("work");
    executing_stage(&work);

    let session = recorded_session(&work);
    let agent_pid = spawn_orphan_process();
    write_test_pid_identity(&work, &session, agent_pid).unwrap();
    assign_stage_session(&work, &session.id);

    let mut orchestrator = orchestrator_for(&work, temp.path());
    orchestrator.graph.mark_executing("test-stage").unwrap();
    assert!(
        orchestrator.active_sessions.is_empty(),
        "the restarted daemon must recover the matching persisted session"
    );

    orchestrator
        .handle_budget_exceeded(&session.id, "test-stage", 200_000, 150_000)
        .unwrap();

    assert!(
        work.join("handoffs")
            .join("test-stage-handoff-001.md")
            .exists(),
        "the handoff must be written before the backstop takes its agent down"
    );
    assert!(
        !crate::process::is_process_alive(agent_pid),
        "the backstop must still take down the recovered agent"
    );
    assert_eq!(
        session_on_disk(&work, &session.id).status,
        SessionStatus::ContextExhausted
    );
    assert_eq!(
        load_stage("test-stage", &work).unwrap().status,
        StageStatus::Queued
    );
}
