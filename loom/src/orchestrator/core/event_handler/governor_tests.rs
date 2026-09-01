//! Tests for the two governor-path defects: a stale `BudgetExceeded` event
//! naming a session the stage has moved past, and a routine handoff that
//! leaves its taken-down session `Running` instead of `ContextExhausted`.
//!
//! Split out of `tests.rs` to keep that file under its line limit.

use super::tests::{
    executing_stage, handoff_work_dir, orchestrator_for, recorded_session, spawn_orphan_process,
    write_pid_file,
};
use super::*;
use crate::fs::session_files::save_session;
use crate::models::session::{Session, SessionStatus};
use crate::orchestrator::terminal::native::write_test_pid_identity;
use crate::verify::transitions::{load_stage, update_stage};
use std::path::Path;

/// Point the stage at its agent, as the executor does at spawn
/// (`stage_executor.rs` calls `assign_session`). The backstop refuses to act
/// for a session its stage does not name.
pub(super) fn assign_stage_session(work: &Path, session_id: &str) {
    update_stage("test-stage", work, |stage| {
        stage.assign_session(session_id.to_string());
        Ok(())
    })
    .unwrap();
}

/// The session record as it stands on disk - the only copy the next poll of a
/// restarted daemon would read.
fn session_on_disk(work: &Path, session_id: &str) -> Session {
    let path = crate::fs::session_files::find_session_file(work, session_id)
        .unwrap()
        .expect("the session record must still be on disk after the takedown");
    let contents = std::fs::read_to_string(&path).unwrap();
    crate::parser::frontmatter::parse_from_markdown(&contents, "Session").unwrap()
}

/// A stale `BudgetExceeded` event, replayed after a daemon restart, must not
/// kill the stage's CURRENT agent.
///
/// `Detection`'s fire-once latch lives only in the daemon's memory. A record
/// left on disk at or past the ceiling re-fires `BudgetExceeded` on the first
/// poll after every restart, naming the session that was active when the
/// ceiling was first crossed - which by then may be a corpse from a previous
/// attempt. Before the guard in `handle_budget_exceeded`, this event took
/// down whatever session `active_sessions` held for the stage, killing the
/// healthy successor instead of the corpse it actually named.
#[test]
fn the_backstop_ignores_an_event_from_a_session_the_stage_has_moved_on_from() {
    let temp = handoff_work_dir();
    let work = temp.path().join(".loom").join("work");
    executing_stage(&work);

    let live_session = recorded_session(&work);
    let agent_pid = spawn_orphan_process();
    write_test_pid_identity(&work, &live_session, agent_pid).unwrap();
    assert!(crate::process::is_process_alive(agent_pid));
    assign_stage_session(&work, &live_session.id);

    let mut orchestrator = orchestrator_for(&work, temp.path());
    orchestrator.graph.mark_executing("test-stage").unwrap();
    orchestrator
        .active_sessions
        .insert("test-stage".to_string(), live_session.clone());

    orchestrator
        .handle_budget_exceeded(
            "session-from-a-previous-attempt",
            "test-stage",
            200_000,
            150_000,
        )
        .unwrap();

    assert!(
        crate::process::is_process_alive(agent_pid),
        "an event from a corpse's session must not kill the stage's current agent"
    );
    assert_eq!(
        load_stage("test-stage", &work).unwrap().status,
        StageStatus::Executing,
        "the stage must not be handed off for an event that names a stale session"
    );
    assert!(
        orchestrator.active_sessions.contains_key("test-stage"),
        "the live session must stay tracked"
    );

    let _ = crate::process::terminate(agent_pid);
}

/// A delayed hook handoff from the previous attempt must obey the same
/// session-identity gate as a delayed daemon backstop event. Otherwise it can
/// kill the healthy successor currently assigned to the stage.
#[test]
fn handoff_ignores_an_event_from_a_session_the_stage_has_moved_on_from() {
    let temp = handoff_work_dir();
    let work = temp.path().join(".loom").join("work");
    executing_stage(&work);

    let live_session = recorded_session(&work);
    let agent_pid = spawn_orphan_process();
    write_test_pid_identity(&work, &live_session, agent_pid).unwrap();
    assign_stage_session(&work, &live_session.id);

    let mut orchestrator = orchestrator_for(&work, temp.path());
    orchestrator.graph.mark_executing("test-stage").unwrap();
    orchestrator
        .active_sessions
        .insert("test-stage".to_string(), live_session.clone());

    orchestrator
        .on_needs_handoff("session-from-a-previous-attempt", "test-stage")
        .unwrap();

    assert!(
        crate::process::is_process_alive(agent_pid),
        "a stale handoff event must not kill the stage's current agent"
    );
    assert_eq!(
        load_stage("test-stage", &work).unwrap().status,
        StageStatus::Executing
    );
    assert!(orchestrator.active_sessions.contains_key("test-stage"));

    let _ = crate::process::terminate(agent_pid);
}

