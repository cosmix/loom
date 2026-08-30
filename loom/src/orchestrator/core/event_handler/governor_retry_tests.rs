//! Retry-specific handoff governor tests.

use super::governor_tests::assign_stage_session;
use super::tests::{
    executing_stage, handoff_work_dir, orchestrator_for, recorded_session, spawn_orphan_process,
    write_pid_file,
};
use super::*;
use crate::fs::session_files::{load_session_exact, save_session};
use crate::handoff::{HandoffOrigin, ParsedHandoff};
use crate::models::session::{Session, SessionStatus};
use crate::orchestrator::terminal::native::write_test_pid_identity;
use crate::verify::transitions::{load_stage, update_stage};

fn handoff_count(work: &std::path::Path) -> usize {
    std::fs::read_dir(work.join("handoffs"))
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("test-stage-handoff-")
        })
        .count()
}

fn assert_red_generation(
    orchestrator: &Orchestrator,
    session: &Session,
    stage: &Stage,
    expected: bool,
) {
    let generated = orchestrator
        .monitor
        .handlers()
        .ensure_context_handoff(session, stage, HandoffOrigin::RedBand)
        .unwrap()
        .is_some();
    assert_eq!(generated, expected);
}

#[test]
fn repeated_handoff_mark_is_idempotent_without_recharging_attempt_time() {
    let started_at = Utc::now();
    let mut stage = Stage {
        status: StageStatus::NeedsHandoff,
        execution_secs: Some(42),
        attempt_started_at: Some(started_at),
        ..Stage::default()
    };

    mark_needs_handoff(&mut stage, Utc::now()).unwrap();

    assert_eq!(stage.status, StageStatus::NeedsHandoff);
    assert_eq!(stage.execution_secs, Some(42));
    assert_eq!(stage.attempt_started_at, Some(started_at));
}

/// A surviving agent keeps the stage fail-closed and makes the next poll retry
/// takedown. That retry must reuse the exact outgoing session's validated V2
/// handoff instead of appending a numbered file on every poll.
#[test]
fn budget_backstop_does_not_requeue_or_amplify_handoffs_when_kill_fails() {
    let temp = handoff_work_dir();
    let work = temp.path().join(".work");
    executing_stage(&work);

    let session = recorded_session(&work);
    write_pid_file(&work, &session, None);
    assign_stage_session(&work, &session.id);

    let mut orchestrator = orchestrator_for(&work, temp.path());
    orchestrator.graph.mark_executing("test-stage").unwrap();
    orchestrator
        .active_sessions
        .insert("test-stage".to_string(), session.clone());

    for _ in 0..2 {
        orchestrator
            .handle_budget_exceeded(&session.id, "test-stage", 200_000, 150_000)
            .unwrap();
    }

    assert_eq!(
        load_stage("test-stage", &work).unwrap().status,
        StageStatus::NeedsHandoff
    );
    assert!(!graph_has_ready_stage(&orchestrator.graph, "test-stage"));
    assert!(orchestrator.active_sessions.contains_key("test-stage"));

    assert_eq!(
        handoff_count(&work),
        1,
        "a failed retry must not amplify handoff files"
    );
}

#[test]
fn budget_handoff_and_terminal_record_use_the_fresh_event_context() {
    let temp = handoff_work_dir();
    let work = temp.path().join(".work");
    executing_stage(&work);

    let mut session = recorded_session(&work);
    session.context_tokens = 20_000;
    save_session(&session, &work).unwrap();
    write_pid_file(&work, &session, None);
    assign_stage_session(&work, &session.id);
    let mut orchestrator = orchestrator_for(&work, temp.path());
    orchestrator.graph.mark_executing("test-stage").unwrap();
    orchestrator
        .active_sessions
        .insert("test-stage".to_string(), session.clone());

    orchestrator
        .handle_budget_exceeded(&session.id, "test-stage", 200_000, 150_000)
        .unwrap();

    let handoff =
        std::fs::read_to_string(work.join("handoffs").join("test-stage-handoff-001.md")).unwrap();
    assert_eq!(
        ParsedHandoff::parse(&handoff)
            .as_v2()
            .unwrap()
            .context_tokens,
        200_000
    );
    assert_eq!(
        load_session_exact(&work, &session.id)
            .unwrap()
            .unwrap()
            .context_tokens,
        200_000
    );
}

