//! Terminal-only repository, log and marker paths for pressure runs.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Resolve the repository root containing the current directory.
pub(super) fn resolve_repo_root() -> Result<PathBuf> {
    let cwd = std::env::current_dir().context("failed to determine current directory")?;
    resolve_repo_root_in(&cwd)
}

fn resolve_repo_root_in(dir: &Path) -> Result<PathBuf> {
    crate::git::revision::toplevel(dir)
}

/// Path Codex's captured output is written to (per driver process), inside the repository's own
/// pressure directory and never the shared temp directory, where another user could plant a
/// link at the predictable name.
pub(super) fn codex_log_path(repo_root: &Path) -> PathBuf {
    repo_root
        .join(".loom")
        .join("work")
        .join("pressure")
        .join(format!("codex-{}.log", std::process::id()))
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

#[cfg(test)]
mod tests {
    use super::{codex_log_path, resolve_repo_root_in};
    use std::path::Path;

    #[test]
    fn rejects_directory_outside_repository() {
        let dir = tempfile::tempdir().unwrap();
        assert!(resolve_repo_root_in(dir.path()).is_err());
    }

    #[test]
    fn the_codex_log_is_in_the_repository_pressure_directory() {
        let repo = Path::new("/repo");
        let log = codex_log_path(repo);
        assert!(log.starts_with("/repo/.loom/work/pressure"), "{log:?}");
        assert!(!log.starts_with(std::env::temp_dir()), "{log:?}");
    }
}
