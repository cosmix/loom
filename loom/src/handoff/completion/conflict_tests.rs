//! Conflict and capacity state recomputed from signed evidence, exercised
//! through the real writer and the public trusted loader.

use std::fs;

use tempfile::TempDir;

use super::{record_accepted_handoff, record_attempt_handoff};
use crate::handoff::generator::{load_trusted_session_checkpoint, MergeOutcome};
use crate::handoff::schema::{
    AcceptedReceipt, CompletionAttemptEvidence, CompletionCheckpoint, CompletionPhase,
    CriterionResult, VerificationCheckpoint, COMPLETION_EVIDENCE_VERSION, MAX_EVIDENCE_NONCES,
};
use crate::models::session::Session;
use crate::models::stage::Stage;

pub(super) const NONCE: &str = "evidence_nonce_000000001";
pub(super) const LATER: &str = "2026-09-14T11:00:00Z";
pub(super) const EARLIER: &str = "2026-09-14T10:00:00Z";

pub(super) fn fixture() -> (TempDir, Session, Stage) {
    let temp = TempDir::new().unwrap();
    fs::create_dir(temp.path().join("handoffs")).unwrap();
    let mut session = Session::new();
    session.id = "session-conflict".to_string();
    let mut stage = Stage::new("conflict".to_string(), None);
    stage.id = "stage-conflict".to_string();
    stage.session = Some(session.id.clone());
    (temp, session, stage)
}

pub(super) fn attempt(nonce: &str, observed_at: &str) -> CompletionAttemptEvidence {
    CompletionAttemptEvidence {
        version: COMPLETION_EVIDENCE_VERSION,
        stage_id: "stage-conflict".to_string(),
        session_id: "session-conflict".to_string(),
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
        observed_at: observed_at.to_string(),
        attestation: None,
    }
}

fn record_all(attempts: &[CompletionAttemptEvidence]) -> CompletionCheckpoint {
    let (temp, session, stage) = fixture();
    for evidence in attempts {
        record_attempt_handoff(&session, &stage, evidence, temp.path()).unwrap();
    }
    load_trusted_session_checkpoint(&stage.id, &session.id, temp.path())
        .unwrap()
        .unwrap()
}

fn with_failure_code(mut evidence: CompletionAttemptEvidence) -> CompletionAttemptEvidence {
    evidence.external_failure_code = Some("daemon_unavailable".to_string());
    evidence
}

pub(super) fn with_other_commit(
    mut evidence: CompletionAttemptEvidence,
) -> CompletionAttemptEvidence {
    evidence.commit = "c".repeat(40);
    evidence
}

pub(super) fn assert_fails_closed(checkpoint: &CompletionCheckpoint) {
    assert!(checkpoint.conflict, "a signed conflict was lost");
    assert!(!checkpoint.is_actionable());
}

#[test]
fn signed_different_failure_code_on_one_nonce_stays_a_conflict() {
    let first = attempt(NONCE, EARLIER);
    let second = with_failure_code(attempt(NONCE, LATER));
    assert_fails_closed(&record_all(&[first, second]));
}

#[test]
fn signed_older_different_failure_code_on_one_nonce_stays_a_conflict() {
    let first = attempt(NONCE, LATER);
    let second = with_failure_code(attempt(NONCE, EARLIER));
    assert_fails_closed(&record_all(&[first, second]));
}

#[test]
fn signed_older_different_identity_on_one_nonce_stays_a_conflict() {
    let first = attempt(NONCE, LATER);
    let second = with_other_commit(attempt(NONCE, EARLIER));
    assert_fails_closed(&record_all(&[first, second]));
}

#[test]
fn a_recorded_conflict_survives_a_later_unrelated_attempt() {
    let first = attempt(NONCE, LATER);
    let second = with_other_commit(attempt(NONCE, EARLIER));
    let third = attempt("evidence_nonce_000000002", "2026-09-14T12:00:00Z");
    assert_fails_closed(&record_all(&[first, second, third]));
}

