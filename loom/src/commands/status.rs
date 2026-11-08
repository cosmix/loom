mod daemon_line;
pub mod data;
mod diagnostics;
mod display;
pub mod merge_status;
pub mod render;
pub mod ui;
mod validation;
pub mod web;

use crate::daemon::{DaemonServer, DaemonStatus};
use crate::fs::work_dir::WorkDir;
use anyhow::Result;
use colored::Colorize;

use crate::orchestrator::scheduling_report::{self, Alert, Severity};
use diagnostics::{check_directory_structure, check_parsing_errors};
use display::count_files;
use validation::validate_markdown_files;

/// Print the scheduler alerts (loop stalls, stages queued too long).
///
/// Shares [`scheduling_report::alerts`] with the live TUI so the two dashboards
/// cannot disagree about what counts as stuck.
fn print_scheduler_alerts(alerts: &[Alert]) {
    for alert in alerts {
        let (marker, text) = match alert.severity {
            Severity::Critical => ("✖".red(), alert.text.as_str().red()),
            Severity::Warning => ("!".yellow(), alert.text.as_str().yellow()),
            Severity::Info => ("·".dimmed(), alert.text.as_str().dimmed()),
        };
        println!("   {marker} {text}");
    }
}

/// Show the status dashboard with context health
pub fn execute(live: bool, compact: bool, verbose: bool) -> Result<()> {
    let work_dir = WorkDir::new(".")?;
    work_dir.load()?;

    let work_path = work_dir.root();

    // Compact mode: single-line output for scripting
    if compact {
        return execute_compact(&work_dir);
    }

    // Live mode: subscribe to daemon for real-time updates
    if live {
        if DaemonServer::is_running(work_path) {
            return ui::run_tui(work_path);
        } else {
            eprintln!("{}", "Daemon not running. Cannot use --live mode.".yellow());
            println!(
                "{}",
                "Start the daemon with 'loom run' or use static mode.".dimmed()
            );
            return Ok(());
        }
    }

    // Static mode (default): show snapshot of current state
    execute_static(&work_dir, verbose)
}

/// Execute compact mode - single line output for scripting
fn execute_compact(work_dir: &WorkDir) -> Result<()> {
    use data::collect_status_data;
    use std::io::stdout;

    let status_data = collect_status_data(work_dir)?;
    render::render_compact(&mut stdout(), &status_data)?;
    Ok(())
}

/// Show static status dashboard
fn execute_static(work_dir: &WorkDir, verbose: bool) -> Result<()> {
    use data::collect_status_data;
    use std::io::stdout;

    let status_data = collect_status_data(work_dir)?;
    let mut out = stdout();
    let stage_count = count_files(&work_dir.stages_dir())?;
    let daemon_status = DaemonServer::check_status(work_dir.root());

    // Logo: blank line, ASCII art, dimmed version subtitle, blank line.
    crate::utils::print_logo_header(crate::version::LABEL);

    // Plan title on its own line, bold.
    if let Some(ref name) = status_data.plan_name {
        println!("   {}", name.bold());
        println!();
    }

    // Daemon status: indicator + hint, separated by a wide gap so they don't
    // run together visually. ProcessOnly is surfaced distinctly (A-16): the
    // daemon process is alive but the IPC socket is genuinely missing or
    // refused, so the operator gets an actionable hint instead of a
    // misleading "stopped". Unreachable is a different condition entirely
    // (see `DaemonStatus::Unreachable`'s doc comment): the socket is fine but
    // THIS process's sandbox denies the connect — it renders like a healthy
    // `Running` daemon and never suggests `loom repair`.
    // A live socket only proves the daemon's server thread is up. The
    // orchestrator loop runs on a separate thread and can stop turning (a
    // wedged subprocess during teardown, a hung git call) while the socket
    // keeps answering — stages then sit Queued indefinitely and "daemon
    // running" actively misleads. Alerts report the loop and the scheduler
    // separately from the socket.
    // `Unreachable` joins `Running` here (not `ProcessOnly`): the flock
    // already proves the daemon is alive in both cases, same as `Running` —
    // the only difference is this process's own sandbox can't dial the
    // socket. Gating alerts off for it would silently drop real stall/queued
    // warnings for exactly the sandboxed callers (stage worktree sessions)
    // this variant exists for. `ProcessOnly` stays excluded: there the
    // socket is genuinely gone, which is itself evidence the daemon may be
    // wedged, so trusting its on-disk report as "live" would be premature.
    let alerts = scheduling_report::alerts(
        work_dir.root(),
        matches!(
            daemon_status,
            DaemonStatus::Running | DaemonStatus::Unreachable
        ),
    );
    let loop_state = daemon_line::loop_state_for(work_dir.root(), daemon_status);

    let line = daemon_line::daemon_status_line(daemon_status, &loop_state);
    println!(
        "   {} {}        {}",
        daemon_line::colorize(line.marker, line.marker_tone),
        daemon_line::colorize(&line.message, line.message_tone),
        line.hint.dimmed()
    );
    print_scheduler_alerts(&alerts);
    println!();

    // Progress bar with stage counts.
    render::render_progress(&mut out, &status_data.progress)?;

    // Unified stage graph (replaces separate Active Stages, Worktrees, Merge sections).
    if stage_count > 0 {
        println!();
        render::render_graph(&mut out, &status_data)?;
    }

    // Merge status: only show if there are pending merges or conflicts.
    if !status_data.merge.pending.is_empty() || !status_data.merge.conflicts.is_empty() {
        render::render_merge_status(&mut out, &status_data.merge)?;
    }

    // Stages needing a decision (blocked, failed acceptance, or awaiting human
    // review) show regardless of --verbose: a stopped stage only helps if the
    // operator can see why. --verbose also expands each stage's Evidence: listing.
    render::render_attention(&mut out, &status_data.stages, verbose)?;

    println!();
    Ok(())
}

/// Validate the integrity of the work directory
pub fn validate() -> Result<()> {
    let work_dir = WorkDir::new(".")?;
    work_dir.load()?;

    println!("{}", "Validating work directory...".bold());

    let mut issues_found = 0;

    issues_found += validate_markdown_files(&work_dir.signals_dir(), "signals")?;
    issues_found += validate_markdown_files(&work_dir.handoffs_dir(), "handoffs")?;

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

    let work_root = work_dir.root();

    if !work_root.exists() {
        println!(
            "{} {} does not exist",
            "ERROR:".red().bold(),
            work_root.display()
        );
        println!("  {} Run 'loom init' to create it", "Fix:".yellow());
        return Ok(());
    }

    issues_found += check_directory_structure(&work_dir)?;
    issues_found += check_parsing_errors(&work_dir)?;

    // Informational only — never counted as an issue. Shared verbatim with
    // `loom run`'s startup preflight, see `sccache_status_line`.
    println!(
        "{} {}",
        "→".cyan().bold(),
        crate::orchestrator::terminal::native::sccache_status_line()
    );

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
