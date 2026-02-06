//! Tests for plan YAML schema

mod acceptance_tests;
mod auto_merge_tests;
mod code_review_tests;
mod knowledge_recommendations_tests;
mod stage_id_tests;
mod validation_tests;

use super::types::{
    LoomConfig, LoomMetadata, SandboxConfig, StageDefinition, StageSandboxConfig, StageType,
};

/// Create a minimal StageDefinition for tests with only required fields
#[cfg(test)]
pub(crate) fn make_stage(id: &str, name: &str) -> StageDefinition {
    StageDefinition {
        id: id.to_string(),
        name: name.to_string(),
        description: None,
        dependencies: vec![],
        parallel_group: None,
        acceptance: vec![],
        setup: vec![],
        files: vec![],
        auto_merge: None,
        working_dir: ".".to_string(),
        stage_type: StageType::default(),
        truths: vec![],
        artifacts: vec![],
        wiring: vec![],
        truth_checks: vec![],
        wiring_tests: vec![],
        dead_code_check: None,
        context_budget: None,
        sandbox: StageSandboxConfig::default(),
        execution_mode: None,
    }
}

/// Helper function to create a valid LoomMetadata for testing
pub(crate) fn create_valid_metadata() -> LoomMetadata {
    let mut stage1 = make_stage("stage-1", "Stage One");
    stage1.truths = vec!["test -f README.md".to_string()];

    let mut stage2 = make_stage("stage-2", "Stage Two");
    stage2.description = Some("Second stage".to_string());
    stage2.dependencies = vec!["stage-1".to_string()];
    stage2.parallel_group = Some("group-a".to_string());
    stage2.acceptance = vec!["cargo test".to_string()];
    stage2.setup = vec!["source .venv/bin/activate".to_string()];
    stage2.files = vec!["src/*.rs".to_string()];
    stage2.truths = vec!["cargo build".to_string()];

    LoomMetadata {
        loom: LoomConfig {
            version: 1,
            auto_merge: None,
            sandbox: SandboxConfig::default(),
            change_impact: None,
            stages: vec![stage1, stage2],
        },
    }
}
