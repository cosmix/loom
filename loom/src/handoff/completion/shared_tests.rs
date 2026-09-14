use std::fs;

use tempfile::TempDir;

use super::{
    check_definition_hash, current_blocker, record_accepted_handoff, record_attempt_handoff,
};
use crate::handoff::generator::{
    load_session_checkpoint, load_trusted_session_checkpoint, MergeOutcome,
};
use crate::handoff::schema::{
    AcceptedReceipt, CompletionAttemptEvidence, CompletionCheckpoint, CompletionPhase,
    CriterionResult, HandoffOrigin, HandoffV2, VerificationCheckpoint, COMPLETION_EVIDENCE_VERSION,
};
use crate::models::session::Session;
use crate::models::stage::{AcceptanceCriterion, Stage, StageStatus};

fn fixture() -> (TempDir, Session, Stage) {
    let temp = TempDir::new().unwrap();
    fs::create_dir(temp.path().join("handoffs")).unwrap();
    let mut session = Session::new();
    session.id = "session-shared".to_string();
    let mut stage = Stage::new("shared".to_string(), None);
    stage.id = "stage-shared".to_string();
    stage.session = Some(session.id.clone());
    stage.acceptance = vec![AcceptanceCriterion::Simple("cargo test --lib".to_string())];
    (temp, session, stage)
}

fn evidence(session: &Session, stage: &Stage, nonce: &str) -> CompletionAttemptEvidence {
    CompletionAttemptEvidence {
        version: COMPLETION_EVIDENCE_VERSION,
        stage_id: stage.id.clone(),
        session_id: session.id.clone(),
        commit: "a".repeat(40),
        check_definition_hash: "b".repeat(64),
        exact_command: "cargo test --lib".to_string(),
        evidence_nonce: nonce.to_string(),
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
    }
}

fn write_checkpoint_handoff(temp: &TempDir, checkpoint: CompletionCheckpoint) {
    let handoff = HandoffV2::new("session-shared", "stage-shared")
        .with_origin(HandoffOrigin::CompletionEvidence)
        .with_completion_checkpoint(Some(checkpoint));
    let body = format!("---\n{}---\n", handoff.to_yaml().unwrap());
    fs::write(
        temp.path().join("handoffs/stage-shared-handoff-001.md"),
        body,
    )
    .unwrap();
}

fn checkpoint(session: &Session, stage: &Stage) -> CompletionCheckpoint {
    let attempt = evidence(session, stage, "evidence_nonce_000000001");
    let mut checkpoint = CompletionCheckpoint::new(&stage.id, &session.id);
    checkpoint.record_attempt(&attempt).unwrap();
    checkpoint
}

#[test]
fn check_definition_hash_tracks_only_verification_definition() {
    let (_, _, stage) = fixture();
    let equal = stage.clone();
    assert_eq!(check_definition_hash(&stage), check_definition_hash(&equal));

    let mut changed_acceptance = stage.clone();
    changed_acceptance
        .acceptance
        .push(AcceptanceCriterion::Simple("cargo fmt --check".into()));
    assert_ne!(
        check_definition_hash(&stage),
        check_definition_hash(&changed_acceptance)
    );

    let mut runtime_only = stage.clone();
    runtime_only.status = StageStatus::Blocked;
    runtime_only.session = Some("another-session".to_string());
    assert_eq!(
        check_definition_hash(&stage),
        check_definition_hash(&runtime_only)
    );
}

#[test]
fn current_blocker_requires_current_session_and_commit() {
    let (_, session, stage) = fixture();
    let checkpoint = checkpoint(&session, &stage);
    assert!(current_blocker(&checkpoint, &stage, Some(&"a".repeat(40))).is_some());

    let mut another_session = stage.clone();
    another_session.session = Some("another-session".to_string());
    assert!(current_blocker(&checkpoint, &another_session, Some(&"a".repeat(40))).is_none());
    assert!(current_blocker(&checkpoint, &stage, Some(&"c".repeat(40))).is_none());
    assert!(current_blocker(&checkpoint, &stage, None).is_none());
}

