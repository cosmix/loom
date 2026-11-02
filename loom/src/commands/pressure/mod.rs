//! `loom pressure` — alternating Claude/Codex plan pressure-testing driver.
//!
//! Each round runs two independent pressure-tests **concurrently**: Claude
//! `/pressure` in the foreground (interactive, the user watches it) and
//! Codex `$pressure` in the background (its noisy event stream captured to
//! a log file). Once both finish, Claude `/address` folds Codex's written
//! review back into the plan. The Codex report is deleted at the start of
//! every round so a failed Codex write can never leave `/address` reading a
//! stale review, plus once more after all rounds as cleanup.
//!
//! ## Why Claude runs in the foreground (and how it auto-exits)
//!
//! Claude Code enters its non-interactive (`-p`) path whenever stdout is not
//! a TTY, and the session stops being interactive. So Claude's stdout MUST
//! stay the real terminal; it cannot be captured or backgrounded. Interactive
//! Claude also never exits on its own after a slash command. We therefore
//! mirror how the loom daemon terminates a session: the agent signals
//! completion (here, by creating a marker file as its final action, injected
//! via `--append-system-prompt`), the driver watches for that marker, and
//! then SIGTERMs the now-idle session. If the marker never appears the user
//! can still exit manually, exactly as before.
//!
//! The marker lives under `<repo>/.loom/work/pressure/`, NOT `std::env::temp_dir()`:
//! Claude is spawned with `--permission-mode auto`, which sandboxes its Bash
//! tool with `/tmp` mounted read-only, so a temp-dir marker could never be
//! created and the driver would poll forever. The repo working tree is the
//! sandbox's writable root (the child's cwd is `repo_root`), so the marker is
//! re-homed there instead.

use anyhow::Result;
use colored::Colorize;
use std::path::Path;

use crate::claude::find_claude_path;
use crate::cli::types_pressure::PressureArgs;
use crate::codex::find_codex_path;
use crate::user_config::UserConfig;

mod paths;
mod spawn;
mod terminal;

use crate::pressure::codex_run::{codex_args, codex_model_label, CodexRetirement};
use crate::pressure::models::{project_pressure_config, resolve_rounds, PressureModels};
use crate::pressure::paths::{codex_report_path, resolve_plan_path, ResolvedPlan};
use crate::pressure::{plan_steps, PipelineInput, PipelineOutcome, Step, RECORD_STEP_LABEL};
use paths::{claude_marker_path, codex_log_path, resolve_repo_root};
use spawn::{claude_args, AGENT_TEAMS_ENV};
use terminal::{retire_on_signal, TerminalDriver};

/// Render the exact commands `--dry-run` would execute.
///
/// Uses the same `claude_args`/`codex_args` builders as the real spawns, so
/// the preview can never diverge from what actually runs.
fn render_dry_run(
    rounds: u32,
    invocation: &str,
    report: &Path,
    repo_root: &Path,
    marker: &Path,
    codex_log: &Path,
    models: &PressureModels,
) -> String {
    let mut out = format!(
        "Dry run: {rounds} round(s) of pressure-testing for {invocation}\n\
         Codex report:            {}\n\
         Codex log (captured):    {}\n\
         Claude auto-close marker: {}\n\
         Models:                  claude={}/{}  codex={}/{}  address={}/{}\n\n",
        report.display(),
        codex_log.display(),
        marker.display(),
        models.claude,
        models.claude_effort,
        codex_model_label(&models.codex),
        models.codex_effort,
        models.address,
        models.address_effort
    );
    let steps = plan_steps(rounds, invocation, report);
    for (n, step) in steps.into_iter().enumerate() {
        render_dry_run_step(&mut out, n + 1, step, invocation, repo_root, marker, models);
    }
    out
}

/// Render the dry-run preview lines for one step (numbered `n`), appending to
/// `out`. Split out of [`render_dry_run`] purely to keep that function under
/// the maintainability line limit.
fn render_dry_run_step(
    out: &mut String,
    n: usize,
    step: Step,
    invocation: &str,
    repo_root: &Path,
    marker: &Path,
    models: &PressureModels,
) {
    match step {
        Step::Record => out.push_str(&format!("  {n}. {RECORD_STEP_LABEL} in {invocation}\n")),
        Step::DeleteReport(p) => {
            out.push_str(&format!("  {n}. delete report {}\n", p.display()));
        }
        Step::Pressure { claude, codex } => {
            out.push_str(&format!(
                "  {n}. [parallel] Claude (foreground) + Codex (background → log):\n"
            ));
            out.push_str(&format!(
                "       {AGENT_TEAMS_ENV}=1 claude {}\n",
                claude_args(&claude, marker, &models.claude, &models.claude_effort).join(" ")
            ));
            out.push_str(&format!(
                "       codex {}\n",
                codex_args(repo_root, &codex, &models.codex, &models.codex_effort).join(" ")
            ));
        }
        Step::Address(slash) => {
            out.push_str(&format!(
                "  {n}. {AGENT_TEAMS_ENV}=1 claude {}\n",
                claude_args(&slash, marker, &models.address, &models.address_effort).join(" ")
            ));
        }
    }
}

