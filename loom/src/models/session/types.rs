use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// The type of session being executed
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum SessionType {
    /// Regular stage execution session (default)
    #[default]
    Stage,
    /// Merge conflict resolution session (post-completion merge to target branch)
    Merge,
    /// Base branch conflict resolution session (pre-stage multi-dep merge)
    /// Legacy persisted session kind from the removed base-branch merge flow.
    /// Retained only so older session files remain deserializable.
    BaseConflict,
    /// Knowledge-gathering session (runs in main repo, no worktree).
    /// Tracking key encodes the session type and stage ID.
    Knowledge,
    /// Adjudication session for one disputed criterion (acceptance
    /// criterion, wiring check, or wiring test; runs in main repo,
    /// no worktree).
    Adjudication,
    /// Contract test writer that runs in the stage worktree before the
    /// `Stage` session of a v2 stage with contracts. It is the stage's agent
    /// while it runs.
    Contract,
}

impl std::fmt::Display for SessionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionType::Stage => write!(f, "stage"),
            SessionType::Merge => write!(f, "merge"),
            SessionType::BaseConflict => write!(f, "base_conflict"),
            SessionType::Knowledge => write!(f, "knowledge"),
            SessionType::Adjudication => write!(f, "adjudication"),
            SessionType::Contract => write!(f, "contract"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SessionStatus {
    Spawning,
    Running,
    /// Session is paused and can be resumed.
    /// Currently set via manual session file editing.
    Paused,
    Completed,
    Crashed,
    ContextExhausted,
}

/// Durable reason a session entered a terminal state.
///
/// Status remains the lifecycle state machine; this value records the cause
/// without making old session records incompatible.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SessionExitReason {
    Completed,
    Crashed,
    ContextCeiling,
    Stalled,
    OperatorStop,
    CriteriaBlocked,
    Replaced,
}

impl std::fmt::Display for SessionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionStatus::Spawning => write!(f, "Spawning"),
            SessionStatus::Running => write!(f, "Running"),
            SessionStatus::Paused => write!(f, "Paused"),
            SessionStatus::Completed => write!(f, "Completed"),
            SessionStatus::Crashed => write!(f, "Crashed"),
            SessionStatus::ContextExhausted => write!(f, "ContextExhausted"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub stage_id: Option<String>,
    pub worktree_path: Option<PathBuf>,
    pub pid: Option<u32>,
    pub status: SessionStatus,
    /// Why the session first became terminal. Delayed terminal observations
    /// may fill a missing value, but never replace an existing reason.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_reason: Option<SessionExitReason>,
    /// Resident context, in absolute tokens, as of the last heartbeat that
    /// carried a reading. Absolute rather than a percentage because that is the
    /// only figure the transcript can supply without guessing the model's
    /// window.
    pub context_tokens: u32,
    /// Path to the agent's transcript, as reported by the heartbeat hook.
    /// Recorded the first time it arrives and never cleared: a later heartbeat
    /// without it means the hook did not resend the path, not that the
    /// transcript went away.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcript_path: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_active: DateTime<Utc>,
    /// The type of session (stage execution or merge resolution)
    #[serde(default)]
    pub session_type: SessionType,
    /// For merge sessions: the source branch being merged
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merge_source_branch: Option<String>,
    /// For merge sessions: the target branch to merge into
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merge_target_branch: Option<String>,

    // ----- Runtime identity -----
    /// Stable stage label used for tmux names and PID-file keys. Derived from
    /// `(stage_id, session_type)` via [`Session::derive_tracking_key`].
    /// May be empty for legacy sessions or sessions created before assignment.
    #[serde(default)]
    pub tracking_key: String,
    /// Whether the session's pane was ever seen on tmux's alternate screen (a
    /// fullscreen program). Sticky: set once by the monitor or at teardown,
    /// never cleared. A flagged session's saved history is its last
    /// alternate-screen frame rather than its scrollback.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub fullscreen_seen: bool,
}
