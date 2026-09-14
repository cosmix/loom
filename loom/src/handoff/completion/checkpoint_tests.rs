use super::RecordOutcome;
use crate::handoff::schema::{
    AcceptedReceipt, CompletionAttemptEvidence, CompletionCheckpoint, CompletionPhase,
    CriterionResult, VerificationCheckpoint, COMPLETION_EVIDENCE_VERSION, MAX_EVIDENCE_NONCES,
};

pub(super) const STAGE: &str = "stage-one";
pub(super) const SESSION: &str = "session-0000000000000001";
const COMMIT: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const CHECK_HASH: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
pub(super) const NONCE: &str = "nonce-0000000000000000001";
pub(super) fn evidence(nonce: &str, observed_at: &str) -> CompletionAttemptEvidence {
    CompletionAttemptEvidence {
        version: COMPLETION_EVIDENCE_VERSION,
        stage_id: STAGE.to_string(),
        session_id: SESSION.to_string(),
        commit: COMMIT.to_string(),
        check_definition_hash: CHECK_HASH.to_string(),
        exact_command: "cargo test --lib completion".to_string(),
        evidence_nonce: nonce.to_string(),
        verification: VerificationCheckpoint {
            criteria: vec![CriterionResult {
                id: "completion-tests".to_string(),
                passed: true,
            }],
            environment_policy: "trusted-host-v1".to_string(),
            environment: Vec::new(),
        },
        phase: CompletionPhase::VerifiedPendingAck,
        external_failure_code: Some("daemon_unavailable".to_string()),
        diagnostic_first_line: Some("daemon did not acknowledge completion".to_string()),
        observed_at: observed_at.to_string(),
        attestation: None,
    }
}

pub(super) fn checkpoint() -> CompletionCheckpoint {
    CompletionCheckpoint::new(STAGE, SESSION)
}
pub(super) fn receipt(nonce: &str, completion_nonce: &str) -> AcceptedReceipt {
    AcceptedReceipt {
        evidence_nonce: nonce.to_string(),
        completion_nonce: completion_nonce.to_string(),
        commit: COMMIT.to_string(),
        attestation: None,
    }
}

#[test]
fn record_first_verified_failure_is_actionable() {
    let mut checkpoint = checkpoint();

    let outcome = checkpoint
        .record_attempt(&evidence(NONCE, "2026-09-14T10:00:00Z"))
        .unwrap();

    assert_eq!(outcome, RecordOutcome::Recorded);
    assert_eq!(checkpoint.repeat_count(), 1);
    assert!(checkpoint.is_actionable());
}

#[test]
fn record_duplicate_delivery_changes_nothing() {
    let mut checkpoint = checkpoint();
    let attempt = evidence(NONCE, "2026-09-14T10:00:00Z");
    checkpoint.record_attempt(&attempt).unwrap();
    let before = checkpoint.clone();

    let outcome = checkpoint.record_attempt(&attempt).unwrap();

    assert_eq!(outcome, RecordOutcome::Duplicate);
    assert_eq!(checkpoint, before);
}

#[test]
fn record_phase_update_keeps_one_repeat() {
    let mut checkpoint = checkpoint();
    let first = evidence(NONCE, "2026-09-14T10:00:00Z");
    checkpoint.record_attempt(&first).unwrap();
    let mut rejected = first;
    rejected.phase = CompletionPhase::DaemonRejected;
    rejected.observed_at = "2026-09-14T10:01:00Z".to_string();

    let outcome = checkpoint.record_attempt(&rejected).unwrap();

    assert_eq!(outcome, RecordOutcome::PhaseUpdated);
    assert_eq!(
        checkpoint.current_phase(),
        Some(CompletionPhase::DaemonRejected)
    );
    assert_eq!(checkpoint.repeat_count(), 1);
}

#[test]
fn record_distinct_nonces_count_same_blocker() {
    let mut checkpoint = checkpoint();
    checkpoint
        .record_attempt(&evidence(NONCE, "2026-09-14T10:00:00Z"))
        .unwrap();

    checkpoint
        .record_attempt(&evidence(
            "nonce-0000000000000000002",
            "2026-09-14T10:01:00Z",
        ))
        .unwrap();

    assert_eq!(checkpoint.repeat_count(), 2);
}

