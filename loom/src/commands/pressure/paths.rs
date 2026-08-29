//! Plan, report, log and marker path resolution, plus small file helpers
//! shared by the pressure pipeline.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

/// A plan path resolved both for local filesystem use and for handing to the
/// slash commands / codex skill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ResolvedPlan {
    /// Absolute, canonicalized path on disk — used for existence checks, the
    /// codex report path, and deletion.
    pub fs_path: PathBuf,
    /// The string handed to the slash commands and codex skill. Repo-relative
    /// when the plan lives under the repo root, else absolute. NEVER
    /// cwd-relative: children run with `current_dir(repo_root)`, not the user's
    /// shell cwd.
    pub invocation: String,
}

/// Resolve the repository root: `git rev-parse --show-toplevel`, else cwd.
pub(super) fn resolve_repo_root() -> Result<PathBuf> {
    if let Ok(output) = std::process::Command::new("git")
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
pub(super) fn starts_with_doc_plans(arg: &str) -> bool {
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
pub(super) fn resolve_plan_path(arg: &str, repo_root: &Path) -> Result<ResolvedPlan> {
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
pub(super) fn codex_report_path(fs_path: &Path) -> PathBuf {
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
pub(super) fn codex_log_path() -> PathBuf {
    std::env::temp_dir().join(format!("loom-pressure-codex-{}.log", std::process::id()))
}

/// Marker path the foreground Claude agent creates as its final action to
/// signal completion (per driver process).
///
/// Deliberately inside the repo's gitignored `.work/`, NOT
/// `std::env::temp_dir()`: the agent runs under `--permission-mode auto`,
/// whose sandbox mounts `/tmp` read-only, so a `/tmp` marker can never be
/// created and the session would never auto-close. The child's cwd is
/// `repo_root`, which the sandbox does allow writes to.
pub(super) fn claude_marker_path(repo_root: &Path) -> PathBuf {
    repo_root
        .join(".work")
        .join("pressure")
        .join(format!("claude-{}.done", std::process::id()))
}

/// Ensure the marker's parent directory exists so the agent's `touch` cannot
/// fail on a missing directory.
pub(super) fn ensure_marker_dir(marker: &Path) -> Result<()> {
    match marker.parent() {
        Some(parent) => std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create marker dir {}", parent.display())),
        None => Ok(()),
    }
}

/// Delete a file, treating "not found" as success.
pub(super) fn delete_file(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e).with_context(|| format!("failed to delete {}", path.display())),
    }
}
