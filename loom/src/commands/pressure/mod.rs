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

use anyhow::{bail, Context, Result};
use colored::Colorize;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::Duration;

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

/// Outcome of a foreground Claude step.
#[derive(Debug)]
enum ClaudeOutcome {
    /// The agent signalled completion (the marker appeared) and the driver
    /// terminated the idle session. Always treated as success.
    Completed,
    /// The process exited on its own — the user exited manually (typically
    /// code 0) or Claude crashed/was interrupted. Classified via [`ExitAction`].
    Exited(ExitStatus),
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

/// Temp path Codex's captured output is written to (per driver process).
fn codex_log_path() -> PathBuf {
    std::env::temp_dir().join(format!("loom-pressure-codex-{}.log", std::process::id()))
}

/// Temp marker path the foreground Claude agent creates as its final action to
/// signal completion (per driver process).
fn claude_marker_path() -> PathBuf {
    std::env::temp_dir().join(format!("loom-pressure-claude-{}.done", std::process::id()))
}

/// Build the ordered list of steps for `rounds` rounds.
///
/// Each round: delete the report (so a failed Codex write can't leave
/// `/address` reading the previous round's report) → run Claude `/pressure` and
/// Codex `$pressure` concurrently → Claude `/address`. After all rounds, one
/// final report deletion as cleanup.
pub fn plan_steps(rounds: u32, invocation: &str, report: &Path) -> Vec<Step> {
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

/// Environment variable enabling Claude Code's agent-teams feature.
const AGENT_TEAMS_ENV: &str = "CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS";

/// How often to poll for the completion marker / child exit.
const POLL_INTERVAL_MS: u64 = 300;
/// Grace period after SIGTERM before escalating to SIGKILL.
const TERM_GRACE_MS: u64 = 4000;
/// Bytes of the codex log tailed to the terminal when codex fails.
const TAIL_BYTES: usize = 2000;

/// Single-line instruction appended to Claude's system prompt so an interactive
/// (subscription-billed) session can be closed by the driver: the agent creates
/// `marker` as its final action, which the driver watches for.
fn completion_instruction(marker: &Path) -> String {
    format!(
        "AUTONOMOUS RUN: this Claude session was launched by `loom pressure`; no human will end it for you. \
         When the task is FULLY complete and the plan file is fully updated (after every subagent has finished), \
         your FINAL action MUST be to run exactly this shell command and nothing after it: touch {}. \
         Do not run it earlier. Once that file exists the driver closes this session.",
        marker.display()
    )
}

/// argv (after the binary) for a Claude spawn. `slash` is the full positional
/// slash invocation; `marker` is injected into the appended system prompt so
/// the agent can signal completion.
fn claude_args(slash: &str, marker: &Path) -> Vec<String> {
    vec![
        "--permission-mode".to_string(),
        "auto".to_string(),
        "--model".to_string(),
        "opus".to_string(),
        "--append-system-prompt".to_string(),
        completion_instruction(marker),
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
pub fn render_dry_run(
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

/// Delete a file, treating "not found" as success.
fn delete_file(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e).with_context(|| format!("failed to delete {}", path.display())),
    }
}

/// Send SIGTERM to a process, ignoring "already gone".
fn send_sigterm(pid: u32) {
    use nix::sys::signal::{kill, Signal};
    use nix::unistd::Pid;
    let _ = kill(Pid::from_raw(pid as i32), Signal::SIGTERM);
}

/// Print the last `max_bytes` of a log file to stderr (for surfacing failures).
fn print_log_tail(log_path: &Path, max_bytes: usize) {
    if let Ok(bytes) = std::fs::read(log_path) {
        let start = bytes.len().saturating_sub(max_bytes);
        eprintln!("{}", String::from_utf8_lossy(&bytes[start..]));
    }
}

/// Spawn Claude in the foreground (inherited TTY → interactive/subscription
/// billing) and return once the agent signals completion by creating `marker`
/// — at which point the now-idle session is SIGTERMed (mirroring how the loom
/// daemon terminates a session whose stage has completed). If the process exits
/// on its own first (e.g. the user exited manually) that status is returned.
fn run_claude_foreground(
    claude_path: &Path,
    repo_root: &Path,
    slash: &str,
    marker: &Path,
) -> Result<ClaudeOutcome> {
    // Clear any stale marker from a previous step before spawning.
    delete_file(marker)?;

    let mut cmd = Command::new(claude_path);
    cmd.args(claude_args(slash, marker));
    cmd.env(AGENT_TEAMS_ENV, "1");
    cmd.current_dir(repo_root);
    cmd.stdin(Stdio::inherit());
    cmd.stdout(Stdio::inherit());
    cmd.stderr(Stdio::inherit());
    let mut child = cmd.spawn().context("failed to spawn claude")?;

    let outcome = loop {
        // The agent exited on its own (manual exit, crash, or Ctrl-C).
        if let Some(status) = child.try_wait().context("failed to poll claude")? {
            break ClaudeOutcome::Exited(status);
        }
        // The agent signalled completion → terminate the idle session.
        if marker.exists() {
            send_sigterm(child.id());
            let grace_polls = TERM_GRACE_MS / POLL_INTERVAL_MS;
            let mut reaped = false;
            for _ in 0..grace_polls {
                thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
                if child.try_wait().context("failed to poll claude")?.is_some() {
                    reaped = true;
                    break;
                }
            }
            if !reaped {
                let _ = child.kill();
                let _ = child.wait();
            }
            break ClaudeOutcome::Completed;
        }
        thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
    };

    delete_file(marker)?;
    Ok(outcome)
}

/// Spawn `codex exec` in the background with its (noisy) output captured to
/// `log_path`, so it runs concurrently with the foreground Claude session
/// without flooding the terminal.
fn spawn_codex_background(
    codex_path: &Path,
    repo_root: &Path,
    skill: &str,
    log_path: &Path,
) -> Result<Child> {
    let log = std::fs::File::create(log_path)
        .with_context(|| format!("failed to create codex log {}", log_path.display()))?;
    let log_err = log
        .try_clone()
        .context("failed to clone codex log handle")?;
    let mut cmd = Command::new(codex_path);
    cmd.args(codex_args(repo_root, skill));
    cmd.current_dir(repo_root);
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::from(log));
    cmd.stderr(Stdio::from(log_err));
    cmd.spawn().context("failed to spawn codex")
}

/// Wait for the background Codex child, showing a small spinner while it is
/// still running after the foreground Claude session has ended.
fn wait_codex(mut child: Child, log_path: &Path) -> Result<ExitStatus> {
    const FRAMES: [&str; 4] = ["⠋", "⠙", "⠹", "⠸"];
    let mut i = 0usize;
    loop {
        if let Some(status) = child.try_wait().context("failed to poll codex")? {
            // Clear the spinner line.
            print!("\r\x1b[K");
            let _ = std::io::stdout().flush();
            return Ok(status);
        }
        print!(
            "\r{} waiting for codex review… (output → {})",
            FRAMES[i % FRAMES.len()],
            log_path.display()
        );
        let _ = std::io::stdout().flush();
        i += 1;
        thread::sleep(Duration::from_millis(200));
    }
}

/// React to a finished child. Returns `true` when the pipeline should stop.
///
/// On abort the child label and exit code (or signal) are printed, so a
/// headless failure — e.g. a codex usage error exiting with clap's code 2 — is
/// surfaced rather than silently mistaken for a clean Ctrl+C interrupt. When a
/// `log` is provided (codex), its tail is printed on any non-clean exit.
fn should_stop(label: &str, status: ExitStatus, log: Option<&Path>) -> bool {
    match classify_exit(status) {
        ExitAction::Continue => false,
        ExitAction::Warn => {
            println!(
                "{} {label} exited with code {} — continuing",
                "!".yellow().bold(),
                status.code().unwrap_or(-1)
            );
            if let Some(p) = log {
                print_log_tail(p, TAIL_BYTES);
            }
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
            if let Some(p) = log {
                print_log_tail(p, TAIL_BYTES);
            }
            true
        }
    }
}

/// Map a foreground Claude outcome to a stop decision. A driver-initiated
/// completion is always success; a self-exit is classified normally.
fn claude_should_stop(outcome: ClaudeOutcome) -> bool {
    match outcome {
        ClaudeOutcome::Completed => false,
        ClaudeOutcome::Exited(status) => should_stop("claude", status, None),
    }
}

/// Execute the pressure pipeline.
pub fn execute(plan: String, rounds: u32, dry_run: bool) -> Result<()> {
    let repo_root = resolve_repo_root()?;
    let repo_root = repo_root.canonicalize().unwrap_or(repo_root);
    let resolved = resolve_plan_path(&plan, &repo_root)?;
    let report = codex_report_path(&resolved.fs_path);
    let marker = claude_marker_path();
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
