use anyhow::{bail, Context, Result};
use std::path::PathBuf;
use std::time::Duration;

use crate::fs::work_dir::WorkDir;
use crate::orchestrator::{Orchestrator, OrchestratorConfig, OrchestratorResult};
use crate::plan::parser::parse_plan;
use crate::plan::graph::ExecutionGraph;

/// Execute plan stages via worktrees/sessions
/// Usage: flux run [--stage <id>] [--manual] [--max-parallel <n>]
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
        tmux_prefix: "flux".to_string(),
        work_dir: work_dir.root().to_path_buf(),
        repo_root: std::env::current_dir()?,
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

/// Read the active plan path from config.toml
fn read_active_plan_path(work_dir: &WorkDir) -> Result<PathBuf> {
    let config_path = work_dir.root().join("config.toml");

    if !config_path.exists() {
        bail!("No active plan. Run 'flux init <plan-path>' first.");
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
        println!("\nRun 'flux resume <stage-id>' to continue these stages.");
    }

    println!("\nTotal sessions spawned: {}", result.total_sessions_spawned);

    if result.is_success() {
        println!("\n✓ All stages completed successfully!");
    }
}
