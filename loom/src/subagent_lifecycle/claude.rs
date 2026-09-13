use super::claude_event::{
    make_idle_record, make_stop_record, verify_idle_event_id, verify_stop_event_id,
};
use super::claude_files::{
    expected_parent_transcript, plain_absolute, transcript_evidence, validate_current_binding,
    validate_parent_transcript, validate_transcript_layout,
};
use crate::commands::subagents::ledger::StartedAgentTypeIndex;
use crate::subagent_lifecycle::model::{
    LifecycleProducer, LifecycleRecord, LifecycleState, WorkerIdentity, LIFECYCLE_VERSION,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;
use std::path::{Path, PathBuf};

use crate::models::forward_receipt::is_safe_id;

#[derive(Debug, Clone, Copy)]
pub struct ClaudeEnvironment<'a> {
    pub work_dir: &'a Path,
    pub stage_id: &'a str,
    pub loom_session_id: &'a str,
    pub observed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy)]
pub struct ActiveStageSession<'a> {
    pub stage_id: &'a str,
    pub loom_session_id: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeStartEvidence {
    pub agent_type: String,
    pub started_at: Option<String>,
}

pub trait ClaudeStarts {
    fn resolve_exact(
        &self,
        stage_id: &str,
        parent_session_id: &str,
        loom_session_id: &str,
        agent_id: &str,
    ) -> Option<ClaudeStartEvidence>;
}

impl ClaudeStarts for StartedAgentTypeIndex {
    fn resolve_exact(
        &self,
        stage_id: &str,
        parent_session_id: &str,
        loom_session_id: &str,
        agent_id: &str,
    ) -> Option<ClaudeStartEvidence> {
        let row = StartedAgentTypeIndex::resolve_exact(
            self,
            stage_id,
            parent_session_id,
            loom_session_id,
            agent_id,
        )?;
        Some(ClaudeStartEvidence {
            agent_type: row.agent_type,
            started_at: row.started_at,
        })
    }
}

#[derive(Debug)]
pub enum ClaudeEvidenceError {
    Malformed(String),
    Mismatch(String),
    Unsafe(String),
    Stale(String),
    Io(std::io::Error),
}

impl fmt::Display for ClaudeEvidenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed(message) => write!(formatter, "malformed Claude evidence: {message}"),
            Self::Mismatch(message) => write!(formatter, "mismatched Claude evidence: {message}"),
            Self::Unsafe(message) => write!(formatter, "unsafe Claude evidence: {message}"),
            Self::Stale(message) => write!(formatter, "stale Claude evidence: {message}"),
            Self::Io(error) => write!(formatter, "Claude evidence I/O: {error}"),
        }
    }
}

impl std::error::Error for ClaudeEvidenceError {}