/// A stage that reached the ceiling normally - not through the daemon
/// backstop - must have its outgoing session's record marked terminal, or
/// the next poll reads the dead-but-`Running` record as a crash.
///
/// `exited_after_stage_finished` forgives a vanished process only when the
/// stage is `Completed`/`MergeConflict`/`MergeBlocked`; a routine handoff
/// leaves the stage `Queued` instead, so an unmarked record is read as a
/// crash, which charges the stage's retry budget and can block it outright.
#[test]
fn a_handoff_marks_the_session_it_took_down_context_exhausted() {
    let temp = handoff_work_dir();
    let work = temp.path().join(".loom").join("work");
    executing_stage(&work);

    let session = recorded_session(&work);
    // No start time this process could ever have: the probe reports it dead
    // and the takedown confirms it gone without ever signalling anything.
    write_pid_file(&work, &session, Some(u64::MAX));
    assign_stage_session(&work, &session.id);

    let mut orchestrator = orchestrator_for(&work, temp.path());
    orchestrator.graph.mark_executing("test-stage").unwrap();
    // The daemon-tracked case: `live_sessions_for_stage` only returns records
    // that are BOTH Running/Spawning AND probe-alive, so the map entry is
    // what puts a dead-but-Running record in front of the takedown at all.
    orchestrator
        .active_sessions
        .insert("test-stage".to_string(), session.clone());

    orchestrator
        .on_needs_handoff(&session.id, "test-stage")
        .unwrap();

    assert_eq!(
        session_on_disk(&work, &session.id).status,
        SessionStatus::ContextExhausted,
        "a record left Running with a dead PID is read as a crash by the next poll"
    );
    assert_eq!(
        load_stage("test-stage", &work).unwrap().status,
        StageStatus::Queued
    );
}

/// Takedown starts from an in-memory liveness snapshot, but status persistence
/// must re-read the record under lock. Otherwise a heartbeat arriving between
/// those two moments is silently rolled back by saving the stale clone.
#[test]
fn handoff_status_update_preserves_newer_persisted_heartbeat_fields() {
    let temp = handoff_work_dir();
    let work = temp.path().join(".loom").join("work");
    executing_stage(&work);

    let stale = recorded_session(&work);
    write_pid_file(&work, &stale, Some(u64::MAX));
    assign_stage_session(&work, &stale.id);

    let mut orchestrator = orchestrator_for(&work, temp.path());
    orchestrator.graph.mark_executing("test-stage").unwrap();
    orchestrator
        .active_sessions
        .insert("test-stage".to_string(), stale.clone());

    let mut current = stale.clone();
    current.context_tokens = 123_456;
    current.transcript_path = Some("/tmp/fresh-heartbeat.jsonl".to_string());
    save_session(&current, &work).unwrap();

    orchestrator
        .on_needs_handoff(&stale.id, "test-stage")
        .unwrap();

    let current = session_on_disk(&work, &stale.id);
    assert_eq!(current.status, SessionStatus::ContextExhausted);
    assert_eq!(current.context_tokens, 123_456);
    assert_eq!(
        current.transcript_path.as_deref(),
        Some("/tmp/fresh-heartbeat.jsonl")
    );
}

/// A completion persisted after takedown took its snapshot is stronger than
/// the governor's generic terminal declaration and must never be downgraded.
#[test]
fn handoff_status_update_preserves_a_concurrently_completed_record() {
    let temp = handoff_work_dir();
    let work = temp.path().join(".loom").join("work");
    executing_stage(&work);

    let stale = recorded_session(&work);
    write_pid_file(&work, &stale, Some(u64::MAX));
    assign_stage_session(&work, &stale.id);

    let mut orchestrator = orchestrator_for(&work, temp.path());
    orchestrator.graph.mark_executing("test-stage").unwrap();
    orchestrator
        .active_sessions
        .insert("test-stage".to_string(), stale.clone());

    let mut completed = stale.clone();
    completed.status = SessionStatus::Completed;
    completed.context_tokens = 123_456;
    save_session(&completed, &work).unwrap();

    orchestrator
        .on_needs_handoff(&stale.id, "test-stage")
        .unwrap();

    let current = session_on_disk(&work, &stale.id);
    assert_eq!(current.status, SessionStatus::Completed);
    assert_eq!(current.context_tokens, 123_456);
    assert_eq!(
        load_stage("test-stage", &work).unwrap().status,
        StageStatus::Queued
    );
}

