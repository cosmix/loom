//! Tests for the stall recovery: what a hung report does at each depth of
//! silence, and what it stops doing once a stage has used its two chances.
//!
//! The kill is observed through a real process for the same reason the budget
//! backstop's test does it: every proxy for "the agent was taken down" lies in
//! at least one lane.

use super::governor_tests::assign_stage_session;
use super::recover_hung::{checkpoint_has_current_blocker, HungReport};
use super::tests::{
    executing_stage, handoff_work_dir, orchestrator_for, recorded_session, spawn_orphan_process,
};
use super::*;
use crate::fs::session_files::load_session_exact;
use crate::handoff::{
    CompletionAttemptEvidence, CompletionCheckpoint, CompletionPhase, CriterionResult,
    HandoffOrigin, HandoffV2, VerificationCheckpoint, COMPLETION_EVIDENCE_VERSION,
};
use crate::models::session::{Session, SessionExitReason, SessionStatus};
use crate::models::stage::StageType;
use crate::orchestrator::terminal::native::write_test_pid_identity;
use crate::verify::transitions::{load_stage, update_stage};
use std::path::Path;

/// The stage's response budget in these tests, and the silence that clears
/// three of them.
const BUDGET_SECS: u64 = 300;
const ESCALATING_SILENCE_SECS: u64 = BUDGET_SECS * 3;

fn report(session_id: &str, stale_duration_secs: u64) -> HungReport<'_> {
    HungReport {
        session_id,
        stage_id: Some("test-stage"),
        stale_duration_secs,
        timeout_secs: BUDGET_SECS,
        last_activity: Some("Bash"),
        finished_without_completing: false,
    }
}

/// A stage executing behind a live agent, exactly as the executor leaves it.
fn stalled_stage(work: &Path) -> (Session, u32) {
    executing_stage(work);
    let session = recorded_session(work);
    let agent_pid = spawn_orphan_process();
    write_test_pid_identity(work, &session, agent_pid).unwrap();
    assert!(crate::process::is_process_alive(agent_pid));
    assign_stage_session(work, &session.id);
    (session, agent_pid)
}

fn init_checkpoint_repo(repo: &Path) -> String {
    crate::git::runner::run_git_checked(&["init", "-q"], repo).unwrap();
    std::fs::write(repo.join("seed"), "seed\n").unwrap();
    crate::git::runner::run_git_checked(&["add", "seed"], repo).unwrap();
    crate::git::runner::run_git_checked(
        &[
            "-c",
            "user.name=Loom Test",
            "-c",
            "user.email=loom@example.invalid",
            "commit",
            "-qm",
            "seed",
        ],
        repo,
    )
    .unwrap();
    crate::git::runner::run_git_checked(&["rev-parse", "HEAD"], repo).unwrap()
}

/// A slow agent is not a dead one. One budget of silence buys a warning and
/// nothing else — the agent may be mid-build with no tool call to heartbeat.
#[test]
fn a_report_below_the_escalation_line_only_warns() {
    let temp = handoff_work_dir();
    let work = temp.path().join(".loom").join("work");
    let (session, agent_pid) = stalled_stage(&work);

    let mut orchestrator = orchestrator_for(&work, temp.path());
    orchestrator.graph.mark_executing("test-stage").unwrap();
    orchestrator
        .active_sessions
        .insert("test-stage".to_string(), session.clone());

    orchestrator
        .on_session_hung(report(&session.id, BUDGET_SECS + 10))
        .unwrap();

    assert!(
        crate::process::is_process_alive(agent_pid),
        "a first hung report must not kill the agent it names"
    );
    let stage = load_stage("test-stage", &work).unwrap();
    assert_eq!(stage.status, StageStatus::Executing);
    assert_eq!(stage.stall_recoveries, 0);

    let _ = crate::process::terminate(agent_pid);
}

