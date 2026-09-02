//! `loom pressure` — alternating Claude/Codex plan pressure-testing driver.
//!
//! Each round runs two independent pressure-tests **concurrently**: Claude
//! `/pressure` in the foreground (interactive → subscription billing, the user
//! watches it) and Codex `$pressure` in the background (its noisy event stream
//! captured to a log file). Once both finish, Claude `/address` folds Codex's
//! written review back into the plan. The Codex report is deleted at the start
//! of every round so a failed Codex write can never leave `/address` reading a
//! stale review, plus once more after all rounds as cleanup.
//!
//! ## Why Claude runs in the foreground (and how it auto-exits)
//!
//! Claude Code enters its non-interactive (`-p`) path — which can bill against
//! pay-per-token API credits instead of the subscription — whenever stdout is
//! not a TTY. So Claude's stdout MUST stay the real terminal; it cannot be
//! captured or backgrounded. Interactive Claude also never exits on its own
//! after a slash command. We therefore mirror how the loom daemon terminates a
//! session: the agent signals completion (here, by creating a marker file as
//! its final action, injected via `--append-system-prompt`), the driver watches
//! for that marker, and then SIGTERMs the now-idle session. If the marker never
//! appears the user can still exit manually, exactly as before.
//!
//! The marker lives under `<repo>/.loom/work/pressure/`, NOT `std::env::temp_dir()`:
//! Claude is spawned with `--permission-mode auto`, which sandboxes its Bash
//! tool with `/tmp` mounted read-only, so a temp-dir marker could never be
//! created and the driver would poll forever. The repo working tree is the
//! sandbox's writable root (the child's cwd is `repo_root`), so the marker is
//! re-homed there instead.

use anyhow::Result;
use colored::Colorize;
use std::path::{Path, PathBuf};

use crate::claude::find_claude_path;
use crate::codex::find_codex_path;

mod paths;
mod spawn;

use paths::{
    claude_marker_path, codex_log_path, codex_report_path, delete_file, resolve_plan_path,
    resolve_repo_root,
};
use spawn::{
    claude_args, claude_should_stop, codex_args, run_claude_foreground, should_stop,
    spawn_codex_background, wait_codex, AGENT_TEAMS_ENV,
};

/// One step in the pressure pipeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Step {
    /// Delete the codex report file if it exists.
    DeleteReport(PathBuf),
    /// Run the two independent pressure-tests concurrently: Claude `/pressure`
    /// in the foreground and Codex `$pressure` in the background.
    Pressure {
        /// Full positional slash invocation, e.g. `/pressure doc/plans/PLAN-foo.md`.
        claude: String,
        /// Full positional skill invocation, e.g. `$pressure doc/plans/PLAN-foo.md`.
        codex: String,
    },
    /// Run Claude `/address` in the foreground to fold the review into the plan.
    Address(String),
}

/// Build the ordered list of steps for `rounds` rounds.
///
/// Each round: delete the report (so a failed Codex write can't leave
/// `/address` reading the previous round's report) → run Claude `/pressure` and
/// Codex `$pressure` concurrently → Claude `/address`. After all rounds, one
/// final report deletion as cleanup.
pub(super) fn plan_steps(rounds: u32, invocation: &str, report: &Path) -> Vec<Step> {
    let mut steps = Vec::new();
    for _ in 0..rounds {
        steps.push(Step::DeleteReport(report.to_path_buf()));
        steps.push(Step::Pressure {
            claude: format!("/pressure {invocation}"),
            codex: format!("$pressure {invocation}"),
        });
        steps.push(Step::Address(format!("/address {invocation}")));
    }
    steps.push(Step::DeleteReport(report.to_path_buf()));
    steps
}