#[test]
fn record_new_blocker_resets_count_without_losing_history() {
    let mut checkpoint = checkpoint();
    checkpoint
        .record_attempt(&evidence(NONCE, "2026-09-14T10:00:00Z"))
        .unwrap();
    let mut changed_commit = evidence("nonce-0000000000000000002", "2026-09-14T10:01:00Z");
    changed_commit.commit = "cccccccccccccccccccccccccccccccccccccccc".to_string();
    checkpoint.record_attempt(&changed_commit).unwrap();
    assert_eq!(checkpoint.repeat_count(), 1);
    let mut changed_code = evidence("nonce-0000000000000000003", "2026-09-14T10:02:00Z");
    changed_code.commit = changed_commit.commit;
    changed_code.external_failure_code = Some("ack_rejected".to_string());

    checkpoint.record_attempt(&changed_code).unwrap();

    assert_eq!(checkpoint.repeat_count(), 1);
    assert_eq!(checkpoint.observations.len(), 3);
    assert_eq!(
        checkpoint.blocker.unwrap().external_failure_code,
        "ack_rejected"
    );
}

#[test]
fn record_same_nonce_with_different_identity_conflicts() {
    let mut checkpoint = checkpoint();
    checkpoint
        .record_attempt(&evidence(NONCE, "2026-09-14T10:00:00Z"))
        .unwrap();
    let mut changed = evidence(NONCE, "2026-09-14T10:01:00Z");
    changed.exact_command = "cargo test --workspace".to_string();

    let outcome = checkpoint.record_attempt(&changed).unwrap();

    assert_eq!(outcome, RecordOutcome::Conflict);
    assert!(checkpoint.conflict);
    assert!(!checkpoint.is_actionable());
}

#[test]
fn record_thirty_third_nonce_exhausts_capacity_without_eviction() {
    let mut checkpoint = checkpoint();
    for index in 0..MAX_EVIDENCE_NONCES {
        let nonce = format!("nonce-{index:019}");
        let observed_at = format!("2026-09-14T10:00:{index:02}Z");
        assert_eq!(
            checkpoint
                .record_attempt(&evidence(&nonce, &observed_at))
                .unwrap(),
            RecordOutcome::Recorded
        );
    }
    let overflow = evidence("nonce-0000000000000000032", "2026-09-14T10:00:32Z");

    let outcome = checkpoint.record_attempt(&overflow).unwrap();

    assert_eq!(outcome, RecordOutcome::CapacityExhausted);
    assert_eq!(checkpoint.observations.len(), MAX_EVIDENCE_NONCES);
    assert!(checkpoint.capacity_exhausted);
}

#[test]
fn record_tool_failure_is_diagnostic_only() {
    let mut checkpoint = checkpoint();
    let mut attempt = evidence(NONCE, "2026-09-14T10:00:00Z");
    attempt.phase = CompletionPhase::ToolFailed;
    attempt.external_failure_code = None;

    let outcome = checkpoint.record_attempt(&attempt).unwrap();

    assert_eq!(outcome, RecordOutcome::Recorded);
    assert!(checkpoint.blocker.is_none());
    assert!(!checkpoint.is_actionable());
}

#[test]
fn merge_is_idempotent_commutative_for_counts_and_preserves_richer_side() {
    let mut left = checkpoint();
    left.record_attempt(&evidence(NONCE, "2026-09-14T10:00:00Z"))
        .unwrap();
    let mut right = checkpoint();
    right
        .record_attempt(&evidence(
            "nonce-0000000000000000002",
            "2026-09-14T10:01:00Z",
        ))
        .unwrap();
    let mut reverse = right.clone();

    assert!(left.merge_from(&right).unwrap());
    assert!(!left.merge_from(&right).unwrap());
    assert!(reverse.merge_from(&checkpoint_with_attempt()).unwrap());

    assert_eq!(left.repeat_count(), 2);
    assert_eq!(reverse.repeat_count(), 2);
    assert_eq!(left.observations.len(), 2);
    assert_eq!(
        left.latest.unwrap().evidence_nonce,
        "nonce-0000000000000000002"
    );
}

