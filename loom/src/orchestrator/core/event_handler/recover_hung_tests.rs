//! Tests for the stall recovery: what a hung report does at each depth of
//! silence, and what it stops doing once a stage has used its two chances.
//!
//! The kill is observed through a real process for the same reason the budget
//! backstop's test does it: every proxy for "the agent was taken down" lies in
//! at least one lane.

#[path = "recover_hung_latch_tests.rs"]
mod latch_tests;

use super::governor_tests::assign_stage_session;
use super::recover_hung::{checkpoint_has_current_blocker, HungReport};
use super::tests::{executing_stage, handoff_work_dir, orchestrator_for, recorded_session};
use super::*;
use crate::fs::session_files::load_session_exact;
use crate::handoff::{
    CompletionAttemptEvidence, CompletionCheckpoint, CompletionPhase, CriterionResult,
    HandoffOrigin, HandoffV2, VerificationCheckpoint, COMPLETION_EVIDENCE_VERSION,
};
use crate::models::session::{Session, SessionExitReason, SessionStatus};
use crate::models::stage::{AcceptanceCriterion, StageType};
use crate::orchestrator::monitor::heartbeat::{write_heartbeat, Heartbeat};
use crate::orchestrator::monitor::stall_line::COMPLETION_ACTIVITY;
use crate::orchestrator::terminal::launch::write_test_pid_identity;
use crate::test_git::{git, TestRepo};
use crate::test_stand_in::{orphan, require_orphan_signals, Orphan};
use crate::verify::transitions::{load_stage, update_stage};
use std::path::Path;

/// The stage's response budget in these tests, and the silence that clears
/// three of them.
pub(super) const BUDGET_SECS: u64 = 300;
pub(super) const ESCALATING_SILENCE_SECS: u64 = BUDGET_SECS * 3;

pub(super) fn report(session_id: &str, stale_duration_secs: u64) -> HungReport<'_> {
    HungReport {
        session_id,
        stage_id: Some("test-stage"),
        stale_duration_secs,
        timeout_secs: BUDGET_SECS,
        last_activity: Some("Bash"),
        finished_without_completing: false,
    }
}

#[test]
#[serial_test::serial]
fn forward_facts_hold_for_a_native_worker() {
    require_orphan_signals!();
    let temp = handoff_work_dir();
    let work = temp.path().join(".loom").join("work");
    let (session, agent_stand_in) = stalled_stage(&work);
    let agent_pid = agent_stand_in.pid();
    let mut orchestrator = orchestrator_for(&work, temp.path());
    orchestrator.graph.mark_executing("test-stage").unwrap();
    orchestrator
        .active_sessions
        .insert("test-stage".to_string(), session.clone());
    crate::host_verify::submit::native_hold_tests::seed(&work, "test-stage");
    orchestrator
        .on_session_hung(report(&session.id, ESCALATING_SILENCE_SECS))
        .unwrap();
    assert!(crate::process::is_process_alive(agent_pid));
    assert_eq!(
        load_stage("test-stage", &work).unwrap().status,
        StageStatus::Executing
    );
}

/// A stage executing behind a live agent, exactly as the executor leaves it.
pub(super) fn stalled_stage(work: &Path) -> (Session, Orphan) {
    executing_stage(work);
    let session = recorded_session(work);
    let agent_stand_in = orphan();
    let agent_pid = agent_stand_in.pid();
    write_test_pid_identity(work, &session, agent_pid).unwrap();
    assert!(crate::process::is_process_alive(agent_pid));
    assign_stage_session(work, &session.id);
    (session, agent_stand_in)
}

fn init_checkpoint_repo(repo: &Path) -> String {
    TestRepo::builder()
        .identity("Loom Test", "loom@example.invalid")
        .seed("seed", "seed\n", "seed")
        .init_in(repo);
    git(repo, &["rev-parse", "HEAD"])
}

/// A slow agent is not a dead one. One budget of silence buys a warning and
/// nothing else — the agent may be mid-build with no tool call to heartbeat.
#[test]
fn a_report_below_the_escalation_line_only_warns() {
    require_orphan_signals!();
    let temp = handoff_work_dir();
    let work = temp.path().join(".loom").join("work");
    let (session, agent_stand_in) = stalled_stage(&work);
    let agent_pid = agent_stand_in.pid();

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
}

