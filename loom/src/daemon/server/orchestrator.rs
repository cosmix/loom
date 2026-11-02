//! Orchestrator spawning and execution graph building.

use super::super::protocol::DaemonConfig;
use super::core::DaemonServer;
use super::status::{TerminalOutcome, COMPLETION_MARKER, SCHEDULER_FAILURE_FILE};
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::fs::mark_plan_done_if_all_merged;
use crate::fs::parse_base_branch_from_config;
use crate::fs::work_dir::WorkDir;
use crate::orchestrator::core::state_identity::LockIdentity;
use crate::orchestrator::{
    Orchestrator, OrchestratorConfig, OrchestratorResult, DEFAULT_KILL_CONFIRM_TIMEOUT,
};
use crate::plan::graph::ExecutionGraph;
use crate::plan::schema::SandboxConfig;

/// Spawn the orchestrator thread to execute stages.
///
/// Returns a join handle for the orchestrator thread.
pub fn spawn_orchestrator(
    server: &DaemonServer,
    lock_identity: Option<LockIdentity>,
) -> Result<Option<JoinHandle<()>>> {
    let work_dir = server.work_dir.clone();
    let daemon_config = server.config.clone();
    let shutdown_flag = Arc::clone(&server.shutdown_flag);

    let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
    let handle = thread::spawn(move || {
        supervise_worker(&work_dir, &shutdown_flag, || {
            run_orchestrator(
                &work_dir,
                &daemon_config,
                shutdown_flag.clone(),
                lock_identity,
                ready_tx,
            )
        });
    });
    if ready_rx.recv().is_err() {
        let _ = handle.join();
        anyhow::bail!("Scheduler initialization failed; see orchestrator.log and scheduler.failed");
    }
    Ok(Some(handle))
}

/// All exits, including initialization errors and panics, retire the essential worker.
fn supervise_worker(
    work_dir: &Path,
    shutdown: &AtomicBool,
    run: impl FnOnce() -> Result<TerminalOutcome>,
) {
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(run));
    let outcome = match outcome {
        Ok(Ok(outcome)) => outcome,
        Ok(Err(error)) => TerminalOutcome::SchedulerFailure(format!("{error:#}")),
        Err(payload) => TerminalOutcome::SchedulerFailure(format!(
            "scheduler panicked: {}",
            payload
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
                .unwrap_or("unknown panic")
        )),
    };
    if let TerminalOutcome::SchedulerFailure(ref failure) = outcome {
        eprintln!("Orchestrator failed: {failure}");
        if let Err(error) = fs::write(work_dir.join(SCHEDULER_FAILURE_FILE), failure) {
            eprintln!("Cannot persist scheduler failure: {error}");
        }
    }
    if let Err(error) = write_completion_marker(work_dir, &outcome) {
        eprintln!("Failed to write completion marker: {error:#}");
    }
    shutdown.store(true, Ordering::Release);
    crate::fs::tmux_tmpdir::remove_tmux_tmpdir_record(work_dir);
}

/// Run the orchestrator loop (static method for thread).
fn run_orchestrator(
    work_dir: &Path,
    daemon_config: &DaemonConfig,
    shutdown_flag: Arc<AtomicBool>,
    lock_identity: Option<LockIdentity>,
    ready: std::sync::mpsc::SyncSender<()>,
) -> Result<TerminalOutcome> {
    let (mut orchestrator, repo_root_for_plan) = initialize_orchestrator(
        work_dir,
        daemon_config,
        shutdown_flag.clone(),
        lock_identity,
    )?;

    ready
        .send(())
        .context("Daemon stopped awaiting scheduler readiness")?;
    println!("Orchestrator started, spawning ready stages...");

    // Check shutdown flag before starting
    if shutdown_flag.load(Ordering::Relaxed) {
        println!("Orchestrator shutdown requested before start");
        return Ok(TerminalOutcome::SchedulerFailure(
            "shutdown requested before scheduler start".into(),
        ));
    }

    let result = orchestrator.run().context("Orchestrator run failed")?;
    let outcome = finish_orchestration(&WorkDir::new(&repo_root_for_plan)?, &result);

    Ok(outcome)
}

fn initialize_orchestrator(
    work_dir: &Path,
    daemon_config: &DaemonConfig,
    shutdown_flag: Arc<AtomicBool>,
    lock_identity: Option<LockIdentity>,
) -> Result<(Orchestrator, PathBuf)> {
    let (graph, plan_sandbox) = build_execution_graph(work_dir)?;

    let (repo_root, plan_id) = plan_context(work_dir);
    let repo_root_for_plan = repo_root.clone();

    let base_branch = parse_base_branch_from_config(work_dir)?;
    if let Some(ref branch) = base_branch {
        eprintln!("Loaded base_branch from config: {branch}");
    } else {
        eprintln!("No base_branch in config.toml; using the repository default branch");
    }

    // Configure orchestrator using daemon config
    let config = orchestrator_config(
        daemon_config,
        work_dir,
        repo_root,
        base_branch,
        plan_sandbox,
        shutdown_flag,
        lock_identity,
        plan_id,
    );
    crate::fs::tmux_tmpdir::record_tmux_tmpdir_best_effort(work_dir);

    let orchestrator = Orchestrator::new(config, graph).context("Failed to create orchestrator")?;

    Ok((orchestrator, repo_root_for_plan))
}

