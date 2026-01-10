mod diagnostics;
mod display;
pub mod merge_status;
mod validation;

use crate::commands::graph::build_tree_display;
use crate::daemon::{read_message, write_message, DaemonServer, Request, Response, StageInfo};
use crate::fs::work_dir::WorkDir;
use crate::models::worktree::WorktreeStatus;
use crate::utils::{cleanup_terminal, install_terminal_panic_hook};
use crate::verify::transitions::list_all_stages;
use anyhow::{Context, Result};
use colored::Colorize;
use std::os::unix::net::UnixStream;
use std::time::Duration;

use diagnostics::{
    check_directory_structure, check_orphaned_tracks, check_parsing_errors, check_stuck_runners,
};
use display::{
    count_files, display_runner_health, display_sessions, display_stages, display_worktrees,
    load_runners,
};
use validation::{validate_markdown_files, validate_references};

/// Show the status dashboard with context health
pub fn execute() -> Result<()> {
    let work_dir = WorkDir::new(".")?;
    work_dir.load()?;

    // Check if daemon is running and use live mode if available
    let work_path = work_dir.root();
    if DaemonServer::is_running(work_path) {
        match execute_live(work_path) {
            Ok(()) => return Ok(()),
            Err(e) => {
                // Connection failed or daemon unresponsive - fall back to static
                eprintln!("{}", format!("Could not connect to daemon: {e}").yellow());
                println!("{}", "Falling back to static status display.".dimmed());
            }
        }
    } else {
        println!(
            "\n{}",
            "Daemon not running. Showing static status.".dimmed()
        );
    }

    execute_static(&work_dir)
}

/// Connection timeout for daemon socket (2 seconds)
const SOCKET_TIMEOUT: Duration = Duration::from_secs(2);

/// Execute live-updating dashboard by subscribing to daemon
fn execute_live(work_path: &std::path::Path) -> Result<()> {
    // Install panic hook to restore terminal on panic
    install_terminal_panic_hook();

    let socket_path = work_path.join("orchestrator.sock");

    let mut stream =
        UnixStream::connect(&socket_path).context("Failed to connect to daemon socket")?;

    // Set read/write timeouts to prevent hanging
    stream
        .set_read_timeout(Some(SOCKET_TIMEOUT))
        .context("Failed to set read timeout")?;
    stream
        .set_write_timeout(Some(SOCKET_TIMEOUT))
        .context("Failed to set write timeout")?;

    // Ping first to verify daemon is responsive
    write_message(&mut stream, &Request::Ping).context("Failed to send Ping request")?;

    let ping_response: Response =
        read_message(&mut stream).context("Failed to read Ping response")?;

    match ping_response {
        Response::Pong => {}
        Response::Error { message } => {
            anyhow::bail!("Daemon returned error on ping: {message}");
        }
        _ => {
            anyhow::bail!("Unexpected response from daemon (expected Pong)");
        }
    }

    // Now that we know daemon is responsive, remove timeout for streaming
    stream
        .set_read_timeout(None)
        .context("Failed to clear read timeout")?;

    // Send subscribe request
    write_message(&mut stream, &Request::SubscribeStatus)
        .context("Failed to send SubscribeStatus request")?;

    // Wait for acknowledgment
    let response: Response =
        read_message(&mut stream).context("Failed to read subscription response")?;

    match response {
        Response::Ok => {}
        Response::Error { message } => {
            anyhow::bail!("Daemon returned error: {message}");
        }
        _ => {
            anyhow::bail!("Unexpected response from daemon");
        }
    }

    // Set up Ctrl+C handler
    let stream_for_signal = stream
        .try_clone()
        .context("Failed to clone stream for signal handler")?;
    ctrlc::set_handler(move || {
        let mut stream = stream_for_signal.try_clone().ok();
        if let Some(ref mut s) = stream {
            let _ = write_message(s, &Request::Unsubscribe);
        }
        // Restore terminal to clean state before exiting
        cleanup_terminal();
        std::process::exit(0);
    })
    .context("Failed to set Ctrl+C handler")?;

    println!("\n{}", "Live Status Dashboard".bold().blue());
    println!(
        "{}",
        "Press Ctrl+C to exit (daemon will continue running)".dimmed()
    );
    println!("{}", "=".repeat(50));

    // Loop receiving status updates
    loop {
        let response: Response = match read_message(&mut stream) {
            Ok(resp) => resp,
            Err(e) => {
                eprintln!("\n{}", "Connection to daemon lost".red());
                eprintln!("{}", format!("Error: {e}").dimmed());
                break;
            }
        };

        match response {
            Response::StatusUpdate {
                stages_executing,
                stages_pending,
                stages_completed,
                stages_blocked,
            } => {
                render_live_status(
                    work_path,
                    &stages_executing,
                    &stages_pending,
                    &stages_completed,
                    &stages_blocked,
                );
            }
            Response::Error { message } => {
                eprintln!("\n{}", format!("Daemon error: {message}").red());
                break;
            }
            _ => {
                // Ignore unexpected messages
            }
        }
    }

    // Clean up: send unsubscribe before exiting
    let _ = write_message(&mut stream, &Request::Unsubscribe);

    // Restore terminal state
    cleanup_terminal();

    Ok(())
}