#[test]
fn repeating_the_same_signed_attempt_is_no_conflict() {
    let trusted = record_all(&[attempt(NONCE, EARLIER), attempt(NONCE, LATER)]);
    assert!(!trusted.conflict);
    assert!(trusted.is_actionable());
}

fn numbered_attempts(count: usize) -> Vec<CompletionAttemptEvidence> {
    (0..count)
        .map(|index| {
            let nonce = format!("evidence_nonce_{index:09}");
            attempt(&nonce, &format!("2026-09-14T10:{index:02}:00Z"))
        })
        .collect()
}

#[test]
fn exactly_full_nonce_capacity_is_not_exhausted() {
    let trusted = record_all(&numbered_attempts(MAX_EVIDENCE_NONCES));
    assert_eq!(trusted.observations.len(), MAX_EVIDENCE_NONCES);
    assert!(!trusted.capacity_exhausted);
    assert!(trusted.is_actionable());
}

#[test]
fn a_signed_attempt_past_capacity_exhausts_it() {
    let trusted = record_all(&numbered_attempts(MAX_EVIDENCE_NONCES + 1));
    assert_eq!(trusted.observations.len(), MAX_EVIDENCE_NONCES);
    assert!(trusted.capacity_exhausted);
    assert!(!trusted.is_actionable());
}

#[test]
fn a_signed_older_attempt_past_capacity_exhausts_it() {
    let mut attempts = numbered_attempts(MAX_EVIDENCE_NONCES);
    attempts.push(attempt("evidence_nonce_overflow01", "2026-09-14T09:00:00Z"));
    let trusted = record_all(&attempts);
    assert!(trusted.capacity_exhausted);
    assert!(!trusted.is_actionable());
}

pub(super) fn receipt(nonce: &str) -> AcceptedReceipt {
    AcceptedReceipt {
        evidence_nonce: nonce.to_string(),
        completion_nonce: "completion_nonce_0000001".to_string(),
        commit: "a".repeat(40),
        attestation: None,
    }
}

fn past_capacity() -> (TempDir, Session, Stage) {
    let fixture = fixture();
    let (temp, session, stage) = &fixture;
    for evidence in numbered_attempts(MAX_EVIDENCE_NONCES + 1) {
        record_attempt_handoff(session, stage, &evidence, temp.path()).unwrap();
    }
    fixture
}

fn document_count(temp: &TempDir) -> usize {
    fs::read_dir(temp.path().join("handoffs")).unwrap().count()
}

/// The stage completed, so the job past capacity is settled: no retry, no new
/// document, and the signed overflow stays visible with no receipt invented.
#[test]
fn the_receipt_of_the_attempt_past_capacity_settles_without_a_new_document() {
    let (temp, session, stage) = past_capacity();
    let overflow = format!("evidence_nonce_{MAX_EVIDENCE_NONCES:09}");
    let documents = document_count(&temp);

    let (_, outcome) =
        record_accepted_handoff(&session, &stage, receipt(&overflow), temp.path()).unwrap();

    assert_eq!(outcome, MergeOutcome::Unchanged);
    assert_eq!(document_count(&temp), documents);
    let trusted = load_trusted_session_checkpoint(&stage.id, &session.id, temp.path())
        .unwrap()
        .unwrap();
    assert!(trusted.capacity_exhausted);
    assert!(trusted.accepted.is_none());
}

#[test]
fn a_receipt_for_another_unknown_nonce_is_refused_past_capacity() {
    let (temp, session, stage) = past_capacity();
    let refused = record_accepted_handoff(
        &session,
        &stage,
        receipt("evidence_nonce_unrecorded"),
        temp.path(),
    );
    assert!(refused.is_err());
}

#[test]
fn a_receipt_for_an_unknown_nonce_is_refused_within_capacity() {
    let (temp, session, stage) = fixture();
    record_attempt_handoff(&session, &stage, &attempt(NONCE, EARLIER), temp.path()).unwrap();
    let refused = record_accepted_handoff(
        &session,
        &stage,
        receipt("evidence_nonce_unrecorded"),
        temp.path(),
    );
    assert!(refused.is_err());
}
