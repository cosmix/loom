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

use loom::plan::schema::{
    AcceptanceCriterion, Implementers, LoomConfig, LoomMetadata, StageDefinition,
};

/// Helper to create a minimal valid stage definition
pub(crate) fn create_valid_stage(id: &str, name: &str) -> StageDefinition {
    StageDefinition {
        id: id.to_string(),
        name: name.to_string(),
        description: None,
        dependencies: vec![],
        parallel_group: None,
        acceptance: vec![AcceptanceCriterion::Simple("true".to_string())],
        setup: vec![],
        files: vec![],
        auto_merge: None,
        working_dir: ".".to_string(),
        sandbox: Default::default(),
        stage_type: None,
        artifacts: vec![],
        wiring: vec![],
        wiring_tests: vec![],
        dead_code_check: None,
        before_stage: vec![],
        after_stage: vec![],
        context_ceiling_tokens: None,
        removed_context_budget: None,
        plan_overview: None,
        execution_mode: None,
        bug_fix: None,
        regression_test: None,
        model: None,
        reasoning_effort: None,
        code_review: None,
        ultracode: false,
        implementers: Implementers::default(),
        subagent_timeout_secs: None,
    }
}

/// Helper to create minimal valid metadata with given stages
pub(crate) fn create_metadata(stages: Vec<StageDefinition>) -> LoomMetadata {
    LoomMetadata {
        loom: LoomConfig {
            version: 1,
            sandbox: Default::default(),
            auto_merge: None,
            change_impact: None,
            adjudication: None,
            context_ceiling_tokens: None,
            subagent_ceiling_tokens: None,
            stages,
        },
    }
}
