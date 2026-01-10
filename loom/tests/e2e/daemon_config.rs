//! Integration tests for daemon and orchestrator configuration
//!
//! Tests verify that configuration options are properly applied and affect
//! orchestrator behavior as expected.

use loom::models::stage::{Stage, StageStatus};
use loom::orchestrator::terminal::BackendType;
use loom::orchestrator::{Orchestrator, OrchestratorConfig};
use loom::plan::graph::ExecutionGraph;
use loom::plan::schema::StageDefinition;
use loom::verify::transitions::save_stage;
use std::time::Duration;
use tempfile::TempDir;

/// Create a basic stage definition for testing
fn create_stage_def(id: &str, name: &str, deps: Vec<String>) -> StageDefinition {
    StageDefinition {
        id: id.to_string(),
        name: name.to_string(),
        description: Some(format!("Test stage {name}")),
        dependencies: deps,
        parallel_group: None,
        acceptance: vec![],
        setup: vec![],
        files: vec![],
        auto_merge: None,
    }
}

#[test]
fn test_orchestrator_config_default_values() {
    let config = OrchestratorConfig::default();

    assert_eq!(config.max_parallel_sessions, 4);
    assert_eq!(config.poll_interval, Duration::from_secs(5));
    assert!(!config.manual_mode);
    assert!(!config.watch_mode);
    assert!(!config.auto_merge);
    assert_eq!(config.status_update_interval, Duration::from_secs(30));
    assert_eq!(config.backend_type, BackendType::Native);
}

#[test]
fn test_orchestrator_config_custom_values() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();
    std::fs::create_dir_all(work_dir.join("stages")).unwrap();

    let config = OrchestratorConfig {
        max_parallel_sessions: 8,
        poll_interval: Duration::from_secs(10),
        manual_mode: true,
        watch_mode: true,
        work_dir: work_dir.to_path_buf(),
        repo_root: work_dir.to_path_buf(),
        status_update_interval: Duration::from_secs(60),
        backend_type: BackendType::Native,
        auto_merge: true,
    };

    assert_eq!(config.max_parallel_sessions, 8);
    assert_eq!(config.poll_interval, Duration::from_secs(10));
    assert!(config.manual_mode);
    assert!(config.watch_mode);
    assert!(config.auto_merge);
    assert_eq!(config.status_update_interval, Duration::from_secs(60));
}

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
    };

    let orchestrator = Orchestrator::new(config.clone(), graph);
    assert!(
        orchestrator.is_ok(),
        "Orchestrator should be created successfully"
    );

    let orchestrator = orchestrator.unwrap();
    assert_eq!(orchestrator.running_session_count(), 0);
}

