mod collector;

pub use collector::{collect_status_data, load_all_sessions};

use serde::{Deserialize, Serialize};

// Re-export types that consumers will need
pub use crate::models::failure::FailureInfo;
pub use crate::models::stage::StageStatus;

/// Activity status derived from heartbeat and session state
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
pub enum ActivityStatus {
    /// No active session or session is idle
    #[default]
    Idle,
    /// Session is actively working (recent heartbeat)
    Working,
    /// Session encountered an error or crashed
    Error,
    /// Session may be hung (no recent heartbeat but PID alive)
    Stale,
}

impl ActivityStatus {
    /// Get Unicode icon for this activity status
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Idle => "\u{23F3}",     // hourglass
            Self::Working => "\u{1F504}", // arrows counterclockwise
            Self::Error => "\u{274C}",    // cross mark
            Self::Stale => "\u{26A0}",    // warning
        }
    }

    /// Get a short label for this status
    pub fn label(&self) -> &'static str {
        match self {
            Self::Idle => "IDLE",
            Self::Working => "WORKING",
            Self::Error => "ERROR",
            Self::Stale => "STALE",
        }
    }
}

/// Main struct aggregating all displayable status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusData {
    pub stages: Vec<StageSummary>,
    pub merge: MergeSummary,
    pub progress: ProgressSummary,
}

/// Stage display data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageSummary {
    pub id: String,
    pub name: String,
    pub status: StageStatus,
    pub dependencies: Vec<String>,
    pub context_pct: Option<f32>,
    pub elapsed_secs: Option<i64>,
    /// Accumulated execution time (excludes wait/backoff time)
    pub execution_secs: Option<i64>,
    pub base_branch: Option<String>,
    pub base_merged_from: Vec<String>,
    pub failure_info: Option<FailureInfo>,
    /// Activity status derived from heartbeat
    pub activity_status: ActivityStatus,
    /// Last tool used (from heartbeat)
    pub last_tool: Option<String>,
    /// Human-readable activity description
    pub last_activity: Option<String>,
    /// Seconds since last heartbeat (for staleness detection)
    pub staleness_secs: Option<u64>,
    /// Stage-specific context budget percentage (if set in plan)
    pub context_budget_pct: Option<f32>,
    /// Reason the stage was flagged for human review
    pub review_reason: Option<String>,
}

/// Session display data (test-only: production code uses SessionInfo in display/stages.rs)
#[cfg(test)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id: String,
    pub stage_id: Option<String>,
    pub pid: Option<u32>,
    pub context_tokens: u32,
    pub context_limit: u32,
    pub uptime_secs: i64,
    pub is_alive: bool,
}

/// Merge state summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeSummary {
    pub merged: Vec<String>,
    pub pending: Vec<String>,
    pub conflicts: Vec<String>,
}

/// Progress counts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressSummary {
    pub total: usize,
    pub completed: usize,
    pub executing: usize,
    pub pending: usize,
    pub blocked: usize,
}