/// Render live status update (clear screen and redraw)
fn render_live_status(
    work_dir: &std::path::Path,
    executing: &[StageInfo],
    pending: &[String],
    completed: &[String],
    blocked: &[String],
) {
    // Clear screen using ANSI escape codes
    print!("\x1B[2J\x1B[1;1H");

    println!("\n{}", "Live Status Dashboard".bold().blue());
    println!(
        "{}",
        "Press Ctrl+C to exit (daemon will continue running)".dimmed()
    );
    println!("{}", "=".repeat(50));

    // Show execution graph first (read from stage files)
    if let Ok(stages) = list_all_stages(work_dir) {
        if !stages.is_empty() {
            println!("\n{}", "Execution Graph".bold());
            let tree_display = build_tree_display(&stages);
            for line in tree_display.lines() {
                println!("  {line}");
            }
            // Compact legend
            println!();
            print!("  {} ", "Legend:".dimmed());
            print!("{} ", "✓".green().bold());
            print!("verified  ");
            print!("{} ", "●".blue().bold());
            print!("executing  ");
            print!("{} ", "▶".cyan().bold());
            print!("ready  ");
            print!("{} ", "○".white().dimmed());
            print!("pending  ");
            print!("{} ", "✔".green());
            print!("completed  ");
            print!("{} ", "✗".red().bold());
            print!("blocked  ");
            print!("{} ", "⟳".yellow().bold());
            println!("handoff");
        }
    }

    println!("{}", "─".repeat(50));

    let total = executing.len() + pending.len() + completed.len() + blocked.len();
    println!("\n{}", format!("Summary: {total} stages").bold());

    if !executing.is_empty() {
        println!(
            "\n{}",
            format!("● Executing ({})", executing.len()).blue().bold()
        );
        for stage in executing {
            let pid_info = stage
                .session_pid
                .map(|pid| format!(" [PID: {pid}]"))
                .unwrap_or_default();
            let elapsed = chrono::Utc::now()
                .signed_duration_since(stage.started_at)
                .num_seconds();
            let time_info = format_elapsed(elapsed);
            let worktree_info = format_worktree_status(&stage.worktree_status);
            println!(
                "    {}  {}{}{}  {}",
                stage.id.dimmed(),
                stage.name,
                pid_info.dimmed(),
                worktree_info,
                time_info.dimmed()
            );
        }
    }

    if !pending.is_empty() {
        println!(
            "\n{}",
            format!("○ Pending ({})", pending.len()).white().dimmed()
        );
        for stage_id in pending {
            println!("    {}", stage_id.dimmed());
        }
    }

    if !completed.is_empty() {
        println!("\n{}", format!("✔ Completed ({})", completed.len()).green());
        for stage_id in completed {
            println!("    {}", stage_id.dimmed());
        }
    }

    if !blocked.is_empty() {
        println!(
            "\n{}",
            format!("✗ Blocked ({})", blocked.len()).red().bold()
        );
        for stage_id in blocked {
            println!("    {stage_id}");
        }
    }

    println!();
}

/// Format elapsed time in human-readable format
fn format_elapsed(seconds: i64) -> String {
    if seconds < 60 {
        format!("{seconds}s")
    } else if seconds < 3600 {
        format!("{}m {}s", seconds / 60, seconds % 60)
    } else {
        format!("{}h {}m", seconds / 3600, (seconds % 3600) / 60)
    }
}

/// Format worktree status for display
fn format_worktree_status(status: &Option<WorktreeStatus>) -> colored::ColoredString {
    match status {
        Some(WorktreeStatus::Conflict) => " [CONFLICT]".red().bold(),
        Some(WorktreeStatus::Merging) => " [MERGING]".yellow().bold(),
        Some(WorktreeStatus::Merged) => " [MERGED]".green(),
        Some(WorktreeStatus::Creating) => " [CREATING]".cyan(),
        Some(WorktreeStatus::Removed) => " [REMOVED]".dimmed(),
        Some(WorktreeStatus::Active) | None => "".normal(),
    }
}

