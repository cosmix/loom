//! Persistent records and derived state for native workers.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub use super::ids::{
    EOF_GRACE_SECS, INTERRUPT_GRACE_SECS, MAX_ACTIVE_PER_STAGE, MAX_BRIEF_BYTES, MAX_FRAME_BYTES,
    MAX_MESSAGE_BYTES, MAX_QUEUED_MESSAGES, MAX_RECORD_BYTES, NATIVE_DEADLINE_SECS,
    PROGRESS_EVENT_INTERVAL_SECS, REAP_GRACE_SECS, REPORT_DEFAULT_BYTES,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeManifest {
    pub v: u32,
    pub stage_id: String,
    pub unit_id: String,
    pub invocation_id: String,
    pub owner_session_id: String,
    pub workspace_root: PathBuf,
    pub brief_path: PathBuf,
    pub brief_sha256: String,
    pub model: String,
    pub effort: String,
    pub deadline_secs: u64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Admitted,
    Starting,
    Running,
    Finalizing,
    Terminal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Succeeded,
    Failed,
    Cancelled,
    TimedOut,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ownership {
    Held,
    Released,
    Unverified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Delivery {
    Steer,
    NextTurn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageState {
    Queued,
    Dispatching,
    Delivered,
    Rejected,
    CancelledBeforeDelivery,
    DeliveryUnknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventRecord {
    pub seq: u64,
    pub at: DateTime<Utc>,
    pub event: NativeEvent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum NativeEvent {
    Admitted,
    BoundaryCreated,
    ThreadStarted {
        thread_id: String,
    },
    TurnStarted {
        turn_id: String,
    },
    TurnCompleted {
        turn_id: String,
        status: String,
    },
    Message {
        message_id: String,
        state: MessageState,
        turn_id: Option<String>,
        detail: Option<String>,
    },
    ServerRequestRefused {
        method: String,
    },
    CancelRequested {
        reason: String,
    },
    DeadlineExpired,
    ReportStored {
        bytes: u64,
        sha256: String,
        complete: bool,
    },
    Progress,
    Terminal {
        outcome: Outcome,
        reason: String,
    },
    OwnershipReleased {
        by: String,
    },
    OwnershipUnverified {
        detail: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoundaryIdentity {
    pub supervisor_pid: i32,
    pub supervisor_start: String,
    pub boundary_pid: i32,
    pub pid_namespace: u64,
    pub boot_id: String,
    /// The macOS boundary's identity; absent (and not serialized) on Linux,
    /// where `pid_namespace` names the boundary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mac: Option<MacIdentity>,
}

/// What names one macOS boundary: its leader started a fresh session, so the
/// leader's pid is the session id and the process group id, and every process
/// it starts inherits the invocation marker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MacIdentity {
    /// The validated invocation id (`ids::validate_invocation_id`).
    pub invocation: String,
    /// Pid of the leader, which is also its session and process group id.
    pub session: u32,
    /// The leader's start time as `process::process_start_time` renders it.
    pub leader_start: String,
    /// `process::boot_id::os_boot_id()` at launch.
    pub boot_id: String,
    /// The private invocation root `<stage cache>/native/<invocation>`.
    pub scratch_root: PathBuf,
}

impl MacIdentity {
    /// The session, group and exact marker entry the boundary's processes keep.
    pub fn mark(&self) -> crate::native_worker::boundary::quiescence_scan::BoundaryMark {
        use crate::native_worker::boundary::quiescence_scan::{BoundaryMark, BOUNDARY_MARKER_VAR};
        BoundaryMark {
            sid: self.session,
            pgid: self.session,
            marker: format!("{BOUNDARY_MARKER_VAR}={}", self.invocation),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CancelRequest {
    pub reason: String,
    pub requested_at: DateTime<Utc>,
    pub by: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlMessage {
    pub message_id: String,
    pub delivery: Delivery,
    pub text: String,
    pub sha256: String,
    pub accepted_at: DateTime<Utc>,
}

/// Folded view of one invocation, built by the index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeUnitState {
    pub manifest: NativeManifest,
    pub phase: Phase,
    pub outcome: Option<Outcome>,
    pub outcome_reason: Option<String>,
    pub ownership: Ownership,
    pub thread_id: Option<String>,
    pub cancel_requested: bool,
    pub last_progress_at: DateTime<Utc>,
    pub report: Option<ReportInfo>,
    pub messages: Vec<MessageInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportInfo {
    pub bytes: u64,
    pub sha256: String,
    pub complete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageInfo {
    pub message_id: String,
    pub state: MessageState,
    pub turn_id: Option<String>,
}

#[cfg(test)]
#[path = "model_tests.rs"]
mod tests;
