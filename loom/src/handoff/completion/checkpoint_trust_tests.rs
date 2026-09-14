use super::checkpoint_tests::{
    checkpoint, checkpoint_with_attempt, evidence, receipt, NONCE, SESSION, STAGE,
};
use super::AttestationKey;
use crate::handoff::schema::{CompletionCheckpoint, CompletionPhase};

fn sign_checkpoint(checkpoint: &mut CompletionCheckpoint, key: &AttestationKey) {
    for observation in &mut checkpoint.observations {
        observation.attestation = Some(key.sign_observation(STAGE, SESSION, observation));
    }
    if let Some(latest) = &mut checkpoint.latest {
        latest.attestation = Some(key.sign_evidence(latest));
    }
    if let Some(accepted) = &mut checkpoint.accepted {
        accepted.attestation = Some(key.sign_receipt(STAGE, SESSION, accepted));
    }
}

#[test]
fn trusted_keeps_signed_state_and_recomputes_blocker() {
    let key = AttestationKey::from_bytes([11; 32]);
    let mut checkpoint = checkpoint_with_attempt();
    checkpoint
        .record_accepted(receipt(NONCE, "completion-0000000000001"))
        .unwrap();
    checkpoint.blocker = None;
    sign_checkpoint(&mut checkpoint, &key);

    let trusted = checkpoint.trusted(&key);

    assert_eq!(trusted.observations, checkpoint.observations);
    assert_eq!(trusted.latest, checkpoint.latest);
    assert_eq!(trusted.accepted, checkpoint.accepted);
    assert_eq!(
        trusted.blocker.unwrap().external_failure_code,
        "daemon_unavailable"
    );
}

#[test]
fn trusted_uses_signed_evidence_time_after_observation_timestamp_rewrite() {
    let key = AttestationKey::from_bytes([14; 32]);
    let mut checkpoint = checkpoint_with_attempt();
    sign_checkpoint(&mut checkpoint, &key);
    checkpoint.observations[0].last_observed_at = "2099-01-01T00:00:00Z".to_string();

    let trusted = checkpoint.trusted(&key);

    assert_eq!(
        trusted.last_observed_at.as_deref(),
        Some("2026-09-14T10:00:00Z")
    );
    assert_eq!(trusted.first_observed_at, trusted.last_observed_at);
}

#[test]
fn trusted_drops_invalid_attestations_and_forged_capacity() {
    let key = AttestationKey::from_bytes([12; 32]);
    let wrong_key = AttestationKey::from_bytes([13; 32]);
    let mut checkpoint = checkpoint_with_attempt();
    checkpoint.capacity_exhausted = true;
    let latest_attestation = wrong_key.sign_evidence(checkpoint.latest.as_ref().unwrap());
    checkpoint.latest.as_mut().unwrap().attestation = Some(latest_attestation);
    let observation_attestation = key.sign_observation(STAGE, SESSION, &checkpoint.observations[0]);
    checkpoint.observations[0].attestation = Some(observation_attestation);
    checkpoint.accepted = Some(receipt(NONCE, "completion-0000000000001"));
    let receipt_attestation =
        wrong_key.sign_receipt(STAGE, SESSION, checkpoint.accepted.as_ref().unwrap());
    checkpoint.accepted.as_mut().unwrap().attestation = Some(receipt_attestation);

    let trusted = checkpoint.trusted(&key);

    assert_eq!(trusted.observations.len(), 1);
    assert!(trusted.latest.is_none());
    assert!(trusted.accepted.is_none());
    assert!(trusted.blocker.is_none());
    assert!(!trusted.capacity_exhausted);

    checkpoint.observations[0].attestation = None;
    assert!(checkpoint.trusted(&key).observations.is_empty());
}

#[test]
fn merge_adopts_observation_attestation_with_new_state() {
    let mut merged = checkpoint_with_attempt();
    merged.observations[0].attestation = Some("a".repeat(64));
    let mut updated = checkpoint();
    let mut attempt = evidence(NONCE, "2026-09-14T10:01:00Z");
    attempt.phase = CompletionPhase::DaemonRejected;
    updated.record_attempt(&attempt).unwrap();
    updated.observations[0].attestation = Some("b".repeat(64));

    merged.merge_from(&updated).unwrap();

    assert_eq!(
        merged.observations[0].phase,
        CompletionPhase::DaemonRejected
    );
    assert_eq!(
        merged.observations[0].attestation.as_deref(),
        Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
    );
}
