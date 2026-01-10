//! Tests for parallel session configuration and behavior

use loom::models::stage::{Stage, StageStatus};
use loom::orchestrator::terminal::BackendType;
use loom::orchestrator::{Orchestrator, OrchestratorConfig};
use loom::plan::graph::ExecutionGraph;
use loom::verify::transitions::save_stage;
use std::time::Duration;
use tempfile::TempDir;

use super::create_stage_def;

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

    let stage_defs: Vec<_> = (1..=5)
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
        base_branch: None,
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
