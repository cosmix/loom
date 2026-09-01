pub mod data;
mod diagnostics;
mod display;
pub mod merge_status;
pub mod render;
pub mod ui;
mod validation;

use crate::daemon::{DaemonServer, DaemonStatus};
use crate::fs::work_dir::WorkDir;
use anyhow::Result;
use colored::{ColoredString, Colorize};

use crate::orchestrator::scheduling_report::{self, Alert, Severity};
use diagnostics::{check_directory_structure, check_parsing_errors};
use display::count_files;
use validation::{validate_markdown_files, validate_references};

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

/// Color category for a daemon-status line, decoupled from the `colored`
/// crate so [`daemon_status_line`]'s output is plain data a unit test can
/// assert on directly (see `mod tests` below).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StatusTone {
    Ok,
    Warning,
    Error,
    Neutral,
}

fn colorize(text: &str, tone: StatusTone) -> ColoredString {
    match tone {
        StatusTone::Ok => text.green(),
        StatusTone::Warning => text.yellow(),
        StatusTone::Error => text.red(),
        StatusTone::Neutral => text.dimmed(),
    }
}

/// Marker glyph, message, and trailing hint for the daemon-status line
/// printed by `execute_static`. A pure mapping from `(DaemonStatus,
/// loop_stalled)`, kept separate from the `colored`/println! plumbing so the
/// "never suggest `loom repair` for `Unreachable`" invariant is unit-testable
/// without parsing colored stdout.
struct DaemonStatusLine {
    marker: &'static str,
    marker_tone: StatusTone,
    message: &'static str,
    message_tone: StatusTone,
    hint: &'static str,
}

/// Shared shape for both "orchestrator loop stalled" arms of
/// `daemon_status_line` (`Running` and `Unreachable`); only the message text
/// differs, based on whether the control socket happens to be reachable
/// from this sandbox.
fn stalled_status_line(message: &'static str) -> DaemonStatusLine {
    DaemonStatusLine {
        marker: "●",
        marker_tone: StatusTone::Error,
        message,
        message_tone: StatusTone::Error,
        hint: "see below",
    }
}

/// Build the [`DaemonStatusLine`] for `status`, accounting for `loop_stalled`.
///
/// Stall guard: `Running` and `Unreachable` each get an "orchestrator loop
/// stalled" line ahead of their plain-status arm below, since once the
/// alerts fix widened `scheduling_report::alerts`'s gate to `Running |
/// Unreachable`, `loop_stalled` can genuinely be true for either. Without
/// this, the plain arms below would print a green "daemon running" headline
/// while `print_scheduler_alerts` lists a stalled-loop alert right
/// underneath it. The stall is the more urgent fact, so it leads in both
/// cases; the `Unreachable` message keeps the sandbox note attached so the
/// reader still knows the socket is unreachable from here. Still no repair
/// hint — the daemon itself may be fine even though its loop is stalled.
///
/// `Unreachable` (non-stalled): A-16 / sandboxed connect denial. The
/// singleton flock already proved a live daemon owns this state directory (see
/// `DaemonServer::check_status`'s `LockState::Held` arm) — the failed
/// `connect()` is a property of THIS process's sandbox, not the daemon. It
/// renders like a healthy `Running` daemon, with a note instead of
/// `ProcessOnly`'s repair hint: a sandboxed `loom status` must never claim
/// the socket is "missing" or suggest repairing (and via `loom repair`,
/// restarting) a daemon that is actually fine.
fn daemon_status_line(status: DaemonStatus, loop_stalled: bool) -> DaemonStatusLine {
    if loop_stalled {
        match status {
            DaemonStatus::Running => {
                return stalled_status_line("daemon running, orchestrator loop stalled")
            }
            DaemonStatus::Unreachable => return stalled_status_line(
                "daemon running, orchestrator loop stalled (socket unreachable from this sandbox)",
            ),
            _ => {}
        }
    }

    match status {
        DaemonStatus::Running => DaemonStatusLine {
            marker: "●",
            marker_tone: StatusTone::Ok,
            message: "daemon running",
            message_tone: StatusTone::Neutral,
            hint: "loom status --live for real-time updates",
        },
        DaemonStatus::Unreachable => DaemonStatusLine {
            marker: "●",
            marker_tone: StatusTone::Ok,
            message: "daemon running",
            message_tone: StatusTone::Neutral,
            hint: "control socket unreachable from this sandbox (expected inside a stage worktree)",
        },
        DaemonStatus::ProcessOnly => DaemonStatusLine {
            marker: "●",
            marker_tone: StatusTone::Warning,
            message: "daemon process alive, socket missing",
            message_tone: StatusTone::Warning,
            hint: "try `loom repair`",
        },
        DaemonStatus::NotRunning => DaemonStatusLine {
            marker: "○",
            marker_tone: StatusTone::Neutral,
            message: "daemon stopped",
            message_tone: StatusTone::Neutral,
            hint: "run `loom run` to start",
        },
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

    // Logo: prints a blank line above and below the ASCII art.
    crate::utils::print_logo_header("");

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
    let loop_stalled = alerts.iter().any(|a| a.severity == Severity::Critical);

    let line = daemon_status_line(daemon_status, loop_stalled);
    println!(
        "   {} {}        {}",
        colorize(line.marker, line.marker_tone),
        colorize(line.message, line.message_tone),
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

    // Verbose mode: show detailed failure information.
    if verbose {
        render::render_attention(&mut out, &status_data.stages)?;
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unreachable_status_never_suggests_repair() {
        let line = daemon_status_line(DaemonStatus::Unreachable, false);
        assert!(
            !line.hint.contains("loom repair"),
            "a sandboxed connect failure must never suggest repairing a healthy daemon"
        );
        assert!(
            !line.message.contains("missing"),
            "Unreachable must not claim the socket is missing — it's a caller-side sandbox limit"
        );
        assert_eq!(line.message, "daemon running");
        assert_eq!(line.marker_tone, StatusTone::Ok);
    }

    #[test]
    fn unreachable_status_with_stalled_loop_reports_the_stall() {
        let line = daemon_status_line(DaemonStatus::Unreachable, true);
        assert_eq!(
            line.marker_tone,
            StatusTone::Error,
            "a stalled loop must not render as healthy just because this caller is sandboxed"
        );
        assert!(
            line.message.contains("stalled"),
            "the headline must report the stall, not just \"daemon running\": {}",
            line.message
        );
        assert!(
            !line.hint.contains("loom repair"),
            "the daemon may be fine even with a stalled loop — still no repair hint"
        );
    }

    #[test]
    fn process_only_status_still_suggests_repair() {
        let line = daemon_status_line(DaemonStatus::ProcessOnly, false);
        assert!(line.hint.contains("loom repair"));
        assert_eq!(line.marker_tone, StatusTone::Warning);
    }

    #[test]
    fn stalled_running_status_overrides_healthy_running() {
        let line = daemon_status_line(DaemonStatus::Running, true);
        assert_eq!(line.marker_tone, StatusTone::Error);
        assert!(line.message.contains("stalled"));
    }

    #[test]
    fn not_running_status_uses_hollow_marker() {
        let line = daemon_status_line(DaemonStatus::NotRunning, false);
        assert_eq!(line.marker, "○");
    }
}
