//! Tests for plan YAML schema

mod acceptance_tests;
mod auto_merge_tests;
mod knowledge_recommendations_tests;
mod stage_id_tests;
mod validation_tests;

use super::types::{LoomConfig, LoomMetadata, StageDefinition, StageType};

/// Helper function to create a valid LoomMetadata for testing
pub(crate) fn create_valid_metadata() -> LoomMetadata {
    LoomMetadata {
        loom: LoomConfig {
            version: 1,
            auto_merge: None,
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
                    truths: vec![],
                    artifacts: vec![],
                    wiring: vec![],
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
                    truths: vec![],
                    artifacts: vec![],
                    wiring: vec![],
                },
            ],
        },
    }
}
