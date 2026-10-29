//! Claude argv construction and terminal exit handling for the pressure
//! pipeline. The foreground Claude driver itself lives in
//! `crate::claude::session`, shared with `loom knowledge bootstrap`.

use anyhow::Result;
use colored::Colorize;
use std::path::Path;
use std::process::ExitStatus;

use crate::claude::{classify_exit, ClaudeOutcome, ExitAction};

pub(super) use crate::claude::AGENT_TEAMS_ENV;

/// Bytes of the codex log tailed to the terminal when codex fails.
pub(super) const TAIL_BYTES: usize = 2000;

/// argv (after the binary) for a Claude spawn. `slash` is the full positional
/// slash invocation; `marker` is injected into the appended system prompt so
/// the agent can signal completion; `model` and `effort` are the resolved
/// [`crate::pressure::models::PressureModels`] slots for this step — the same
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
        crate::pressure::completion_instruction(marker, crate::pressure::MarkerHome::Workspace),
        slash.to_string(),
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

/// React to a finished child, returning the stop message without decoration.
///
/// On abort the child label and exit code (or signal) are printed, so a
/// headless failure — e.g. a codex usage error exiting with clap's code 2 — is
/// surfaced rather than silently mistaken for a clean Ctrl+C interrupt. When a
/// `log` is provided (codex), its tail is printed on any non-clean exit.
pub(super) fn should_stop(label: &str, status: ExitStatus, log: Option<&Path>) -> Option<String> {
    match classify_exit(status) {
        ExitAction::Continue => None,
        ExitAction::Warn => {
            println!(
                "{} {label} exited with code {} — continuing",
                "!".yellow().bold(),
                status.code().unwrap_or(-1)
            );
            if let Some(p) = log {
                print_log_tail(p, TAIL_BYTES);
            }
            None
        }
        ExitAction::Abort => {
            let reason = match status.code() {
                Some(code) => format!("{label} exited with code {code} — stopping pressure run."),
                None => format!("{label} was terminated by a signal — stopping pressure run."),
            };
            println!("\n{} {reason}", "─".dimmed());
            if let Some(p) = log {
                print_log_tail(p, TAIL_BYTES);
            }
            Some(reason)
        }
    }
}

/// Map a foreground Claude outcome to a stop message. A driver-initiated
/// completion is always success; a self-exit is classified normally.
pub(super) fn claude_should_stop(outcome: ClaudeOutcome) -> Option<String> {
    match outcome {
        ClaudeOutcome::Completed => None,
        ClaudeOutcome::Exited(status) => should_stop("claude", status, None),
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;

    #[test]
    fn clean_and_warning_exits_continue_without_a_stop_reason() {
        for code in [0, 1, 42] {
            let status = ExitStatus::from_raw(code << 8);
            assert_eq!(should_stop("codex", status, None), None);
            assert_eq!(claude_should_stop(ClaudeOutcome::Exited(status)), None);
        }
        assert_eq!(claude_should_stop(ClaudeOutcome::Completed), None);
    }

    #[test]
    fn abort_exit_codes_return_the_printed_reason_for_each_agent() {
        for code in [2, 130] {
            let status = ExitStatus::from_raw(code << 8);
            assert_eq!(
                should_stop("codex", status, None),
                Some(format!(
                    "codex exited with code {code} — stopping pressure run."
                ))
            );
            assert_eq!(
                claude_should_stop(ClaudeOutcome::Exited(status)),
                Some(format!(
                    "claude exited with code {code} — stopping pressure run."
                ))
            );
        }
    }

    #[test]
    fn signal_exit_returns_the_printed_signal_reason() {
        let status = ExitStatus::from_raw(15);
        assert_eq!(
            should_stop("codex", status, None),
            Some("codex was terminated by a signal — stopping pressure run.".to_string())
        );
        assert_eq!(
            claude_should_stop(ClaudeOutcome::Exited(status)),
            Some("claude was terminated by a signal — stopping pressure run.".to_string())
        );
    }
}
