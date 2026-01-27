//! Orchestrator spawning and execution graph building.

use super::super::protocol::DaemonConfig;
use super::core::DaemonServer;
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::commands::run::mark_plan_done_if_all_merged;
use crate::fs::parse_base_branch_from_config;
use crate::fs::work_dir::WorkDir;
use crate::orchestrator::terminal::BackendType;
use crate::orchestrator::{Orchestrator, OrchestratorConfig};
use crate::plan::graph::ExecutionGraph;
use crate::plan::schema::SandboxConfig;

/// Spawn the orchestrator thread to execute stages.
///
/// Returns a join handle for the orchestrator thread.
pub fn spawn_orchestrator(server: &DaemonServer) -> Option<JoinHandle<()>> {
    let work_dir = server.work_dir.clone();
    let daemon_config = server.config.clone();
    let shutdown_flag = Arc::clone(&server.shutdown_flag);

    Some(thread::spawn(move || {
        if let Err(e) = run_orchestrator(&work_dir, &daemon_config, shutdown_flag) {
            eprintln!("Orchestrator error: {e}");
        }
    }))
}

/// Run the orchestrator loop (static method for thread).
fn run_orchestrator(
    work_dir: &Path,
    daemon_config: &DaemonConfig,
    shutdown_flag: Arc<AtomicBool>,
) -> Result<()> {
    // Build execution graph from stage files
    let graph = build_execution_graph(work_dir)?;

    // Get repo root (parent of .work/)
    // Clone for later use in mark_plan_done_if_all_merged
    let repo_root = work_dir
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    let repo_root_for_plan = repo_root.clone();

    // Parse base_branch from config.toml
    let base_branch = match parse_base_branch_from_config(work_dir) {
        Ok(branch) => {
            if let Some(ref b) = branch {
                eprintln!("Loaded base_branch from config: {b}");
            } else {
                eprintln!("Warning: No base_branch in config.toml, will use default_branch()");
            }
            branch
        }
        Err(e) => {
            eprintln!("Warning: Failed to parse base_branch from config.toml: {e}");
            None
        }
    };

    // Configure orchestrator using daemon config
    let config = OrchestratorConfig {
        max_parallel_sessions: daemon_config.max_parallel.unwrap_or(4),
        poll_interval: Duration::from_secs(5),
        manual_mode: daemon_config.manual_mode,
        watch_mode: daemon_config.watch_mode,
        work_dir: work_dir.to_path_buf(),
        repo_root,
        status_update_interval: Duration::from_secs(30),
        backend_type: BackendType::Native,
        auto_merge: daemon_config.auto_merge,
        base_branch,
        skills_dir: None, // Use default ~/.claude/skills/
        enable_skill_routing: true,
        max_skill_recommendations: 5,
        sandbox_config: SandboxConfig::default(),
    };

    // Create and run orchestrator
    let mut orchestrator =
        Orchestrator::new(config, graph).context("Failed to create orchestrator")?;

    println!("Orchestrator started, spawning ready stages...");

    // Check shutdown flag before starting
    if shutdown_flag.load(Ordering::Relaxed) {
        println!("Orchestrator shutdown requested before start");
        return Ok(());
    }

    // Run orchestrator - it runs its own loop internally and returns when complete
    let result = orchestrator.run();

    match result {
        Ok(result) => {
            if !result.completed_stages.is_empty() {
                println!("Completed stages: {}", result.completed_stages.join(", "));
            }
            if !result.failed_stages.is_empty() {
                println!("Failed stages: {}", result.failed_stages.join(", "));
            }
            if result.is_success() {
                println!("All stages completed successfully");

                // Mark plan as done if all stages are merged
                // Note: WorkDir::new expects repo_root, not work_dir (it appends .work internally)
                if let Ok(work_dir_obj) = WorkDir::new(&repo_root_for_plan) {
                    if let Err(e) = mark_plan_done_if_all_merged(&work_dir_obj) {
                        eprintln!("Warning: Failed to mark plan as done: {e}");
                    }
                }
            }

            // Write completion marker file to signal broadcaster
            write_completion_marker(work_dir);
        }
        Err(e) => {
            eprintln!("Orchestrator run error: {e}");
            // Still write completion marker on error so clients know orchestration stopped
            write_completion_marker(work_dir);
        }
    }

    // Signal daemon to exit now that orchestration is complete
    println!("Orchestration finished, signaling daemon shutdown");
    shutdown_flag.store(true, Ordering::Relaxed);

    Ok(())
}

/// Write a completion marker file to signal that orchestration has finished.
///
/// The status broadcaster detects this file and sends OrchestrationComplete
/// to all subscribers.
fn write_completion_marker(work_dir: &Path) {
    let marker_path = work_dir.join("orchestrator.complete");
    if let Err(e) = fs::write(&marker_path, chrono::Utc::now().to_rfc3339()) {
        eprintln!("Failed to write completion marker: {e}");
    }
}

/// Build execution graph from .work/stages/ files.
///
/// This function now delegates to the shared implementation in plan::graph::loader.
pub(super) fn build_execution_graph(work_dir: &Path) -> Result<ExecutionGraph> {
    crate::plan::graph::build_execution_graph(work_dir)
}
