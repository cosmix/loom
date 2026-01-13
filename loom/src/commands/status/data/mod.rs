mod collector;

pub use collector::collect_status_data;

use serde::{Deserialize, Serialize};

// Re-export types that consumers will need
pub use crate::models::failure::FailureInfo;
pub use crate::models::stage::StageStatus;

/// Main struct aggregating all displayable status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusData {
    pub stages: Vec<StageSummary>,
    pub sessions: Vec<SessionSummary>,
    pub merge: MergeSummary,
    pub progress: ProgressSummary,
}

/// Stage display data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageSummary {
    pub id: String,
    pub name: String,
    pub status: StageStatus,
    pub context_pct: Option<f32>,
    pub elapsed_secs: Option<i64>,
    pub base_branch: Option<String>,
    pub failure_info: Option<FailureInfo>,
}

/// Session display data
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