/// A daemon restart drops `active_sessions`, but leaves the session record on
/// disk. If that agent exited before the restart, the liveness-filtered
/// discovery scan is empty; takedown must nevertheless find the persisted
/// `Running` record and declare it spent before it re-queues the stage.
#[test]
fn a_restarted_daemon_marks_a_dead_persisted_session_context_exhausted() {
    let temp = handoff_work_dir();
    let work = temp.path().join(".loom").join("work");
    executing_stage(&work);

    let session = recorded_session(&work);
    write_pid_file(&work, &session, Some(u64::MAX));
    assign_stage_session(&work, &session.id);

    let mut orchestrator = orchestrator_for(&work, temp.path());
    orchestrator.graph.mark_executing("test-stage").unwrap();
    assert!(
        orchestrator.active_sessions.is_empty(),
        "the restarted daemon has no in-memory handle for the persisted session"
    );

    orchestrator
        .on_needs_handoff(&session.id, "test-stage")
        .unwrap();

    assert_eq!(
        session_on_disk(&work, &session.id).status,
        SessionStatus::ContextExhausted,
        "a dead persisted Running record must not be left for crash detection"
    );
    assert_eq!(
        load_stage("test-stage", &work).unwrap().status,
        StageStatus::Queued
    );
}

/// The daemon's spawn-time clone does not receive heartbeat mutations, and the
/// BudgetExceeded event can be newer than both that clone and the disk record.
/// Its reading must be persisted before formatting the handoff.
#[test]
fn backstop_handoff_prefers_fresh_event_context_over_stale_records() {
    let temp = handoff_work_dir();
    let work = temp.path().join(".loom").join("work");
    executing_stage(&work);

    let stale = recorded_session(&work);
    write_pid_file(&work, &stale, Some(u64::MAX));
    assign_stage_session(&work, &stale.id);

    let mut orchestrator = orchestrator_for(&work, temp.path());
    orchestrator.graph.mark_executing("test-stage").unwrap();
    orchestrator
        .active_sessions
        .insert("test-stage".to_string(), stale.clone());

    let mut current = stale.clone();
    current.context_tokens = 123_456;
    current.transcript_path = Some("/tmp/fresh-heartbeat.jsonl".to_string());
    save_session(&current, &work).unwrap();

    orchestrator
        .handle_budget_exceeded(&stale.id, "test-stage", 200_000, 150_000)
        .unwrap();

    let handoff =
        std::fs::read_to_string(work.join("handoffs").join("test-stage-handoff-001.md")).unwrap();
    assert!(
        handoff.contains("**Context**: 200,000 tokens resident at handoff"),
        "handoff did not use the fresh event context:\n{handoff}"
    );
    let current = session_on_disk(&work, &stale.id);
    assert_eq!(current.context_tokens, 200_000);
    assert_eq!(current.status, SessionStatus::ContextExhausted);
}

/// The same rule for an agent killed before it ever reported `Running`.
///
/// `Spawning -> ContextExhausted` is not a legal transition, so a takedown
/// that asked the state machine politely would refuse this record and leave
/// behind exactly the dead non-terminal session the marking exists to remove -
/// on the narrowest path, where a stage is handed off moments after spawning.
#[test]
fn a_handoff_marks_a_session_it_killed_before_it_ever_ran() {
    let temp = handoff_work_dir();
    let work = temp.path().join(".loom").join("work");
    executing_stage(&work);

    let mut session = recorded_session(&work);
    session.status = SessionStatus::Spawning;
    save_session(&session, &work).unwrap();
    write_pid_file(&work, &session, Some(u64::MAX));
    assign_stage_session(&work, &session.id);

    let mut orchestrator = orchestrator_for(&work, temp.path());
    orchestrator.graph.mark_executing("test-stage").unwrap();
    orchestrator
        .active_sessions
        .insert("test-stage".to_string(), session.clone());

    orchestrator
        .on_needs_handoff(&session.id, "test-stage")
        .unwrap();

    assert_eq!(
        session_on_disk(&work, &session.id).status,
        SessionStatus::ContextExhausted,
        "a session killed while still Spawning must not be left non-terminal"
    );
    assert_eq!(
        load_stage("test-stage", &work).unwrap().status,
        StageStatus::Queued
    );
}

#[path = "governor_tests_restart.rs"]
mod restart_tests;
