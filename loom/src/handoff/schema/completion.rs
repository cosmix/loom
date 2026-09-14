use anyhow::{ensure, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

#[path = "completion_validate.rs"]
mod validate;

use validate::{
    blocker_fingerprint, hash_criteria, hash_field, parse_utc_timestamp, validate_attestation,
    validate_blocker, validate_commit, validate_environment_fact, validate_identity,
    validate_length, validate_nonce, validate_observation, validate_optional_timestamp,
    validate_receipt, validate_text,
};

pub const COMPLETION_EVIDENCE_VERSION: u32 = 1;
pub const MAX_EVIDENCE_NONCES: usize = 32;
pub const MAX_IDENTITY_LEN: usize = 128;
pub const MAX_COMMAND_LEN: usize = 1024;
pub const MAX_TEXT_LEN: usize = 512;
pub const MAX_CRITERIA: usize = 128;
pub const MAX_ENVIRONMENT_FACTS: usize = 16;
pub const MAX_EVIDENCE_BYTES: usize = 65536;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompletionPhase {
    ToolFailed,
    EvidenceMissing,
    VerifiedPendingAck,
    DaemonRejected,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CriterionResult {
    pub id: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub passed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvironmentFact {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationCheckpoint {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub criteria: Vec<CriterionResult>,
    #[serde(default)]
    pub environment_policy: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub environment: Vec<EnvironmentFact>,
}

impl VerificationCheckpoint {
    pub fn all_passed(&self) -> bool {
        !self.criteria.is_empty() && self.criteria.iter().all(|criterion| criterion.passed)
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.criteria.len() <= MAX_CRITERIA,
            "criteria exceeds maximum entries"
        );
        let mut ids = HashSet::new();
        for criterion in &self.criteria {
            validate_identity("criteria.id", &criterion.id)?;
            ensure!(ids.insert(&criterion.id), "criteria.id is duplicated");
        }
        if !self.environment_policy.is_empty() {
            validate_identity("environment_policy", &self.environment_policy)?;
        }
        validate_length(
            "environment",
            self.environment.len(),
            0,
            MAX_ENVIRONMENT_FACTS,
        )?;
        self.environment
            .iter()
            .try_for_each(validate_environment_fact)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompletionAttemptEvidence {
    pub version: u32,
    pub stage_id: String,
    pub session_id: String,
    pub commit: String,
    pub check_definition_hash: String,
    pub exact_command: String,
    pub evidence_nonce: String,
    pub verification: VerificationCheckpoint,
    pub phase: CompletionPhase,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_failure_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostic_first_line: Option<String>,
    pub observed_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attestation: Option<String>,
}

impl CompletionAttemptEvidence {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.version == COMPLETION_EVIDENCE_VERSION,
            "version is unsupported"
        );
        validate_identity("stage_id", &self.stage_id)?;
        validate_identity("session_id", &self.session_id)?;
        validate_commit("commit", &self.commit)?;
        validate_identity("check_definition_hash", &self.check_definition_hash)?;
        validate_text("exact_command", &self.exact_command, MAX_COMMAND_LEN, true)?;
        validate_nonce("evidence_nonce", &self.evidence_nonce)?;
        self.verification.validate()?;
        if let Some(code) = &self.external_failure_code {
            validate_identity("external_failure_code", code)?;
        }
        if let Some(diagnostic) = &self.diagnostic_first_line {
            validate_text("diagnostic_first_line", diagnostic, MAX_TEXT_LEN, false)?;
        }
        validate_attestation(self.attestation.as_deref())?;
        self.observed_at_utc()?;
        validate_length(
            "evidence",
            serde_json::to_vec(self)?.len(),
            0,
            MAX_EVIDENCE_BYTES,
        )?;
        Ok(())
    }

    pub fn observed_at_utc(&self) -> Result<DateTime<Utc>> {
        parse_utc_timestamp("observed_at", &self.observed_at)
    }

    pub fn is_actionable(&self) -> bool {
        self.validate().is_ok()
            && self.verification.all_passed()
            && matches!(
                self.phase,
                CompletionPhase::VerifiedPendingAck | CompletionPhase::DaemonRejected
            )
            && self.external_failure_code.is_some()
    }

    pub fn identity_digest(&self) -> String {
        let mut hash = Sha256::new();
        for field in [
            "loom-completion-attempt-v1",
            &self.stage_id,
            &self.session_id,
            &self.commit,
            &self.check_definition_hash,
            &self.exact_command,
            &self.evidence_nonce,
        ] {
            hash_field(&mut hash, field);
        }
        hash_criteria(&mut hash, &self.verification.criteria);
        hash_field(&mut hash, &self.verification.environment_policy);
        hex::encode(hash.finalize())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompletionBlocker {
    pub fingerprint: String,
    pub commit: String,
    pub check_definition_hash: String,
    pub external_failure_code: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

impl CompletionBlocker {
    pub fn from_attempt(evidence: &CompletionAttemptEvidence) -> Result<Self> {
        ensure!(
            evidence.is_actionable(),
            "completion evidence is not actionable"
        );
        let failure_code = evidence
            .external_failure_code
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("external_failure_code is missing"))?;
        Ok(Self {
            fingerprint: blocker_fingerprint(evidence, failure_code),
            commit: evidence.commit.clone(),
            check_definition_hash: evidence.check_definition_hash.clone(),
            external_failure_code: failure_code.to_owned(),
            summary: evidence.diagnostic_first_line.clone(),
        })
    }

    pub fn short_fingerprint(&self) -> &str {
        short_fingerprint(&self.fingerprint)
    }
}

/// Truncates a fingerprint to its first 12 characters, for compact display.
pub fn short_fingerprint(fingerprint: &str) -> &str {
    let end = fingerprint
        .char_indices()
        .nth(12)
        .map_or(fingerprint.len(), |(index, _)| index);
    &fingerprint[..end]
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcceptedReceipt {
    pub evidence_nonce: String,
    pub completion_nonce: String,
    pub commit: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attestation: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NonceObservation {
    pub evidence_nonce: String,
    pub identity_digest: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    pub phase: CompletionPhase,
    pub first_observed_at: String,
    pub last_observed_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attestation: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompletionCheckpoint {
    pub stage_id: String,
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest: Option<CompletionAttemptEvidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocker: Option<CompletionBlocker>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub observations: Vec<NonceObservation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_observed_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_observed_at: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub conflict: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub capacity_exhausted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accepted: Option<AcceptedReceipt>,
}

impl CompletionCheckpoint {
    pub fn new(stage_id: impl Into<String>, session_id: impl Into<String>) -> Self {
        Self {
            stage_id: stage_id.into(),
            session_id: session_id.into(),
            latest: None,
            blocker: None,
            observations: Vec::new(),
            first_observed_at: None,
            last_observed_at: None,
            conflict: false,
            capacity_exhausted: false,
            accepted: None,
        }
    }

    pub fn validate(&self) -> Result<()> {
        validate_identity("stage_id", &self.stage_id)?;
        validate_identity("session_id", &self.session_id)?;
        if let Some(latest) = &self.latest {
            latest.validate()?;
            ensure!(
                latest.stage_id == self.stage_id,
                "latest.stage_id does not match"
            );
            ensure!(
                latest.session_id == self.session_id,
                "latest.session_id does not match"
            );
        }
        if let Some(blocker) = &self.blocker {
            validate_blocker(blocker)?;
        }
        validate_length(
            "observations",
            self.observations.len(),
            0,
            MAX_EVIDENCE_NONCES,
        )?;
        let mut nonces = HashSet::new();
        for observation in &self.observations {
            validate_observation(observation)?;
            ensure!(
                nonces.insert(&observation.evidence_nonce),
                "observations.evidence_nonce is duplicated"
            );
        }
        validate_optional_timestamp("first_observed_at", self.first_observed_at.as_deref())?;
        validate_optional_timestamp("last_observed_at", self.last_observed_at.as_deref())?;
        if let Some(receipt) = &self.accepted {
            validate_receipt(receipt)?;
        }
        Ok(())
    }
}