impl From<std::io::Error> for ClaudeEvidenceError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TranscriptEvidence {
    pub transcript_bytes: u64,
    pub final_record_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct IdleEvidence {
    pub(super) transcript_path: PathBuf,
    pub(super) transcript_bytes: u64,
    pub(super) final_record_sha256: String,
}

pub fn validate_subagent_stop<S: ClaudeStarts + ?Sized>(
    payload: &Value,
    env: &ClaudeEnvironment<'_>,
    starts: &S,
    active_stage_session: &ActiveStageSession<'_>,
) -> Result<LifecycleRecord, ClaudeEvidenceError> {
    validate_active(env, active_stage_session)?;
    let parent_id = required(payload, "session_id")?;
    let agent_id = required(payload, "agent_id")?;
    let agent_type = required(payload, "agent_type")?;
    validate_ids(&[parent_id, agent_id, agent_type])?;
    let worker = plain_absolute(Path::new(required(payload, "agent_transcript_path")?))?;
    let parent = plain_absolute(Path::new(required(payload, "transcript_path")?))?;
    validate_transcript_layout(&worker, &parent, parent_id, agent_id)?;
    let start = starts
        .resolve_exact(env.stage_id, parent_id, env.loom_session_id, agent_id)
        .ok_or_else(|| ClaudeEvidenceError::Mismatch("no exact SubagentStart row".into()))?;
    if start.agent_type != agent_type {
        return Err(ClaudeEvidenceError::Mismatch(
            "agent type differs from start row".into(),
        ));
    }
    validate_started_at(start.started_at.as_deref(), env.observed_at)?;
    let evidence = transcript_evidence(&worker)?;
    let identity = WorkerIdentity::ClaudeSubagent {
        stage_id: env.stage_id.to_owned(),
        loom_session_id: env.loom_session_id.to_owned(),
        parent_session_id: parent_id.to_owned(),
        agent_id: agent_id.to_owned(),
        agent_type: agent_type.to_owned(),
        transcript_path: worker,
    };
    make_stop_record(identity, evidence, env.observed_at)
}

pub fn validate_teammate_idle(
    payload: &Value,
    env: &ClaudeEnvironment<'_>,
    active_stage_session: &ActiveStageSession<'_>,
) -> Result<LifecycleRecord, ClaudeEvidenceError> {
    validate_active(env, active_stage_session)?;
    let parent_id = required(payload, "session_id")?;
    let team_name = required(payload, "team_name")?;
    let teammate_name = required(payload, "teammate_name")?;
    validate_ids(&[parent_id, team_name, teammate_name])?;
    let transcript = plain_absolute(Path::new(required(payload, "transcript_path")?))?;
    validate_parent_transcript(&transcript, parent_id)?;
    let snapshot = transcript_evidence(&transcript)?;
    let identity = WorkerIdentity::ClaudeTeammate {
        stage_id: env.stage_id.to_owned(),
        loom_session_id: env.loom_session_id.to_owned(),
        parent_session_id: parent_id.to_owned(),
        team_name: team_name.to_owned(),
        teammate_name: teammate_name.to_owned(),
    };
    make_idle_record(identity, transcript, snapshot, env.observed_at)
}

pub(crate) fn revalidate_claude_record(
    work_dir: &Path,
    record: &LifecycleRecord,
) -> Result<(), ClaudeEvidenceError> {
    match (&record.producer, &record.identity) {
        (LifecycleProducer::ClaudeSubagentStop, WorkerIdentity::ClaudeSubagent { .. }) => {
            revalidate_stop(work_dir, record)
        }
        (LifecycleProducer::ClaudeTeammateIdle, WorkerIdentity::ClaudeTeammate { .. }) => {
            revalidate_idle(work_dir, record)
        }
        _ => Err(ClaudeEvidenceError::Mismatch(
            "producer and identity disagree".into(),
        )),
    }
}

fn revalidate_stop(work_dir: &Path, record: &LifecycleRecord) -> Result<(), ClaudeEvidenceError> {
    let WorkerIdentity::ClaudeSubagent {
        stage_id,
        loom_session_id,
        parent_session_id,
        agent_id,
        agent_type,
        transcript_path,
    } = &record.identity
    else {
        return Err(ClaudeEvidenceError::Mismatch(
            "not a subagent identity".into(),
        ));
    };
    validate_stop_start(
        work_dir,
        record,
        stage_id,
        loom_session_id,
        parent_session_id,
        agent_id,
        agent_type,
    )?;
    let worker = plain_absolute(transcript_path)?;
    let parent = expected_parent_transcript(&worker, parent_session_id)?;
    plain_absolute(&parent)?;
    validate_transcript_layout(&worker, &parent, parent_session_id, agent_id)?;
    let evidence: TranscriptEvidence = evidence(record)?;
    if transcript_evidence(&worker)? != evidence {
        return Err(ClaudeEvidenceError::Stale(
            "transcript changed after stop".into(),
        ));
    }
    verify_stop_event_id(record, &evidence)
}

fn validate_stop_start(
    work_dir: &Path,
    record: &LifecycleRecord,
    stage_id: &str,
    loom_session_id: &str,
    parent_session_id: &str,
    agent_id: &str,
    agent_type: &str,
) -> Result<(), ClaudeEvidenceError> {
    if record.version != LIFECYCLE_VERSION || record.state != LifecycleState::Completed {
        return Err(ClaudeEvidenceError::Mismatch(
            "invalid stop version or state".into(),
        ));
    }
    validate_current_binding(work_dir, stage_id, loom_session_id)?;
    validate_ids(&[
        stage_id,
        loom_session_id,
        parent_session_id,
        agent_id,
        agent_type,
    ])?;
    let starts = StartedAgentTypeIndex::load(Some(work_dir));
    let start = ClaudeStarts::resolve_exact(
        &starts,
        stage_id,
        parent_session_id,
        loom_session_id,
        agent_id,
    )
    .ok_or_else(|| ClaudeEvidenceError::Mismatch("no exact SubagentStart row".into()))?;
    if start.agent_type != agent_type {
        return Err(ClaudeEvidenceError::Mismatch(
            "agent type differs from start row".into(),
        ));
    }
    validate_started_at(start.started_at.as_deref(), record.observed_at)
}

fn revalidate_idle(work_dir: &Path, record: &LifecycleRecord) -> Result<(), ClaudeEvidenceError> {
    let WorkerIdentity::ClaudeTeammate {
        stage_id,
        loom_session_id,
        parent_session_id,
        team_name,
        teammate_name,
    } = &record.identity
    else {
        return Err(ClaudeEvidenceError::Mismatch(
            "not a teammate identity".into(),
        ));
    };
    if record.version != LIFECYCLE_VERSION || record.state != LifecycleState::Idle {
        return Err(ClaudeEvidenceError::Mismatch(
            "invalid idle version or state".into(),
        ));
    }
    validate_current_binding(work_dir, stage_id, loom_session_id)?;
    validate_ids(&[
        stage_id,
        loom_session_id,
        parent_session_id,
        team_name,
        teammate_name,
    ])?;
    let evidence: IdleEvidence = evidence(record)?;
    let transcript = plain_absolute(&evidence.transcript_path)?;
    validate_parent_transcript(&transcript, parent_session_id)?;
    let snapshot = transcript_evidence(&transcript)?;
    if snapshot.transcript_bytes != evidence.transcript_bytes
        || snapshot.final_record_sha256 != evidence.final_record_sha256
    {
        return Err(ClaudeEvidenceError::Stale(
            "parent transcript changed after idle".into(),
        ));
    }
    verify_idle_event_id(record, &evidence)
}

fn validate_active(
    env: &ClaudeEnvironment<'_>,
    active: &ActiveStageSession<'_>,
) -> Result<(), ClaudeEvidenceError> {
    validate_ids(&[env.stage_id, env.loom_session_id])?;
    if env.stage_id != active.stage_id || env.loom_session_id != active.loom_session_id {
        return Err(ClaudeEvidenceError::Stale(
            "stage session is no longer active".into(),
        ));
    }
    validate_current_binding(env.work_dir, env.stage_id, env.loom_session_id)
}

fn validate_started_at(
    started: Option<&str>,
    observed: DateTime<Utc>,
) -> Result<(), ClaudeEvidenceError> {
    let started = started
        .ok_or_else(|| ClaudeEvidenceError::Malformed("start row has no timestamp".into()))?;
    let started = DateTime::parse_from_rfc3339(started)
        .map_err(malformed)?
        .with_timezone(&Utc);
    if observed < started {
        return Err(ClaudeEvidenceError::Stale(
            "event predates SubagentStart".into(),
        ));
    }
    Ok(())
}

fn required<'a>(payload: &'a Value, field: &str) -> Result<&'a str, ClaudeEvidenceError> {
    payload
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| malformed(format!("missing {field}")))
}

fn validate_ids(ids: &[&str]) -> Result<(), ClaudeEvidenceError> {
    if ids.iter().any(|id| !is_safe_id(id)) {
        return Err(ClaudeEvidenceError::Unsafe(
            "invalid identity component".into(),
        ));
    }
    Ok(())
}

fn evidence<T: for<'de> Deserialize<'de>>(
    record: &LifecycleRecord,
) -> Result<T, ClaudeEvidenceError> {
    serde_json::from_value(record.evidence.clone()).map_err(malformed)
}

fn malformed(error: impl fmt::Display) -> ClaudeEvidenceError {
    ClaudeEvidenceError::Malformed(error.to_string())
}