/// Integration test: Verify manual mode sets up sessions without spawning Claude
///
/// In manual mode, `run()` should:
/// 1. Create worktrees and signal files for ready stages
/// 2. Track sessions in `active_sessions` (so `running_session_count()` reflects them)
/// 3. NOT spawn actual Claude processes - just print setup instructions
/// 4. Exit immediately after setting up the first batch of stages
///
/// This test verifies the manual mode behavior by checking that after calling
/// `run()`, sessions are tracked, worktrees exist, but no Claude is spawned.
#[test]
#[ignore] // Integration test - requires git repo, run with --ignored
fn test_orchestrator_with_manual_mode() {
    use crate::helpers::create_temp_git_repo;

    // Create a git repo (required for worktree creation)
    let temp_dir = create_temp_git_repo().expect("Should create git repo");
    let repo_root = temp_dir.path();

    // Create .work directory structure
    let work_dir = repo_root.join(".work");
    std::fs::create_dir_all(work_dir.join("stages")).unwrap();
    std::fs::create_dir_all(work_dir.join("sessions")).unwrap();
    std::fs::create_dir_all(work_dir.join("signals")).unwrap();

    // Create a stage file with Queued status (ready to execute)
    let mut stage = Stage::new("Test Stage".to_string(), None);
    stage.id = "stage-1".to_string();
    stage.status = StageStatus::Queued; // Already queued, ready for execution
    save_stage(&stage, &work_dir).expect("Should save stage");

    // Build execution graph - stage with no deps becomes Queued automatically
    let stage_defs = vec![create_stage_def("stage-1", "Test Stage", vec![])];
    let graph = ExecutionGraph::build(stage_defs).expect("Should build execution graph");

    // Verify the graph has the stage as Queued (ready)
    let ready = graph.ready_stages();
    assert_eq!(ready.len(), 1, "Stage should be ready in the graph");
    assert_eq!(ready[0].id, "stage-1");

    let config = OrchestratorConfig {
        max_parallel_sessions: 4,
        poll_interval: Duration::from_millis(50),
        manual_mode: true, // Key: manual mode - sets up but doesn't spawn
        watch_mode: false,
        work_dir: work_dir.clone(),
        repo_root: repo_root.to_path_buf(),
        status_update_interval: Duration::from_secs(30),
        backend_type: BackendType::Native,
        auto_merge: false,
    };

    let mut orchestrator = Orchestrator::new(config, graph).expect("Should create orchestrator");

    // Initially no sessions running
    assert_eq!(
        orchestrator.running_session_count(),
        0,
        "No sessions should be running initially"
    );

    // Run the orchestrator - in manual mode it sets up sessions and exits immediately
    let result = orchestrator.run().expect("Orchestrator run should succeed");

    // Verify: one stage was "started" (set up)
    assert_eq!(
        result.total_sessions_spawned, 1,
        "Should have set up 1 stage"
    );

    // Verify: session is tracked in active_sessions
    // In manual mode, sessions ARE tracked but Claude is not actually spawned
    assert_eq!(
        orchestrator.running_session_count(),
        1,
        "Session should be tracked (manual mode still tracks sessions)"
    );

    // Verify: worktree was created
    let worktrees_dir = repo_root.join(".worktrees");
    assert!(
        worktrees_dir.exists(),
        "Worktrees directory should be created"
    );

    // Verify: signal file was created
    let signals_dir = work_dir.join("signals");
    let signal_files: Vec<_> = std::fs::read_dir(&signals_dir)
        .expect("Should read signals dir")
        .filter_map(|e| e.ok())
        .collect();
    assert_eq!(
        signal_files.len(),
        1,
        "Signal file should be created for the session"
    );
}

