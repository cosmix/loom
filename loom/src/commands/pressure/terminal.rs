//! Terminal execution and presentation for the shared pressure pipeline.

use anyhow::Result;
use colored::Colorize;
use std::io::Write;
use std::path::Path;
use std::thread;
use std::time::Duration;

use crate::pressure::codex_run::{spawn_codex_with_fallback, CodexOutcome, CodexRun};
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
}

impl crate::pressure::PressureDriver for TerminalDriver<'_> {
    fn delete_report(&mut self, report: &Path) -> Result<()> {
        crate::claude::remove_if_exists(report)
    }

    fn run_pressure(&mut self, round: u32, claude: &str, codex: &str) -> Result<PressureResult> {
        let candidates = crate::codex::codex_model_candidates(&self.models.codex)
            .into_iter()
            .map(String::from)
            .collect();
        let codex_run = spawn_codex_with_fallback(
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
        let claude_outcome = run_claude_foreground(
            self.claude_path,
            self.repo_root,
            claude,
            self.marker,
            &self.models.claude,
            &self.models.claude_effort,
        )?;
        let claude_reason = claude_should_stop(claude_outcome);
        let outcome = wait_codex(codex_run, self.codex_log)?;
        print_fallbacks(&outcome);
        let codex_reason = should_stop("codex", outcome.status, Some(self.codex_log));
        if outcome.status.success() {
            report_codex_review(self.report, &outcome.model_used);
        }
        Ok(PressureResult {
            verdict: step_verdict(round, "/pressure", claude_reason.or(codex_reason)),
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
mod tests {
    use super::*;

    #[test]
    fn stop_verdict_identifies_the_round_and_step_and_preserves_the_reason() {
        let reason = "claude exited with code 130 — stopping pressure run.";
        for slash in ["/pressure", "/address"] {
            assert_eq!(
                step_verdict(2, slash, Some(reason.to_string())),
                StepVerdict::Stop {
                    step: format!("round 2 {slash}"),
                    reason: reason.to_string(),
                }
            );
        }
        assert_eq!(step_verdict(2, "/address", None), StepVerdict::Continue);
    }
}
