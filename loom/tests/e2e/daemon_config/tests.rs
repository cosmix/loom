//! Remaining orchestrator configuration tests
//!
//! Tests for orchestrator creation, auto-merge, watch mode,
//! directory configuration, and stage-specific execution.

use loom::orchestrator::{Orchestrator, OrchestratorConfig};
use loom::plan::graph::ExecutionGraph;
use loom::plan::schema::SandboxConfig;
use loom::plan::schema::StageDefinition;
use serial_test::serial;
use std::time::Duration;
use tempfile::TempDir;

use super::{create_stage_def, isolate_user_config};

#[test]
#[ignore] // Requires a terminal emulator - skipped in CI
#[serial]
fn test_orchestrator_creation_with_config() {
    let temp_dir = TempDir::new().unwrap();
    let _home_env = isolate_user_config(&temp_dir);
    let work_dir = temp_dir.path();
    std::fs::create_dir_all(work_dir.join("stages")).unwrap();

    let stage_defs = vec![create_stage_def("stage-1", "Test Stage", vec![])];

    let graph = ExecutionGraph::build(stage_defs).expect("Should build execution graph");

    let config = OrchestratorConfig {
        max_parallel_sessions: 2,
        poll_interval: Duration::from_millis(100),
        manual_mode: true,
        watch_mode: false,
        work_dir: work_dir.to_path_buf(),
        repo_root: work_dir.to_path_buf(),
        status_update_interval: Duration::from_secs(5),
        auto_merge: false,
        base_branch: None,
        skills_dir: None,
        enable_skill_routing: false,
        max_skill_recommendations: 5,
        sandbox_config: SandboxConfig::default(),
        shutdown_flag: None,
        lock_identity: None,
        plan_id: None,
        ..Default::default()
    };

    let orchestrator = Orchestrator::new(config.clone(), graph);
    assert!(
        orchestrator.is_ok(),
        "Orchestrator should be created successfully"
    );

    let orchestrator = orchestrator.unwrap();
    assert_eq!(orchestrator.running_session_count(), 0);
}

/// Build a `StageDefinition` for the auto-merge cascade test below, varying
/// only the field the test asserts on; remaining fields use defaults.
fn stage_def(id: &str, name: &str, auto_merge: Option<bool>) -> StageDefinition {
    StageDefinition {
        id: id.to_string(),
        name: name.to_string(),
        auto_merge,
        working_dir: ".".to_string(),
        ..Default::default()
    }
}

#[test]
fn test_stage_definition_auto_merge_serialization() {
    let stage_with_auto_merge = stage_def("stage-override", "Override Stage", Some(true));
    assert_eq!(stage_with_auto_merge.auto_merge, Some(true));

    // An absent stage override is represented as None.
    let stage_without_override = stage_def("stage-default", "Default Stage", None);
    assert_eq!(stage_without_override.auto_merge, None);
    for stage in [stage_with_auto_merge, stage_without_override] {
        let serialized = serde_json::to_string(&stage).unwrap();
        let restored: StageDefinition = serde_json::from_str(&serialized).unwrap();
        assert_eq!(restored.auto_merge, stage.auto_merge);
        assert_eq!(restored.id, stage.id);
    }
}

#[test]
#[ignore] // Requires a terminal emulator - skipped in CI
#[serial]
fn test_orchestrator_creates_successfully() {
    // Test that the orchestrator creates with default config
    let temp_dir = TempDir::new().unwrap();
    let _home_env = isolate_user_config(&temp_dir);
    let work_dir = temp_dir.path();
    std::fs::create_dir_all(work_dir.join("stages")).unwrap();

    let stage_defs = vec![create_stage_def("stage-1", "Test", vec![])];
    let graph = ExecutionGraph::build(stage_defs).unwrap();

    let config = OrchestratorConfig {
        work_dir: work_dir.to_path_buf(),
        repo_root: work_dir.to_path_buf(),
        ..Default::default()
    };

    let orchestrator = Orchestrator::new(config, graph);
    assert!(
        orchestrator.is_ok(),
        "Orchestrator should create successfully"
    );
}

#[test]
fn test_watch_mode_default() {
    assert!(!OrchestratorConfig::default().watch_mode);
}

#[test]
fn test_work_dir_and_repo_root_defaults() {
    assert_eq!(
        OrchestratorConfig::default().work_dir,
        std::path::Path::new(".loom/work")
    );
    assert_eq!(
        OrchestratorConfig::default().repo_root,
        std::path::Path::new(".")
    );
}

/// Construct an orchestrator with auto-merge enabled.
#[test]
#[ignore] // Integration test - run with --ignored
#[serial]
fn test_orchestrator_creation_with_auto_merge_enabled() {
    let temp_dir = TempDir::new().unwrap();
    let _home_env = isolate_user_config(&temp_dir);
    let work_dir = temp_dir.path();
    std::fs::create_dir_all(work_dir.join("stages")).unwrap();

    let config = OrchestratorConfig {
        auto_merge: true,
        manual_mode: true,
        work_dir: work_dir.to_path_buf(),
        repo_root: temp_dir.path().to_path_buf(),
        ..Default::default()
    };

    let stage_defs = vec![create_stage_def("stage-1", "Test Stage", vec![])];
    let graph = ExecutionGraph::build(stage_defs).expect("Should build execution graph");
    let orchestrator = Orchestrator::new(config, graph).expect("Should create orchestrator");
    assert_eq!(orchestrator.running_session_count(), 0);
}
