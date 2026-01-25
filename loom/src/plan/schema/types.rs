//! Plan YAML schema type definitions

use serde::{Deserialize, Serialize};

/// Type of stage for specialized handling
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StageType {
    /// Standard implementation stage
    #[default]
    Standard,
    /// Knowledge-gathering stage (e.g., knowledge-bootstrap)
    Knowledge,
}

/// Root structure of the loom metadata block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoomMetadata {
    pub loom: LoomConfig,
}

/// Main loom configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoomConfig {
    pub version: u32,
    #[serde(default)]
    pub auto_merge: Option<bool>,
    pub stages: Vec<StageDefinition>,
}

/// Stage definition from plan metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageDefinition {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub parallel_group: Option<String>,
    #[serde(default)]
    pub acceptance: Vec<String>,
    #[serde(default)]
    pub setup: Vec<String>,
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default)]
    pub auto_merge: Option<bool>,
    /// Working directory for acceptance criteria, relative to worktree root.
    /// REQUIRED field - forces explicit choice of execution directory.
    /// Use "." for worktree root, or a subdirectory like "loom".
    pub working_dir: String,
    /// Type of stage for specialized handling (e.g., knowledge vs standard)
    #[serde(default)]
    pub stage_type: StageType,
    /// Observable behaviors that must be true from user perspective
    /// Each truth is a shell command that returns exit code 0 if the behavior works
    #[serde(default)]
    pub truths: Vec<String>,
    /// Files that must exist with real implementation (not stubs)
    /// Supports glob patterns like "src/auth/*.rs"
    #[serde(default)]
    pub artifacts: Vec<String>,
    /// Critical connections between components
    #[serde(default)]
    pub wiring: Vec<WiringCheck>,
}

/// Wiring check to verify component connections
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WiringCheck {
    /// Source file path (relative to working_dir)
    pub source: String,
    /// What to check for (grep pattern)
    pub pattern: String,
    /// Human-readable description of what this verifies
    pub description: String,
}

/// Validation error with context
#[derive(Debug)]
pub struct ValidationError {
    pub message: String,
    pub stage_id: Option<String>,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(id) = &self.stage_id {
            write!(f, "Stage '{}': {}", id, self.message)
        } else {
            write!(f, "{}", self.message)
        }
    }
}

impl std::error::Error for ValidationError {}
