//! Foreground execution mode for the orchestrator.

use anyhow::{bail, Context, Result};
use colored::Colorize;
use std::time::Duration;

use crate::fs::work_dir::WorkDir;
use crate::orchestrator::terminal::BackendType;
use crate::orchestrator::{Orchestrator, OrchestratorConfig, OrchestratorResult};

use super::graph_loader::build_execution_graph;

/// Parse base_branch from config.toml
fn parse_base_branch_from_config(work_dir: &WorkDir) -> Result<Option<String>> {
    let config_path = work_dir.root().join("config.toml");

    if !config_path.exists() {
        return Ok(None);
    }

    let config_content =
        std::fs::read_to_string(&config_path).context("Failed to read config.toml")?;

    let config: toml::Value =
        toml::from_str(&config_content).context("Failed to parse config.toml")?;

    let base_branch = config
        .get("plan")
        .and_then(|p| p.get("base_branch"))
        .and_then(|b| b.as_str())
        .map(String::from);

    Ok(base_branch)
}

/// Execute plan stages in foreground (for --foreground flag)
/// Usage: loom run --foreground [--stage <id>] [--manual] [--max-parallel <n>] [--watch] [--auto-merge]
pub fn execute(
    stage_id: Option<String>,
    manual: bool,
    max_parallel: Option<usize>,
    watch: bool,
    auto_merge: bool,
) -> Result<()> {
    let work_dir = WorkDir::new(".")?;
    work_dir.load()?;

    execute_foreground(stage_id, manual, max_parallel, watch, auto_merge, &work_dir)
}

/// Execute orchestrator in foreground mode (for debugging)
fn execute_foreground(
    stage_id: Option<String>,
    manual: bool,
    max_parallel: Option<usize>,
    watch: bool,
    auto_merge: bool,
    work_dir: &WorkDir,
) -> Result<()> {
    let graph = build_execution_graph(work_dir)?;

    // Parse config.toml to extract base_branch
    let base_branch = parse_base_branch_from_config(work_dir)?;

    let config = OrchestratorConfig {
        max_parallel_sessions: max_parallel.unwrap_or(4),
        poll_interval: Duration::from_secs(5),
        manual_mode: manual,
        watch_mode: watch,
        work_dir: work_dir.root().to_path_buf(),
        repo_root: std::env::current_dir()?,
        status_update_interval: Duration::from_secs(30),
        backend_type: BackendType::Native,
        auto_merge,
        base_branch,
    };

    let mut orchestrator =
        Orchestrator::new(config, graph).context("Failed to create orchestrator")?;

    let result = if let Some(id) = stage_id {
        println!("{} Running single stage: {}", "→".cyan().bold(), id.bold());
        orchestrator.run_single(&id)?
    } else {
        if watch {
            println!(
                "{} Running in watch mode {}",
                "→".cyan().bold(),
                "(continuous execution)".dimmed()
            );
            println!("  {} Press {} to stop\n", "→".dimmed(), "Ctrl+C".bold());
        } else {
            println!("{} Running all ready stages...", "→".cyan().bold());
        }
        orchestrator.run()?
    };

    print_result(&result);

    if result.is_success() {
        Ok(())
    } else {
        bail!("Orchestration completed with failures")
    }
}

/// Print orchestrator result summary
fn print_result(result: &OrchestratorResult) {
    println!();
    println!("{}", "╭──────────────────────────────────────╮".cyan());
    println!(
        "{}",
        "│       Orchestration Complete         │".cyan().bold()
    );
    println!("{}", "╰──────────────────────────────────────╯".cyan());

    if !result.completed_stages.is_empty() {
        println!(
            "\n{} {}",
            "Completed".green().bold(),
            format!("({})", result.completed_stages.len()).dimmed()
        );
        println!("{}", "─".repeat(40).dimmed());
        for stage in &result.completed_stages {
            println!("  {} {}", "✓".green().bold(), stage);
        }
    }

    if !result.failed_stages.is_empty() {
        println!(
            "\n{} {}",
            "Failed".red().bold(),
            format!("({})", result.failed_stages.len()).dimmed()
        );
        println!("{}", "─".repeat(40).dimmed());
        for stage in &result.failed_stages {
            println!("  {} {}", "✗".red().bold(), stage);
        }
    }

    if !result.needs_handoff.is_empty() {
        println!(
            "\n{} {}",
            "Needs Handoff".yellow().bold(),
            format!("({})", result.needs_handoff.len()).dimmed()
        );
        println!("{}", "─".repeat(40).dimmed());
        for stage in &result.needs_handoff {
            println!("  {} {}", "⚠".yellow().bold(), stage);
        }
        println!(
            "\n  {} Run {} to continue",
            "→".dimmed(),
            "loom resume <stage-id>".cyan()
        );
    }

    println!();
    println!("{}", "═".repeat(40).dimmed());
    println!(
        "Sessions spawned: {}",
        result.total_sessions_spawned.to_string().bold()
    );

    if result.is_success() {
        println!(
            "\n{} All stages completed successfully!",
            "✓".green().bold()
        );
    }
}