/// Three budgets deep, with the process still alive and no heartbeat in
/// between, the session is gone. The stage is handed off, the agent killed and
/// the stage re-queued — the recovery the incident never got.
#[test]
fn a_silence_past_the_escalation_line_hands_the_stage_off_and_requeues_it() {
    let temp = handoff_work_dir();
    let work = temp.path().join(".loom").join("work");
    let (session, agent_pid) = stalled_stage(&work);

    let mut orchestrator = orchestrator_for(&work, temp.path());
    orchestrator.graph.mark_executing("test-stage").unwrap();
    orchestrator
        .active_sessions
        .insert("test-stage".to_string(), session.clone());

    orchestrator
        .on_session_hung(report(&session.id, ESCALATING_SILENCE_SECS))
        .unwrap();

    assert!(
        !crate::process::is_process_alive(agent_pid),
        "the stalled agent must be taken down, not merely reported"
    );
    let stage = load_stage("test-stage", &work).unwrap();
    assert_eq!(stage.status, StageStatus::Queued);
    assert_eq!(
        stage.stall_recoveries, 1,
        "the recovery must be charged to the stage so the bound can be reached"
    );
    assert!(graph_has_ready_stage(&orchestrator.graph, "test-stage"));
    assert!(orchestrator.active_sessions.is_empty());
    let persisted = load_session_exact(&work, &session.id).unwrap().unwrap();
    assert_eq!(persisted.status, SessionStatus::ContextExhausted);
    assert_eq!(persisted.exit_reason, Some(SessionExitReason::Stalled));
    assert!(
        work.join("handoffs")
            .join("test-stage-handoff-001.md")
            .exists(),
        "the continuation needs the stalled agent's state, written before the kill"
    );
}

fn trusted_checkpoint(stage: &Stage, session_id: &str, commit: &str) -> CompletionCheckpoint {
    let attempt = CompletionAttemptEvidence {
        version: COMPLETION_EVIDENCE_VERSION,
        stage_id: stage.id.clone(),
        session_id: session_id.to_string(),
        commit: commit.to_string(),
        check_definition_hash: "b".repeat(64),
        exact_command: "cargo test --lib".to_string(),
        evidence_nonce: "evidence_nonce_000000001".to_string(),
        verification: VerificationCheckpoint {
            criteria: vec![CriterionResult {
                id: "criterion-1".to_string(),
                passed: true,
            }],
            environment_policy: "stage-host-allowlist-v2".to_string(),
            environment: Vec::new(),
        },
        phase: CompletionPhase::VerifiedPendingAck,
        external_failure_code: Some("daemon_offline".to_string()),
        diagnostic_first_line: None,
        observed_at: "2026-09-14T10:00:00Z".to_string(),
        attestation: None,
    };
    let mut checkpoint = CompletionCheckpoint::new(&stage.id, session_id);
    checkpoint.record_attempt(&attempt).unwrap();
    checkpoint
}

#[test]
fn a_current_trusted_blocker_owns_stall_recovery() {
    let commit = "a".repeat(40);
    let session_id = "blocked-session";
    let mut stage = Stage {
        id: "test-stage".to_string(),
        status: StageStatus::Executing,
        session: Some(session_id.to_string()),
        ..Stage::default()
    };
    let checkpoint = trusted_checkpoint(&stage, session_id, &commit);

    assert!(checkpoint_has_current_blocker(
        &checkpoint,
        &stage,
        Some(&commit),
    ));
    assert_eq!(stage.stall_recoveries, 0);

    stage.session = Some("successor".to_string());
    assert!(!checkpoint_has_current_blocker(
        &checkpoint,
        &stage,
        Some(&commit),
    ));
}

