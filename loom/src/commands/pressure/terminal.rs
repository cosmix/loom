//! Terminal execution and presentation for the shared pressure pipeline.

use anyhow::{Context, Result};
use colored::Colorize;
use std::io::Write;
use std::path::Path;
use std::thread;
use std::time::Duration;

use crate::pressure::codex_run::{CodexOutcome, CodexRetirement, CodexRun};
use crate::pressure::{PressureResult, StepVerdict};

use super::spawn::{claude_should_stop, run_claude_foreground, should_stop};

pub(super) struct TerminalDriver<'a> {
    pub(super) claude_path: &'a Path,
    pub(super) codex_path: &'a Path,
    pub(super) repo_root: &'a Path,
    pub(super) marker: &'a Path,
    pub(super) codex_log: &'a Path,
    pub(super) report: &'a Path,
    pub(super) models: &'a crate::pressure::models::PressureModels,
    /// Stops the Codex runs below on a termination signal ([`retire_on_signal`]).
    pub(super) retirement: CodexRetirement,
}

/// Stop every run started through `retirement` when this process gets SIGINT,
/// SIGTERM or SIGHUP (the `ctrlc` crate's `termination` feature), then exit
/// 130. Codex leads its own process group, so without this a Ctrl-C ends the
/// driver and leaves Codex running. The crate allows one handler per process,
/// and `loom pressure` installs no other.
///
/// The run ends before its record step, so the handler prints `notice` (from
/// [`interrupted_notice`]): the foreground Claude session outlives a SIGTERM
/// or SIGHUP sent to `loom` alone (`dev-install.sh`'s `pkill -x loom`) and
/// finishes its review, which otherwise reads as a completed run.
pub(super) fn retire_on_signal(retirement: CodexRetirement, notice: String) -> Result<()> {
    ctrlc::set_handler(move || {
        if let Err(error) = retirement.retire() {
            eprintln!("\n{error:#}");
        }
        eprintln!("\n{notice}");
        std::process::exit(130);
    })
    .context("failed to install the pressure run's signal handler")
}

/// What an interrupted run tells the terminal: no `## Pressure tests` record was
/// written, and where the Codex review was left.
pub(super) fn interrupted_notice(plan: &str, report: &Path) -> String {
    format!(
        "! pressure run interrupted: {plan} was not recorded as pressure-tested; \
         the Codex review, if written, is at {}. Run `loom pressure {plan}` again to record one.",
        report.display()
    )
}

impl TerminalDriver<'_> {
    /// Start the Codex review in the background, through `self.retirement`.
    fn start_codex(&self, codex: &str) -> Result<CodexRun> {
        let candidates = crate::codex::codex_model_candidates(&self.models.codex)
            .into_iter()
            .map(String::from)
            .collect();
        let codex_run = self.retirement.spawn_with_fallback(
            self.codex_path,
            self.repo_root,
            codex,
            self.codex_log,
            candidates,
            &self.models.codex_effort,
        )?;
        println!(
            "{} codex review started in background (log: {})",
            "→".cyan().bold(),
            self.codex_log.display()
        );
        Ok(codex_run)
    }
}