/// Show static status dashboard (original implementation)
fn execute_static(work_dir: &WorkDir) -> Result<()> {
    println!();
    println!("{}", "Loom Status Dashboard".bold().blue());
    println!("{}", "=".repeat(50));

    let (runners, runner_count) = load_runners(work_dir)?;
    let track_count = count_files(&work_dir.tracks_dir())?;
    let signal_count = count_files(&work_dir.signals_dir())?;
    let handoff_count = count_files(&work_dir.handoffs_dir())?;
    let stage_count = count_files(&work_dir.stages_dir())?;
    let session_count = count_files(&work_dir.sessions_dir())?;

    println!("\n{}", "Entities".bold());
    println!("  Runners:  {runner_count}");
    println!("  Tracks:   {track_count}");
    println!("  Signals:  {signal_count}");
    println!("  Handoffs: {handoff_count}");

    if stage_count > 0 || session_count > 0 {
        println!("  Stages:   {stage_count}");
        println!("  Sessions: {session_count}");
    }

    if !runners.is_empty() {
        println!("\n{}", "Runner Context Health".bold());
        for runner in runners {
            display_runner_health(&runner);
        }
    }

    if stage_count > 0 {
        display_stages(work_dir)?;
    }

    if session_count > 0 {
        display_sessions(work_dir)?;
    }

    // Show worktrees status
    display_worktrees(work_dir)?;

    // Show execution graph if stages exist
    if stage_count > 0 {
        display_execution_graph(work_dir)?;
    }

    println!();
    Ok(())
}

/// Display the execution graph showing stage dependencies
fn display_execution_graph(work_dir: &WorkDir) -> Result<()> {
    let stages_dir = work_dir.stages_dir();
    let work_path = stages_dir.parent().ok_or_else(|| {
        anyhow::anyhow!("Stages directory has no parent: {}", stages_dir.display())
    })?;

    let stages = list_all_stages(work_path)?;
    if stages.is_empty() {
        return Ok(());
    }

    println!("\n{}", "Execution Graph".bold());
    let tree_display = build_tree_display(&stages);
    for line in tree_display.lines() {
        println!("  {line}");
    }

    // Print compact legend with colored symbols
    println!();
    print!("  {} ", "Legend:".dimmed());
    print!("{} ", "✓".green().bold());
    print!("verified  ");
    print!("{} ", "●".blue().bold());
    print!("executing  ");
    print!("{} ", "▶".cyan().bold());
    print!("ready  ");
    print!("{} ", "○".white().dimmed());
    print!("pending  ");
    print!("{} ", "✔".green());
    print!("completed  ");
    print!("{} ", "✗".red().bold());
    print!("blocked  ");
    print!("{} ", "⟳".yellow().bold());
    println!("handoff");

    Ok(())
}

/// Validate the integrity of the work directory
pub fn validate() -> Result<()> {
    let work_dir = WorkDir::new(".")?;
    work_dir.load()?;

    println!("{}", "Validating work directory...".bold());

    let mut issues_found = 0;

    issues_found += validate_markdown_files(&work_dir.runners_dir(), "runners")?;
    issues_found += validate_markdown_files(&work_dir.tracks_dir(), "tracks")?;
    issues_found += validate_markdown_files(&work_dir.signals_dir(), "signals")?;
    issues_found += validate_markdown_files(&work_dir.handoffs_dir(), "handoffs")?;

    issues_found += validate_references(&work_dir)?;

    if issues_found == 0 {
        println!("\n{}", "All validations passed!".green().bold());
    } else {
        println!(
            "\n{} {}",
            "Found".red().bold(),
            format!("{issues_found} issue(s)").red().bold()
        );
    }

    Ok(())
}

/// Diagnose issues with the work directory
pub fn doctor() -> Result<()> {
    let work_dir = WorkDir::new(".")?;

    println!("{}", "Running diagnostics...".bold());

    let mut issues_found = 0;

    let runners_dir = work_dir.runners_dir();
    let work_root = runners_dir.parent().ok_or_else(|| {
        anyhow::anyhow!("Runners directory has no parent: {}", runners_dir.display())
    })?;

    if !work_root.exists() {
        println!("{} .work directory does not exist", "ERROR:".red().bold());
        println!("  {} Run 'loom init' to create it", "Fix:".yellow());
        return Ok(());
    }

    issues_found += check_directory_structure(&work_dir)?;
    issues_found += check_parsing_errors(&work_dir)?;
    issues_found += check_stuck_runners(&work_dir)?;
    issues_found += check_orphaned_tracks(&work_dir)?;

    if issues_found == 0 {
        println!("\n{}", "No issues found!".green().bold());
    } else {
        println!(
            "\n{} {}",
            "Found".yellow().bold(),
            format!("{issues_found} potential issue(s)").yellow().bold()
        );
    }

    Ok(())
}
