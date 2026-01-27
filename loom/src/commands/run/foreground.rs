//! Foreground execution mode for the orchestrator.

use anyhow::{bail, Context, Result};
use colored::Colorize;
use std::time::Duration;

use crate::commands::status::render::print_completion_summary;
use crate::daemon::collect_completion_summary;
use crate::fs::work_dir::WorkDir;
use crate::orchestrator::terminal::BackendType;
use crate::orchestrator::{Orchestrator, OrchestratorConfig, OrchestratorResult};
use crate::plan::schema::SandboxConfig;

use super::checks::check_for_uncommitted_changes;
use super::graph_loader::build_execution_graph;
use super::plan_lifecycle;

/// Execute plan stages in foreground (for --foreground flag)
/// Usage: loom run --foreground [--manual] [--max-parallel <n>] [--watch] [--no-merge]
pub fn execute(
    manual: bool,
    max_parallel: Option<usize>,
    watch: bool,
    auto_merge: bool,
) -> Result<()> {
    // Check for uncommitted changes before starting
    let repo_root = std::env::current_dir()?;
    check_for_uncommitted_changes(&repo_root)?;

    let work_dir = WorkDir::new(".")?;
    work_dir.load()?;

    // Mark plan as in-progress when starting execution
    plan_lifecycle::mark_plan_in_progress(&work_dir)?;

    execute_foreground(manual, max_parallel, watch, auto_merge, &work_dir)
}

/// Execute orchestrator in foreground mode (for debugging)
fn execute_foreground(
    manual: bool,
    max_parallel: Option<usize>,
    watch: bool,
    auto_merge: bool,
    work_dir: &WorkDir,
) -> Result<()> {
    let graph = build_execution_graph(work_dir)?;

    // Parse config.toml to extract base_branch
    let base_branch = crate::fs::parse_base_branch_from_config(work_dir.root())?;

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
        skills_dir: None, // Use default ~/.claude/skills/
        enable_skill_routing: true,
        max_skill_recommendations: 5,
        sandbox_config: SandboxConfig::default(),
    };

    let mut orchestrator =
        Orchestrator::new(config, graph).context("Failed to create orchestrator")?;

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
    let result = orchestrator.run()?;

    // Collect and print the completion summary with timing and execution graph
    match collect_completion_summary(work_dir.root()) {
        Ok(summary) => {
            print_completion_summary(&summary);
        }
        Err(e) => {
            eprintln!("Warning: Failed to collect completion summary: {e}");
            // Fall back to basic result printing
            print_result(&result);
        }
    }

    // Print additional details for stages that need attention
    print_needs_attention(&result);

    // If successful, check if all stages are merged and mark plan as done
    if result.is_success() {
        plan_lifecycle::mark_plan_done_if_all_merged(work_dir)?;
        Ok(())
    } else {
        bail!("Orchestration completed with failures")
    }
}

/// Print orchestrator result summary (fallback for when completion summary fails)
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

/// Print additional details for stages that need attention (handoff/failures).
///
/// This supplements the completion summary with actionable information.
fn print_needs_attention(result: &OrchestratorResult) {
    if !result.needs_handoff.is_empty() {
        println!(
            "{} {}",
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
        println!();
    }
}
