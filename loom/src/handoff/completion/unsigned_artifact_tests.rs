//! Unsigned artifacts beside signed completion evidence, exercised through the
//! real writer and the public trusted loader in both filename orders.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use tempfile::TempDir;

use super::conflict_tests::{
    assert_fails_closed, attempt, fixture, receipt, with_other_commit, EARLIER, LATER, NONCE,
};
use super::{
    attestation_key, record_accepted_handoff, record_attempt_handoff, ATTESTATION_KEY_FILE,
};
use crate::handoff::generator::{
    find_latest_session_handoff, load_trusted_session_checkpoint, merge_session_handoff,
    HandoffContent,
};
use crate::handoff::schema::{
    CompletionCheckpoint, HandoffOrigin, HandoffV2, ParsedHandoff, MAX_EVIDENCE_NONCES,
};
use crate::models::session::Session;
use crate::models::stage::Stage;

const NEWER: &str = "2026-09-20T00:00:00Z";
const OTHER_NONCE: &str = "evidence_nonce_000000002";

fn unsigned_attempts(count: usize) -> CompletionCheckpoint {
    let mut checkpoint = CompletionCheckpoint::new("stage-conflict", "session-conflict");
    for index in 0..count {
        let nonce = format!("evidence_nonce_forged{index:04}");
        checkpoint.record_attempt(&attempt(&nonce, NEWER)).unwrap();
    }
    checkpoint
}

fn unsigned_conflict_flag() -> CompletionCheckpoint {
    let mut checkpoint = unsigned_attempts(1);
    checkpoint.conflict = true;
    checkpoint
}

fn unsigned_capacity_fillers() -> CompletionCheckpoint {
    let mut checkpoint = unsigned_attempts(MAX_EVIDENCE_NONCES);
    checkpoint.capacity_exhausted = true;
    checkpoint
}

