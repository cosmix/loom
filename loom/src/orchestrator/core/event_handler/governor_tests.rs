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
    let work = temp.path().join(".work");
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
    let work = temp.path().join(".work");
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

/// The same rule for an agent killed before it ever reported `Running`.
///
/// `Spawning -> ContextExhausted` is not a legal transition, so a takedown
/// that asked the state machine politely would refuse this record and leave
/// behind exactly the dead non-terminal session the marking exists to remove -
/// on the narrowest path, where a stage is handed off moments after spawning.
#[test]
fn a_handoff_marks_a_session_it_killed_before_it_ever_ran() {
    let temp = handoff_work_dir();
    let work = temp.path().join(".work");
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
