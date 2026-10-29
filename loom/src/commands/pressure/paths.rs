//! Terminal-only repository, log and marker paths for pressure runs.

use crate::git::runner::NO_HOOKS_ARGS;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Resolve the repository root: `git rev-parse --show-toplevel`, else cwd.
pub(super) fn resolve_repo_root() -> Result<PathBuf> {
    if let Ok(output) = std::process::Command::new("git")
        .args(NO_HOOKS_ARGS)
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

/// Temp path Codex's captured output is written to (per driver process).
pub(super) fn codex_log_path() -> PathBuf {
    std::env::temp_dir().join(format!("loom-pressure-codex-{}.log", std::process::id()))
}

/// Marker path the foreground Claude agent creates as its final action to
/// signal completion (per driver process).
///
/// Deliberately inside the repo's gitignored `.loom/work/`, NOT
/// `std::env::temp_dir()`: the agent runs under `--permission-mode auto`,
/// whose sandbox mounts `/tmp` read-only, so a `/tmp` marker can never be
/// created and the session would never auto-close. The child's cwd is
/// `repo_root`, which the sandbox does allow writes to.
pub(super) fn claude_marker_path(repo_root: &Path) -> PathBuf {
    repo_root
        .join(".loom")
        .join("work")
        .join("pressure")
        .join(format!("claude-{}.done", std::process::id()))
}