impl crate::pressure::PressureDriver for TerminalDriver<'_> {
    fn delete_report(&mut self, report: &Path) -> Result<()> {
        crate::claude::remove_if_exists(report)
    }

    fn run_pressure(&mut self, round: u32, claude: &str, codex: &str) -> Result<PressureResult> {
        let codex_run = self.start_codex(codex)?;
        let (claude_outcome, codex_run) = run_foreground_with_codex(codex_run, || {
            run_claude_foreground(
                self.claude_path,
                self.repo_root,
                claude,
                self.marker,
                &self.models.claude,
                &self.models.claude_effort,
            )
        })?;
        let claude_reason = claude_should_stop(claude_outcome);
        if claude_reason.is_some() {
            // Ctrl-C at Claude's prompt is a keystroke, so its exit is the only
            // sign of it. The run stops here and deletes the report: waiting
            // would only keep the terminal, and the user, on a discarded review.
            if let Err(cancel) = codex_run.cancel_and_join() {
                tracing::warn!(error = %cancel, "failed to stop codex after claude stopped the run");
            }
            return Ok(PressureResult {
                verdict: step_verdict(round, "/pressure", claude_reason),
                codex_model_used: None,
            });
        }
        let outcome = wait_codex(codex_run, self.codex_log)?;
        print_fallbacks(&outcome);
        let codex_reason = should_stop("codex", outcome.status, Some(self.codex_log));
        if outcome.status.success() {
            report_codex_review(self.report, &outcome.model_used);
        }
        Ok(PressureResult {
            verdict: step_verdict(round, "/pressure", codex_reason),
            codex_model_used: Some(outcome.model_used),
        })
    }

    fn run_address(&mut self, round: u32, slash: &str) -> Result<StepVerdict> {
        let outcome = run_claude_foreground(
            self.claude_path,
            self.repo_root,
            slash,
            self.marker,
            &self.models.address,
            &self.models.address_effort,
        )?;
        Ok(step_verdict(round, "/address", claude_should_stop(outcome)))
    }
}

/// Keep the foreground failure intact after stopping the concurrent worker.
/// The pipeline deletes the report once this returns, after the worker is gone.
fn run_foreground_with_codex(
    codex_run: CodexRun,
    foreground: impl FnOnce() -> Result<crate::claude::ClaudeOutcome>,
) -> Result<(crate::claude::ClaudeOutcome, CodexRun)> {
    match foreground() {
        Ok(outcome) => Ok((outcome, codex_run)),
        Err(error) => {
            if let Err(cancel) = codex_run.cancel_and_join() {
                tracing::warn!(error = %cancel, "failed to stop codex after foreground error");
            }
            Err(error)
        }
    }
}

fn step_verdict(round: u32, slash: &str, reason: Option<String>) -> StepVerdict {
    match reason {
        Some(reason) => StepVerdict::Stop {
            step: format!("round {round} {slash}"),
            reason,
        },
        None => StepVerdict::Continue,
    }
}

/// After a clean Codex exit, name the model and whether it left its review.
fn report_codex_review(report: &Path, model: &str) {
    if report.is_file() {
        println!(
            "{} codex review ({model}) written → {}",
            "✓".green().bold(),
            report.display()
        );
    } else {
        println!(
            "{} codex ({model}) exited cleanly but wrote no review at {} — /address will run without it",
            "!".yellow().bold(),
            report.display()
        );
    }
}

/// Wait with a spinner once the foreground Claude session releases the TTY.
fn wait_codex(run: CodexRun, log_path: &Path) -> Result<CodexOutcome> {
    const FRAMES: [&str; 4] = ["⠋", "⠙", "⠹", "⠸"];
    let mut i = 0usize;
    while !run.is_finished() {
        print!(
            "\r{} waiting for codex review… (output → {})",
            FRAMES[i % FRAMES.len()],
            log_path.display()
        );
        let _ = std::io::stdout().flush();
        i += 1;
        thread::sleep(Duration::from_millis(200));
    }
    // Clear the spinner line.
    print!("\r\x1b[K");
    let _ = std::io::stdout().flush();
    run.join()
}

/// Print fallback notes only after the foreground Claude session has exited.
fn print_fallbacks(outcome: &CodexOutcome) {
    for note in &outcome.notes {
        println!("{} {note}", "!".yellow().bold());
    }
    if outcome.exhausted && outcome.tried.len() > 1 {
        println!(
            "{} codex: no candidate model is available to this account (tried {})",
            "!".yellow().bold(),
            outcome.tried.join(", ")
        );
    }
}

#[cfg(test)]
#[path = "terminal_tests.rs"]
mod tests;