/// The advisory Red snapshot and the enforced budget snapshot serve different
/// purposes. Both are durable and restart-idempotent, but Red must never cause
/// the first backstop event to reuse an older, less complete snapshot.
#[test]
fn red_and_budget_handoffs_are_distinct_and_restart_idempotent() {
    let temp = handoff_work_dir();
    let work = temp.path().join(".work");
    executing_stage(&work);

    let mut session = recorded_session(&work);
    session.context_tokens = 200_000;
    save_session(&session, &work).unwrap();
    write_pid_file(&work, &session, None);
    assign_stage_session(&work, &session.id);
    let stage = load_stage("test-stage", &work).unwrap();

    let mut first = orchestrator_for(&work, temp.path());
    first.graph.mark_executing("test-stage").unwrap();
    assert_red_generation(&first, &session, &stage, true);
    drop(first);

    let mut restarted = orchestrator_for(&work, temp.path());
    restarted.graph.mark_executing("test-stage").unwrap();
    assert_red_generation(&restarted, &session, &stage, false);
    for _ in 0..2 {
        restarted
            .handle_budget_exceeded(&session.id, "test-stage", 200_000, 150_000)
            .unwrap();
    }
    assert_eq!(handoff_count(&work), 2);
    drop(restarted);

    let mut second_restart = orchestrator_for(&work, temp.path());
    second_restart.graph.mark_executing("test-stage").unwrap();
    second_restart
        .handle_budget_exceeded(&session.id, "test-stage", 200_000, 150_000)
        .unwrap();
    assert_eq!(
        handoff_count(&work),
        2,
        "neither cause may append again after daemon reconstruction"
    );

    let budget =
        std::fs::read_to_string(work.join("handoffs").join("test-stage-handoff-002.md")).unwrap();
    let parsed = ParsedHandoff::parse(&budget);
    let budget = parsed.as_v2().unwrap();
    assert_eq!(budget.origin, Some(HandoffOrigin::BudgetExceeded));
    assert_eq!(budget.context_tokens, 200_000);
}

#[test]
fn budget_snapshot_is_refreshed_when_a_newer_artifact_is_not_reusable() {
    let temp = handoff_work_dir();
    let work = temp.path().join(".work");
    executing_stage(&work);
    let session = recorded_session(&work);
    assign_stage_session(&work, &session.id);
    let stage = load_stage("test-stage", &work).unwrap();
    let orchestrator = orchestrator_for(&work, temp.path());

    assert!(orchestrator
        .monitor
        .handlers()
        .ensure_context_handoff(&session, &stage, HandoffOrigin::BudgetExceeded)
        .unwrap()
        .is_some());
    std::fs::write(
        work.join("handoffs").join("test-stage-handoff-002.md"),
        "malformed newer artifact",
    )
    .unwrap();

    let refreshed = orchestrator
        .monitor
        .handlers()
        .ensure_context_handoff(&session, &stage, HandoffOrigin::BudgetExceeded)
        .unwrap()
        .unwrap();
    assert!(refreshed.ends_with("test-stage-handoff-003.md"));
    assert!(orchestrator
        .monitor
        .handlers()
        .ensure_context_handoff(&session, &stage, HandoffOrigin::BudgetExceeded)
        .unwrap()
        .is_none());
}

/// After a daemon restart the in-memory map may be empty. The stage's durable
/// assignment still says which writer must be retired, so a missing session
/// record is uncertainty and must keep the stage fail-closed in handoff.
#[test]
fn handoff_does_not_requeue_when_the_assigned_session_record_is_missing() {
    let temp = handoff_work_dir();
    let work = temp.path().join(".work");
    executing_stage(&work);
    assign_stage_session(&work, "missing-session");

    let mut orchestrator = orchestrator_for(&work, temp.path());
    orchestrator.graph.mark_executing("test-stage").unwrap();

    let error = orchestrator
        .on_needs_handoff("missing-session", "test-stage")
        .unwrap_err();

    assert!(
        format!("{error:#}").contains("no exact session record exists"),
        "unexpected error: {error:#}"
    );
    assert_eq!(
        load_stage("test-stage", &work).unwrap().status,
        StageStatus::NeedsHandoff
    );
    assert!(!graph_has_ready_stage(&orchestrator.graph, "test-stage"));
}

