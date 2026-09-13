use super::claude::{ClaudeEvidenceError, IdleEvidence, TranscriptEvidence};
use super::model::{
    LifecycleProducer, LifecycleRecord, LifecycleState, WorkerIdentity, LIFECYCLE_VERSION,
};
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use std::path::Path;

pub(super) fn make_stop_record(
    identity: WorkerIdentity,
    evidence: TranscriptEvidence,
    observed_at: DateTime<Utc>,
) -> Result<LifecycleRecord, ClaudeEvidenceError> {
    let mut record = LifecycleRecord {
        version: LIFECYCLE_VERSION,
        event_id: String::new(),
        producer: LifecycleProducer::ClaudeSubagentStop,
        identity,
        observed_at,
        state: LifecycleState::Completed,
        evidence: serde_json::to_value(&evidence).map_err(malformed)?,
    };
    record.event_id = stop_event_id(&record.identity, &evidence)?;
    Ok(record)
}

pub(super) fn make_idle_record(
    identity: WorkerIdentity,
    transcript_path: std::path::PathBuf,
    snapshot: TranscriptEvidence,
    observed_at: DateTime<Utc>,
) -> Result<LifecycleRecord, ClaudeEvidenceError> {
    let evidence = IdleEvidence {
        transcript_path,
        transcript_bytes: snapshot.transcript_bytes,
        final_record_sha256: snapshot.final_record_sha256,
    };
    let mut record = LifecycleRecord {
        version: LIFECYCLE_VERSION,
        event_id: String::new(),
        producer: LifecycleProducer::ClaudeTeammateIdle,
        identity,
        observed_at,
        state: LifecycleState::Idle,
        evidence: serde_json::to_value(&evidence).map_err(malformed)?,
    };
    record.event_id = idle_event_id(&record.identity, &evidence, observed_at)?;
    Ok(record)
}

pub(super) fn verify_stop_event_id(
    record: &LifecycleRecord,
    evidence: &TranscriptEvidence,
) -> Result<(), ClaudeEvidenceError> {
    if stop_event_id(&record.identity, evidence)? != record.event_id {
        return Err(ClaudeEvidenceError::Mismatch(
            "stop event id digest differs".into(),
        ));
    }
    Ok(())
}

pub(super) fn verify_idle_event_id(
    record: &LifecycleRecord,
    evidence: &IdleEvidence,
) -> Result<(), ClaudeEvidenceError> {
    if idle_event_id(&record.identity, evidence, record.observed_at)? != record.event_id {
        return Err(ClaudeEvidenceError::Mismatch(
            "idle event id digest differs".into(),
        ));
    }
    Ok(())
}

pub(super) fn stop_event_id(
    identity: &WorkerIdentity,
    evidence: &TranscriptEvidence,
) -> Result<String, ClaudeEvidenceError> {
    let WorkerIdentity::ClaudeSubagent {
        stage_id,
        loom_session_id,
        parent_session_id,
        agent_id,
        agent_type,
        transcript_path,
    } = identity
    else {
        return Err(ClaudeEvidenceError::Mismatch("not a stop identity".into()));
    };
    let transcript_bytes = evidence.transcript_bytes.to_string();
    canonical_digest(
        "loom.lifecycle.claude_subagent_stop.v1",
        &[
            stage_id,
            loom_session_id,
            parent_session_id,
            agent_id,
            agent_type,
            path_text(transcript_path)?,
            &transcript_bytes,
            &evidence.final_record_sha256,
        ],
    )
}

pub(super) fn idle_event_id(
    identity: &WorkerIdentity,
    evidence: &IdleEvidence,
    observed: DateTime<Utc>,
) -> Result<String, ClaudeEvidenceError> {
    let WorkerIdentity::ClaudeTeammate {
        stage_id,
        loom_session_id,
        parent_session_id,
        team_name,
        teammate_name,
    } = identity
    else {
        return Err(ClaudeEvidenceError::Mismatch("not an idle identity".into()));
    };
    let transcript_bytes = evidence.transcript_bytes.to_string();
    let observed = observed.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    canonical_digest(
        "loom.lifecycle.claude_teammate_idle.v1",
        &[
            stage_id,
            loom_session_id,
            parent_session_id,
            team_name,
            teammate_name,
            path_text(&evidence.transcript_path)?,
            &transcript_bytes,
            &evidence.final_record_sha256,
            &observed,
        ],
    )
}

fn canonical_digest(prefix: &str, fields: &[&str]) -> Result<String, ClaudeEvidenceError> {
    if fields.iter().any(|value| value.as_bytes().contains(&0)) {
        return Err(ClaudeEvidenceError::Unsafe("NUL in event identity".into()));
    }
    let mut bytes = prefix.as_bytes().to_vec();
    for field in fields {
        bytes.push(0);
        bytes.extend_from_slice(field.as_bytes());
    }
    Ok(digest(&bytes))
}

pub(super) fn digest(bytes: &[u8]) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(bytes)))
}

fn path_text(path: &Path) -> Result<&str, ClaudeEvidenceError> {
    path.to_str()
        .ok_or_else(|| ClaudeEvidenceError::Unsafe("non-UTF-8 transcript path".into()))
}

fn malformed(error: impl std::fmt::Display) -> ClaudeEvidenceError {
    ClaudeEvidenceError::Malformed(error.to_string())
}
