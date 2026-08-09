use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

use crate::models::stage::StageStatus;
use crate::models::worktree::WorktreeStatus;

/// Information about a single stage's completion status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageCompletionInfo {
    /// Stage identifier
    pub id: String,
    /// Human-readable stage name
    pub name: String,
    /// Final status of the stage
    pub status: StageStatus,
    /// Duration in seconds from start to completion (None if never started)
    pub duration_secs: Option<i64>,
    /// Accumulated execution time (excludes wait/backoff time)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_secs: Option<i64>,
    /// Number of retry attempts
    #[serde(default)]
    pub retry_count: u32,
    /// Whether the stage was merged
    pub merged: bool,
    /// Dependencies of this stage
    #[serde(default)]
    pub dependencies: Vec<String>,
}

/// Summary of orchestration completion.
///
/// Sent to all status subscribers when the orchestrator finishes
/// executing all stages (successfully or with failures).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionSummary {
    /// Total orchestration duration in seconds
    pub total_duration_secs: i64,
    /// Completion info for each stage
    pub stages: Vec<StageCompletionInfo>,
    /// Number of successfully completed stages
    pub success_count: usize,
    /// Number of failed/blocked stages
    pub failure_count: usize,
    /// Path to the plan that was executed
    pub plan_path: String,
}

/// Configuration parameters for daemon mode.
///
/// These parameters control how the daemon executes stages,
/// matching the CLI flags available with `loom run`.
///
/// Note: Configuration is set when the daemon starts and cannot be
/// changed at runtime. To change configuration, first mint a proof with
/// `LOOM_ADMIN_TOKEN=<daemon-admin-token> loom stage admin-proof --daemon-stop`, then stop with
/// `LOOM_ADMIN_PROOF=<printed-proof> loom stop` and restart with `loom run` using the desired flags.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    /// Manual mode - don't auto-start stages (maps to --manual)
    pub manual_mode: bool,
    /// Maximum concurrent stages (maps to --max-parallel)
    pub max_parallel: Option<usize>,
    /// Watch mode - monitor for changes (maps to --watch)
    pub watch_mode: bool,
    /// Auto-merge completed stages (default: true, disable with --no-merge)
    pub auto_merge: bool,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            manual_mode: false,
            max_parallel: None,
            watch_mode: true,
            auto_merge: true,
        }
    }
}

/// Authorization capability required by a request.
///
/// `User` requests are unprivileged RPCs (Ping, SubscribeStatus,
/// SubscribeLogs, Unsubscribe, DisputeCriteria, CompleteStage). They use the
/// user token. CompleteStage is accepted only for the exact active
/// stage/session pair and carries no command, path, or privileged flags.
///
/// `Admin` requests are privileged host-only operations (Stop). They require
/// an action-bound, one-time operator proof minted from the mode-0600 admin
/// secret by a trusted host process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Capability {
    User,
    Admin,
}

/// Client request to daemon
#[derive(Clone, Serialize, Deserialize)]
pub enum Request {
    /// Subscribe to live status updates (Capability::User)
    SubscribeStatus { auth_token: String },
    /// Subscribe to raw log stream (Capability::User)
    SubscribeLogs { auth_token: String },
    /// Request daemon shutdown (Capability::Admin)
    Stop { auth_token: String },
    /// Disconnect cleanly (Capability::User)
    Unsubscribe { auth_token: String },
    /// Ping to check if daemon is alive (Capability::User)
    Ping { auth_token: String },
    /// File a structured dispute against a stage's acceptance criterion.
    /// The daemon writes request.md, increments dispute_count, transitions
    /// the stage to NeedsAdjudication, and replies with the assigned id.
    DisputeCriteria {
        auth_token: String,
        stage_id: String,
        criterion_index: usize,
        reason: String,
        evidence_commit: Option<String>,
        failure_output: Option<String>, // pre-truncated to 4KB by the CLI
    },
    /// Apply the narrow post-verification completion transition.
    ///
    /// Acceptance/build commands run in the calling session's host sandbox.
    /// A trusted PostToolUse hook sends this data-only request only after that
    /// exact sandboxed command reports successful verification.
    CompleteStage {
        auth_token: String,
        stage_id: String,
        session_id: String,
        nonce: String,
    },
}

