//! Foreground execution mode for the orchestrator.

use anyhow::{bail, Context, Result};
use colored::Colorize;
use std::time::Duration;

use super::checks::prepare_repo_for_run;
use super::graph_loader::build_execution_graph;
use crate::commands::status::render::print_completion_summary;
use crate::daemon::{collect_completion_summary, DaemonServer};
use crate::fs::plan_lifecycle;
use crate::fs::work_dir::{read_terminal_config, write_terminal_config, WorkDir};
use crate::models::session::{SessionBackendKind, TerminalConfig};
use crate::orchestrator::{Orchestrator, OrchestratorConfig, OrchestratorResult};

/// Execute plan stages in foreground (for --foreground flag)
/// Usage: loom run --foreground [--manual] [--max-parallel <n>] [--watch] [--no-merge] [--backend <native|tmux>]
pub fn execute(
    manual: bool,
    max_parallel: Option<usize>,
    watch: bool,
    auto_merge: bool,
    backend: Option<String>,
) -> Result<()> {
    // Ensure git worktree prerequisites are met before starting.
    let repo_root = std::env::current_dir()?;
    prepare_repo_for_run(&repo_root)?;

    let work_dir = WorkDir::new(".")?;
    work_dir.load()?;

    // Resolve --backend: persist an explicit selection, guarding against
    // desync with an already-running daemon (its backend is fixed at
    // construction, so a config flip alone cannot reach it). `loom run`
    // never prompts — only `loom init` does.
    if let Some(value) = backend {
        let requested = match value.as_str() {
            "native" => SessionBackendKind::Native,
            "tmux" => SessionBackendKind::Tmux,
            other => bail!("Invalid terminal backend: {other}"),
        };

        let persisted = read_terminal_config(work_dir.root())?.backend;

        if DaemonServer::is_running(work_dir.root()) && requested != persisted {
            println!(
                "{} backend change requires a restart: run `loom stop`, then `loom run --backend {}`",
                "─".dimmed(),
                value
            );
        } else {
            if requested == SessionBackendKind::Tmux {
                // An explicit re-selection is a request to retry tmux.
                crate::orchestrator::terminal::backend::clear_fallback_marker(work_dir.root());
            }
            write_terminal_config(work_dir.root(), &TerminalConfig { backend: requested })?;
        }
    }

    // Advisory tmux preflight — never aborts startup.
    if read_terminal_config(work_dir.root())?.backend == SessionBackendKind::Tmux
        && which::which("tmux").is_err()
    {
        eprintln!(
            "tmux backend selected but tmux not found - sessions will fail to spawn until tmux is \
             installed or the backend is set back to native"
        );
    }

    // Mark plan as in-progress when starting execution
    plan_lifecycle::mark_plan_in_progress(&work_dir)?;

    crate::utils::print_logo_header("Run (foreground)");

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
    // Advisory Remote Control preflight — never aborts startup.
    if let Ok(claude_path) = crate::claude::find_claude_path() {
        crate::remote_control::run_startup_preflight(&claude_path, work_dir.root());
    }

    let (graph, plan_sandbox) = build_execution_graph(work_dir)?;

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
        auto_merge,
        base_branch,
        skills_dir: None, // Use default ~/.claude/skills/
        enable_skill_routing: true,
        max_skill_recommendations: 5,
        sandbox_config: plan_sandbox,
        shutdown_flag: None,
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
    crate::utils::print_logo_header("Orchestration Complete");

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
