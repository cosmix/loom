//! Remaining orchestrator configuration tests
//!
//! Tests for orchestrator creation, auto-merge, backend types, watch mode,
//! directory configuration, and stage-specific execution.

use loom::models::stage::Stage;
use loom::orchestrator::terminal::BackendType;
use loom::orchestrator::{Orchestrator, OrchestratorConfig};
use loom::plan::graph::ExecutionGraph;
use loom::plan::schema::StageDefinition;
use std::time::Duration;
use tempfile::TempDir;

use super::create_stage_def;

#[test]
#[ignore] // Requires a terminal emulator - skipped in CI
fn test_orchestrator_creation_with_config() {
    let temp_dir = TempDir::new().unwrap();
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
        backend_type: BackendType::Native,
        auto_merge: false,
        base_branch: None,
        skills_dir: None,
        enable_skill_routing: false,
        max_skill_recommendations: 5,
    };

    let orchestrator = Orchestrator::new(config.clone(), graph);
    assert!(
        orchestrator.is_ok(),
        "Orchestrator should be created successfully"
    );

    let orchestrator = orchestrator.unwrap();
    assert_eq!(orchestrator.running_session_count(), 0);
}

#[test]
fn test_auto_merge_config_cascade() {
    // Test that auto_merge can be configured at different levels

    // Stage-level auto_merge overrides plan-level
    let stage_with_auto_merge = StageDefinition {
        id: "stage-override".to_string(),
        name: "Override Stage".to_string(),
        description: None,
        dependencies: vec![],
        parallel_group: None,
        acceptance: vec![],
        setup: vec![],
        files: vec![],
        auto_merge: Some(true), // Stage-level override
        working_dir: ".".to_string(),
        sandbox: Default::default(),
        stage_type: loom::plan::schema::StageType::default(),
        truths: vec![],
        artifacts: vec![],
        wiring: vec![],
        context_budget: None,
    };

    assert_eq!(stage_with_auto_merge.auto_merge, Some(true));

    // Stage without override uses plan-level (represented as None)
    let stage_without_override = StageDefinition {
        id: "stage-default".to_string(),
        name: "Default Stage".to_string(),
        description: None,
        dependencies: vec![],
        parallel_group: None,
        acceptance: vec![],
        setup: vec![],
        files: vec![],
        auto_merge: None, // Uses plan default
        working_dir: ".".to_string(),
        sandbox: Default::default(),
        stage_type: loom::plan::schema::StageType::default(),
        truths: vec![],
        artifacts: vec![],
        wiring: vec![],
        context_budget: None,
    };

    assert_eq!(stage_without_override.auto_merge, None);
}

#[test]
#[ignore] // Requires a terminal emulator - skipped in CI
fn test_backend_type_native() {
    // Test that Native backend type is the default and works correctly
    let config = OrchestratorConfig::default();
    assert_eq!(config.backend_type, BackendType::Native);

    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();
    std::fs::create_dir_all(work_dir.join("stages")).unwrap();

    let stage_defs = vec![create_stage_def("stage-1", "Test", vec![])];
    let graph = ExecutionGraph::build(stage_defs).unwrap();

    let config = OrchestratorConfig {
        backend_type: BackendType::Native,
        work_dir: work_dir.to_path_buf(),
        repo_root: work_dir.to_path_buf(),
        ..Default::default()
    };

    let orchestrator = Orchestrator::new(config, graph);
    assert!(
        orchestrator.is_ok(),
        "Native backend should create successfully"
    );
}

#[test]
fn test_watch_mode_configuration() {
    // Test watch mode flag
    let config_watch = OrchestratorConfig {
        watch_mode: true,
        ..Default::default()
    };
    assert!(config_watch.watch_mode);

    let config_no_watch = OrchestratorConfig {
        watch_mode: false,
        ..Default::default()
    };
    assert!(!config_no_watch.watch_mode);
}

#[test]
fn test_work_dir_and_repo_root_configuration() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path().join(".work");
    let repo_root = temp_dir.path().to_path_buf();

    std::fs::create_dir_all(&work_dir).unwrap();

    let config = OrchestratorConfig {
        work_dir: work_dir.clone(),
        repo_root: repo_root.clone(),
        ..Default::default()
    };

    assert_eq!(config.work_dir, work_dir);
    assert_eq!(config.repo_root, repo_root);
}

/// Integration test: Verify auto-merge configuration cascade
///
/// Tests that `is_auto_merge_enabled()` correctly prioritizes:
/// 1. Stage-level `auto_merge` setting (highest priority)
/// 2. Plan-level `auto_merge` setting
/// 3. Orchestrator config `auto_merge` setting (lowest priority)
#[test]
#[ignore] // Integration test - run with --ignored
fn test_daemon_respects_auto_merge() {
    use loom::orchestrator::auto_merge::is_auto_merge_enabled;

    // Create a test stage
    let mut stage = Stage::new("Test Stage".to_string(), None);
    stage.id = "stage-1".to_string();

    // Test 1: Stage override = None, Plan = None -> uses orchestrator config
    stage.auto_merge = None;
    assert!(
        is_auto_merge_enabled(&stage, true, None),
        "Should use orchestrator config (true) when no overrides"
    );
    assert!(
        !is_auto_merge_enabled(&stage, false, None),
        "Should use orchestrator config (false) when no overrides"
    );

    // Test 2: Stage override = None, Plan = Some -> uses plan config
    stage.auto_merge = None;
    assert!(
        is_auto_merge_enabled(&stage, false, Some(true)),
        "Plan override (true) should take precedence over orchestrator (false)"
    );
    assert!(
        !is_auto_merge_enabled(&stage, true, Some(false)),
        "Plan override (false) should take precedence over orchestrator (true)"
    );

    // Test 3: Stage override = Some -> uses stage config (highest priority)
    stage.auto_merge = Some(true);
    assert!(
        is_auto_merge_enabled(&stage, false, Some(false)),
        "Stage override (true) should take precedence over everything"
    );
    stage.auto_merge = Some(false);
    assert!(
        !is_auto_merge_enabled(&stage, true, Some(true)),
        "Stage override (false) should take precedence over everything"
    );

    // Test 4: Verify orchestrator config flows through to OrchestratorConfig
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();
    std::fs::create_dir_all(work_dir.join("stages")).unwrap();

    let config = OrchestratorConfig {
        auto_merge: true,
        manual_mode: true,
        work_dir: work_dir.to_path_buf(),
        repo_root: temp_dir.path().to_path_buf(),
        ..Default::default()
    };

    assert!(config.auto_merge, "auto_merge should be enabled in config");

    let stage_defs = vec![create_stage_def("stage-1", "Test Stage", vec![])];
    let graph = ExecutionGraph::build(stage_defs).expect("Should build execution graph");
    let orchestrator = Orchestrator::new(config, graph).expect("Should create orchestrator");
    assert_eq!(orchestrator.running_session_count(), 0);
}

// NOTE: test_daemon_respects_stage_id was removed because it tested
// a run_single() method that doesn't exist in the Orchestrator API.
// The --stage flag functionality is tested via CLI integration tests instead.