/// Render the exact commands `--dry-run` would execute.
///
/// Uses the same `claude_args`/`codex_args` builders as the real spawns, so
/// the preview can never diverge from what actually runs.
pub(super) fn render_dry_run(
    rounds: u32,
    invocation: &str,
    report: &Path,
    repo_root: &Path,
    marker: &Path,
    codex_log: &Path,
) -> String {
    let mut out = format!(
        "Dry run: {rounds} round(s) of pressure-testing for {invocation}\n\
         Codex report:            {}\n\
         Codex log (captured):    {}\n\
         Claude auto-close marker: {}\n\n",
        report.display(),
        codex_log.display(),
        marker.display()
    );
    let mut n = 1;
    for step in plan_steps(rounds, invocation, report) {
        match step {
            Step::DeleteReport(p) => {
                out.push_str(&format!("  {n}. delete report {}\n", p.display()));
                n += 1;
            }
            Step::Pressure { claude, codex } => {
                out.push_str(&format!(
                    "  {n}. [parallel] Claude (foreground) + Codex (background → log):\n"
                ));
                out.push_str(&format!(
                    "       {AGENT_TEAMS_ENV}=1 claude {}\n",
                    claude_args(&claude, marker).join(" ")
                ));
                out.push_str(&format!(
                    "       codex {}\n",
                    codex_args(repo_root, &codex).join(" ")
                ));
                n += 1;
            }
            Step::Address(slash) => {
                out.push_str(&format!(
                    "  {n}. {AGENT_TEAMS_ENV}=1 claude {}\n",
                    claude_args(&slash, marker).join(" ")
                ));
                n += 1;
            }
        }
    }
    out
}

/// Execute the pressure pipeline.
pub fn execute(plan: String, rounds: u32, dry_run: bool) -> Result<()> {
    let repo_root = resolve_repo_root()?;
    let repo_root = repo_root.canonicalize().unwrap_or(repo_root);
    let resolved = resolve_plan_path(&plan, &repo_root)?;
    let report = codex_report_path(&resolved.fs_path);
    let marker = claude_marker_path(&repo_root);
    let codex_log = codex_log_path();

    if dry_run {
        print!(
            "{}",
            render_dry_run(
                rounds,
                &resolved.invocation,
                &report,
                &repo_root,
                &marker,
                &codex_log
            )
        );
        return Ok(());
    }

    let claude_path = find_claude_path()?;
    let codex_path = find_codex_path()?;

    crate::utils::print_logo_header("Pressure Test");
    println!(
        "{} {} round(s) on {}\n",
        "→".cyan().bold(),
        rounds,
        resolved.invocation.cyan()
    );

    for step in plan_steps(rounds, &resolved.invocation, &report) {
        let stop = match step {
            Step::DeleteReport(path) => {
                delete_file(&path)?;
                false
            }
            Step::Pressure { claude, codex } => {
                // Codex reviews the plan independently in the background (quiet,
                // captured to a log) while Claude pressure-tests in the
                // foreground (interactive → subscription billing).
                let codex_child =
                    spawn_codex_background(&codex_path, &repo_root, &codex, &codex_log)?;
                println!(
                    "{} codex review started in background (log: {})",
                    "→".cyan().bold(),
                    codex_log.display()
                );
                let claude_outcome =
                    run_claude_foreground(&claude_path, &repo_root, &claude, &marker)?;
                let claude_stop = claude_should_stop(claude_outcome);
                let codex_status = wait_codex(codex_child, &codex_log)?;
                let codex_stop = should_stop("codex", codex_status, Some(&codex_log));
                if codex_status.success() {
                    if report.is_file() {
                        println!(
                            "{} codex review written → {}",
                            "✓".green().bold(),
                            report.display()
                        );
                    } else {
                        println!(
                            "{} codex exited cleanly but wrote no review at {} — /address will run without it",
                            "!".yellow().bold(),
                            report.display()
                        );
                    }
                }
                claude_stop || codex_stop
            }
            Step::Address(slash) => {
                let outcome = run_claude_foreground(&claude_path, &repo_root, &slash, &marker)?;
                claude_should_stop(outcome)
            }
        };
        if stop {
            return Ok(());
        }
    }

    println!("\n{} Pressure test complete.", "✓".green().bold());
    Ok(())
}

#[cfg(test)]
mod tests;
