use anyhow::{ensure, Result};
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};

use super::{
    AcceptedReceipt, CompletionAttemptEvidence, CompletionBlocker, CriterionResult,
    EnvironmentFact, NonceObservation, MAX_IDENTITY_LEN, MAX_TEXT_LEN,
};

pub(super) fn validate_identity(field: &str, value: &str) -> Result<()> {
    validate_length(field, value.chars().count(), 1, MAX_IDENTITY_LEN)?;
    validate_ascii(field, value, |byte| {
        byte.is_ascii_alphanumeric() || b"._:-".contains(&byte)
    })
}

pub(super) fn validate_nonce(field: &str, value: &str) -> Result<()> {
    validate_length(field, value.len(), 16, 128)?;
    validate_ascii(field, value, |byte| {
        byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-'
    })
}

pub(super) fn validate_commit(field: &str, value: &str) -> Result<()> {
    validate_length(field, value.len(), 7, 64)?;
    validate_ascii(field, value, is_lower_hex)
}

fn validate_digest(field: &str, value: &str) -> Result<()> {
    validate_length(field, value.len(), 64, 64)?;
    validate_ascii(field, value, is_lower_hex)
}

pub(super) fn validate_text(field: &str, value: &str, max: usize, required: bool) -> Result<()> {
    validate_length(field, value.chars().count(), usize::from(required), max)?;
    ensure!(
        !value.chars().any(char::is_control),
        "{field} has control characters"
    );
    Ok(())
}

pub(super) fn parse_utc_timestamp(field: &str, value: &str) -> Result<DateTime<Utc>> {
    let parsed = DateTime::parse_from_rfc3339(value)
        .map_err(|_| anyhow::anyhow!("{field} is not RFC3339"))?;
    ensure!(parsed.offset().local_minus_utc() == 0, "{field} is not UTC");
    Ok(parsed.with_timezone(&Utc))
}

pub(super) fn validate_optional_timestamp(field: &str, value: Option<&str>) -> Result<()> {
    value.map_or(Ok(()), |value| {
        parse_utc_timestamp(field, value).map(|_| ())
    })
}

pub(super) fn validate_environment_fact(fact: &EnvironmentFact) -> Result<()> {
    validate_length("environment.name", fact.name.len(), 1, 64)?;
    validate_ascii("environment.name", &fact.name, |byte| {
        byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_'
    })?;
    validate_text("environment.value", &fact.value, 256, false)
}

pub(super) fn validate_blocker(blocker: &CompletionBlocker) -> Result<()> {
    validate_digest("blocker.fingerprint", &blocker.fingerprint)?;
    validate_commit("blocker.commit", &blocker.commit)?;
    validate_identity(
        "blocker.check_definition_hash",
        &blocker.check_definition_hash,
    )?;
    validate_identity(
        "blocker.external_failure_code",
        &blocker.external_failure_code,
    )?;
    if let Some(summary) = &blocker.summary {
        validate_text("blocker.summary", summary, MAX_TEXT_LEN, false)?;
    }
    Ok(())
}

pub(super) fn validate_receipt(receipt: &AcceptedReceipt) -> Result<()> {
    validate_nonce("accepted.evidence_nonce", &receipt.evidence_nonce)?;
    validate_nonce("accepted.completion_nonce", &receipt.completion_nonce)?;
    validate_commit("accepted.commit", &receipt.commit)?;
    validate_attestation(receipt.attestation.as_deref())
}

pub(super) fn validate_observation(observation: &NonceObservation) -> Result<()> {
    validate_nonce("observations.evidence_nonce", &observation.evidence_nonce)?;
    validate_digest("observations.identity_digest", &observation.identity_digest)?;
    if let Some(fingerprint) = &observation.fingerprint {
        validate_digest("observations.fingerprint", fingerprint)?;
    }
    parse_utc_timestamp(
        "observations.first_observed_at",
        &observation.first_observed_at,
    )?;
    parse_utc_timestamp(
        "observations.last_observed_at",
        &observation.last_observed_at,
    )?;
    validate_attestation(observation.attestation.as_deref())
}

pub(super) fn validate_attestation(value: Option<&str>) -> Result<()> {
    value.map_or(Ok(()), |value| validate_digest("attestation", value))
}

fn validate_ascii(field: &str, value: &str, allowed: impl FnMut(u8) -> bool) -> Result<()> {
    ensure!(value.bytes().all(allowed), "{field} has invalid characters");
    Ok(())
}

pub(super) fn validate_length(field: &str, len: usize, min: usize, max: usize) -> Result<()> {
    ensure!((min..=max).contains(&len), "{field} has invalid length");
    Ok(())
}

fn is_lower_hex(byte: u8) -> bool {
    byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)
}

pub(super) fn hash_field(hash: &mut Sha256, field: &str) {
    hash.update((field.len() as u64).to_be_bytes());
    hash.update(field.as_bytes());
}

pub(super) fn hash_criteria(hash: &mut Sha256, criteria: &[CriterionResult]) {
    let mut criteria: Vec<_> = criteria.iter().collect();
    criteria.sort_by(|left, right| left.id.cmp(&right.id));
    for criterion in criteria {
        let encoded = format!("{}={}", criterion.id, u8::from(criterion.passed));
        hash_field(hash, &encoded);
    }
}

pub(super) fn blocker_fingerprint(
    evidence: &CompletionAttemptEvidence,
    failure_code: &str,
) -> String {
    let mut hash = Sha256::new();
    for field in [
        "loom-completion-blocker-v1",
        &evidence.stage_id,
        &evidence.session_id,
        &evidence.commit,
        &evidence.check_definition_hash,
        &evidence.exact_command,
    ] {
        hash_field(&mut hash, field);
    }
    hash_criteria(&mut hash, &evidence.verification.criteria);
    hash_field(&mut hash, &evidence.verification.environment_policy);
    hash_field(&mut hash, failure_code);
    hex::encode(hash.finalize())
}
