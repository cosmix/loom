//! Plan and report path resolution for the pressure pipeline.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

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

/// Whether a relative path already begins with `doc/plans/`.
pub fn starts_with_doc_plans(arg: &str) -> bool {
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
