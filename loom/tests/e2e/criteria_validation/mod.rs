//! Integration tests for acceptance criteria validation
//!
//! These tests verify that acceptance criteria are validated at plan init time,
//! preventing invalid criteria from being used in plans.
//!
//! ## Test Organization
//!
//! - `stage_id`: Stage ID format and security validation
//! - `acceptance`: Acceptance criteria content validation
//! - `dependencies`: Dependency graph validation
//! - `structure`: Plan structure and metadata validation

mod acceptance;
mod dependencies;
mod stage_id;
mod structure;

use loom::plan::schema::{AcceptanceCriterion, LoomConfig, LoomMetadata, StageDefinition};

/// Helper to create a minimal valid stage definition
pub(crate) fn create_valid_stage(id: &str, name: &str) -> StageDefinition {
    StageDefinition {
        id: id.to_string(),
        name: name.to_string(),
        acceptance: vec![AcceptanceCriterion::Simple("true".to_string())],
        working_dir: ".".to_string(),
        ..Default::default()
    }
}

/// Helper to create minimal valid metadata with given stages
pub(crate) fn create_metadata(stages: Vec<StageDefinition>) -> LoomMetadata {
    LoomMetadata {
        loom: LoomConfig {
            version: 1,
            stages,
            ..Default::default()
        },
    }
}
