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
/// Use this when you need access to both stdout and stderr, or when
/// you need custom error handling logic.
///
/// # Arguments
/// * `args` - Git command arguments (e.g., `&["branch", "-v"]`)
/// * `repo_root` - Working directory for the git command
pub fn run_git(args: &[&str], repo_root: &Path) -> Result<Output> {
    Command::new("git")
        .args(args)
        .current_dir(repo_root)
        .output()
        .with_context(|| format!("Failed to execute: git {}", args.join(" ")))
}

/// Run a git command, check for success, and return stdout as a trimmed String.
///
/// On failure, bails with the stderr content. Use this for commands where
/// you expect success and want the output as a string.
///
/// # Arguments
/// * `args` - Git command arguments
/// * `repo_root` - Working directory for the git command
pub fn run_git_checked(args: &[&str], repo_root: &Path) -> Result<String> {
    let output = run_git(args, repo_root)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let cmd = args.first().unwrap_or(&"");
        bail!("git {cmd} failed: {stderr}");
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