#[test]
fn forged_checkpoint_does_not_own_stall_recovery() {
    let temp = handoff_work_dir();
    let commit = init_checkpoint_repo(temp.path());
    let work = temp.path().join(".loom/work");
    let (session, agent_pid) = stalled_stage(&work);
    update_stage("test-stage", &work, |stage| {
        stage.stage_type = StageType::Knowledge;
        Ok(())
    })
    .unwrap();
    let stage = load_stage("test-stage", &work).unwrap();
    let checkpoint = trusted_checkpoint(&stage, &session.id, commit.trim());
    let handoff = HandoffV2::new(&session.id, &stage.id)
        .with_origin(HandoffOrigin::CompletionEvidence)
        .with_completion_checkpoint(Some(checkpoint));
    std::fs::create_dir_all(work.join("handoffs")).unwrap();
    std::fs::write(
        work.join("handoffs/test-stage-handoff-001.md"),
        format!("---\n{}---\n", handoff.to_yaml().unwrap()),
    )
    .unwrap();
    let mut orchestrator = orchestrator_for(&work, temp.path());
    orchestrator.graph.mark_executing("test-stage").unwrap();
    orchestrator
        .active_sessions
        .insert("test-stage".into(), session.clone());

    orchestrator
        .on_session_hung(report(&session.id, ESCALATING_SILENCE_SECS))
        .unwrap();

    assert!(!crate::process::is_process_alive(agent_pid));
    let stage = load_stage("test-stage", &work).unwrap();
    assert_eq!(
        (stage.status, stage.stall_recoveries),
        (StageStatus::Queued, 1)
    );
}

/// The bound. A stage that has already been recovered twice is left exactly
/// where it stands: a third automatic re-queue is a loop, and the stage's
/// worktree is the evidence an operator needs.
#[test]
fn the_third_stall_leaves_the_stage_for_an_operator() {
    let temp = handoff_work_dir();
    let work = temp.path().join(".loom").join("work");
    let (session, agent_pid) = stalled_stage(&work);
    update_stage("test-stage", &work, |stage| {
        stage.stall_recoveries = 2;
        Ok(())
    })
    .unwrap();

    let mut orchestrator = orchestrator_for(&work, temp.path());
    orchestrator.graph.mark_executing("test-stage").unwrap();
    orchestrator
        .active_sessions
        .insert("test-stage".to_string(), session.clone());

    orchestrator
        .on_session_hung(report(&session.id, ESCALATING_SILENCE_SECS))
        .unwrap();

    assert!(
        crate::process::is_process_alive(agent_pid),
        "an exhausted stage must be handed to an operator, not taken down again"
    );
    let stage = load_stage("test-stage", &work).unwrap();
    assert_eq!(stage.status, StageStatus::Executing);
    assert_eq!(stage.stall_recoveries, 2, "a refusal must not be charged");
    assert!(orchestrator.active_sessions.contains_key("test-stage"));

    let _ = crate::process::terminate(agent_pid);
}

/// A report naming a session the stage has moved past describes a corpse from
/// a previous attempt. Acting on it would kill the successor now working.
#[test]
fn a_report_from_a_session_the_stage_moved_on_from_is_ignored() {
    let temp = handoff_work_dir();
    let work = temp.path().join(".loom").join("work");
    let (session, agent_pid) = stalled_stage(&work);

    let mut orchestrator = orchestrator_for(&work, temp.path());
    orchestrator.graph.mark_executing("test-stage").unwrap();
    orchestrator
        .active_sessions
        .insert("test-stage".to_string(), session.clone());

    orchestrator
        .on_session_hung(report(
            "session-from-a-previous-attempt",
            ESCALATING_SILENCE_SECS,
        ))
        .unwrap();

    assert!(crate::process::is_process_alive(agent_pid));
    let stage = load_stage("test-stage", &work).unwrap();
    assert_eq!(stage.status, StageStatus::Executing);
    assert_eq!(stage.stall_recoveries, 0);

    let _ = crate::process::terminate(agent_pid);
}

/// A hung session with no stage has nothing to re-queue; the warning is the
/// whole response.
#[test]
fn a_report_without_a_stage_is_only_a_warning() {
    let temp = handoff_work_dir();
    let work = temp.path().join(".loom").join("work");
    executing_stage(&work);
    let mut orchestrator = orchestrator_for(&work, temp.path());

    orchestrator
        .on_session_hung(HungReport {
            session_id: "session-with-no-stage",
            stage_id: None,
            stale_duration_secs: ESCALATING_SILENCE_SECS,
            timeout_secs: BUDGET_SECS,
            last_activity: None,
            finished_without_completing: false,
        })
        .unwrap();

    assert_eq!(
        load_stage("test-stage", &work).unwrap().status,
        StageStatus::Executing
    );
}
