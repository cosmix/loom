//! Graph node types for the execution graph

use crate::models::stage::{StageOutput, StageStatus};
use serde::{Deserialize, Serialize};

/// A node in the execution graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageNode {
    pub id: String,
    pub name: String,
    pub dependencies: Vec<String>,
    pub parallel_group: Option<String>,
    pub status: StageStatus,
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
    /// Structured outputs from this stage for dependent stages
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<StageOutput>,
    /// Whether this stage's work has been merged to the merge point (main).
    ///
    /// A stage with `status: Completed` but `merged: false` has finished its work
    /// but the changes are still on the stage branch. Dependent stages cannot be
    /// scheduled until `merged: true` because they need the merged changes as their base.
    #[serde(default)]
    pub merged: bool,
}
