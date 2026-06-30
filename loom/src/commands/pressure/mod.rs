//! `loom pressure` — alternating Claude/Codex plan pressure-testing driver.
//!
//! Each round spawns Claude to pressure-test and update the plan, then Codex to
//! write an independent review next to the plan, then Claude again to address
//! the review. The Codex report is deleted at the start of every round so that
//! if Codex fails to write a fresh review, `/address` never reads the previous
//! round's report, plus once more after all rounds as cleanup.

use anyhow::{bail, Context, Result};
use colored::Colorize;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};

use crate::claude::find_claude_path;
use crate::codex::find_codex_path;

/// A plan path resolved both for local filesystem use and for handing to the
/// slash commands / codex skill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPlan {
    /// Absolute, canonicalized path on disk — used for existence checks, the
    /// codex report path, and deletion.
    pub fs_path: PathBuf,
    /// The string handed to the slash commands and codex skill. Repo-relative
    /// when the plan lives under the repo root, else absolute. NEVER
    /// cwd-relative: children run with `current_dir(repo_root)`, not the user's
    /// shell cwd.
    pub invocation: String,
}

/// One step in the pressure pipeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Spawn Claude with a single positional slash invocation (e.g. `/pressure <plan>`).
    Claude(String),
    /// Delete the codex report file if it exists.
    DeleteReport(PathBuf),
    /// Spawn codex with a `$pressure <plan>` skill invocation.
    Codex(String),
}

/// What to do after a child process exits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitAction {
    /// Exit 0 — proceed to the next step.
    Continue,
    /// User interrupt (130/2) or signal-killed child (no code) — abort cleanly.
    Abort,
    /// Other non-zero — warn and continue.
    Warn,
}

/// Resolve the repository root: `git rev-parse --show-toplevel`, else cwd.
fn resolve_repo_root() -> Result<PathBuf> {
    if let Ok(output) = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
    {
        if output.status.success() {
            if let Ok(s) = String::from_utf8(output.stdout) {
                let trimmed = s.trim();
                if !trimmed.is_empty() {
                    return Ok(PathBuf::from(trimmed));
                }
            }
        }
    }
    std::env::current_dir().context("failed to determine current directory")
}

/// Whether a relative path already begins with `doc/plans/`.
fn starts_with_doc_plans(arg: &str) -> bool {
    arg.starts_with("doc/plans/")
}

/// Resolve a user-supplied plan argument into both a filesystem path and the
/// invocation string handed to the slash commands / codex skill.
///
/// `repo_root` MUST be canonicalized by the caller so the repo-relative
/// invocation can be derived via `strip_prefix`. The raw argument (absolute, or
/// relative to `repo_root`) is tried first; only when it is absent do we fall
/// back to `doc/plans/<arg>`, and never when `<arg>` already starts with
/// `doc/plans/` (guards against `doc/plans/doc/plans/...`).
pub fn resolve_plan_path(arg: &str, repo_root: &Path) -> Result<ResolvedPlan> {
    let raw = Path::new(arg);

    let primary = if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        repo_root.join(raw)
    };

    // `is_file()` (not `exists()`) so a directory argument fails cleanly here
    // rather than canonicalizing and spawning the agents against a non-file.
    let chosen = if primary.is_file() {
        primary
    } else if !raw.is_absolute() && !starts_with_doc_plans(arg) {
        let fallback = repo_root.join("doc/plans").join(raw);
        if fallback.is_file() {
            fallback
        } else {
            bail!(
                "plan file not found: tried {} and {}",
                primary.display(),
                fallback.display()
            );
        }
    } else {
        bail!("plan file not found: {}", primary.display());
    };

    let fs_path = chosen
        .canonicalize()
        .with_context(|| format!("failed to canonicalize plan path {}", chosen.display()))?;

    let invocation = match fs_path.strip_prefix(repo_root) {
        Ok(rel) => rel.to_string_lossy().into_owned(),
        Err(_) => fs_path.to_string_lossy().into_owned(),
    };

    Ok(ResolvedPlan {
        fs_path,
        invocation,
    })
}

/// Sibling report path for a plan: `codex-<basename>` next to the plan.
pub fn codex_report_path(fs_path: &Path) -> PathBuf {
    let file_name = fs_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let report_name = format!("codex-{file_name}");
    match fs_path.parent() {
        Some(dir) => dir.join(report_name),
        None => PathBuf::from(report_name),
    }
}

/// Build the ordered list of steps for `rounds` rounds.
///
/// Each round: delete the report (so a failed Codex write can't leave `/address`
/// reading the previous round's report) → Claude `/pressure` → Codex `$pressure`
/// → Claude `/address`. After all rounds, one final report deletion as cleanup.
pub fn plan_steps(rounds: u32, invocation: &str, report: &Path) -> Vec<Step> {
    let mut steps = Vec::new();
    for _ in 0..rounds {
        steps.push(Step::DeleteReport(report.to_path_buf()));
        steps.push(Step::Claude(format!("/pressure {invocation}")));
        steps.push(Step::Codex(format!("$pressure {invocation}")));
        steps.push(Step::Claude(format!("/address {invocation}")));
    }
    steps.push(Step::DeleteReport(report.to_path_buf()));
    steps
}

/// Environment variable enabling Claude Code's agent-teams feature.
const AGENT_TEAMS_ENV: &str = "CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS";