impl Request {
    /// Required capability for this request variant.
    ///
    /// Anything mutating the daemon's lifecycle (`Stop`) requires
    /// [`Capability::Admin`]. Everything else is [`Capability::User`].
    pub fn required_capability(&self) -> Capability {
        match self {
            Request::Stop { .. } => Capability::Admin,
            Request::Ping { .. }
            | Request::SubscribeStatus { .. }
            | Request::SubscribeLogs { .. }
            | Request::Unsubscribe { .. }
            | Request::DisputeCriteria { .. }
            | Request::CompleteStage { .. } => Capability::User,
        }
    }

    /// Credential carried by this request.
    ///
    /// Callers must never include the returned value in logs or diagnostics.
    pub fn credential(&self) -> &str {
        match self {
            Request::SubscribeStatus { auth_token }
            | Request::SubscribeLogs { auth_token }
            | Request::Stop { auth_token }
            | Request::Unsubscribe { auth_token }
            | Request::Ping { auth_token }
            | Request::DisputeCriteria { auth_token, .. }
            | Request::CompleteStage { auth_token, .. } => auth_token,
        }
    }
}

impl fmt::Debug for Request {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Request::SubscribeStatus { .. } => {
                formatter.write_str("SubscribeStatus { auth_token: [REDACTED] }")
            }
            Request::SubscribeLogs { .. } => {
                formatter.write_str("SubscribeLogs { auth_token: [REDACTED] }")
            }
            Request::Stop { .. } => formatter.write_str("Stop { auth_token: [REDACTED] }"),
            Request::Unsubscribe { .. } => {
                formatter.write_str("Unsubscribe { auth_token: [REDACTED] }")
            }
            Request::Ping { .. } => formatter.write_str("Ping { auth_token: [REDACTED] }"),
            Request::DisputeCriteria {
                stage_id,
                criterion_index,
                evidence_commit,
                ..
            } => formatter
                .debug_struct("DisputeCriteria")
                .field("auth_token", &"[REDACTED]")
                .field("stage_id", stage_id)
                .field("criterion_index", criterion_index)
                .field("reason", &"[REDACTED]")
                .field("evidence_commit", evidence_commit)
                .field("failure_output", &"[REDACTED]")
                .finish(),
            Request::CompleteStage {
                stage_id,
                session_id,
                nonce,
                ..
            } => formatter
                .debug_struct("CompleteStage")
                .field("auth_token", &"[REDACTED]")
                .field("stage_id", stage_id)
                .field("session_id", session_id)
                .field("nonce", nonce)
                .finish(),
        }
    }
}

/// Daemon response to client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Response {
    Ok,
    Error {
        message: String,
    },
    AuthenticationFailed,
    StatusUpdate {
        stages_executing: Vec<StageInfo>,
        stages_pending: Vec<StageInfo>,
        stages_completed: Vec<StageInfo>,
        stages_blocked: Vec<StageInfo>,
    },
    /// Orchestration has completed (all stages terminal)
    OrchestrationComplete {
        summary: CompletionSummary,
    },
    LogLine {
        line: String,
    },
    Pong,
    /// Reply from a successful DisputeCriteria — carries the allocated dispute id.
    DisputeCreated {
        id: u32,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageInfo {
    pub id: String,
    pub name: String,
    pub session_pid: Option<u32>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub worktree_status: Option<WorktreeStatus>,
    /// Current status of the stage in the execution lifecycle
    pub status: StageStatus,
    /// Whether this stage's changes have been merged to the merge point
    #[serde(default)]
    pub merged: bool,
    /// IDs of stages this stage depends on
    #[serde(default)]
    pub dependencies: Vec<String>,
    /// Effective model name for this stage (explicit override or stage-type default)
    #[serde(default)]
    pub model: String,
}

pub use super::wire::{
    read_message, read_request_body, read_request_length, read_request_preface, write_message,
    RequestPreface, WireMessage,
};