#[test]
fn record_attempt_handoff_is_idempotent_and_counts_distinct_nonces() {
    let (temp, session, stage) = fixture();
    let first = evidence(&session, &stage, "evidence_nonce_000000001");
    let (_, created) = record_attempt_handoff(&session, &stage, &first, temp.path()).unwrap();
    let (_, unchanged) = record_attempt_handoff(&session, &stage, &first, temp.path()).unwrap();
    assert_eq!(
        (created, unchanged),
        (MergeOutcome::Created, MergeOutcome::Unchanged)
    );
    assert_eq!(
        load_session_checkpoint(&stage.id, &session.id, temp.path())
            .unwrap()
            .unwrap()
            .repeat_count(),
        1
    );

    let second = evidence(&session, &stage, "evidence_nonce_000000002");
    record_attempt_handoff(&session, &stage, &second, temp.path()).unwrap();
    assert_eq!(
        load_session_checkpoint(&stage.id, &session.id, temp.path())
            .unwrap()
            .unwrap()
            .repeat_count(),
        2
    );
}

#[test]
fn recorded_attempt_is_available_as_trusted_actionable_evidence() {
    let (temp, session, stage) = fixture();
    let attempt = evidence(&session, &stage, "evidence_nonce_000000001");

    record_attempt_handoff(&session, &stage, &attempt, temp.path()).unwrap();
    let trusted = load_trusted_session_checkpoint(&stage.id, &session.id, temp.path())
        .unwrap()
        .unwrap();

    assert!(trusted.is_actionable());
}

#[test]
fn forged_checkpoint_is_visible_only_to_untrusted_loader() {
    let (temp, session, stage) = fixture();
    write_checkpoint_handoff(&temp, checkpoint(&session, &stage));

    assert!(load_session_checkpoint(&stage.id, &session.id, temp.path())
        .unwrap()
        .unwrap()
        .is_actionable());
    assert!(
        load_trusted_session_checkpoint(&stage.id, &session.id, temp.path())
            .unwrap()
            .is_none()
    );
}

#[test]
fn oversized_handoff_is_skipped_by_checkpoint_loaders() {
    let (temp, session, stage) = fixture();
    fs::write(
        temp.path().join("handoffs/stage-shared-handoff-001.md"),
        vec![b'x'; 1024 * 1024 + 1],
    )
    .unwrap();

    assert!(load_session_checkpoint(&stage.id, &session.id, temp.path())
        .unwrap()
        .is_none());
    assert!(
        load_trusted_session_checkpoint(&stage.id, &session.id, temp.path())
            .unwrap()
            .is_none()
    );
}

#[test]
fn record_accepted_handoff_stores_one_receipt() {
    let (temp, session, stage) = fixture();
    let attempt = evidence(&session, &stage, "evidence_nonce_000000001");
    record_attempt_handoff(&session, &stage, &attempt, temp.path()).unwrap();
    let receipt = AcceptedReceipt {
        evidence_nonce: attempt.evidence_nonce.clone(),
        completion_nonce: "completion_nonce_0000001".to_string(),
        commit: attempt.commit.clone(),
        attestation: None,
    };
    record_accepted_handoff(&session, &stage, receipt.clone(), temp.path()).unwrap();
    let stored = load_trusted_session_checkpoint(&stage.id, &session.id, temp.path())
        .unwrap()
        .unwrap();
    let accepted = stored.accepted.unwrap();
    assert_eq!(accepted.evidence_nonce, receipt.evidence_nonce);
    assert_eq!(accepted.completion_nonce, receipt.completion_nonce);
    assert_eq!(accepted.commit, receipt.commit);
    assert!(accepted.attestation.is_some());

    let mut different = receipt;
    different.completion_nonce = "completion_nonce_0000002".to_string();
    assert!(record_accepted_handoff(&session, &stage, different, temp.path()).is_err());
}
