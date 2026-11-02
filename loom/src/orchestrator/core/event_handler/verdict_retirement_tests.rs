//! Tests for retiring a stage's agents: a disputing stage's before a verdict
//! applies, and a blocked stage's once the daemon learns of the block.
//!
//! The kill is observed through a real process, the same way the budget
//! backstop and stall-recovery tests do it: every proxy for "the agent was
//! taken down" lies in at least one lane.

use super::governor_tests::assign_stage_session;
use super::tests::{
    executing_stage, handoff_work_dir, orchestrator_for, recorded_session, spawn_orphan_process,
};
use super::*;
use crate::fs::session_files::{load_session_exact, save_session};
use crate::models::failure::{FailureInfo, FailureType};
use crate::models::session::{Session, SessionBackendKind, SessionExitReason, SessionStatus};
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

#[test]
fn retirement_keeps_a_missing_assigned_writer_for_block_and_dispute() {
    for expected in [StageStatus::Blocked, StageStatus::NeedsAdjudication] {
        let temp = handoff_work_dir();
        let work = temp.path().join(".loom").join("work");
        executing_stage(&work);
        update_stage("test-stage", &work, |stage| {
            match expected {
                StageStatus::Blocked => stage.try_mark_blocked()?,
                StageStatus::NeedsAdjudication => stage.try_request_adjudication(None)?,
                _ => unreachable!(),
            }
            Ok(())
        })
        .unwrap();
        assign_stage_session(&work, "missing-writer");
        let mut orchestrator = orchestrator_for(&work, temp.path());

        let error = orchestrator
            .retire_stage_agents("test-stage", expected.clone())
            .unwrap_err();
        assert!(format!("{error:#}").contains("no exact session record exists"));
        let stage = load_stage("test-stage", &work).unwrap();
        assert_eq!(stage.status, expected);
        assert_eq!(stage.session.as_deref(), Some("missing-writer"));
    }
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
    let retired = load_session_exact(&work, &session.id).unwrap().unwrap();
    assert_eq!(
        (retired.status, retired.exit_reason),
        (
            SessionStatus::ContextExhausted,
            Some(SessionExitReason::Replaced)
        )
    );
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

/// The crash evidence an earlier attempt leaves behind: a crash auto-retry
/// and a `loom stage retry` without `--force` both requeue with it in place.
fn prior_crash() -> FailureInfo {
    FailureInfo {
        failure_type: FailureType::SessionCrash,
        detected_at: chrono::Utc::now(),
        evidence: vec!["Process no longer running".to_string()],
    }
}

/// An Executing `test-stage` whose agent is a real, live stand-in process.
fn stage_with_live_agent(work: &std::path::Path) -> (Session, u32) {
    executing_stage(work);
    let session = recorded_session(work);
    let agent_pid = spawn_orphan_process();
    write_test_pid_identity(work, &session, agent_pid).unwrap();
    assert!(crate::process::is_process_alive(agent_pid));
    assign_stage_session(work, &session.id);
    (session, agent_pid)
}

fn block_directly(work: &std::path::Path, failure_info: Option<FailureInfo>) {
    update_stage("test-stage", work, |stage| {
        stage.try_mark_blocked()?;
        stage.failure_info = failure_info;
        Ok(())
    })
    .unwrap();
}

/// A stage agent's `loom stage block` ends its work: the daemon retires the
/// agent, so its later exit is not filed as a crash and a retry spawns a
/// fresh session instead of adopting the idle one. The stale crash evidence
/// of an earlier attempt must not stand in the way.
#[test]
fn an_agent_block_retires_the_live_session() {
    let temp = handoff_work_dir();
    let work = temp.path().join(".loom").join("work");
    let (session, agent_pid) = stage_with_live_agent(&work);
    update_stage("test-stage", &work, |stage| {
        stage.failure_info = Some(prior_crash());
        Ok(())
    })
    .unwrap();

    let mut orchestrator = orchestrator_for(&work, temp.path());
    orchestrator
        .active_sessions
        .insert("test-stage".to_string(), session.clone());

    let response =
        crate::daemon::handle_block_stage(&work, "test-stage", "spec is ambiguous").unwrap();
    assert!(matches!(response, crate::daemon::Response::Ok));
    orchestrator
        .handle_events(vec![MonitorEvent::StageBlocked {
            stage_id: "test-stage".into(),
            reason: "r".into(),
        }])
        .unwrap();

    assert!(
        !crate::process::is_process_alive(agent_pid),
        "the agent that blocked its stage must be retired"
    );
    let stage = load_stage("test-stage", &work).unwrap();
    assert_eq!(stage.session, None);
    assert_eq!(stage.status, StageStatus::Blocked);
    assert_eq!(stage.close_reason.as_deref(), Some("spec is ambiguous"));
    assert!(stage.failure_info.is_none());
    let retired = load_session_exact(&work, &session.id).unwrap().unwrap();
    assert!(
        retired.status.is_terminal(),
        "the retired session must be terminal, not left Running: {:?}",
        retired.status
    );
    assert!(orchestrator.active_sessions.is_empty());
    assert!(
        work.join("handoffs")
            .join("test-stage-handoff-001.md")
            .exists(),
        "a retry's fresh session needs the retired agent's state"
    );
}

/// A stage the crash path blocked carries `failure_info`; its session is
/// already gone or owned by the crash path, so the block handler leaves it.
#[test]
fn a_crash_blocked_stage_is_left_to_the_crash_path() {
    let temp = handoff_work_dir();
    let work = temp.path().join(".loom").join("work");
    let (session, agent_pid) = stage_with_live_agent(&work);
    block_directly(&work, Some(prior_crash()));

    let mut orchestrator = orchestrator_for(&work, temp.path());
    orchestrator.on_stage_blocked("test-stage").unwrap();

    assert!(
        crate::process::is_process_alive(agent_pid),
        "a crash-blocked stage's session is the crash path's, not the block handler's"
    );
    let stage = load_stage("test-stage", &work).unwrap();
    assert_eq!(stage.session.as_deref(), Some(session.id.as_str()));

    let _ = crate::process::terminate(agent_pid);
}

/// A stage retried between the block and the daemon learning of it has moved
/// on; its session belongs to the new attempt and must not be touched.
#[test]
fn a_stage_that_left_blocked_is_not_retired() {
    let temp = handoff_work_dir();
    let work = temp.path().join(".loom").join("work");
    let (_session, agent_pid) = stage_with_live_agent(&work);
    block_directly(&work, None);
    update_stage("test-stage", &work, |stage| stage.try_mark_queued()).unwrap();

    let mut orchestrator = orchestrator_for(&work, temp.path());
    orchestrator.on_stage_blocked("test-stage").unwrap();

    assert!(
        crate::process::is_process_alive(agent_pid),
        "a stage no longer Blocked must not have its agent retired"
    );
    let stage = load_stage("test-stage", &work).unwrap();
    assert_eq!(stage.status, StageStatus::Queued);

    let _ = crate::process::terminate(agent_pid);
}

/// `MergeConflict -> Blocked` is legal, so an operator can block a stage
/// whose merge resolver is live. Retiring the resolver would record it
/// `ContextExhausted`, and `MergeSessionCompleted` would never fire.
#[test]
fn a_merge_resolver_is_not_retired_by_a_block() {
    let temp = handoff_work_dir();
    let work = temp.path().join(".loom").join("work");
    executing_stage(&work);
    block_directly(&work, None);

    let mut resolver = Session::new_merge("loom/test-stage".into(), "main".into());
    resolver.assign_to_stage("test-stage".to_string());
    resolver.status = SessionStatus::Running;
    resolver.backend = SessionBackendKind::Tmux;
    save_session(&resolver, &work).unwrap();
    let resolver_pid = spawn_orphan_process();
    write_test_pid_identity(&work, &resolver, resolver_pid).unwrap();

    let mut orchestrator = orchestrator_for(&work, temp.path());
    orchestrator
        .active_sessions
        .insert("test-stage".to_string(), resolver.clone());

    orchestrator.on_stage_blocked("test-stage").unwrap();

    assert!(
        crate::process::is_process_alive(resolver_pid),
        "a block must not retire the merge resolver working the stage's branch"
    );
    assert!(orchestrator.active_sessions.contains_key("test-stage"));

    let _ = crate::process::terminate(resolver_pid);
}

/// A daemon restart re-emits `StageBlocked` for every Blocked stage. One with no
/// `failure_info`, no session and no live agent has nothing to retire, so its
/// stage file must not be rewritten (a rewrite bumps `updated_at`).
#[test]
fn a_block_with_nothing_to_retire_does_not_rewrite_the_stage_file() {
    let temp = handoff_work_dir();
    let work = temp.path().join(".loom").join("work");
    executing_stage(&work);
    block_directly(&work, None);
    let file = crate::fs::stage_files::find_stage_file(&work.join("stages"), "test-stage")
        .unwrap()
        .unwrap();
    let before = std::fs::read_to_string(&file).unwrap();
    let updated_at = load_stage("test-stage", &work).unwrap().updated_at;
    let mut orchestrator = orchestrator_for(&work, temp.path());

    orchestrator.on_stage_blocked("test-stage").unwrap();

    assert_eq!(
        load_stage("test-stage", &work).unwrap().updated_at,
        updated_at
    );
    assert_eq!(std::fs::read_to_string(&file).unwrap(), before);
}

/// The block reason is free text a stage agent wrote: it reaches the daemon
/// console as one flattened, bounded line.
#[test]
fn the_block_reason_is_flattened_before_it_reaches_the_console() {
    let reason = format!(
        "blocked\n\u{1b}]52;c;ZXZpbA==\u{7}\u{202E}tail`x{}",
        "z".repeat(5_000)
    );

    let line = blocked_line("test-stage", &reason);

    assert!(
        line.starts_with("Stage 'test-stage' blocked: blocked ]52;c;ZXZpbA== tailˋxzzz"),
        "line: {line}"
    );
    assert!(!line.contains(|c: char| c.is_control() || c == '\u{202E}' || c == '`'));
    let prefix_len = "Stage 'test-stage' blocked: ".chars().count();
    let max = prefix_len + crate::context::untrusted::MAX_INLINE_CHARS;
    assert!(line.chars().count() <= max, "line is not bounded");
}