#[test]
fn merge_same_second_same_nonce_failure_update_is_actionable() {
    let mut initial = evidence(NONCE, "2026-09-14T10:00:00Z");
    initial.external_failure_code = None;
    let mut merged = checkpoint();
    merged.record_attempt(&initial).unwrap();
    let mut updated = checkpoint();
    let mut failed = evidence(NONCE, "2026-09-14T10:00:00Z");
    failed.external_failure_code = Some("daemon_transport".to_string());
    updated.record_attempt(&failed).unwrap();

    assert!(merged.merge_from(&updated).unwrap());

    assert!(merged.is_actionable());
    assert_eq!(merged.repeat_count(), 1);
}

#[test]
fn merge_same_second_new_fingerprint_replaces_blocker_and_keeps_history() {
    let mut merged = checkpoint_with_attempt();
    let mut updated = checkpoint();
    let mut attempt = evidence("nonce-0000000000000000002", "2026-09-14T10:00:00Z");
    attempt.external_failure_code = Some("ack_rejected".to_string());
    updated.record_attempt(&attempt).unwrap();

    assert!(merged.merge_from(&updated).unwrap());

    assert_eq!(merged.observations.len(), 2);
    assert_eq!(merged.repeat_count(), 1);
    assert_eq!(
        merged.blocker.unwrap().external_failure_code,
        "ack_rejected"
    );
}

#[test]
fn merge_same_second_equal_fingerprint_prefers_incoming_phase() {
    let mut merged = checkpoint();
    merged
        .record_attempt(&evidence(NONCE, "2026-09-14T10:00:00Z"))
        .unwrap();
    let mut incoming = checkpoint();
    let mut rejected = evidence(NONCE, "2026-09-14T10:00:00Z");
    rejected.phase = CompletionPhase::DaemonRejected;
    incoming.record_attempt(&rejected).unwrap();

    assert!(merged.merge_from(&incoming).unwrap());

    assert_eq!(
        merged.observations[0].phase,
        CompletionPhase::DaemonRejected
    );
}

#[test]
fn merge_equal_copy_is_unchanged() {
    let mut checkpoint = checkpoint_with_attempt();
    let equal = checkpoint.clone();

    assert!(!checkpoint.merge_from(&equal).unwrap());
}

pub(super) fn checkpoint_with_attempt() -> CompletionCheckpoint {
    let mut value = checkpoint();
    value
        .record_attempt(&evidence(NONCE, "2026-09-14T10:00:00Z"))
        .unwrap();
    value
}

#[test]
fn merge_wrong_session_errors() {
    let mut checkpoint = checkpoint();
    let other = CompletionCheckpoint::new(STAGE, "session-0000000000000002");

    assert!(checkpoint.merge_from(&other).is_err());
}

#[test]
fn record_accepted_requires_known_nonce_and_is_idempotent() {
    let mut checkpoint = checkpoint();
    checkpoint
        .record_attempt(&evidence(NONCE, "2026-09-14T10:00:00Z"))
        .unwrap();
    assert!(checkpoint
        .record_accepted(receipt("nonce-0000000000000000009", NONCE))
        .is_err());
    let accepted = receipt(NONCE, "completion-0000000000001");

    checkpoint.record_accepted(accepted.clone()).unwrap();
    checkpoint.record_accepted(accepted).unwrap();

    assert!(!checkpoint.is_actionable());
    assert!(checkpoint
        .record_accepted(receipt(NONCE, "completion-0000000000002"))
        .is_err());
}

#[test]
fn merge_unknown_accepted_receipt_sets_conflict() {
    let mut merged = checkpoint();
    let mut incoming = checkpoint();
    incoming.accepted = Some(receipt(
        "nonce-0000000000000000009",
        "completion-0000000000001",
    ));

    merged.merge_from(&incoming).unwrap();

    assert!(merged.conflict);
    assert!(merged.accepted.is_none());
}

#[test]
fn serde_round_trip_preserves_repeat_count() {
    let mut checkpoint = checkpoint();
    checkpoint
        .record_attempt(&evidence(NONCE, "2026-09-14T10:00:00Z"))
        .unwrap();
    checkpoint
        .record_attempt(&evidence(
            "nonce-0000000000000000002",
            "2026-09-14T10:01:00Z",
        ))
        .unwrap();

    let encoded = serde_json::to_string(&checkpoint).unwrap();
    let decoded: CompletionCheckpoint = serde_json::from_str(&encoded).unwrap();

    assert_eq!(decoded.repeat_count(), 2);
}