fn unsigned_artifacts() -> [(&'static str, CompletionCheckpoint); 3] {
    [
        ("newer latest", unsigned_attempts(1)),
        ("conflict flag", unsigned_conflict_flag()),
        ("nonce fillers", unsigned_capacity_fillers()),
    ]
}

fn write_artifact(temp: &TempDir, checkpoint: CompletionCheckpoint) {
    let dir = temp.path().join("handoffs");
    let number = fs::read_dir(&dir).unwrap().count() + 1;
    let handoff = HandoffV2::new("session-conflict", "stage-conflict")
        .with_origin(HandoffOrigin::CompletionEvidence)
        .with_completion_checkpoint(Some(checkpoint));
    write_handoff(
        dir.join(format!("stage-conflict-handoff-{number:03}.md")),
        &handoff,
    );
}

fn write_handoff(path: PathBuf, handoff: &HandoffV2) {
    fs::write(path, format!("---\n{}---\n", handoff.to_yaml().unwrap())).unwrap();
}

fn newest_document(temp: &TempDir) -> (PathBuf, HandoffV2) {
    let path = find_latest_session_handoff("stage-conflict", "session-conflict", temp.path())
        .unwrap()
        .unwrap();
    let markdown = fs::read_to_string(&path).unwrap();
    let handoff = ParsedHandoff::parse(&markdown).as_v2().unwrap().clone();
    (path, handoff)
}

fn trusted(temp: &TempDir) -> CompletionCheckpoint {
    load_trusted_session_checkpoint("stage-conflict", "session-conflict", temp.path())
        .unwrap()
        .expect("the signed attempt is missing from the trusted checkpoint")
}

fn assert_attempt_actionable(temp: &TempDir, observed_at: &str, artifact: &str) {
    let trusted = trusted(temp);
    let latest = trusted.latest.as_ref().expect(artifact);
    assert_eq!(latest.evidence_nonce, NONCE, "{artifact}");
    assert_eq!(latest.observed_at, observed_at, "{artifact}");
    assert_eq!(trusted.observations.len(), 1, "{artifact}");
    assert!(!trusted.conflict, "{artifact}");
    assert!(!trusted.capacity_exhausted, "{artifact}");
    assert!(trusted.is_actionable(), "{artifact}");
}

fn record(temp: &TempDir, session: &Session, stage: &Stage, nonce: &str, observed_at: &str) {
    record_attempt_handoff(session, stage, &attempt(nonce, observed_at), temp.path()).unwrap();
}

#[test]
fn a_signed_attempt_recorded_after_unsigned_evidence_is_actionable() {
    for (name, artifact) in unsigned_artifacts() {
        let (temp, session, stage) = fixture();
        write_artifact(&temp, artifact);

        record(&temp, &session, &stage, NONCE, EARLIER);

        assert_attempt_actionable(&temp, EARLIER, name);
    }
}

#[test]
fn a_signed_attempt_recorded_before_unsigned_evidence_stays_actionable() {
    for (name, artifact) in unsigned_artifacts() {
        let (temp, session, stage) = fixture();
        record(&temp, &session, &stage, NONCE, EARLIER);
        write_artifact(&temp, artifact);
        assert_attempt_actionable(&temp, EARLIER, name);

        record(&temp, &session, &stage, NONCE, LATER);

        assert_attempt_actionable(&temp, LATER, name);
    }
}

#[test]
fn an_accepted_receipt_survives_unsigned_evidence_in_either_order() {
    for (name, artifact) in unsigned_artifacts() {
        for artifact_first in [true, false] {
            let (temp, session, stage) = fixture();
            if artifact_first {
                write_artifact(&temp, artifact.clone());
            }
            record(&temp, &session, &stage, NONCE, EARLIER);
            if !artifact_first {
                write_artifact(&temp, artifact.clone());
            }

            record_accepted_handoff(&session, &stage, receipt(NONCE), temp.path()).unwrap();

            let trusted = trusted(&temp);
            let accepted = trusted.accepted.as_ref().expect(name);
            assert_eq!(accepted.evidence_nonce, NONCE, "{name}");
            assert!(accepted.attestation.is_some(), "{name}");
            assert_eq!(trusted.latest.unwrap().evidence_nonce, NONCE, "{name}");
            assert!(!trusted.conflict, "{name}");
            assert!(!trusted.capacity_exhausted, "{name}");
        }
    }
}

#[test]
fn a_signed_identity_conflict_beside_unsigned_evidence_stays_fail_closed() {
    let conflicting = with_other_commit(attempt(NONCE, LATER));
    for (_, artifact) in unsigned_artifacts() {
        let (temp, session, stage) = fixture();
        write_artifact(&temp, artifact.clone());
        record(&temp, &session, &stage, NONCE, EARLIER);
        record_attempt_handoff(&session, &stage, &conflicting, temp.path()).unwrap();
        assert_fails_closed(&trusted(&temp));

        let (temp, session, stage) = fixture();
        record(&temp, &session, &stage, NONCE, EARLIER);
        write_artifact(&temp, artifact);
        record_attempt_handoff(&session, &stage, &conflicting, temp.path()).unwrap();
        assert_fails_closed(&trusted(&temp));
    }
}

#[test]
fn the_written_aggregate_holds_only_signed_components() {
    let (temp, session, stage) = fixture();
    let mut forged = unsigned_capacity_fillers();
    forged.accepted = Some(receipt("evidence_nonce_forged0000"));
    write_artifact(&temp, forged);

    record(&temp, &session, &stage, NONCE, EARLIER);

    let key = attestation_key(temp.path()).unwrap();
    let written = newest_document(&temp).1.completion_checkpoint.unwrap();
    assert_eq!(written.observations.len(), 1);
    assert!(key.verify_observation(&stage.id, &session.id, &written.observations[0]));
    assert!(key.verify_evidence(written.latest.as_ref().unwrap()));
    assert!(written.accepted.is_none());
    assert!(!written.conflict);
    assert!(!written.capacity_exhausted);
}

#[test]
fn unsigned_components_edited_into_a_signed_artifact_are_not_carried() {
    let (temp, session, stage) = fixture();
    record(&temp, &session, &stage, NONCE, EARLIER);
    let (path, mut handoff) = newest_document(&temp);
    let signed = handoff.completion_checkpoint.as_mut().unwrap();
    let fillers = unsigned_attempts(MAX_EVIDENCE_NONCES - 1);
    signed.observations.extend(fillers.observations);
    signed.conflict = true;
    signed.capacity_exhausted = true;
    write_handoff(path, &handoff);

    record(&temp, &session, &stage, OTHER_NONCE, LATER);

    let key = attestation_key(temp.path()).unwrap();
    let written = newest_document(&temp).1.completion_checkpoint.unwrap();
    assert_eq!(written.observations.len(), 2);
    assert!(written
        .observations
        .iter()
        .all(|observation| key.verify_observation(&stage.id, &session.id, observation)));
    assert!(!written.conflict);
    assert!(!written.capacity_exhausted);
    assert!(trusted(&temp).is_actionable());
}

#[test]
fn a_handoff_without_evidence_beside_unsigned_evidence_keeps_the_attempt_actionable() {
    for (name, artifact) in unsigned_artifacts() {
        for artifact_first in [true, false] {
            let (temp, session, stage) = fixture();
            if artifact_first {
                write_artifact(&temp, artifact.clone());
            }
            record(&temp, &session, &stage, NONCE, EARLIER);
            if !artifact_first {
                write_artifact(&temp, artifact.clone());
            }

            let content = HandoffContent::new(session.id.clone(), stage.id.clone());
            merge_session_handoff(&session, &stage, None, content, temp.path()).unwrap();

            assert_attempt_actionable(&temp, EARLIER, name);
        }
    }
}

/// Session hooks write handoffs directly from where the sandbox denies the key.
#[test]
fn a_handoff_without_evidence_needs_no_attestation_key() {
    let (temp, session, stage) = fixture();
    record(&temp, &session, &stage, NONCE, EARLIER);
    let key_path = temp.path().join(ATTESTATION_KEY_FILE);
    fs::set_permissions(&key_path, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(attestation_key(temp.path()).is_err());

    let content = HandoffContent::new(session.id.clone(), stage.id.clone())
        .with_completed_work(vec!["kept".to_string()]);
    merge_session_handoff(&session, &stage, None, content, temp.path()).unwrap();

    let written = newest_document(&temp).1;
    assert_eq!(written.completed_tasks[0].description, "kept");
    assert_eq!(written.completion_checkpoint.unwrap().observations.len(), 1);
}
