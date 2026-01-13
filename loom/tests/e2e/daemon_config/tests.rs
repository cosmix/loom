//! Remaining orchestrator configuration tests
//!
//! Tests for orchestrator creation, auto-merge, backend types, watch mode,
//! directory configuration, and stage-specific execution.

use crate::helpers::create_temp_git_repo;
use loom::models::stage::{Stage, StageStatus};
use loom::orchestrator::terminal::BackendType;
use loom::orchestrator::{Orchestrator, OrchestratorConfig};
use loom::plan::graph::ExecutionGraph;
use loom::plan::schema::StageDefinition;
use loom::verify::transitions::save_stage;
use std::time::Duration;
use tempfile::TempDir;

use super::create_stage_def;

#[test]
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
        stage_type: loom::plan::schema::StageType::default(),
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
        stage_type: loom::plan::schema::StageType::default(),
    };

    assert_eq!(stage_without_override.auto_merge, None);
}

#[test]
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

/// Integration test: Verify --stage flag causes only specified stage to run
///
/// When calling `orchestrator.run_single(stage_id)`, only that specific stage
/// should be started, while other ready stages remain untouched.
#[test]
#[ignore] // Integration test - requires git repo, run with --ignored
fn test_daemon_respects_stage_id() {
    use loom::verify::transitions::load_stage;

    // Create a git repo (required for worktree creation)
    let temp_dir = create_temp_git_repo().expect("Should create git repo");
    let repo_root = temp_dir.path();

    // Create .work directory structure
    let work_dir = repo_root.join(".work");
    std::fs::create_dir_all(work_dir.join("stages")).unwrap();
    std::fs::create_dir_all(work_dir.join("sessions")).unwrap();
    std::fs::create_dir_all(work_dir.join("signals")).unwrap();

    // Create 3 stages - all without dependencies (all become Queued)
    for (id, name) in [
        ("stage-1", "First Stage"),
        ("stage-2", "Second Stage"),
        ("stage-3", "Third Stage"),
    ] {
        let mut stage = Stage::new(name.to_string(), None);
        stage.id = id.to_string();
        stage.status = StageStatus::Queued;
        save_stage(&stage, &work_dir).expect("Should save stage");
    }

    // Build execution graph
    let stage_defs = vec![
        create_stage_def("stage-1", "First Stage", vec![]),
        create_stage_def("stage-2", "Second Stage", vec![]),
        create_stage_def("stage-3", "Third Stage", vec![]),
    ];
    let graph = ExecutionGraph::build(stage_defs).expect("Should build execution graph");

    // Verify all 3 stages are ready
    let ready = graph.ready_stages();
    assert_eq!(ready.len(), 3, "All 3 stages should be ready");

    let config = OrchestratorConfig {
        max_parallel_sessions: 4,
        poll_interval: Duration::from_millis(50),
        manual_mode: true, // Key: exits immediately after setup
        watch_mode: false,
        work_dir: work_dir.clone(),
        repo_root: repo_root.to_path_buf(),
        status_update_interval: Duration::from_secs(30),
        backend_type: BackendType::Native,
        auto_merge: false,
        base_branch: None,
    };

    let mut orchestrator = Orchestrator::new(config, graph).expect("Should create orchestrator");

    // Run ONLY stage-2 (not stage-1 or stage-3)
    let result = orchestrator
        .run_single("stage-2")
        .expect("run_single should succeed");

    // Verify only 1 session was spawned
    assert_eq!(
        result.total_sessions_spawned, 1,
        "Should spawn exactly 1 session for the specified stage"
    );

    // Verify stage-2 is now Executing
    let stage_2 = load_stage("stage-2", &work_dir).expect("Should load stage-2");
    assert_eq!(
        stage_2.status,
        StageStatus::Executing,
        "stage-2 should be Executing"
    );

    // Verify stage-1 and stage-3 are still Queued (untouched)
    let stage_1 = load_stage("stage-1", &work_dir).expect("Should load stage-1");
    assert_eq!(
        stage_1.status,
        StageStatus::Queued,
        "stage-1 should still be Queued (not started)"
    );

    let stage_3 = load_stage("stage-3", &work_dir).expect("Should load stage-3");
    assert_eq!(
        stage_3.status,
        StageStatus::Queued,
        "stage-3 should still be Queued (not started)"
    );
}
