use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};

use super::{attestation_key, AttestationKey, ATTESTATION_KEY_FILE};
use crate::handoff::schema::{
    AcceptedReceipt, CompletionAttemptEvidence, CompletionPhase, CriterionResult, NonceObservation,
    VerificationCheckpoint, COMPLETION_EVIDENCE_VERSION,
};

fn evidence() -> CompletionAttemptEvidence {
    CompletionAttemptEvidence {
        version: COMPLETION_EVIDENCE_VERSION,
        stage_id: "stage-a".into(),
        session_id: "session-a".into(),
        commit: "0123456789abcdef0123456789abcdef01234567".into(),
        check_definition_hash: "checks-v1".into(),
        exact_command: "cargo test --locked".into(),
        evidence_nonce: "evidence-nonce-01".into(),
        verification: VerificationCheckpoint {
            criteria: vec![CriterionResult {
                id: "tests".into(),
                passed: true,
            }],
            environment_policy: "host".into(),
            environment: Vec::new(),
        },
        phase: CompletionPhase::VerifiedPendingAck,
        external_failure_code: Some("daemon_unavailable".into()),
        diagnostic_first_line: Some("connection refused".into()),
        observed_at: "2026-09-14T10:00:00Z".into(),
        attestation: None,
    }
}

fn observation() -> NonceObservation {
    NonceObservation {
        evidence_nonce: "evidence-nonce-01".into(),
        identity_digest: "a".repeat(64),
        fingerprint: Some("b".repeat(64)),
        phase: CompletionPhase::DaemonRejected,
        first_observed_at: "2026-09-14T10:00:00Z".into(),
        last_observed_at: "2026-09-14T10:01:00Z".into(),
        attestation: None,
    }
}

fn receipt() -> AcceptedReceipt {
    AcceptedReceipt {
        evidence_nonce: "evidence-nonce-01".into(),
        completion_nonce: "completion-nonce1".into(),
        commit: "0123456789abcdef0123456789abcdef01234567".into(),
        attestation: None,
    }
}

#[test]
fn attestation_key_creates_private_file_and_reuses_it() {
    let dir = tempfile::tempdir().unwrap();
    let first = attestation_key(dir.path()).unwrap();
    let path = dir.path().join(ATTESTATION_KEY_FILE);
    let metadata = std::fs::metadata(&path).unwrap();
    let mut signed = evidence();
    signed.attestation = Some(first.sign_evidence(&signed));
    let second = attestation_key(dir.path()).unwrap();

    assert_eq!(metadata.mode() & 0o777, 0o600);
    assert!(second.verify_evidence(&signed));
    assert_eq!(std::fs::read(path).unwrap().len(), 32);
}

#[test]
fn attestation_key_reuses_key_through_symlinked_work_dir() {
    let parent = tempfile::tempdir().unwrap();
    let real = parent.path().join("real-work");
    let linked = parent.path().join("linked-work");
    std::fs::create_dir(&real).unwrap();
    symlink(&real, &linked).unwrap();
    let first = attestation_key(&real).unwrap();
    let second = attestation_key(&linked).unwrap();
    let mut signed = evidence();
    signed.attestation = Some(first.sign_evidence(&signed));

    assert!(second.verify_evidence(&signed));
    assert!(real.join(ATTESTATION_KEY_FILE).is_file());
}

#[test]
fn attestation_key_refuses_symlinked_key_file() {
    let dir = tempfile::tempdir().unwrap();
    let target = tempfile::NamedTempFile::new().unwrap();
    symlink(target.path(), dir.path().join(ATTESTATION_KEY_FILE)).unwrap();

    assert!(attestation_key(dir.path()).is_err());
}

#[test]
fn attestation_key_refuses_group_readable_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(ATTESTATION_KEY_FILE);
    std::fs::write(&path, [7; 32]).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();

    assert!(attestation_key(dir.path()).is_err());
}

#[test]
fn every_attestation_kind_round_trips() {
    let key = AttestationKey::from_bytes([3; 32]);
    let mut evidence = evidence();
    evidence.attestation = Some(key.sign_evidence(&evidence));
    let mut observation = observation();
    observation.attestation = Some(key.sign_observation("stage-a", "session-a", &observation));
    let mut receipt = receipt();
    receipt.attestation = Some(key.sign_receipt("stage-a", "session-a", &receipt));

    assert!(key.verify_evidence(&evidence));
    assert!(key.verify_observation("stage-a", "session-a", &observation));
    assert!(key.verify_receipt("stage-a", "session-a", &receipt));
}

