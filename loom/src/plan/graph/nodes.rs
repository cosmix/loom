//! Graph node types for the execution graph

use serde::{Deserialize, Serialize};

/// A node in the execution graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageNode {
    pub id: String,
    pub name: String,
    pub dependencies: Vec<String>,
    pub parallel_group: Option<String>,
    pub status: NodeStatus,
    /// Stage description - provides task context for the agent
    #[serde(default)]
    pub description: Option<String>,
    /// Acceptance criteria - commands to verify stage completion
    #[serde(default)]
    pub acceptance: Vec<String>,
    /// Setup commands to run before stage execution
    #[serde(default)]
    pub setup: Vec<String>,
    /// Files to modify in this stage
    #[serde(default)]
    pub files: Vec<String>,
    /// Whether to auto-merge after completion
    #[serde(default)]
    pub auto_merge: Option<bool>,
}

/// Status of a node in the execution graph.
///
/// Mirrors StageStatus but only includes states relevant to scheduling:
/// - `WaitingForDeps` - Dependencies not yet satisfied
/// - `Queued` - Dependencies satisfied, ready to execute
/// - `Executing` - Currently running
/// - `Completed` - Done (includes Verified stages)
/// - `Blocked` - Hit an error
/// - `Skipped` - Intentionally skipped (does NOT satisfy dependencies)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum NodeStatus {
    /// Waiting for upstream dependencies to complete.
    #[serde(rename = "waiting-for-deps", alias = "pending")]
    WaitingForDeps,

    /// Dependencies satisfied; queued for execution.
    #[serde(rename = "queued", alias = "ready")]
    Queued,

    /// Currently executing in a session.
    #[serde(rename = "executing")]
    Executing,

    /// Successfully completed.
    #[serde(rename = "completed")]
    Completed,

    /// Blocked due to error.
    #[serde(rename = "blocked")]
    Blocked,

    /// Intentionally skipped (does NOT satisfy dependencies).
    #[serde(rename = "skipped")]
    Skipped,
}
