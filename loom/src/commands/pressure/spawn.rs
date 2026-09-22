//! Argv construction and codex background lifecycle for the pressure
//! pipeline. The foreground Claude driver itself lives in
//! `crate::claude::session`, shared with `loom knowledge bootstrap`.

use anyhow::{Context, Result};
use colored::Colorize;
use std::io::Write;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::Duration;

use crate::claude::{classify_exit, ClaudeOutcome, ExitAction};

pub(super) use crate::claude::AGENT_TEAMS_ENV;

/// Bytes of the codex log tailed to the terminal when codex fails.
pub(super) const TAIL_BYTES: usize = 2000;

/// Single-line instruction appended to Claude's system prompt so an interactive
/// (subscription-billed) session can be closed by the driver: the agent creates
/// `marker` as its final action, which the driver watches for.
pub(super) fn completion_instruction(marker: &Path) -> String {
    format!(
        "AUTONOMOUS RUN: this Claude session was launched by `loom pressure`; no human will end it for you. \
         When the task is FULLY complete and the plan file is fully updated (after every subagent has finished), \
         your FINAL action MUST be to run exactly this shell command and nothing after it: touch {}. \
         Do not run it earlier. That path is inside the repo's gitignored `.loom/work/` because the agent sandbox \
         mounts /tmp read-only; creating this one marker is the sanctioned exception to the rule against \
         writing under `.loom/work/` directly. Once that file exists the driver closes this session.",
        marker.display()
    )
}

/// argv (after the binary) for a Claude spawn. `slash` is the full positional
/// slash invocation; `marker` is injected into the appended system prompt so
/// the agent can signal completion; `model` and `effort` are the resolved
/// [`super::models::PressureModels`] slots for this step — the same
/// `--model`/`--effort` pair the daemon's own Claude launcher uses.
pub(super) fn claude_args(slash: &str, marker: &Path, model: &str, effort: &str) -> Vec<String> {
    vec![
        "--permission-mode".to_string(),
        "auto".to_string(),
        "--model".to_string(),
        model.to_string(),
        "--effort".to_string(),
        effort.to_string(),
        "--append-system-prompt".to_string(),
        completion_instruction(marker),
        slash.to_string(),
    ]
}

/// argv (after the binary) for a Codex spawn. `skill` is the full positional
/// skill invocation, e.g. `$pressure doc/plans/PLAN-foo.md`; `model` and
/// `effort` are the resolved [`super::models::PressureModels`] slots for this
/// step. Codex has no dedicated effort flag, so `effort` travels as a `-c`
/// override instead.
pub(super) fn codex_args(repo_root: &Path, skill: &str, model: &str, effort: &str) -> Vec<String> {
    vec![
        "exec".to_string(),
        "--sandbox".to_string(),
        "workspace-write".to_string(),
        "-m".to_string(),
        model.to_string(),
        "-c".to_string(),
        format!("model_reasoning_effort={effort}"),
        "-C".to_string(),
        repo_root.display().to_string(),
        skill.to_string(),
    ]
}

/// Print the last `max_bytes` of a log file to stderr (for surfacing failures).
pub(super) fn print_log_tail(log_path: &Path, max_bytes: usize) {
    if let Ok(bytes) = std::fs::read(log_path) {
        let start = bytes.len().saturating_sub(max_bytes);
        eprintln!("{}", String::from_utf8_lossy(&bytes[start..]));
    }
}

/// Spawn Claude in the foreground (inherited TTY → interactive) and return
/// once the agent signals completion by creating `marker` — at which point
/// the now-idle session is SIGTERMed (mirroring how the loom daemon
/// terminates a session whose stage has completed). If the process exits
/// on its own first (e.g. the user exited manually) that status is returned.
pub(super) fn run_claude_foreground(
    claude_path: &Path,
    repo_root: &Path,
    slash: &str,
    marker: &Path,
    model: &str,
    effort: &str,
) -> Result<ClaudeOutcome> {
    let args = claude_args(slash, marker, model, effort);
    crate::claude::run_foreground(claude_path, repo_root, &args, marker)
}

/// Spawn `codex exec` in the background with its (noisy) output captured to
/// `log_path`, so it runs concurrently with the foreground Claude session
/// without flooding the terminal.
pub(super) fn spawn_codex_background(
    codex_path: &Path,
    repo_root: &Path,
    skill: &str,
    log_path: &Path,
    model: &str,
    effort: &str,
) -> Result<Child> {
    let log = std::fs::File::create(log_path)
        .with_context(|| format!("failed to create codex log {}", log_path.display()))?;
    let log_err = log
        .try_clone()
        .context("failed to clone codex log handle")?;
    let mut cmd = Command::new(codex_path);
    cmd.args(codex_args(repo_root, skill, model, effort));
    cmd.current_dir(repo_root);
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::from(log));
    cmd.stderr(Stdio::from(log_err));
    cmd.spawn().context("failed to spawn codex")
}

/// Wait for the background Codex child, showing a small spinner while it is
/// still running after the foreground Claude session has ended.
pub(super) fn wait_codex(mut child: Child, log_path: &Path) -> Result<ExitStatus> {
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
pub(super) fn should_stop(label: &str, status: ExitStatus, log: Option<&Path>) -> bool {
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
pub(super) fn claude_should_stop(outcome: ClaudeOutcome) -> bool {
    match outcome {
        ClaudeOutcome::Completed => false,
        ClaudeOutcome::Exited(status) => should_stop("claude", status, None),
    }
}
