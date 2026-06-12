//! Tests for the run command module.

use super::frontmatter::{extract_stage_definition, load_stages_from_work_dir};
use super::graph_loader::build_execution_graph;
use crate::fs::work_dir::WorkDir;
use crate::orchestrator::OrchestratorResult;
use crate::plan::schema::{
    LoomConfig, LoomMetadata, SandboxConfig, StageDefinition, StageSandboxConfig, StageType,
};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn create_test_plan(dir: &Path, stages: Vec<StageDefinition>) -> PathBuf {
    let metadata = LoomMetadata {
        loom: LoomConfig {
            version: 1,
            auto_merge: None,
            sandbox: SandboxConfig::default(),
            change_impact: None,
            adjudication: None,
            stages,
        },
    };

    let yaml = serde_yaml::to_string(&metadata).unwrap();
    let plan_content = format!(
        "# Test Plan\n\n## Overview\n\nTest plan\n\n<!-- loom METADATA -->\n```yaml\n{yaml}```\n<!-- END loom METADATA -->\n"
    );

    let plan_path = dir.join("test-plan.md");
    fs::write(&plan_path, plan_content).unwrap();
    plan_path
}

fn setup_work_dir_with_plan(temp_dir: &TempDir) -> (PathBuf, WorkDir) {
    let work_dir = WorkDir::new(temp_dir.path()).unwrap();
    work_dir.initialize().unwrap();

    let stage_def = StageDefinition {
        id: "test-stage".to_string(),
        name: "Test Stage".to_string(),
        description: None,
        dependencies: vec![],
        parallel_group: None,
        acceptance: vec![crate::plan::schema::AcceptanceCriterion::Simple(
            "echo ok".to_string(),
        )],
        setup: vec![],
        files: vec![],
        auto_merge: None,
        working_dir: ".".to_string(),
        stage_type: StageType::default(),
        artifacts: vec![],
        wiring: vec![],
        wiring_tests: vec![],
        dead_code_check: None,
        before_stage: vec![],
        after_stage: vec![],
        context_budget: None,
        sandbox: StageSandboxConfig::default(),
        execution_mode: None,
        bug_fix: None,
        regression_test: None,
        model: None,
        reasoning_effort: None,
        code_review: None,
        ultracode: false,
    };

    let plan_path = create_test_plan(temp_dir.path(), vec![stage_def]);

    let config_content = format!(
        "[plan]\nsource_path = \"{}\"\nplan_id = \"test-plan\"\nplan_name = \"Test Plan\"\n",
        plan_path.display()
    );
    fs::write(work_dir.root().join("config.toml"), config_content).unwrap();

    (plan_path, work_dir)
}

#[test]
fn test_extract_stage_definition_valid() {
    let content = r#"---
id: stage-1
name: Test Stage
working_dir: "."
dependencies: []
acceptance: []
setup: []
files: []
---

# Stage: Test Stage

Content here
"#;

    let result = extract_stage_definition(content);

    assert!(result.is_ok());
    let def = result.unwrap();
    assert_eq!(def.id, "stage-1");
    assert_eq!(def.name, "Test Stage");
    assert_eq!(def.dependencies.len(), 0);
}

