//! Plan YAML schema type definitions

use serde::{Deserialize, Serialize};

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
