//! Git command runner abstraction
//!
//! Provides centralized functions for running git commands with consistent
//! error handling, reducing boilerplate across the codebase.

use anyhow::{bail, Context, Result};
use std::path::Path;
use std::process::{Command, Output};
use std::time::Duration;

const GIT_READ_TIMEOUT: Duration = Duration::from_secs(15);
const GIT_MUTATION_TIMEOUT: Duration = Duration::from_secs(120);
const GIT_NETWORK_TIMEOUT: Duration = Duration::from_secs(300);

fn git_timeout(args: &[&str]) -> Duration {
    match args.first().copied() {
        Some("clone" | "fetch" | "pull" | "push") => GIT_NETWORK_TIMEOUT,
        Some(
            "checkout" | "commit" | "merge" | "rebase" | "reset" | "restore" | "switch"
            | "worktree",
        ) => GIT_MUTATION_TIMEOUT,
        _ => GIT_READ_TIMEOUT,
    }
}

fn run_git_program(
    program: &str,
    args: &[&str],
    repo_root: &Path,
    timeout: Duration,
) -> Result<Output> {
    let mut command = Command::new(program);
    command
        .args(args)
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .current_dir(repo_root);
    crate::process::run_bounded_output(
        &mut command,
        timeout,
        format!("git {}", args.first().unwrap_or(&"command")),
    )
    .with_context(|| format!("Failed to execute: git {}", args.join(" ")))
}

/// Run a git command and return the raw Output.
///
/// Wraps `Command::new("git")` with `current_dir` and error context.
/// Sets `LC_ALL=C` and `LANG=C` so git output is always in English,
/// making stdout/stderr parsing locale-independent.
///
/// Use this when you need access to both stdout and stderr, or when
/// you need custom error handling logic.
///
/// # Arguments
/// * `args` - Git command arguments (e.g., `&["branch", "-v"]`)
/// * `repo_root` - Working directory for the git command
pub fn run_git(args: &[&str], repo_root: &Path) -> Result<Output> {
    run_git_program("git", args, repo_root, git_timeout(args))
}

/// Run a git command, check for success, and return stdout as a trimmed String.
///
/// On failure, bails with the full command + directory + exit code + stdout +
/// stderr context (conventions.md git error format).
///
/// # Arguments
/// * `args` - Git command arguments
/// * `repo_root` - Working directory for the git command
pub fn run_git_checked(args: &[&str], repo_root: &Path) -> Result<String> {
    let output = run_git(args, repo_root)?;
    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let exit_code = output
            .status
            .code()
            .map(|c| c.to_string())
            .unwrap_or_else(|| "signal".to_string());
        bail!(
            "git {} failed (exit code {exit_code}):\n\
             Command: git {}\n\
             Directory: {}\n\
             Stdout: {}\n\
             Stderr: {}",
            args.first().unwrap_or(&""),
            args.join(" "),
            repo_root.display(),
            if stdout.trim().is_empty() {
                "(empty)"
            } else {
                stdout.trim()
            },
            if stderr.trim().is_empty() {
                "(empty)"
            } else {
                stderr.trim()
            },
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Run a git command and return true if exit code is 0.
///
/// Silently swallows errors (both spawn failures and non-zero exits).
/// Use this for status checks like `branch_exists`, `rev-parse --verify`, etc.
///
/// # Arguments
/// * `args` - Git command arguments
/// * `repo_root` - Working directory for the git command
pub fn run_git_bool(args: &[&str], repo_root: &Path) -> bool {
    run_git(args, repo_root)
        .map(|output| output.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_deadlines_are_operation_specific() {
        assert_eq!(git_timeout(&["status"]), GIT_READ_TIMEOUT);
        assert_eq!(git_timeout(&["merge"]), GIT_MUTATION_TIMEOUT);
        assert_eq!(git_timeout(&["fetch"]), GIT_NETWORK_TIMEOUT);
        assert!(GIT_READ_TIMEOUT < GIT_MUTATION_TIMEOUT);
        assert!(GIT_MUTATION_TIMEOUT < GIT_NETWORK_TIMEOUT);
    }

    #[test]
    fn git_runner_returns_structured_timeout() {
        let repo = tempfile::tempdir().unwrap();
        let error = run_git_program(
            "sh",
            &["-c", "sleep 60"],
            repo.path(),
            Duration::from_millis(100),
        )
        .expect_err("fake git command must time out");

        let timeout = error
            .downcast_ref::<crate::process::ProcessTimeoutError>()
            .expect("caller must be able to classify a timeout");
        assert_eq!(timeout.operation(), "git -c");
    }
}
