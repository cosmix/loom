use anyhow::{bail, Context, Result};
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use crate::fs::work_dir::WorkDir;
use crate::orchestrator::{Orchestrator, OrchestratorConfig, OrchestratorResult};
use crate::plan::parser::parse_plan;
use crate::plan::graph::ExecutionGraph;

const ORCHESTRATOR_SESSION: &str = "loom-orchestrator";

/// Execute plan stages via worktrees/sessions
/// Usage: loom run [--stage <id>] [--manual] [--max-parallel <n>]
pub fn execute(
    stage_id: Option<String>,
    manual: bool,
    max_parallel: Option<usize>,
) -> Result<()> {
    // 1. Load .work/ directory
    let work_dir = WorkDir::new(".")?;
    work_dir.load()?;

    // 2. Get active plan from config
    let plan_path = read_active_plan_path(&work_dir)?;

    // 3. Parse plan and build execution graph
    let parsed_plan = parse_plan(&plan_path)
        .with_context(|| format!("Failed to parse plan: {}", plan_path.display()))?;

    let graph = ExecutionGraph::build(parsed_plan.stages)
        .context("Failed to build execution graph")?;

    // 4. Configure orchestrator
    let config = OrchestratorConfig {
        max_parallel_sessions: max_parallel.unwrap_or(4),
        poll_interval: Duration::from_secs(5),
        manual_mode: manual,
        tmux_prefix: "loom".to_string(),
        work_dir: work_dir.root().to_path_buf(),
        repo_root: std::env::current_dir()?,
        status_update_interval: Duration::from_secs(30),
    };

    // 5. Create and run orchestrator
    let mut orchestrator = Orchestrator::new(config, graph);

    let result = if let Some(id) = stage_id {
        println!("Running single stage: {id}");
        orchestrator.run_single(&id)?
    } else {
        println!("Running all ready stages...");
        orchestrator.run()?
    };

    // 6. Print results
    print_result(&result);

    if result.is_success() {
        Ok(())
    } else {
        bail!("Orchestration completed with failures")
    }
}

/// Execute orchestrator in a background tmux session
/// Usage: loom run --background
pub fn execute_background(
    stage_id: Option<String>,
    manual: bool,
    max_parallel: Option<usize>,
) -> Result<()> {
    // Check if orchestrator session already exists
    let check_output = Command::new("tmux")
        .args(["has-session", "-t", ORCHESTRATOR_SESSION])
        .output();

    if let Ok(output) = check_output {
        if output.status.success() {
            println!("Orchestrator is already running in tmux session '{ORCHESTRATOR_SESSION}'");
            println!();
            println!("To attach:  tmux attach -t {ORCHESTRATOR_SESSION}");
            println!("To kill:    tmux kill-session -t {ORCHESTRATOR_SESSION}");
            return Ok(());
        }
    }

    // Build the loom run command to execute in tmux (must use --foreground!)
    let mut loom_cmd = String::from("loom run --foreground");
    if let Some(ref id) = stage_id {
        loom_cmd.push_str(&format!(" --stage {id}"));
    }
    if manual {
        loom_cmd.push_str(" --manual");
    }
    if let Some(n) = max_parallel {
        loom_cmd.push_str(&format!(" --max-parallel {n}"));
    }

    // Get current directory for tmux session
    let cwd = std::env::current_dir()?;

    // Create new tmux session with orchestrator
    let status = Command::new("tmux")
        .args([
            "new-session",
            "-d",                           // Detached
            "-s", ORCHESTRATOR_SESSION,     // Session name
            "-c", &cwd.to_string_lossy(),   // Working directory
            &loom_cmd,                      // Command to run
        ])
        .status()
        .context("Failed to create tmux session for orchestrator")?;

    if !status.success() {
        bail!("Failed to start orchestrator in tmux session");
    }

    println!("Orchestrator started in background.");
    println!();
    println!("To view orchestrator:   loom run --attach");
    println!("To view stage sessions: loom attach <stage-id>");
    println!("To list sessions:       loom sessions list");
    println!("To check status:        loom status");
    println!("To stop orchestrator:   loom clean --sessions");

    Ok(())
}

/// Attach to the running orchestrator tmux session
pub fn attach_orchestrator() -> Result<()> {
    // Check if orchestrator session exists
    let check_output = Command::new("tmux")
        .args(["has-session", "-t", ORCHESTRATOR_SESSION])
        .output();

    match check_output {
        Ok(output) if output.status.success() => {
            println!("Attaching to orchestrator session...");
            println!("(Press Ctrl+B, D to detach)\n");

            // Attach to the session
            #[cfg(unix)]
            {
                use std::os::unix::process::CommandExt;
                let error = Command::new("tmux")
                    .args(["attach", "-t", ORCHESTRATOR_SESSION])
                    .exec();
                bail!("Failed to attach to orchestrator: {error}");
            }

            #[cfg(not(unix))]
            {
                let status = Command::new("tmux")
                    .args(["attach", "-t", ORCHESTRATOR_SESSION])
                    .status()
                    .context("Failed to attach to orchestrator")?;
                if !status.success() {
                    bail!("Failed to attach to orchestrator");
                }
                Ok(())
            }
        }
        _ => {
            println!("No orchestrator is currently running.");
            println!();
            println!("To start the orchestrator:  loom run");
            println!("To check status:            loom status");
            Ok(())
        }
    }
}

/// Read the active plan path from config.toml
fn read_active_plan_path(work_dir: &WorkDir) -> Result<PathBuf> {
    let config_path = work_dir.root().join("config.toml");

    if !config_path.exists() {
        bail!("No active plan. Run 'loom init <plan-path>' first.");
    }

    let config_content =
        std::fs::read_to_string(&config_path).context("Failed to read config.toml")?;

    let config: toml::Value =
        toml::from_str(&config_content).context("Failed to parse config.toml")?;

    let source_path = config
        .get("plan")
        .and_then(|p| p.get("source_path"))
        .and_then(|s| s.as_str())
        .ok_or_else(|| anyhow::anyhow!("No 'plan.source_path' found in config.toml"))?;

    let path = PathBuf::from(source_path);

    if !path.exists() {
        bail!(
            "Plan file not found: {}\nThe plan may have been moved or deleted.",
            path.display()
        );
    }

    Ok(path)
}

/// Print orchestrator result summary
fn print_result(result: &OrchestratorResult) {
    println!("\n=== Orchestration Complete ===\n");

    if !result.completed_stages.is_empty() {
        println!("✓ Completed stages:");
        for stage in &result.completed_stages {
            println!("  - {stage}");
        }
    }

    if !result.failed_stages.is_empty() {
        println!("\n✗ Failed stages:");
        for stage in &result.failed_stages {
            println!("  - {stage}");
        }
    }

    if !result.needs_handoff.is_empty() {
        println!("\n⚠ Stages needing handoff:");
        for stage in &result.needs_handoff {
            println!("  - {stage}");
        }
        println!("\nRun 'loom resume <stage-id>' to continue these stages.");
    }

    println!("\nTotal sessions spawned: {}", result.total_sessions_spawned);

    if result.is_success() {
        println!("\n✓ All stages completed successfully!");
    }
}