/// Print the pressure run's header: round count and target, then the
/// per-role model selection. Split out of [`execute`] purely to keep that
/// function under the maintainability line limit.
fn print_run_header(rounds: u32, invocation: &str, models: &PressureModels) {
    println!(
        "{} {} round(s) on {}",
        "→".cyan().bold(),
        rounds,
        invocation.cyan()
    );
    println!(
        "{} models: claude={}/{}  codex={}/{}  address={}/{}\n",
        "→".cyan().bold(),
        models.claude,
        models.claude_effort,
        codex_model_label(&models.codex),
        models.codex_effort,
        models.address,
        models.address_effort
    );
}

/// Plain terminal outcome lines; the printer colours only their markers.
fn outcome_lines(outcome: &PipelineOutcome, plan: &str) -> Vec<String> {
    match outcome {
        PipelineOutcome::Recorded { line } => vec![
            "✓ Pressure test complete.".to_string(),
            format!("✓ recorded in {plan}: {line}"),
        ],
        PipelineOutcome::RecordSkipped { line, .. } => vec![format!(
            "! {plan} is no longer at the path the run started with; not recorded: {line}"
        )],
        PipelineOutcome::Stopped { .. } => Vec::new(),
    }
}

fn print_outcome(outcome: &PipelineOutcome, plan: &str) {
    let lines = outcome_lines(outcome, plan);
    if !lines.is_empty() {
        println!();
    }
    for line in lines {
        let (marker, text) = line.split_once(' ').unwrap_or(("", &line));
        let marker = if marker == "!" {
            marker.yellow().bold()
        } else {
            marker.green().bold()
        };
        println!("{marker} {text}");
    }
}

fn run_terminal_pipeline(
    driver: &mut TerminalDriver<'_>,
    rounds: u32,
    resolved: &ResolvedPlan,
) -> Result<()> {
    crate::utils::print_logo_header("Pressure Test");
    print_run_header(rounds, &resolved.invocation, driver.models);
    let input = PipelineInput {
        rounds,
        invocation: &resolved.invocation,
        report: driver.report,
        plan_path: &resolved.fs_path,
        claude_model: &driver.models.claude,
        today: chrono::Local::now().date_naive(),
    };
    let outcome = crate::pressure::run_pipeline(driver, &input)?;
    print_outcome(&outcome, &resolved.invocation);
    Ok(())
}

/// Execute the pressure pipeline.
pub fn execute(args: PressureArgs) -> Result<()> {
    let repo_root = resolve_repo_root()?;
    let repo_root = repo_root.canonicalize().unwrap_or(repo_root);
    let project = project_pressure_config(&repo_root);
    let user = UserConfig::load();
    let models = PressureModels::resolve(args.models, &project, &user);
    let rounds = resolve_rounds(args.rounds, &project, &user);
    let resolved = resolve_plan_path(&args.plan, &repo_root)?;
    let report = codex_report_path(&resolved.fs_path);
    let marker = claude_marker_path(&repo_root);
    let codex_log = codex_log_path();

    if args.dry_run {
        print!(
            "{}",
            render_dry_run(
                rounds,
                &resolved.invocation,
                &report,
                &repo_root,
                &marker,
                &codex_log,
                &models
            )
        );
        return Ok(());
    }
    let claude_path = find_claude_path()?;
    let codex_path = find_codex_path()?;
    let retirement = CodexRetirement::default();
    retire_on_signal(retirement.clone())?;
    let mut driver = TerminalDriver {
        claude_path: &claude_path,
        codex_path: &codex_path,
        repo_root: &repo_root,
        marker: &marker,
        codex_log: &codex_log,
        report: &report,
        models: &models,
        retirement,
    };
    run_terminal_pipeline(&mut driver, rounds, &resolved)
}

#[cfg(test)]
mod tests;