#[test]
fn evidence_tampering_and_different_key_fail_verification() {
    let key = AttestationKey::from_bytes([4; 32]);
    let mut original = evidence();
    original.attestation = Some(key.sign_evidence(&original));
    assert_evidence_tamper_fails(&key, &original, |value| value.version += 1);
    assert_evidence_tamper_fails(&key, &original, |value| value.stage_id.push('x'));
    assert_evidence_tamper_fails(&key, &original, |value| value.session_id.push('x'));
    assert_evidence_tamper_fails(&key, &original, |value| {
        value.commit.replace_range(..1, "f")
    });
    assert_evidence_tamper_fails(&key, &original, |value| {
        value.check_definition_hash.push('x')
    });
    assert_evidence_tamper_fails(&key, &original, |value| value.exact_command.push('x'));
    assert_evidence_tamper_fails(&key, &original, |value| value.evidence_nonce.push('x'));
    assert_evidence_tamper_fails(&key, &original, |value| {
        value.verification.criteria[0].passed = false
    });
    assert_evidence_tamper_fails(&key, &original, |value| {
        value.phase = CompletionPhase::ToolFailed
    });
    assert_evidence_tamper_fails(&key, &original, |value| value.external_failure_code = None);
    assert_evidence_tamper_fails(&key, &original, |value| value.diagnostic_first_line = None);
    assert_evidence_tamper_fails(&key, &original, |value| value.observed_at.push('x'));
    assert!(!AttestationKey::from_bytes([5; 32]).verify_evidence(&original));
}

fn assert_evidence_tamper_fails(
    key: &AttestationKey,
    original: &CompletionAttemptEvidence,
    mutate: impl FnOnce(&mut CompletionAttemptEvidence),
) {
    let mut changed = original.clone();
    mutate(&mut changed);
    assert!(!key.verify_evidence(&changed));
}

#[test]
fn observation_tampering_fails_but_timestamp_changes_do_not() {
    let key = AttestationKey::from_bytes([6; 32]);
    let mut original = observation();
    original.attestation = Some(key.sign_observation("stage-a", "session-a", &original));
    let mut changed = original.clone();
    changed.last_observed_at = "2026-09-14T11:00:00Z".into();
    assert!(key.verify_observation("stage-a", "session-a", &changed));
    changed.phase = CompletionPhase::ToolFailed;
    assert!(!key.verify_observation("stage-a", "session-a", &changed));
    changed = original.clone();
    changed.fingerprint = None;
    assert!(!key.verify_observation("stage-a", "session-a", &changed));
    changed = original.clone();
    changed.evidence_nonce.push('x');
    assert!(!key.verify_observation("stage-a", "session-a", &changed));
}

#[test]
fn receipt_tampering_fails() {
    let key = AttestationKey::from_bytes([7; 32]);
    let mut original = receipt();
    original.attestation = Some(key.sign_receipt("stage-a", "session-a", &original));
    let mut changed = original.clone();
    changed.commit.replace_range(..1, "f");
    assert!(!key.verify_receipt("stage-a", "session-a", &changed));
    changed = original.clone();
    changed.evidence_nonce.push('x');
    assert!(!key.verify_receipt("stage-a", "session-a", &changed));
}

#[test]
fn absent_attestations_fail_verification() {
    let key = AttestationKey::from_bytes([8; 32]);

    assert!(!key.verify_evidence(&evidence()));
    assert!(!key.verify_observation("stage-a", "session-a", &observation()));
    assert!(!key.verify_receipt("stage-a", "session-a", &receipt()));
}

#[test]
fn validation_accepts_only_lowercase_sha256_attestations() {
    let mut value = evidence();
    value.attestation = Some("a".repeat(64));
    assert!(value.validate().is_ok());
    value.attestation = Some("A".repeat(64));
    assert!(value.validate().is_err());
    value.attestation = Some("a".repeat(63));
    assert!(value.validate().is_err());
}
