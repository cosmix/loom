use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// The type of session being executed
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum SessionType {
    /// Regular stage execution session (default)
    #[default]
    Stage,
    /// Merge conflict resolution session (post-completion merge to target branch)
    Merge,
    /// Base branch conflict resolution session (pre-stage multi-dep merge)
    BaseConflict,
}

impl std::fmt::Display for SessionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionType::Stage => write!(f, "stage"),
            SessionType::Merge => write!(f, "merge"),
            SessionType::BaseConflict => write!(f, "base_conflict"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SessionStatus {
    Spawning,
    Running,
    /// Session is paused and can be resumed.
    /// Currently set via manual session file editing or attach operations.
    Paused,
    Completed,
    Crashed,
    ContextExhausted,
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
    pub context_tokens: u32,
    pub context_limit: u32,
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
}