#[test]
fn test_extract_stage_definition_with_fields() {
    let content = r#"---
id: stage-2
name: Complex Stage
description: A complex stage
working_dir: "loom"
dependencies:
  - stage-1
parallel_group: core
acceptance:
  - cargo test
setup:
  - cargo build
files:
  - src/*.rs
---

# Stage
"#;

    let result = extract_stage_definition(content);

    assert!(result.is_ok());
    let def = result.unwrap();
    assert_eq!(def.id, "stage-2");
    assert_eq!(def.description, Some("A complex stage".to_string()));
    assert_eq!(def.working_dir, "loom");
    assert_eq!(def.dependencies, vec!["stage-1".to_string()]);
    assert_eq!(def.parallel_group, Some("core".to_string()));
    assert_eq!(def.acceptance.len(), 1);
    assert_eq!(def.setup.len(), 1);
    assert_eq!(def.files.len(), 1);
}

/// Regression test for A-10: the old `StageFrontmatter` intermediate struct
/// hardcoded `stage_type`, `auto_merge`, `sandbox`, `context_budget`, and
/// `before_stage`/`after_stage` to defaults, silently dropping them on every
/// daemon restart (the loader prefers stage files over the plan). Deserializing
/// `StageDefinition` directly must preserve all of them.
#[test]
fn test_extract_stage_definition_preserves_previously_dropped_fields() {
    use crate::models::stage::StageType;

    let content = r#"---
id: kn-stage
name: Knowledge Stage
working_dir: "."
stage_type: knowledge
auto_merge: false
context_budget: 50
before_stage:
  - command: "echo pre"
after_stage:
  - command: "echo post"
sandbox:
  enabled: false
---

# Stage
"#;

    let def = extract_stage_definition(content).expect("should deserialize");

    assert_eq!(
        def.stage_type,
        StageType::Knowledge,
        "stage_type must survive"
    );
    assert_eq!(def.auto_merge, Some(false), "auto_merge must survive");
    assert_eq!(def.context_budget, Some(50), "context_budget must survive");
    assert_eq!(def.before_stage.len(), 1, "before_stage must survive");
    assert_eq!(def.after_stage.len(), 1, "after_stage must survive");
    assert_eq!(
        def.sandbox.enabled,
        Some(false),
        "sandbox override must survive"
    );
}

#[test]
fn test_extract_stage_definition_no_delimiter() {
    let content = "No frontmatter here";

    let result = extract_stage_definition(content);

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("frontmatter"));
}

#[test]
fn test_extract_stage_definition_not_closed() {
    let content = "---\nid: test\nname: Test\n\nNo closing delimiter";

    let result = extract_stage_definition(content);

    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("not properly closed"));
}

#[test]
fn test_extract_stage_definition_invalid_yaml() {
    let content = "---\ninvalid: yaml: content:\n---\n";

    let result = extract_stage_definition(content);

    assert!(result.is_err());
}

#[test]
fn test_build_execution_graph_no_config() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = WorkDir::new(temp_dir.path()).unwrap();
    work_dir.initialize().unwrap();

    let result = build_execution_graph(&work_dir);

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("No active plan"));
}

#[test]
fn test_build_execution_graph_from_config() {
    let temp_dir = TempDir::new().unwrap();
    let (_plan_path, work_dir) = setup_work_dir_with_plan(&temp_dir);

    let result = build_execution_graph(&work_dir);

    assert!(result.is_ok());
    let (_graph, _sandbox) = result.unwrap();
}

#[test]
fn test_build_execution_graph_missing_plan_file() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = WorkDir::new(temp_dir.path()).unwrap();
    work_dir.initialize().unwrap();

    let config_content =
        "[plan]\nsource_path = \"/nonexistent/plan.md\"\nplan_id = \"test\"\nplan_name = \"Test\"\n";
    fs::write(work_dir.root().join("config.toml"), config_content).unwrap();

    let result = build_execution_graph(&work_dir);

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));
}

#[test]
fn test_load_stages_from_work_dir_empty() {
    let temp_dir = TempDir::new().unwrap();
    let stages_dir = temp_dir.path().join("stages");
    fs::create_dir(&stages_dir).unwrap();

    let result = load_stages_from_work_dir(&stages_dir);

    assert!(result.is_ok());
    assert!(result.unwrap().is_empty());
}

#[test]
fn test_load_stages_from_work_dir_with_stages() {
    let temp_dir = TempDir::new().unwrap();
    let stages_dir = temp_dir.path().join("stages");
    fs::create_dir(&stages_dir).unwrap();

    let stage_content = r#"---
id: stage-1
name: Test Stage
working_dir: "."
dependencies: []
acceptance: []
setup: []
files: []
---

# Stage: Test Stage
"#;

    fs::write(stages_dir.join("0-stage-1.md"), stage_content).unwrap();

    let result = load_stages_from_work_dir(&stages_dir);

    assert!(result.is_ok());
    let stages = result.unwrap();
    assert_eq!(stages.len(), 1);
    assert_eq!(stages[0].id, "stage-1");
}

#[test]
fn test_load_stages_from_work_dir_ignores_non_markdown() {
    let temp_dir = TempDir::new().unwrap();
    let stages_dir = temp_dir.path().join("stages");
    fs::create_dir(&stages_dir).unwrap();

    fs::write(stages_dir.join("readme.txt"), "Not a stage").unwrap();

    let result = load_stages_from_work_dir(&stages_dir);

    assert!(result.is_ok());
    assert!(result.unwrap().is_empty());
}

#[test]
fn test_load_stages_from_work_dir_skips_invalid() {
    let temp_dir = TempDir::new().unwrap();
    let stages_dir = temp_dir.path().join("stages");
    fs::create_dir(&stages_dir).unwrap();

    let valid_stage = r#"---
id: valid
name: Valid
working_dir: "."
dependencies: []
---
"#;
    fs::write(stages_dir.join("valid.md"), valid_stage).unwrap();
    fs::write(stages_dir.join("invalid.md"), "Invalid content").unwrap();

    let result = load_stages_from_work_dir(&stages_dir);

    assert!(result.is_ok());
    let stages = result.unwrap();
    assert_eq!(stages.len(), 1);
    assert_eq!(stages[0].id, "valid");
}

#[test]
fn test_orchestrator_result_success() {
    let result = OrchestratorResult {
        completed_stages: vec!["stage-1".to_string(), "stage-2".to_string()],
        failed_stages: vec![],
        needs_handoff: vec![],
        total_sessions_spawned: 2,
        started_at: chrono::Utc::now(),
        completed_at: chrono::Utc::now(),
    };

    assert!(result.is_success());
}

#[test]
fn test_orchestrator_result_with_failures() {
    let result = OrchestratorResult {
        completed_stages: vec!["stage-1".to_string()],
        failed_stages: vec!["stage-2".to_string()],
        needs_handoff: vec![],
        total_sessions_spawned: 2,
        started_at: chrono::Utc::now(),
        completed_at: chrono::Utc::now(),
    };

    assert!(!result.is_success());
}

#[test]
fn test_orchestrator_result_with_handoffs() {
    let result = OrchestratorResult {
        completed_stages: vec![],
        failed_stages: vec![],
        needs_handoff: vec!["stage-1".to_string()],
        total_sessions_spawned: 1,
        started_at: chrono::Utc::now(),
        completed_at: chrono::Utc::now(),
    };

    assert!(!result.is_success());
}
