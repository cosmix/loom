//! Tests for manual mode orchestrator behavior

use crate::helpers::create_temp_git_repo;
use loom::models::stage::{Stage, StageStatus};
use loom::orchestrator::terminal::BackendType;
use loom::orchestrator::OrchestratorConfig;
use loom::plan::graph::ExecutionGraph;
use loom::verify::transitions::save_stage;
use std::time::Duration;

use super::create_stage_def;

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
        base_branch: None,
    };

    let mut orchestrator =
        loom::orchestrator::Orchestrator::new(config, graph).expect("Should create orchestrator");

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
