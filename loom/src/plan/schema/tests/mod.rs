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

/// Helper function to create a valid LoomMetadata for testing
pub(crate) fn create_valid_metadata() -> LoomMetadata {
    LoomMetadata {
        loom: LoomConfig {
            version: 1,
            auto_merge: None,
            sandbox: SandboxConfig::default(),
            stages: vec![
                StageDefinition {
                    id: "stage-1".to_string(),
                    name: "Stage One".to_string(),
                    description: None,
                    dependencies: vec![],
                    parallel_group: None,
                    acceptance: vec![],
                    setup: vec![],
                    files: vec![],
                    auto_merge: None,
                    working_dir: ".".to_string(),
                    stage_type: StageType::default(),
                    // Standard stages require at least one goal-backward check
                    truths: vec!["test -f README.md".to_string()],
                    artifacts: vec![],
                    wiring: vec![],
                    context_budget: None,
                    sandbox: StageSandboxConfig::default(),
                },
                StageDefinition {
                    id: "stage-2".to_string(),
                    name: "Stage Two".to_string(),
                    description: Some("Second stage".to_string()),
                    dependencies: vec!["stage-1".to_string()],
                    parallel_group: Some("group-a".to_string()),
                    acceptance: vec!["cargo test".to_string()],
                    setup: vec!["source .venv/bin/activate".to_string()],
                    files: vec!["src/*.rs".to_string()],
                    auto_merge: None,
                    working_dir: ".".to_string(),
                    stage_type: StageType::default(),
                    // Standard stages require at least one goal-backward check
                    truths: vec!["cargo build".to_string()],
                    artifacts: vec![],
                    wiring: vec![],
                    context_budget: None,
                    sandbox: StageSandboxConfig::default(),
                },
            ],
        },
    }
}
