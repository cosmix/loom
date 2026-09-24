//! Tests for plan YAML schema

mod acceptance_tests;
mod auto_merge_tests;
mod build_tool_path_tests;
mod implementer_tests;
mod knowledge_recommendations_tests;
mod reasoning_effort_tests;
mod regression_test_tests;
mod security_policy_tests;
mod stage_id_tests;
mod stage_type_tests;
mod subagent_timeout_tests;
mod ultracode_tests;
mod v2_contract_lint_tests;
mod v2_lint_tests;
mod v2_tests;
mod validation_suite_tests;
mod validation_tests;

use super::types::{
    AcceptanceCriterion, ContractSpec, LoomConfig, LoomMetadata, StageDefinition, StageType,
};

/// Create a minimal StageDefinition for tests with only required fields
#[cfg(test)]
pub(crate) fn make_stage(id: &str, name: &str) -> StageDefinition {
    StageDefinition {
        id: id.to_string(),
        name: name.to_string(),
        working_dir: ".".to_string(),
        ..Default::default()
    }
}

/// Helper function to create a valid LoomMetadata for testing
pub(crate) fn create_valid_metadata() -> LoomMetadata {
    let mut stage1 = make_stage("stage-1", "Stage One");
    stage1.artifacts = vec!["README.md".to_string()];

    let mut stage2 = make_stage("stage-2", "Stage Two");
    stage2.description = Some("Second stage".to_string());
    stage2.dependencies = vec!["stage-1".to_string()];
    stage2.parallel_group = Some("group-a".to_string());
    stage2.acceptance = vec![AcceptanceCriterion::Simple("cargo test".to_string())];
    stage2.setup = vec!["source .venv/bin/activate".to_string()];
    stage2.files = vec!["src/*.rs".to_string()];

    LoomMetadata {
        loom: LoomConfig {
            version: 1,
            stages: vec![stage1, stage2],
            ..Default::default()
        },
    }
}

/// A valid `version: 2` plan: one standard stage with one contract.
pub(crate) fn create_valid_metadata_v2() -> LoomMetadata {
    let mut stage = make_stage("stage-1", "Stage One");
    stage.stage_type = Some(StageType::Standard);
    stage.acceptance = vec![AcceptanceCriterion::Simple("cargo test".to_string())];
    stage.contracts = vec![ContractSpec {
        id: "parses-v2-plan".to_string(),
        file: "tests/plan_v2.rs".to_string(),
        test: "parses_v2_plan".to_string(),
        runner: Some("cargo-test".to_string()),
        scenario: "a plan file declaring version 2 is parsed".to_string(),
        rejects: "a parser that still accepts only version 1".to_string(),
    }];

    LoomMetadata {
        loom: LoomConfig {
            version: 2,
            stages: vec![stage],
            ..Default::default()
        },
    }
}