#[test]
fn test_execution_graph_with_parallel_stages() {
    let stage_defs = vec![
        create_stage_def("stage-1", "Foundation", vec![]),
        create_stage_def("stage-2a", "Parallel A", vec!["stage-1".to_string()]),
        create_stage_def("stage-2b", "Parallel B", vec!["stage-1".to_string()]),
        create_stage_def(
            "stage-3",
            "Final",
            vec!["stage-2a".to_string(), "stage-2b".to_string()],
        ),
    ];

    let graph = ExecutionGraph::build(stage_defs).expect("Should build execution graph");

    // Initial ready stages should only include stage-1
    let ready = graph.ready_stages();
    assert_eq!(ready.len(), 1);
    assert_eq!(ready[0].id, "stage-1");
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
fn test_poll_interval_configuration() {
    // Test different poll intervals can be configured
    let configs = vec![
        Duration::from_millis(100),
        Duration::from_millis(500),
        Duration::from_secs(1),
        Duration::from_secs(5),
        Duration::from_secs(30),
    ];

    for poll_interval in configs {
        let config = OrchestratorConfig {
            poll_interval,
            ..Default::default()
        };
        assert_eq!(config.poll_interval, poll_interval);
    }
}

#[test]
fn test_status_update_interval_configuration() {
    // Test different status update intervals can be configured
    let intervals = vec![
        Duration::from_secs(5),
        Duration::from_secs(10),
        Duration::from_secs(30),
        Duration::from_secs(60),
    ];

    for interval in intervals {
        let config = OrchestratorConfig {
            status_update_interval: interval,
            ..Default::default()
        };
        assert_eq!(config.status_update_interval, interval);
    }
}

#[test]
fn test_max_parallel_sessions_configuration() {
    // Test different max_parallel_sessions values
    let values = vec![1, 2, 4, 8, 16];

    for max in values {
        let config = OrchestratorConfig {
            max_parallel_sessions: max,
            ..Default::default()
        };
        assert_eq!(config.max_parallel_sessions, max);
    }
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

/// Integration test: Verify orchestrator respects max_parallel_sessions at runtime
///
/// This test verifies that when 5 ready stages exist and max_parallel_sessions=2,
/// only 2 stages start executing at once. This tests the actual runtime behavior,
/// not just configuration storage.
#[test]
#[ignore] // Integration test - run with --ignored
fn test_orchestrator_respects_max_parallel_sessions() {
    use std::process::Command;

    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path();
    let work_dir = repo_root.join(".work");

    // Initialize a git repository (required for worktree creation)
    let git_init = Command::new("git")
        .args(["init"])
        .current_dir(repo_root)
        .output()
        .expect("Failed to run git init");
    assert!(git_init.status.success(), "git init failed");

    // Configure git user for commits (required for initial commit)
    Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(repo_root)
        .output()
        .expect("Failed to configure git email");
    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(repo_root)
        .output()
        .expect("Failed to configure git name");

    // Create an initial commit (worktree creation requires at least one commit)
    std::fs::write(repo_root.join("README.md"), "# Test Project").unwrap();
    Command::new("git")
        .args(["add", "."])
        .current_dir(repo_root)
        .output()
        .expect("Failed to git add");
    let git_commit = Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(repo_root)
        .output()
        .expect("Failed to git commit");
    assert!(git_commit.status.success(), "git commit failed");

    // Create work directories
    std::fs::create_dir_all(work_dir.join("stages")).unwrap();
    std::fs::create_dir_all(work_dir.join("sessions")).unwrap();
    std::fs::create_dir_all(work_dir.join("signals")).unwrap();

    // Create 5 stages without dependencies - all will be Queued (ready to execute)
    for i in 1..=5 {
        let mut stage = Stage::new(format!("Stage {i}"), None);
        stage.id = format!("stage-{i}");
        stage.status = StageStatus::Queued; // Ready to execute
        save_stage(&stage, &work_dir).expect("Should save stage");
    }

    let stage_defs: Vec<StageDefinition> = (1..=5)
        .map(|i| create_stage_def(&format!("stage-{i}"), &format!("Stage {i}"), vec![]))
        .collect();

    let graph = ExecutionGraph::build(stage_defs).expect("Should build execution graph");

    // Verify all 5 stages are ready (Queued status in graph)
    let ready_before = graph.ready_stages();
    assert_eq!(
        ready_before.len(),
        5,
        "All 5 stages should be ready (Queued) before run"
    );

    let config = OrchestratorConfig {
        max_parallel_sessions: 2, // Limit to 2 parallel
        poll_interval: Duration::from_millis(50),
        manual_mode: true, // Exit after first batch - does not actually spawn Claude
        watch_mode: false,
        work_dir: work_dir.clone(),
        repo_root: repo_root.to_path_buf(),
        status_update_interval: Duration::from_secs(30),
        backend_type: BackendType::Native,
        auto_merge: false,
    };

    let mut orchestrator = Orchestrator::new(config, graph).expect("Should create orchestrator");

    // Verify initial state
    assert_eq!(
        orchestrator.running_session_count(),
        0,
        "No sessions should be running before run()"
    );

    // Run the orchestrator - in manual mode it starts stages and exits
    let result = orchestrator.run().expect("Orchestrator run should succeed");

    // Verify that exactly 2 sessions were spawned (respecting max_parallel_sessions)
    assert_eq!(
        result.total_sessions_spawned, 2,
        "Should spawn exactly 2 sessions (max_parallel_sessions limit)"
    );

    // Verify that exactly 2 sessions are now active
    assert_eq!(
        orchestrator.running_session_count(),
        2,
        "Should have exactly 2 active sessions after run"
    );
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
    use loom::models::stage::Stage;
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
    use crate::helpers::create_temp_git_repo;
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
