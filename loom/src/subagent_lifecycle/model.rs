use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

pub const LIFECYCLE_VERSION: u16 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorkerIdentity {
    ClaudeSubagent {
        stage_id: String,
        loom_session_id: String,
        parent_session_id: String,
        agent_id: String,
        agent_type: String,
        transcript_path: PathBuf,
    },
    ClaudeTeammate {
        stage_id: String,
        loom_session_id: String,
        parent_session_id: String,
        team_name: String,
        teammate_name: String,
    },
    Codex {
        stage_id: String,
        loom_session_id: String,
        parent_session_id: String,
        forwarder_agent_id: String,
        unit_id: String,
        invocation_id: String,
        workspace_root: PathBuf,
        execution: CodexExecution,
    },
}

impl WorkerIdentity {
    pub(crate) fn stage_id(&self) -> &str {
        match self {
            Self::ClaudeSubagent { stage_id, .. }
            | Self::ClaudeTeammate { stage_id, .. }
            | Self::Codex { stage_id, .. } => stage_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum CodexExecution {
    Companion {
        job_id: String,
    },
    Direct {
        thread_id: String,
        tool_use_id: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleProducer {
    ClaudeSubagentStop,
    ClaudeTeammateIdle,
    CodexCompanion,
    CodexDirect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleState {
    TurnFinished,
    Idle,
    Running,
    Completed,
    Failed,
    Cancelled,
    Unknown,
}

/// Strict contents of [`LifecycleRecord::evidence`] for Codex producers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodexEvidence {
    pub evidence_kind: CodexEvidenceKind,
    pub requested_model: String,
    pub requested_effort: String,
    pub invocation_id: String,
    pub job_id: Option<String>,
    pub thread_id: Option<String>,
    pub turn_id: Option<String>,
    pub tool_use_id: Option<String>,
    pub terminal_at: Option<DateTime<Utc>>,
    pub outcome: CodexEvidenceOutcome,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodexEvidenceKind {
    Authorization,
    Observation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodexEvidenceOutcome {
    Running,
    Succeeded,
    Failed,
    Cancelled,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LifecycleRecord {
    pub version: u16,
    pub event_id: String,
    pub producer: LifecycleProducer,
    pub identity: WorkerIdentity,
    pub observed_at: DateTime<Utc>,
    pub state: LifecycleState,
    pub evidence: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerOutcome {
    Active,
    Succeeded,
    Failed(String),
    Cancelled(String),
    /// A worker the bounded wait found alive but making no progress within its
    /// stall budget. Never produced by the lifecycle journal -- a hung worker
    /// writes no record by definition -- only by
    /// `commands::subagents::wait::stall`, which reads process and transcript
    /// evidence directly. Matches that treat [`WorkerOutcome::Unknown`] as
    /// non-terminal must treat this the same way.
    Stalled(String),
    Unknown(String),
}