/// Three budgets deep, with the process still alive and no heartbeat in
/// between, the session is gone. The stage is handed off, the agent killed and
/// the stage re-queued — the recovery the incident never got.
#[test]
fn a_silence_past_the_escalation_line_hands_the_stage_off_and_requeues_it() {
    require_orphan_signals!();
    let temp = handoff_work_dir();
    let work = temp.path().join(".loom").join("work");
    let (session, agent_stand_in) = stalled_stage(&work);
    let agent_pid = agent_stand_in.pid();

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
    require_orphan_signals!();
    let temp = handoff_work_dir();
    let commit = init_checkpoint_repo(temp.path());
    let work = temp.path().join(".loom/work");
    let (session, agent_stand_in) = stalled_stage(&work);
    let agent_pid = agent_stand_in.pid();
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

/// A report naming a session the stage has moved past describes a corpse from
/// a previous attempt. Acting on it would kill the successor now working.
#[test]
fn a_report_from_a_session_the_stage_moved_on_from_is_ignored() {
    require_orphan_signals!();
    let temp = handoff_work_dir();
    let work = temp.path().join(".loom").join("work");
    let (session, agent_stand_in) = stalled_stage(&work);
    let agent_pid = agent_stand_in.pid();

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

/// The line a stage with two acceptance criteria gets while its pinned
/// completion runs: one timeout each and one for impact selection, plus the margin.
const COMPLETION_LINE_SECS: u64 = 3 * 300 + 1_800;

/// A stalled stage whose own latest heartbeat says its pinned completion is running.
fn completing_stage(work: &Path) -> (Session, Orphan) {
    let (session, agent_stand_in) = stalled_stage(work);
    update_stage("test-stage", work, |stage| {
        stage.acceptance = vec![
            AcceptanceCriterion::Simple("cargo test".to_string()),
            AcceptanceCriterion::Simple("cargo clippy".to_string()),
        ];
        Ok(())
    })
    .unwrap();
    let heartbeat = Heartbeat::new("test-stage".to_string(), session.id.clone())
        .with_last_tool("Bash".to_string())
        .with_activity(COMPLETION_ACTIVITY.to_string());
    write_heartbeat(work, &heartbeat).unwrap();
    (session, agent_stand_in)
}

fn report_completion_silence(work: &Path, root: &Path, session: &Session, silence: u64) {
    let mut orchestrator = orchestrator_for(work, root);
    orchestrator.graph.mark_executing("test-stage").unwrap();
    orchestrator
        .active_sessions
        .insert("test-stage".to_string(), session.clone());
    orchestrator
        .on_session_hung(report(&session.id, silence))
        .unwrap();
}

/// Acceptance prints nothing until every criterion has run, so a completion
/// is one long silence. Past three budgets but inside its own line, it is left
/// running and charged nothing.
#[test]
fn a_running_completion_inside_its_line_is_not_requeued() {
    require_orphan_signals!();
    let temp = handoff_work_dir();
    let work = temp.path().join(".loom").join("work");
    let (session, agent_stand_in) = completing_stage(&work);
    let agent_pid = agent_stand_in.pid();
    const { assert!(COMPLETION_LINE_SECS - 1 > ESCALATING_SILENCE_SECS) };

    report_completion_silence(&work, temp.path(), &session, COMPLETION_LINE_SECS - 1);

    assert!(crate::process::is_process_alive(agent_pid));
    let stage = load_stage("test-stage", &work).unwrap();
    assert_eq!(
        (stage.status, stage.stall_recoveries),
        (StageStatus::Executing, 0)
    );
}

/// The allowance is finite: a completion silent past its line is a stall.
#[test]
fn a_running_completion_past_its_line_is_requeued() {
    require_orphan_signals!();
    let temp = handoff_work_dir();
    let work = temp.path().join(".loom").join("work");
    let (session, agent_stand_in) = completing_stage(&work);
    let agent_pid = agent_stand_in.pid();

    report_completion_silence(&work, temp.path(), &session, COMPLETION_LINE_SECS);

    assert!(!crate::process::is_process_alive(agent_pid));
    let stage = load_stage("test-stage", &work).unwrap();
    assert_eq!(
        (stage.status, stage.stall_recoveries),
        (StageStatus::Queued, 1)
    );
}