/// `Completed` is persisted before an agent necessarily exits its
/// merge/teardown path. After restart that terminal record must still be
/// treated as a process candidate, not as proof of death.
#[test]
fn handoff_probes_an_untracked_terminal_assigned_session() {
    let temp = handoff_work_dir();
    let work = temp.path().join(".work");
    executing_stage(&work);

    let mut session = recorded_session(&work);
    session.status = SessionStatus::Completed;
    save_session(&session, &work).unwrap();
    write_pid_file(&work, &session, None);
    assign_stage_session(&work, &session.id);

    let mut orchestrator = orchestrator_for(&work, temp.path());
    orchestrator.graph.mark_executing("test-stage").unwrap();
    orchestrator
        .on_needs_handoff(&session.id, "test-stage")
        .unwrap();

    assert_eq!(
        load_stage("test-stage", &work).unwrap().status,
        StageStatus::NeedsHandoff
    );
    assert_eq!(
        load_session_exact(&work, &session.id)
            .unwrap()
            .unwrap()
            .status,
        SessionStatus::Completed
    );
    assert!(!graph_has_ready_stage(&orchestrator.graph, "test-stage"));
}

/// A concurrent block is stronger than a pending handoff. The destructive
/// phase must recheck workflow state and leave that decision intact.
#[test]
fn handoff_does_not_kill_or_requeue_a_stage_blocked_after_begin() {
    let temp = handoff_work_dir();
    let work = temp.path().join(".work");
    executing_stage(&work);
    let session = recorded_session(&work);
    let session_pid = spawn_orphan_process();
    write_test_pid_identity(&work, &session, session_pid).unwrap();
    assign_stage_session(&work, &session.id);

    let mut orchestrator = orchestrator_for(&work, temp.path());
    orchestrator.graph.mark_executing("test-stage").unwrap();
    orchestrator
        .begin_handoff("test-stage", &session.id)
        .unwrap();
    update_stage("test-stage", &work, |stage| {
        stage.status = StageStatus::Blocked;
        Ok(())
    })
    .unwrap();

    let error = orchestrator
        .finish_handoff_and_requeue("test-stage", &session.id, "state race")
        .unwrap_err();

    assert!(format!("{error:#}").contains("moved out of NeedsHandoff"));
    assert_eq!(
        load_stage("test-stage", &work).unwrap().status,
        StageStatus::Blocked
    );
    assert!(crate::process::is_process_alive(session_pid));
    assert!(!graph_has_ready_stage(&orchestrator.graph, "test-stage"));

    let _ = crate::process::terminate(session_pid);
}

/// Identity must be rechecked immediately before the destructive step, not
/// only when the asynchronous event first arrives. A concurrent retry can
/// replace the assignment after `begin_handoff`; that successor must survive.
#[test]
fn handoff_does_not_kill_an_assignment_that_changed_after_begin() {
    let temp = handoff_work_dir();
    let work = temp.path().join(".work");
    executing_stage(&work);

    let predecessor = recorded_session(&work);
    assign_stage_session(&work, &predecessor.id);
    let mut orchestrator = orchestrator_for(&work, temp.path());
    orchestrator.graph.mark_executing("test-stage").unwrap();
    assert!(orchestrator
        .begin_handoff("test-stage", &predecessor.id)
        .unwrap()
        .is_some());

    let successor = recorded_session(&work);
    let successor_pid = spawn_orphan_process();
    write_test_pid_identity(&work, &successor, successor_pid).unwrap();
    update_stage("test-stage", &work, |stage| {
        stage.session = Some(successor.id.clone());
        Ok(())
    })
    .unwrap();
    orchestrator
        .active_sessions
        .insert("test-stage".to_string(), successor.clone());

    let error = orchestrator
        .finish_handoff_and_requeue("test-stage", &predecessor.id, "race regression")
        .unwrap_err();

    assert!(
        format!("{error:#}").contains("moved from handoff session"),
        "unexpected error: {error:#}"
    );
    assert!(
        crate::process::is_process_alive(successor_pid),
        "a changed assignment must be rejected before any kill"
    );
    let stage = load_stage("test-stage", &work).unwrap();
    assert_eq!(stage.status, StageStatus::NeedsHandoff);
    assert_eq!(stage.session.as_deref(), Some(successor.id.as_str()));
    assert!(!graph_has_ready_stage(&orchestrator.graph, "test-stage"));

    let _ = crate::process::terminate(successor_pid);
}
