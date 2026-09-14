use anyhow::{bail, Context, Result};
use std::fs::File;
use std::io::Read;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use uuid::Uuid;

use crate::fs::safe_fs::safe_create_new;
use crate::fs::safe_read::open_regular_no_follow;
use crate::handoff::schema::{
    AcceptedReceipt, CompletionAttemptEvidence, CompletionPhase, NonceObservation,
};

use crate::commands::stage::admin_hmac::{constant_time_eq, hmac_sha256};

pub const ATTESTATION_KEY_FILE: &str = "completion-attestation.key";

pub struct AttestationKey([u8; 32]);

pub fn attestation_key(work_dir: &Path) -> Result<AttestationKey> {
    let work_dir = work_dir
        .canonicalize()
        .context("failed to resolve completion attestation work directory")?;
    if let Some(key) = read_key(&work_dir)? {
        return Ok(key);
    }
    let candidate = random_key();
    match safe_create_new(&work_dir, Path::new(ATTESTATION_KEY_FILE), &candidate) {
        Ok(()) => read_key(&work_dir)?.context("created attestation key disappeared"),
        Err(error) if is_already_exists(&error) => {
            read_key(&work_dir)?.context("attestation key race winner disappeared")
        }
        Err(error) => Err(error.context("failed to create completion attestation key")),
    }
}

impl AttestationKey {
    pub fn sign_evidence(&self, evidence: &CompletionAttemptEvidence) -> String {
        let mut unsigned = evidence.clone();
        unsigned.attestation = None;
        let json = serde_json::to_vec(&unsigned)
            .expect("CompletionAttemptEvidence serialization is infallible");
        self.sign(&[b"loom-completion-evidence-attest-v1", &json])
    }

    pub fn verify_evidence(&self, evidence: &CompletionAttemptEvidence) -> bool {
        verify_mac(
            evidence.attestation.as_deref(),
            self.sign_evidence(evidence),
        )
    }

    pub fn sign_observation(
        &self,
        stage_id: &str,
        session_id: &str,
        observation: &NonceObservation,
    ) -> String {
        self.sign(&[
            b"loom-completion-observation-attest-v1",
            stage_id.as_bytes(),
            session_id.as_bytes(),
            observation.evidence_nonce.as_bytes(),
            observation.identity_digest.as_bytes(),
            observation.fingerprint.as_deref().unwrap_or("").as_bytes(),
            phase_name(observation.phase).as_bytes(),
        ])
    }

    pub fn verify_observation(
        &self,
        stage_id: &str,
        session_id: &str,
        observation: &NonceObservation,
    ) -> bool {
        verify_mac(
            observation.attestation.as_deref(),
            self.sign_observation(stage_id, session_id, observation),
        )
    }

    pub fn sign_receipt(
        &self,
        stage_id: &str,
        session_id: &str,
        receipt: &AcceptedReceipt,
    ) -> String {
        self.sign(&[
            b"loom-completion-receipt-attest-v1",
            stage_id.as_bytes(),
            session_id.as_bytes(),
            receipt.evidence_nonce.as_bytes(),
            receipt.completion_nonce.as_bytes(),
            receipt.commit.as_bytes(),
        ])
    }

    pub fn verify_receipt(
        &self,
        stage_id: &str,
        session_id: &str,
        receipt: &AcceptedReceipt,
    ) -> bool {
        verify_mac(
            receipt.attestation.as_deref(),
            self.sign_receipt(stage_id, session_id, receipt),
        )
    }

    #[cfg(test)]
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    fn sign(&self, fields: &[&[u8]]) -> String {
        let mut message = Vec::new();
        for field in fields {
            message.extend_from_slice(&(field.len() as u64).to_be_bytes());
            message.extend_from_slice(field);
        }
        hex::encode(hmac_sha256(&self.0, &message))
    }
}

fn read_key(work_dir: &Path) -> Result<Option<AttestationKey>> {
    let Some(file) = open_regular_no_follow(work_dir, ATTESTATION_KEY_FILE, libc::O_RDONLY)
        .context("failed to open completion attestation key")?
    else {
        return Ok(None);
    };
    refuse_unsafe_mode(&file)?;
    let mut bytes = Vec::with_capacity(32);
    file.take(33)
        .read_to_end(&mut bytes)
        .context("failed to read completion attestation key")?;
    let secret: [u8; 32] = bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("completion attestation key must be exactly 32 bytes"))?;
    Ok(Some(AttestationKey(secret)))
}

fn refuse_unsafe_mode(file: &File) -> Result<()> {
    let mode = file
        .metadata()
        .context("failed to inspect completion attestation key")?
        .mode();
    if mode & 0o044 != 0 {
        bail!("completion attestation key is readable by group or other");
    }
    Ok(())
}

fn random_key() -> [u8; 32] {
    let mut bytes = [0; 32];
    bytes[..16].copy_from_slice(Uuid::new_v4().as_bytes());
    bytes[16..].copy_from_slice(Uuid::new_v4().as_bytes());
    bytes
}

fn is_already_exists(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause
            .downcast_ref::<std::io::Error>()
            .is_some_and(|error| error.kind() == std::io::ErrorKind::AlreadyExists)
    })
}

fn verify_mac(attestation: Option<&str>, expected_hex: String) -> bool {
    let Some(value) = attestation.filter(|value| {
        value.len() == 64
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    }) else {
        return false;
    };
    let Ok(supplied) = hex::decode(value) else {
        return false;
    };
    let Ok(expected_bytes) = hex::decode(expected_hex) else {
        return false;
    };
    let Ok(expected) = <[u8; 32]>::try_from(expected_bytes) else {
        return false;
    };
    constant_time_eq(&expected, &supplied)
}

fn phase_name(phase: CompletionPhase) -> &'static str {
    match phase {
        CompletionPhase::ToolFailed => "tool_failed",
        CompletionPhase::EvidenceMissing => "evidence_missing",
        CompletionPhase::VerifiedPendingAck => "verified_pending_ack",
        CompletionPhase::DaemonRejected => "daemon_rejected",
    }
}