fn plan_context(work_dir: &Path) -> (PathBuf, Option<String>) {
    // repo_root: hop count from the state root is layout-dependent (nested
    // `.loom/work` vs. legacy `.work`), so this goes through
    // `WorkDir::project_root()` rather than a bare `.parent()`. The fallback
    // is unreachable once work_dir is absolute.
    let work_dir_obj = WorkDir::new(work_dir).ok();
    let repo_root = work_dir_obj
        .as_ref()
        .and_then(|wd| wd.project_root().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));
    let plan_id = work_dir_obj
        .as_ref()
        .and_then(|wd| wd.load_config().ok().flatten())
        .and_then(|config| config.plan_id().map(str::to_owned));

    (repo_root, plan_id)
}

fn finish_orchestration(work_dir: &WorkDir, result: &OrchestratorResult) -> TerminalOutcome {
    log_stage_lists(result);
    if result.is_success() {
        println!("All stages completed successfully");
        if let Err(error) = mark_plan_done_if_all_merged(work_dir) {
            return TerminalOutcome::FinalizationFailure(format!("{error:#}"));
        }
    } else {
        return TerminalOutcome::Incomplete(format!(
            "{} failed, {} unfinished, {} requiring handoff",
            result.failed_stages.len(),
            result.unfinished_stages.len(),
            result.needs_handoff.len()
        ));
    }
    TerminalOutcome::Success
}

pub(crate) fn finish_foreground(work_dir: &WorkDir, result: &OrchestratorResult) -> Result<()> {
    let outcome = finish_orchestration(work_dir, result);
    write_completion_marker(work_dir.root(), &outcome)?;
    match outcome {
        TerminalOutcome::Success => Ok(()),
        TerminalOutcome::FinalizationFailure(message) => {
            anyhow::bail!("Finalization failure: {message}")
        }
        TerminalOutcome::SchedulerFailure(message) => anyhow::bail!("Scheduler failure: {message}"),
        TerminalOutcome::Incomplete(message) => {
            anyhow::bail!("Orchestration incomplete: {message}")
        }
    }
}

/// Log the completed/failed/unfinished stage id lists from a finished
/// orchestrator run, one line each, skipping any list that is empty.
fn log_stage_lists(result: &OrchestratorResult) {
    if !result.completed_stages.is_empty() {
        println!("Completed stages: {}", result.completed_stages.join(", "));
    }
    if !result.failed_stages.is_empty() {
        println!("Failed stages: {}", result.failed_stages.join(", "));
    }
    if !result.unfinished_stages.is_empty() {
        println!("Unfinished stages: {}", result.unfinished_stages.join(", "));
    }
}

/// Build the `OrchestratorConfig` for a daemon-spawned orchestrator run.
#[allow(clippy::too_many_arguments)]
fn orchestrator_config(
    daemon_config: &DaemonConfig,
    work_dir: &Path,
    repo_root: PathBuf,
    base_branch: Option<String>,
    plan_sandbox: SandboxConfig,
    shutdown_flag: Arc<AtomicBool>,
    lock_identity: Option<LockIdentity>,
    plan_id: Option<String>,
) -> OrchestratorConfig {
    OrchestratorConfig {
        max_parallel_sessions: daemon_config.max_parallel.unwrap_or(4),
        poll_interval: Duration::from_secs(5),
        manual_mode: daemon_config.manual_mode,
        watch_mode: daemon_config.watch_mode,
        work_dir: work_dir.to_path_buf(),
        repo_root,
        status_update_interval: Duration::from_secs(30),
        auto_merge: daemon_config.auto_merge,
        base_branch,
        skills_dir: None, // `skills::installed_skills_dir`
        enable_skill_routing: true,
        max_skill_recommendations: 8,
        sandbox_config: plan_sandbox,
        shutdown_flag: Some(shutdown_flag),
        lock_identity,
        plan_id,
        kill_confirm_timeout: DEFAULT_KILL_CONFIRM_TIMEOUT,
    }
}

/// Write a completion marker file to signal that orchestration has finished.
///
/// The status broadcaster detects this file and sends OrchestrationComplete
/// to all subscribers.
fn write_completion_marker(work_dir: &Path, outcome: &TerminalOutcome) -> Result<()> {
    let marker_path = work_dir.join(COMPLETION_MARKER);
    let content = serde_json::to_string(outcome)?;
    crate::fs::cache_io::atomic_write(&marker_path, &content)
}

/// Build execution graph from .loom/work/stages/ files.
///
/// This function now delegates to the shared implementation in plan::graph::loader.
pub(super) fn build_execution_graph(work_dir: &Path) -> Result<(ExecutionGraph, SandboxConfig)> {
    crate::plan::graph::build_execution_graph(work_dir)
}

#[cfg(test)]
#[path = "orchestrator_tests.rs"]
mod supervision_tests;
