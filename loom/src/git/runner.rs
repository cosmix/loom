//! Git command runner abstraction
//!
//! Provides centralized functions for running git commands with consistent
//! error handling, reducing boilerplate across the codebase.

use anyhow::{bail, Context, Result};
use std::path::Path;
use std::process::{Command, Output};

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
    Command::new("git")
        .args(args)
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .current_dir(repo_root)
        .output()
        .with_context(|| format!("Failed to execute: git {}", args.join(" ")))
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