/// argv (after the binary) for a Claude spawn. `slash` is the full positional
/// slash invocation, e.g. `/pressure doc/plans/PLAN-foo.md`.
fn claude_args(slash: &str) -> Vec<String> {
    vec![
        "--permission-mode".to_string(),
        "acceptEdits".to_string(),
        "--model".to_string(),
        "opus".to_string(),
        slash.to_string(),
    ]
}

/// argv (after the binary) for a Codex spawn. `skill` is the full positional
/// skill invocation, e.g. `$pressure doc/plans/PLAN-foo.md`.
fn codex_args(repo_root: &Path, skill: &str) -> Vec<String> {
    vec![
        "exec".to_string(),
        "--sandbox".to_string(),
        "workspace-write".to_string(),
        "-C".to_string(),
        repo_root.display().to_string(),
        skill.to_string(),
    ]
}

/// Render the exact commands `--dry-run` would execute.
///
/// Uses the same [`claude_args`]/[`codex_args`] builders as the real spawns, so
/// the preview can never diverge from what actually runs.
pub fn render_dry_run(rounds: u32, invocation: &str, report: &Path, repo_root: &Path) -> String {
    let mut out = format!(
        "Dry run: {rounds} round(s) of pressure-testing for {invocation}\nCodex report: {}\n\n",
        report.display()
    );
    for (i, step) in plan_steps(rounds, invocation, report).iter().enumerate() {
        let line = match step {
            Step::DeleteReport(p) => format!("delete report {}", p.display()),
            Step::Claude(slash) => {
                format!(
                    "{AGENT_TEAMS_ENV}=1 claude {}",
                    claude_args(slash).join(" ")
                )
            }
            Step::Codex(skill) => format!("codex {}", codex_args(repo_root, skill).join(" ")),
        };
        out.push_str(&format!("  {}. {line}\n", i + 1));
    }
    out
}

/// Classify a finished child process for pipeline control.
pub fn classify_exit(status: ExitStatus) -> ExitAction {
    classify_code(status.code())
}

/// Pure classification of a child exit code (`None` = killed by a signal).
pub fn classify_code(code: Option<i32>) -> ExitAction {
    match code {
        Some(0) => ExitAction::Continue,
        // Ctrl+C (130/2) or signal-killed (no code) → abort the whole pipeline.
        None | Some(130) | Some(2) => ExitAction::Abort,
        Some(_) => ExitAction::Warn,
    }
}

/// Delete the codex report, treating "not found" as success.
fn delete_report(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => {
            Err(e).with_context(|| format!("failed to delete codex report {}", path.display()))
        }
    }
}

/// Spawn an interactive Claude session with a single positional slash invocation.
fn spawn_claude(claude_path: &Path, repo_root: &Path, slash: &str) -> Result<ExitStatus> {
    let mut cmd = Command::new(claude_path);
    cmd.args(claude_args(slash));
    cmd.env(AGENT_TEAMS_ENV, "1");
    cmd.current_dir(repo_root);
    cmd.stdin(Stdio::inherit());
    cmd.stdout(Stdio::inherit());
    cmd.stderr(Stdio::inherit());
    cmd.status().context("failed to spawn claude")
}

/// Spawn `codex exec` with the `$pressure` skill invocation.
fn spawn_codex(codex_path: &Path, repo_root: &Path, skill: &str) -> Result<ExitStatus> {
    let mut cmd = Command::new(codex_path);
    cmd.args(codex_args(repo_root, skill));
    cmd.current_dir(repo_root);
    cmd.stdin(Stdio::inherit());
    cmd.stdout(Stdio::inherit());
    cmd.stderr(Stdio::inherit());
    cmd.status().context("failed to spawn codex")
}

/// React to a finished child. Returns `true` when the pipeline should stop.
///
/// On abort the child label and exit code (or signal) are printed, so a
/// headless failure — e.g. a codex usage error exiting with clap's code 2 — is
/// surfaced rather than silently mistaken for a clean Ctrl+C interrupt.
fn should_stop(label: &str, status: ExitStatus) -> bool {
    match classify_exit(status) {
        ExitAction::Continue => false,
        ExitAction::Warn => {
            println!(
                "{} {label} exited with code {} — continuing",
                "!".yellow().bold(),
                status.code().unwrap_or(-1)
            );
            false
        }
        ExitAction::Abort => {
            match status.code() {
                Some(code) => println!(
                    "\n{} {label} exited with code {code} — stopping pressure run.",
                    "─".dimmed()
                ),
                None => println!(
                    "\n{} {label} was terminated by a signal — stopping pressure run.",
                    "─".dimmed()
                ),
            }
            true
        }
    }
}

/// Execute the pressure pipeline.
pub fn execute(plan: String, rounds: u32, dry_run: bool) -> Result<()> {
    let repo_root = resolve_repo_root()?;
    let repo_root = repo_root.canonicalize().unwrap_or(repo_root);
    let resolved = resolve_plan_path(&plan, &repo_root)?;
    let report = codex_report_path(&resolved.fs_path);

    if dry_run {
        print!(
            "{}",
            render_dry_run(rounds, &resolved.invocation, &report, &repo_root)
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
                delete_report(&path)?;
                false
            }
            Step::Claude(invocation) => {
                let status = spawn_claude(&claude_path, &repo_root, &invocation)?;
                should_stop("claude", status)
            }
            Step::Codex(invocation) => {
                let status = spawn_codex(&codex_path, &repo_root, &invocation)?;
                should_stop("codex", status)
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
