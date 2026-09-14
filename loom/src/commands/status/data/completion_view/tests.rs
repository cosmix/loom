use super::*;
use crate::handoff::{
    CompletionAttemptEvidence, CompletionPhase, CriterionResult, VerificationCheckpoint,
    COMPLETION_EVIDENCE_VERSION,
};
use crate::models::session::SessionStatus;

const COMMIT: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn stage(status: StageStatus) -> Stage {
    let mut stage = Stage::new("completion view".to_string(), None);
    stage.id = "stage-1".to_string();
    stage.status = status;
    stage.session = Some("session-1".to_string());
    stage
}

fn session(status: SessionStatus, exit_reason: Option<SessionExitReason>) -> Session {
    let mut session = Session::new();
    session.id = "session-1".to_string();
    session.stage_id = Some("stage-1".to_string());
    session.status = status;
    session.exit_reason = exit_reason;
    session
}

fn evidence(nonce: &str) -> CompletionAttemptEvidence {
    CompletionAttemptEvidence {
        version: COMPLETION_EVIDENCE_VERSION,
        stage_id: "stage-1".to_string(),
        session_id: "session-1".to_string(),
        commit: COMMIT.to_string(),
        check_definition_hash: "check-v1".to_string(),
        exact_command: "cargo test --lib".to_string(),
        evidence_nonce: nonce.to_string(),
        verification: VerificationCheckpoint {
            criteria: vec![CriterionResult {
                id: "criterion-1".to_string(),
                passed: true,
            }],
            environment_policy: "trusted-host-v1".to_string(),
            environment: Vec::new(),
        },
        phase: CompletionPhase::VerifiedPendingAck,
        external_failure_code: Some("sandbox_denied".to_string()),
        diagnostic_first_line: Some("sandbox denied execution".to_string()),
        observed_at: "2026-09-14T10:00:00Z".to_string(),
        attestation: None,
    }
}

fn checkpoint(repeats: usize) -> CompletionCheckpoint {
    let mut checkpoint = CompletionCheckpoint::new("stage-1", "session-1");
    for index in 0..repeats {
        checkpoint
            .record_attempt(&evidence(&format!("nonce-{index:020}")))
            .unwrap();
    }
    checkpoint
}

#[test]
fn executing_first_observation_is_pending() {
    let summary = completion_blocker_summary(
        &stage(StageStatus::Executing),
        None,
        &checkpoint(1),
        Some(COMMIT),
    )
    .unwrap();

    assert_eq!(summary.state, CompletionBlockerState::Pending);
    assert_eq!(
        summary.next_action,
        "watching: a repeat of sandbox_denied parks this stage"
    );
}

#[test]
fn executing_repeat_is_blocked() {
    let summary = completion_blocker_summary(
        &stage(StageStatus::Executing),
        None,
        &checkpoint(2),
        Some(COMMIT),
    )
    .unwrap();

    assert_eq!(summary.state, CompletionBlockerState::Blocked);
}

#[test]
fn capacity_exhaustion_is_blocked() {
    let mut checkpoint = checkpoint(1);
    checkpoint.capacity_exhausted = true;

    let summary = completion_blocker_summary(
        &stage(StageStatus::Executing),
        None,
        &checkpoint,
        Some(COMMIT),
    )
    .unwrap();

    assert_eq!(summary.state, CompletionBlockerState::Blocked);
}

#[test]
fn parked_terminal_session_is_blocked() {
    let outgoing = session(
        SessionStatus::Crashed,
        Some(SessionExitReason::CriteriaBlocked),
    );
    let summary = completion_blocker_summary(
        &stage(StageStatus::NeedsHumanReview),
        Some(&outgoing),
        &checkpoint(2),
        Some(COMMIT),
    )
    .unwrap();

    assert_eq!(summary.state, CompletionBlockerState::Blocked);
    assert_eq!(
        summary.next_action,
        "fix sandbox_denied, then retry or reset the stage"
    );
}

#[test]
fn parked_live_session_has_unknown_ownership() {
    let outgoing = session(SessionStatus::Running, None);
    let summary = completion_blocker_summary(
        &stage(StageStatus::NeedsHumanReview),
        Some(&outgoing),
        &checkpoint(2),
        Some(COMMIT),
    )
    .unwrap();

    assert_eq!(summary.state, CompletionBlockerState::OwnershipUnknown);
    assert_eq!(
        summary.next_action,
        "confirm session session-1 has exited before reassigning"
    );
    let missing = completion_blocker_summary(
        &stage(StageStatus::NeedsHumanReview),
        None,
        &checkpoint(2),
        Some(COMMIT),
    )
    .unwrap();
    assert_eq!(missing.state, CompletionBlockerState::OwnershipUnknown);
}

#[test]
fn changed_session_commit_and_completed_stage_hide_checkpoint() {
    let checkpoint = checkpoint(2);
    let mut changed_session = stage(StageStatus::Executing);
    changed_session.session = Some("session-2".to_string());

    assert!(
        completion_blocker_summary(&changed_session, None, &checkpoint, Some(COMMIT)).is_none()
    );
    assert!(completion_blocker_summary(
        &stage(StageStatus::Executing),
        None,
        &checkpoint,
        Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
    )
    .is_none());
    assert!(completion_blocker_summary(
        &stage(StageStatus::Completed),
        None,
        &checkpoint,
        Some(COMMIT)
    )
    .is_none());
    assert!(completion_blocker_summary(
        &stage(StageStatus::Executing),
        None,
        &CompletionCheckpoint::new("stage-1", "session-1"),
        Some(COMMIT),
    )
    .is_none());
}

#[test]
fn outgoing_reason_requires_a_terminal_session() {
    let live = session(SessionStatus::Running, Some(SessionExitReason::Replaced));
    let terminal = session(SessionStatus::Completed, Some(SessionExitReason::Completed));

    assert_eq!(outgoing_exit_reason(Some(&live)), None);
    assert_eq!(
        outgoing_exit_reason(Some(&terminal)),
        Some(SessionExitReason::Completed)
    );
}
